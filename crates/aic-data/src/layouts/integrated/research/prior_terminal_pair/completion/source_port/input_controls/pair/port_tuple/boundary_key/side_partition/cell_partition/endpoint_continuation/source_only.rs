use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;

mod separator_cut;

pub use separator_cut::*;

pub const ENDPOINT_SOURCE_ONLY_CONTROL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EndpointSourceRegionEvidence {
    pub source_case_index: usize,
    pub parent_case_count: usize,
    pub parent_validated_feasible_count: usize,
    pub parent_proven_infeasible_count: usize,
    pub parent_unknown_count: usize,
    pub parent_invalid_witness_count: usize,
    pub source_only_outcome: Option<ExactDimensionCaseOutcome>,
    pub logical_evidence_compatible: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EndpointSourceOnlyCaseReport {
    pub case_index: usize,
    pub source_case_index: usize,
    pub source_selected: [usize; 2],
    pub source_preceding: Vec<[usize; 2]>,
    pub root_infeasible: bool,
    pub solve: ExternalBoundaryKeySolveReport,
    pub boundary_certificates_equal: bool,
    pub boundary_certificate_satisfied: bool,
    pub continuation_certificates_equal: bool,
    pub source_only_certificate_satisfied: bool,
    pub root_source_restriction_satisfied: bool,
    pub facility_fixation_satisfied: bool,
    pub semantic_model_contract_satisfied: bool,
    pub controlled_axis_model_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EndpointSourceOnlyControlReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: EndpointContinuationPartitionReport,
    pub selected_boundary_key: i32,
    pub selected_network_id: String,
    pub selected_network_index: usize,
    pub selected_item: String,
    pub source_terminal: String,
    pub demand_terminal: String,
    pub source_flow_units: i32,
    pub demand_flow_units: i32,
    pub source_cell: usize,
    pub demand_cell: usize,
    pub source_candidates: Vec<EndpointContinuationCandidate>,
    pub source_partition_non_empty: bool,
    pub source_partition_pairwise_disjoint: bool,
    pub source_partition_exact_cover: bool,
    pub demand_continuation_unrestricted: bool,
    pub parent_source_regions: Vec<EndpointSourceRegionEvidence>,
    pub parent_region_evidence_complete: bool,
    pub worker_count: usize,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub cases: Vec<EndpointSourceOnlyCaseReport>,
    pub root_infeasible_count: usize,
    pub cross_experiment_evidence_compatible: bool,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub witness_found: bool,
    pub interpretation_blocked: bool,
    pub authoritative_wave_wall_ms: u64,
    pub observation_wave_wall_ms: u64,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone)]
