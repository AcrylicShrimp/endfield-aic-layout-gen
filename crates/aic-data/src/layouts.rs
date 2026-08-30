mod integrated;
mod placement;
mod ports;

pub use integrated::{
    IntegratedLayoutDiagnostic, IntegratedLayoutReport, IntegratedLayoutStatus, IntegratedRoute,
    IntegratedRouteEndpoint, solve_integrated_layout,
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
