use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;
use crate::facilities::FacilityPortDirection;

pub const ENDPOINT_CONTINUATION_PARTITION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EndpointContinuationCandidate {
    pub case_index: usize,
    pub terminal_cell: usize,
    pub terminal_arm_direction: usize,
    pub from: usize,
    pub to: usize,
    pub preceding: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EndpointContinuationCaseReport {
    pub case_index: usize,
    pub source_case_index: usize,
    pub demand_case_index: usize,
    pub source_selected: [usize; 2],
    pub source_preceding: Vec<[usize; 2]>,
    pub demand_selected: [usize; 2],
    pub demand_preceding: Vec<[usize; 2]>,
    pub solve: ExternalBoundaryKeySolveReport,
    pub boundary_certificates_equal: bool,
    pub boundary_certificate_satisfied: bool,
    pub continuation_certificates_equal: bool,
    pub continuation_certificate_satisfied: bool,
    pub root_restriction_satisfied: bool,
    pub facility_fixation_satisfied: bool,
    pub semantic_model_contract_satisfied: bool,
    pub controlled_axis_model_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EndpointContinuationPartitionReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: ExternalBoundaryCellPartitionReport,
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
    pub demand_candidates: Vec<EndpointContinuationCandidate>,
    pub endpoint_geometry_singleton: bool,
    pub terminal_presence_fixed: bool,
    pub positive_terminal_flow: bool,
    pub source_and_demand_cells_distinct: bool,
    pub selected_network_has_one_source_and_one_demand: bool,
    pub continuation_sets_non_empty: bool,
    pub canonical_partition_pairwise_disjoint: bool,
    pub canonical_partition_exact_cover: bool,
    pub mandatory_continuation_proof_satisfied: bool,
    pub worker_count: usize,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub cases: Vec<EndpointContinuationCaseReport>,
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
struct CaseInput {
    case_index: usize,
    source: EndpointContinuationCandidate,
    demand: EndpointContinuationCandidate,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_endpoint_continuation_partition(
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
    authoritative_case_search_budget: Duration,
    observation_case_search_budget: Duration,
) -> Result<EndpointContinuationPartitionReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if worker_count == 0
        || authoritative_case_search_budget.is_zero()
        || observation_case_search_budget.is_zero()
    {
        return Err(invalid_input(
            "/endpoint_continuation_partition",
            "worker count and endpoint-continuation budgets must be positive",
        ));
    }
    let parent = diagnose_external_boundary_cell_partition(
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
    )?;
    if parent.interpretation_blocked
        || parent.selected_side_outcome != ExactDimensionCaseOutcome::Unknown
    {
        return Err(invalid_input(
            "/parent",
            "endpoint-continuation partition requires an unblocked unresolved boundary-cell parent",
        ));
    }
    let selected_boundary_key = *parent.unresolved_keys.first().ok_or_else(|| {
        invalid_input(
            "/parent/unresolved_keys",
            "endpoint-continuation partition requires one unresolved boundary key",
        )
    })?;
    let selected_parent = parent
        .cases
        .iter()
        .find(|case| case.key == selected_boundary_key)
        .ok_or_else(|| invalid_input("/parent/cases", "selected boundary child is missing"))?;
    let root = selected_parent.solve.root_snapshot.clone();
    let network = root
        .networks
        .iter()
        .find(|network| network.network_id == selected_network_id)
        .cloned()
        .ok_or_else(|| {
            invalid_input(
                "/selected_network_id",
                "selected network is absent from the parent root snapshot",
            )
        })?;
    let terminals = root
        .terminals
        .iter()
        .filter(|terminal| terminal.network_id == selected_network_id)
        .cloned()
        .collect::<Vec<_>>();
    let sources = terminals
        .iter()
        .filter(|terminal| terminal.direction == FacilityPortDirection::Output)
        .cloned()
        .collect::<Vec<_>>();
    let demands = terminals
        .iter()
        .filter(|terminal| terminal.direction == FacilityPortDirection::Input)
        .cloned()
        .collect::<Vec<_>>();
    let selected_network_has_one_source_and_one_demand = sources.len() == 1 && demands.len() == 1;
    if !selected_network_has_one_source_and_one_demand {
        return Err(invalid_input(
            "/selected_network_id",
            "selected network must have exactly one source and one demand for this experiment",
        ));
    }
    let source = sources[0].clone();
    let demand = demands[0].clone();
    let source_candidates = continuation_candidates(&source);
    let demand_candidates = continuation_candidates(&demand);
    let source_cells = source_candidates
        .iter()
        .map(|candidate| candidate.terminal_cell)
        .collect::<BTreeSet<_>>();
    let demand_cells = demand_candidates
        .iter()
        .map(|candidate| candidate.terminal_cell)
        .collect::<BTreeSet<_>>();
    let endpoint_geometry_singleton = source.geometry.cardinality == 1
        && demand.geometry.cardinality == 1
        && source_cells.len() == 1
        && demand_cells.len() == 1;
    let terminal_presence_fixed = source.routing_options.fixed_true == 1
        && source.routing_options.unresolved == 0
        && demand.routing_options.fixed_true == 1
        && demand.routing_options.unresolved == 0;
    let positive_terminal_flow = source.flow_units > 0 && demand.flow_units > 0;
    let source_cell = source_cells.iter().next().copied().unwrap_or(usize::MAX);
    let demand_cell = demand_cells.iter().next().copied().unwrap_or(usize::MAX);
    let source_and_demand_cells_distinct = source_cell != demand_cell;
    let continuation_sets_non_empty =
        !source_candidates.is_empty() && !demand_candidates.is_empty();
    let canonical_partition_pairwise_disjoint =
        unique_candidates(&source_candidates) && unique_candidates(&demand_candidates);
    let canonical_partition_exact_cover = continuation_sets_non_empty
        && canonical_partition_pairwise_disjoint
        && network.possible_supply_options == 1
        && network.possible_demand_options == 1;
    let mandatory_continuation_proof_satisfied = endpoint_geometry_singleton
        && terminal_presence_fixed
        && positive_terminal_flow
        && source_and_demand_cells_distinct
        && selected_network_has_one_source_and_one_demand
        && canonical_partition_exact_cover;
    if !mandatory_continuation_proof_satisfied {
        return Err(invalid_input(
            "/preflight",
            "root evidence does not prove a complete mandatory endpoint-continuation partition",
        ));
    }

    let boundary_parent = &parent.parent.parent;
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
    let selected_terminal = parent.selected_terminal.clone();
    let allowed_keys = vec![selected_boundary_key];
    let case_inputs = source_candidates
        .iter()
        .flat_map(|source| {
            demand_candidates
                .iter()
                .map(move |demand| (source.clone(), demand.clone()))
        })
        .enumerate()
        .map(|(case_index, (source, demand))| CaseInput {
            case_index,
            source,
            demand,
        })
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
                    let restriction = continuation_restriction(
                        &selected_network_id,
                        source.terminal.as_str(),
                        demand.terminal.as_str(),
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
                        .expect("authoritative endpoint-continuation worker panicked"),
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
                    let restriction = continuation_restriction(
                        &selected_network_id,
                        source.terminal.as_str(),
                        demand.terminal.as_str(),
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
                        .expect("observation endpoint-continuation worker panicked"),
                ));
            }
        });
    }
    observations.sort_by_key(|(case, _)| case.case_index);
    let observation_wave_wall_ms = millis(observation_started.elapsed());

    let expected_external_terminal_count = parent.parent.parent.static_certificates.len();
    let legal_boundary_keys = exact::reachable_boundary_keys(dimensions.width, dimensions.height);
    let parent_model_scale = selected_parent.solve.model_scale;
    let mut cases = Vec::with_capacity(case_inputs.len());
    for ((case, authoritative_result), (observed_case, observation_result)) in
        authoritative.into_iter().zip(observations)
    {
        if case.case_index != observed_case.case_index {
            return Err(invalid_input(
                "/cases",
                "authoritative and observation continuation cases differ",
            ));
        }
        let (authoritative_layout, authoritative_boundary, authoritative_continuation) =
            authoritative_result;
        let (observation_layout, root_snapshot, observation_boundary, observation_continuation) =
            observation_result;
        let root_snapshot = root_snapshot.ok_or_else(|| {
            invalid_input(
                "/cases/root_snapshot",
                "endpoint-continuation observation did not return a root snapshot",
            )
        })?;
        let fixation_observation = assess_fixation(&root_snapshot, &requested);
        let solve = solve_report(
            &format!("endpoint-continuation-{}", case.case_index),
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
            selected_boundary_key,
            &legal_boundary_keys,
        ) && boundary_certificates_satisfied(
            &observation_boundary,
            expected_external_terminal_count,
            &selected_terminal,
            selected_boundary_key,
            &legal_boundary_keys,
        );
        let continuation_certificates_equal =
            authoritative_continuation == observation_continuation;
        let expected_restriction = continuation_restriction(
            &selected_network_id,
            source.terminal.as_str(),
            demand.terminal.as_str(),
            &case,
        );
        let continuation_certificate_satisfied = authoritative_continuation.len() == 1
            && observation_continuation.len() == 1
            && continuation_certificate_matches(
                &authoritative_continuation[0],
                &expected_restriction,
                network.network_index,
                &network.item,
                source.flow_units,
                demand.flow_units,
            );
        let root_restriction_satisfied = root_continuation_audit_satisfied(
            &solve.root_snapshot,
            solve.observation_outcome,
            boundary_certificate_satisfied,
            continuation_certificate_satisfied,
            source.terminal.as_str(),
            demand.terminal.as_str(),
            &case,
        );
        let facility_fixation_satisfied =
            root_facility_fixation_satisfied(&solve.root_snapshot, input.instances.len())
                && (!solve.fixation_observation.assertion_applies
                    || solve.fixation_observation.assertion_satisfied);
        let semantic_model_contract_satisfied =
            semantic_model_contract(&solve.authoritative_layout, &solve.observation_layout);
        let restriction_count = 2_u64
            + u64::try_from(case.source.preceding.len() + case.demand.preceding.len())
                .expect("continuation restriction count fits u64");
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
            || !continuation_certificate_satisfied
            || !root_restriction_satisfied
            || !facility_fixation_satisfied
            || !semantic_model_contract_satisfied
            || !controlled_axis_model_satisfied;
        cases.push(EndpointContinuationCaseReport {
            case_index: case.case_index,
            source_case_index: case.source.case_index,
            demand_case_index: case.demand.case_index,
            source_selected: [case.source.from, case.source.to],
            source_preceding: case.source.preceding.clone(),
            demand_selected: [case.demand.from, case.demand.to],
            demand_preceding: case.demand.preceding.clone(),
            solve,
            boundary_certificates_equal,
            boundary_certificate_satisfied,
            continuation_certificates_equal,
            continuation_certificate_satisfied,
            root_restriction_satisfied,
            facility_fixation_satisfied,
            semantic_model_contract_satisfied,
            controlled_axis_model_satisfied,
            interpretation_blocked,
        });
    }

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
    let witness_found = validated_feasible_count > 0;
    let interpretation_blocked =
        cases.iter().any(|case| case.interpretation_blocked) || invalid_witness_count > 0;
    Ok(EndpointContinuationPartitionReport {
        schema_version: ENDPOINT_CONTINUATION_PARTITION_SCHEMA_VERSION,
        target_phase_index,
        parent,
        selected_boundary_key,
        selected_network_id,
        selected_network_index: network.network_index,
        selected_item: network.item.clone(),
        source_terminal: source.terminal.clone(),
        demand_terminal: demand.terminal.clone(),
        source_flow_units: source.flow_units,
        demand_flow_units: demand.flow_units,
        source_cell,
        demand_cell,
        source_candidates,
        demand_candidates,
        endpoint_geometry_singleton,
        terminal_presence_fixed,
        positive_terminal_flow,
        source_and_demand_cells_distinct,
        selected_network_has_one_source_and_one_demand,
        continuation_sets_non_empty,
        canonical_partition_pairwise_disjoint,
        canonical_partition_exact_cover,
        mandatory_continuation_proof_satisfied,
        worker_count,
        authoritative_case_search_budget_ms: millis(authoritative_case_search_budget),
        observation_case_search_budget_ms: millis(observation_case_search_budget),
        cases,
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
    })
}

