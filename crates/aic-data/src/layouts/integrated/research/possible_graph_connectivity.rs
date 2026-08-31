use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{ExactObjectiveValue, ExactSearchStatistics, IntegratedLayoutReport, exact};
use super::coordinate_partition::{invalid_input, millis, model_scale, prepare_target_input};
use super::rotation_partition::diagnose_cumulative_facility_rotation_partitions;
use super::{ExactDimensionCaseOutcome, ExactUsedDimensionCandidate, PartitionCaseModelScale};

pub const POSSIBLE_GRAPH_CONNECTIVITY_DIAGNOSIS_SCHEMA_VERSION: u32 = 7;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PossibleGraphConnectivityCaseKind {
    Baseline,
    PossibleGraphPropagator,
    EventSelectivePossibleGraphPropagator,
    LazyTraversalPossibleGraphPropagator,
    GroupedDemandPossibleGraphPropagator,
    DemandSilentPossibleGraphPropagator,
    LayerGridOpportunityAnalyzer,
    TerminalSupportGridPropagator,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct PossibleGraphConnectivityScale {
    pub propagators: u64,
    pub subscribed_variable_incidences: u64,
    pub maximum_propagator_arity: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct PossibleGraphConnectivityRuntime {
    pub propagations: u64,
    pub arcs_scanned: u64,
    pub demand_options_checked: u64,
    pub demand_pruning_attempts: u64,
    pub selected_demand_conflicts: u64,
    pub maximum_reason_predicates: u64,
    pub predicate_notifications: u64,
    pub registered_predicates: u64,
    pub reachability_arc_checks: u64,
    pub reason_builds: u64,
    pub reason_arc_scans: u64,
    pub demand_cells_checked: u64,
    pub registered_domain_variables: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct LayerGridAnalyzerRuntime {
    pub executions: u64,
    pub material_passes: u64,
    pub selected_demand_options: u64,
    pub selected_demand_cells: u64,
    pub reachable_selected_demand_cells: u64,
    pub unique_support_steps: u64,
    pub unresolved_predicate_observations: u64,
    pub terminal_support_steps: u64,
    pub terminal_unresolved_predicate_observations: u64,
    pub distinct_support_arcs: u64,
    pub distinct_unresolved_predicates: u64,
    pub distinct_terminal_support_arcs: u64,
    pub distinct_terminal_unresolved_predicates: u64,
    pub maximum_unique_support_chain: u64,
    pub registered_domain_variables: u64,
    pub forced_predicate_attempts: u64,
    pub forcing_conflicts: u64,
    pub maximum_reason_predicates: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PossibleGraphConnectivityCaseReport {
    pub kind: PossibleGraphConnectivityCaseKind,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub connectivity_scale: PossibleGraphConnectivityScale,
    pub connectivity_runtime: PossibleGraphConnectivityRuntime,
    pub grid_analyzer_runtime: LayerGridAnalyzerRuntime,
    pub observed_objective: Option<ExactObjectiveValue>,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PossibleGraphConnectivityDiagnosisReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub reference_search_ms: u64,
    pub reference_objective: Option<ExactObjectiveValue>,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<PossibleGraphConnectivityCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_phase2_possible_graph_connectivity(
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
    prefix_search_budget: Duration,
    reference_search_budget: Duration,
    case_search_budget: Duration,
) -> Result<PossibleGraphConnectivityDiagnosisReport, IntegratedLayoutReport> {
    if case_search_budget.is_zero() {
        return Err(invalid_input(
            "/case_search_budget",
            "possible graph connectivity diagnosis requires a positive per-case budget",
        ));
    }
    let reference_run = diagnose_cumulative_facility_rotation_partitions(
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
        prefix_search_budget,
        reference_search_budget,
    )?;
    let reference = reference_run.selected_witness.ok_or_else(|| {
        invalid_input(
            "/reference",
            "rotation partition did not reproduce a validated possible graph reference witness",
        )
    })?;
    let reference_exact = reference
        .exact
        .as_ref()
        .expect("validated possible graph reference has exact metrics");
    let reference_search_ms = reference_exact.search_ms;
    let reference_objective = reference_exact.objective;
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
    let fixed_dimensions = exact::shared_layer::FixedUsedDimensions {
        width: fixed_width,
        height: fixed_height,
    };
    let started = Instant::now();
    let baseline =
        exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_ablation(
            input.clone(),
            logistics_components,
            Some(case_search_budget),
            fixed_dimensions,
            &reference,
            exact::shared_layer::ReferenceAblationFixation::PlacementsAndAllTerminals,
        );
    let (propagated, propagated_runtime) = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_possible_graph_connectivity(
        input.clone(),
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );
    let (event_selective, event_selective_runtime) = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_event_selective_possible_graph_connectivity(
        input.clone(),
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );
    let (lazy, lazy_runtime) = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_lazy_possible_graph_connectivity(
        input.clone(),
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );
    let (grouped, grouped_runtime) = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_grouped_demand_possible_graph_connectivity(
        input.clone(),
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );
    let (demand_silent, demand_silent_runtime) = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_demand_silent_possible_graph_connectivity(
        input.clone(),
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );
    let (grid_analyzed, grid_connectivity_runtime, grid_runtime) = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_layer_grid_analysis(
        input.clone(),
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );
    let (grid_propagated, active_grid_connectivity_runtime, active_grid_runtime) = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_terminal_support_grid_propagation(
        input,
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );

    Ok(PossibleGraphConnectivityDiagnosisReport {
        schema_version: POSSIBLE_GRAPH_CONNECTIVITY_DIAGNOSIS_SCHEMA_VERSION,
        target_phase_index,
        fixed_dimensions: ExactUsedDimensionCandidate {
            width: fixed_width,
            height: fixed_height,
            area: i64::from(fixed_width) * i64::from(fixed_height),
        },
        reference_search_ms,
        reference_objective,
        case_search_budget_ms: millis(case_search_budget),
        outer_wall_ms: millis(started.elapsed()),
        cases: vec![
            case_report(
                PossibleGraphConnectivityCaseKind::Baseline,
                baseline,
                PossibleGraphConnectivityRuntime::default(),
                LayerGridAnalyzerRuntime::default(),
            ),
            case_report(
                PossibleGraphConnectivityCaseKind::PossibleGraphPropagator,
                propagated,
                connectivity_runtime(propagated_runtime),
                LayerGridAnalyzerRuntime::default(),
            ),
            case_report(
                PossibleGraphConnectivityCaseKind::EventSelectivePossibleGraphPropagator,
                event_selective,
                connectivity_runtime(event_selective_runtime),
                LayerGridAnalyzerRuntime::default(),
            ),
            case_report(
                PossibleGraphConnectivityCaseKind::LazyTraversalPossibleGraphPropagator,
                lazy,
                connectivity_runtime(lazy_runtime),
                LayerGridAnalyzerRuntime::default(),
            ),
            case_report(
                PossibleGraphConnectivityCaseKind::GroupedDemandPossibleGraphPropagator,
                grouped,
                connectivity_runtime(grouped_runtime),
                LayerGridAnalyzerRuntime::default(),
            ),
            case_report(
                PossibleGraphConnectivityCaseKind::DemandSilentPossibleGraphPropagator,
                demand_silent,
                connectivity_runtime(demand_silent_runtime),
                LayerGridAnalyzerRuntime::default(),
            ),
            case_report(
                PossibleGraphConnectivityCaseKind::LayerGridOpportunityAnalyzer,
                grid_analyzed,
                connectivity_runtime(grid_connectivity_runtime),
                grid_analyzer_runtime(grid_runtime),
            ),
            case_report(
                PossibleGraphConnectivityCaseKind::TerminalSupportGridPropagator,
                grid_propagated,
                connectivity_runtime(active_grid_connectivity_runtime),
                grid_analyzer_runtime(active_grid_runtime),
            ),
        ],
        diagnostic_only: true,
    })
}

fn connectivity_runtime(
    statistics: exact::PossibleRouteReachabilityStatistics,
) -> PossibleGraphConnectivityRuntime {
    PossibleGraphConnectivityRuntime {
        propagations: statistics.propagations,
        arcs_scanned: statistics.arcs_scanned,
        demand_options_checked: statistics.demand_options_checked,
        demand_pruning_attempts: statistics.demand_pruning_attempts,
        selected_demand_conflicts: statistics.selected_demand_conflicts,
        maximum_reason_predicates: statistics.maximum_reason_predicates,
        predicate_notifications: statistics.predicate_notifications,
        registered_predicates: statistics.registered_predicates,
        reachability_arc_checks: statistics.reachability_arc_checks,
        reason_builds: statistics.reason_builds,
        reason_arc_scans: statistics.reason_arc_scans,
        demand_cells_checked: statistics.demand_cells_checked,
        registered_domain_variables: statistics.registered_domain_variables,
    }
}

fn grid_analyzer_runtime(
    statistics: exact::LayerGridAnalyzerStatistics,
) -> LayerGridAnalyzerRuntime {
    LayerGridAnalyzerRuntime {
        executions: statistics.executions,
        material_passes: statistics.material_passes,
        selected_demand_options: statistics.selected_demand_options,
        selected_demand_cells: statistics.selected_demand_cells,
        reachable_selected_demand_cells: statistics.reachable_selected_demand_cells,
        unique_support_steps: statistics.unique_support_steps,
        unresolved_predicate_observations: statistics.unresolved_predicate_observations,
        terminal_support_steps: statistics.terminal_support_steps,
        terminal_unresolved_predicate_observations: statistics
            .terminal_unresolved_predicate_observations,
        distinct_support_arcs: statistics.distinct_support_arcs,
        distinct_unresolved_predicates: statistics.distinct_unresolved_predicates,
        distinct_terminal_support_arcs: statistics.distinct_terminal_support_arcs,
        distinct_terminal_unresolved_predicates: statistics.distinct_terminal_unresolved_predicates,
        maximum_unique_support_chain: statistics.maximum_unique_support_chain,
        registered_domain_variables: statistics.registered_domain_variables,
        forced_predicate_attempts: statistics.forced_predicate_attempts,
        forcing_conflicts: statistics.forcing_conflicts,
        maximum_reason_predicates: statistics.maximum_reason_predicates,
    }
}

fn case_report(
    kind: PossibleGraphConnectivityCaseKind,
    layout: IntegratedLayoutReport,
    connectivity_runtime: PossibleGraphConnectivityRuntime,
    grid_analyzer_runtime: LayerGridAnalyzerRuntime,
) -> PossibleGraphConnectivityCaseReport {
    let outcome = super::coordinate_partition::classify_outcome(&layout);
    let exact = layout
        .exact
        .as_ref()
        .expect("executed possible graph case has exact metrics");
    PossibleGraphConnectivityCaseReport {
        kind,
        outcome,
        construction_ms: exact.construction_ms,
        search_ms: exact.search_ms,
        first_incumbent_ms: exact.first_incumbent_ms,
        search_statistics: exact.search_statistics,
        model_scale: model_scale(exact),
        connectivity_scale: connectivity_scale(exact),
        connectivity_runtime,
        grid_analyzer_runtime,
        observed_objective: exact.objective,
        layout,
    }
}

fn connectivity_scale(exact: &super::super::ExactSolveReport) -> PossibleGraphConnectivityScale {
    exact
        .model_complexity
        .constraints
        .as_ref()
        .into_iter()
        .flat_map(|summary| &summary.by_family)
        .filter(|family| family.family == "connectivity-propagator")
        .fold(
            PossibleGraphConnectivityScale::default(),
            |mut scale, family| {
                scale.propagators += family.constraints;
                scale.subscribed_variable_incidences += family.terms;
                scale.maximum_propagator_arity =
                    scale.maximum_propagator_arity.max(family.maximum_arity);
                scale
            },
        )
}
