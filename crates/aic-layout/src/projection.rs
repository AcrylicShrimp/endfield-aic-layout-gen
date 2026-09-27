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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionAdmission {
    Granted,
    Pending,
    Denied,
}

#[derive(Debug)]
pub enum ProjectionAdvance {
    Pending,
    Denied,
    Complete(Vec<Port>),
}

#[derive(Default)]
pub struct PortProjectionCursor {
    next_port: usize,
    ports: Vec<Port>,
    reserved_bytes: usize,
}

impl PortProjectionCursor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    pub fn advance(
        &mut self,
        spec: &FacilityDefinition,
        rotation: i64,
        origin: Point,
        mut admit: impl FnMut(usize) -> ProjectionAdmission,
    ) -> ProjectionAdvance {
        while let Some(port) = spec.ports.get(self.next_port) {
            // The catalog fixes the final port count. Allocate that vector once
            // so bounded validation need not guess at reallocation peak memory.
            let vector_bytes = if self.next_port == 0 {
                spec.ports.len().saturating_mul(std::mem::size_of::<Port>())
            } else {
                0
            };
            let bytes = vector_bytes.saturating_add(port.id.len());
            match admit(bytes) {
                ProjectionAdmission::Granted => {}
                ProjectionAdmission::Pending => return ProjectionAdvance::Pending,
                ProjectionAdmission::Denied => return ProjectionAdvance::Denied,
            }
            if self.next_port == 0 {
                self.ports.reserve_exact(spec.ports.len());
            }
            self.ports.push(project_port(spec, port, rotation, origin));
            self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
            self.next_port += 1;
        }
        ProjectionAdvance::Complete(std::mem::take(&mut self.ports))
    }
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
    project_port_with_id(spec, port, rotation, origin, port.id.clone())
}

/// Project a selected port while transferring an already-owned catalog ID.
/// Retained callers can copy that ID under their own page and memory admission.
pub fn project_port_with_id(
    spec: &FacilityDefinition,
    port: &FacilityPortDefinition,
    rotation: i64,
    origin: Point,
    id: String,
) -> Port {
    let (position, edge) = projected_port_geometry(spec, port, rotation, origin);
    Port {
        id,
        direction: port.direction,
        transport: port.transport,
        position,
        edge,
        connection: position.offset(edge_index(edge)),
    }
}

/// Return only the selected port position for route-arm checks without
/// allocating a projected port ID or an entire facility-port list.
pub fn project_port_position(
    spec: &FacilityDefinition,
    port: &FacilityPortDefinition,
    rotation: i64,
    origin: Point,
) -> Point {
    projected_port_geometry(spec, port, rotation, origin).0
}

fn projected_port_geometry(
    spec: &FacilityDefinition,
    port: &FacilityPortDefinition,
    rotation: i64,
    origin: Point,
) -> (Point, FacilityPortEdge) {
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
    (position, edge)
}

pub fn rotate(spec: &FacilityDefinition, rotation: i64, origin: Point) -> (i64, i64, Vec<Port>) {
    let mut width = spec.footprint.width;
    let mut height = spec.footprint.height;
    for _ in 0..rotation / 90 {
        std::mem::swap(&mut width, &mut height);
    }
    let ports = match PortProjectionCursor::new()
        .advance(spec, rotation, origin, |_| ProjectionAdmission::Granted)
    {
        ProjectionAdvance::Complete(ports) => ports,
        ProjectionAdvance::Pending | ProjectionAdvance::Denied => {
            unreachable!("unbounded port projection always admits")
        }
    };
    (width, height, ports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityFootprint, FacilityPortPosition};

    #[test]
    fn many_port_projection_resumes_without_repeating_admission() {
        let spec = FacilityDefinition {
            id: "many-ports".into(),
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![90],
            ports: (0..400)
                .map(|index| FacilityPortDefinition {
                    id: format!("port-{index}"),
                    direction: FacilityPortDirection::Output,
                    transport: TransportKind::Belt,
                    position: FacilityPortPosition { x: 0, y: 0 },
                    edge: FacilityPortEdge::East,
                })
                .collect(),
        };
        let origin = Point { x: 8, y: 13 };
        let expected = rotate(&spec, 90, origin).2;
        let mut cursor = PortProjectionCursor::new();
        let mut charged = 0usize;
        let actual = loop {
            let mut page_credit = 1;
            match cursor.advance(&spec, 90, origin, |bytes| {
                if page_credit == 0 {
                    ProjectionAdmission::Pending
                } else {
                    page_credit -= 1;
                    charged += bytes;
                    ProjectionAdmission::Granted
                }
            }) {
                ProjectionAdvance::Pending => {}
                ProjectionAdvance::Complete(ports) => break ports,
                ProjectionAdvance::Denied => panic!("test admission cannot deny"),
            }
        };
        assert_eq!(actual.len(), expected.len());
        for ((actual, expected), port) in actual.iter().zip(&expected).zip(&spec.ports) {
            assert_eq!(actual.id, expected.id);
            assert_eq!(actual.position, expected.position);
            assert_eq!(
                actual.position,
                project_port_position(&spec, port, 90, origin)
            );
            assert_eq!(actual.connection, expected.connection);
            assert_eq!(actual.edge, expected.edge);
        }
        assert_eq!(charged, cursor.reserved_bytes());
    }
}
