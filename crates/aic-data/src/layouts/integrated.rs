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
pub use exact::shared_layer::{
    RootBooleanDomainCounts, RootDomainCardinality, RootDomainSnapshot,
    RootEndpointContinuationArcSnapshot, RootExternalGeometrySnapshot, RootFacilityStateSnapshot,
    RootFirstDecisionSnapshot, RootFlowDomainCounts, RootMaterialJunctionArcSnapshot,
    RootMaterialJunctionSnapshot, RootMaterialNetworkSnapshot, RootMaterialSeparatorArcSnapshot,
    RootMaterialSeparatorSnapshot, RootTerminalDomainSnapshot, RootTransportLayerSnapshot,
    RootVariableCoverageSnapshot, RootVariableFamilySnapshot,
};
use geometry::{candidate_port_connections, grid_index, world_position};
pub use html::{render_integrated_layout_html, render_integrated_layout_html_with_localization};
use model::{
    ComponentCapacityRates, EdgeInput, EndpointInput, InstanceInput, ModelInput, prepare_model,
    required_facility_area,
};
pub use report::{
    ExactModelMetrics, ExactObjectiveKind, ExactObjectiveStageReport, ExactObjectiveValue,
    ExactProofStatus, ExactSearchStatistics, ExactSolveReport, ExactTerminationReason,
    ExactValidationStatus, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutPhase, IntegratedLayoutReport, IntegratedLayoutStatus,
    PlacedLogisticsComponent, TransportNetwork, TransportNetworkEndpoint, TransportNetworkSegment,
    TransportNetworkTerminal,
};
pub use research::{
    BOUNDARY_CELL_WIDTH_SENSITIVITY_SCHEMA_VERSION, BoundaryCellWidthCaseReport,
    BoundaryCellWidthSensitivityReport, CONNECTIVITY_WITNESS_DIAGNOSIS_SCHEMA_VERSION,
    CUMULATIVE_EXACT_DIMENSION_SWEEP_SCHEMA_VERSION,
    CUMULATIVE_FACILITY_COORDINATE_PARTITION_SCHEMA_VERSION,
    CUMULATIVE_FACILITY_PORT_PARTITION_SCHEMA_VERSION,
    CUMULATIVE_FACILITY_ROTATION_PARTITION_SCHEMA_VERSION,
    CUMULATIVE_FACILITY_STATE_PARTITION_SCHEMA_VERSION, CUMULATIVE_SCC_GROWTH_SCHEMA_VERSION,
    ConnectivityWitnessCaseKind, ConnectivityWitnessCaseReport, ConnectivityWitnessDiagnosisReport,
    ConnectivityWitnessStateScale, CumulativeExactDimensionSweepReport,
    CumulativeFacilityCoordinatePartitionReport, CumulativeFacilityPortPartitionReport,
    CumulativeFacilityRotationPartitionReport, CumulativeFacilityStatePartitionReport,
    CumulativeSccGrowthReport, DiagnosticSearchMode,
    ENDPOINT_CONTINUATION_PARTITION_SCHEMA_VERSION, ENDPOINT_SOURCE_ONLY_CONTROL_SCHEMA_VERSION,
    EXACT_ABLATION_MATRIX_SCHEMA_VERSION, EXACT_DIMENSION_PARTITION_SCHEMA_VERSION,
    EXTERNAL_BOUNDARY_CELL_PARTITION_SCHEMA_VERSION,
    EXTERNAL_BOUNDARY_KEY_LEGAL_SUPPORT_AB_SCHEMA_VERSION,
    EXTERNAL_BOUNDARY_SIDE_PARTITION_SCHEMA_VERSION, EXTERNAL_CONNECTOR_PORT_DOMAIN_SCHEMA_VERSION,
    EXTERNAL_CONNECTOR_SUBSET_SCHEMA_VERSION, EndpointContinuationCandidate,
    EndpointContinuationCaseReport, EndpointContinuationPartitionReport,
    EndpointSourceOnlyCaseReport, EndpointSourceOnlyControlReport, EndpointSourceRegionEvidence,
    ExactAblationCaseReport, ExactAblationFixation, ExactAblationMatrixReport,
    ExactDimensionCaseDisposition, ExactDimensionCaseOutcome, ExactDimensionLowerBoundsReport,
    ExactDimensionPartitionCaseReport, ExactDimensionSolverStack,
    ExactDimensionUpperBoundImprovement, ExactUsedDimensionCandidate,
    ExternalBoundaryCellCaseReport, ExternalBoundaryCellPartitionReport,
    ExternalBoundaryKeyCommonModelContract, ExternalBoundaryKeyLegalSupportAbReport,
    ExternalBoundaryKeyNetworkContract, ExternalBoundaryKeyRootComparison,
    ExternalBoundaryKeyRootTotals, ExternalBoundaryKeySolveReport,
    ExternalBoundaryKeyStaticCertificate, ExternalBoundarySideCaseReport,
    ExternalBoundarySideDomain, ExternalBoundarySidePartitionReport,
    ExternalConnectorPortDomainClassification, ExternalConnectorPortDomainReport,
    ExternalConnectorRequirementDescriptor, ExternalConnectorSubsetReport,
    FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION, FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION,
    FACTORED_REQUIREMENT_DECOMPOSITION_SCHEMA_VERSION, FacilityCoordinateCaseDisposition,
    FacilityCoordinateCaseReport, FacilityPortAssignment, FacilityPortDomainReport,
    FacilityPortPartitionCaseReport, FacilityRotationPartitionCaseReport,
    FacilityStatePartitionCaseReport, FactoredEndpointComparisonReport,
    FactoredNetworkDecompositionReport, FactoredNetworkSubsetCaseReport,
    FactoredRequirementDecompositionReport, FactoredRequirementSubsetCaseReport,
    GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION, GuardedCoreAcceptedFixture,
    GuardedCoreInitialGateReport, GuardedCoreInitialGateStatus, GuardedCoreSequentialShrinkReport,
    GuardedCoreSequentialShrinkStatus, GuardedCoreShrinkAttempt,
    GuardedItemIntersectionObservation, LayerGridAnalyzerRuntime,
    MATERIAL_JUNCTION_CONTINUATION_SCHEMA_VERSION, MATERIAL_ROW5_SEPARATOR_SCHEMA_VERSION,
    MATERIAL_SEPARATOR_CUT_SCHEMA_VERSION, MaterialJunctionCaseReport,
    MaterialJunctionContinuationReport, MaterialRow5SeparatorCaseReport,
    MaterialRow5SeparatorReport, MaterialSeparatorCaseReport, MaterialSeparatorCutReport,
    PARALLEL_EXACT_DIMENSION_SWEEP_SCHEMA_VERSION, PHASE2_REFERENCE_ABLATION_SCHEMA_VERSION,
    POSSIBLE_GRAPH_CONNECTIVITY_DIAGNOSIS_SCHEMA_VERSION,
    PRIOR_INPUT_PAIR_ROOT_SNAPSHOT_SCHEMA_VERSION, PRIOR_INPUT_PORT_CONTROLS_SCHEMA_VERSION,
    PRIOR_INPUT_PORT_PAIR_PORTFOLIO_SCHEMA_VERSION, PRIOR_PORT_SUBSET_ABLATION_SCHEMA_VERSION,
    PRIOR_SOURCE_PORT_PORTFOLIO_SCHEMA_VERSION, PRIOR_TERMINAL_COMPLETION_PORTFOLIO_SCHEMA_VERSION,
    PRIOR_TERMINAL_PAIR_VALUE_PORTFOLIO_SCHEMA_VERSION, ParallelExactDimensionCaseReport,
    ParallelExactDimensionSweepReport, PartitionCaseModelScale, Phase2ReferenceAblationCaseReport,
    Phase2ReferenceAblationKind, Phase2ReferenceAblationReport, PossibleGraphConnectivityCaseKind,
    PossibleGraphConnectivityCaseReport, PossibleGraphConnectivityDiagnosisReport,
    PossibleGraphConnectivityRuntime, PossibleGraphConnectivityScale,
    PriorInputPairRootSnapshotReport, PriorInputPortControlCaseReport,
    PriorInputPortControlSuiteReport, PriorInputPortControlsReport, PriorInputPortPairCaseReport,
    PriorInputPortPairPortfolioReport, PriorInputPortProofExclusion, PriorInputPortResidualDomain,
    PriorPortSubsetAblationReport, PriorPortSubsetCaseReport, PriorPortSubsetFacility,
    PriorSourcePortCaseReport, PriorSourcePortParentReport, PriorSourcePortPortfolioReport,
    PriorTerminalCompletionCaseReport, PriorTerminalCompletionDomain,
    PriorTerminalCompletionParentReport, PriorTerminalCompletionPortfolioReport,
    PriorTerminalPairDomain, PriorTerminalPairValueCaseReport,
    PriorTerminalPairValuePortfolioReport, PriorTerminalSubsetPartition,
    PriorTerminalSubsetTerminal, RESIDUAL_FACILITY_PORT_TUPLE_PORTFOLIO_SCHEMA_VERSION,
    RESIDUAL_FACILITY_STATE_ABLATION_SCHEMA_VERSION, ROUTE_CELL_BREAKDOWN_SCHEMA_VERSION,
    ROUTING_STATE_BREAKDOWN_SCHEMA_VERSION, ResidualFacilityPortDomain,
    ResidualFacilityPortFixationObservation, ResidualFacilityPortTupleCaseReport,
    ResidualFacilityPortTuplePortfolioReport, ResidualFacilityStateAblationReport,
    ResidualFacilityStateCaseKind, ResidualFacilityStateCaseReport, RouteCellBreakdownCaseReport,
    RouteCellBreakdownReport, RouteCellLayerScope, RouteCellValueScope,
    RoutingStateBreakdownCaseReport, RoutingStateBreakdownReport, RoutingStateFamily,
    RoutingStateMatrixKind, SEARCH_MODE_DIAGNOSIS_SCHEMA_VERSION,
    SHARED_LAYER_COMPARISON_SCHEMA_VERSION, SearchModeDiagnosisCaseReport,
    SharedLayerComparisonReport, TRANSPORT_TILE_CAP_DIAGNOSIS_SCHEMA_VERSION,
    TransportTileCapCaseReport, TransportTileCapDiagnosisReport,
    compare_first_integrated_layout_phase_factored_endpoints,
    compare_first_integrated_layout_phase_shared_layer,
    decompose_first_integrated_layout_phase_factored_networks,
    decompose_first_integrated_layout_phase_factored_requirements,
    decompose_first_integrated_layout_phase_pair, diagnose_boundary_cell_width_sensitivity,
    diagnose_cumulative_facility_coordinate_partitions,
    diagnose_cumulative_facility_coordinate_partitions_with_local_continuation,
    diagnose_cumulative_facility_port_partitions,
    diagnose_cumulative_facility_port_partitions_with_local_continuation,
    diagnose_cumulative_facility_rotation_partitions,
    diagnose_cumulative_facility_rotation_partitions_with_local_continuation,
    diagnose_cumulative_facility_state_partitions_with_local_continuation,
    diagnose_cumulative_facility_state_partitions_with_prior_overlap_facility_state,
    diagnose_cumulative_transport_tile_caps, diagnose_endpoint_continuation_partition,
    diagnose_endpoint_source_only_control, diagnose_external_boundary_cell_partition,
    diagnose_external_boundary_key_legal_support_ab, diagnose_external_boundary_side_partition,
    diagnose_guarded_core_initial_gate, diagnose_guarded_core_sequential_shrinking,
    diagnose_material_junction_continuation, diagnose_material_row5_separator,
    diagnose_material_separator_cut, diagnose_phase2_connectivity_witness,
    diagnose_phase2_possible_graph_connectivity, diagnose_phase2_reference_ablation,
    diagnose_phase2_route_cell_breakdown, diagnose_phase2_routing_state_breakdown,
    diagnose_prior_input_pair_root_snapshot, diagnose_prior_input_port_controls,
    diagnose_prior_input_port_pair_portfolio, diagnose_prior_port_subset_ablation,
    diagnose_prior_source_port_portfolio, diagnose_prior_terminal_completion_portfolio,
    diagnose_prior_terminal_pair_value_portfolio, diagnose_residual_facility_port_tuple_portfolio,
    diagnose_residual_facility_state_ablation, solve_cumulative_scc_growth_v2,
    solve_first_integrated_layout_phase_external_connector_port_domain,
    solve_first_integrated_layout_phase_external_connector_subset,
    solve_first_integrated_layout_phase_fixed_dimensions,
    solve_first_integrated_layout_phase_search_mode,
    sweep_cumulative_integrated_layout_fixed_dimensions,
    sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation,
    sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation_guarded_intersection_observation,
    sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation_guarded_intersection_propagation,
    sweep_first_integrated_layout_phase_fixed_dimensions,
};
pub use research::{
    ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION, EndpointChannelCaseReport,
    EndpointChannelDomainSnapshot, EndpointChannelEncoding, EndpointChannelEndpointSnapshot,
    EndpointChannelProbeReport, EndpointChannelRestriction, render_endpoint_channel_probe_html,
    run_endpoint_channel_probe,
};
pub use research::{
    INTEGRATED_ENDPOINT_CHANNEL_COMPARISON_SCHEMA_VERSION, IntegratedEndpointChannelCaseReport,
    IntegratedEndpointTableScale, run_integrated_endpoint_channel_case,
};
pub use research::{
    PHYSICAL_OCCUPANCY_PROBE_SCHEMA_VERSION, PhysicalOccupancyCaseReport,
    PhysicalOccupancyDomainSnapshot, PhysicalOccupancyEncoding, PhysicalOccupancyProbeReport,
    PhysicalOccupancyRestriction, render_physical_occupancy_probe_html,
    run_physical_occupancy_probe,
};
pub use research::{
    SCALED_ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION, ScaledEndpointChannelProbeReport,
    ScaledEndpointDomainSnapshot, ScaledEndpointRestrictionReport,
    ScaledEndpointTerminalDomainSnapshot, ScaledEndpointTerminalScale,
    render_scaled_endpoint_channel_probe_html, run_scaled_endpoint_channel_probe,
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
        Ok(input) => exact::shared_layer::solve_factored_endpoints_with_prior(
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
        .transport_networks
        .iter()
        .flat_map(|network| network.cells.iter())
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
        .transport_networks
        .iter_mut()
        .flat_map(|network| network.cells.iter_mut())
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
