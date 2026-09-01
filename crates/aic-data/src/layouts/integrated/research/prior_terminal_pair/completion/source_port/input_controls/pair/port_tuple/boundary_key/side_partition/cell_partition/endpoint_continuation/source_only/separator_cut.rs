use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;
use crate::logistics::TransportKind;

mod continuation;

pub use continuation::{
    GUARDED_CORE_BOUNDARY_CENSUS_SCHEMA_VERSION, GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION,
    GuardedCoreAcceptedFixture, GuardedCoreBoundaryCensusCase, GuardedCoreBoundaryCensusReport,
    GuardedCoreBoundaryCensusRootStatus, GuardedCoreBoundaryCensusStatus,
    GuardedCoreInitialGateReport, GuardedCoreInitialGateStatus, GuardedCoreReplayReport,
    GuardedCoreReplayStatus, GuardedCoreSequentialShrinkReport, GuardedCoreSequentialShrinkStatus,
    GuardedCoreShrinkAttempt, MATERIAL_JUNCTION_CONTINUATION_SCHEMA_VERSION,
    MATERIAL_ROW5_SEPARATOR_SCHEMA_VERSION, MaterialJunctionCaseReport,
    MaterialJunctionContinuationReport, MaterialRow5SeparatorCaseReport,
    MaterialRow5SeparatorReport, diagnose_guarded_core_boundary_census,
    diagnose_guarded_core_initial_gate, diagnose_guarded_core_replay,
    diagnose_guarded_core_sequential_shrinking, diagnose_material_junction_continuation,
    diagnose_material_row5_separator,
};

