mod growth;
mod integrated;
mod placement;
mod ports;

pub use growth::{
    FacilityGrowthComponent, FacilityGrowthDiagnostic, FacilityGrowthPhase,
    FacilityGrowthPlanReport, plan_facility_growth,
};

pub use integrated::{
    CUMULATIVE_EXACT_DIMENSION_SWEEP_SCHEMA_VERSION,
    CUMULATIVE_FACILITY_COORDINATE_PARTITION_SCHEMA_VERSION,
    CUMULATIVE_FACILITY_PORT_PARTITION_SCHEMA_VERSION,
    CUMULATIVE_FACILITY_ROTATION_PARTITION_SCHEMA_VERSION, CUMULATIVE_SCC_GROWTH_SCHEMA_VERSION,
    CumulativeExactDimensionSweepReport, CumulativeFacilityCoordinatePartitionReport,
    CumulativeFacilityPortPartitionReport, CumulativeFacilityRotationPartitionReport,
    CumulativeSccGrowthReport, DiagnosticSearchMode, EXACT_ABLATION_MATRIX_SCHEMA_VERSION,
    EXACT_DIMENSION_PARTITION_SCHEMA_VERSION, EXTERNAL_CONNECTOR_PORT_DOMAIN_SCHEMA_VERSION,
    EXTERNAL_CONNECTOR_SUBSET_SCHEMA_VERSION, ExactAblationCaseReport, ExactAblationFixation,
    ExactAblationMatrixReport, ExactDimensionCaseDisposition, ExactDimensionCaseOutcome,
    ExactDimensionLowerBoundsReport, ExactDimensionPartitionCaseReport,
    ExactDimensionUpperBoundImprovement, ExactModelMetrics, ExactObjectiveKind,
    ExactObjectiveStageReport, ExactObjectiveValue, ExactProofStatus, ExactSolveReport,
    ExactTerminationReason, ExactUsedDimensionCandidate, ExactValidationStatus,
    ExternalConnectorPortDomainClassification, ExternalConnectorPortDomainReport,
    ExternalConnectorRequirementDescriptor, ExternalConnectorSubsetReport,
    FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION, FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION,
    FACTORED_REQUIREMENT_DECOMPOSITION_SCHEMA_VERSION, FacilityCoordinateCaseDisposition,
    FacilityCoordinateCaseReport, FacilityPortAssignment, FacilityPortDomainReport,
    FacilityPortPartitionCaseReport, FacilityRotationPartitionCaseReport,
    FactoredEndpointComparisonReport, FactoredNetworkDecompositionReport,
    FactoredNetworkSubsetCaseReport, FactoredRequirementDecompositionReport,
    FactoredRequirementSubsetCaseReport, INTEGRATED_LAYOUT_SCHEMA_VERSION,
    IntegratedLayoutDiagnostic, IntegratedLayoutPhase, IntegratedLayoutReport,
    IntegratedLayoutStatus, PARALLEL_EXACT_DIMENSION_SWEEP_SCHEMA_VERSION,
    PHASE2_REFERENCE_ABLATION_SCHEMA_VERSION, PHYSICAL_OCCUPANCY_PROBE_SCHEMA_VERSION,
    ParallelExactDimensionCaseReport, ParallelExactDimensionSweepReport, PartitionCaseModelScale,
    Phase2ReferenceAblationCaseReport, Phase2ReferenceAblationKind, Phase2ReferenceAblationReport,
    PhysicalOccupancyCaseReport, PhysicalOccupancyDomainSnapshot, PhysicalOccupancyEncoding,
    PhysicalOccupancyProbeReport, PhysicalOccupancyRestriction, PlacedLogisticsComponent,
    SEARCH_MODE_DIAGNOSIS_SCHEMA_VERSION, SHARED_LAYER_COMPARISON_SCHEMA_VERSION,
    SearchModeDiagnosisCaseReport, SharedLayerComparisonReport,
    TRANSPORT_TILE_CAP_DIAGNOSIS_SCHEMA_VERSION, TransportNetwork, TransportNetworkEndpoint,
    TransportNetworkSegment, TransportNetworkTerminal, TransportTileCapCaseReport,
    TransportTileCapDiagnosisReport, analyze_integrated_layout_search_space,
    compare_first_integrated_layout_phase_factored_endpoints,
    compare_first_integrated_layout_phase_shared_layer,
    decompose_first_integrated_layout_phase_factored_networks,
    decompose_first_integrated_layout_phase_factored_requirements,
    decompose_first_integrated_layout_phase_pair,
    diagnose_cumulative_facility_coordinate_partitions,
    diagnose_cumulative_facility_port_partitions, diagnose_cumulative_facility_rotation_partitions,
    diagnose_cumulative_transport_tile_caps, diagnose_phase2_reference_ablation,
    render_integrated_layout_html, render_integrated_layout_html_with_localization,
    render_physical_occupancy_probe_html, run_physical_occupancy_probe,
    solve_cumulative_scc_growth_v2,
    solve_first_integrated_layout_phase_external_connector_port_domain,
    solve_first_integrated_layout_phase_external_connector_subset,
    solve_first_integrated_layout_phase_fixed_dimensions,
    solve_first_integrated_layout_phase_search_mode,
    solve_first_integrated_layout_phase_with_time_limit, solve_integrated_layout,
    solve_integrated_layout_with_time_limit, sweep_cumulative_integrated_layout_fixed_dimensions,
    sweep_first_integrated_layout_phase_fixed_dimensions,
};

pub use placement::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementDiagnostic,
    FacilityPlacementReport, FacilityPlacementRequest, FacilityPlacementStatus,
    SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION, solve_facility_placement,
    validate_facility_placement_request,
};
pub use ports::{
    FacilityPortProjectionDiagnostic, FacilityPortProjectionReport, PlacedFacilityPort,
    WorldGridPosition, project_facility_ports,
};
