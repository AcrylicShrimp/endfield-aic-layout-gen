use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;

mod guarded_core;

pub use guarded_core::{
    GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION, GuardedCoreAcceptedFixture,
    GuardedCoreInitialGateReport, GuardedCoreInitialGateStatus, GuardedCoreReplayReport,
    GuardedCoreReplayStatus, GuardedCoreSequentialShrinkReport, GuardedCoreSequentialShrinkStatus,
    GuardedCoreShrinkAttempt, diagnose_guarded_core_initial_gate, diagnose_guarded_core_replay,
    diagnose_guarded_core_sequential_shrinking,
};

pub const MATERIAL_ROW5_SEPARATOR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MaterialRow5SeparatorCaseReport {
    pub case_index: Option<usize>,
    pub selected_arc: Option<[usize; 2]>,
    pub preceding_arcs: Vec<[usize; 2]>,
    pub root_infeasible: bool,
    pub solve: ExternalBoundaryKeySolveReport,
    pub(in crate::layouts::integrated) authoritative_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) observation_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) authoritative_continuation_certificates:
        Vec<exact::shared_layer::EndpointContinuationBuildCertificate>,
    pub(in crate::layouts::integrated) observation_continuation_certificates:
        Vec<exact::shared_layer::EndpointContinuationBuildCertificate>,
    pub(in crate::layouts::integrated) authoritative_separator_certificates:
        Vec<exact::shared_layer::MaterialSeparatorBuildCertificate>,
    pub(in crate::layouts::integrated) observation_separator_certificates:
        Vec<exact::shared_layer::MaterialSeparatorBuildCertificate>,
    pub(in crate::layouts::integrated) authoritative_junction_certificates:
        Vec<exact::shared_layer::MaterialJunctionBuildCertificate>,
    pub(in crate::layouts::integrated) observation_junction_certificates:
        Vec<exact::shared_layer::MaterialJunctionBuildCertificate>,
    pub boundary_certificates_equal: bool,
    pub boundary_certificate_satisfied: bool,
    pub continuation_certificates_equal: bool,
    pub source_only_certificate_satisfied: bool,
    pub separator_certificates_equal: bool,
    pub ordered_separator_identity_satisfied: bool,
    pub junction_certificates_equal: bool,
    pub junction_e_certificate_satisfied: bool,
    pub inherited_boundary_matches_control: bool,
    pub inherited_continuation_matches_control: bool,
    pub inherited_row4_separator_matches_control: bool,
    pub inherited_junction_matches_control: bool,
    pub inherited_certificates_match_control: bool,
    pub theorem_premises_satisfied: bool,
    pub root_restriction_observed: bool,
    pub root_restriction_satisfied: bool,
    pub facility_fixation_observed: bool,
    pub facility_fixation_satisfied: bool,
    pub semantic_model_contract_satisfied: bool,
    pub hidden_domain_delta_observed: bool,
    pub hidden_domain_delta_satisfied: bool,
    pub complete_family_delta_satisfied: bool,
    pub controlled_axis_model_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MaterialRow5SeparatorReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: MaterialJunctionContinuationReport,
    pub selected_junction_case_index: usize,
    pub selected_network_id: String,
    pub selected_network_index: usize,
    pub selected_item: String,
    pub selected_item_code: i32,
    pub source_terminal: String,
    pub demand_terminal: String,
    pub source_flow_units: i32,
    pub demand_flow_units: i32,
    pub source_cell: usize,
    pub source_continuation_cell: usize,
    pub demand_cell: usize,
    pub fixed_dimensions: [i32; 2],
    pub separator_after_row: usize,
    pub candidates: Vec<[usize; 2]>,
    pub partition_non_empty: bool,
    pub partition_pairwise_disjoint: bool,
    pub partition_exact_cover_within_e: bool,
    pub sibling_s_unresolved: bool,
    pub demand_continuation_unrestricted: bool,
    pub worker_count: usize,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub execution_order: Vec<String>,
    pub control: MaterialRow5SeparatorCaseReport,
    pub cases: Vec<MaterialRow5SeparatorCaseReport>,
    pub control_parent_evidence_compatible: bool,
    pub child_control_evidence_compatible: bool,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub witness_found: bool,
    pub e_proven_infeasible: bool,
    pub interpretation_blocked: bool,
    pub control_authoritative_wall_ms: u64,
    pub control_observation_wall_ms: u64,
    pub authoritative_wave_wall_ms: u64,
    pub observation_wave_wall_ms: u64,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone)]
