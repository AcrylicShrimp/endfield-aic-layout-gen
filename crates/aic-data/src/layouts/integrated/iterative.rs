use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::growth::plan_facility_growth;
use crate::layouts::placement::search_facility_placement_candidates;
use crate::layouts::{
    FacilityPlacement, FacilityPlacementRequest, FacilityPlacementSearchScope,
    FacilityPlacementStatus,
};
use crate::logistics::{
    LogisticsComponentKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
    FacilityInstanceWiringProjectedEndpoint, FacilityInstanceWiringProjection,
    FacilityInstanceWiringReport,
};

use super::{
    CandidateCounts, CandidatePolicyTable, CandidateRank, DeterministicCandidateKey,
    FacilityChangeCounts, IncumbentProvenance, IntegratedLayoutDiagnostic,
    IntegratedLayoutIncumbentSummary, IntegratedLayoutPhase, IntegratedLayoutPhaseAttempt,
    IntegratedLayoutPhaseOptimization, IntegratedLayoutReport, IntegratedLayoutStatus,
    IterativeOptimizationConfig, LayoutScore, LayoutScoreDelta, OptimizationProofStatus,
    OptimizationTerminationReason, PRODUCTION_FACILITY_GAP, PhaseElapsedMilliseconds,
    PhaseIncumbent, RefinementKind, RetainedRoutingState, RouteChangeCounts,
    budget::{GrowthPhaseBudget, StrategyBudget},
    extend_phase_incumbent, frame_placements_for_routing, prepare_model, route_turn_count, sparse,
    validate_candidate_policy_table, validate_iterative_optimization_config,
};

