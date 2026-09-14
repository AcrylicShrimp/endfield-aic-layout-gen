use aic_data::facilities::{FacilityDefinition, FacilityPortDirection, FacilityPortEdge};
use aic_data::layouts::{FacilityPlacement, FactoryEditCatalogs};
use aic_data::logistics::{CardinalDirection, TransportKind};
use indexmap::IndexMap;
use serde::Serialize;

use super::types::*;

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
pub fn placement_ports(p: &FacilityPlacement, catalogs: &FactoryEditCatalogs) -> Result<Vec<Port>> {
    let spec = catalogs.facilities.facility(&p.facility).ok_or_else(|| {
        Diagnostic::new(
            "factory-unknown-facility",
            "Facility is absent from the runtime catalog",
            p.facility.clone(),
        )
    })?;
    Ok(rotate(spec, p.rotation, Point { x: p.x, y: p.y }).2)
}
pub fn project(w: &Witness, catalogs: &FactoryEditCatalogs) -> Result<IndexMap<String, Vec<Port>>> {
    w.placements
        .iter()
        .map(|p| Ok((p.instance.clone(), placement_ports(p, catalogs)?)))
        .collect()
}