struct SourceCaseInput {
    case_index: usize,
    source: EndpointContinuationCandidate,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_endpoint_source_only_control(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
    fixed_x: i32,
    fixed_y: i32,
    port_assignment_index: usize,
    fixed_rotation: i64,
    prior_facility_bit_index: usize,
    terminal_bit_indices: [usize; 2],
    representative_source_leaf_index: usize,
    worker_count: usize,
    prefix_search_budget: Duration,
    initial_pair_case_search_budget: Duration,
    completion_case_search_budget: Duration,
    source_case_search_budget: Duration,
    control_case_search_budget: Duration,
    residual_pair_case_search_budget: Duration,
    parent_observation_search_budget: Duration,
    tuple_authoritative_search_budget: Duration,
    tuple_observation_search_budget: Duration,
    ab_authoritative_search_budget: Duration,
    ab_observation_search_budget: Duration,
    side_authoritative_search_budget: Duration,
    side_observation_search_budget: Duration,
    cell_authoritative_search_budget: Duration,
    cell_observation_search_budget: Duration,
    selected_network_id: String,
    endpoint_authoritative_search_budget: Duration,
    endpoint_observation_search_budget: Duration,
    authoritative_case_search_budget: Duration,
    observation_case_search_budget: Duration,
) -> Result<EndpointSourceOnlyControlReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if worker_count == 0
        || authoritative_case_search_budget.is_zero()
        || observation_case_search_budget.is_zero()
    {
        return Err(invalid_input(
            "/endpoint_source_only_control",
            "worker count and source-only budgets must be positive",
        ));
    }
    let parent = diagnose_endpoint_continuation_partition(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        target_phase_index,
        fixed_width,
        fixed_height,
        fixed_x,
        fixed_y,
        port_assignment_index,
        fixed_rotation,
        prior_facility_bit_index,
        terminal_bit_indices,
        representative_source_leaf_index,
        worker_count,
        prefix_search_budget,
        initial_pair_case_search_budget,
        completion_case_search_budget,
        source_case_search_budget,
        control_case_search_budget,
        residual_pair_case_search_budget,
        parent_observation_search_budget,
        tuple_authoritative_search_budget,
        tuple_observation_search_budget,
        ab_authoritative_search_budget,
        ab_observation_search_budget,
        side_authoritative_search_budget,
        side_observation_search_budget,
        cell_authoritative_search_budget,
        cell_observation_search_budget,
        selected_network_id.clone(),
        endpoint_authoritative_search_budget,
        endpoint_observation_search_budget,
    )?;
    if parent.interpretation_blocked || !parent.mandatory_continuation_proof_satisfied {
        return Err(invalid_input(
            "/parent",
            "source-only control requires an unblocked exact endpoint-continuation parent",
        ));
    }
    let source_partition_non_empty = !parent.source_candidates.is_empty();
    let source_partition_pairwise_disjoint = unique_candidates(&parent.source_candidates);
    let source_partition_exact_cover = source_partition_non_empty
        && source_partition_pairwise_disjoint
        && parent.canonical_partition_exact_cover;
    if !source_partition_exact_cover {
        return Err(invalid_input(
            "/parent/source_candidates",
            "parent does not certify an exact canonical source partition",
        ));
    }