fn continuation_candidates(
    terminal: &crate::layouts::RootTerminalDomainSnapshot,
) -> Vec<EndpointContinuationCandidate> {
    let mut arcs = terminal.endpoint_continuation_arcs.clone();
    arcs.sort_by_key(|arc| (arc.from, arc.to));
    arcs.into_iter()
        .enumerate()
        .map(|(case_index, arc)| EndpointContinuationCandidate {
            case_index,
            terminal_cell: arc.terminal_cell,
            terminal_arm_direction: arc.terminal_arm_direction,
            from: arc.from,
            to: arc.to,
            preceding: terminal
                .endpoint_continuation_arcs
                .iter()
                .map(|candidate| [candidate.from, candidate.to])
                .filter(|candidate| *candidate < [arc.from, arc.to])
                .collect(),
        })
        .collect()
}

fn unique_candidates(candidates: &[EndpointContinuationCandidate]) -> bool {
    candidates
        .iter()
        .map(|candidate| (candidate.from, candidate.to))
        .collect::<BTreeSet<_>>()
        .len()
        == candidates.len()
}

fn continuation_restriction(
    network_id: &str,
    source_terminal: &str,
    demand_terminal: &str,
    case: &CaseInput,
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
        demand_selected: convert([case.demand.from, case.demand.to]),
        demand_preceding: case.demand.preceding.iter().copied().map(convert).collect(),
    }
}

