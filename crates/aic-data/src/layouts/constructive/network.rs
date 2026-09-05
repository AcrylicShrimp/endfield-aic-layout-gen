//! Commodity-network attachment and independent constructive witness validation.
use super::{ProfileOp, profile_span};
pub(super) mod crossings;
mod external;
mod first;
mod paths;
pub(super) use external::connect_external;
#[cfg(test)]
mod tests;
mod validation;
pub(super) use first::FUTURE_PORT_ESCAPE_CELLS;
pub(super) use first::connect_first;
pub(super) use first::{
    NearestPortSearch, PORT_PRESERVATION_VARIANTS, PortDomainGuard, PortPairTrial, connect_nearest,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};
pub(super) use validation::validate;

use super::first_pipe_frontier::occupied_cells;
use super::routing::RouteWorkspace;
use super::{
    ConstructiveBoundaryAttachment as Attachment, ConstructiveCompositionStatistics,
    ConstructiveFrontierDiagnostic, ConstructiveNode, ConstructiveProcessModuleBoundary,
};
use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    PlacedFacilityPort, PlacedLogisticsComponent, TransportNetwork, TransportNetworkEndpoint,
    TransportNetworkSegment, TransportNetworkTerminal, WorldGridPosition,
};
use crate::logistics::{
    CardinalDirection, LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::{FacilityInstanceWiringEdge, Rate};

fn xy(p: &WorldGridPosition) -> (i64, i64) {
    (p.x, p.y)
}
fn delta(d: CardinalDirection) -> (i64, i64) {
    match d {
        CardinalDirection::North => (0, -1),
        CardinalDirection::East => (1, 0),
        CardinalDirection::South => (0, 1),
        CardinalDirection::West => (-1, 0),
    }
}
fn rotate(d: CardinalDirection, r: i64) -> CardinalDirection {
    let all = [
        CardinalDirection::North,
        CardinalDirection::East,
        CardinalDirection::South,
        CardinalDirection::West,
    ];
    all[(all.iter().position(|v| *v == d).unwrap() + r as usize / 90) % 4]
}
fn direction(a: &WorldGridPosition, b: &WorldGridPosition) -> Option<CardinalDirection> {
    match (b.x - a.x, b.y - a.y) {
        (0, -1) => Some(CardinalDirection::North),
        (1, 0) => Some(CardinalDirection::East),
        (0, 1) => Some(CardinalDirection::South),
        (-1, 0) => Some(CardinalDirection::West),
        _ => None,
    }
}
fn line_capacity(t: TransportKind, transports: &ValidatedTransportCatalog) -> Option<Rate> {
    let c = transports.capacity(t);
    Rate::from_quantity_per_duration_ms(c.quantity, c.duration_ms).ok()
}
fn component_capacity(c: &crate::logistics::LogisticsComponentDefinition) -> Option<Rate> {
    Rate::from_quantity_per_duration_ms(c.capacity.quantity, c.capacity.duration_ms).ok()
}

pub(super) fn normalize(networks: &mut Vec<TransportNetwork>) {
    let mut groups = BTreeMap::<(TransportKind, String), TransportNetwork>::new();
    for mut n in std::mem::take(networks) {
        let key = (n.transport, n.item.clone());
        n.id = format!("commodity:{:?}:{}", n.transport, n.item).to_lowercase();
        if let Some(g) = groups.get_mut(&key) {
            g.cells.extend(n.cells);
            g.segments.extend(n.segments);
            g.terminals.extend(n.terminals);
            g.requirement_ids.extend(n.requirement_ids);
            g.component_ids.extend(n.component_ids);
        } else {
            groups.insert(key, n);
        }
    }
    for n in groups.values_mut() {
        n.cells.sort_by_key(xy);
        n.cells.dedup();
        n.requirement_ids.sort();
        n.requirement_ids.dedup();
        n.component_ids.sort();
        n.component_ids.dedup();
    }
    *networks = groups.into_values().collect();
}

pub(super) fn report_capacity(
    node: &mut ConstructiveNode,
    transports: &ValidatedTransportCatalog,
) -> Result<(), ConstructiveFrontierDiagnostic> {
    let _span = profile_span(ProfileOp::CapacityReport);
    node.network_capacity = node
        .transport_networks
        .iter()
        .map(|n| {
            let failure = || {
                ConstructiveFrontierDiagnostic::error(
                    "constructive-network-attachment-capacity-insufficient",
                    "/network_capacity",
                    Some(n.id.clone()),
                    "network load cannot be represented within runtime line capacity",
                )
            };
            let capacity = line_capacity(n.transport, transports).ok_or_else(failure)?;
            let segments = n
                .segments
                .iter()
                .map(|s| {
                    Ok(super::ConstructiveSegmentCapacity {
                        from: s.from.clone(),
                        to: s.to.clone(),
                        load: s.rate,
                        residual: capacity.checked_sub(s.rate).map_err(|_| failure())?,
                    })
                })
                .collect::<Result<Vec<_>, ConstructiveFrontierDiagnostic>>()?;
            Ok(super::ConstructiveNetworkCapacity {
                network: n.id.clone(),
                line_capacity: capacity,
                segments,
            })
        })
        .collect::<Result<Vec<_>, ConstructiveFrontierDiagnostic>>()?;
    Ok(())
}

fn arms(
    n: &TransportNetwork,
    p: &WorldGridPosition,
) -> (Vec<CardinalDirection>, Vec<CardinalDirection>, Rate) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut load = Rate::zero();
    for s in &n.segments {
        if s.to == *p {
            if let Some(d) = direction(p, &s.from) {
                inputs.push(d);
                load = load.checked_add(s.rate).unwrap_or(Rate {
                    numerator: i64::MAX,
                    denominator: 1,
                });
            }
        }
        if s.from == *p {
            if let Some(d) = direction(p, &s.to) {
                outputs.push(d);
            }
        }
    }
    (inputs, outputs, load)
}

