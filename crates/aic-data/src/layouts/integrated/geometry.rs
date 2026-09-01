use std::collections::BTreeMap;

use crate::facilities::{FacilityDefinition, FacilityPortEdge, FacilityPortPosition};

use super::WorldGridPosition;

pub(super) fn candidate_port_connections(
    definition: &FacilityDefinition,
    rotation: i64,
    origin_x: i32,
    origin_y: i32,
    max_width: i32,
    max_height: i32,
) -> BTreeMap<String, usize> {
    let mut connections = BTreeMap::new();
    for port in &definition.ports {
        let (position, edge) = rotate_port(
            &port.position,
            port.edge,
            rotation,
            definition.footprint.width,
            definition.footprint.height,
        );
        let port_x = origin_x + position.x as i32;
        let port_y = origin_y + position.y as i32;
        let (connection_x, connection_y) = match edge {
            FacilityPortEdge::North => (port_x, port_y - 1),
            FacilityPortEdge::East => (port_x + 1, port_y),
            FacilityPortEdge::South => (port_x, port_y + 1),
            FacilityPortEdge::West => (port_x - 1, port_y),
        };
        if connection_x < 0
            || connection_y < 0
            || connection_x >= max_width
            || connection_y >= max_height
        {
            continue;
        }
        connections.insert(
            port.id.clone(),
            grid_index(connection_x, connection_y, max_width),
        );
    }
    connections
}

pub(super) fn grid_index(x: i32, y: i32, width: i32) -> usize {
    (y as usize) * (width as usize) + (x as usize)
}

pub(super) fn world_position(index: usize, width: i32) -> WorldGridPosition {
    WorldGridPosition {
        x: (index % width as usize) as i64,
        y: (index / width as usize) as i64,
    }
}

pub(super) fn rotate_port(
    position: &FacilityPortPosition,
    edge: FacilityPortEdge,
    rotation: i64,
    width: i64,
    height: i64,
) -> (FacilityPortPosition, FacilityPortEdge) {
    let position = match rotation {
        0 => position.clone(),
        90 => FacilityPortPosition {
            x: height - 1 - position.y,
            y: position.x,
        },
        180 => FacilityPortPosition {
            x: width - 1 - position.x,
            y: height - 1 - position.y,
        },
        270 => FacilityPortPosition {
            x: position.y,
            y: width - 1 - position.x,
        },
        _ => unreachable!("validated facility rotation"),
    };
    (position, edge.rotated_clockwise(rotation))
}
