//! Catalog-backed port projection; rotations are validated by the caller.
use crate::facilities::{FacilityDefinition, FacilityPortDirection, FacilityPortEdge};
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
pub fn rotate(spec: &FacilityDefinition, rotation: i64, origin: Point) -> (i64, i64, Vec<Port>) {
    let mut width = spec.footprint.width;
    let mut height = spec.footprint.height;
    let mut ports: Vec<_> = spec
        .ports
        .iter()
        .map(|p| Port {
            id: p.id.clone(),
            direction: p.direction,
            transport: p.transport,
            position: Point {
                x: p.position.x,
                y: p.position.y,
            },
            edge: p.edge,
            connection: Point::default(),
        })
        .collect();
    for _ in 0..rotation / 90 {
        for p in &mut ports {
            p.position = Point {
                x: height - 1 - p.position.y,
                y: p.position.x,
            };
            p.edge = [
                FacilityPortEdge::East,
                FacilityPortEdge::South,
                FacilityPortEdge::West,
                FacilityPortEdge::North,
            ][(edge_index(p.edge) + 1) % 4];
        }
        std::mem::swap(&mut width, &mut height);
    }
    for p in &mut ports {
        p.position = p.position.shifted(origin.x, origin.y);
        p.connection = p.position.offset(edge_index(p.edge));
    }
    (width, height, ports)
}
