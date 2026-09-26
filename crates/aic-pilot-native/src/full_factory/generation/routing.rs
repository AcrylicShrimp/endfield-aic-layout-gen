//! Reembed affected chains inside the measured current extent, pipe first.

use aic_data::layouts::{FactoryEditCatalogs, PlacedLogisticsComponent, TransportNetworkSegment};
use aic_data::logistics::LogisticsComponentKind;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::chains::{self, Chain};
use super::changes::{self, ComponentPose, FacilityRotation, Move, PortAssignment};
use super::movement::{self, MovementIndex};
use super::path::{self, PathOptions};
use super::types::*;

mod region;
pub use region::{Limits, Report, reembed};

#[derive(Clone, Default, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    #[serde(default)]
    pub moves: Vec<Move>,
    #[serde(default)]
    pub facility_rotations: Vec<FacilityRotation>,
    #[serde(default)]
    pub component_poses: Vec<ComponentPose>,
    #[serde(default)]
    pub release_bridges: Vec<String>,
    #[serde(default)]
    pub chain_ids: Vec<usize>,
    #[serde(default)]
    pub port_assignments: Vec<PortAssignment>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RouteReport {
    pub id: usize,
    pub item: String,
    pub first: Option<usize>,
    pub last: Option<usize>,
    pub status: &'static str,
    pub expanded: usize,
    pub preflight_expanded: usize,
    pub preflight_status: &'static str,
    pub old_steps: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<i64>,
    pub crossing_sites: usize,
    pub crossings: Vec<path::Crossing>,
}
#[derive(Debug, Serialize)]
pub struct Execution {
    pub status: &'static str,
    pub witness: Option<Witness>,
    pub diagnostics: Vec<Diagnostic>,
    pub routes: Vec<RouteReport>,
    pub route_expansions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Report>,
}
struct Job {
    chain: Chain,
    start: Point,
    end: Point,
}