#[allow(clippy::too_many_arguments)]
pub fn construct_iterative_scc_layout(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    config: &IterativeOptimizationConfig,
) -> IntegratedLayoutReport {
    construct_iterative_scc_layout_with_cancellation(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        config,
        &|| false,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn construct_iterative_scc_layout_with_cancellation(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    config: &IterativeOptimizationConfig,
    cancellation_requested: &dyn Fn() -> bool,
) -> IntegratedLayoutReport {
    if let Err(diagnostics) = validate_iterative_optimization_config(config) {
        let mut report = IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::InvalidInput,
            IntegratedLayoutDiagnostic::error(
                "invalid-iterative-optimization-config",
                "/optimization_config",
                None,
                "iterative optimization configuration is invalid",
            ),
        );
        report
            .diagnostics
            .extend(diagnostics.into_iter().map(|diagnostic| {
                IntegratedLayoutDiagnostic::error(
                    diagnostic.code,
                    diagnostic.path,
                    None,
                    diagnostic.message,
                )
            }));
        return report;
    }
    let policy_table = CandidatePolicyTable::default();
    if let Err(diagnostics) = validate_candidate_policy_table(&policy_table) {
        let mut report = IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::InvalidInput,
            IntegratedLayoutDiagnostic::error(
                "invalid-candidate-policy-table",
                "/candidate_policy_table",
                None,
                "built-in candidate policy table is invalid",
            ),
        );
        report
            .diagnostics
            .extend(diagnostics.into_iter().map(|diagnostic| {
                IntegratedLayoutDiagnostic::error(
                    diagnostic.code,
                    diagnostic.path,
                    None,
                    diagnostic.message,
                )
            }));
        return report;
    }
    let mut strategy_budget = StrategyBudget::new(config, Instant::now());
    let strategy_deadline = strategy_budget.strategy_deadline();
    let growth = plan_facility_growth(instance_wiring, config.max_new_facilities_per_phase);
    if !growth.success {
        let diagnostic = growth.diagnostics.into_iter().next().map_or_else(
            || {
                IntegratedLayoutDiagnostic::error(
                    "iterative-growth-planning-failed",
                    "/",
                    None,
                    "SCC growth planning failed without a diagnostic",
                )
            },
            |diagnostic| {
                IntegratedLayoutDiagnostic::error(
                    "iterative-growth-planning-failed",
                    diagnostic.path,
                    diagnostic.entity,
                    diagnostic.message,
                )
            },
        );
        return IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic);
    }
    if growth.phases.is_empty() {
        return super::construct_coordinate_integrated_layout_with_time_limit(
            instance_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
            strategy_deadline.saturating_duration_since(Instant::now()),
        );
    }

    let total_facilities = growth
        .phases
        .iter()
        .map(|phase| phase.facilities.len())
        .sum::<usize>();
    let mut cumulative_facilities = BTreeSet::new();
    let mut anchors = Vec::<FacilityPlacement>::new();
    let mut snapshots = Vec::with_capacity(growth.phases.len());
    let mut latest_success = None;
    let mut previous_partial_wiring = None;

    for (phase_offset, phase) in growth.phases.iter().enumerate() {
        let phase_started = Instant::now();
        if cancellation_requested() {
            let mut report = IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "iterative-scc-cancelled",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    "iterative SCC optimization was cancelled before starting the phase",
                ),
            );
            report.phases = snapshots;
            return report;
        }
        let remaining_growth_phases = growth.phases.len() - phase_offset;
        let mut phase_budget =
            strategy_budget.begin_growth_phase(remaining_growth_phases, phase_started);
        if Instant::now() >= strategy_deadline {
            let mut report = IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "iterative-scc-strategy-time-limit",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    "iterative SCC construction exhausted the total strategy deadline",
                ),
            );
            report.phases = snapshots;
            return report;
        }
        cumulative_facilities.extend(phase.facilities.iter().cloned());
        let graph_started = Instant::now();
        let partial_wiring = match project_cumulative_wiring(
            instance_wiring,
            &cumulative_facilities,
            total_facilities,
        ) {
            Ok(wiring) => wiring,
            Err(diagnostic) => {
                let mut report = IntegratedLayoutReport::failure(
                    IntegratedLayoutStatus::InvalidInput,
                    diagnostic,
                );
                report.phases = snapshots;
                return report;
            }
        };
        let graph_construction_ms = elapsed_milliseconds(graph_started);
        let prior_reference = anchors.clone();
        let phase_search = match optimize_cumulative_phase(
            phase.index,
            previous_partial_wiring.as_ref(),
            &partial_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
            config,
            &policy_table,
            &cumulative_facilities,
            &prior_reference,
            latest_success.as_ref(),
            &mut strategy_budget,
            &mut phase_budget,
            cancellation_requested,
        ) {
            Ok(search) => search,
            Err(diagnostic) => {
                let mut report = IntegratedLayoutReport::failure(
                    IntegratedLayoutStatus::InvalidInput,
                    diagnostic,
                );
                report.phases = snapshots;
                return report;
            }
        };
        let PhaseSearchResult {
            incumbent,
            initial_incumbent,
            attempts: attempt_reports,
            candidate_counts,
            route_changes,
            incumbent_extension_ms,
            placement_ms,
            routing_ms,
            validation_ms,
            termination_reason,
        } = phase_search;
        let Some(selected_incumbent) = incumbent else {
            let attempt_summary = attempt_reports
                .iter()
                .map(|attempt| {
                    format!(
                        "placement_hints={}, status={:?}, diagnostic={}",
                        attempt.placement_hint_count,
                        attempt.status,
                        attempt.diagnostic_code.as_deref().unwrap_or("none"),
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let mut report = IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "iterative-scc-phase-unsolved",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    format!(
                        "could not place and route SCC growth phase {} after {} bounded and fallback attempts: {attempt_summary}",
                        phase.index,
                        attempt_reports.len(),
                    ),
                ),
            );
            report.phases = snapshots;
            return report;
        };
        let final_score = selected_incumbent.score;
        let final_incumbent = IntegratedLayoutIncumbentSummary {
            score: final_score,
            candidate_key: selected_incumbent.candidate_key,
            provenance: selected_incumbent.provenance,
        };
        let mut phase_report = selected_incumbent.witness;

        let bounds = phase_report
            .bounds
            .clone()
            .expect("successful integrated phase must have bounds");
        let route_turns = phase_report.routes.iter().map(route_turn_count).sum();
        let route_cells = phase_report
            .routes
            .iter()
            .map(|route| route.cells.len())
            .sum();
        let bridge_count = phase_report
            .logistics_components
            .iter()
            .filter(|component| component.kind == LogisticsComponentKind::Bridge)
            .count();
        let unchanged_prior = prior_reference
            .iter()
            .filter(|prior| {
                phase_report.placements.iter().any(|placement| {
                    placement.instance == prior.instance
                        && placement.x == prior.x
                        && placement.y == prior.y
                        && placement.rotation == prior.rotation
                })
            })
            .count();
        let newly_placed = phase_report
            .placements
            .iter()
            .filter(|placement| {
                !prior_reference
                    .iter()
                    .any(|prior| prior.instance == placement.instance)
            })
            .count();
        let optimization = IntegratedLayoutPhaseOptimization {
            search_bounds: crate::layouts::FacilityPlacementBounds {
                width: request.max_width,
                height: request.max_height,
            },
            canonical_used_bounds: bounds.clone(),
            score_delta: initial_incumbent
                .as_ref()
                .map(|initial| LayoutScoreDelta::between(initial.score, final_incumbent.score)),
            initial_incumbent,
            final_incumbent,
            candidate_counts,
            facility_changes: FacilityChangeCounts {
                unchanged_prior,
                moved_prior: final_score.moved_prior_facility_count,
                newly_placed,
                rotation_changed: final_score.rotation_change_count,
            },
            route_changes,
            elapsed_ms: PhaseElapsedMilliseconds {
                graph_construction: graph_construction_ms,
                incumbent_extension: incumbent_extension_ms,
                placement: placement_ms,
                routing: routing_ms,
                validation: Some(validation_ms),
                total: elapsed_milliseconds(phase_started),
            },
            termination_reason,
            optimality: OptimizationProofStatus::Unproven,
        };
        anchors = phase_report.placements.clone();
        snapshots.push(IntegratedLayoutPhase {
            index: phase.index,
            introduced_components: phase.components.clone(),
            introduced_facilities: phase.facilities.clone(),
            ready_component_count: phase.ready_component_count,
            selected_component_count: phase.selected_component_count,
            deferred_component_count: phase.deferred_component_count,
            oversized_component_count: phase.oversized_component_count,
            cumulative_facility_count: cumulative_facilities.len(),
            cumulative_route_requirement_count: phase_report.routes.len(),
            prior_placement_hint_count: prior_reference.len(),
            bounds,
            placements: phase_report.placements.clone(),
            logistics_components: phase_report.logistics_components.clone(),
            routes: phase_report.routes.clone(),
            route_turns,
            route_cells,
            bridge_count,
            attempts: attempt_reports,
            optimization,
        });
        phase_report.diagnostics.push(IntegratedLayoutDiagnostic::info_for(
            "iterative-scc-phase-solved",
            format!("phase:{}", phase.index),
            format!(
                "solved output-first SCC growth phase {} with {} cumulative facilities using {} prior coordinate hints without movement constraints",
                phase.index,
                cumulative_facilities.len(),
                anchors.len(),
            ),
        ));
        if !phase_budget.reserve_borrowed().is_zero() {
            phase_report.diagnostics.push(IntegratedLayoutDiagnostic::info_for(
                "final-reserve-borrowed",
                format!("phase:{}", phase.index),
                format!(
                    "borrowed {} ms from the final refinement reserve because the phase had no incumbent",
                    phase_budget.reserve_borrowed().as_millis(),
                ),
            ));
        }
        strategy_budget.finish_growth_phase(&phase_budget, Instant::now());
        previous_partial_wiring = Some(partial_wiring);
        latest_success = Some(phase_report);
        if termination_reason == OptimizationTerminationReason::Cancelled {
            let mut report = latest_success
                .take()
                .expect("cancelled phase retained its incumbent");
            report.phases = snapshots;
            report.diagnostics.push(IntegratedLayoutDiagnostic::info(
                "iterative-scc-cancelled-with-incumbent",
                "iterative SCC optimization returned the best validated incumbent after cancellation",
            ));
            return report;
        }
    }

    let mut report = latest_success.expect("non-empty growth plan produced a final report");
    report.phases = snapshots;
    report.diagnostics.push(IntegratedLayoutDiagnostic::info(
        "iterative-scc-layout-complete",
        format!(
            "constructed and validated the full layout through {} output-first SCC growth phases",
            report.phases.len(),
        ),
    ));
    let _final_refinement_grant = strategy_budget.final_refinement_grant(Instant::now());
    report
}