    let cell_parent = &parent.parent;
    let selected_parent = cell_parent
        .cases
        .iter()
        .find(|case| case.key == parent.selected_boundary_key)
        .ok_or_else(|| invalid_input("/parent/cases", "selected boundary child is missing"))?;
    let boundary_parent = &cell_parent.parent.parent;
    let tuple_parent = &boundary_parent.parent;
    let parent_assignments = tuple_parent
        .parent
        .inherited_assignments
        .iter()
        .chain(&tuple_parent.parent.assignments)
        .cloned()
        .collect::<Vec<_>>();
    let requested = parent_assignments
        .iter()
        .chain(&boundary_parent.selected_assignments)
        .cloned()
        .collect::<Vec<_>>();
    assert_distinct_assignments(
        &requested,
        EXPECTED_TOTAL_FIXED_TERMINALS,
        "/selected_case/assignments",
    )?;
    let fixed_ports = exact_ports(&parent_assignments, &boundary_parent.selected_assignments);
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    let input = prepare_target_input(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        &growth,
        target_phase_index,
    )?;
    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: tuple_parent.parent.fixed_dimensions[0],
        height: tuple_parent.parent.fixed_dimensions[1],
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: tuple_parent.parent.partitioned_facility.clone(),
        x: tuple_parent.parent.fixed_coordinate[0],
        y: tuple_parent.parent.fixed_coordinate[1],
        rotation: Some(tuple_parent.parent.fixed_rotation),
    };
    let prior_reference = &tuple_parent.parent.prior_reference;
    let fixation = exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements;
    let selected_terminal = cell_parent.selected_terminal.clone();
    let allowed_keys = vec![parent.selected_boundary_key];
    let case_inputs = parent
        .source_candidates
        .iter()
        .cloned()
        .enumerate()
        .map(|(case_index, source)| SourceCaseInput { case_index, source })
        .collect::<Vec<_>>();
    let experiment_started = Instant::now();

    let authoritative_started = Instant::now();
    let mut authoritative = Vec::with_capacity(case_inputs.len());
    for chunk in case_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .cloned()
                .map(|case| {
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let allowed_keys = allowed_keys.clone();
                    let restriction = source_only_restriction(
                        &parent.selected_network_id,
                        &parent.source_terminal,
                        &parent.demand_terminal,
                        &case,
                    );
                    (
                        case,
                        scope.spawn(move || {
                            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_and_continuation_restricted_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                                input,
                                logistics_components,
                                Some(authoritative_case_search_budget),
                                dimensions,
                                coordinate,
                                fixed_ports,
                                prior_reference,
                                fixation,
                                terminal,
                                allowed_keys,
                                restriction,
                            )
                        }),
                    )
                })
                .collect::<Vec<_>>();
            for (case, handle) in handles {
                authoritative.push((
                    case,
                    handle
                        .join()
                        .expect("authoritative source-only worker panicked"),
                ));
            }
        });
    }
    authoritative.sort_by_key(|(case, _)| case.case_index);
    let authoritative_wave_wall_ms = millis(authoritative_started.elapsed());

    let observation_started = Instant::now();
    let mut observations = Vec::with_capacity(case_inputs.len());
    for chunk in case_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .cloned()
                .map(|case| {
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let allowed_keys = allowed_keys.clone();
                    let restriction = source_only_restriction(
                        &parent.selected_network_id,
                        &parent.source_terminal,
                        &parent.demand_terminal,
                        &case,
                    );
                    (
                        case,
                        scope.spawn(move || {
                            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_and_continuation_restricted_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
                                input,
                                logistics_components,
                                Some(observation_case_search_budget),
                                dimensions,
                                coordinate,
                                fixed_ports,
                                prior_reference,
                                fixation,
                                terminal,
                                allowed_keys,
                                restriction,
                            )
                        }),
                    )
                })
                .collect::<Vec<_>>();
            for (case, handle) in handles {
                observations.push((
                    case,
                    handle
                        .join()
                        .expect("observation source-only worker panicked"),
                ));
            }
        });
    }
    observations.sort_by_key(|(case, _)| case.case_index);
    let observation_wave_wall_ms = millis(observation_started.elapsed());

    let expected_external_terminal_count = cell_parent.parent.parent.static_certificates.len();
    let legal_boundary_keys = exact::reachable_boundary_keys(dimensions.width, dimensions.height);
    let parent_model_scale = selected_parent.solve.model_scale;
    let mut cases = Vec::with_capacity(case_inputs.len());
    for ((case, authoritative_result), (observed_case, observation_result)) in
        authoritative.into_iter().zip(observations)
    {
        if case.case_index != observed_case.case_index {
            return Err(invalid_input(
                "/cases",
                "authoritative and observation source-only cases differ",
            ));
        }
        let (authoritative_layout, authoritative_boundary, authoritative_continuation) =
            authoritative_result;
        let (observation_layout, root_snapshot, observation_boundary, observation_continuation) =
            observation_result;
        let root_snapshot = root_snapshot.ok_or_else(|| {
            invalid_input(
                "/cases/root_snapshot",
                "source-only observation did not return a root snapshot",
            )
        })?;
        let fixation_observation = assess_fixation(&root_snapshot, &requested);
        let solve = solve_report(
            &format!("endpoint-source-only-{}", case.case_index),
            authoritative_layout,
            observation_layout,
            root_snapshot,
            fixation_observation,
        );
        let boundary_certificates_equal = authoritative_boundary == observation_boundary;
        let boundary_certificate_satisfied = boundary_certificates_satisfied(
            &authoritative_boundary,
            expected_external_terminal_count,
            &selected_terminal,
            parent.selected_boundary_key,
            &legal_boundary_keys,
        ) && boundary_certificates_satisfied(
            &observation_boundary,
            expected_external_terminal_count,
            &selected_terminal,
            parent.selected_boundary_key,
            &legal_boundary_keys,
        );
        let continuation_certificates_equal =
            authoritative_continuation == observation_continuation;
        let expected_restriction = source_only_restriction(
            &parent.selected_network_id,
            &parent.source_terminal,
            &parent.demand_terminal,
            &case,
        );
        let source_only_certificate_satisfied = authoritative_continuation.len() == 1
            && observation_continuation.len() == 1
            && continuation_certificate_matches(
                &authoritative_continuation[0],
                &expected_restriction,
                parent.selected_network_index,
                &parent.selected_item,
                parent.source_flow_units,
                parent.demand_flow_units,
            )
            && authoritative_continuation[0].demand_selected.is_none()
            && authoritative_continuation[0].demand_preceding.is_empty();
        let root_source_restriction_satisfied = source_only_root_audit_satisfied(
            &solve.root_snapshot,
            solve.observation_outcome,
            boundary_certificate_satisfied,
            source_only_certificate_satisfied,
            &parent.source_terminal,
            &case,
        );
        let facility_fixation_satisfied =
            root_facility_fixation_satisfied(&solve.root_snapshot, input.instances.len())
                && (!solve.fixation_observation.assertion_applies
                    || solve.fixation_observation.assertion_satisfied);
        let semantic_model_contract_satisfied =
            semantic_model_contract(&solve.authoritative_layout, &solve.observation_layout);
        let restriction_count = 1_u64
            + u64::try_from(case.source.preceding.len())
                .expect("source-only restriction count fits u64");
        let controlled_axis_model_satisfied = solve.model_scale.variables
            == parent_model_scale.variables
            && solve.model_scale.constraints == parent_model_scale.constraints + restriction_count
            && solve.model_scale.incidences == parent_model_scale.incidences + restriction_count
            && solve.model_scale.placement_routing_incidences
                == parent_model_scale.placement_routing_incidences;
        let interpretation_blocked = solve.evidence_conflict
            || solve.combined_outcome == ExactDimensionCaseOutcome::InvalidWitness
            || !boundary_certificates_equal
            || !boundary_certificate_satisfied
            || !continuation_certificates_equal
            || !source_only_certificate_satisfied
            || !root_source_restriction_satisfied
            || !facility_fixation_satisfied
            || !semantic_model_contract_satisfied
            || !controlled_axis_model_satisfied;
        let root_infeasible = solve.root_snapshot.capture_status == "root-infeasible";
        cases.push(EndpointSourceOnlyCaseReport {
            case_index: case.case_index,
            source_case_index: case.source.case_index,
            source_selected: [case.source.from, case.source.to],
            source_preceding: case.source.preceding.clone(),
            root_infeasible,
            solve,
            boundary_certificates_equal,
            boundary_certificate_satisfied,
            continuation_certificates_equal,
            source_only_certificate_satisfied,
            root_source_restriction_satisfied,
            facility_fixation_satisfied,
            semantic_model_contract_satisfied,
            controlled_axis_model_satisfied,
            interpretation_blocked,
        });
    }

    let parent_source_regions = parent
        .source_candidates
        .iter()
        .map(|source| {
            let matching = parent
                .cases
                .iter()
                .filter(|case| case.source_case_index == source.case_index)
                .collect::<Vec<_>>();
            let count = |outcome| {
                matching
                    .iter()
                    .filter(|case| case.solve.combined_outcome == outcome)
                    .count()
            };
            let source_only_outcome = cases
                .iter()
                .find(|case| case.source_case_index == source.case_index)
                .map(|case| case.solve.combined_outcome);
            let logical_evidence_compatible = source_only_outcome.is_some_and(|outcome| {
                source_region_evidence_compatible(
                    matching.len(),
                    count(ExactDimensionCaseOutcome::ValidatedFeasible),
                    count(ExactDimensionCaseOutcome::ProvenInfeasible),
                    outcome,
                )
            });
            EndpointSourceRegionEvidence {
                source_case_index: source.case_index,
                parent_case_count: matching.len(),
                parent_validated_feasible_count: count(
                    ExactDimensionCaseOutcome::ValidatedFeasible,
                ),
                parent_proven_infeasible_count: count(ExactDimensionCaseOutcome::ProvenInfeasible),
                parent_unknown_count: count(ExactDimensionCaseOutcome::Unknown),
                parent_invalid_witness_count: count(ExactDimensionCaseOutcome::InvalidWitness),
                source_only_outcome,
                logical_evidence_compatible,
            }
        })
        .collect::<Vec<_>>();
    let parent_region_evidence_complete = parent_source_regions.iter().all(|region| {
        region.parent_case_count == parent.demand_candidates.len()
            && region.parent_case_count
                == region.parent_validated_feasible_count
                    + region.parent_proven_infeasible_count
                    + region.parent_unknown_count
                    + region.parent_invalid_witness_count
    });
    let cross_experiment_evidence_compatible = parent_source_regions
        .iter()
        .all(|region| region.logical_evidence_compatible);
    let count = |outcome| {
        cases
            .iter()
            .filter(|case| case.solve.combined_outcome == outcome)
            .count()
    };
    let validated_feasible_count = count(ExactDimensionCaseOutcome::ValidatedFeasible);
    let proven_infeasible_count = count(ExactDimensionCaseOutcome::ProvenInfeasible);
    let unknown_count = count(ExactDimensionCaseOutcome::Unknown);
    let invalid_witness_count = count(ExactDimensionCaseOutcome::InvalidWitness);
    let root_infeasible_count = cases.iter().filter(|case| case.root_infeasible).count();
    let witness_found = validated_feasible_count > 0;
    let demand_continuation_unrestricted = cases
        .iter()
        .all(|case| case.source_only_certificate_satisfied);
    let interpretation_blocked = !parent_region_evidence_complete
        || !cross_experiment_evidence_compatible
        || !demand_continuation_unrestricted
        || cases.iter().any(|case| case.interpretation_blocked)
        || invalid_witness_count > 0;
    Ok(EndpointSourceOnlyControlReport {
        schema_version: ENDPOINT_SOURCE_ONLY_CONTROL_SCHEMA_VERSION,
        target_phase_index,
        selected_boundary_key: parent.selected_boundary_key,
        selected_network_id: parent.selected_network_id.clone(),
        selected_network_index: parent.selected_network_index,
        selected_item: parent.selected_item.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_flow_units: parent.source_flow_units,
        demand_flow_units: parent.demand_flow_units,
        source_cell: parent.source_cell,
        demand_cell: parent.demand_cell,
        source_candidates: parent.source_candidates.clone(),
        source_partition_non_empty,
        source_partition_pairwise_disjoint,
        source_partition_exact_cover,
        demand_continuation_unrestricted,
        parent_source_regions,
        parent_region_evidence_complete,
        worker_count,
        authoritative_case_search_budget_ms: millis(authoritative_case_search_budget),
        observation_case_search_budget_ms: millis(observation_case_search_budget),
        cases,
        root_infeasible_count,
        cross_experiment_evidence_compatible,
        validated_feasible_count,
        proven_infeasible_count,
        unknown_count,
        invalid_witness_count,
        witness_found,
        interpretation_blocked,
        authoritative_wave_wall_ms,
        observation_wave_wall_ms,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        diagnostic_only: true,
        parent,
    })
}

