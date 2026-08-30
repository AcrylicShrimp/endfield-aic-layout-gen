mod growth;
mod integrated;
mod placement;
mod ports;

pub use growth::{
    FacilityGrowthComponent, FacilityGrowthDiagnostic, FacilityGrowthPhase,
    FacilityGrowthPlanReport, plan_facility_growth,
};

pub use integrated::{
    ExactModelMetrics, ExactProofStatus, ExactSolveReport, ExactTerminationReason,
    ExactValidationStatus, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutPhase, IntegratedLayoutPhaseAttempt, IntegratedLayoutPhaseOptimization,
    IntegratedLayoutReport, IntegratedLayoutStatus, IntegratedRoute, IntegratedRouteEndpoint,
    PlacedLogisticsComponent, RouteRequirementFingerprint, render_integrated_layout_html,
    render_integrated_layout_html_with_localization, solve_integrated_layout,
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
