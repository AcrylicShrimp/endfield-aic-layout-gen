use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{ExactSearchStatistics, IntegratedLayoutReport, exact};
use super::coordinate_partition::{
    FacilityPortAssignment, FacilityPortDomainReport, PartitionCaseModelScale, classify_outcome,
    enumerate_port_assignments, invalid_input, millis, model_scale, prepare_target_input,
    validate_inputs,
};
use super::{
    ExactDimensionCaseOutcome, ExactDimensionSolverStack,
    sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation,
};

pub const RESIDUAL_FACILITY_STATE_ABLATION_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ResidualFacilityStateCaseKind {
    IntroducedStateOnly,
    PriorOverlapPlacements,
    PriorOverlapPlacementsAndFacilityPorts,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResidualFacilityStateCaseReport {
    pub kind: ResidualFacilityStateCaseKind,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub added_constraint_count_from_baseline: i64,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResidualFacilityStateAblationReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub fixed_dimensions: [i32; 2],
    pub partitioned_facility: String,
    pub fixed_coordinate: [i32; 2],
    pub port_assignment_index: usize,
    pub fixed_ports: Vec<FacilityPortAssignment>,
    pub fixed_rotation: i64,
    pub prior_placement_count: usize,
    pub prior_facility_terminal_count: usize,
    pub prefix_search_budget_ms_per_case: u64,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<ResidualFacilityStateCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_residual_facility_state_ablation(
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
    worker_count: usize,
    prefix_search_budget: Duration,
    case_search_budget: Duration,
) -> Result<ResidualFacilityStateAblationReport, IntegratedLayoutReport> {
    validate_inputs(
        target_phase_index,
        fixed_width,
        fixed_height,
        worker_count,
        prefix_search_budget,
        case_search_budget,
    )?;
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success || target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            "facility growth planning failed or the target phase is out of range",
        ));
    }
    let introduced = &growth.phases[target_phase_index].facilities;
    if introduced.len() != 1 {
        return Err(invalid_input(
            "/target_phase_index",
            "residual facility-state ablation requires exactly one introduced facility",
        ));
    }
    let partitioned_facility = introduced[0].clone();
    let prefix = sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        target_phase_index - 1,
        worker_count,
        prefix_search_budget,
    )?;
    let prior_solution = prefix.layout;
    if !prefix.completed_target_phase || !prior_solution.success {
        return Err(invalid_input(
            "/prefix",
            "preceding cumulative phase did not produce a validated active-stack hint",
        ));
    }

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
    let legal_rotations = exact::shared_layer::facility_rotations_at_coordinate(
        &input,
        &partitioned_facility,
        fixed_x,
        fixed_y,
    )
    .map_err(IntegratedLayoutReport::invalid)?;
    if !legal_rotations.contains(&fixed_rotation) {
        return Err(invalid_input(
            "/fixed_rotation",
            "the selected rotation is not legal at the fixed coordinate",
        ));
    }
    let terminal_domains =
        exact::shared_layer::facility_port_partition_domains(&input, &partitioned_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .map(|domain| FacilityPortDomainReport {
                terminal: domain.terminal,
                ports: domain.ports,
            })
            .collect::<Vec<_>>();
    let assignments = enumerate_port_assignments(&terminal_domains);
    let fixed_ports = assignments
        .get(port_assignment_index)
        .cloned()
        .ok_or_else(|| {
            invalid_input(
                "/port_assignment_index",
                format!(
                    "port assignment index {port_assignment_index} is outside 0..{}",
                    assignments.len()
                ),
            )
        })?;
    let exact_ports = fixed_ports
        .iter()
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();
    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: fixed_width,
        height: fixed_height,
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: partitioned_facility.clone(),
        x: fixed_x,
        y: fixed_y,
        rotation: Some(fixed_rotation),
    };
    let kinds = [
        ResidualFacilityStateCaseKind::IntroducedStateOnly,
        ResidualFacilityStateCaseKind::PriorOverlapPlacements,
        ResidualFacilityStateCaseKind::PriorOverlapPlacementsAndFacilityPorts,
    ];
    let started = Instant::now();
    let mut completed = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for kind in kinds {
            let input = input.clone();
            let ports = exact_ports.clone();
            let coordinate = coordinate.clone();
            let prior_solution = &prior_solution;
            handles.push((
                kind,
                scope.spawn(move || match kind {
                    ResidualFacilityStateCaseKind::IntroducedStateOnly => {
                        exact::shared_layer::solve_factored_endpoints_fixed_dimensions_coordinate_ports_feasibility_only_with_prior_and_local_continuation(
                            input,
                            logistics_components,
                            Some(case_search_budget),
                            dimensions,
                            coordinate,
                            ports,
                            Some(prior_solution),
                        )
                    }
                    ResidualFacilityStateCaseKind::PriorOverlapPlacements => {
                        exact::shared_layer::solve_factored_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                            input,
                            logistics_components,
                            Some(case_search_budget),
                            dimensions,
                            coordinate,
                            ports,
                            prior_solution,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                        )
                    }
                    ResidualFacilityStateCaseKind::PriorOverlapPlacementsAndFacilityPorts => {
                        exact::shared_layer::solve_factored_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                            input,
                            logistics_components,
                            Some(case_search_budget),
                            dimensions,
                            coordinate,
                            ports,
                            prior_solution,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacementsAndFacilityPorts,
                        )
                    }
                }),
            ));
        }
        for (kind, handle) in handles {
            completed.push((
                kind,
                handle
                    .join()
                    .expect("residual facility-state ablation worker panicked"),
            ));
        }
    });
    let baseline_constraints = completed
        .iter()
        .find(|(kind, _)| *kind == ResidualFacilityStateCaseKind::IntroducedStateOnly)
        .and_then(|(_, layout)| layout.exact.as_ref())
        .map(model_scale)
        .map(|scale| scale.constraints)
        .expect("executed baseline has exact model metrics");
    let cases = completed
        .into_iter()
        .map(|(kind, layout)| {
            let exact = layout
                .exact
                .as_ref()
                .expect("executed ablation has exact metrics");
            let scale = model_scale(exact);
            ResidualFacilityStateCaseReport {
                kind,
                outcome: classify_outcome(&layout),
                construction_ms: exact.construction_ms,
                search_ms: exact.search_ms,
                first_incumbent_ms: exact.first_incumbent_ms,
                search_statistics: exact.search_statistics,
                model_scale: scale,
                added_constraint_count_from_baseline: i64::try_from(scale.constraints)
                    .expect("constraint count fits i64")
                    - i64::try_from(baseline_constraints).expect("constraint count fits i64"),
                layout,
            }
        })
        .collect();
    let prior_facility_terminal_count = prior_solution
        .transport_networks
        .iter()
        .flat_map(|network| network.terminals.iter())
        .filter(|terminal| {
            matches!(
                terminal.endpoint,
                super::super::TransportNetworkEndpoint::Facility { .. }
            )
        })
        .count();

    Ok(ResidualFacilityStateAblationReport {
        schema_version: RESIDUAL_FACILITY_STATE_ABLATION_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        fixed_dimensions: [fixed_width, fixed_height],
        partitioned_facility,
        fixed_coordinate: [fixed_x, fixed_y],
        port_assignment_index,
        fixed_ports,
        fixed_rotation,
        prior_placement_count: prior_solution.placements.len(),
        prior_facility_terminal_count,
        prefix_search_budget_ms_per_case: millis(prefix_search_budget),
        case_search_budget_ms: millis(case_search_budget),
        outer_wall_ms: millis(started.elapsed()),
        cases,
        diagnostic_only: true,
    })
}