fn source_region_evidence_compatible(
    parent_case_count: usize,
    parent_validated_feasible_count: usize,
    parent_proven_infeasible_count: usize,
    source_only_outcome: ExactDimensionCaseOutcome,
) -> bool {
    if parent_validated_feasible_count > 0
        && source_only_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
    {
        return false;
    }
    if parent_case_count > 0
        && parent_proven_infeasible_count == parent_case_count
        && source_only_outcome == ExactDimensionCaseOutcome::ValidatedFeasible
    {
        return false;
    }
    true
}

fn source_only_restriction(
    network_id: &str,
    source_terminal: &str,
    demand_terminal: &str,
    case: &SourceCaseInput,
) -> exact::shared_layer::EndpointContinuationRestriction {
    let convert = |arc: [usize; 2]| exact::shared_layer::DirectedGridArcRestriction {
        from: arc[0],
        to: arc[1],
    };
    exact::shared_layer::EndpointContinuationRestriction {
        network_id: network_id.to_string(),
        source_terminal: source_terminal.to_string(),
        demand_terminal: demand_terminal.to_string(),
        source_selected: convert([case.source.from, case.source.to]),
        source_preceding: case.source.preceding.iter().copied().map(convert).collect(),
        demand_selected: None,
        demand_preceding: Vec::new(),
    }
}