pub const MATERIAL_SEPARATOR_CUT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MaterialSeparatorCaseReport {
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
    pub boundary_certificates_equal: bool,
    pub boundary_certificate_satisfied: bool,
    pub continuation_certificates_equal: bool,
    pub source_only_certificate_satisfied: bool,
    pub separator_certificates_equal: bool,
    pub separator_certificate_satisfied: bool,
    pub root_separator_restriction_satisfied: bool,
    pub facility_fixation_satisfied: bool,
    pub semantic_model_contract_satisfied: bool,
    pub hidden_domain_delta_observed: bool,
    pub hidden_domain_delta_satisfied: bool,
    pub material_separator_family_satisfied: bool,
    pub controlled_axis_model_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MaterialSeparatorCutReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: EndpointSourceOnlyControlReport,
    pub selected_source_case_index: usize,
    pub selected_source_arc: [usize; 2],
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
    pub partition_exact_cover: bool,
    pub demand_continuation_unrestricted: bool,
    pub worker_count: usize,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub execution_order: Vec<String>,
    pub control: MaterialSeparatorCaseReport,
    pub cases: Vec<MaterialSeparatorCaseReport>,
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
struct SeparatorCaseInput {
    case_index: Option<usize>,
}

type AuthoritativeResult = (
    IntegratedLayoutReport,
    Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    Vec<exact::shared_layer::EndpointContinuationBuildCertificate>,
    Vec<exact::shared_layer::MaterialSeparatorBuildCertificate>,
);

type ObservationResult = (
    IntegratedLayoutReport,
    Option<crate::layouts::RootDomainSnapshot>,
    Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    Vec<exact::shared_layer::EndpointContinuationBuildCertificate>,
    Vec<exact::shared_layer::MaterialSeparatorBuildCertificate>,
);

#[allow(clippy::too_many_arguments)]
pub fn diagnose_material_separator_cut(
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
    authoritative_case_search_budget: Duration,
    observation_case_search_budget: Duration,
) -> Result<MaterialSeparatorCutReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if worker_count == 0
        || authoritative_case_search_budget.is_zero()
        || observation_case_search_budget.is_zero()
    {
        return Err(invalid_input(
            "/material_separator_cut",
            "worker count and separator budgets must be positive",
        ));
    }
    let parent = diagnose_endpoint_source_only_control(
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
    )?;
    if parent.interpretation_blocked
        || !parent.source_partition_exact_cover
        || !parent.demand_continuation_unrestricted
    {
        return Err(invalid_input(
            "/parent",
            "material-separator cut requires an unblocked exact source-only parent",
        ));
    }
    if !accepted_phase3_fixture_satisfied(&parent, target_phase_index, fixed_width, fixed_height) {
        return Err(invalid_input(
            "/parent/accepted_fixture",
            "material-separator cut requires the accepted Phase 3 key-24 16x16 source-only fixture",
        ));
    }
    let surviving = parent
        .cases
        .iter()
        .filter(|case| case.solve.combined_outcome != ExactDimensionCaseOutcome::ProvenInfeasible)
        .collect::<Vec<_>>();
    if surviving.len() != 1
        || surviving[0].solve.combined_outcome != ExactDimensionCaseOutcome::Unknown
    {
        return Err(invalid_input(
            "/parent/cases",
            "material-separator cut requires exactly one unresolved source region",
        ));
    }
    let selected_source_case = surviving[0];
    let selected_source_arc = selected_source_case.source_selected;

    let endpoint_parent = &parent.parent;
    let cell_parent = &endpoint_parent.parent;
    if !cell_parent
        .cases
        .iter()
        .any(|case| case.key == endpoint_parent.selected_boundary_key)
    {
        return Err(invalid_input(
            "/parent/cases",
            "selected boundary child is missing",
        ));
    }
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
            "accepted separator fixture must retain four facilities and every fixed facility terminal",
        ));
    }
    let expected_separator_item_upper = i32::try_from(
        input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Pipe)
            .count(),
    )
    .map_err(|_| invalid_input("/parent/networks", "pipe network count exceeds i32"))?;
    let expected_separator_flow_upper = input
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
    if dimensions.width != fixed_width || dimensions.height != fixed_height {
        return Err(invalid_input(
            "/fixed_dimensions",
            "separator dimensions differ from the reconstructed parent",
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
    let source_continuation_cell = selected_source_arc[1];
    let base_restriction = exact::shared_layer::EndpointContinuationRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_selected: exact::shared_layer::DirectedGridArcRestriction {
            from: selected_source_arc[0],
            to: selected_source_arc[1],
        },
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
    let separator_base = exact::shared_layer::MaterialSeparatorRestriction {
        network_id: parent.selected_network_id.clone(),
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_cell: parent.source_cell,
        source_continuation_cell,
        demand_cell: parent.demand_cell,
        separator_after_row,
        selected_case_index: None,
    };
    let width = usize::try_from(dimensions.width).map_err(|_| {
        invalid_input(
            "/fixed_dimensions/width",
            "separator width must be positive",
        )
    })?;
    let height = usize::try_from(dimensions.height).map_err(|_| {
        invalid_input(
            "/fixed_dimensions/height",
            "separator height must be positive",
        )
    })?;
    if width == 0 || separator_after_row >= height.saturating_sub(1) {
        return Err(invalid_input(
            "/separator_after_row",
            "separator must have one complete row below it",
        ));
    }
    let candidates = (0..width)
        .map(|x| {
            [
                separator_after_row * width + x,
                (separator_after_row + 1) * width + x,
            ]
        })
        .collect::<Vec<_>>();
    let experiment_started = Instant::now();

    let control_case = SeparatorCaseInput { case_index: None };
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
        base_restriction.clone(),
        separator_base.clone(),
    );
    let control_authoritative_wall_ms = millis(control_authoritative_started.elapsed());
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
        base_restriction.clone(),
        separator_base.clone(),
    );
    let control_observation_wall_ms = millis(control_observation_started.elapsed());

    let child_inputs = (0..width)
        .map(|case_index| SeparatorCaseInput {
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
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let keys = allowed_keys.clone();
                    let continuation = base_restriction.clone();
                    let mut separator = separator_base.clone();
                    separator.selected_case_index = case.case_index;
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
                                separator,
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
                        .expect("authoritative material-separator worker panicked"),
                ));
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
                    let input = input.clone();
                    let coordinate = coordinate.clone();
                    let fixed_ports = fixed_ports.clone();
                    let terminal = selected_terminal.clone();
                    let keys = allowed_keys.clone();
                    let continuation = base_restriction.clone();
                    let mut separator = separator_base.clone();
                    separator.selected_case_index = case.case_index;
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
                                separator,
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
                        .expect("observation material-separator worker panicked"),
                ));
            }
        });
    }
    observations.sort_by_key(|(case, _)| case.case_index);
    let observation_wave_wall_ms = millis(observation_started.elapsed());

    let expected_external_terminal_count = cell_parent.parent.parent.static_certificates.len();
    let legal_boundary_keys = exact::reachable_boundary_keys(dimensions.width, dimensions.height);
    let control = build_case_report(
        &control_case,
        control_authoritative,
        control_observation,
        &requested,
        input.instances.len(),
        expected_external_terminal_count,
        &selected_terminal,
        endpoint_parent.selected_boundary_key,
        &legal_boundary_keys,
        &base_restriction,
        &separator_base,
        endpoint_parent,
        &candidates,
        [dimensions.width, dimensions.height],
        expected_separator_flow_upper,
        expected_separator_item_upper,
        &selected_source_case.solve,
        None,
    )?;
    let mut cases = Vec::with_capacity(child_inputs.len());
    for ((case, authoritative_result), (observed_case, observation_result)) in
        authoritative.into_iter().zip(observations)
    {
        if case.case_index != observed_case.case_index {
            return Err(invalid_input(
                "/cases",
                "authoritative and observation separator cases differ",
            ));
        }
        let selected = case.case_index.expect("child has a selected index");
        let mut restriction = separator_base.clone();
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
            &base_restriction,
            &restriction,
            endpoint_parent,
            &candidates,
            [dimensions.width, dimensions.height],
            expected_separator_flow_upper,
            expected_separator_item_upper,
            &control.solve,
            Some(selected),
        )?);
    }

    let control_parent_evidence_compatible = outcomes_compatible(
        selected_source_case.solve.combined_outcome,
        control.solve.combined_outcome,
    );
    let child_control_evidence_compatible =
        child_partition_evidence_compatible(control.solve.combined_outcome, &cases);
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
    let all_children_proven_infeasible = proven_infeasible_count == cases.len();
    let partition_non_empty = !candidates.is_empty();
    let partition_pairwise_disjoint =
        candidates.iter().collect::<BTreeSet<_>>().len() == candidates.len();
    let partition_exact_cover = partition_non_empty && partition_pairwise_disjoint;
    let interpretation_blocked = control.interpretation_blocked
        || cases.iter().any(|case| case.interpretation_blocked)
        || !control_parent_evidence_compatible
        || !child_control_evidence_compatible
        || !partition_exact_cover
        || invalid_witness_count > 0;
    let selected_item_code = control
        .authoritative_separator_certificates
        .first()
        .map(|certificate| certificate.selected_item_code)
        .ok_or_else(|| {
            invalid_input("/control/certificates", "separator certificate is missing")
        })?;
    Ok(MaterialSeparatorCutReport {
        schema_version: MATERIAL_SEPARATOR_CUT_SCHEMA_VERSION,
        target_phase_index,
        selected_source_case_index: selected_source_case.source_case_index,
        selected_source_arc,
        selected_network_id: parent.selected_network_id.clone(),
        selected_network_index: parent.selected_network_index,
        selected_item: parent.selected_item.clone(),
        selected_item_code,
        source_terminal: parent.source_terminal.clone(),
        demand_terminal: parent.demand_terminal.clone(),
        source_flow_units: parent.source_flow_units,
        demand_flow_units: parent.demand_flow_units,
        source_cell: parent.source_cell,
        source_continuation_cell,
        demand_cell: parent.demand_cell,
        fixed_dimensions: [dimensions.width, dimensions.height],
        separator_after_row,
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
        all_children_proven_infeasible,
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
    separator: exact::shared_layer::MaterialSeparatorRestriction,
) -> AuthoritativeResult {
    exact::shared_layer::solve_sparse_support_endpoints_boundary_key_continuation_and_material_separator_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
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
        vec![separator],
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
    separator: exact::shared_layer::MaterialSeparatorRestriction,
) -> ObservationResult {
    exact::shared_layer::solve_sparse_support_endpoints_boundary_key_continuation_and_material_separator_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
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
        vec![separator],
    )
}

