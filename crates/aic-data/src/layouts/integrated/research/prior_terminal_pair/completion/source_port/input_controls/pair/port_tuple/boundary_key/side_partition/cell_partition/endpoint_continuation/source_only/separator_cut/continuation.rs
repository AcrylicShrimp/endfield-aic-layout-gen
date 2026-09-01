use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;

mod row5;

pub use row5::{
    GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION, GuardedCoreAcceptedFixture,
    GuardedCoreInitialGateReport, GuardedCoreInitialGateStatus, GuardedCoreSequentialShrinkReport,
    GuardedCoreSequentialShrinkStatus, GuardedCoreShrinkAttempt,
    MATERIAL_ROW5_SEPARATOR_SCHEMA_VERSION, MaterialRow5SeparatorCaseReport,
    MaterialRow5SeparatorReport, diagnose_guarded_core_initial_gate,
    diagnose_guarded_core_sequential_shrinking, diagnose_material_row5_separator,
};

pub const MATERIAL_JUNCTION_CONTINUATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MaterialJunctionCaseReport {
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
    pub separator_certificate_satisfied: bool,
    pub junction_certificates_equal: bool,
    pub junction_certificate_satisfied: bool,
    pub theorem_premises_satisfied: bool,
    pub root_restriction_satisfied: bool,
    pub facility_fixation_satisfied: bool,
    pub semantic_model_contract_satisfied: bool,
    pub hidden_domain_delta_observed: bool,
    pub hidden_domain_delta_satisfied: bool,
    pub material_junction_family_satisfied: bool,
    pub controlled_axis_model_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MaterialJunctionContinuationReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: MaterialSeparatorCutReport,
    pub selected_separator_case_index: usize,
    pub inherited_incoming_arc: [usize; 2],
    pub selected_network_id: String,
    pub selected_network_index: usize,
    pub selected_item: String,
    pub selected_item_code: i32,
    pub source_terminal: String,
    pub demand_terminal: String,
    pub source_flow_units: i32,
    pub demand_flow_units: i32,
    pub source_cell: usize,
    pub demand_cell: usize,
    pub fixed_dimensions: [i32; 2],
    pub junction_cell: usize,
    pub candidates: Vec<[usize; 2]>,
    pub partition_non_empty: bool,
    pub partition_pairwise_disjoint: bool,
    pub partition_exact_cover: bool,
    pub demand_continuation_unrestricted: bool,
    pub worker_count: usize,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub execution_order: Vec<String>,
    pub control: MaterialJunctionCaseReport,
    pub cases: Vec<MaterialJunctionCaseReport>,
    pub control_parent_evidence_compatible: bool,
    pub child_control_evidence_compatible: bool,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub witness_found: bool,
    pub all_children_proven_infeasible: bool,
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
struct JunctionCaseInput {
    case_index: Option<usize>,
}

type AuthoritativeResult = (
    IntegratedLayoutReport,
    Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    Vec<exact::shared_layer::EndpointContinuationBuildCertificate>,
    Vec<exact::shared_layer::MaterialSeparatorBuildCertificate>,
    Vec<exact::shared_layer::MaterialJunctionBuildCertificate>,
);

type ObservationResult = (
    IntegratedLayoutReport,
    Option<crate::layouts::RootDomainSnapshot>,
    Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    Vec<exact::shared_layer::EndpointContinuationBuildCertificate>,
    Vec<exact::shared_layer::MaterialSeparatorBuildCertificate>,
    Vec<exact::shared_layer::MaterialJunctionBuildCertificate>,
);

#[allow(clippy::too_many_arguments)]
pub fn diagnose_material_junction_continuation(
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
    separator_after_row: usize,
    separator_authoritative_search_budget: Duration,
    separator_observation_search_budget: Duration,
    authoritative_case_search_budget: Duration,
    observation_case_search_budget: Duration,
) -> Result<MaterialJunctionContinuationReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if worker_count == 0
        || authoritative_case_search_budget.is_zero()
        || observation_case_search_budget.is_zero()
    {
        return Err(invalid_input(
            "/material_junction_continuation",
            "worker count and junction budgets must be positive",
        ));
    }
    let parent = diagnose_material_separator_cut(
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
        separator_after_row,
        separator_authoritative_search_budget,
        separator_observation_search_budget,
    )?;
    if parent.interpretation_blocked
        || !parent.partition_exact_cover
        || !parent.demand_continuation_unrestricted
        || parent.cases.len() != 16
        || parent.proven_infeasible_count != 15
        || parent.unknown_count != 1
    {
        return Err(invalid_input(
            "/parent",
            "material-junction continuation requires the unblocked 15+1 row-4 separator result",
        ));
    }
    let selected_separator_case = &parent.cases[0];
    if selected_separator_case.case_index != Some(0)
        || selected_separator_case.selected_arc != Some([64, 80])
        || selected_separator_case.solve.combined_outcome != ExactDimensionCaseOutcome::Unknown
        || parent.fixed_dimensions != [16, 16]
        || parent.target_phase_index != 3
        || parent.selected_network_id != "network:pipe:item-liquid-xiranite-poly"
        || parent.selected_item != "item-liquid-xiranite-poly"
        || parent.source_cell != 48
        || parent.demand_cell != 113
        || parent.separator_after_row != 4
    {
        return Err(invalid_input(
            "/parent/accepted_fixture",
            "material-junction continuation requires the accepted Phase 3 row-4 case-0 fixture",
        ));
    }

    let source_parent = &parent.parent;
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
            "accepted junction fixture must retain four facilities and every fixed terminal",
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
            "junction dimensions differ from the reconstructed parent",
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
    let separator = exact::shared_layer::MaterialSeparatorRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_cell: parent.source_cell,
        source_continuation_cell: 64,
        demand_cell: parent.demand_cell,
        separator_after_row: parent.separator_after_row,
        selected_case_index: Some(0),
    };
    let candidates = vec![[80, 81], [80, 96]];
    let junction_base = exact::shared_layer::MaterialJunctionRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_cell: parent.source_cell,
        demand_cell: parent.demand_cell,
        incoming: exact::shared_layer::DirectedGridArcRestriction { from: 64, to: 80 },
        junction_cell: 80,
        candidates: candidates
            .iter()
            .map(|arc| exact::shared_layer::DirectedGridArcRestriction {
                from: arc[0],
                to: arc[1],
            })
            .collect(),
        selected_case_index: None,
    };
    let experiment_started = Instant::now();

    let solve_case = |junction: exact::shared_layer::MaterialJunctionRestriction,
                      budget: Duration,
                      observation: bool| {
        if observation {
            EitherSolve::Observation(solve_observation(
                input.clone(),
                logistics_components,
                budget,
                dimensions,
                coordinate.clone(),
                fixed_ports.clone(),
                prior_reference,
                fixation,
                selected_terminal.clone(),
                allowed_keys.clone(),
                continuation.clone(),
                vec![separator.clone()],
                junction,
            ))
        } else {
            EitherSolve::Authoritative(solve_authoritative(
                input.clone(),
                logistics_components,
                budget,
                dimensions,
                coordinate.clone(),
                fixed_ports.clone(),
                prior_reference,
                fixation,
                selected_terminal.clone(),
                allowed_keys.clone(),
                continuation.clone(),
                vec![separator.clone()],
                junction,
            ))
        }
    };

    let control_input = JunctionCaseInput { case_index: None };
    let control_authoritative_started = Instant::now();
    let EitherSolve::Authoritative(control_authoritative) = solve_case(
        junction_base.clone(),
        authoritative_case_search_budget,
        false,
    ) else {
        unreachable!()
    };
    let control_authoritative_wall_ms = millis(control_authoritative_started.elapsed());
    let control_observation_started = Instant::now();
    let EitherSolve::Observation(control_observation) =
        solve_case(junction_base.clone(), observation_case_search_budget, true)
    else {
        unreachable!()
    };
    let control_observation_wall_ms = millis(control_observation_started.elapsed());

    let child_inputs = (0..candidates.len())
        .map(|case_index| JunctionCaseInput {
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
                    let mut junction = junction_base.clone();
                    junction.selected_case_index = case.case_index;
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let keys = allowed_keys.clone();
                    let continuation = continuation.clone();
                    let separators = vec![separator.clone()];
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
                authoritative.push((case, handle.join().expect("junction worker panicked")));
            }
        });
    }
    authoritative.sort_by_key(|(case, _)| case.case_index);
    let authoritative_wave_wall_ms = millis(authoritative_started.elapsed());

    let observation_started = Instant::now();
    let mut observations = Vec::with_capacity(child_inputs.len());
    for chunk in child_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .cloned()
                .map(|case| {
                    let mut junction = junction_base.clone();
                    junction.selected_case_index = case.case_index;
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let keys = allowed_keys.clone();
                    let continuation = continuation.clone();
                    let separators = vec![separator.clone()];
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
                observations.push((case, handle.join().expect("junction observer panicked")));
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
        &separator,
        &junction_base,
        endpoint_parent,
        &parent.candidates,
        &candidates,
        [dimensions.width, dimensions.height],
        expected_flow_upper,
        expected_item_upper,
        &selected_separator_case.solve,
        None,
    )?;
    let mut cases = Vec::with_capacity(child_inputs.len());
    for ((case, authoritative_result), (observed_case, observation_result)) in
        authoritative.into_iter().zip(observations)
    {
        if case.case_index != observed_case.case_index {
            return Err(invalid_input(
                "/cases",
                "authoritative and observation junction cases differ",
            ));
        }
        let selected = case.case_index.expect("child has selected case");
        let mut restriction = junction_base.clone();
        restriction.selected_case_index = Some(selected);
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
            &separator,
            &restriction,
            endpoint_parent,
            &parent.candidates,
            &candidates,
            [dimensions.width, dimensions.height],
            expected_flow_upper,
            expected_item_upper,
            &control.solve,
            Some(selected),
        )?);
    }

    let control_parent_evidence_compatible = outcomes_compatible(
        selected_separator_case.solve.combined_outcome,
        control.solve.combined_outcome,
    );
    let child_control_evidence_compatible =
        child_evidence_compatible(control.solve.combined_outcome, &cases);
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
    let partition_non_empty = !candidates.is_empty();
    let partition_pairwise_disjoint = candidates.len() == 2 && candidates[0] != candidates[1];
    let partition_exact_cover =
        partition_non_empty && partition_pairwise_disjoint && candidates == [[80, 81], [80, 96]];
    let interpretation_blocked = control.interpretation_blocked
        || cases.iter().any(|case| case.interpretation_blocked)
        || !control_parent_evidence_compatible
        || !child_control_evidence_compatible
        || !partition_exact_cover
        || invalid_witness_count > 0;
    Ok(MaterialJunctionContinuationReport {
        schema_version: MATERIAL_JUNCTION_CONTINUATION_SCHEMA_VERSION,
        target_phase_index,
        selected_separator_case_index: 0,
        inherited_incoming_arc: [64, 80],
        selected_network_id: parent.selected_network_id.clone(),
        selected_network_index: parent.selected_network_index,
        selected_item: parent.selected_item.clone(),
        selected_item_code: parent.selected_item_code,
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_flow_units: parent.source_flow_units,
        demand_flow_units: parent.demand_flow_units,
        source_cell: parent.source_cell,
        demand_cell: parent.demand_cell,
        fixed_dimensions: [dimensions.width, dimensions.height],
        junction_cell: 80,
        candidates,
        partition_non_empty,
        partition_pairwise_disjoint,
        partition_exact_cover,
        demand_continuation_unrestricted: true,
        worker_count,
        authoritative_case_search_budget_ms: millis(authoritative_case_search_budget),
        observation_case_search_budget_ms: millis(observation_case_search_budget),
        execution_order: vec![
            "control-authoritative".to_string(),
            "control-observation".to_string(),
            "children-authoritative-wave".to_string(),
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
        all_children_proven_infeasible: proven_infeasible_count == 2,
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

enum EitherSolve {
    Authoritative(AuthoritativeResult),
    Observation(ObservationResult),
}

#[allow(clippy::too_many_arguments)]
fn solve_authoritative(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    budget: Duration,
    dimensions: exact::shared_layer::FixedUsedDimensions,
    coordinate: exact::shared_layer::FixedFacilityCoordinate,
    fixed_ports: Vec<exact::shared_layer::FixedTerminalPortChoice>,
    prior_reference: &IntegratedLayoutReport,
    fixation: exact::shared_layer::ReferenceAblationFixation,
    terminal: String,
    allowed_keys: Vec<i32>,
    continuation: exact::shared_layer::EndpointContinuationRestriction,
    separators: Vec<exact::shared_layer::MaterialSeparatorRestriction>,
    junction: exact::shared_layer::MaterialJunctionRestriction,
) -> AuthoritativeResult {
    exact::shared_layer::solve_sparse_support_endpoints_boundary_key_continuation_material_separator_and_junction_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
        input,
        logistics_components,
        Some(budget),
        dimensions,
        coordinate,
        fixed_ports,
        prior_reference,
        fixation,
        terminal,
        allowed_keys,
        continuation,
        separators,
        junction,
    )
}

#[allow(clippy::too_many_arguments)]
fn solve_observation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    budget: Duration,
    dimensions: exact::shared_layer::FixedUsedDimensions,
    coordinate: exact::shared_layer::FixedFacilityCoordinate,
    fixed_ports: Vec<exact::shared_layer::FixedTerminalPortChoice>,
    prior_reference: &IntegratedLayoutReport,
    fixation: exact::shared_layer::ReferenceAblationFixation,
    terminal: String,
    allowed_keys: Vec<i32>,
    continuation: exact::shared_layer::EndpointContinuationRestriction,
    separators: Vec<exact::shared_layer::MaterialSeparatorRestriction>,
    junction: exact::shared_layer::MaterialJunctionRestriction,
) -> ObservationResult {
    exact::shared_layer::solve_sparse_support_endpoints_boundary_key_continuation_material_separator_and_junction_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
        input,
        logistics_components,
        Some(budget),
        dimensions,
        coordinate,
        fixed_ports,
        prior_reference,
        fixation,
        terminal,
        allowed_keys,
        continuation,
        separators,
        junction,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_case_report(
    case: &JunctionCaseInput,
    authoritative: AuthoritativeResult,
    observation: ObservationResult,
    requested: &[FacilityPortAssignment],
    facility_count: usize,
    expected_external_terminal_count: usize,
    selected_terminal: &str,
    selected_boundary_key: i32,
    legal_boundary_keys: &[i32],
    continuation: &exact::shared_layer::EndpointContinuationRestriction,
    separator: &exact::shared_layer::MaterialSeparatorRestriction,
    junction: &exact::shared_layer::MaterialJunctionRestriction,
    endpoint_parent: &EndpointContinuationPartitionReport,
    separator_candidates: &[[usize; 2]],
    junction_candidates: &[[usize; 2]],
    fixed_dimensions: [i32; 2],
    expected_flow_upper: i32,
    expected_item_upper: i32,
    baseline: &ExternalBoundaryKeySolveReport,
    expected_child_index: Option<usize>,
) -> Result<MaterialJunctionCaseReport, IntegratedLayoutReport> {
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
            "material-junction observation did not return a root snapshot",
        )
    })?;
    let fixation_observation = assess_fixation(&root_snapshot, requested);
    let solve = solve_report(
        &format!(
            "material-junction-{}",
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
    let separator_certificate_satisfied = super::separator_certificate_satisfied(
        &authoritative_separator,
        separator,
        endpoint_parent,
        separator_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    ) && super::separator_certificate_satisfied(
        &observation_separator,
        separator,
        endpoint_parent,
        separator_candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    );
    let junction_certificates_equal = authoritative_junction == observation_junction;
    let junction_certificate_satisfied = junction_certificate_satisfied(
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
    let theorem_premises_satisfied = authoritative_junction.first().is_some_and(|certificate| {
        certificate.network_terminal_count == 2
            && certificate.junction_cell == 80
            && certificate.junction_is_west_boundary
            && certificate.junction_is_not_selected_terminal
            && certificate.actual_outgoing_cells == [64, 81, 96]
            && certificate.incoming.from == 64
            && certificate.incoming.to == 80
            && certificate.incoming.direction == "south"
            && certificate
                .candidates
                .iter()
                .map(|arc| arc.direction.as_str())
                .collect::<Vec<_>>()
                == ["east", "south"]
    });
    let root_restriction_satisfied = root_audit_satisfied(
        &solve.root_snapshot,
        solve.observation_outcome,
        boundary_certificate_satisfied,
        source_only_certificate_satisfied,
        separator_certificate_satisfied,
        junction_certificate_satisfied,
        expected_child_index,
    );
    let facility_fixation_satisfied =
        root_facility_fixation_satisfied(&solve.root_snapshot, facility_count)
            && (!solve.fixation_observation.assertion_applies
                || solve.fixation_observation.assertion_satisfied);
    let semantic_model_contract_satisfied =
        semantic_model_contract(&solve.authoritative_layout, &solve.observation_layout);
    let expected_constraint_delta = expected_child_index.map_or(0, |index| 2 + index as u64);
    let expected_incidence_delta = expected_child_index.map_or(0, |index| 2 + 2 * index as u64);
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
    let control_complexity_identity = expected_child_index.is_some()
        || solve
            .authoritative_layout
            .exact
            .as_ref()
            .is_some_and(|exact| {
                baseline
                    .authoritative_layout
                    .exact
                    .as_ref()
                    .is_some_and(|baseline| exact.model_complexity == baseline.model_complexity)
            });
    let controlled_axis_model_satisfied = exact_model_identity
        && control_complexity_identity
        && solve.model_scale.variables == baseline.model_scale.variables
        && solve.model_scale.constraints
            == baseline.model_scale.constraints + expected_constraint_delta
        && solve.model_scale.incidences
            == baseline.model_scale.incidences + expected_incidence_delta
        && solve.model_scale.placement_routing_incidences
            == baseline.model_scale.placement_routing_incidences;
    let hidden_domain_delta_observed = solve.root_snapshot.capture_status
        == "captured-before-first-decision"
        && baseline.root_snapshot.capture_status == "captured-before-first-decision";
    let hidden_domain_delta_satisfied = !hidden_domain_delta_observed
        || solve.root_snapshot.variable_coverage == baseline.root_snapshot.variable_coverage;
    let material_junction_family_satisfied = junction_family_satisfied(
        &solve.authoritative_layout,
        &baseline.authoritative_layout,
        expected_constraint_delta,
        expected_incidence_delta,
    );
    let interpretation_blocked = solve.evidence_conflict
        || solve.combined_outcome == ExactDimensionCaseOutcome::InvalidWitness
        || !boundary_certificates_equal
        || !boundary_certificate_satisfied
        || !continuation_certificates_equal
        || !source_only_certificate_satisfied
        || !separator_certificates_equal
        || !separator_certificate_satisfied
        || !junction_certificates_equal
        || !junction_certificate_satisfied
        || !theorem_premises_satisfied
        || !root_restriction_satisfied
        || !facility_fixation_satisfied
        || !semantic_model_contract_satisfied
        || !hidden_domain_delta_satisfied
        || !material_junction_family_satisfied
        || !controlled_axis_model_satisfied;
    Ok(MaterialJunctionCaseReport {
        case_index: case.case_index,
        selected_arc: case.case_index.map(|index| junction_candidates[index]),
        preceding_arcs: case
            .case_index
            .map(|index| junction_candidates[..index].to_vec())
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
        separator_certificate_satisfied,
        junction_certificates_equal,
        junction_certificate_satisfied,
        theorem_premises_satisfied,
        root_restriction_satisfied,
        facility_fixation_satisfied,
        semantic_model_contract_satisfied,
        hidden_domain_delta_observed,
        hidden_domain_delta_satisfied,
        material_junction_family_satisfied,
        controlled_axis_model_satisfied,
        interpretation_blocked,
    })
}

fn junction_certificate_satisfied(
    certificates: &[exact::shared_layer::MaterialJunctionBuildCertificate],
    restriction: &exact::shared_layer::MaterialJunctionRestriction,
    endpoint_parent: &EndpointContinuationPartitionReport,
    candidates: &[[usize; 2]],
    fixed_dimensions: [i32; 2],
    expected_flow_upper: i32,
    expected_item_upper: i32,
) -> bool {
    let [certificate] = certificates else {
        return false;
    };
    let expected_preceding = restriction
        .selected_case_index
        .map(|index| (0..index).collect::<Vec<_>>())
        .unwrap_or_default();
    certificate.network_id == restriction.network_id
        && certificate.network_index == endpoint_parent.selected_network_index
        && certificate.transport == TransportKind::Pipe
        && certificate.item == endpoint_parent.selected_item
        && certificate.source_terminal == restriction.source_terminal
        && certificate.source_flow_units == endpoint_parent.source_flow_units
        && certificate.source_cell == restriction.source_cell
        && certificate.demand_terminal == restriction.demand_terminal
        && certificate.demand_flow_units == endpoint_parent.demand_flow_units
        && certificate.demand_cell == restriction.demand_cell
        && certificate.network_terminal_count == 2
        && [certificate.width, certificate.height] == fixed_dimensions
        && certificate.junction_cell == restriction.junction_cell
        && certificate.selected_case_index == restriction.selected_case_index
        && certificate.preceding_case_indices == expected_preceding
        && certificate.posted_selected_unary_constraints
            == usize::from(restriction.selected_case_index.is_some()) * 2
        && certificate.posted_exclusion_clauses
            == restriction.selected_case_index.unwrap_or_default()
        && certificate.incoming.case_index.is_none()
        && certificate.incoming.from == restriction.incoming.from
        && certificate.incoming.to == restriction.incoming.to
        && certificate.candidates.len() == candidates.len()
        && certificate
            .candidates
            .iter()
            .zip(candidates)
            .enumerate()
            .all(|(index, (candidate, expected))| {
                candidate.case_index == Some(index)
                    && [candidate.from, candidate.to] == *expected
                    && candidate.route_selected_family == "route-arc"
                    && candidate.route_selected_declared_lower_bound == 0
                    && candidate.route_selected_declared_upper_bound == 1
                    && candidate.route_selected_declared_cardinality == 2
                    && candidate.flow_family == "flow"
                    && candidate.flow_declared_lower_bound == 0
                    && candidate.flow_declared_upper_bound == expected_flow_upper
                    && candidate.from_item_family == "arm-item"
                    && candidate.from_item_declared_lower_bound == 0
                    && candidate.from_item_declared_upper_bound == expected_item_upper
                    && candidate.from_item_declared_cardinality
                        == u64::try_from(expected_item_upper + 1).unwrap()
                    && candidate.selected_item_code == certificate.selected_item_code
                    && !candidate.route_selected_name.is_empty()
                    && !candidate.flow_name.is_empty()
                    && !candidate.from_item_name.is_empty()
            })
}

fn root_audit_satisfied(
    root: &crate::layouts::RootDomainSnapshot,
    observation_outcome: ExactDimensionCaseOutcome,
    boundary_ok: bool,
    continuation_ok: bool,
    separator_ok: bool,
    junction_ok: bool,
    selected_case_index: Option<usize>,
) -> bool {
    if root.capture_status == "root-infeasible" {
        return observation_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
            && boundary_ok
            && continuation_ok
            && separator_ok
            && junction_ok;
    }
    let [separator] = root.material_separators.as_slice() else {
        return false;
    };
    let Some(junction) = &root.material_junction else {
        return false;
    };
    let incoming_ok = junction.incoming.route_selected.lower_bound == 1
        && junction.incoming.flow.lower_bound >= 1
        && junction.incoming.from_item.cardinality == 1
        && junction.incoming.from_item.lower_bound == junction.selected_item_code;
    let inherited_separator_ok = separator.selected_case_index == Some(0)
        && separator.candidates.first().is_some_and(|candidate| {
            candidate.route_selected.lower_bound == 1
                && candidate.flow.lower_bound >= 1
                && candidate.from_item.cardinality == 1
                && candidate.from_item.lower_bound == candidate.selected_item_code
        });
    incoming_ok
        && inherited_separator_ok
        && junction.selected_case_index == selected_case_index
        && selected_case_index.is_none_or(|selected| {
            junction.candidates.get(selected).is_some_and(|candidate| {
                candidate.route_selected.lower_bound == 1
                    && candidate.flow.lower_bound >= 1
                    && candidate.from_item.cardinality == 1
                    && candidate.from_item.lower_bound == candidate.selected_item_code
            })
        })
}

fn junction_family_satisfied(
    layout: &IntegratedLayoutReport,
    baseline: &IntegratedLayoutReport,
    expected_constraints: u64,
    expected_terms: u64,
) -> bool {
    let totals = |report: &IntegratedLayoutReport| {
        report
            .exact
            .as_ref()?
            .model_complexity
            .constraints
            .as_ref()
            .map(|constraints| {
                constraints
                    .by_family
                    .iter()
                    .filter(|family| family.family == "material-junction")
                    .fold((0_u64, 0_u64), |(constraints, terms), family| {
                        (constraints + family.constraints, terms + family.terms)
                    })
            })
    };
    totals(baseline) == Some((0, 0))
        && totals(layout) == Some((expected_constraints, expected_terms))
}

fn child_evidence_compatible(
    control: ExactDimensionCaseOutcome,
    cases: &[MaterialJunctionCaseReport],
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
    #[test]
    fn canonical_junction_cases_cover_east_or_south() {
        let states = [(false, false), (true, false), (false, true), (true, true)];
        let accepted = states.map(|(east, south)| {
            let east_child = east;
            let south_child = !east && south;
            (east_child as usize) + (south_child as usize)
        });
        assert_eq!(accepted, [0, 1, 1, 1]);
    }
}
