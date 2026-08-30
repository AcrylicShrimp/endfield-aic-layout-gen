mod growth;
mod integrated;
mod placement;
mod ports;

pub use growth::{
    FacilityGrowthComponent, FacilityGrowthDiagnostic, FacilityGrowthPhase,
    FacilityGrowthPlanReport, plan_facility_growth,
};

pub use integrated::{
    BoundarySide, IntegratedLayoutDiagnostic, IntegratedLayoutPhase, IntegratedLayoutPhaseAttempt,
    IntegratedLayoutReport, IntegratedLayoutStatus, IntegratedRoute, IntegratedRouteEndpoint,
    PlacedLogisticsComponent, construct_coordinate_integrated_layout_with_time_limit,
    construct_iterative_scc_layout_with_time_limit, construct_sparse_integrated_layout,
    render_integrated_layout_html, solve_integrated_layout,
    solve_integrated_layout_with_time_limit,
};

pub use placement::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementDiagnostic,
    FacilityPlacementReport, FacilityPlacementRequest, FacilityPlacementStatus,
    SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION, solve_anchored_facility_placement_with_time_limit,
    solve_facility_placement, validate_facility_placement_request,
};
pub use ports::{
    FacilityPortProjectionDiagnostic, FacilityPortProjectionReport, PlacedFacilityPort,
    WorldGridPosition, project_facility_ports,
};