fn accepted_phase3_fixture_satisfied(
    parent: &EndpointSourceOnlyControlReport,
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
) -> bool {
    let expected_network = "network:pipe:item-liquid-xiranite-poly";
    let expected_item = "item-liquid-xiranite-poly";
    let candidates_satisfied = parent.source_candidates.len() == 2
        && parent
            .source_candidates
            .iter()
            .enumerate()
            .all(|(index, candidate)| {
                candidate.case_index == index
                    && match index {
                        0 => {
                            [candidate.from, candidate.to] == [48, 32]
                                && candidate.preceding.is_empty()
                        }
                        1 => {
                            [candidate.from, candidate.to] == [48, 64]
                                && candidate.preceding == [[48, 32]]
                        }
                        _ => false,
                    }
            });
    let rejected_satisfied = parent.cases.iter().any(|case| {
        case.source_case_index == 0
            && case.source_selected == [48, 32]
            && case.root_infeasible
            && case.solve.combined_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
    });
    let survivor_satisfied = parent.cases.iter().any(|case| {
        case.source_case_index == 1
            && case.source_selected == [48, 64]
            && !case.root_infeasible
            && case.solve.combined_outcome == ExactDimensionCaseOutcome::Unknown
    });
    target_phase_index == 3
        && fixed_width == 16
        && fixed_height == 16
        && parent.selected_boundary_key == 24
        && parent.selected_network_id == expected_network
        && parent.selected_item == expected_item
        && parent.source_cell == 48
        && parent.demand_cell == 113
        && parent.cases.len() == 2
        && candidates_satisfied
        && rejected_satisfied
        && survivor_satisfied
}

