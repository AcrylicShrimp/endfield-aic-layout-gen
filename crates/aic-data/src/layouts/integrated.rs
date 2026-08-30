use std::time::Duration;

use crate::facilities::{FacilityPortEdge, ValidatedFacilityCatalog};
use crate::layouts::{FacilityPlacement, FacilityPlacementBounds, FacilityPlacementRequest};
use crate::logistics::{
    TransportKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::WorldGridPosition;

mod exact;
mod geometry;
mod html;
mod identity;
mod model;
mod networks;
mod report;
mod score;
mod witness;

use exact::solve;
use geometry::{candidate_port_connections, grid_index, world_position};
pub use html::{render_integrated_layout_html, render_integrated_layout_html_with_localization};
use model::{
    EdgeInput, EndpointInput, InstanceInput, ModelInput, prepare_model, required_facility_area,
};
pub use report::{
    ExactModelMetrics, ExactProofStatus, ExactSolveReport, ExactTerminationReason,
    ExactValidationStatus, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutPhase, IntegratedLayoutPhaseAttempt, IntegratedLayoutPhaseOptimization,
    IntegratedLayoutReport, IntegratedLayoutStatus, IntegratedRoute, IntegratedRouteEndpoint,
    PlacedLogisticsComponent, RouteRequirementFingerprint,
};
pub use score::{DeterministicCandidateKey, LayoutScore};

pub fn solve_integrated_layout(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        None,
    )
}

pub fn solve_integrated_layout_with_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        Some(time_limit),
    )
}

fn solve_integrated_layout_with_optional_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    match prepare_model(instance_wiring, facilities, items, transports, request) {
        Ok(input) => match required_facility_area(&input) {
            Ok(required_area) => {
                let available_area = i64::from(input.width) * i64::from(input.height);
                if required_area > available_area {
                    IntegratedLayoutReport::failure(
                        IntegratedLayoutStatus::Infeasible,
                        IntegratedLayoutDiagnostic::error(
                            "facility-area-exceeds-layout-bounds",
                            "/",
                            None,
                            format!(
                                "facility footprints require at least {required_area} cells but hard layout bounds provide {available_area} cells"
                            ),
                        ),
                    )
                } else {
                    solve(input, logistics_components, time_limit)
                }
            }
            Err(diagnostic) => {
                IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
            }
        },
        Err(diagnostic) => {
            IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
        }
    }
}

pub(super) fn canonicalize_report_geometry(report: &mut IntegratedLayoutReport) {
    let mut minimum_x = i64::MAX;
    let mut minimum_y = i64::MAX;
    for placement in &report.placements {
        minimum_x = minimum_x.min(placement.x);
        minimum_y = minimum_y.min(placement.y);
    }
    for position in report
        .routes
        .iter()
        .flat_map(|route| route.cells.iter())
        .chain(
            report
                .logistics_components
                .iter()
                .map(|component| &component.position),
        )
    {
        minimum_x = minimum_x.min(position.x);
        minimum_y = minimum_y.min(position.y);
    }
    if minimum_x == i64::MAX {
        report.bounds = Some(FacilityPlacementBounds {
            width: 0,
            height: 0,
        });
        return;
    }
    for placement in &mut report.placements {
        placement.x -= minimum_x;
        placement.y -= minimum_y;
    }
    for position in report
        .routes
        .iter_mut()
        .flat_map(|route| route.cells.iter_mut())
        .chain(
            report
                .logistics_components
                .iter_mut()
                .map(|component| &mut component.position),
        )
    {
        position.x -= minimum_x;
        position.y -= minimum_y;
    }
    let width = report
        .placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .chain(
            report
                .routes
                .iter()
                .flat_map(|route| route.cells.iter().map(|cell| cell.x + 1)),
        )
        .chain(
            report
                .logistics_components
                .iter()
                .map(|component| component.position.x + 1),
        )
        .max()
        .unwrap_or(0);
    let height = report
        .placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .chain(
            report
                .routes
                .iter()
                .flat_map(|route| route.cells.iter().map(|cell| cell.y + 1)),
        )
        .chain(
            report
                .logistics_components
                .iter()
                .map(|component| component.position.y + 1),
        )
        .max()
        .unwrap_or(0);
    report.bounds = Some(FacilityPlacementBounds { width, height });
}

fn route_turn_count(route: &IntegratedRoute) -> usize {
    route
        .cells
        .windows(3)
        .filter(|cells| {
            let first_dx = cells[1].x - cells[0].x;
            let first_dy = cells[1].y - cells[0].y;
            let second_dx = cells[2].x - cells[1].x;
            let second_dy = cells[2].y - cells[1].y;
            (first_dx, first_dy) != (second_dx, second_dy)
        })
        .count()
}

#[cfg(test)]
mod tests;
