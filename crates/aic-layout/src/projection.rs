//! Catalog-backed port projection; rotations are validated by the caller.
use crate::facilities::{
    FacilityDefinition, FacilityPortDefinition, FacilityPortDirection, FacilityPortEdge,
};
use crate::geometry::Point;
use crate::logistics::{CardinalDirection, TransportKind};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Port {
    pub id: String,
    pub direction: FacilityPortDirection,
    pub transport: TransportKind,
    pub position: Point,
    pub edge: FacilityPortEdge,
    pub connection: Point,
}

pub fn edge_index(edge: FacilityPortEdge) -> usize {
    match edge {
        FacilityPortEdge::East => 0,
        FacilityPortEdge::South => 1,
        FacilityPortEdge::West => 2,
        FacilityPortEdge::North => 3,
    }
}
pub fn cardinal_index(edge: CardinalDirection) -> usize {
    match edge {
        CardinalDirection::East => 0,
        CardinalDirection::South => 1,
        CardinalDirection::West => 2,
        CardinalDirection::North => 3,
    }
}

pub fn project_port(
    spec: &FacilityDefinition,
    port: &FacilityPortDefinition,
    rotation: i64,
    origin: Point,
) -> Port {
    let mut width = spec.footprint.width;
    let mut height = spec.footprint.height;
    let mut position = Point {
        x: port.position.x,
        y: port.position.y,
    };
    let mut edge = port.edge;
    for _ in 0..rotation / 90 {
        position = Point {
            x: height - 1 - position.y,
            y: position.x,
        };
        edge = [
            FacilityPortEdge::East,
            FacilityPortEdge::South,
            FacilityPortEdge::West,
            FacilityPortEdge::North,
        ][(edge_index(edge) + 1) % 4];
        std::mem::swap(&mut width, &mut height);
    }
    position = position.shifted(origin.x, origin.y);
    Port {
        id: port.id.clone(),
        direction: port.direction,
        transport: port.transport,
        position,
        edge,
        connection: position.offset(edge_index(edge)),
    }
}

pub fn rotate(spec: &FacilityDefinition, rotation: i64, origin: Point) -> (i64, i64, Vec<Port>) {
    let mut width = spec.footprint.width;
    let mut height = spec.footprint.height;
    for _ in 0..rotation / 90 {
        std::mem::swap(&mut width, &mut height);
    }
    let ports: Vec<_> = spec
        .ports
        .iter()
        .map(|p| project_port(spec, p, rotation, origin))
        .collect();
    (width, height, ports)
}