fn boundary_certificates_satisfied(
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    expected_count: usize,
    selected_terminal: &str,
    selected_key: i32,
    legal_keys: &[i32],
) -> bool {
    certificates.len() == expected_count
        && certificates
            .iter()
            .map(|certificate| certificate.terminal.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == expected_count
        && certificates.iter().all(|certificate| {
            let selected = certificate.terminal == selected_terminal;
            let selected_values = [selected_key];
            let expected = if selected {
                selected_values.as_slice()
            } else {
                legal_keys
            };
            certificate.declared_domain_kind == "sparse-legal"
                && certificate.declared_lower_bound == expected[0]
                && certificate.declared_upper_bound == *expected.last().expect("non-empty")
                && certificate.declared_values == expected
                && certificate.unary_table_projection == expected
                && certificate.routing_option_keys == expected
                && if selected {
                    certificate.restriction_values.as_deref() == Some(selected_values.as_slice())
                } else {
                    certificate.restriction_values.is_none()
                }
        })
}

fn continuation_certificate_matches(
    certificate: &exact::shared_layer::EndpointContinuationBuildCertificate,
    restriction: &exact::shared_layer::EndpointContinuationRestriction,
    network_index: usize,
    item: &str,
    source_flow_units: i32,
    demand_flow_units: i32,
) -> bool {
    certificate.network_id == restriction.network_id
        && certificate.network_index == network_index
        && certificate.item == item
        && certificate.source_terminal == restriction.source_terminal
        && certificate.source_flow_units == source_flow_units
        && certificate.source_selected == restriction.source_selected
        && certificate.source_preceding == restriction.source_preceding
        && certificate.demand_terminal == restriction.demand_terminal
        && certificate.demand_flow_units == demand_flow_units
        && certificate.demand_selected == restriction.demand_selected
        && certificate.demand_preceding == restriction.demand_preceding
}

fn root_continuation_restriction_satisfied(
    root: &crate::layouts::RootDomainSnapshot,
    source_terminal: &str,
    demand_terminal: &str,
    case: &CaseInput,
) -> bool {
    let terminal_satisfied = |terminal_id: &str, selected: [usize; 2], preceding: &[[usize; 2]]| {
        root.terminals
            .iter()
            .find(|terminal| terminal.terminal == terminal_id)
            .is_some_and(|terminal| {
                terminal
                    .endpoint_continuation_arcs
                    .iter()
                    .any(|arc| [arc.from, arc.to] == selected && arc.flow.lower_bound >= 1)
                    && preceding.iter().all(|excluded| {
                        terminal
                            .endpoint_continuation_arcs
                            .iter()
                            .all(|arc| [arc.from, arc.to] != *excluded)
                    })
            })
    };
    terminal_satisfied(
        source_terminal,
        [case.source.from, case.source.to],
        &case.source.preceding,
    ) && terminal_satisfied(
        demand_terminal,
        [case.demand.from, case.demand.to],
        &case.demand.preceding,
    )
}

#[allow(clippy::too_many_arguments)]
fn root_continuation_audit_satisfied(
    root: &crate::layouts::RootDomainSnapshot,
    observation_outcome: ExactDimensionCaseOutcome,
    boundary_certificate_satisfied: bool,
    continuation_certificate_satisfied: bool,
    source_terminal: &str,
    demand_terminal: &str,
    case: &CaseInput,
) -> bool {
    if root.capture_status == "root-infeasible" {
        return observation_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
            && boundary_certificate_satisfied
            && continuation_certificate_satisfied;
    }
    root_continuation_restriction_satisfied(root, source_terminal, demand_terminal, case)
}

fn semantic_model_contract(
    authoritative: &IntegratedLayoutReport,
    observation: &IntegratedLayoutReport,
) -> bool {
    let (Some(authoritative), Some(observation)) = (&authoritative.exact, &observation.exact)
    else {
        return false;
    };
    authoritative.formulation == observation.formulation
        && authoritative.model == observation.model
        && authoritative.model_complexity == observation.model_complexity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
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
    fn canonical_case_restriction_fixes_only_selected_and_earlier_arcs() {
        let case = CaseInput {
            case_index: 5,
            source: candidate(1, 10, 11, vec![[10, 9]]),
            demand: candidate(2, 6, 7, vec![[5, 7], [6, 7]]),
        };
        let restriction = continuation_restriction("network", "source", "demand", &case);

        assert_eq!(
            restriction.source_selected,
            exact::shared_layer::DirectedGridArcRestriction { from: 10, to: 11 }
        );
        assert_eq!(
            restriction.source_preceding,
            vec![exact::shared_layer::DirectedGridArcRestriction { from: 10, to: 9 }]
        );
        assert_eq!(
            restriction.demand_preceding,
            vec![
                exact::shared_layer::DirectedGridArcRestriction { from: 5, to: 7 },
                exact::shared_layer::DirectedGridArcRestriction { from: 6, to: 7 },
            ]
        );
    }

    #[test]
    fn duplicate_arc_candidates_fail_disjointness_gate() {
        assert!(unique_candidates(&[
            candidate(0, 1, 2, Vec::new()),
            candidate(1, 2, 3, vec![[1, 2]]),
        ]));
        assert!(!unique_candidates(&[
            candidate(0, 1, 2, Vec::new()),
            candidate(1, 1, 2, Vec::new()),
        ]));
    }

    #[test]
    fn root_infeasibility_is_valid_only_with_proof_and_exact_certificates() {
        let root = crate::layouts::RootDomainSnapshot::root_infeasible_without_brancher_call();
        let case = CaseInput {
            case_index: 0,
            source: candidate(0, 1, 2, Vec::new()),
            demand: candidate(0, 3, 4, Vec::new()),
        };
        assert!(root_continuation_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            true,
            true,
            "source",
            "demand",
            &case,
        ));
        assert!(!root_continuation_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::Unknown,
            true,
            true,
            "source",
            "demand",
            &case,
        ));
        assert!(!root_continuation_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            false,
            true,
            "source",
            "demand",
            &case,
        ));
    }

    #[test]
    fn boundary_certificate_requires_unselected_terminals_to_keep_the_full_domain() {
        let certificate =
            |terminal: &str, values: Vec<i32>, restriction_values: Option<Vec<i32>>| {
                exact::shared_layer::BoundaryKeyBuildCertificate {
                    terminal: terminal.to_string(),
                    network_index: usize::from(terminal == "other"),
                    network_id: format!("network:{terminal}"),
                    declared_domain_kind: "sparse-legal".to_string(),
                    declared_lower_bound: values[0],
                    declared_upper_bound: *values.last().expect("non-empty"),
                    declared_values: values.clone(),
                    unary_table_projection: values.clone(),
                    routing_option_keys: values,
                    restriction_values,
                }
            };
        let selected = certificate("selected", vec![0], Some(vec![0]));
        let other = certificate("other", vec![0, 1], None);
        assert!(boundary_certificates_satisfied(
            &[selected.clone(), other],
            2,
            "selected",
            0,
            &[0, 1],
        ));
        let incorrectly_restricted = certificate("other", vec![0], Some(vec![0]));
        assert!(!boundary_certificates_satisfied(
            &[selected, incorrectly_restricted],
            2,
            "selected",
            0,
            &[0, 1],
        ));
    }
}