struct Row5CaseInput {
    case_index: Option<usize>,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_material_row5_separator(
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
    source_only_authoritative_search_budget: Duration,
    source_only_observation_search_budget: Duration,
    row4_separator_authoritative_search_budget: Duration,
    row4_separator_observation_search_budget: Duration,
    junction_authoritative_search_budget: Duration,
    junction_observation_search_budget: Duration,
    authoritative_case_search_budget: Duration,
    observation_case_search_budget: Duration,
) -> Result<MaterialRow5SeparatorReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if worker_count == 0
        || authoritative_case_search_budget.is_zero()
        || observation_case_search_budget.is_zero()
    {
        return Err(invalid_input(
            "/material_row5_separator",
            "worker count and row-5 budgets must be positive",
        ));
    }
    let parent = diagnose_material_junction_continuation(
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
        selected_network_id,
        endpoint_authoritative_search_budget,
        endpoint_observation_search_budget,
        source_only_authoritative_search_budget,
        source_only_observation_search_budget,
        4,
        row4_separator_authoritative_search_budget,
        row4_separator_observation_search_budget,
        junction_authoritative_search_budget,
        junction_observation_search_budget,
    )?;
    let e_parent = parent.cases.first().ok_or_else(|| {
        invalid_input("/parent/cases", "row-5 separator requires junction child E")
    })?;
    let sibling_s_unresolved = parent.cases.get(1).is_some_and(|case| {
        case.case_index == Some(1)
            && case.solve.combined_outcome == ExactDimensionCaseOutcome::Unknown
            && !case.interpretation_blocked
    });
    if parent.interpretation_blocked
        || !parent.partition_exact_cover
        || !parent.demand_continuation_unrestricted
        || parent.target_phase_index != 3
        || parent.fixed_dimensions != [16, 16]
        || parent.selected_network_id != "network:pipe:item-liquid-xiranite-poly"
        || parent.selected_item != "item-liquid-xiranite-poly"
        || parent.selected_item_code != 5
        || parent.source_cell != 48
        || parent.demand_cell != 113
        || parent.junction_cell != 80
        || parent.candidates != [[80, 81], [80, 96]]
        || e_parent.case_index != Some(0)
        || e_parent.selected_arc != Some([80, 81])
        || e_parent.solve.combined_outcome != ExactDimensionCaseOutcome::Unknown
        || e_parent.interpretation_blocked
        || !sibling_s_unresolved
    {
        return Err(invalid_input(
            "/parent/accepted_fixture",
            "row-5 separator requires the unblocked Phase 3 junction-E fixture with unresolved S",
        ));
    }

