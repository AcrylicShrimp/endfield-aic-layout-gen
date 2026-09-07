//! Typed owned layouts; all external exports are independent of cached indexes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::catalog::Catalog;
use crate::diagnostic::{Diagnostic, Result, require};

pub type Point = [i64; 2];
pub type Pose = [i64; 3];
pub type Edge = [Point; 2];
pub type Bounds = [i64; 4];
pub type Score = [i64; 3];
pub const ARMS: [Point; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
pub const SIDES: [&str; 4] = ["N", "E", "S", "W"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged, deny_unknown_fields)]
pub enum Binding {
    Port { port: String },
    External { cell: Point, side: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub cells: BTreeSet<Point>,
    pub edges: BTreeSet<Edge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub poses: Vec<Pose>,
    pub bindings: Vec<Option<Binding>>,
    pub networks: Vec<Network>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Layout {
    pub schema_version: u32,
    pub placements: BTreeMap<String, Pose>,
    pub bindings: BTreeMap<String, Option<Binding>>,
    pub networks: BTreeMap<String, NetworkDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetworkDto {
    pub cells: Vec<Point>,
    pub edges: Vec<Edge>,
}

pub fn range_error(message: &str) -> Diagnostic {
    Diagnostic::new("geometry.range", "geometry", message, None)
}

pub fn add(a: Point, b: Point) -> Result<Point> {
    Ok([
        a[0].checked_add(b[0])
            .ok_or_else(|| range_error("Coordinate addition overflow"))?,
        a[1].checked_add(b[1])
            .ok_or_else(|| range_error("Coordinate addition overflow"))?,
    ])
}

pub fn arm(a: Point, b: Point) -> Result<Point> {
    let dx = b[0] as i128 - a[0] as i128;
    let dy = b[1] as i128 - a[1] as i128;
    require(
        dx.abs() + dy.abs() == 1,
        "network.edge",
        "Edge is not an occupied adjacent cell pair",
        None,
    )?;
    Ok([dx as i64, dy as i64])
}

pub fn opposite(a: Point) -> Point {
    [-a[0], -a[1]]
}

pub fn side_arm(side: &str) -> Result<Point> {
    SIDES
        .iter()
        .position(|s| *s == side)
        .map(|i| ARMS[i])
        .ok_or_else(|| {
            Diagnostic::acceptance("terminal.boundary", "Unknown boundary side", Some(side))
        })
}

pub fn bounds<'a>(points: impl IntoIterator<Item = &'a Point>) -> Result<Bounds> {
    let mut result = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
    let mut any = false;
    for &[x, y] in points {
        any = true;
        result = [
            result[0].min(x),
            result[1].min(y),
            result[2].max(x),
            result[3].max(y),
        ];
    }
    require(any, "geometry.empty", "Empty used geometry", None)?;
    Ok(result)
}

pub fn on_boundary([x, y]: Point, side: &str, b: Bounds) -> bool {
    match side {
        "N" => y == b[1],
        "E" => x == b[2],
        "S" => y == b[3],
        "W" => x == b[0],
        _ => false,
    }
}

pub fn area(b: Bounds) -> Result<i64> {
    let w = b[2] as i128 - b[0] as i128 + 1;
    let h = b[3] as i128 - b[1] as i128 + 1;
    w.checked_mul(h)
        .and_then(|n| i64::try_from(n).ok())
        .ok_or_else(|| range_error("Used area exceeds score representation"))
}

pub fn body(catalog: &Catalog, fi: usize, pose: Pose) -> Result<BTreeSet<Point>> {
    let definition = &catalog.types[fi];
    require(
        definition.allowed_rotations.contains(&pose[2]),
        "geometry.rotation",
        "Disallowed rotation",
        Some(&catalog.case.facilities[fi].id),
    )?;
    let (mut w, mut h) = (definition.footprint.width, definition.footprint.height);
    if pose[2] % 180 != 0 {
        std::mem::swap(&mut w, &mut h);
    }
    // Catalog geometry size is admitted against the configured work guard before use.
    add([pose[0], pose[1]], [w - 1, h - 1])?;
    let mut cells = BTreeSet::new();
    for x in 0..w {
        for y in 0..h {
            cells.insert([pose[0] + x, pose[1] + y]);
        }
    }
    Ok(cells)
}

pub fn port_geometry(
    catalog: &Catalog,
    fi: usize,
    pose: Pose,
    port_id: &str,
) -> Result<(Point, Point, usize)> {
    let definition = &catalog.types[fi];
    let pi = definition
        .ports
        .iter()
        .position(|p| p.id == port_id)
        .ok_or_else(|| {
            Diagnostic::acceptance(
                "terminal.port",
                "Unknown catalog port",
                Some(&format!("{}/{port_id}", catalog.case.facilities[fi].id)),
            )
        })?;
    require(
        definition.allowed_rotations.contains(&pose[2]),
        "geometry.rotation",
        "Disallowed rotation",
        Some(&catalog.case.facilities[fi].id),
    )?;
    let p = &definition.ports[pi];
    let (mut a, mut b) = (p.position.x, p.position.y);
    let (mut w, mut h) = (definition.footprint.width, definition.footprint.height);
    for _ in 0..pose[2] / 90 {
        (a, b, w, h) = (h - 1 - b, a, h, w);
    }
    let side = ["north", "east", "south", "west"]
        .iter()
        .position(|v| *v == p.edge)
        .ok_or_else(|| range_error("Invalid port side"))?;
    let direction = ARMS[(side + (pose[2] / 90) as usize) % 4];
    Ok((
        add(add([pose[0], pose[1]], [a, b])?, direction)?,
        opposite(direction),
        pi,
    ))
}

pub fn location(
    catalog: &Catalog,
    state: &State,
    ti: usize,
    binding: Option<&Binding>,
) -> Result<Option<(Point, Point)>> {
    let Some(binding) = binding else {
        return Ok(None);
    };
    let terminal = &catalog.terminals[ti];
    match (terminal.facility, binding) {
        (Some(fi), Binding::Port { port }) => {
            let (cell, arm, _) = port_geometry(catalog, fi, state.poses[fi], port)?;
            Ok(Some((cell, arm)))
        }
        (None, Binding::External { cell, side }) => Ok(Some((*cell, side_arm(side)?))),
        _ => Err(Diagnostic::acceptance(
            "terminal.binding",
            if terminal.facility.is_some() {
                "Expected a facility port binding"
            } else {
                "Expected external boundary binding"
            },
            Some(&terminal.id),
        )),
    }
}

pub fn cells_on_line(a: Point, b: Point, guard: usize) -> Result<Vec<Point>> {
    require(
        a[0] == b[0] || a[1] == b[1],
        "input.segment",
        "A segment must be cardinal",
        None,
    )?;
    let dx = b[0] as i128 - a[0] as i128;
    let dy = b[1] as i128 - a[1] as i128;
    let len = dx.abs() + dy.abs() + 1;
    require(
        len <= guard as i128,
        "input.resource_guard",
        "Segment exceeds declared geometry-work guard",
        None,
    )?;
    Ok((0..len)
        .map(|i| {
            [
                (a[0] as i128 + i * dx.signum()) as i64,
                (a[1] as i128 + i * dy.signum()) as i64,
            ]
        })
        .collect())
}

impl State {
    pub fn initial(catalog: &Catalog, guard: usize) -> Result<Self> {
        let poses = catalog.case.facilities.iter().map(|f| f.pose).collect();
        let mut bindings = Vec::new();
        let mut networks = Vec::new();
        for n in &catalog.case.networks {
            let mut path = Vec::new();
            for pair in n.corners.windows(2) {
                let segment = cells_on_line(pair[0], pair[1], guard)?;
                path.extend(segment.into_iter().skip(usize::from(!path.is_empty())));
                require(
                    path.len() <= guard,
                    "input.resource_guard",
                    "Root path exceeds declared geometry-work guard",
                    Some(&n.id),
                )?;
            }
            if n.corners.len() == 1 {
                path.push(n.corners[0]);
            }
            networks.push(Network {
                cells: path.iter().copied().collect(),
                edges: path.windows(2).map(|p| [p[0], p[1]]).collect(),
            });
            bindings.extend([n.source_binding.clone(), n.target_binding.clone()]);
        }
        Ok(Self {
            poses,
            bindings,
            networks,
        })
    }

    pub fn export(&self, catalog: &Catalog) -> Layout {
        Layout {
            schema_version: 1,
            placements: catalog
                .case
                .facilities
                .iter()
                .zip(&self.poses)
                .map(|(f, p)| (f.id.clone(), *p))
                .collect(),
            bindings: catalog
                .terminals
                .iter()
                .zip(&self.bindings)
                .map(|(t, b)| (t.id.clone(), b.clone()))
                .collect(),
            networks: catalog
                .case
                .networks
                .iter()
                .zip(&self.networks)
                .map(|(n, s)| {
                    (
                        n.id.clone(),
                        NetworkDto {
                            cells: s.cells.iter().copied().collect(),
                            edges: s.edges.iter().copied().collect(),
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn import(catalog: &Catalog, layout: Layout) -> Result<Self> {
        require(
            layout.schema_version == 1,
            "layout.schema",
            "Unsupported layout schema",
            None,
        )?;
        require(
            layout.placements.keys().eq(catalog.facility_ids.keys()),
            "layout.inventory",
            "Facility inventory changed",
            None,
        )?;
        require(
            layout.networks.keys().eq(catalog.network_ids.keys()),
            "layout.networks",
            "Network inventory changed",
            None,
        )?;
        require(
            layout.bindings.keys().eq(catalog.terminal_ids.keys()),
            "terminal.inventory",
            "Terminal inventory changed",
            None,
        )?;
        let poses = catalog
            .case
            .facilities
            .iter()
            .map(|f| layout.placements[&f.id])
            .collect();
        let bindings = catalog
            .terminals
            .iter()
            .map(|t| layout.bindings[&t.id].clone())
            .collect();
        let mut networks = Vec::new();
        for n in &catalog.case.networks {
            let dto = &layout.networks[&n.id];
            let cells: BTreeSet<_> = dto.cells.iter().copied().collect();
            let edges: BTreeSet<_> = dto.edges.iter().copied().collect();
            require(
                cells.len() == dto.cells.len(),
                "network.cells",
                "Empty or duplicate network cells",
                Some(&n.id),
            )?;
            require(
                edges.len() == dto.edges.len(),
                "network.edge",
                "Duplicate directed edge",
                Some(&n.id),
            )?;
            networks.push(Network { cells, edges });
        }
        Ok(Self {
            poses,
            bindings,
            networks,
        })
    }
}

pub type Arms = BTreeMap<Point, Vec<Point>>;

pub struct Index {
    pub bodies: Vec<BTreeSet<Point>>,
    pub occupied: BTreeSet<Point>,
    pub incoming: Vec<Arms>,
    pub outgoing: Vec<Arms>,
    pub protected: Vec<BTreeSet<Point>>,
    pub used: Bounds,
}

impl Index {
    pub fn build(catalog: &Catalog, state: &State) -> Result<Self> {
        let bodies: Vec<_> = state
            .poses
            .iter()
            .enumerate()
            .map(|(fi, p)| body(catalog, fi, *p))
            .collect::<Result<_>>()?;
        let mut occupied: BTreeSet<_> = bodies.iter().flatten().copied().collect();
        let mut incoming = Vec::new();
        let mut outgoing = Vec::new();
        let mut protected = vec![BTreeSet::new(); state.networks.len()];
        for (ni, n) in state.networks.iter().enumerate() {
            occupied.extend(&n.cells);
            let (mut ins, mut outs) = (Arms::new(), Arms::new());
            for &[a, b] in &n.edges {
                let d = arm(a, b)?;
                outs.entry(a).or_default().push(d);
                ins.entry(b).or_default().push(opposite(d));
            }
            for ti in [ni * 2, ni * 2 + 1] {
                if let Some((cell, arm)) =
                    location(catalog, state, ti, state.bindings[ti].as_ref())?
                {
                    (if catalog.terminals[ti].source {
                        &mut ins
                    } else {
                        &mut outs
                    })
                    .entry(cell)
                    .or_default()
                    .push(arm);
                    protected[ni].insert(cell);
                }
            }
            incoming.push(ins);
            outgoing.push(outs);
        }
        Ok(Self {
            used: bounds(&occupied)?,
            bodies,
            occupied,
            protected,
            incoming,
            outgoing,
        })
    }
}