fn source_only_root_restriction_satisfied(
    root: &crate::layouts::RootDomainSnapshot,
    source_terminal: &str,
    case: &SourceCaseInput,
) -> bool {
    root.terminals
        .iter()
        .find(|terminal| terminal.terminal == source_terminal)
        .is_some_and(|terminal| {
            terminal.endpoint_continuation_arcs.iter().any(|arc| {
                [arc.from, arc.to] == [case.source.from, case.source.to]
                    && arc.flow.lower_bound >= 1
            }) && case.source.preceding.iter().all(|excluded| {
                terminal
                    .endpoint_continuation_arcs
                    .iter()
                    .all(|arc| [arc.from, arc.to] != *excluded)
            })
        })
}

fn source_only_root_audit_satisfied(
    root: &crate::layouts::RootDomainSnapshot,
    observation_outcome: ExactDimensionCaseOutcome,
    boundary_certificate_satisfied: bool,
    source_only_certificate_satisfied: bool,
    source_terminal: &str,
    case: &SourceCaseInput,
) -> bool {
    if root.capture_status == "root-infeasible" {
        return observation_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
            && boundary_certificate_satisfied
            && source_only_certificate_satisfied;
    }
    source_only_root_restriction_satisfied(root, source_terminal, case)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(
        case_index: usize,
        from: usize,
        to: usize,
        preceding: Vec<[usize; 2]>,
    ) -> EndpointContinuationCandidate {
        EndpointContinuationCandidate {
            case_index,
            terminal_cell: from,
            terminal_arm_direction: 1,
            from,
            to,
            preceding,
        }
    }

    #[test]
    fn source_only_restriction_leaves_demand_unrestricted() {
        let case = SourceCaseInput {
            case_index: 1,
            source: source(1, 48, 64, vec![[48, 32]]),
        };
        let restriction = source_only_restriction("network", "source", "demand", &case);
        assert_eq!(
            restriction.source_selected,
            exact::shared_layer::DirectedGridArcRestriction { from: 48, to: 64 }
        );
        assert_eq!(restriction.source_preceding.len(), 1);
        assert!(restriction.demand_selected.is_none());
        assert!(restriction.demand_preceding.is_empty());
    }

    #[test]
    fn root_infeasible_source_case_requires_proof_and_certificates() {
        let root = crate::layouts::RootDomainSnapshot::root_infeasible_without_brancher_call();
        let case = SourceCaseInput {
            case_index: 0,
            source: source(0, 48, 32, Vec::new()),
        };
        assert!(source_only_root_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            true,
            true,
            "source",
            &case,
        ));
        assert!(!source_only_root_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::Unknown,
            true,
            true,
            "source",
            &case,
        ));
        assert!(!source_only_root_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            false,
            true,
            "source",
            &case,
        ));
    }

    #[test]
    fn cross_experiment_evidence_rejects_only_real_contradictions() {
        assert!(!source_region_evidence_compatible(
            3,
            0,
            3,
            ExactDimensionCaseOutcome::ValidatedFeasible,
        ));
        assert!(!source_region_evidence_compatible(
            3,
            1,
            0,
            ExactDimensionCaseOutcome::ProvenInfeasible,
        ));
        assert!(source_region_evidence_compatible(
            3,
            0,
            3,
            ExactDimensionCaseOutcome::Unknown,
        ));
        assert!(source_region_evidence_compatible(
            3,
            0,
            1,
            ExactDimensionCaseOutcome::ValidatedFeasible,
        ));
        assert!(source_region_evidence_compatible(
            3,
            0,
            1,
            ExactDimensionCaseOutcome::ProvenInfeasible,
        ));
    }
}