    let row4_parent = &parent.parent;
    let source_parent = &row4_parent.parent;
    let endpoint_parent = &source_parent.parent;
    let cell_parent = &endpoint_parent.parent;
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
    if input.instances.len() != 4 || requested.len() != EXPECTED_TOTAL_FIXED_TERMINALS {
        return Err(invalid_input(
            "/parent/fixation",
            "accepted row-5 fixture must retain four facilities and every fixed terminal",
        ));
    }
    let expected_item_upper = i32::try_from(
        input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Pipe)
            .count(),
    )
    .map_err(|_| invalid_input("/parent/networks", "pipe network count exceeds i32"))?;
    let expected_flow_upper = input
        .networks
        .iter()
        .filter(|network| network.transport() == TransportKind::Pipe)
        .map(|network| network.line_capacity_units())
        .max()
        .ok_or_else(|| invalid_input("/parent/networks", "accepted fixture has no pipe layer"))?;
    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: tuple_parent.parent.fixed_dimensions[0],
        height: tuple_parent.parent.fixed_dimensions[1],
    };
    if [dimensions.width, dimensions.height] != [fixed_width, fixed_height] {
        return Err(invalid_input(
            "/fixed_dimensions",
            "row-5 dimensions differ from the reconstructed parent",
        ));
    }
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: tuple_parent.parent.partitioned_facility.clone(),
        x: tuple_parent.parent.fixed_coordinate[0],
        y: tuple_parent.parent.fixed_coordinate[1],
        rotation: Some(tuple_parent.parent.fixed_rotation),
    };
    let prior_reference = &tuple_parent.parent.prior_reference;
    let fixation = exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements;
    let selected_terminal = cell_parent.selected_terminal.clone();
    let allowed_keys = vec![endpoint_parent.selected_boundary_key];
    let selected_source_case = source_parent
        .cases
        .iter()
        .find(|case| case.source_selected == [48, 64])
        .ok_or_else(|| invalid_input("/parent/source", "selected source case is missing"))?;
    let continuation = exact::shared_layer::EndpointContinuationRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_selected: exact::shared_layer::DirectedGridArcRestriction { from: 48, to: 64 },
        source_preceding: selected_source_case
            .source_preceding
            .iter()
            .map(|arc| exact::shared_layer::DirectedGridArcRestriction {
                from: arc[0],
                to: arc[1],
            })
            .collect(),
        demand_selected: None,
        demand_preceding: Vec::new(),
    };
    let row4 = exact::shared_layer::MaterialSeparatorRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_cell: parent.source_cell,
        source_continuation_cell: 64,
        demand_cell: parent.demand_cell,
        separator_after_row: 4,
        selected_case_index: Some(0),
    };
    let junction = exact::shared_layer::MaterialJunctionRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_cell: parent.source_cell,
        demand_cell: parent.demand_cell,
        incoming: exact::shared_layer::DirectedGridArcRestriction { from: 64, to: 80 },
        junction_cell: 80,
        candidates: vec![
            exact::shared_layer::DirectedGridArcRestriction { from: 80, to: 81 },
            exact::shared_layer::DirectedGridArcRestriction { from: 80, to: 96 },
        ],
        selected_case_index: Some(0),
    };
    let row5_candidates = (0..16).map(|x| [80 + x, 96 + x]).collect::<Vec<_>>();
    let row5_base = exact::shared_layer::MaterialSeparatorRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_cell: parent.source_cell,
        source_continuation_cell: 81,
        demand_cell: parent.demand_cell,
        separator_after_row: 5,
        selected_case_index: None,
    };
    let experiment_started = Instant::now();
    let control_input = Row5CaseInput { case_index: None };

    let control_authoritative_started = Instant::now();
    let control_authoritative = solve_authoritative(
        input.clone(),
        logistics_components,
        authoritative_case_search_budget,
        dimensions,
        coordinate.clone(),
        fixed_ports.clone(),
        prior_reference,
        fixation,
        selected_terminal.clone(),
        allowed_keys.clone(),
        continuation.clone(),
        vec![row4.clone(), row5_base.clone()],
        junction.clone(),
    );
    let control_authoritative_wall_ms = millis(control_authoritative_started.elapsed());

    let child_inputs = (0..row5_candidates.len())
        .map(|case_index| Row5CaseInput {
            case_index: Some(case_index),
        })
        .collect::<Vec<_>>();
    let authoritative_started = Instant::now();
    let mut authoritative = Vec::with_capacity(child_inputs.len());
    for chunk in child_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .cloned()
                .map(|case| {
                    let mut row5 = row5_base.clone();
                    row5.selected_case_index = case.case_index;
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let keys = allowed_keys.clone();
                    let continuation = continuation.clone();
                    let separators = vec![row4.clone(), row5];
                    let junction = junction.clone();
                    (
                        case,
                        scope.spawn(move || {
                            solve_authoritative(
                                input,
                                logistics_components,
                                authoritative_case_search_budget,
                                dimensions,
                                coordinate,
                                fixed_ports,
                                prior_reference,
                                fixation,
                                terminal,
                                keys,
                                continuation,
                                separators,
                                junction,
                            )
                        }),
                    )
                })
                .collect::<Vec<_>>();
            for (case, handle) in handles {
                authoritative.push((case, handle.join().expect("row-5 worker panicked")));
            }
        });
    }
    authoritative.sort_by_key(|(case, _)| case.case_index);
    let authoritative_wave_wall_ms = millis(authoritative_started.elapsed());

    let control_observation_started = Instant::now();
    let control_observation = solve_observation(
        input.clone(),
        logistics_components,
        observation_case_search_budget,
        dimensions,
        coordinate.clone(),
        fixed_ports.clone(),
        prior_reference,
        fixation,
        selected_terminal.clone(),
        allowed_keys.clone(),
        continuation.clone(),
        vec![row4.clone(), row5_base.clone()],
        junction.clone(),
    );
    let control_observation_wall_ms = millis(control_observation_started.elapsed());

    let observation_started = Instant::now();
    let mut observations = Vec::with_capacity(child_inputs.len());
    for chunk in child_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .cloned()
                .map(|case| {
                    let mut row5 = row5_base.clone();
                    row5.selected_case_index = case.case_index;
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let keys = allowed_keys.clone();
                    let continuation = continuation.clone();
                    let separators = vec![row4.clone(), row5];
                    let junction = junction.clone();
                    (
                        case,
                        scope.spawn(move || {
                            solve_observation(
                                input,
                                logistics_components,
                                observation_case_search_budget,
                                dimensions,
                                coordinate,
                                fixed_ports,
                                prior_reference,
                                fixation,
                                terminal,
                                keys,
                                continuation,
                                separators,
                                junction,
                            )
                        }),
                    )
                })
                .collect::<Vec<_>>();
            for (case, handle) in handles {
                observations.push((case, handle.join().expect("row-5 observer panicked")));
            }
        });
    }
    observations.sort_by_key(|(case, _)| case.case_index);
    let observation_wave_wall_ms = millis(observation_started.elapsed());

    let expected_external_terminal_count = cell_parent.parent.parent.static_certificates.len();
    let legal_boundary_keys = exact::reachable_boundary_keys(dimensions.width, dimensions.height);
    let control = build_case_report(
        &control_input,
        control_authoritative,
        control_observation,
        &requested,
        input.instances.len(),
        expected_external_terminal_count,
        &selected_terminal,
        endpoint_parent.selected_boundary_key,
        &legal_boundary_keys,
        &continuation,
        &row4,
        &row5_base,
        &junction,
        endpoint_parent,
        &row4_parent.candidates,
        &row5_candidates,
        &parent.candidates,
        [dimensions.width, dimensions.height],
        expected_flow_upper,
        expected_item_upper,
        &e_parent.solve,
        None,
        None,
    )?;
    let mut cases = Vec::with_capacity(child_inputs.len());
    for ((case, authoritative_result), (observed_case, observation_result)) in
        authoritative.into_iter().zip(observations)
    {
        if case.case_index != observed_case.case_index {
            return Err(invalid_input(
                "/cases",
                "authoritative and observation row-5 cases differ",
            ));
        }
        let selected = case.case_index.expect("child has selected case");
        let mut row5 = row5_base.clone();
        row5.selected_case_index = Some(selected);
        cases.push(build_case_report(
            &case,
            authoritative_result,
            observation_result,
            &requested,
            input.instances.len(),
            expected_external_terminal_count,
            &selected_terminal,
            endpoint_parent.selected_boundary_key,
            &legal_boundary_keys,
            &continuation,
            &row4,
            &row5,
            &junction,
            endpoint_parent,
            &row4_parent.candidates,
            &row5_candidates,
            &parent.candidates,
            [dimensions.width, dimensions.height],
            expected_flow_upper,
            expected_item_upper,
            &control.solve,
            Some(&control),
            Some(selected),
        )?);
    }

    let control_parent_evidence_compatible = outcomes_compatible(
        e_parent.solve.combined_outcome,
        control.solve.combined_outcome,
    );
    let child_control_evidence_compatible =
        row5_child_evidence_compatible(control.solve.combined_outcome, &cases);
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
    let partition_non_empty = !row5_candidates.is_empty();
    let partition_pairwise_disjoint =
        row5_candidates.len() == 16 && row5_candidates.iter().collect::<BTreeSet<_>>().len() == 16;
    let expected_candidates = (0..16).map(|x| [80 + x, 96 + x]).collect::<Vec<_>>();
    let partition_exact_cover_within_e = partition_non_empty
        && partition_pairwise_disjoint
        && row5_candidates == expected_candidates;
    let interpretation_blocked = control.interpretation_blocked
        || cases.iter().any(|case| case.interpretation_blocked)
        || !control_parent_evidence_compatible
        || !child_control_evidence_compatible
        || !partition_exact_cover_within_e
        || !sibling_s_unresolved
        || invalid_witness_count > 0;
    Ok(MaterialRow5SeparatorReport {
        schema_version: MATERIAL_ROW5_SEPARATOR_SCHEMA_VERSION,
        target_phase_index,
        selected_junction_case_index: 0,
        selected_network_id: parent.selected_network_id.clone(),
        selected_network_index: parent.selected_network_index,
        selected_item: parent.selected_item.clone(),
        selected_item_code: parent.selected_item_code,
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_flow_units: parent.source_flow_units,
        demand_flow_units: parent.demand_flow_units,
        source_cell: parent.source_cell,
        source_continuation_cell: 81,
        demand_cell: parent.demand_cell,
        fixed_dimensions: [dimensions.width, dimensions.height],
        separator_after_row: 5,
        candidates: row5_candidates,
        partition_non_empty,
        partition_pairwise_disjoint,
        partition_exact_cover_within_e,
        sibling_s_unresolved,
        demand_continuation_unrestricted: true,
        worker_count,
        authoritative_case_search_budget_ms: millis(authoritative_case_search_budget),
        observation_case_search_budget_ms: millis(observation_case_search_budget),
        execution_order: vec![
            "control-authoritative".to_string(),
            "children-authoritative-wave".to_string(),
            "control-observation".to_string(),
            "children-observation-wave".to_string(),
        ],
        control,
        cases,
        control_parent_evidence_compatible,
        child_control_evidence_compatible,
        validated_feasible_count,
        proven_infeasible_count,
        unknown_count,
        invalid_witness_count,
        witness_found: validated_feasible_count > 0,
        e_proven_infeasible: !interpretation_blocked && proven_infeasible_count == 16,
        interpretation_blocked,
        control_authoritative_wall_ms,
        control_observation_wall_ms,
        authoritative_wave_wall_ms,
        observation_wave_wall_ms,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        diagnostic_only: true,
        parent,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_case_report(
    case: &Row5CaseInput,
    authoritative: AuthoritativeResult,
    observation: ObservationResult,
    requested: &[FacilityPortAssignment],
    facility_count: usize,
    expected_external_terminal_count: usize,
    selected_terminal: &str,
    selected_boundary_key: i32,
    legal_boundary_keys: &[i32],
    continuation: &exact::shared_layer::EndpointContinuationRestriction,
    row4: &exact::shared_layer::MaterialSeparatorRestriction,
    row5: &exact::shared_layer::MaterialSeparatorRestriction,
    junction: &exact::shared_layer::MaterialJunctionRestriction,
    endpoint_parent: &EndpointContinuationPartitionReport,
    row4_candidates: &[[usize; 2]],
    row5_candidates: &[[usize; 2]],
    junction_candidates: &[[usize; 2]],
    fixed_dimensions: [i32; 2],
    expected_flow_upper: i32,
    expected_item_upper: i32,
    baseline: &ExternalBoundaryKeySolveReport,
    control: Option<&MaterialRow5SeparatorCaseReport>,
    expected_child_index: Option<usize>,
) -> Result<MaterialRow5SeparatorCaseReport, IntegratedLayoutReport> {
    let (
        authoritative_layout,
        authoritative_boundary,
        authoritative_continuation,
        authoritative_separator,
        authoritative_junction,
    ) = authoritative;
    let (
        observation_layout,
        root_snapshot,
        observation_boundary,
        observation_continuation,
        observation_separator,
        observation_junction,
    ) = observation;
    let root_snapshot = root_snapshot.ok_or_else(|| {
        invalid_input(
            "/cases/root_snapshot",
            "row-5 observation did not return a root snapshot",
        )
    })?;
    let fixation_observation = assess_fixation(&root_snapshot, requested);
    let solve = solve_report(
        &format!(
            "material-row5-separator-{}",
            case.case_index
                .map_or_else(|| "control".to_string(), |index| index.to_string())
        ),
        authoritative_layout,
        observation_layout,
        root_snapshot,
        fixation_observation,
    );
    let boundary_certificates_equal = authoritative_boundary == observation_boundary;
    let boundary_certificate_satisfied = boundary_certificates_satisfied(
        &authoritative_boundary,
        expected_external_terminal_count,
        selected_terminal,
        selected_boundary_key,
        legal_boundary_keys,
    ) && boundary_certificates_satisfied(
        &observation_boundary,
        expected_external_terminal_count,
        selected_terminal,
        selected_boundary_key,
        legal_boundary_keys,
    );
    let continuation_certificates_equal = authoritative_continuation == observation_continuation;
    let source_only_certificate_satisfied = authoritative_continuation.len() == 1
        && observation_continuation.len() == 1
        && continuation_certificate_matches(
            &authoritative_continuation[0],
            continuation,
            endpoint_parent.selected_network_index,
            &endpoint_parent.selected_item,
            endpoint_parent.source_flow_units,
            endpoint_parent.demand_flow_units,
        )
        && authoritative_continuation[0].demand_selected.is_none();
    let separator_certificates_equal = authoritative_separator == observation_separator;
    let ordered_separator_identity_satisfied = separator_stack_satisfied(
        &authoritative_separator,
        row4,
        row5,
        endpoint_parent,
        row4_candidates,
        row5_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    ) && separator_stack_satisfied(
        &observation_separator,
        row4,
        row5,
        endpoint_parent,
        row4_candidates,
        row5_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    );
    let junction_certificates_equal = authoritative_junction == observation_junction;
    let junction_e_certificate_satisfied = junction_certificate_satisfied(
        &authoritative_junction,
        junction,
        endpoint_parent,
        junction_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    ) && junction_certificate_satisfied(
        &observation_junction,
        junction,
        endpoint_parent,
        junction_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    );
    let inherited_boundary_matches_control = control.is_none_or(|control| {
        authoritative_boundary == control.authoritative_boundary_certificates
            && observation_boundary == control.observation_boundary_certificates
    });
    let inherited_continuation_matches_control = control.is_none_or(|control| {
        authoritative_continuation == control.authoritative_continuation_certificates
            && observation_continuation == control.observation_continuation_certificates
    });
    let inherited_row4_separator_matches_control = control.is_none_or(|control| {
        authoritative_separator.first() == control.authoritative_separator_certificates.first()
            && observation_separator.first() == control.observation_separator_certificates.first()
    });
    let inherited_junction_matches_control = control.is_none_or(|control| {
        authoritative_junction == control.authoritative_junction_certificates
            && observation_junction == control.observation_junction_certificates
    });
    let inherited_certificates_match_control = inherited_boundary_matches_control
        && inherited_continuation_matches_control
        && inherited_row4_separator_matches_control
        && inherited_junction_matches_control;
    let theorem_premises_satisfied = authoritative_separator.get(1).is_some_and(|certificate| {
        certificate.width == 16
            && certificate.height == 16
            && certificate.separator_after_row == 5
            && certificate.source_cell == 48
            && certificate.source_continuation_cell == 81
            && certificate.demand_cell == 113
            && certificate.source_flow_units > 0
            && certificate.source_flow_units == certificate.demand_flow_units
            && certificate.candidates.len() == 16
            && certificate
                .candidates
                .iter()
                .enumerate()
                .all(|(index, candidate)| {
                    candidate.case_index == index
                        && candidate.from == 80 + index
                        && candidate.to == 96 + index
                        && candidate.direction == "south"
                })
    });
    let root_restriction_observed =
        solve.root_snapshot.capture_status == "captured-before-first-decision";
    let root_restriction_satisfied = root_row5_audit_satisfied(
        &solve.root_snapshot,
        solve.observation_outcome,
        boundary_certificate_satisfied,
        source_only_certificate_satisfied,
        ordered_separator_identity_satisfied,
        junction_e_certificate_satisfied,
        expected_child_index,
    );
    let facility_fixation_observed = root_restriction_observed;
    let facility_fixation_satisfied = !facility_fixation_observed
        || (root_facility_fixation_satisfied(&solve.root_snapshot, facility_count)
            && (!solve.fixation_observation.assertion_applies
                || solve.fixation_observation.assertion_satisfied));
    let semantic_model_contract_satisfied =
        semantic_model_contract(&solve.authoritative_layout, &solve.observation_layout);
    let expected_constraint_delta = expected_child_index.map_or(0, |index| 2 + index as u64);
    let expected_incidence_delta = expected_child_index.map_or(0, |index| 2 + 2 * index as u64);
    let complete_family_delta_satisfied = family_delta_satisfied(
        &solve.authoritative_layout,
        &baseline.authoritative_layout,
        expected_constraint_delta,
        expected_incidence_delta,
    );
    let exact_model_identity = solve
        .authoritative_layout
        .exact
        .as_ref()
        .is_some_and(|exact| {
            baseline
                .authoritative_layout
                .exact
                .as_ref()
                .is_some_and(|baseline| exact.model == baseline.model)
        });
    let controlled_axis_model_satisfied = exact_model_identity
        && complete_family_delta_satisfied
        && solve.model_scale.variables == baseline.model_scale.variables
        && solve.model_scale.constraints
            == baseline.model_scale.constraints + expected_constraint_delta
        && solve.model_scale.incidences
            == baseline.model_scale.incidences + expected_incidence_delta
        && solve.model_scale.placement_routing_incidences
            == baseline.model_scale.placement_routing_incidences;
    let (hidden_domain_delta_observed, hidden_domain_delta_satisfied) =
        hidden_domain_delta_audit(&solve.root_snapshot, &baseline.root_snapshot);
    let interpretation_blocked = solve.evidence_conflict
        || solve.combined_outcome == ExactDimensionCaseOutcome::InvalidWitness
        || !boundary_certificates_equal
        || !boundary_certificate_satisfied
        || !continuation_certificates_equal
        || !source_only_certificate_satisfied
        || !separator_certificates_equal
        || !ordered_separator_identity_satisfied
        || !junction_certificates_equal
        || !junction_e_certificate_satisfied
        || !inherited_certificates_match_control
        || !theorem_premises_satisfied
        || !root_restriction_satisfied
        || !facility_fixation_satisfied
        || !semantic_model_contract_satisfied
        || !hidden_domain_delta_satisfied
        || !complete_family_delta_satisfied
        || !controlled_axis_model_satisfied;
    Ok(MaterialRow5SeparatorCaseReport {
        case_index: case.case_index,
        selected_arc: case.case_index.map(|index| row5_candidates[index]),
        preceding_arcs: case
            .case_index
            .map(|index| row5_candidates[..index].to_vec())
            .unwrap_or_default(),
        root_infeasible: solve.root_snapshot.capture_status == "root-infeasible",
        solve,
        authoritative_boundary_certificates: authoritative_boundary,
        observation_boundary_certificates: observation_boundary,
        authoritative_continuation_certificates: authoritative_continuation,
        observation_continuation_certificates: observation_continuation,
        authoritative_separator_certificates: authoritative_separator,
        observation_separator_certificates: observation_separator,
        authoritative_junction_certificates: authoritative_junction,
        observation_junction_certificates: observation_junction,
        boundary_certificates_equal,
        boundary_certificate_satisfied,
        continuation_certificates_equal,
        source_only_certificate_satisfied,
        separator_certificates_equal,
        ordered_separator_identity_satisfied,
        junction_certificates_equal,
        junction_e_certificate_satisfied,
        inherited_boundary_matches_control,
        inherited_continuation_matches_control,
        inherited_row4_separator_matches_control,
        inherited_junction_matches_control,
        inherited_certificates_match_control,
        theorem_premises_satisfied,
        root_restriction_observed,
        root_restriction_satisfied,
        facility_fixation_observed,
        facility_fixation_satisfied,
        semantic_model_contract_satisfied,
        hidden_domain_delta_observed,
        hidden_domain_delta_satisfied,
        complete_family_delta_satisfied,
        controlled_axis_model_satisfied,
        interpretation_blocked,
    })
}

#[allow(clippy::too_many_arguments)]
fn separator_stack_satisfied(
    certificates: &[exact::shared_layer::MaterialSeparatorBuildCertificate],
    row4: &exact::shared_layer::MaterialSeparatorRestriction,
    row5: &exact::shared_layer::MaterialSeparatorRestriction,
    endpoint_parent: &EndpointContinuationPartitionReport,
    row4_candidates: &[[usize; 2]],
    row5_candidates: &[[usize; 2]],
    fixed_dimensions: [i32; 2],
    expected_flow_upper: i32,
    expected_item_upper: i32,
) -> bool {
    let separator_rows = certificates
        .iter()
        .map(|certificate| certificate.separator_after_row)
        .collect::<Vec<_>>();
    if !ordered_separator_row_numbers_satisfied(&separator_rows) {
        return false;
    }
    let [row4_certificate, row5_certificate] = certificates else {
        return false;
    };
    super::super::separator_certificate_satisfied(
        std::slice::from_ref(row4_certificate),
        row4,
        endpoint_parent,
        row4_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    ) && super::super::separator_certificate_satisfied(
        std::slice::from_ref(row5_certificate),
        row5,
        endpoint_parent,
        row5_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    )
}

fn ordered_separator_row_numbers_satisfied(rows: &[usize]) -> bool {
    rows == [4, 5]
}

fn hidden_domain_delta_audit(
    root: &crate::layouts::RootDomainSnapshot,
    baseline: &crate::layouts::RootDomainSnapshot,
) -> (bool, bool) {
    let observed = root.capture_status == "captured-before-first-decision"
        && baseline.capture_status == "captured-before-first-decision";
    let satisfied = !observed || root.variable_coverage == baseline.variable_coverage;
    (observed, satisfied)
}

fn root_row5_audit_satisfied(
    root: &crate::layouts::RootDomainSnapshot,
    observation_outcome: ExactDimensionCaseOutcome,
    boundary_ok: bool,
    continuation_ok: bool,
    separators_ok: bool,
    junction_ok: bool,
    selected_case_index: Option<usize>,
) -> bool {
    if root.capture_status == "root-infeasible" {
        return observation_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
            && boundary_ok
            && continuation_ok
            && separators_ok
            && junction_ok;
    }
    let [row4, row5] = root.material_separators.as_slice() else {
        return false;
    };
    let Some(junction) = &root.material_junction else {
        return false;
    };
    let fixed = |candidate: &crate::layouts::RootMaterialSeparatorArcSnapshot| {
        candidate.route_selected.lower_bound == 1
            && candidate.flow.lower_bound >= 1
            && candidate.from_item.cardinality == 1
            && candidate.from_item.lower_bound == candidate.selected_item_code
    };
    row4.separator_after_row == 4
        && row4.selected_case_index == Some(0)
        && row4.candidates.first().is_some_and(fixed)
        && row5.separator_after_row == 5
        && row5.selected_case_index == selected_case_index
        && selected_case_index
            .is_none_or(|selected| row5.candidates.get(selected).is_some_and(fixed))
        && junction.selected_case_index == Some(0)
        && junction.candidates.first().is_some_and(|candidate| {
            candidate.route_selected.lower_bound == 1
                && candidate.flow.lower_bound >= 1
                && candidate.from_item.cardinality == 1
                && candidate.from_item.lower_bound == candidate.selected_item_code
        })
}

fn family_delta_satisfied(
    layout: &IntegratedLayoutReport,
    baseline: &IntegratedLayoutReport,
    expected_constraints: u64,
    expected_terms: u64,
) -> bool {
    let Some(layout_constraints) = layout
        .exact
        .as_ref()
        .and_then(|exact| exact.model_complexity.constraints.as_ref())
    else {
        return false;
    };
    let Some(baseline_constraints) = baseline
        .exact
        .as_ref()
        .and_then(|exact| exact.model_complexity.constraints.as_ref())
    else {
        return false;
    };
    let separator_totals = |families: &[crate::research::ConstraintFamilyMetrics]| {
        families
            .iter()
            .filter(|family| family.family == "material-separator")
            .fold((0_u64, 0_u64), |(constraints, terms), family| {
                (constraints + family.constraints, terms + family.terms)
            })
    };
    let (layout_separator_constraints, layout_separator_terms) =
        separator_totals(&layout_constraints.by_family);
    let (baseline_separator_constraints, baseline_separator_terms) =
        separator_totals(&baseline_constraints.by_family);
    constraint_families_match_except_material_separator(
        &layout_constraints.by_family,
        &baseline_constraints.by_family,
    ) && layout_separator_constraints == baseline_separator_constraints + expected_constraints
        && layout_separator_terms == baseline_separator_terms + expected_terms
        && layout_constraints.total_constraints
            == baseline_constraints.total_constraints + expected_constraints
        && layout_constraints.total_terms == baseline_constraints.total_terms + expected_terms
}

fn constraint_families_match_except_material_separator(
    layout: &[crate::research::ConstraintFamilyMetrics],
    baseline: &[crate::research::ConstraintFamilyMetrics],
) -> bool {
    let without_separator = |families: &[crate::research::ConstraintFamilyMetrics]| {
        families
            .iter()
            .filter(|family| family.family != "material-separator")
            .cloned()
            .collect::<Vec<_>>()
    };
    without_separator(layout) == without_separator(baseline)
}

fn row5_child_evidence_compatible(
    control: ExactDimensionCaseOutcome,
    cases: &[MaterialRow5SeparatorCaseReport],
) -> bool {
    let child_witness = cases
        .iter()
        .any(|case| case.solve.combined_outcome == ExactDimensionCaseOutcome::ValidatedFeasible);
    let all_infeasible = !cases.is_empty()
        && cases
            .iter()
            .all(|case| case.solve.combined_outcome == ExactDimensionCaseOutcome::ProvenInfeasible);
    !(child_witness && control == ExactDimensionCaseOutcome::ProvenInfeasible
        || all_infeasible && control == ExactDimensionCaseOutcome::ValidatedFeasible)
}

#[cfg(test)]
mod tests {
    use crate::research::{ConstraintFamilyMetrics, ConstraintRelation};

    use super::*;

    #[test]
    fn canonical_row5_children_accept_exactly_the_first_true_crossing_for_every_subset() {
        for subset in 1_u32..(1_u32 << 16) {
            let predicates = (0..16)
                .map(|index| subset & (1 << index) != 0)
                .collect::<Vec<_>>();
            let first = predicates
                .iter()
                .position(|selected| *selected)
                .expect("non-empty subset has a first crossing");
            for child in 0..16 {
                let accepted = predicates[child] && predicates[..child].iter().all(|value| !value);
                assert_eq!(accepted, child == first);
            }
        }
    }

    #[test]
    fn ordered_separator_rows_reject_reordered_missing_and_duplicated_stacks() {
        assert!(ordered_separator_row_numbers_satisfied(&[4, 5]));
        assert!(!ordered_separator_row_numbers_satisfied(&[5, 4]));
        assert!(!ordered_separator_row_numbers_satisfied(&[4]));
        assert!(!ordered_separator_row_numbers_satisfied(&[4, 4]));
        assert!(!ordered_separator_row_numbers_satisfied(&[4, 5, 5]));
    }

    #[test]
    fn family_audit_rejects_non_separator_drift() {
        let metric = |family: &str, constraints: u64| ConstraintFamilyMetrics {
            family: family.to_string(),
            relation: ConstraintRelation::Other,
            constraints,
            terms: constraints,
            maximum_arity: 1,
            p95_arity: 1,
            maximum_absolute_coefficient: 1,
        };
        let baseline = vec![metric("placement", 3), metric("material-separator", 1)];
        let separator_only_change = vec![metric("placement", 3), metric("material-separator", 9)];
        let non_separator_drift = vec![metric("placement", 4), metric("material-separator", 9)];
        assert!(constraint_families_match_except_material_separator(
            &separator_only_change,
            &baseline,
        ));
        assert!(!constraint_families_match_except_material_separator(
            &non_separator_drift,
            &baseline,
        ));
    }

    #[test]
    fn root_infeasible_requires_proof_and_static_certificates_without_domain_comparison() {
        let root = crate::layouts::RootDomainSnapshot::root_infeasible_without_brancher_call();
        let baseline = crate::layouts::RootDomainSnapshot::root_infeasible_without_brancher_call();
        assert_eq!(hidden_domain_delta_audit(&root, &baseline), (false, true));
        assert!(root_row5_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            true,
            true,
            true,
            true,
            Some(0),
        ));
        assert!(!root_row5_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::Unknown,
            true,
            true,
            true,
            true,
            Some(0),
        ));
        assert!(!root_row5_audit_satisfied(
            &root,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            true,
            true,
            false,
            true,
            Some(0),
        ));
    }
}
