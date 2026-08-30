use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::growth::plan_facility_growth;
use crate::layouts::placement::{
    solve_facility_placement_feasibly_with_time_limit,
    solve_hinted_facility_placement_with_time_limit,
};
use crate::layouts::{FacilityPlacement, FacilityPlacementRequest, FacilityPlacementStatus};
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
    CandidateCounts, DeterministicCandidateKey, FacilityChangeCounts, IncumbentProvenance,
    IntegratedLayoutDiagnostic, IntegratedLayoutIncumbentSummary, IntegratedLayoutPhase,
    IntegratedLayoutPhaseAttempt, IntegratedLayoutPhaseOptimization, IntegratedLayoutReport,
    IntegratedLayoutStatus, IterativeOptimizationConfig, LayoutScore, OptimizationProofStatus,
    OptimizationTerminationReason, PRODUCTION_FACILITY_GAP, PhaseElapsedMilliseconds,
    RefinementKind, RouteChangeCounts, frame_placements_for_routing, prepare_model,
    route_turn_count, sparse, validate_iterative_optimization_config,
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
    let time_limit = Duration::from_millis(config.total_time_limit_ms);
    let strategy_deadline = Instant::now()
        .checked_add(time_limit)
        .unwrap_or_else(Instant::now);
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

    for phase in &growth.phases {
        let phase_started = Instant::now();
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
        let phase_time_limit = strategy_deadline.saturating_duration_since(Instant::now());
        if phase_time_limit.is_zero() {
            let mut report = IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "iterative-scc-strategy-time-limit",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    "iterative SCC graph construction exhausted the total strategy deadline",
                ),
            );
            report.phases = snapshots;
            return report;
        }

        let prior_reference = anchors.clone();
        let placement_started = Instant::now();
        let placement = if anchors.is_empty() {
            solve_facility_placement_feasibly_with_time_limit(
                &partial_wiring,
                facilities,
                request,
                PRODUCTION_FACILITY_GAP,
                phase_time_limit,
            )
        } else {
            solve_hinted_facility_placement_with_time_limit(
                &partial_wiring,
                facilities,
                request,
                PRODUCTION_FACILITY_GAP,
                &anchors,
                phase_time_limit,
            )
        };
        let placement_ms = elapsed_milliseconds(placement_started);
        let mut attempt_reports = Vec::with_capacity(2);
        let mut selected = None;
        let mut candidate_counts = CandidateCounts::default();
        let mut routing_ms = 0;
        if !placement.success {
            attempt_reports.push(IntegratedLayoutPhaseAttempt {
                candidate_key: None,
                policy_id: None,
                placement_hint_count: anchors.len(),
                status: placement_status(placement.status),
                diagnostic_code: placement
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.code.to_string()),
            });
        } else {
            candidate_counts.generated += 1;
            let input = match prepare_model(&partial_wiring, facilities, items, transports, request)
            {
                Ok(input) => input,
                Err(diagnostic) => {
                    let mut report = IntegratedLayoutReport::failure(
                        IntegratedLayoutStatus::InvalidInput,
                        diagnostic,
                    );
                    report.phases = snapshots;
                    return report;
                }
            };
            let framed_placements = frame_placements_for_routing(
                placement.placements,
                request.max_width,
                request.max_height,
            );
            if let Some(framed_placements) = framed_placements {
                let routing_started = Instant::now();
                candidate_counts.routed += 1;
                let mut routed = sparse::construct_from_placements(
                    input,
                    logistics_components,
                    framed_placements,
                    strategy_deadline,
                );
                if !routed.success {
                    candidate_counts.rejected += 1;
                    let fallback_input = match prepare_model(
                        &partial_wiring,
                        facilities,
                        items,
                        transports,
                        request,
                    ) {
                        Ok(input) => input,
                        Err(diagnostic) => {
                            let mut report = IntegratedLayoutReport::failure(
                                IntegratedLayoutStatus::InvalidInput,
                                diagnostic,
                            );
                            report.phases = snapshots;
                            return report;
                        }
                    };
                    candidate_counts.generated += 1;
                    candidate_counts.routed += 1;
                    routed = sparse::construct_until(
                        fallback_input,
                        logistics_components,
                        strategy_deadline,
                    );
                }
                routing_ms = elapsed_milliseconds(routing_started);
                attempt_reports.push(IntegratedLayoutPhaseAttempt {
                    candidate_key: Some(DeterministicCandidateKey {
                        phase_index: phase.index,
                        refinement_kind: RefinementKind::GrowthNeighborhood,
                        neighborhood_rank: 3,
                        restart_index: 0,
                        policy_index: 0,
                        attempt_index: candidate_counts.routed.saturating_sub(1),
                        yield_index: 0,
                    }),
                    policy_id: None,
                    placement_hint_count: anchors.len(),
                    status: routed.status,
                    diagnostic_code: routed
                        .diagnostics
                        .first()
                        .map(|diagnostic| diagnostic.code.to_string()),
                });
                if routed.success {
                    candidate_counts.validated += 1;
                    candidate_counts.improved += 1;
                    selected = Some(routed);
                } else if candidate_counts.rejected < candidate_counts.routed {
                    candidate_counts.rejected += 1;
                }
            } else {
                attempt_reports.push(IntegratedLayoutPhaseAttempt {
                    candidate_key: None,
                    policy_id: None,
                    placement_hint_count: anchors.len(),
                    status: IntegratedLayoutStatus::Unknown,
                    diagnostic_code: Some("iterative-routing-frame-does-not-fit".to_string()),
                });
            }
        }

        let Some(mut phase_report) = selected else {
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
        let final_score = LayoutScore::from_report(&phase_report, &prior_reference)
            .expect("successful iterative phase must have a score");
        let candidate_key = DeterministicCandidateKey {
            phase_index: phase.index,
            refinement_kind: RefinementKind::GrowthNeighborhood,
            neighborhood_rank: 3,
            restart_index: 0,
            policy_index: 0,
            attempt_index: candidate_counts.routed.saturating_sub(1),
            yield_index: 0,
        };
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
            initial_incumbent: None,
            final_incumbent: IntegratedLayoutIncumbentSummary {
                score: final_score,
                candidate_key,
                provenance: IncumbentProvenance::NeighborhoodCandidate {
                    neighborhood_rank: 3,
                    attempt_index: candidate_key.attempt_index,
                },
            },
            score_delta: None,
            candidate_counts,
            facility_changes: FacilityChangeCounts {
                unchanged_prior,
                moved_prior: final_score.moved_prior_facility_count,
                newly_placed,
                rotation_changed: final_score.rotation_change_count,
            },
            route_changes: RouteChangeCounts {
                new: phase_report.routes.len(),
                ..RouteChangeCounts::default()
            },
            elapsed_ms: PhaseElapsedMilliseconds {
                graph_construction: graph_construction_ms,
                incumbent_extension: 0,
                placement: placement_ms,
                routing: routing_ms,
                validation: None,
                total: elapsed_milliseconds(phase_started),
            },
            termination_reason: OptimizationTerminationReason::NeighborhoodScheduleExhausted,
            optimality: OptimizationProofStatus::NotAttempted,
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
        latest_success = Some(phase_report);
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
    report
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