struct Occupancy {
    facilities: HashSet<(i64, i64)>,
    layers: BTreeMap<TransportKind, HashSet<(i64, i64)>>,
    ports: BTreeSet<(String, String)>,
}

impl Occupancy {
    fn new(node: &ConstructiveNode) -> Self {
        let mut layers = BTreeMap::from([
            (TransportKind::Belt, HashSet::new()),
            (TransportKind::Pipe, HashSet::new()),
        ]);
        let mut ports = BTreeSet::new();
        for n in &node.transport_networks {
            layers
                .get_mut(&n.transport)
                .unwrap()
                .extend(n.cells.iter().map(xy));
            for t in &n.terminals {
                if let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint {
                    ports.insert((instance.clone(), port.clone()));
                }
            }
        }
        Self {
            facilities: occupied_cells(&node.placements),
            layers,
            ports,
        }
    }
}

fn attachments(
    node: &ConstructiveNode,
    b: &ConstructiveProcessModuleBoundary,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    occupancy: &Occupancy,
    stats: &mut ConstructiveCompositionStatistics,
) -> Vec<Attachment> {
    let _span = profile_span(ProfileOp::AttachmentEnumeration);
    let Some(capacity) = line_capacity(b.transport, transports) else {
        return Vec::new();
    };
    let facilities = &occupancy.facilities;
    let occupied = &occupancy.layers[&b.transport];
    let mut ports = BTreeMap::new();
    for a in &b.attachments {
        ports.insert(a.port().port.clone(), a.port().clone());
    }
    let mut result = Vec::new();
    for port in ports.into_values() {
        let used = occupancy
            .ports
            .contains(&(port.instance.clone(), port.port.clone()));
        if !used
            && !facilities.contains(&xy(&port.connection))
            && !occupied.contains(&xy(&port.connection))
        {
            result.push(Attachment::NewPort { port: port.clone() });
        }
    }
    let kind = if b.direction == FacilityPortDirection::Output {
        LogisticsComponentKind::Splitter
    } else {
        LogisticsComponentKind::Converger
    };
    for n in node
        .transport_networks
        .iter()
        .filter(|n| n.item == b.item && n.transport == b.transport)
    {
        let mut paths = None;
        for terminal in n
            .terminals
            .iter()
            .filter(|t| t.node == b.inside_instance && t.direction == b.direction)
        {
            // Every attachment increases this port's aggregate flow, regardless of anchor.
            // Reject a saturated terminal before repeating directed-path searches for its cells.
            let Some(terminal_residual) = capacity
                .checked_sub(terminal.rate)
                .ok()
                .filter(|r| !r.is_zero())
            else {
                stats.attachment_capacity_rejections += 1;
                continue;
            };
            let TransportNetworkEndpoint::Facility { port: port_id, .. } = &terminal.endpoint
            else {
                continue;
            };
            // Ports used by a prior attachment remain part of the discriminated boundary contract.
            // Fresh modules provide every compatible port before this function filters them.
            let Some(port) = b
                .attachments
                .iter()
                .map(Attachment::port)
                .find(|p| p.port == *port_id)
            else {
                continue;
            };
            let paths = paths.get_or_insert_with(|| paths::DirectedPaths::new(&n.segments));
            for anchor in &n.cells {
                stats.attachment_anchors_considered += 1;
                if n.terminals.iter().any(|t| t.position == *anchor) {
                    continue;
                }
                let path = if b.direction == FacilityPortDirection::Output {
                    paths.path(&terminal.position, anchor)
                } else {
                    paths.path(anchor, &terminal.position)
                };
                let Some(path) = path else {
                    continue;
                };
                let (inputs, outputs, load) = arms(n, anchor);
                let mut residual = Some(terminal_residual);
                for i in &path {
                    residual = residual.and_then(|r| {
                        capacity
                            .checked_sub(n.segments[*i].rate)
                            .ok()
                            .map(|s| r.min(s))
                    });
                }
                // Every component along the shared path must have aggregate capacity, including
                // unrelated flow on its other arms.
                for placed in node.logistics_components.iter().filter(|c| {
                    c.transport == b.transport
                        && path.iter().any(|i| {
                            n.segments[*i].from == c.position || n.segments[*i].to == c.position
                        })
                }) {
                    residual = residual.and_then(|r| {
                        let def = components.component(&placed.component)?;
                        let (_, _, l) = arms(n, &placed.position);
                        component_capacity(def)?
                            .checked_sub(l)
                            .ok()
                            .map(|s| r.min(s))
                    });
                }
                let Some(residual) = residual.filter(|r| !r.is_zero()) else {
                    stats.attachment_capacity_rejections += 1;
                    continue;
                };
                let existing = node
                    .logistics_components
                    .iter()
                    .find(|c| c.transport == b.transport && c.position == *anchor);
                for def in components
                    .catalog()
                    .components
                    .iter()
                    .filter(|c| c.kind == kind && c.transport == b.transport)
                {
                    if existing.is_some_and(|c| c.component != def.id) {
                        continue;
                    }
                    for &rotation in &def.allowed_rotations {
                        if existing.is_some_and(|c| c.rotation != rotation) {
                            continue;
                        }
                        let allowed_in = def
                            .input_directions
                            .iter()
                            .map(|&d| rotate(d, rotation))
                            .collect::<Vec<_>>();
                        let allowed_out = def
                            .output_directions
                            .iter()
                            .map(|&d| rotate(d, rotation))
                            .collect::<Vec<_>>();
                        if !inputs.iter().all(|d| allowed_in.contains(d))
                            || !outputs.iter().all(|d| allowed_out.contains(d))
                        {
                            stats.component_topology_rejections += 1;
                            continue;
                        }
                        let Some(component_residual) =
                            component_capacity(def).and_then(|c| c.checked_sub(load).ok())
                        else {
                            continue;
                        };
                        let residual = residual.min(component_residual);
                        if residual.is_zero() {
                            stats.attachment_capacity_rejections += 1;
                            continue;
                        }
                        let free = if kind == LogisticsComponentKind::Splitter {
                            &allowed_out
                        } else {
                            &allowed_in
                        };
                        for d in free {
                            if inputs.contains(d) || outputs.contains(d) {
                                continue;
                            }
                            let (dx, dy) = delta(*d);
                            let branch_cell = WorldGridPosition {
                                x: anchor.x + dx,
                                y: anchor.y + dy,
                            };
                            if occupied.contains(&xy(&branch_cell))
                                || facilities.contains(&xy(&branch_cell))
                            {
                                continue;
                            }
                            if kind == LogisticsComponentKind::Splitter {
                                stats.splitter_candidates_considered += 1;
                            } else {
                                stats.converger_candidates_considered += 1;
                            }
                            result.push(Attachment::ExistingNetwork {
                                port: port.clone(),
                                network: n.id.clone(),
                                terminal: terminal.id.clone(),
                                anchor: anchor.clone(),
                                branch_cell,
                                component: existing.cloned().unwrap_or_else(|| {
                                    PlacedLogisticsComponent {
                                        id: format!(
                                            "branch:{}:{}:{}",
                                            terminal.id, anchor.x, anchor.y
                                        ),
                                        component: def.id.clone(),
                                        kind,
                                        transport: b.transport,
                                        position: anchor.clone(),
                                        rotation,
                                    }
                                }),
                                shared_segments: path.clone(),
                                residual_capacity: residual,
                            });
                        }
                    }
                }
            }
        }
    }
    result
}

