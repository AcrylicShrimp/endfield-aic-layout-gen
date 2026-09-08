//! Measured used geometry; physical layer cost and XY density remain separate.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::facilities::FacilityPortEdge;
use crate::layouts::{
    FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, TransportNetworkEndpoint, WorldGridPosition, project_facility_ports,
};

type Point = (i64, i64);
type Arms = BTreeMap<Point, Vec<Point>>;

fn xy(p: &WorldGridPosition) -> Point {
    (p.x, p.y)
}
fn delta(a: &WorldGridPosition, b: &WorldGridPosition) -> Point {
    (b.x - a.x, b.y - a.y)
}

pub(super) fn measure(
    input: &WitnessProbeInput,
    catalogs: &FactoryEditCatalogs,
) -> Result<FactoryLayoutMetrics> {
    let mut bodies = BTreeSet::new();
    for p in &input.placements {
        for x in p.x..p.x + p.width {
            for y in p.y..p.y + p.height {
                bodies.insert((x, y));
            }
        }
    }
    let physical = input
        .transport_networks
        .iter()
        .flat_map(|n| n.cells.iter().map(move |p| (n.transport, xy(p))))
        .collect::<BTreeSet<_>>();
    let transport = physical.iter().map(|(_, p)| *p).collect::<BTreeSet<_>>();
    let occupied = bodies.union(&transport).copied().collect::<BTreeSet<_>>();
    let width = occupied.iter().map(|p| p.0).max().unwrap() as u64 + 1;
    let height = occupied.iter().map(|p| p.1).max().unwrap() as u64 + 1;
    let area = width
        .checked_mul(height)
        .ok_or_else(|| error("factory.score-overflow", "layout", "Used area overflow"))?;
    let bounds = FacilityPlacementBounds {
        width: width as i64 + 2,
        height: height as i64 + 2,
    };
    // Padding is only for neutral port projection, never a placement/search bound.
    let projected = project_facility_ports(
        &FacilityPlacementReport {
            success: true,
            status: FacilityPlacementStatus::Feasible,
            bounds: Some(bounds.clone()),
            placements: input
                .placements
                .iter()
                .map(|p| crate::layouts::FacilityPlacement {
                    x: p.x + 1,
                    y: p.y + 1,
                    ..p.clone()
                })
                .collect(),
            diagnostics: vec![],
        },
        &catalogs.facilities,
        &FacilityPlacementRequest {
            schema_version: crate::layouts::SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
            max_width: bounds.width,
            max_height: bounds.height,
        },
    );
    if !projected.success {
        return Err(error(
            "factory.score-ports",
            "layout",
            "Validated facility port projection failed",
        ));
    }
    let ports = projected
        .ports
        .into_iter()
        .map(|mut p| {
            p.position.x -= 1;
            p.position.y -= 1;
            ((p.instance.clone(), p.port.clone()), p.position)
        })
        .collect::<BTreeMap<_, _>>();
    let mut turns = 0u64;
    for n in &input.transport_networks {
        let mut incoming = Arms::new();
        let mut outgoing = Arms::new();
        for s in &n.segments {
            let d = delta(&s.from, &s.to);
            outgoing.entry(xy(&s.from)).or_default().push(d);
            incoming.entry(xy(&s.to)).or_default().push(d);
        }
        for t in &n.terminals {
            let source = t.direction == FacilityPortDirection::Output;
            let direction = match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, port } => {
                    let p = ports
                        .get(&(instance.clone(), port.clone()))
                        .ok_or_else(|| {
                            error(
                                "factory.score-ports",
                                &t.id,
                                "Validated terminal port is absent",
                            )
                        })?;
                    if source {
                        delta(p, &t.position)
                    } else {
                        delta(&t.position, p)
                    }
                }
                TransportNetworkEndpoint::External { side, .. } => {
                    let d = match side {
                        FacilityPortEdge::North => (0, -1),
                        FacilityPortEdge::East => (1, 0),
                        FacilityPortEdge::South => (0, 1),
                        FacilityPortEdge::West => (-1, 0),
                    };
                    if source { (-d.0, -d.1) } else { d }
                }
            };
            (if source { &mut incoming } else { &mut outgoing })
                .entry(xy(&t.position))
                .or_default()
                .push(direction);
        }
        for p in &n.cells {
            if input.logistics_components.iter().any(|c| {
                c.transport == n.transport
                    && c.kind == crate::logistics::LogisticsComponentKind::Bridge
                    && c.position == *p
            }) {
                continue;
            }
            for a in incoming.get(&xy(p)).into_iter().flatten() {
                for b in outgoing.get(&xy(p)).into_iter().flatten() {
                    turns += u64::from(a.0 * b.0 + a.1 * b.1 == 0);
                }
            }
        }
    }
    Ok(FactoryLayoutMetrics {
        definition: "used-area-layer-tiles-all-terminal-arms-v1",
        score: [area, physical.len() as u64, turns],
        width,
        height,
        facility_xy_cells: bodies.len() as u64,
        transport_xy_cells: transport.len() as u64,
        occupied_xy_cells: occupied.len() as u64,
        empty_xy_cells: area - occupied.len() as u64,
    })
}
