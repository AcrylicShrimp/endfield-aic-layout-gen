use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{ExactObjectiveValue, IntegratedLayoutReport, exact};
use super::coordinate_partition::{invalid_input, millis, model_scale, prepare_target_input};
use super::rotation_partition::diagnose_cumulative_facility_rotation_partitions;
use super::{ExactDimensionCaseOutcome, ExactUsedDimensionCandidate, PartitionCaseModelScale};

pub const CONNECTIVITY_WITNESS_DIAGNOSIS_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectivityWitnessCaseKind {
    Baseline,
    ParentDepthForest,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct ConnectivityWitnessStateScale {
    pub reachability_variables: u64,
    pub root_variables: u64,
    pub parent_variables: u64,
    pub depth_variables: u64,
    pub constraints: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectivityWitnessCaseReport {
    pub kind: ConnectivityWitnessCaseKind,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub model_scale: PartitionCaseModelScale,
    pub witness_state: ConnectivityWitnessStateScale,
    pub observed_objective: Option<ExactObjectiveValue>,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectivityWitnessDiagnosisReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub reference_search_ms: u64,
    pub reference_objective: Option<ExactObjectiveValue>,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<ConnectivityWitnessCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_phase2_connectivity_witness(
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
) -> Result<ConnectivityWitnessDiagnosisReport, IntegratedLayoutReport> {
    if case_search_budget.is_zero() {
        return Err(invalid_input(
            "/case_search_budget",
            "connectivity witness diagnosis requires a positive per-case budget",
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
            "rotation partition did not reproduce a validated connectivity reference witness",
        )
    })?;
    let reference_exact = reference
        .exact
        .as_ref()
        .expect("validated connectivity reference has exact metrics");
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
    let witnessed = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_connectivity_witness(
        input,
        logistics_components,
        Some(case_search_budget),
        fixed_dimensions,
        &reference,
    );

    Ok(ConnectivityWitnessDiagnosisReport {
        schema_version: CONNECTIVITY_WITNESS_DIAGNOSIS_SCHEMA_VERSION,
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
            case_report(ConnectivityWitnessCaseKind::Baseline, baseline),
            case_report(ConnectivityWitnessCaseKind::ParentDepthForest, witnessed),
        ],
        diagnostic_only: true,
    })
}

fn case_report(
    kind: ConnectivityWitnessCaseKind,
    layout: IntegratedLayoutReport,
) -> ConnectivityWitnessCaseReport {
    let outcome = super::coordinate_partition::classify_outcome(&layout);
    let exact = layout
        .exact
        .as_ref()
        .expect("executed connectivity case has exact metrics");
    ConnectivityWitnessCaseReport {
        kind,
        outcome,
        construction_ms: exact.construction_ms,
        search_ms: exact.search_ms,
        first_incumbent_ms: exact.first_incumbent_ms,
        model_scale: model_scale(exact),
        witness_state: witness_state(exact),
        observed_objective: exact.objective,
        layout,
    }
}

fn witness_state(exact: &super::super::ExactSolveReport) -> ConnectivityWitnessStateScale {
    let variable_count = |name: &str| {
        exact
            .model_complexity
            .variables
            .by_family
            .iter()
            .find(|family| family.family == name)
            .map_or(0, |family| family.total_variables)
    };
    let constraints = exact
        .model_complexity
        .constraints
        .as_ref()
        .into_iter()
        .flat_map(|summary| &summary.by_family)
        .filter(|family| family.family == "connectivity-witness")
        .map(|family| family.constraints)
        .sum();
    ConnectivityWitnessStateScale {
        reachability_variables: variable_count("connectivity-reachability"),
        root_variables: variable_count("connectivity-root"),
        parent_variables: variable_count("connectivity-parent"),
        depth_variables: variable_count("connectivity-depth"),
        constraints,
    }
}
