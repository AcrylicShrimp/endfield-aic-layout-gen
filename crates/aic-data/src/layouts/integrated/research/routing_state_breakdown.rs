use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, WorldGridPosition, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{ExactObjectiveValue, IntegratedLayoutReport, exact};
use super::coordinate_partition::{invalid_input, millis, model_scale, prepare_target_input};
use super::rotation_partition::diagnose_cumulative_facility_rotation_partitions;
use super::{ExactDimensionCaseOutcome, ExactUsedDimensionCandidate, PartitionCaseModelScale};

pub const ROUTING_STATE_BREAKDOWN_SCHEMA_VERSION: u32 = 1;
pub const ROUTE_CELL_BREAKDOWN_SCHEMA_VERSION: u32 = 2;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingStateFamily {
    RouteCells,
    ArmItems,
    ArcActivation,
    ArcFlow,
    TopologyComponents,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingStateMatrixKind {
    Baseline,
    Independent,
    Cumulative,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RoutingStateBreakdownCaseReport {
    pub id: String,
    pub matrix: RoutingStateMatrixKind,
    pub fixed_families: Vec<RoutingStateFamily>,
    pub added_routing_fixation_equalities: u64,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub model_scale: PartitionCaseModelScale,
    pub observed_objective: Option<ExactObjectiveValue>,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RoutingStateBreakdownReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub reference_search_ms: u64,
    pub reference_objective: Option<ExactObjectiveValue>,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<RoutingStateBreakdownCaseReport>,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RouteCellLayerScope {
    Both,
    Belt,
    Pipe,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RouteCellValueScope {
    Both,
    Occupied,
    Empty,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RouteCellBreakdownCaseReport {
    pub id: String,
    pub layer: Option<RouteCellLayerScope>,
    pub value: Option<RouteCellValueScope>,
    pub network_id: Option<String>,
    pub cell: Option<WorldGridPosition>,
    pub added_route_cell_equalities: u64,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub model_scale: PartitionCaseModelScale,
    pub observed_objective: Option<ExactObjectiveValue>,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RouteCellBreakdownReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub reference_search_ms: u64,
    pub reference_objective: Option<ExactObjectiveValue>,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<RouteCellBreakdownCaseReport>,
    pub diagnostic_only: bool,
}

struct CaseDefinition {
    id: &'static str,
    matrix: RoutingStateMatrixKind,
    families: &'static [RoutingStateFamily],
}

const A: &[RoutingStateFamily] = &[RoutingStateFamily::RouteCells];
const B: &[RoutingStateFamily] = &[RoutingStateFamily::ArmItems];
const C: &[RoutingStateFamily] = &[RoutingStateFamily::ArcActivation];
const D: &[RoutingStateFamily] = &[RoutingStateFamily::ArcFlow];
const E: &[RoutingStateFamily] = &[RoutingStateFamily::TopologyComponents];
const AB: &[RoutingStateFamily] = &[RoutingStateFamily::RouteCells, RoutingStateFamily::ArmItems];
const ABC: &[RoutingStateFamily] = &[
    RoutingStateFamily::RouteCells,
    RoutingStateFamily::ArmItems,
    RoutingStateFamily::ArcActivation,
];
const ABCD: &[RoutingStateFamily] = &[
    RoutingStateFamily::RouteCells,
    RoutingStateFamily::ArmItems,
    RoutingStateFamily::ArcActivation,
    RoutingStateFamily::ArcFlow,
];
const ABCDE: &[RoutingStateFamily] = &[
    RoutingStateFamily::RouteCells,
    RoutingStateFamily::ArmItems,
    RoutingStateFamily::ArcActivation,
    RoutingStateFamily::ArcFlow,
    RoutingStateFamily::TopologyComponents,
];

const CASES: &[CaseDefinition] = &[
    CaseDefinition {
        id: "independent-a-route-cells",
        matrix: RoutingStateMatrixKind::Independent,
        families: A,
    },
    CaseDefinition {
        id: "independent-b-arm-items",
        matrix: RoutingStateMatrixKind::Independent,
        families: B,
    },
    CaseDefinition {
        id: "independent-c-arc-activation",
        matrix: RoutingStateMatrixKind::Independent,
        families: C,
    },
    CaseDefinition {
        id: "independent-d-arc-flow",
        matrix: RoutingStateMatrixKind::Independent,
        families: D,
    },
    CaseDefinition {
        id: "independent-e-topology-components",
        matrix: RoutingStateMatrixKind::Independent,
        families: E,
    },
    CaseDefinition {
        id: "cumulative-a-route-cells",
        matrix: RoutingStateMatrixKind::Cumulative,
        families: A,
    },
    CaseDefinition {
        id: "cumulative-ab-arm-items",
        matrix: RoutingStateMatrixKind::Cumulative,
        families: AB,
    },
    CaseDefinition {
        id: "cumulative-abc-arc-activation",
        matrix: RoutingStateMatrixKind::Cumulative,
        families: ABC,
    },
    CaseDefinition {
        id: "cumulative-abcd-arc-flow",
        matrix: RoutingStateMatrixKind::Cumulative,
        families: ABCD,
    },
    CaseDefinition {
        id: "cumulative-abcde-topology-components",
        matrix: RoutingStateMatrixKind::Cumulative,
        families: ABCDE,
    },
];

#[allow(clippy::too_many_arguments)]
pub fn diagnose_phase2_routing_state_breakdown(
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
) -> Result<RoutingStateBreakdownReport, IntegratedLayoutReport> {
    if case_search_budget.is_zero() {
        return Err(invalid_input(
            "/case_search_budget",
            "routing state breakdown requires a positive per-case budget",
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
            "rotation partition did not reproduce a validated routing reference witness",
        )
    })?;
    let reference_exact = reference
        .exact
        .as_ref()
        .expect("validated routing reference has exact metrics");
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
    let baseline_fixation_count = research_fixation_count(&baseline);
    let mut cases = vec![case_report(
        "baseline-routing-free",
        RoutingStateMatrixKind::Baseline,
        Vec::new(),
        0,
        baseline,
    )];

    for definition in CASES {
        let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_routing_ablation(
            input.clone(),
            logistics_components,
            Some(case_search_budget),
            fixed_dimensions,
            &reference,
            exact_fixation(definition.families),
        );
        let added = research_fixation_count(&layout).saturating_sub(baseline_fixation_count);
        cases.push(case_report(
            definition.id,
            definition.matrix,
            definition.families.to_vec(),
            added,
            layout,
        ));
    }

    Ok(RoutingStateBreakdownReport {
        schema_version: ROUTING_STATE_BREAKDOWN_SCHEMA_VERSION,
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
        cases,
        diagnostic_only: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_phase2_route_cell_breakdown(
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
) -> Result<RouteCellBreakdownReport, IntegratedLayoutReport> {
    if case_search_budget.is_zero() {
        return Err(invalid_input(
            "/case_search_budget",
            "route-cell breakdown requires a positive per-case budget",
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
            "rotation partition did not reproduce a validated route-cell reference witness",
        )
    })?;
    let reference_exact = reference
        .exact
        .as_ref()
        .expect("validated route-cell reference has exact metrics");
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
    let baseline_fixation_count = research_fixation_count(&baseline);
    let mut cases = vec![route_cell_case_report(
        "baseline-routing-free",
        None,
        None,
        None,
        None,
        0,
        baseline,
    )];
    let definitions = [
        (
            "all-cells",
            RouteCellLayerScope::Both,
            RouteCellValueScope::Both,
        ),
        (
            "belt-all-cells",
            RouteCellLayerScope::Belt,
            RouteCellValueScope::Both,
        ),
        (
            "pipe-all-cells",
            RouteCellLayerScope::Pipe,
            RouteCellValueScope::Both,
        ),
        (
            "occupied-cells",
            RouteCellLayerScope::Both,
            RouteCellValueScope::Occupied,
        ),
        (
            "empty-cells",
            RouteCellLayerScope::Both,
            RouteCellValueScope::Empty,
        ),
        (
            "belt-occupied-cells",
            RouteCellLayerScope::Belt,
            RouteCellValueScope::Occupied,
        ),
        (
            "belt-empty-cells",
            RouteCellLayerScope::Belt,
            RouteCellValueScope::Empty,
        ),
        (
            "pipe-occupied-cells",
            RouteCellLayerScope::Pipe,
            RouteCellValueScope::Occupied,
        ),
        (
            "pipe-empty-cells",
            RouteCellLayerScope::Pipe,
            RouteCellValueScope::Empty,
        ),
    ];
    for (id, layer, value) in definitions {
        let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_routing_ablation(
            input.clone(),
            logistics_components,
            Some(case_search_budget),
            fixed_dimensions,
            &reference,
            route_cell_fixation(layer, value),
        );
        let added = research_fixation_count(&layout).saturating_sub(baseline_fixation_count);
        cases.push(route_cell_case_report(
            id,
            Some(layer),
            Some(value),
            None,
            None,
            added,
            layout,
        ));
    }
    let mut successful_networks = Vec::new();
    for (network_index, network) in input.networks.iter().enumerate() {
        let network_id = network.id().to_string();
        let layer = match network.transport() {
            crate::logistics::TransportKind::Belt => RouteCellLayerScope::Belt,
            crate::logistics::TransportKind::Pipe => RouteCellLayerScope::Pipe,
        };
        let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_routing_ablation(
            input.clone(),
            logistics_components,
            Some(case_search_budget),
            fixed_dimensions,
            &reference,
            exact::shared_layer::ReferenceRoutingFixation {
                route_cells: true,
                route_cell_transport: Some(network.transport()),
                route_cell_value: Some(true),
                route_cell_network_index: Some(network_index),
                ..Default::default()
            },
        );
        let added = research_fixation_count(&layout).saturating_sub(baseline_fixation_count);
        let id = format!("{}-occupied-cells", network_id.replace(':', "-"));
        let case = route_cell_case_report(
            &id,
            Some(layer),
            Some(RouteCellValueScope::Occupied),
            Some(&network_id),
            None,
            added,
            layout,
        );
        if case.outcome == ExactDimensionCaseOutcome::ValidatedFeasible {
            successful_networks.push((added, network_index, network_id));
        }
        cases.push(case);
    }
    if let Some((_, network_index, network_id)) = successful_networks
        .into_iter()
        .min_by_key(|(added, _, _)| *added)
    {
        let network = &input.networks[network_index];
        let prior_network = reference
            .transport_networks
            .iter()
            .find(|candidate| candidate.id == network_id)
            .expect("selected reference network exists");
        let mut cells = prior_network.cells.clone();
        cells.sort_by_key(|cell| (cell.y, cell.x));
        for (cell_index, cell) in cells.into_iter().enumerate() {
            let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_routing_ablation(
                input.clone(),
                logistics_components,
                Some(case_search_budget),
                fixed_dimensions,
                &reference,
                exact::shared_layer::ReferenceRoutingFixation {
                    route_cells: true,
                    route_cell_transport: Some(network.transport()),
                    route_cell_value: Some(true),
                    route_cell_network_index: Some(network_index),
                    route_cell_network_cell_index: Some(cell_index),
                    ..Default::default()
                },
            );
            let added = research_fixation_count(&layout).saturating_sub(baseline_fixation_count);
            let id = format!(
                "single-occupied-cell-{}-{}-{}",
                network_id.replace(':', "-"),
                cell.x,
                cell.y
            );
            cases.push(route_cell_case_report(
                &id,
                Some(match network.transport() {
                    crate::logistics::TransportKind::Belt => RouteCellLayerScope::Belt,
                    crate::logistics::TransportKind::Pipe => RouteCellLayerScope::Pipe,
                }),
                Some(RouteCellValueScope::Occupied),
                Some(&network_id),
                Some(cell),
                added,
                layout,
            ));
        }
    }

    Ok(RouteCellBreakdownReport {
        schema_version: ROUTE_CELL_BREAKDOWN_SCHEMA_VERSION,
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
        cases,
        diagnostic_only: true,
    })
}

fn route_cell_fixation(
    layer: RouteCellLayerScope,
    value: RouteCellValueScope,
) -> exact::shared_layer::ReferenceRoutingFixation {
    exact::shared_layer::ReferenceRoutingFixation {
        route_cells: true,
        route_cell_transport: match layer {
            RouteCellLayerScope::Both => None,
            RouteCellLayerScope::Belt => Some(crate::logistics::TransportKind::Belt),
            RouteCellLayerScope::Pipe => Some(crate::logistics::TransportKind::Pipe),
        },
        route_cell_value: match value {
            RouteCellValueScope::Both => None,
            RouteCellValueScope::Occupied => Some(true),
            RouteCellValueScope::Empty => Some(false),
        },
        ..Default::default()
    }
}

fn exact_fixation(
    families: &[RoutingStateFamily],
) -> exact::shared_layer::ReferenceRoutingFixation {
    exact::shared_layer::ReferenceRoutingFixation {
        route_cells: families.contains(&RoutingStateFamily::RouteCells),
        arm_items: families.contains(&RoutingStateFamily::ArmItems),
        arc_activation: families.contains(&RoutingStateFamily::ArcActivation),
        arc_flow: families.contains(&RoutingStateFamily::ArcFlow),
        topology_components: families.contains(&RoutingStateFamily::TopologyComponents),
        ..Default::default()
    }
}

fn research_fixation_count(layout: &IntegratedLayoutReport) -> u64 {
    layout
        .exact
        .as_ref()
        .and_then(|exact| exact.model_complexity.constraints.as_ref())
        .into_iter()
        .flat_map(|constraints| constraints.by_family.iter())
        .filter(|family| family.family == "research-fixation")
        .map(|family| family.constraints)
        .sum()
}

fn case_report(
    id: &str,
    matrix: RoutingStateMatrixKind,
    fixed_families: Vec<RoutingStateFamily>,
    added_routing_fixation_equalities: u64,
    layout: IntegratedLayoutReport,
) -> RoutingStateBreakdownCaseReport {
    let outcome = super::coordinate_partition::classify_outcome(&layout);
    let exact = layout
        .exact
        .as_ref()
        .expect("executed routing state case has exact metrics");
    RoutingStateBreakdownCaseReport {
        id: id.to_string(),
        matrix,
        fixed_families,
        added_routing_fixation_equalities,
        outcome,
        construction_ms: exact.construction_ms,
        search_ms: exact.search_ms,
        first_incumbent_ms: exact.first_incumbent_ms,
        model_scale: model_scale(exact),
        observed_objective: exact.objective,
        layout,
    }
}

fn route_cell_case_report(
    id: &str,
    layer: Option<RouteCellLayerScope>,
    value: Option<RouteCellValueScope>,
    network_id: Option<&str>,
    cell: Option<WorldGridPosition>,
    added_route_cell_equalities: u64,
    layout: IntegratedLayoutReport,
) -> RouteCellBreakdownCaseReport {
    let outcome = super::coordinate_partition::classify_outcome(&layout);
    let exact = layout
        .exact
        .as_ref()
        .expect("executed route-cell case has exact metrics");
    RouteCellBreakdownCaseReport {
        id: id.to_string(),
        layer,
        value,
        network_id: network_id.map(str::to_string),
        cell,
        added_route_cell_equalities,
        outcome,
        construction_ms: exact.construction_ms,
        search_ms: exact.search_ms,
        first_incumbent_ms: exact.first_incumbent_ms,
        model_scale: model_scale(exact),
        observed_objective: exact.objective,
        layout,
    }
}