struct PhaseSearchResult {
    incumbent: Option<PhaseIncumbent>,
    initial_incumbent: Option<IntegratedLayoutIncumbentSummary>,
    attempts: Vec<IntegratedLayoutPhaseAttempt>,
    candidate_counts: CandidateCounts,
    route_changes: RouteChangeCounts,
    incumbent_extension_ms: u64,
    placement_ms: u64,
    routing_ms: u64,
    validation_ms: u64,
    termination_reason: OptimizationTerminationReason,
}

#[allow(clippy::too_many_arguments)]
fn optimize_cumulative_phase(
    phase_index: usize,
    previous_wiring: Option<&FacilityInstanceWiringReport>,
    current_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    config: &IterativeOptimizationConfig,
    policy_table: &CandidatePolicyTable,
    cumulative_facilities: &BTreeSet<String>,
    prior_reference: &[FacilityPlacement],
    prior_witness: Option<&IntegratedLayoutReport>,
    strategy_budget: &mut StrategyBudget,
    phase_budget: &mut GrowthPhaseBudget,
    cancellation_requested: &dyn Fn() -> bool,
) -> Result<PhaseSearchResult, IntegratedLayoutDiagnostic> {
    let extension_started = Instant::now();
    let minimum_attempt = std::time::Duration::from_millis(config.minimum_phase_attempt_ms);
    let mut incumbent = None;
    let mut initial_incumbent = None;
    let mut route_changes = RouteChangeCounts::default();
    let mut attempts = Vec::new();
    if let (Some(previous_wiring), Some(prior_witness)) = (previous_wiring, prior_witness) {
        let now = Instant::now();
        let phase_remaining = phase_budget.remaining(now);
        let extension_grant = (phase_remaining / 4)
            .max(minimum_attempt)
            .min(phase_remaining);
        let extension_deadline = min_instant(
            phase_budget.deadline(),
            now.checked_add(extension_grant).unwrap_or(now),
        );
        let extension = extend_phase_incumbent(
            previous_wiring,
            current_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
            prior_witness,
            phase_index,
            extension_deadline,
        );
        route_changes = RouteChangeCounts {
            reused: extension.counts.reused_routes,
            invalidated: extension.counts.invalidated_routes,
            rerouted: extension.counts.rerouted_routes,
            new: extension.counts.new_routes,
        };
        if let Some(extended) = extension.incumbent {
            initial_incumbent = Some(IntegratedLayoutIncumbentSummary {
                score: extended.score,
                candidate_key: extended.candidate_key,
                provenance: extended.provenance.clone(),
            });
            incumbent = Some(extended);
        } else if let Some(diagnostic) = extension.diagnostics.first() {
            attempts.push(IntegratedLayoutPhaseAttempt {
                candidate_key: None,
                policy_id: Some("incumbent-extension".to_string()),
                placement_hint_count: prior_reference.len(),
                status: IntegratedLayoutStatus::Unknown,
                diagnostic_code: Some(diagnostic.code.to_string()),
            });
        }
    }
    let incumbent_extension_ms = elapsed_milliseconds(extension_started);
    let placement_scope = FacilityPlacementSearchScope {
        free_facility_ids: cumulative_facilities.clone(),
        fixed_facility_ids: BTreeSet::new(),
    };
    let mut candidate_counts = CandidateCounts::default();
    let mut placement_ms = 0_u64;
    let mut routing_ms = 0_u64;
    let mut validation_ms = 0_u64;
    let attempt_limit = config.candidate_attempts_per_neighborhood;
    let mut cancelled = false;
    let mut restart_index = 0_usize;

    loop {
        let improvements_before_pass = candidate_counts.improved;
        let mut attempt_index = 0_usize;
        while attempt_index < attempt_limit {
            if cancellation_requested() {
                cancelled = true;
                break;
            }
            let now = Instant::now();
            let remaining_slots = attempt_limit - attempt_index;
            let mut remaining = phase_budget.remaining(now);
            let mut attempt_grant =
                remaining / u32::try_from(remaining_slots).unwrap_or(u32::MAX).max(1);
            if attempt_grant < minimum_attempt {
                let borrowed = if incumbent.is_none() {
                    strategy_budget.borrow_for_missing_incumbent(phase_budget)
                } else {
                    std::time::Duration::ZERO
                };
                if !borrowed.is_zero() {
                    remaining = phase_budget.remaining(now);
                    attempt_grant = remaining;
                }
                if attempt_grant < minimum_attempt {
                    break;
                }
            }
            let attempt_deadline = min_instant(
                phase_budget.deadline(),
                now.checked_add(attempt_grant).unwrap_or(now),
            );
            let policy_index = attempt_index % policy_table.policies.len();
            let policy = &policy_table.policies[policy_index];
            let hints = incumbent
                .as_ref()
                .map(|incumbent| incumbent.witness.placements.clone())
                .unwrap_or_else(|| prior_reference.to_vec());
            let producer_started = Instant::now();
            let producer_deadline = min_instant(
                attempt_deadline,
                producer_started
                    .checked_add(attempt_grant / 2)
                    .unwrap_or(producer_started),
            );
            let batch = search_facility_placement_candidates(
                current_wiring,
                facilities,
                request,
                PRODUCTION_FACILITY_GAP,
                &hints,
                &placement_scope,
                policy.placement_policy,
                policy.max_candidate_yields,
                producer_deadline,
            );
            placement_ms = placement_ms.saturating_add(elapsed_milliseconds(producer_started));
            candidate_counts.generated = candidate_counts
                .generated
                .saturating_add(batch.candidates.len());
            candidate_counts.timed_out = candidate_counts
                .timed_out
                .saturating_add(usize::from(batch.timed_out));
            if cancellation_requested() {
                cancelled = true;
                break;
            }
            if batch.candidates.is_empty() {
                attempts.push(IntegratedLayoutPhaseAttempt {
                    candidate_key: None,
                    policy_id: Some(policy.id.clone()),
                    placement_hint_count: hints.len(),
                    status: placement_status(batch.status),
                    diagnostic_code: batch
                        .diagnostics
                        .first()
                        .map(|diagnostic| diagnostic.code.to_string()),
                });
            }

            for candidate in batch.candidates {
                let candidate_key = DeterministicCandidateKey {
                    phase_index,
                    refinement_kind: RefinementKind::GrowthNeighborhood,
                    neighborhood_rank: 3,
                    restart_index,
                    policy_index,
                    attempt_index,
                    yield_index: candidate.yield_index,
                };
                if Instant::now() >= attempt_deadline {
                    candidate_counts.timed_out += 1;
                    attempts.push(IntegratedLayoutPhaseAttempt {
                        candidate_key: Some(candidate_key),
                        policy_id: Some(policy.id.clone()),
                        placement_hint_count: hints.len(),
                        status: IntegratedLayoutStatus::Unknown,
                        diagnostic_code: Some("candidate-routing-deadline-exhausted".to_string()),
                    });
                    break;
                }
                if cancellation_requested() {
                    cancelled = true;
                    break;
                }
                let Some(placements) = frame_placements_for_routing(
                    candidate.report.placements,
                    request.max_width,
                    request.max_height,
                ) else {
                    candidate_counts.rejected += 1;
                    attempts.push(IntegratedLayoutPhaseAttempt {
                        candidate_key: Some(candidate_key),
                        policy_id: Some(policy.id.clone()),
                        placement_hint_count: hints.len(),
                        status: IntegratedLayoutStatus::Unknown,
                        diagnostic_code: Some("iterative-routing-frame-does-not-fit".to_string()),
                    });
                    continue;
                };
                let routing_started = Instant::now();
                candidate_counts.routed += 1;
                let input = prepare_model(current_wiring, facilities, items, transports, request)?;
                let (routed, candidate_route_changes) = if let Some(current_incumbent) = &incumbent
                {
                    let retained = RetainedRoutingState::from_validated_report(
                        &input,
                        &current_incumbent.witness,
                    )?;
                    let routed = sparse::construct_from_retained_with_policy(
                        input,
                        logistics_components,
                        placements,
                        &retained,
                        &BTreeSet::new(),
                        Some(policy.routing_order_policy),
                        attempt_deadline,
                    );
                    let changes = RouteChangeCounts {
                        reused: routed.reused_requirement_ids.len(),
                        invalidated: routed.invalidated_requirement_ids.len(),
                        rerouted: routed
                            .invalidated_requirement_ids
                            .iter()
                            .filter(|requirement_id| {
                                retained.retained_routes.contains_key(*requirement_id)
                            })
                            .count(),
                        new: routed
                            .invalidated_requirement_ids
                            .iter()
                            .filter(|requirement_id| {
                                !retained.retained_routes.contains_key(*requirement_id)
                            })
                            .count(),
                    };
                    (routed.report, changes)
                } else {
                    let routed = sparse::construct_from_placements_with_policy(
                        input,
                        logistics_components,
                        placements,
                        Some(policy.routing_order_policy),
                        attempt_deadline,
                    );
                    let changes = RouteChangeCounts {
                        new: routed.routes.len(),
                        ..RouteChangeCounts::default()
                    };
                    (routed, changes)
                };
                routing_ms = routing_ms.saturating_add(elapsed_milliseconds(routing_started));
                if cancellation_requested() {
                    cancelled = true;
                    break;
                }
                if !routed.success || Instant::now() > attempt_deadline {
                    candidate_counts.rejected += 1;
                    candidate_counts.timed_out += usize::from(Instant::now() > attempt_deadline);
                    attempts.push(IntegratedLayoutPhaseAttempt {
                        candidate_key: Some(candidate_key),
                        policy_id: Some(policy.id.clone()),
                        placement_hint_count: hints.len(),
                        status: routed.status,
                        diagnostic_code: routed
                            .diagnostics
                            .first()
                            .map(|diagnostic| diagnostic.code.to_string()),
                    });
                    continue;
                }
                let validation_started = Instant::now();
                let validation_input =
                    prepare_model(current_wiring, facilities, items, transports, request)?;
                super::witness::validate(&validation_input, logistics_components, &routed)?;
                let retained =
                    RetainedRoutingState::from_validated_report(&validation_input, &routed)?;
                validation_ms =
                    validation_ms.saturating_add(elapsed_milliseconds(validation_started));
                candidate_counts.validated += 1;
                let score = LayoutScore::from_report(&routed, prior_reference)
                    .expect("validated routed candidate must be scoreable");
                let rank = CandidateRank {
                    score,
                    deterministic_candidate_key: candidate_key,
                };
                let improves = incumbent.as_ref().is_none_or(|current| {
                    rank < CandidateRank {
                        score: current.score,
                        deterministic_candidate_key: current.candidate_key,
                    }
                });
                if improves {
                    candidate_counts.improved += 1;
                    route_changes = candidate_route_changes;
                    incumbent = Some(PhaseIncumbent {
                        cumulative_graph_key: retained.graph_key,
                        cumulative_graph_fingerprint: retained.graph_fingerprint,
                        witness: routed,
                        score,
                        candidate_key,
                        provenance: IncumbentProvenance::NeighborhoodCandidate {
                            neighborhood_rank: 3,
                            attempt_index,
                        },
                    });
                }
                attempts.push(IntegratedLayoutPhaseAttempt {
                    candidate_key: Some(candidate_key),
                    policy_id: Some(policy.id.clone()),
                    placement_hint_count: hints.len(),
                    status: IntegratedLayoutStatus::Feasible,
                    diagnostic_code: None,
                });
            }
            if cancelled {
                break;
            }
            attempt_index += 1;
        }
        if cancelled
            || candidate_counts.improved == improvements_before_pass
            || restart_index >= config.same_neighborhood_restart_limit
        {
            break;
        }
        restart_index += 1;
    }

    let termination_reason = if cancelled {
        OptimizationTerminationReason::Cancelled
    } else if phase_budget.remaining(Instant::now()).is_zero() {
        if incumbent.is_some() {
            OptimizationTerminationReason::PhaseBudgetExhaustedWithIncumbent
        } else {
            OptimizationTerminationReason::PhaseBudgetExhaustedWithoutIncumbent
        }
    } else {
        OptimizationTerminationReason::NeighborhoodScheduleExhausted
    };
    Ok(PhaseSearchResult {
        incumbent,
        initial_incumbent,
        attempts,
        candidate_counts,
        route_changes,
        incumbent_extension_ms,
        placement_ms,
        routing_ms,
        validation_ms,
        termination_reason,
    })
}

