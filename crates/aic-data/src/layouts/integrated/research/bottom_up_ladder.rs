use std::collections::BTreeSet;
use std::time::Duration;

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{
    IntegratedLayoutDiagnostic, IntegratedLayoutReport, exact, harness, prepare_exact_model,
};
use super::MAX_NEW_FACILITIES_PER_GROWTH_PHASE;

pub const BOTTOM_UP_FACILITY_GEOMETRY_EXPERIMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpFacilityGeometryExperimentReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub total_phase_count: usize,
    pub cumulative_facility_count: usize,
    pub search_budget_ms: u64,
    pub rung: exact::ladder::BottomUpRungReport,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_bottom_up_facility_geometry(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    search_budget: Duration,
) -> Result<BottomUpFacilityGeometryExperimentReport, IntegratedLayoutReport> {
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success {
        let diagnostic = growth.diagnostics.into_iter().next().map_or_else(
            || {
                IntegratedLayoutDiagnostic::error(
                    "bottom-up-growth-planning-failed",
                    "/",
                    None,
                    "bottom-up ladder could not plan the cumulative production graph",
                )
            },
            |diagnostic| {
                IntegratedLayoutDiagnostic::error(
                    "bottom-up-growth-planning-failed",
                    diagnostic.path,
                    diagnostic.entity,
                    diagnostic.message,
                )
            },
        );
        return Err(IntegratedLayoutReport::invalid(diagnostic));
    }
    let total_phase_count = growth.phases.len();
    if target_phase_index >= total_phase_count {
        return Err(IntegratedLayoutReport::invalid(
            IntegratedLayoutDiagnostic::error(
                "bottom-up-target-phase-out-of-range",
                "/target_phase_index",
                Some(target_phase_index.to_string()),
                format!(
                    "target phase {target_phase_index} is outside the cumulative SCC phase range 0..{total_phase_count}"
                ),
            ),
        ));
    }

    let total_facilities = growth
        .components
        .iter()
        .map(|component| component.facilities.len())
        .sum();
    let cumulative_facilities = growth
        .phases
        .iter()
        .take(target_phase_index + 1)
        .flat_map(|phase| phase.facilities.iter().cloned())
        .collect::<BTreeSet<_>>();
    let partial_wiring = harness::project_cumulative_wiring(
        instance_wiring,
        &cumulative_facilities,
        total_facilities,
    )
    .map_err(IntegratedLayoutReport::invalid)?;
    let input = prepare_exact_model(
        &partial_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let rung = exact::ladder::solve_facility_geometry_rung(input, search_budget);
    Ok(BottomUpFacilityGeometryExperimentReport {
        schema_version: BOTTOM_UP_FACILITY_GEOMETRY_EXPERIMENT_SCHEMA_VERSION,
        target_phase_index,
        total_phase_count,
        cumulative_facility_count: cumulative_facilities.len(),
        search_budget_ms: u64::try_from(search_budget.as_millis()).unwrap_or(u64::MAX),
        rung,
    })
}
