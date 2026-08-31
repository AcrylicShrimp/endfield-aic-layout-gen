mod growth;
mod integrated;
mod placement;
mod ports;

pub use growth::{
    FacilityGrowthComponent, FacilityGrowthDiagnostic, FacilityGrowthPhase,
    FacilityGrowthPlanReport, plan_facility_growth,
};

pub use integrated::{
    CUMULATIVE_SCC_GROWTH_SCHEMA_VERSION, CumulativeSccGrowthReport,
    EXACT_ABLATION_MATRIX_SCHEMA_VERSION, EXTERNAL_CONNECTOR_PORT_DOMAIN_SCHEMA_VERSION,
    EXTERNAL_CONNECTOR_SUBSET_SCHEMA_VERSION, ExactAblationCaseReport, ExactAblationFixation,
    ExactAblationMatrixReport, ExactModelMetrics, ExactObjectiveKind, ExactObjectiveStageReport,
    ExactObjectiveValue, ExactProofStatus, ExactSolveReport, ExactTerminationReason,
    ExactValidationStatus, ExternalConnectorPortDomainClassification,
    ExternalConnectorPortDomainReport, ExternalConnectorRequirementDescriptor,
    ExternalConnectorSubsetReport, FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION,
    FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION,
    FACTORED_REQUIREMENT_DECOMPOSITION_SCHEMA_VERSION, FactoredEndpointComparisonReport,
    FactoredNetworkDecompositionReport, FactoredNetworkSubsetCaseReport,
    FactoredRequirementDecompositionReport, FactoredRequirementSubsetCaseReport,
    INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic, IntegratedLayoutPhase,
    IntegratedLayoutReport, IntegratedLayoutStatus, PlacedLogisticsComponent,
    SHARED_LAYER_COMPARISON_SCHEMA_VERSION, SharedLayerComparisonReport, TransportNetwork,
    TransportNetworkEndpoint, TransportNetworkSegment, TransportNetworkTerminal,
    analyze_integrated_layout_search_space,
    compare_first_integrated_layout_phase_factored_endpoints,
    compare_first_integrated_layout_phase_shared_layer,
    decompose_first_integrated_layout_phase_factored_networks,
    decompose_first_integrated_layout_phase_factored_requirements,
    decompose_first_integrated_layout_phase_pair, render_integrated_layout_html,
    render_integrated_layout_html_with_localization, solve_cumulative_scc_growth_v2,
    solve_first_integrated_layout_phase_external_connector_port_domain,
    solve_first_integrated_layout_phase_external_connector_subset,
    solve_first_integrated_layout_phase_with_time_limit, solve_integrated_layout,
    solve_integrated_layout_with_time_limit,
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