fn attempt(
    witness: &Witness,
    catalogs: &FactoryEditCatalogs,
    edit: &Edit,
    work_budget: usize,
    limits: &Limits,
    priority: Option<usize>,
    report: &mut region::Attempt,
) -> Result<Execution> {
    let (original, released) = changes::release_bridges(witness, catalogs, &edit.release_bridges)?;
    let decomposition = chains::extract(&original, &released)?;
    let chains = &decomposition.chains;
    let bounds = extent(&original)?;
    let flexible = chains::flexible_terminals(&original, chains);
    let mut changed: IndexSet<_> = edit.chain_ids.iter().copied().collect();
    let mut reroute: IndexSet<_> = chains
        .iter()
        .filter(|c| c.cells.iter().any(|&p| released.contains(&c.tag(p))))
        .map(|c| c.id)
        .collect();
    changed.extend(reroute.iter().copied());
    if changed.iter().any(|&i| i >= chains.len()) {
        return Err(Diagnostic::new(
            "local-chain-id",
            "Unknown chain id",
            serde_json::Value::Null,
        ));
    }
    reroute.extend(edit.chain_ids.iter().copied());
    let component_changes = changes::component_poses(&original, catalogs, &edit.component_poses)?;
    let facility_changes =
        changes::facility_rotations(&original, catalogs, &edit.moves, &edit.facility_rotations)?;
    let movement = if !edit.moves.is_empty() || !component_changes.is_empty() {
        Some(movement::plan(
            &original,
            chains,
            &decomposition.anchors,
            &edit.moves,
            &component_changes,
            &facility_changes,
            &reroute,
            &MovementIndex::new(&original, chains),
            None,
        )?)
    } else {
        None
    };
    let mut w = original.clone();
    if let Some(m) = &movement {
        for p in &mut w.placements {
            if m.selected.contains(&p.instance) {
                *p = m.shift_facility(p);
            }
        }
        for c in &mut w.logistics_components {
            if m.components.contains(&c.id) {
                c.position = m
                    .shift_anchor(layer(c.transport), Point::from(&c.position))
                    .into();
                if let Some(pose) = component_changes.get(&c.id) {
                    c.rotation = pose.rotation;
                }
            }
        }
        for n in &mut w.transport_networks {
            for t in &mut n.terminals {
                t.position = m.shift_terminal(t).into();
            }
        }
    }
    let mut paths = movement
        .as_ref()
        .map(|m| m.fixed_chains.clone())
        .unwrap_or_default();
    if let Some(m) = &movement {
        changed.extend(m.changed.iter().copied());
        for id in paths.keys() {
            changed.shift_remove(id);
        }
        let carried = w
            .placements
            .iter()
            .flat_map(|p| {
                [
                    Point { x: p.x, y: p.y },
                    Point {
                        x: p.x + p.width - 1,
                        y: p.y + p.height - 1,
                    },
                ]
            })
            .chain(
                w.logistics_components
                    .iter()
                    .map(|c| Point::from(&c.position)),
            )
            .chain(
                w.transport_networks
                    .iter()
                    .flat_map(|n| n.terminals.iter().map(|t| Point::from(&t.position))),
            )
            .chain(paths.values().flatten().copied());
        if carried.into_iter().any(|p| !bounds.contains(p)) {
            return Err(Diagnostic::new(
                "local-move-bounds",
                "Translated geometry leaves the explicit current-extent neighborhood",
                serde_json::json!(
                    edit.moves
                        .iter()
                        .flat_map(|m| &m.instances)
                        .collect::<Vec<_>>()
                ),
            ));
        }
    }
    let zero_terminals: IndexSet<_> = chains
        .iter()
        .filter(|c| c.zero_length)
        .flat_map(|c| {
            c.start_terminals
                .iter()
                .chain(&c.end_terminals)
                .map(|t| t.id.clone())
        })
        .collect();
    let eligible = original
        .transport_networks
        .iter()
        .flat_map(|n| {
            n.terminals
                .iter()
                .filter(|t| {
                    zero_terminals.contains(&t.id)
                        || flexible.contains(&(layer(n.transport), Point::from(&t.position)))
                })
                .map(|t| t.id.clone())
        })
        .collect();
    let port_positions = if edit.port_assignments.is_empty() {
        IndexMap::new()
    } else {
        changes::assign_ports(&mut w, catalogs, &edit.port_assignments, &eligible)?
    };
    let changed_port = |c: &Chain, end: bool| {
        (if end {
            &c.end_terminals
        } else {
            &c.start_terminals
        })
        .iter()
        .any(|t| port_positions.contains_key(&t.id))
    };
    let new_ports: IndexSet<_> = w
        .transport_networks
        .iter()
        .flat_map(|n| {
            n.terminals
                .iter()
                .filter(|t| port_positions.contains_key(&t.id))
                .map(|t| (layer(n.transport), Point::from(&t.position)))
        })
        .collect();
    let final_terminals: IndexMap<_, _> = w
        .transport_networks
        .iter()
        .flat_map(|n| {
            n.terminals
                .iter()
                .map(|t| (t.id.clone(), Point::from(&t.position)))
        })
        .collect();
    if port_positions.values().any(|&p| !bounds.contains(p)) {
        return Err(Diagnostic::new(
            "local-move-bounds",
            "Reassigned port leaves the explicit current-extent neighborhood",
            serde_json::Value::Null,
        ));
    }
    if !new_ports.is_empty() {
        for c in chains {
            if changed_port(c, false)
                || changed_port(c, true)
                || paths
                    .get(&c.id)
                    .unwrap_or(&c.cells)
                    .iter()
                    .any(|&p| new_ports.contains(&c.tag(p)))
            {
                paths.shift_remove(&c.id);
                changed.insert(c.id);
            }
        }
    }
    let relocated = |c: &Chain, end: bool| {
        let ts = if end {
            &c.end_terminals
        } else {
            &c.start_terminals
        };
        if ts.len() == 1 {
            final_terminals[&ts[0].id]
        } else if let Some(m) = &movement {
            m.shift_endpoint(c, end)
        } else if end {
            c.end()
        } else {
            c.start()
        }
    };
    let mut jobs: Vec<_> = changed
        .iter()
        .map(|&id| {
            let c = &chains[id];
            let joined = movement.as_ref().is_some_and(|m| m.joined.contains(&id));
            let arm = |p, d: Option<usize>| {
                d.map(|d| {
                    (d + movement
                        .as_ref()
                        .and_then(|m| m.anchor_turns.get(&c.tag(p)))
                        .copied()
                        .unwrap_or_default())
                        % 4
                })
            };
            let mut chain = c.clone();
            chain.zero_length |= joined;
            chain.first =
                if joined || changed_port(c, false) || flexible.contains(&c.tag(c.start())) {
                    None
                } else {
                    arm(c.start(), c.first)
                };
            chain.last = if joined || changed_port(c, true) || flexible.contains(&c.tag(c.end())) {
                None
            } else {
                arm(c.end(), c.last)
            };
            Job {
                chain,
                start: relocated(c, false),
                end: relocated(c, true),
            }
        })
        .collect();
    // Longer obligations get first access to shared corridors; pipes always lead.
    jobs.sort_by_key(|j| {
        (
            1 - layer(j.chain.transport),
            usize::from(priority != Some(j.chain.id)),
            std::cmp::Reverse(j.start.distance(j.end)),
            j.chain.id,
        )
    });
    report.chains = changed.iter().copied().collect();
    report.chains.sort_unstable();
    report.carried_chains = paths.keys().copied().collect();
    report.carried_chains.sort_unstable();
    report.fixed_contacts = region::contacts(&jobs, edit);
    report.route_order = jobs.iter().map(|j| j.chain.id).collect();
    if changed.len() > limits.chains {
        let mut d = Diagnostic::new(
            "region-chain-limit",
            "Initial or expanded repair set exceeds the configured chain limit",
            serde_json::json!({"chains": changed.len(), "limit": limits.chains}),
        );
        d.stage = "local-region-repair";
        return Err(d);
    }
    let body: IndexSet<_> = w.placements.iter().flat_map(body_cells).collect();
    let mut occupied: [IndexSet<Point>; 2] = Default::default();
    for c in chains {
        if !changed.contains(&c.id) {
            occupied[layer(c.transport)]
                .extend(paths.get(&c.id).unwrap_or(&c.cells).iter().copied());
        }
    }
    for n in &w.transport_networks {
        for t in &n.terminals {
            occupied[layer(n.transport)].insert(Point::from(&t.position));
        }
    }
    for c in &w.logistics_components {
        occupied[layer(c.transport)].insert(Point::from(&c.position));
    }
    for j in &jobs {
        occupied[layer(j.chain.transport)].extend([j.start, j.end]);
    }
    let mut reservations: [IndexMap<Point, IndexSet<usize>>; 2] = Default::default();
    for j in &jobs {
        for p in [
            j.chain.first.map(|d| j.start.offset(d)),
            j.chain.last.map(|d| j.end.offset((d + 2) % 4)),
        ]
        .into_iter()
        .flatten()
        {
            reservations[layer(j.chain.transport)]
                .entry(p)
                .or_default()
                .insert(j.chain.id);
        }
    }
    let mut expansions = 0;
    for job in &jobs {
        let c = &job.chain;
        let tr = layer(c.transport);
        let mut blocked = body.clone();
        blocked.extend(occupied[tr].iter().copied());
        for (&p, owners) in &reservations[tr] {
            if !owners.contains(&c.id) {
                blocked.insert(p);
            }
        }
        if body.contains(&job.start) || body.contains(&job.end) {
            return Err(Diagnostic::new(
                "local-endpoint-body",
                "Route endpoint intersects a body",
                c.id,
            ));
        }
        blocked.shift_remove(&job.start);
        blocked.shift_remove(&job.end);
        let mut excluded = body.clone();
        excluded.extend(reservations[tr].keys().copied());
        excluded.extend(
            w.logistics_components
                .iter()
                .filter(|c| layer(c.transport) == tr)
                .map(|c| Point::from(&c.position)),
        );
        excluded.extend(
            w.transport_networks
                .iter()
                .filter(|n| layer(n.transport) == tr)
                .flat_map(|n| n.terminals.iter().map(|t| Point::from(&t.position))),
        );
        excluded.extend(
            jobs.iter()
                .filter(|j| layer(j.chain.transport) == tr)
                .flat_map(|j| [j.start, j.end]),
        );
        let carriers: Vec<_> = if limits.crossings_per_route > 0 {
            chains
                .iter()
                .filter(|c| !changed.contains(&c.id) || paths.contains_key(&c.id))
                .map(|c| {
                    let mut c = c.clone();
                    if let Some(path) = paths.get(&c.id) {
                        c.cells = path.clone();
                    }
                    c
                })
                .collect()
        } else {
            vec![]
        };
        let views: Vec<_> = carriers
            .iter()
            .map(|c| path::Carrier {
                id: c.id,
                network: c.network,
                transport: c.transport,
                rate: c.rate,
                cells: &c.cells,
            })
            .collect();
        let route_job = path::RouteJob {
            id: c.id,
            transport: c.transport,
            rate: c.rate,
        };
        let sites = path::crossing_options(&views, &route_job, &catalogs.components, &excluded);
        let remaining = work_budget.saturating_sub(expansions);
        let result = if remaining == 0 {
            path::PathResult::stopped("work-budget", 0)
        } else {
            path::shortest_path(
                job.start,
                job.end,
                &blocked,
                bounds,
                PathOptions {
                    first: c.first,
                    last: c.last,
                    work_budget: remaining,
                    crossings: &sites,
                    crossing_limit: limits.crossings_per_route,
                    prefer_crossings: false,
                    allow_zero_length: c.zero_length,
                    cell_penalties: None,
                    edge_penalties: None,
                },
            )?
        };
        expansions += result.expanded;
        report.route_expansions = expansions;
        report.routes.push(RouteReport {
            id: c.id,
            item: c.item.clone(),
            first: c.first,
            last: c.last,
            status: result.status,
            expanded: result.expanded,
            preflight_expanded: result.preflight_expanded,
            preflight_status: result.preflight_status,
            old_steps: c.cells.len() - 1,
            steps: result.steps,
            turns: result.turns,
            crossing_sites: sites.len(),
            crossings: result.crossings.clone().unwrap_or_default(),
        });
        if result.status != "found" {
            report.blockers =
                region::blockers(chains, &changed, &paths, job, &result.blocked_frontier);
            report.port_options =
                region::ports(&w, catalogs, edit, &eligible, job, &report.locked_ports)?;
            let mut diagnostic = Diagnostic::new(
                format!("local-{}", result.status),
                "Affected chain could not be restored within this candidate neighborhood",
                c.id,
            );
            diagnostic.severity = "info";
            return Ok(Execution {
                status: "rejected",
                witness: None,
                diagnostics: vec![diagnostic],
                routes: vec![],
                route_expansions: expansions,
                region: None,
            });
        }
        for site in result.crossings.unwrap_or_default() {
            let mut ordinal = w.logistics_components.len();
            let id = loop {
                let id = serde_json::to_string(&serde_json::json!([
                    "local-crossing",
                    c.transport,
                    ordinal
                ]))
                .expect("crossing identity");
                ordinal += 1;
                if !w.logistics_components.iter().any(|c| c.id == id)
                    && !witness.logistics_components.iter().any(|c| c.id == id)
                {
                    break id;
                }
            };
            w.logistics_components.push(PlacedLogisticsComponent {
                id: id.clone(),
                component: site.component,
                kind: LogisticsComponentKind::Bridge,
                transport: site.transport,
                rotation: site.rotation,
                position: site.position.into(),
            });
            for ni in IndexSet::from([site.carrier_network, c.network]) {
                w.transport_networks[ni].component_ids.push(id.clone());
            }
        }
        let cells = result.cells.expect("found path");
        occupied[tr].extend(cells.iter().copied());
        paths.insert(c.id, cells);
    }
    let unchanged_poses = component_changes.iter().all(|(id, p)| {
        original.logistics_components.iter().any(|c| {
            c.id == *id && c.rotation == p.rotation && Point::from(&c.position) == p.position
        })
    });
    if released.is_empty()
        && unchanged_poses
        && facility_changes.is_empty()
        && edit.port_assignments.is_empty()
        && edit.moves.iter().all(|m| m.dx == 0 && m.dy == 0)
        && jobs.iter().all(|j| paths[&j.chain.id] == j.chain.cells)
    {
        return Ok(Execution {
            status: "unchanged",
            witness: None,
            diagnostics: vec![],
            routes: vec![],
            route_expansions: expansions,
            region: None,
        });
    }
    for (ni, n) in w.transport_networks.iter_mut().enumerate() {
        if !chains
            .iter()
            .any(|c| c.network == ni && paths.contains_key(&c.id))
            && !original.transport_networks[ni].terminals.iter().any(|t| {
                movement.as_ref().is_some_and(|m| {
                    m.moved
                        .contains(&(layer(n.transport), Point::from(&t.position)))
                })
            })
        {
            continue;
        }
        let network_chains: Vec<_> = chains.iter().filter(|c| c.network == ni).collect();
        let used: IndexSet<_> = network_chains
            .iter()
            .flat_map(|c| c.cells.iter().copied())
            .collect();
        let mut cells: IndexSet<_> = original.transport_networks[ni]
            .cells
            .iter()
            .map(Point::from)
            .filter(|p| !used.contains(p))
            .map(|p| {
                movement
                    .as_ref()
                    .map_or(p, |m| m.shift_anchor(layer(n.transport), p))
            })
            .collect();
        let mut segments = Vec::new();
        for c in network_chains {
            let ps = paths.get(&c.id).unwrap_or(&c.cells);
            cells.extend(ps.iter().copied());
            segments.extend(ps.windows(2).map(|ps| TransportNetworkSegment {
                from: ps[0].into(),
                to: ps[1].into(),
                rate: c.rate,
            }));
        }
        n.cells = cells.into_iter().map(Into::into).collect();
        n.segments = segments;
    }
    Ok(Execution {
        status: "candidate",
        witness: Some(w),
        diagnostics: vec![],
        routes: vec![],
        route_expansions: expansions,
        region: None,
    })
}
