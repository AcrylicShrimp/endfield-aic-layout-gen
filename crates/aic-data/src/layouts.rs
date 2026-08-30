mod growth;
mod integrated;
mod placement;
mod ports;

pub use growth::{
    FacilityGrowthComponent, FacilityGrowthDiagnostic, FacilityGrowthPhase,
    FacilityGrowthPlanReport, plan_facility_growth,
};

pub use integrated::{
    CANDIDATE_POLICY_TABLE_SCHEMA_VERSION, CUMULATIVE_GRAPH_KEY_SCHEMA_VERSION, CandidateCounts,
    CandidatePolicy, CandidatePolicyTable, CandidateRank, CumulativeGraphFingerprint,
    CumulativeGraphKey, DeterministicCandidateKey, EndpointPortSelection, FacilityChangeCounts,
    FacilityGraphRecord, GridCellKey, INTEGRATED_LAYOUT_SCHEMA_VERSION,
    ITERATIVE_OPTIMIZATION_CONFIG_SCHEMA_VERSION, IncumbentExtensionCounts,
    IncumbentExtensionResult, IncumbentProvenance, IntegratedLayoutDiagnostic,
    IntegratedLayoutIncumbentSummary, IntegratedLayoutPhase, IntegratedLayoutPhaseAttempt,
    IntegratedLayoutPhaseOptimization, IntegratedLayoutReport, IntegratedLayoutStatus,
    IntegratedRoute, IntegratedRouteEndpoint, IterativeOptimizationConfig, LayoutScore,
    LayoutScoreDelta, OptimizationConfigDiagnostic, OptimizationProofStatus,
    OptimizationTerminationReason, PhaseElapsedMilliseconds, PhaseIncumbent,
    PlacedLogisticsComponent, PlacementPolicy, RefinementKind, RequirementGraphRecord,
    RetainedComponent, RetainedOccupant, RetainedRoutingResult, RetainedRoutingState,
    RouteChangeCounts, RouteRequirementFingerprint, RoutingConflict, RoutingOrderPolicy,
    SelectedPortAssignment, construct_coordinate_integrated_layout_with_time_limit,
    construct_iterative_scc_layout, construct_sparse_integrated_layout, extend_phase_incumbent,
    render_integrated_layout_html, render_integrated_layout_html_with_localization,
    reroute_integrated_layout_subset, solve_integrated_layout,
    solve_integrated_layout_with_time_limit, validate_candidate_policy_table,
    validate_iterative_optimization_config,
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