fn min_instant(left: Instant, right: Instant) -> Instant {
    if left <= right { left } else { right }
}

fn elapsed_milliseconds(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn placement_status(status: FacilityPlacementStatus) -> IntegratedLayoutStatus {
    match status {
        FacilityPlacementStatus::Optimal => IntegratedLayoutStatus::Optimal,
        FacilityPlacementStatus::Feasible => IntegratedLayoutStatus::Feasible,
        FacilityPlacementStatus::Infeasible => IntegratedLayoutStatus::Infeasible,
        FacilityPlacementStatus::InvalidInput => IntegratedLayoutStatus::InvalidInput,
        FacilityPlacementStatus::Unknown => IntegratedLayoutStatus::Unknown,
    }
}

fn project_cumulative_wiring(
    wiring: &FacilityInstanceWiringReport,
    cumulative_facilities: &BTreeSet<String>,
    total_facilities: usize,
) -> Result<FacilityInstanceWiringReport, IntegratedLayoutDiagnostic> {
    if cumulative_facilities.len() == total_facilities {
        return Ok(wiring.clone());
    }

    let nodes = wiring
        .nodes
        .iter()
        .map(|node| (node_id(node).to_string(), node))
        .collect::<BTreeMap<_, _>>();
    let mut retained_nodes = cumulative_facilities.clone();
    let mut projected_edges = Vec::new();
    let mut synthetic_nodes = Vec::new();

    for (edge_index, edge) in wiring.edges.iter().enumerate() {
        let source = nodes.get(&edge.source).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-wiring-edge-endpoint",
                format!("/edges/{edge_index}/source"),
                Some(edge.source.clone()),
                format!(
                    "wiring edge source '{}' does not reference a known node",
                    edge.source
                ),
            )
        })?;
        let target = nodes.get(&edge.target).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-wiring-edge-endpoint",
                format!("/edges/{edge_index}/target"),
                Some(edge.target.clone()),
                format!(
                    "wiring edge target '{}' does not reference a known node",
                    edge.target
                ),
            )
        })?;
        let source_is_facility = matches!(source, FacilityInstanceWiringNode::Facility { .. });
        let target_is_facility = matches!(target, FacilityInstanceWiringNode::Facility { .. });
        let source_included = source_is_facility && cumulative_facilities.contains(&edge.source);
        let target_included = target_is_facility && cumulative_facilities.contains(&edge.target);

        match (
            source_is_facility,
            target_is_facility,
            source_included,
            target_included,
        ) {
            (true, true, true, true) => projected_edges.push(edge.clone()),
            (true, true, false, true) => {
                let frontier_id = format!("iterative-external:{}", edge.id);
                synthetic_nodes.push(FacilityInstanceWiringNode::External {
                    id: frontier_id.clone(),
                    item: edge.item.clone(),
                });
                projected_edges.push(FacilityInstanceWiringEdge {
                    id: edge.id.clone(),
                    source: frontier_id,
                    target: edge.target.clone(),
                    kind: edge.kind.clone(),
                    item: edge.item.clone(),
                    rate: edge.rate,
                    projection: FacilityInstanceWiringProjection::FrontierExternal {
                        missing_facility: edge.source.clone(),
                        original_endpoint: FacilityInstanceWiringProjectedEndpoint::Source,
                    },
                });
            }
            (true, true, true, false) => {
                return Err(IntegratedLayoutDiagnostic::error(
                    "invalid-output-first-growth-order",
                    format!("/edges/{edge_index}"),
                    Some(format!("{}->{}", edge.source, edge.target)),
                    "an included facility feeds a facility that is absent from the cumulative output-first phase",
                ));
            }
            (false, true, false, true) | (true, false, true, false) => {
                retained_nodes.insert(edge.source.clone());
                retained_nodes.insert(edge.target.clone());
                projected_edges.push(edge.clone());
            }
            _ => {}
        }
    }

    let mut projected_nodes = wiring
        .nodes
        .iter()
        .filter(|node| retained_nodes.contains(node_id(node)))
        .cloned()
        .collect::<Vec<_>>();
    projected_nodes.extend(synthetic_nodes);
    Ok(FacilityInstanceWiringReport {
        schema_version: wiring.schema_version,
        success: true,
        nodes: projected_nodes,
        edges: projected_edges,
        diagnostics: wiring.diagnostics.clone(),
    })
}

