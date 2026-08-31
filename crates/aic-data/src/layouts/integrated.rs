use std::time::Duration;

use crate::facilities::{FacilityPortEdge, ValidatedFacilityCatalog};
use crate::layouts::{FacilityPlacement, FacilityPlacementBounds, FacilityPlacementRequest};
use crate::logistics::{
    TransportKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::WorldGridPosition;

mod analysis;
mod exact;
mod geometry;
mod harness;
mod html;
mod identity;
mod model;
mod networks;
mod report;
mod research;
mod score;
mod witness;

pub use analysis::analyze_integrated_layout_search_space;
use geometry::{candidate_port_connections, grid_index, world_position};
pub use html::{render_integrated_layout_html, render_integrated_layout_html_with_localization};
use model::{
    ComponentCapacityRates, EdgeInput, EndpointInput, InstanceInput, ModelInput, prepare_model,
    required_facility_area,
};
pub use report::{
    ExactModelMetrics, ExactObjectiveKind, ExactObjectiveStageReport, ExactObjectiveValue,
    ExactProofStatus, ExactSolveReport, ExactTerminationReason, ExactValidationStatus,
    ExternalBoundaryConnector, ExternalConnectorTemplate, INTEGRATED_LAYOUT_SCHEMA_VERSION,
    IntegratedLayoutDiagnostic, IntegratedLayoutPhase, IntegratedLayoutReport,
    IntegratedLayoutStatus, PlacedLogisticsComponent, TransportNetwork, TransportNetworkEndpoint,
    TransportNetworkSegment, TransportNetworkTerminal,
};
pub use research::{
    CUMULATIVE_SCC_GROWTH_SCHEMA_VERSION, CumulativeSccGrowthReport,
    EXACT_ABLATION_MATRIX_SCHEMA_VERSION, EXTERNAL_CONNECTOR_PORT_DOMAIN_SCHEMA_VERSION,
    EXTERNAL_CONNECTOR_SUBSET_SCHEMA_VERSION, ExactAblationCaseReport, ExactAblationFixation,
    ExactAblationMatrixReport, ExternalConnectorPortDomainClassification,
    ExternalConnectorPortDomainReport, ExternalConnectorRequirementDescriptor,
    ExternalConnectorSubsetReport, FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION,
    FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION,
    FACTORED_REQUIREMENT_DECOMPOSITION_SCHEMA_VERSION, FactoredEndpointComparisonReport,
    FactoredNetworkDecompositionReport, FactoredNetworkSubsetCaseReport,
    FactoredRequirementDecompositionReport, FactoredRequirementSubsetCaseReport,
    SHARED_LAYER_COMPARISON_SCHEMA_VERSION, SharedLayerComparisonReport,
    compare_first_integrated_layout_phase_factored_endpoints,
    compare_first_integrated_layout_phase_shared_layer,
    decompose_first_integrated_layout_phase_factored_networks,
    decompose_first_integrated_layout_phase_factored_requirements,
    decompose_first_integrated_layout_phase_pair, solve_cumulative_scc_growth_v2,
    solve_first_integrated_layout_phase_external_connector_port_domain,
    solve_first_integrated_layout_phase_external_connector_subset,
};
pub use score::LayoutScore;

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

#[allow(clippy::too_many_arguments)]
pub fn solve_first_integrated_layout_phase_with_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    harness::solve_first_iterative_scc_phase(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        time_limit,
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
    harness::solve_iterative_scc(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        time_limit,
    )
}

fn solve_exact_model(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Option<Duration>,
    prior_solution: Option<&IntegratedLayoutReport>,
) -> IntegratedLayoutReport {
    match prepare_exact_model(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    ) {
        Ok(input) => exact::solve_with_prior_solution(
            input,
            logistics_components,
            time_limit,
            prior_solution,
        ),
        Err(report) => report,
    }
}

fn prepare_exact_model(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
) -> Result<ModelInput, IntegratedLayoutReport> {
    let input = prepare_model(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )
    .map_err(|diagnostic| {
        IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
    })?;
    let required_area = required_facility_area(&input).map_err(|diagnostic| {
        IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
    })?;
    let available_area = i64::from(input.width) * i64::from(input.height);
    if required_area > available_area {
        return Err(IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Infeasible,
            IntegratedLayoutDiagnostic::error(
                "facility-area-exceeds-layout-bounds",
                "/",
                None,
                format!(
                    "facility footprints require at least {required_area} cells but hard layout bounds provide {available_area} cells"
                ),
            ),
        ));
    }
    Ok(input)
}

pub(super) fn canonicalize_report_geometry(report: &mut IntegratedLayoutReport) {
    let mut minimum_x = i64::MAX;
    let mut minimum_y = i64::MAX;
    for placement in &report.placements {
        minimum_x = minimum_x.min(placement.x);
        minimum_y = minimum_y.min(placement.y);
    }
    for position in report
        .external_connectors
        .iter()
        .flat_map(|connector| connector.cells.iter())
        .chain(
            report
                .transport_networks
                .iter()
                .flat_map(|network| network.cells.iter()),
        )
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
        .external_connectors
        .iter_mut()
        .flat_map(|connector| connector.cells.iter_mut())
        .chain(
            report
                .transport_networks
                .iter_mut()
                .flat_map(|network| network.cells.iter_mut()),
        )
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
    for connector in &mut report.external_connectors {
        if let Some(turn) = &mut connector.turn {
            turn.x -= minimum_x;
            turn.y -= minimum_y;
        }
        connector.exit.x -= minimum_x;
        connector.exit.y -= minimum_y;
    }
    for network in &mut report.transport_networks {
        for segment in &mut network.segments {
            segment.from.x -= minimum_x;
            segment.from.y -= minimum_y;
            segment.to.x -= minimum_x;
            segment.to.y -= minimum_y;
        }
        for terminal in &mut network.terminals {
            terminal.position.x -= minimum_x;
            terminal.position.y -= minimum_y;
        }
    }
    let width = report
        .placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .chain(
            report
                .external_connectors
                .iter()
                .flat_map(|connector| connector.cells.iter().map(|cell| cell.x + 1)),
        )
        .chain(
            report
                .transport_networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.x + 1)),
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
                .external_connectors
                .iter()
                .flat_map(|connector| connector.cells.iter().map(|cell| cell.y + 1)),
        )
        .chain(
            report
                .transport_networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.y + 1)),
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

#[cfg(test)]
mod tests;
