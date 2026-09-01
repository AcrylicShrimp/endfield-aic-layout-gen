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

pub const BOTTOM_UP_EXPERIMENT_SCHEMA_VERSION: u32 = 8;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpExperimentReport {
    pub schema_version: u32,
    pub workload_id: Option<String>,
    pub workload_manifest_sha256: Option<String>,
    pub target_phase_index: usize,
    pub total_phase_count: usize,
    pub cumulative_facility_count: usize,
    pub introduced_facility_ids: Vec<String>,
    pub search_budget_ms: u64,
    pub rung: exact::ladder::BottomUpRungReport,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_bottom_up_rung(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    rung_kind: exact::ladder::BottomUpRungKind,
    endpoint_clearance_priority: exact::ladder::EndpointClearanceSchedulingPriority,
    endpoint_clearance_counters_enabled: bool,
    endpoint_clearance_false_event_filter_enabled: bool,
    target_phase_index: usize,
    search_budget: Duration,
) -> Result<BottomUpExperimentReport, IntegratedLayoutReport> {
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
    let introduced_facility_ids = growth.phases[target_phase_index].facilities.clone();
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
    let rung = match rung_kind {
        exact::ladder::BottomUpRungKind::FacilityGeometry => {
            exact::ladder::solve_facility_geometry_rung(input, search_budget)
        }
        exact::ladder::BottomUpRungKind::FacilityPortGeometry => {
            exact::ladder::solve_facility_port_geometry_rung(input, search_budget)
        }
        exact::ladder::BottomUpRungKind::FacilityPorts => {
            exact::ladder::solve_facility_ports_rung(input, search_budget)
        }
        exact::ladder::BottomUpRungKind::FacilityPortsPropagated => {
            exact::ladder::solve_facility_ports_propagated_rung(
                input,
                search_budget,
                endpoint_clearance_priority,
                endpoint_clearance_counters_enabled,
                endpoint_clearance_false_event_filter_enabled,
            )
        }
    };
    Ok(BottomUpExperimentReport {
        schema_version: BOTTOM_UP_EXPERIMENT_SCHEMA_VERSION,
        workload_id: None,
        workload_manifest_sha256: None,
        target_phase_index,
        total_phase_count,
        cumulative_facility_count: cumulative_facilities.len(),
        introduced_facility_ids,
        search_budget_ms: u64::try_from(search_budget.as_millis()).unwrap_or(u64::MAX),
        rung,
    })
}