fn node_id(node: &FacilityInstanceWiringNode) -> &str {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::recipes::{
        FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
        FacilityInstanceWiringProjectedEndpoint, FacilityInstanceWiringProjection,
        FacilityInstanceWiringReport, Rate,
    };

    use super::project_cumulative_wiring;

    #[test]
    fn replaces_missing_upstream_facilities_with_frontier_external_connections() {
        let wiring = chain_wiring();
        let original = wiring
            .edges
            .iter()
            .find(|edge| edge.source == "facility:a")
            .expect("fixture has the upstream edge");
        let frontier_id = format!("iterative-external:{}", original.id);
        let projected =
            project_cumulative_wiring(&wiring, &BTreeSet::from(["facility:b".to_string()]), 2)
                .expect("output phase should project");

        assert_eq!(projected.edges.len(), 2);
        assert!(projected.nodes.iter().any(|node| matches!(
            node,
            FacilityInstanceWiringNode::External { id, item }
                if id == &frontier_id && item == "middle"
        )));
        let projected_edge = projected
            .edges
            .iter()
            .find(|edge| edge.source == frontier_id)
            .expect("projected wiring has the frontier edge");
        assert_eq!(projected_edge.id, original.id);
        assert_eq!(projected_edge.target, "facility:b");
        assert_eq!(
            projected_edge.projection,
            FacilityInstanceWiringProjection::FrontierExternal {
                missing_facility: "facility:a".to_string(),
                original_endpoint: FacilityInstanceWiringProjectedEndpoint::Source,
            }
        );
    }

    #[test]
    fn final_phase_preserves_the_original_wiring() {
        let wiring = chain_wiring();
        let projected = project_cumulative_wiring(
            &wiring,
            &BTreeSet::from(["facility:a".to_string(), "facility:b".to_string()]),
            2,
        )
        .expect("complete phase should project");

        assert_eq!(projected, wiring);
    }

    fn chain_wiring() -> FacilityInstanceWiringReport {
        let facility = |id: &str| FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("recipe:{id}"),
            facility: "assembler".to_string(),
            index: 0,
            runs_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            work_seconds_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            unused_capacity: Rate {
                numerator: 0,
                denominator: 1,
            },
        };
        FacilityInstanceWiringReport {
            schema_version: crate::recipes::FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                facility("facility:a"),
                facility("facility:b"),
                FacilityInstanceWiringNode::Target {
                    id: "target:final".to_string(),
                    item: "final".to_string(),
                },
            ],
            edges: vec![
                FacilityInstanceWiringEdge::original(
                    "facility:a",
                    "facility:b",
                    "production",
                    "middle",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
                FacilityInstanceWiringEdge::original(
                    "facility:b",
                    "target:final",
                    "target",
                    "final",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
            ],
            diagnostics: Vec::new(),
        }
    }
}
