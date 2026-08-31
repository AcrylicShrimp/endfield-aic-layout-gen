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
use super::{
    ExactDimensionCaseOutcome, ExactUsedDimensionCandidate, PartitionCaseModelScale,
    sweep_cumulative_integrated_layout_fixed_dimensions,
};

pub const TRANSPORT_TILE_CAP_DIAGNOSIS_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransportTileCapCaseReport {
    pub transport_tile_upper_bound: Option<u32>,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub model_scale: PartitionCaseModelScale,
    pub observed_objective: Option<ExactObjectiveValue>,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransportTileCapDiagnosisReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub prefix_hint_dimensions: Option<[i64; 2]>,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<TransportTileCapCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_cumulative_transport_tile_caps(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
    transport_tile_upper_bounds: &[u32],
    prefix_worker_count: usize,
    prefix_search_budget: Duration,
    case_search_budget: Duration,
) -> Result<TransportTileCapDiagnosisReport, IntegratedLayoutReport> {
    if target_phase_index == 0 {
        return Err(invalid_input(
            "/target_phase_index",
            "transport tile cap diagnosis requires a preceding cumulative phase hint",
        ));
    }
    if fixed_width <= 0 || fixed_height <= 0 {
        return Err(invalid_input(
            "/fixed_dimensions",
            "transport tile cap diagnosis requires positive fixed dimensions",
        ));
    }
    if prefix_worker_count == 0 {
        return Err(invalid_input(
            "/prefix_worker_count",
            "transport tile cap diagnosis requires at least one prefix worker",
        ));
    }
    if prefix_search_budget.is_zero() || case_search_budget.is_zero() {
        return Err(invalid_input(
            "/search_budget",
            "transport tile cap diagnosis requires positive prefix and case budgets",
        ));
    }
    if transport_tile_upper_bounds.is_empty() {
        return Err(invalid_input(
            "/transport_tile_upper_bounds",
            "transport tile cap diagnosis requires at least one cap",
        ));
    }

    let maximum_transport_tiles = i64::from(fixed_width)
        .checked_mul(i64::from(fixed_height))
        .and_then(|area| area.checked_mul(2))
        .ok_or_else(|| invalid_input("/fixed_dimensions", "fixed dimensions overflow"))?;
    let mut caps = transport_tile_upper_bounds.to_vec();
    caps.sort_unstable();
    caps.dedup();
    if let Some(invalid) = caps
        .iter()
        .find(|cap| i64::from(**cap) > maximum_transport_tiles)
    {
        return Err(invalid_input(
            "/transport_tile_upper_bounds",
            format!(
                "transport tile cap {invalid} exceeds the two-layer physical maximum {maximum_transport_tiles}"
            ),
        ));
    }

    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success || target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            "facility growth planning failed or the target phase is out of range",
        ));
    }
    let prefix = sweep_cumulative_integrated_layout_fixed_dimensions(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        target_phase_index - 1,
        prefix_worker_count,
        prefix_search_budget,
    )?;
    let prior_solution = prefix.layout;
    if !prefix.completed_target_phase || !prior_solution.success {
        return Err(invalid_input(
            "/prefix",
            "preceding cumulative phase did not produce a validated hint",
        ));
    }
    let prefix_hint_dimensions = prior_solution
        .bounds
        .as_ref()
        .map(|bounds| [bounds.width, bounds.height]);
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
    let mut cases = Vec::with_capacity(caps.len() + 1);

    let baseline =
        exact::shared_layer::solve_factored_endpoints_fixed_dimensions_feasibility_only_with_prior(
            input.clone(),
            logistics_components,
            Some(case_search_budget),
            fixed_dimensions,
            Some(&prior_solution),
        );
    cases.push(case_report(None, baseline));

    for cap in caps {
        let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_transport_tile_cap_feasibility_only_with_prior(
            input.clone(),
            logistics_components,
            Some(case_search_budget),
            fixed_dimensions,
            i32::try_from(cap).map_err(|_| {
                invalid_input(
                    "/transport_tile_upper_bounds",
                    format!("transport tile cap {cap} does not fit the solver integer domain"),
                )
            })?,
            Some(&prior_solution),
        );
        cases.push(case_report(Some(cap), layout));
    }

    Ok(TransportTileCapDiagnosisReport {
        schema_version: TRANSPORT_TILE_CAP_DIAGNOSIS_SCHEMA_VERSION,
        target_phase_index,
        fixed_dimensions: ExactUsedDimensionCandidate {
            width: fixed_width,
            height: fixed_height,
            area: i64::from(fixed_width) * i64::from(fixed_height),
        },
        prefix_hint_dimensions,
        case_search_budget_ms: millis(case_search_budget),
        outer_wall_ms: millis(started.elapsed()),
        cases,
        diagnostic_only: true,
    })
}

fn case_report(
    transport_tile_upper_bound: Option<u32>,
    layout: IntegratedLayoutReport,
) -> TransportTileCapCaseReport {
    let outcome = super::coordinate_partition::classify_outcome(&layout);
    let exact = layout
        .exact
        .as_ref()
        .expect("executed transport tile cap case has exact metrics");
    TransportTileCapCaseReport {
        transport_tile_upper_bound,
        outcome,
        construction_ms: exact.construction_ms,
        search_ms: exact.search_ms,
        first_incumbent_ms: exact.first_incumbent_ms,
        model_scale: model_scale(exact),
        observed_objective: exact.objective,
        layout,
    }
}
