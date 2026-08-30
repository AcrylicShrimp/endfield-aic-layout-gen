use std::collections::BTreeSet;

use serde::Serialize;

use crate::facilities::{
    FacilityPortDirection, FacilityPortEdge, FacilityPortPosition, ValidatedFacilityCatalog,
};
use crate::logistics::TransportKind;

use super::{
    FacilityPlacement, FacilityPlacementReport, FacilityPlacementRequest,
    validate_facility_placement_request,
};

const STAGE: &str = "facility-port-projection";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPortProjectionReport {
    pub success: bool,
    pub ports: Vec<PlacedFacilityPort>,
    pub diagnostics: Vec<FacilityPortProjectionDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlacedFacilityPort {
    pub instance: String,
    pub facility: String,
    pub port: String,
    pub direction: FacilityPortDirection,
    pub transport: TransportKind,
    pub position: WorldGridPosition,
    pub edge: FacilityPortEdge,
    pub connection: WorldGridPosition,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorldGridPosition {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPortProjectionDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl FacilityPortProjectionDiagnostic {
    fn error(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "error",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }
}

pub fn project_facility_ports(
    placement_report: &FacilityPlacementReport,
    catalog: &ValidatedFacilityCatalog,
    request: &FacilityPlacementRequest,
) -> FacilityPortProjectionReport {
    if !placement_report.success {
        return failure(vec![FacilityPortProjectionDiagnostic::error(
            "upstream-facility-placement-failed",
            "/",
            None,
            "facility port projection requires a successful facility placement report",
        )]);
    }

    if !validate_facility_placement_request(request).is_empty() {
        return failure(vec![FacilityPortProjectionDiagnostic::error(
            "invalid-facility-placement-request",
            "/request",
            None,
            "facility port projection requires a valid facility placement request",
        )]);
    }

    let mut seen_instances = BTreeSet::new();
    let mut ports = Vec::new();
    let mut diagnostics = Vec::new();

    for (placement_index, placement) in placement_report.placements.iter().enumerate() {
        if !seen_instances.insert(placement.instance.as_str()) {
            diagnostics.push(FacilityPortProjectionDiagnostic::error(
                "duplicate-placed-facility-instance",
                format!("/placements/{placement_index}/instance"),
                Some(placement.instance.clone()),
                format!(
                    "placed facility instance '{}' appears more than once",
                    placement.instance
                ),
            ));
            continue;
        }

        project_placement_ports(
            placement,
            placement_index,
            catalog,
            request,
            &mut ports,
            &mut diagnostics,
        );
    }

    if !diagnostics.is_empty() {
        return failure(diagnostics);
    }

    ports.sort_by(|left, right| {
        left.instance
            .cmp(&right.instance)
            .then_with(|| left.port.cmp(&right.port))
    });

    FacilityPortProjectionReport {
        success: true,
        ports,
        diagnostics,
    }
}

fn project_placement_ports(
    placement: &FacilityPlacement,
    placement_index: usize,
    catalog: &ValidatedFacilityCatalog,
    request: &FacilityPlacementRequest,
    ports: &mut Vec<PlacedFacilityPort>,
    diagnostics: &mut Vec<FacilityPortProjectionDiagnostic>,
) {
    let path = format!("/placements/{placement_index}");
    let Some(definition) = catalog.facility(&placement.facility) else {
        diagnostics.push(FacilityPortProjectionDiagnostic::error(
            "missing-facility-definition",
            format!("{path}/facility"),
            Some(placement.instance.clone()),
            format!(
                "placed facility instance '{}' references missing facility '{}'",
                placement.instance, placement.facility
            ),
        ));
        return;
    };

    let Some((expected_width, expected_height)) = rotated_dimensions(
        definition.footprint.width,
        definition.footprint.height,
        placement.rotation,
    ) else {
        diagnostics.push(FacilityPortProjectionDiagnostic::error(
            "unsupported-facility-rotation",
            format!("{path}/rotation"),
            Some(placement.instance.clone()),
            format!(
                "placed facility instance '{}' rotation must be 0, 90, 180, or 270 degrees, found {}",
                placement.instance, placement.rotation
            ),
        ));
        return;
    };

    if !definition.allowed_rotations.contains(&placement.rotation) {
        diagnostics.push(FacilityPortProjectionDiagnostic::error(
            "disallowed-facility-rotation",
            format!("{path}/rotation"),
            Some(placement.instance.clone()),
            format!(
                "placed facility instance '{}' uses rotation {} which facility '{}' does not allow",
                placement.instance, placement.rotation, placement.facility
            ),
        ));
        return;
    }

    if placement.width != expected_width || placement.height != expected_height {
        diagnostics.push(FacilityPortProjectionDiagnostic::error(
            "placed-facility-dimensions-mismatch",
            path.clone(),
            Some(placement.instance.clone()),
            format!(
                "placed facility instance '{}' dimensions {}x{} do not match facility '{}' rotation {} dimensions {}x{}",
                placement.instance,
                placement.width,
                placement.height,
                placement.facility,
                placement.rotation,
                expected_width,
                expected_height
            ),
        ));
        return;
    }

    if !placement_is_inside_request(placement, request) {
        diagnostics.push(FacilityPortProjectionDiagnostic::error(
            "placed-facility-out-of-bounds",
            path.clone(),
            Some(placement.instance.clone()),
            format!(
                "placed facility instance '{}' must be inside the {}x{} layout area",
                placement.instance, request.max_width, request.max_height
            ),
        ));
        return;
    }

    for (port_index, port) in definition.ports.iter().enumerate() {
        let (position, edge) = rotate_port(
            &port.position,
            port.edge,
            definition.footprint.width,
            definition.footprint.height,
            placement.rotation,
        );
        let world_position = WorldGridPosition {
            x: placement.x + position.x,
            y: placement.y + position.y,
        };
        let connection = connection_position(&world_position, edge);

        if !position_is_inside_request(&connection, request) {
            diagnostics.push(FacilityPortProjectionDiagnostic::error(
                "facility-port-connection-out-of-bounds",
                format!("{path}/ports/{port_index}/connection"),
                Some(placement.instance.clone()),
                format!(
                    "facility instance '{}' port '{}' connects outside the {}x{} layout area at ({}, {})",
                    placement.instance,
                    port.id,
                    request.max_width,
                    request.max_height,
                    connection.x,
                    connection.y
                ),
            ));
            continue;
        }

        ports.push(PlacedFacilityPort {
            instance: placement.instance.clone(),
            facility: placement.facility.clone(),
            port: port.id.clone(),
            direction: port.direction,
            transport: port.transport,
            position: world_position,
            edge,
            connection,
        });
    }
}

fn rotated_dimensions(width: i64, height: i64, rotation: i64) -> Option<(i64, i64)> {
    match rotation {
        0 | 180 => Some((width, height)),
        90 | 270 => Some((height, width)),
        _ => None,
    }
}

fn rotate_port(
    position: &FacilityPortPosition,
    edge: FacilityPortEdge,
    width: i64,
    height: i64,
    rotation: i64,
) -> (FacilityPortPosition, FacilityPortEdge) {
    match rotation {
        0 => (position.clone(), edge),
        90 => (
            FacilityPortPosition {
                x: height - 1 - position.y,
                y: position.x,
            },
            edge.rotated_clockwise(rotation),
        ),
        180 => (
            FacilityPortPosition {
                x: width - 1 - position.x,
                y: height - 1 - position.y,
            },
            edge.rotated_clockwise(rotation),
        ),
        270 => (
            FacilityPortPosition {
                x: position.y,
                y: width - 1 - position.x,
            },
            edge.rotated_clockwise(rotation),
        ),
        _ => unreachable!("rotation is checked before rotating ports"),
    }
}

fn connection_position(position: &WorldGridPosition, edge: FacilityPortEdge) -> WorldGridPosition {
    match edge {
        FacilityPortEdge::North => WorldGridPosition {
            x: position.x,
            y: position.y - 1,
        },
        FacilityPortEdge::East => WorldGridPosition {
            x: position.x + 1,
            y: position.y,
        },
        FacilityPortEdge::South => WorldGridPosition {
            x: position.x,
            y: position.y + 1,
        },
        FacilityPortEdge::West => WorldGridPosition {
            x: position.x - 1,
            y: position.y,
        },
    }
}

fn placement_is_inside_request(
    placement: &FacilityPlacement,
    request: &FacilityPlacementRequest,
) -> bool {
    placement.x >= 0
        && placement.y >= 0
        && placement
            .x
            .checked_add(placement.width)
            .is_some_and(|right| right <= request.max_width)
        && placement
            .y
            .checked_add(placement.height)
            .is_some_and(|bottom| bottom <= request.max_height)
}

fn position_is_inside_request(
    position: &WorldGridPosition,
    request: &FacilityPlacementRequest,
) -> bool {
    position.x >= 0
        && position.y >= 0
        && position.x < request.max_width
        && position.y < request.max_height
}

fn failure(diagnostics: Vec<FacilityPortProjectionDiagnostic>) -> FacilityPortProjectionReport {
    FacilityPortProjectionReport {
        success: false,
        ports: Vec::new(),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{
        FacilityCatalog, FacilityDefinition, FacilityFootprint, FacilityPortDefinition,
    };
    use crate::layouts::{FacilityPlacementBounds, FacilityPlacementStatus};

    fn catalog() -> ValidatedFacilityCatalog {
        ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![FacilityDefinition {
                id: "machine".to_string(),
                footprint: FacilityFootprint {
                    width: 3,
                    height: 2,
                },
                allowed_rotations: vec![0, 90, 180, 270],
                ports: vec![FacilityPortDefinition {
                    id: "input".to_string(),
                    direction: FacilityPortDirection::Input,
                    transport: TransportKind::Belt,
                    position: FacilityPortPosition { x: 1, y: 1 },
                    edge: FacilityPortEdge::South,
                }],
            }],
        })
        .expect("test catalog should validate")
    }

    fn request() -> FacilityPlacementRequest {
        FacilityPlacementRequest {
            schema_version: 2,
            max_width: 10,
            max_height: 10,
        }
    }

    fn report(rotation: i64, x: i64, y: i64) -> FacilityPlacementReport {
        let (width, height) = rotated_dimensions(3, 2, rotation).expect("valid test rotation");
        FacilityPlacementReport {
            success: true,
            status: FacilityPlacementStatus::Optimal,
            bounds: Some(FacilityPlacementBounds { width, height }),
            placements: vec![FacilityPlacement {
                instance: "machine:0".to_string(),
                recipe: "recipe".to_string(),
                facility: "machine".to_string(),
                x,
                y,
                width,
                height,
                rotation,
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn rotates_port_positions_edges_and_connections() {
        let cases = [
            (
                0,
                WorldGridPosition { x: 4, y: 4 },
                FacilityPortEdge::South,
                WorldGridPosition { x: 4, y: 5 },
            ),
            (
                90,
                WorldGridPosition { x: 3, y: 4 },
                FacilityPortEdge::West,
                WorldGridPosition { x: 2, y: 4 },
            ),
            (
                180,
                WorldGridPosition { x: 4, y: 3 },
                FacilityPortEdge::North,
                WorldGridPosition { x: 4, y: 2 },
            ),
            (
                270,
                WorldGridPosition { x: 4, y: 4 },
                FacilityPortEdge::East,
                WorldGridPosition { x: 5, y: 4 },
            ),
        ];

        for (rotation, position, edge, connection) in cases {
            let projected = project_facility_ports(&report(rotation, 3, 3), &catalog(), &request());

            assert!(projected.success, "rotation {rotation}: {projected:?}");
            assert_eq!(projected.ports[0].position, position);
            assert_eq!(projected.ports[0].edge, edge);
            assert_eq!(projected.ports[0].connection, connection);
        }
    }

    #[test]
    fn rejects_connection_outside_layout_area() {
        let projected = project_facility_ports(&report(180, 3, 0), &catalog(), &request());

        assert!(!projected.success);
        assert!(projected.ports.is_empty());
        assert_eq!(
            projected.diagnostics[0].code,
            "facility-port-connection-out-of-bounds"
        );
    }

    #[test]
    fn rejects_malformed_placed_dimensions() {
        let mut placement_report = report(90, 3, 3);
        placement_report.placements[0].width = 3;

        let projected = project_facility_ports(&placement_report, &catalog(), &request());

        assert!(!projected.success);
        assert_eq!(
            projected.diagnostics[0].code,
            "placed-facility-dimensions-mismatch"
        );
    }

    #[test]
    fn rejects_failed_placement_report() {
        let placement_report = FacilityPlacementReport {
            success: false,
            status: FacilityPlacementStatus::Infeasible,
            bounds: None,
            placements: Vec::new(),
            diagnostics: Vec::new(),
        };

        let projected = project_facility_ports(&placement_report, &catalog(), &request());

        assert!(!projected.success);
        assert_eq!(
            projected.diagnostics[0].code,
            "upstream-facility-placement-failed"
        );
    }
}