pub(super) fn refresh(
    node: &mut ConstructiveNode,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    stats: &mut ConstructiveCompositionStatistics,
) -> bool {
    let _span = profile_span(ProfileOp::BoundaryRefresh);
    let occupancy = Occupancy::new(node);
    let updated = node
        .boundary_requirements
        .iter()
        .map(|b| attachments(node, b, transports, components, &occupancy, stats))
        .collect::<Vec<_>>();
    for (b, a) in node.boundary_requirements.iter_mut().zip(updated) {
        b.attachments = a;
        if b.attachments.is_empty() {
            *stats
                .boundary_requirement_rejections
                .entry(b.requirement.clone())
                .or_default() += 1;
        }
    }
    node.boundary_requirements
        .iter()
        .all(|b| !b.attachments.is_empty())
}

fn endpoint(a: &Attachment) -> &WorldGridPosition {
    match a {
        Attachment::NewPort { port } => &port.connection,
        Attachment::ExistingNetwork { branch_cell, .. } => branch_cell,
    }
}

fn attach(
    n: &mut TransportNetwork,
    a: &Attachment,
    rate: Rate,
    source: bool,
    edge: &FacilityInstanceWiringEdge,
    lane: usize,
    components: &mut Vec<PlacedLogisticsComponent>,
) -> Option<()> {
    match a {
        Attachment::NewPort { port } => n.terminals.push(TransportNetworkTerminal {
            id: format!(
                "{}:lane:{lane}:{}",
                edge.id,
                if source { "source" } else { "target" }
            ),
            node: port.instance.clone(),
            direction: port.direction,
            endpoint: TransportNetworkEndpoint::Facility {
                instance: port.instance.clone(),
                port: port.port.clone(),
            },
            position: port.connection.clone(),
            rate,
        }),
        Attachment::ExistingNetwork {
            terminal,
            anchor,
            branch_cell,
            component,
            shared_segments,
            residual_capacity,
            ..
        } => {
            if rate > *residual_capacity {
                return None;
            }
            for i in shared_segments {
                n.segments[*i].rate = n.segments[*i].rate.checked_add(rate).ok()?;
            }
            let t = n.terminals.iter_mut().find(|t| t.id == *terminal)?;
            t.rate = t.rate.checked_add(rate).ok()?;
            n.segments.push(TransportNetworkSegment {
                from: if source {
                    anchor.clone()
                } else {
                    branch_cell.clone()
                },
                to: if source {
                    branch_cell.clone()
                } else {
                    anchor.clone()
                },
                rate,
            });
            if !components.iter().any(|c| c.id == component.id) {
                components.push(component.clone());
                n.component_ids.push(component.id.clone());
            }
        }
    }
    Some(())
}