#[allow(clippy::too_many_arguments)]
fn build_case_report(
    case: &SeparatorCaseInput,
    authoritative: AuthoritativeResult,
    observation: ObservationResult,
    requested: &[FacilityPortAssignment],
    facility_count: usize,
    expected_external_terminal_count: usize,
    selected_terminal: &str,
    selected_boundary_key: i32,
    legal_boundary_keys: &[i32],
    continuation_restriction: &exact::shared_layer::EndpointContinuationRestriction,
    separator_restriction: &exact::shared_layer::MaterialSeparatorRestriction,
    endpoint_parent: &EndpointContinuationPartitionReport,
    candidates: &[[usize; 2]],
    fixed_dimensions: [i32; 2],
    expected_flow_upper: i32,
    expected_item_upper: i32,
    baseline: &ExternalBoundaryKeySolveReport,
    expected_child_index: Option<usize>,
) -> Result<MaterialSeparatorCaseReport, IntegratedLayoutReport> {
    let (
        authoritative_layout,
        authoritative_boundary,
        authoritative_continuation,
        authoritative_separator,
    ) = authoritative;
    let (
        observation_layout,
        root_snapshot,
        observation_boundary,
        observation_continuation,
        observation_separator,
    ) = observation;
    let root_snapshot = root_snapshot.ok_or_else(|| {
        invalid_input(
            "/cases/root_snapshot",
            "material-separator observation did not return a root snapshot",
        )
    })?;
    let fixation_observation = assess_fixation(&root_snapshot, requested);
    let solve = solve_report(
        &format!(
            "material-separator-{}",
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
            continuation_restriction,
            endpoint_parent.selected_network_index,
            &endpoint_parent.selected_item,
            endpoint_parent.source_flow_units,
            endpoint_parent.demand_flow_units,
        )
        && authoritative_continuation[0].demand_selected.is_none()
        && authoritative_continuation[0].demand_preceding.is_empty();
    let separator_certificates_equal = authoritative_separator == observation_separator;
    let separator_certificate_satisfied = separator_certificate_satisfied(
        &authoritative_separator,
        separator_restriction,
        endpoint_parent,
        candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    ) && separator_certificate_satisfied(
        &observation_separator,
        separator_restriction,
        endpoint_parent,
        candidates,
        fixed_dimensions,
        expected_flow_upper,
        expected_item_upper,
    );
    let root_separator_restriction_satisfied = root_separator_audit_satisfied(
        &solve.root_snapshot,
        solve.observation_outcome,
        boundary_certificate_satisfied,
        source_only_certificate_satisfied,
        separator_certificate_satisfied,
        expected_child_index,
    );
    let facility_fixation_satisfied =
        root_facility_fixation_satisfied(&solve.root_snapshot, facility_count)
            && (!solve.fixation_observation.assertion_applies
                || solve.fixation_observation.assertion_satisfied);
    let semantic_model_contract_satisfied =
        semantic_model_contract(&solve.authoritative_layout, &solve.observation_layout);
    let baseline_scale = baseline.model_scale;
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
        && solve.model_scale.variables == baseline_scale.variables
        && solve.model_scale.constraints == baseline_scale.constraints + expected_constraint_delta
        && solve.model_scale.incidences == baseline_scale.incidences + expected_incidence_delta
        && solve.model_scale.placement_routing_incidences
            == baseline_scale.placement_routing_incidences;
    let hidden_domain_delta_observed = solve.root_snapshot.capture_status
        == "captured-before-first-decision"
        && baseline.root_snapshot.capture_status == "captured-before-first-decision";
    let hidden_domain_delta_satisfied = !hidden_domain_delta_observed
        || solve.root_snapshot.variable_coverage == baseline.root_snapshot.variable_coverage;
    let material_separator_family_satisfied = separator_family_satisfied(
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
        || !root_separator_restriction_satisfied
        || !facility_fixation_satisfied
        || !semantic_model_contract_satisfied
        || !hidden_domain_delta_satisfied
        || !material_separator_family_satisfied
        || !controlled_axis_model_satisfied;
    let root_infeasible = solve.root_snapshot.capture_status == "root-infeasible";
    let selected_arc = case.case_index.map(|index| candidates[index]);
    let preceding_arcs = case
        .case_index
        .map(|index| candidates[..index].to_vec())
        .unwrap_or_default();
    Ok(MaterialSeparatorCaseReport {
        case_index: case.case_index,
        selected_arc,
        preceding_arcs,
        root_infeasible,
        solve,
        authoritative_boundary_certificates: authoritative_boundary,
        observation_boundary_certificates: observation_boundary,
        authoritative_continuation_certificates: authoritative_continuation,
        observation_continuation_certificates: observation_continuation,
        authoritative_separator_certificates: authoritative_separator,
        observation_separator_certificates: observation_separator,
        boundary_certificates_equal,
        boundary_certificate_satisfied,
        continuation_certificates_equal,
        source_only_certificate_satisfied,
        separator_certificates_equal,
        separator_certificate_satisfied,
        root_separator_restriction_satisfied,
        facility_fixation_satisfied,
        semantic_model_contract_satisfied,
        hidden_domain_delta_observed,
        hidden_domain_delta_satisfied,
        material_separator_family_satisfied,
        controlled_axis_model_satisfied,
        interpretation_blocked,
    })
}

fn separator_certificate_satisfied(
    certificates: &[exact::shared_layer::MaterialSeparatorBuildCertificate],
    restriction: &exact::shared_layer::MaterialSeparatorRestriction,
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
        && certificate.source_continuation_cell == restriction.source_continuation_cell
        && certificate.demand_terminal == restriction.demand_terminal
        && certificate.demand_flow_units == endpoint_parent.demand_flow_units
        && certificate.demand_cell == restriction.demand_cell
        && [certificate.width, certificate.height] == fixed_dimensions
        && certificate.separator_after_row == restriction.separator_after_row
        && certificate.selected_case_index == restriction.selected_case_index
        && certificate.preceding_case_indices == expected_preceding
        && certificate.posted_selected_unary_constraints
            == usize::from(restriction.selected_case_index.is_some()) * 2
        && certificate.posted_exclusion_clauses
            == restriction.selected_case_index.unwrap_or_default()
        && certificate.candidates.len() == candidates.len()
        && certificate
            .candidates
            .iter()
            .zip(candidates)
            .enumerate()
            .all(|(index, (candidate, expected))| {
                candidate.case_index == index
                    && [candidate.from, candidate.to] == *expected
                    && candidate.direction == "south"
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
                    && candidate.selected_item_code == certificate.selected_item_code
                    && !candidate.route_selected_name.is_empty()
                    && !candidate.flow_name.is_empty()
                    && !candidate.from_item_name.is_empty()
            })
}

fn root_separator_audit_satisfied(
    root: &crate::layouts::RootDomainSnapshot,
    observation_outcome: ExactDimensionCaseOutcome,
    boundary_certificate_satisfied: bool,
    source_only_certificate_satisfied: bool,
    separator_certificate_satisfied: bool,
    selected_case_index: Option<usize>,
) -> bool {
    if root.capture_status == "root-infeasible" {
        return observation_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
            && boundary_certificate_satisfied
            && source_only_certificate_satisfied
            && separator_certificate_satisfied;
    }
    let [separator] = root.material_separators.as_slice() else {
        return false;
    };
    if separator.selected_case_index != selected_case_index {
        return false;
    }
    selected_case_index.is_none_or(|selected| {
        separator.candidates.get(selected).is_some_and(|candidate| {
            candidate.route_selected.lower_bound == 1
                && candidate.flow.lower_bound >= 1
                && candidate.from_item.cardinality == 1
                && candidate.from_item.lower_bound == candidate.selected_item_code
                && candidate.selected_item_possible
        })
    })
}

fn separator_family_satisfied(
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
                    .filter(|family| family.family == "material-separator")
                    .fold((0_u64, 0_u64), |(constraints, terms), family| {
                        (constraints + family.constraints, terms + family.terms)
                    })
            })
    };
    totals(baseline) == Some((0, 0))
        && totals(layout) == Some((expected_constraints, expected_terms))
}

fn outcomes_compatible(left: ExactDimensionCaseOutcome, right: ExactDimensionCaseOutcome) -> bool {
    !matches!(
        (left, right),
        (
            ExactDimensionCaseOutcome::ValidatedFeasible,
            ExactDimensionCaseOutcome::ProvenInfeasible
        ) | (
            ExactDimensionCaseOutcome::ProvenInfeasible,
            ExactDimensionCaseOutcome::ValidatedFeasible
        )
    )
}

fn child_partition_evidence_compatible(
    control: ExactDimensionCaseOutcome,
    cases: &[MaterialSeparatorCaseReport],
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
    use super::*;

    #[test]
    fn only_witness_proof_pairs_are_evidence_conflicts() {
        assert!(!outcomes_compatible(
            ExactDimensionCaseOutcome::ValidatedFeasible,
            ExactDimensionCaseOutcome::ProvenInfeasible,
        ));
        assert!(outcomes_compatible(
            ExactDimensionCaseOutcome::Unknown,
            ExactDimensionCaseOutcome::ProvenInfeasible,
        ));
    }
}
