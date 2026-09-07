//! Independent acceptance rebuilt from owned geometry, without edit/index caches.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::catalog::Catalog;
use crate::diagnostic::{Diagnostic, Result, require};
use crate::state::{self, Binding, Bounds, Index, Layout, Point, Score, State};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Acceptance {
    pub valid: bool,
    pub score: Option<Score>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Acceptance {
    pub fn from_result(result: Result<(Score, Bounds)>) -> Self {
        match result {
            Ok((score, bounds)) => Self {
                valid: true,
                score: Some(score),
                bounds: Some(bounds),
                diagnostics: Vec::new(),
            },
            Err(error) => Self {
                valid: false,
                score: None,
                bounds: None,
                diagnostics: vec![error],
            },
        }
    }
}

pub fn check_layout(catalog: &Catalog, layout: Layout) -> Acceptance {
    Acceptance::from_result(State::import(catalog, layout).and_then(|s| check(catalog, &s)))
}

pub fn check(catalog: &Catalog, s: &State) -> Result<(Score, Bounds)> {
    let mut occupied = BTreeSet::new();
    for (fi, &pose) in s.poses.iter().enumerate() {
        let body = state::body(catalog, fi, pose)?;
        require(
            body.is_disjoint(&occupied),
            "geometry.collision",
            "Facility footprints overlap",
            Some(&catalog.case.facilities[fi].id),
        )?;
        occupied.extend(body);
    }
    for (ni, n) in s.networks.iter().enumerate() {
        let nid = &catalog.case.networks[ni].id;
        require(
            !n.cells.is_empty(),
            "network.cells",
            "Empty or duplicate network cells",
            Some(nid),
        )?;
        require(
            n.cells.is_disjoint(&occupied),
            "geometry.collision",
            "Transport overlap or unsupported crossing",
            Some(nid),
        )?;
        occupied.extend(&n.cells);
    }
    let used = state::bounds(&occupied)?;
    let mut ports = BTreeSet::new();
    let (mut tile_count, mut turns) = (0_i64, 0_i64);
    for (ni, n) in s.networks.iter().enumerate() {
        let nid = &catalog.case.networks[ni].id;
        let (mut ins, mut outs) = (state::Arms::new(), state::Arms::new());
        let mut adjacency = BTreeMap::<Point, Vec<Point>>::new();
        for &[a, b] in &n.edges {
            require(
                n.cells.contains(&a) && n.cells.contains(&b),
                "network.edge",
                "Edge is not an occupied adjacent cell pair",
                Some(nid),
            )?;
            let d = state::arm(a, b).map_err(|mut e| {
                e.entity = Some(nid.clone());
                e
            })?;
            outs.entry(a).or_default().push(d);
            ins.entry(b).or_default().push(state::opposite(d));
            adjacency.entry(a).or_default().push(b);
        }
        let mut endpoints = [[0, 0]; 2];
        for (role, ti) in [ni * 2, ni * 2 + 1].into_iter().enumerate() {
            let t = &catalog.terminals[ti];
            let binding = s.bindings[ti].as_ref().ok_or_else(|| {
                Diagnostic::acceptance("terminal.unbound", "Unbound required terminal", Some(&t.id))
            })?;
            let (cell, arm) = match (t.facility, binding) {
                (Some(fi), Binding::Port { port }) => {
                    let (cell, arm, pi) = state::port_geometry(catalog, fi, s.poses[fi], port)?;
                    let p = &catalog.types[fi].ports[pi];
                    require(
                        p.transport == "belt"
                            && p.direction == if t.source { "output" } else { "input" },
                        "terminal.direction",
                        "Wrong port direction or transport",
                        Some(&t.id),
                    )?;
                    require(
                        ports.insert((fi, pi)),
                        "terminal.port_reuse",
                        "Physical port assigned more than once",
                        Some(&t.id),
                    )?;
                    (cell, arm)
                }
                (None, Binding::External { cell, side }) => {
                    require(
                        state::on_boundary(*cell, side, used),
                        "terminal.boundary",
                        "External endpoint is not on actual used boundary",
                        Some(&t.id),
                    )?;
                    (*cell, state::side_arm(side)?)
                }
                _ => {
                    return Err(Diagnostic::acceptance(
                        "terminal.binding",
                        if t.facility.is_some() {
                            "Expected a facility port binding"
                        } else {
                            "Expected external boundary binding"
                        },
                        Some(&t.id),
                    ));
                }
            };
            require(
                n.cells.contains(&cell),
                "terminal.cell",
                "Bound terminal has no network cell",
                Some(&t.id),
            )?;
            endpoints[role] = cell;
            (if t.source { &mut ins } else { &mut outs })
                .entry(cell)
                .or_default()
                .push(arm);
        }
        for c in &n.cells {
            let i = ins.get(c).map_or(&[][..], Vec::as_slice);
            let o = outs.get(c).map_or(&[][..], Vec::as_slice);
            require(
                i.len() == 1 && o.len() == 1,
                "network.degree",
                "Incomplete path or unsupported junction",
                Some(nid),
            )?;
            require(
                i[0] != o[0],
                "network.arm",
                "Input and output use the same side",
                Some(nid),
            )?;
            turns += i64::from(i[0] != state::opposite(o[0]));
        }
        let mut visited = BTreeSet::new();
        let mut frontier = vec![endpoints[0]];
        while let Some(c) = frontier.pop() {
            if visited.insert(c) {
                frontier.extend(adjacency.get(&c).into_iter().flatten());
            }
        }
        require(
            visited == n.cells && visited.contains(&endpoints[1]),
            "network.reachability",
            "Disconnected geometry or transport cycle",
            Some(nid),
        )?;
        require(
            catalog.rates[ni] <= catalog.capacity,
            "network.capacity",
            "Path exceeds belt capacity",
            Some(nid),
        )?;
        tile_count = tile_count
            .checked_add(
                i64::try_from(n.cells.len())
                    .map_err(|_| state::range_error("Tile count overflow"))?,
            )
            .ok_or_else(|| state::range_error("Tile count overflow"))?;
    }
    Ok(([state::area(used)?, tile_count, turns], used))
}

pub fn mechanical(catalog: &Catalog, s: &State) -> Result<()> {
    let error = |message: &str| Diagnostic::new("edit.rejected", "edit", message, None);
    let mut occupied = BTreeSet::new();
    let mut ports = BTreeSet::new();
    for (fi, &pose) in s.poses.iter().enumerate() {
        if !catalog.types[fi].allowed_rotations.contains(&pose[2]) {
            return Err(error("Disallowed facility rotation"));
        }
        let body = state::body(catalog, fi, pose)?;
        if !body.is_disjoint(&occupied) {
            return Err(error("Facility collision"));
        }
        occupied.extend(body);
    }
    for n in &s.networks {
        if !n.cells.is_disjoint(&occupied) {
            return Err(error("Transport collision or unsupported crossing"));
        }
        occupied.extend(&n.cells);
        for &[a, b] in &n.edges {
            if !n.cells.contains(&a) || !n.cells.contains(&b) || state::arm(a, b).is_err() {
                return Err(error("Invalid edge"));
            }
        }
    }
    for (ti, binding) in s.bindings.iter().enumerate() {
        let Some(binding) = binding else {
            continue;
        };
        let t = &catalog.terminals[ti];
        if let Some(fi) = t.facility {
            let Binding::Port { port } = binding else {
                return Err(error("Expected a facility port binding"));
            };
            let pi = catalog.types[fi]
                .ports
                .iter()
                .position(|p| p.id == *port)
                .ok_or_else(|| error("Unknown catalog port"))?;
            let p = &catalog.types[fi].ports[pi];
            if p.transport != "belt" || p.direction != if t.source { "output" } else { "input" } {
                return Err(error("Wrong terminal port role"));
            }
            if !ports.insert((fi, pi)) {
                return Err(error("Port already bound"));
            }
        }
        let (cell, _) = state::location(catalog, s, ti, Some(binding))?
            .ok_or_else(|| error("Missing terminal location"))?;
        if !s.networks[t.network].cells.contains(&cell) {
            return Err(error("Bound terminal cell is absent"));
        }
    }
    let index = Index::build(catalog, s)?;
    for (ni, n) in s.networks.iter().enumerate() {
        for c in &n.cells {
            let i = index.incoming[ni].get(c).map_or(&[][..], Vec::as_slice);
            let o = index.outgoing[ni].get(c).map_or(&[][..], Vec::as_slice);
            if i.len() > 1 || o.len() > 1 || (!i.is_empty() && !o.is_empty() && i[0] == o[0]) {
                return Err(error("Unsupported junction or conflicting directed arms"));
            }
        }
    }
    Ok(())
}