pub(super) fn connect(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transport: TransportKind,
    lane_rates: &[Rate],
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workspace: &mut RouteWorkspace,
    stats: &mut ConstructiveCompositionStatistics,
) -> Vec<ConstructiveNode> {
    let mut results = Vec::new();
    connect_lane(
        node,
        edge,
        transport,
        lane_rates,
        0,
        transports,
        components,
        workspace,
        stats,
        &mut results,
    );
    results
}

fn connect_lane(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transport: TransportKind,
    rates: &[Rate],
    lane: usize,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workspace: &mut RouteWorkspace,
    stats: &mut ConstructiveCompositionStatistics,
    results: &mut Vec<ConstructiveNode>,
) {
    if lane == rates.len() {
        stats.completed_lane_bundles += 1;
        let mut candidate = node.clone();
        candidate.internal_requirements.push(edge.id.clone());
        candidate.internal_requirements.sort();
        candidate.internal_requirements.dedup();
        candidate
            .boundary_requirements
            .retain(|b| b.requirement != edge.id);
        if !refresh(&mut candidate, transports, components, stats) {
            stats.boundary_option_dead_ends_rejected += 1;
            stats.boundary_dead_ends_rejected += 1;
            return;
        }
        if !super::analyze_constructive_port_demands(
            &candidate.boundary_requirements,
            &candidate.transport_networks,
            transports,
        )
        .success
        {
            stats.boundary_capacity_dead_ends_rejected += 1;
            stats.boundary_dead_ends_rejected += 1;
            return;
        }
        results.push(candidate);
        return;
    }
    let Some(sb) = node
        .boundary_requirements
        .iter()
        .find(|b| b.requirement == edge.id && b.direction == FacilityPortDirection::Output)
    else {
        return;
    };
    let Some(tb) = node
        .boundary_requirements
        .iter()
        .find(|b| b.requirement == edge.id && b.direction == FacilityPortDirection::Input)
    else {
        return;
    };
    let occupancy = Occupancy::new(node);
    let sources = attachments(node, sb, transports, components, &occupancy, stats);
    let targets = attachments(node, tb, transports, components, &occupancy, stats);
    let mut blocked = occupancy.facilities;
    blocked.extend(occupancy.layers[&transport].iter().copied());
    for source in &sources {
        for target in &targets {
            stats.port_pairs_considered += 1;
            let shared = matches!(source, Attachment::ExistingNetwork { .. })
                || matches!(target, Attachment::ExistingNetwork { .. });
            if shared {
                stats.existing_network_attempts += 1;
            } else {
                stats.new_port_attempts += 1;
            }
            let insufficient_capacity = [source, target].iter().any(|a| {
                matches!(a, Attachment::ExistingNetwork { residual_capacity, .. }
                    if *residual_capacity < rates[lane])
            });
            if insufficient_capacity {
                stats.attachment_capacity_rejections += 1;
                continue;
            }
            if blocked.contains(&xy(endpoint(source))) || blocked.contains(&xy(endpoint(target))) {
                stats.blocked_port_pairs_rejected += 1;
                continue;
            }
            stats.astar_searches += 1;
            let Some(path) = workspace.route(&blocked, endpoint(source), endpoint(target)) else {
                stats.astar_failures += 1;
                if shared {
                    stats.branch_route_failures += 1;
                }
                continue;
            };
            let mut next = node.clone();
            if !next
                .transport_networks
                .iter()
                .any(|n| n.transport == transport && n.item == edge.item)
            {
                next.transport_networks.push(TransportNetwork {
                    id: format!("commodity:{transport:?}:{}", edge.item).to_lowercase(),
                    requirement_ids: Vec::new(),
                    item: edge.item.clone(),
                    transport,
                    cells: Vec::new(),
                    segments: Vec::new(),
                    terminals: Vec::new(),
                    component_ids: Vec::new(),
                });
            }
            let n = next
                .transport_networks
                .iter_mut()
                .find(|n| n.transport == transport && n.item == edge.item)
                .unwrap();
            if attach(
                n,
                source,
                rates[lane],
                true,
                edge,
                lane,
                &mut next.logistics_components,
            )
            .is_none()
                || attach(
                    n,
                    target,
                    rates[lane],
                    false,
                    edge,
                    lane,
                    &mut next.logistics_components,
                )
                .is_none()
            {
                stats.attachment_capacity_rejections += 1;
                continue;
            }
            n.requirement_ids.push(edge.id.clone());
            n.requirement_ids.sort();
            n.requirement_ids.dedup();
            n.segments
                .extend(path.windows(2).map(|p| TransportNetworkSegment {
                    from: p[0].clone(),
                    to: p[1].clone(),
                    rate: rates[lane],
                }));
            n.cells.extend(path);
            n.cells.sort_by_key(xy);
            n.cells.dedup();
            next.attachment_events
                .push(super::ConstructiveAttachmentEvent {
                    requirement: edge.id.clone(),
                    rate: rates[lane],
                    network: n.id.clone(),
                    source_kind: if matches!(source, Attachment::NewPort { .. }) {
                        "new-port"
                    } else {
                        "existing-network"
                    },
                    target_kind: if matches!(target, Attachment::NewPort { .. }) {
                        "new-port"
                    } else {
                        "existing-network"
                    },
                    component_ids: [source, target]
                        .iter()
                        .filter_map(|a| match a {
                            Attachment::ExistingNetwork { component, .. } => {
                                Some(component.id.clone())
                            }
                            _ => None,
                        })
                        .collect(),
                });
            // Keep all original port identities while routing a multi-lane transaction. Refresh only
            // at commit so the next lane cannot accidentally reuse a now-occupied new port.
            connect_lane(
                &next,
                edge,
                transport,
                rates,
                lane + 1,
                transports,
                components,
                workspace,
                stats,
                results,
            );
        }
    }
}

pub(super) fn turns(n: &TransportNetwork) -> usize {
    n.cells
        .iter()
        .map(|p| {
            let (i, o, _) = arms(n, p);
            i.iter()
                .flat_map(|a| o.iter().map(move |b| (a, b)))
                .filter(|(a, b)| rotate(**a, 180) != **b)
                .count()
        })
        .sum()
}
