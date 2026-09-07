//! Existing explicit local action domain; no ranking, repair or route search.

use std::collections::BTreeSet;

use crate::catalog::Catalog;
use crate::command::Command;
use crate::diagnostic::Result;
use crate::engine::Config;
use crate::state::{self, ARMS, Binding, Index, Point, State};

fn arms<'a>(index: &'a [state::Arms], ni: usize, cell: &Point) -> &'a [Point] {
    index[ni].get(cell).map_or(&[], Vec::as_slice)
}

pub fn generate(
    catalog: &Catalog,
    config: &Config,
    s: &State,
    index: &Index,
) -> Result<Vec<Command>> {
    let mut candidates = vec![Command::Discard];
    let mut port_use = BTreeSet::new();
    for (ti, binding) in s.bindings.iter().enumerate() {
        if let (Some(fi), Some(Binding::Port { port })) = (catalog.terminals[ti].facility, binding)
        {
            port_use.insert((fi, port.as_str()));
        }
    }
    let mut unbound_facilities = Vec::new();
    for fi in 0..s.poses.len() {
        if catalog
            .terminals
            .iter()
            .enumerate()
            .any(|(ti, t)| t.facility == Some(fi) && s.bindings[ti].is_some())
        {
            continue;
        }
        unbound_facilities.push(fi);
        let [x, y, r] = s.poses[fi];
        let mut poses = BTreeSet::new();
        for d in ARMS {
            for &length in &config.move_lengths {
                let p = state::add([x, y], [d[0] * length, d[1] * length])?;
                poses.insert([p[0], p[1], r]);
            }
        }
        poses.extend(
            catalog.types[fi]
                .allowed_rotations
                .iter()
                .filter(|&&rot| rot != r)
                .map(|&rot| [x, y, rot]),
        );
        let blocked: BTreeSet<_> = index
            .occupied
            .difference(&index.bodies[fi])
            .copied()
            .collect();
        for pose in poses {
            if state::body(catalog, fi, pose)?.is_disjoint(&blocked) {
                candidates.push(Command::SetPlacements(vec![(fi, pose)]));
            }
        }
    }
    for (i, &a) in unbound_facilities.iter().enumerate() {
        for &b in &unbound_facilities[i + 1..] {
            let (pa, pb) = (s.poses[a], s.poses[b]);
            let (aa, bb) = ([pb[0], pb[1], pa[2]], [pa[0], pa[1], pb[2]]);
            let (body_a, body_b) = (state::body(catalog, a, aa)?, state::body(catalog, b, bb)?);
            let blocked: BTreeSet<_> = index
                .occupied
                .iter()
                .filter(|c| !index.bodies[a].contains(*c) && !index.bodies[b].contains(*c))
                .copied()
                .collect();
            if body_a.is_disjoint(&body_b)
                && body_a.is_disjoint(&blocked)
                && body_b.is_disjoint(&blocked)
            {
                candidates.push(Command::SetPlacements(vec![(a, aa), (b, bb)]));
            }
        }
    }
    for (ti, t) in catalog.terminals.iter().enumerate() {
        if s.bindings[ti].is_some() {
            candidates.push(Command::Unbind(ti));
            continue;
        }
        let ni = t.network;
        let mut options = Vec::new();
        if let Some(fi) = t.facility {
            for p in &catalog.types[fi].ports {
                if p.direction == if t.source { "output" } else { "input" }
                    && p.transport == "belt"
                    && !port_use.contains(&(fi, p.id.as_str()))
                {
                    options.push(Binding::Port { port: p.id.clone() });
                }
            }
        } else {
            for &cell in &s.networks[ni].cells {
                for side in ["W", "N", "E", "S"] {
                    if state::on_boundary(cell, side, index.used) {
                        options.push(Binding::External {
                            cell,
                            side: side.into(),
                        });
                    }
                }
            }
        }
        for binding in options {
            let Some((cell, arm)) = state::location(catalog, s, ti, Some(&binding))? else {
                continue;
            };
            if index.occupied.contains(&cell) && !s.networks[ni].cells.contains(&cell) {
                continue;
            }
            let (own, other) = if t.source {
                (
                    arms(&index.incoming, ni, &cell),
                    arms(&index.outgoing, ni, &cell),
                )
            } else {
                (
                    arms(&index.outgoing, ni, &cell),
                    arms(&index.incoming, ni, &cell),
                )
            };
            if !own.is_empty() || other.first() == Some(&arm) {
                continue;
            }
            candidates.push(Command::Bind(ti, binding));
        }
    }
    for (ni, n) in s.networks.iter().enumerate() {
        let removable: BTreeSet<_> = n.cells.difference(&index.protected[ni]).copied().collect();
        for &c in &removable {
            candidates.push(Command::Erase(ni, vec![c]));
        }
        for &[a, b] in &n.edges {
            if removable.contains(&a) && removable.contains(&b) {
                candidates.push(Command::Erase(ni, vec![a, b]));
            }
        }
        for &c in &n.cells {
            let (ins, outs) = (arms(&index.incoming, ni, &c), arms(&index.outgoing, ni, &c));
            for (backward, own, other) in [(false, outs, ins), (true, ins, outs)] {
                if !own.is_empty() {
                    continue;
                }
                for (direction, d) in ARMS.into_iter().enumerate() {
                    if other.first() == Some(&d) {
                        continue;
                    }
                    for &length in &config.segment_lengths {
                        let mut clear = true;
                        for k in 1..=length {
                            let p = state::add(c, [d[0] * k, d[1] * k])?;
                            if index.occupied.contains(&p) {
                                clear = false;
                                break;
                            }
                        }
                        if clear {
                            candidates.push(Command::Extend {
                                network: ni,
                                anchor: c,
                                backward,
                                direction,
                                length,
                            });
                        }
                    }
                }
            }
            if outs.is_empty() {
                for d in ARMS {
                    let target = state::add(c, d)?;
                    if n.cells.contains(&target)
                        && arms(&index.incoming, ni, &target).is_empty()
                        && !n.edges.contains(&[c, target])
                        && ins.first() != Some(&d)
                        && arms(&index.outgoing, ni, &target).first() != Some(&state::opposite(d))
                    {
                        candidates.push(Command::Connect {
                            network: ni,
                            source: c,
                            target,
                        });
                    }
                }
            }
        }
    }
    Ok(candidates)
}
