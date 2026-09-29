//! Bounded first-feasible connection search. No area incumbent or completed-bundle collection.
use super::*;

/// Constructive reservation policy, not a required game clearance.
pub(in crate::layouts::constructive) const FUTURE_PORT_ESCAPE_CELLS: i64 = 3;
pub(in crate::layouts::constructive) const PORT_PRESERVATION_VARIANTS: usize = 8;

/// One physical search in a nearest-port, atomic requirement transaction.
#[derive(Debug, serde::Serialize)]
pub struct PortPairTrial {
    pub lane: usize,
    pub parent_trial: Option<usize>,
    pub source_port: String,
    pub target_port: String,
    pub distance: i64,
    pub rate: Rate,
    pub straight: bool,
    pub connected: bool,
    pub astar_searches: u64,
    pub preservation_conflict: Option<String>,
    pub avoided_cells: Vec<WorldGridPosition>,
}

pub(in crate::layouts::constructive) struct PortDomainGuard {
    pub requirement: String,
    pub cells: Vec<WorldGridPosition>,
}

/// Unassigned alternatives are choices, not simultaneous escape-corridor reservations.
#[derive(Default)]
pub(in crate::layouts::constructive) struct NearestPortSearch {
    pub trials: Vec<PortPairTrial>,
    pub guards: Vec<PortDomainGuard>,
}

pub(in crate::layouts::constructive) fn connect_nearest(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workspace: &mut RouteWorkspace,
    stats: &mut ConstructiveCompositionStatistics,
    limit: u64,
    nearest: &mut NearestPortSearch,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    let mut stop = ConnectionSearchStop::default();
    search(
        node,
        edge,
        edge.rate,
        0,
        transports,
        components,
        workspace,
        stats,
        limit,
        Some(nearest),
        None,
        &mut stop,
        accept,
    )
}

pub(in crate::layouts::constructive) fn connect_first(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workspace: &mut RouteWorkspace,
    stats: &mut ConstructiveCompositionStatistics,
    limit: u64,
    stop: &mut ConnectionSearchStop,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    search(
        node, edge, edge.rate, 0, transports, components, workspace, stats, limit, None, None,
        stop, accept,
    )
}

fn search(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    remaining: Rate,
    lane: usize,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workspace: &mut RouteWorkspace,
    stats: &mut ConstructiveCompositionStatistics,
    limit: u64,
    mut nearest: Option<&mut NearestPortSearch>,
    parent_trial: Option<usize>,
    stop: &mut ConnectionSearchStop,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    if crate::work_control::stopped() {
        stop.interrupted();
        return None;
    }
    if remaining.is_zero() {
        stats.completed_lane_bundles += 1;
        let mut candidate = node.clone();
        candidate.internal_requirements.push(edge.id.clone());
        candidate.internal_requirements.sort();
        candidate.internal_requirements.dedup();
        candidate
            .boundary_requirements
            .retain(|b| b.requirement != edge.id);
        if !refresh(&mut candidate, transports, components, stats) || !accept(&candidate) {
            stats.boundary_dead_ends_rejected += 1;
            return None;
        }
        return Some(candidate);
    }
    // Each recursive level spends an A* call; the connection budget also bounds stack depth.
    if limit == 0 {
        stop.budget();
        return None;
    }
    let source_boundary = node
        .boundary_requirements
        .iter()
        .find(|b| b.requirement == edge.id && b.direction == FacilityPortDirection::Output)?;
    let target_boundary = node
        .boundary_requirements
        .iter()
        .find(|b| b.requirement == edge.id && b.direction == FacilityPortDirection::Input)?;
    let transport = source_boundary.transport;
    let capacity = line_capacity(transport, transports)?;
    let occupancy = Occupancy::new(node);
    let sources = attachments(
        node,
        source_boundary,
        transports,
        components,
        &occupancy,
        stats,
    );
    let targets = attachments(
        node,
        target_boundary,
        transports,
        components,
        &occupancy,
        stats,
    );
    let mut pairs = sources
        .iter()
        .flat_map(|a| targets.iter().map(move |b| (a, b)))
        .collect::<Vec<_>>();
    let distance = |a: &Attachment, b: &Attachment| {
        (a.port().connection.x - b.port().connection.x).abs()
            + (a.port().connection.y - b.port().connection.y).abs()
    };
    if nearest.is_some() {
        pairs.sort_by_key(|(a, b)| (distance(a, b), a.port().port.clone(), b.port().port.clone()));
    } else {
        // Reuse residual network capacity before claiming another physical port.
        pairs.sort_by_key(|(a, b)| {
            usize::from(matches!(a, Attachment::NewPort { .. }))
                + usize::from(matches!(b, Attachment::NewPort { .. }))
        });
    }
    for (source, target) in pairs {
        let available = |a: &Attachment| match a {
            Attachment::NewPort { .. } => capacity,
            Attachment::ExistingNetwork {
                residual_capacity, ..
            } => *residual_capacity,
        };
        let rate = remaining.min(available(source)).min(available(target));
        if rate.is_zero() {
            continue;
        }
        if stats.astar_searches >= limit {
            stop.budget();
            return None;
        }
        let mut blocked = occupancy.facilities.clone();
        blocked.extend(&occupancy.layers[&transport]);
        // Only a singleton future port is committed; other attachments are alternatives.
        for b in node
            .boundary_requirements
            .iter()
            .filter(|b| nearest.is_none() && b.transport == transport && b.attachments.len() == 1)
        {
            for a in &b.attachments {
                let Attachment::NewPort { port } = a else {
                    continue;
                };
                if [source, target].iter().any(|selected| {
                    selected.port().instance == port.instance && selected.port().port == port.port
                }) {
                    continue;
                }
                let dx = port.connection.x - port.position.x;
                let dy = port.connection.y - port.position.y;
                for distance in 0..FUTURE_PORT_ESCAPE_CELLS {
                    blocked.insert((
                        port.connection.x + dx * distance,
                        port.connection.y + dy * distance,
                    ));
                }
            }
        }
        let shared = matches!(source, Attachment::ExistingNetwork { .. })
            || matches!(target, Attachment::ExistingNetwork { .. });
        let choices = crossings::options(node, transport, rate, components);
        let axes = choices.iter().map(|(p, (axis, _))| (*p, *axis)).collect();
        let mut variants = std::collections::VecDeque::from([BTreeSet::<(i64, i64)>::new()]);
        let mut seen = BTreeSet::from([BTreeSet::<(i64, i64)>::new()]);
        let mut routed = None;
        for _ in 0..PORT_PRESERVATION_VARIANTS {
            let Some(avoided) = variants.pop_front() else {
                break;
            };
            if stats.astar_searches >= limit {
                stop.budget();
                break;
            }
            stats.port_pairs_considered += 1;
            if shared {
                stats.existing_network_attempts += 1;
            } else {
                stats.new_port_attempts += 1;
            }
            stats.astar_searches += 1;
            let trial_index = nearest.as_deref_mut().map(|trace| {
                let index = trace.trials.len();
                trace.trials.push(PortPairTrial {
                    lane,
                    parent_trial,
                    source_port: source.port().port.clone(),
                    target_port: target.port().port.clone(),
                    distance: distance(source, target),
                    rate,
                    straight: false,
                    connected: false,
                    astar_searches: 1,
                    preservation_conflict: None,
                    avoided_cells: avoided
                        .iter()
                        .map(|&(x, y)| WorldGridPosition { x, y })
                        .collect(),
                });
                index
            });
            let mut mask = blocked.clone();
            mask.extend(&avoided);
            let Some(path) =
                workspace.route_crossings(&mask, &axes, endpoint(source), endpoint(target))
            else {
                stats.astar_failures += 1;
                if shared {
                    stats.branch_route_failures += 1;
                }
                continue;
            };
            if let (Some(trace), Some(index)) = (nearest.as_deref_mut(), trial_index) {
                trace.trials[index].straight = !shared
                    && (path.iter().all(|p| p.x == path[0].x)
                        || path.iter().all(|p| p.y == path[0].y));
                let path_cells = path.iter().map(xy).collect::<HashSet<_>>();
                let lost = trace.guards.iter().find_map(|guard| {
                    let free = guard
                        .cells
                        .iter()
                        .map(xy)
                        .filter(|p| !occupancy.layers[&transport].contains(p))
                        .collect::<BTreeSet<_>>();
                    (!free.is_empty() && free.iter().all(|p| path_cells.contains(p)))
                        .then(|| (guard.requirement.clone(), free))
                });
                if let Some((requirement, free)) = lost {
                    trace.trials[index].preservation_conflict = Some(requirement);
                    // Branch on preserving one approach, not reserving every alternative.
                    for cell in free {
                        let mut variant = avoided.clone();
                        variant.insert(cell);
                        if seen.insert(variant.clone()) {
                            variants.push_back(variant);
                        }
                    }
                    continue;
                }
            }
            routed = Some((path, trial_index));
            break;
        }
        let Some((path, trial_index)) = routed else {
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
            .find(|n| n.transport == transport && n.item == edge.item)?;
        if attach(
            n,
            source,
            rate,
            true,
            edge,
            lane,
            &mut next.logistics_components,
        )
        .is_none()
            || attach(
                n,
                target,
                rate,
                false,
                edge,
                lane,
                &mut next.logistics_components,
            )
            .is_none()
        {
            continue;
        }
        n.requirement_ids.push(edge.id.clone());
        n.requirement_ids.sort();
        n.requirement_ids.dedup();
        n.segments
            .extend(path.windows(2).map(|p| TransportNetworkSegment {
                from: p[0].clone(),
                to: p[1].clone(),
                rate,
            }));
        n.cells.extend(path.iter().cloned());
        n.cells.sort_by_key(xy);
        n.cells.dedup();
        next.attachment_events
            .push(super::super::ConstructiveAttachmentEvent {
                requirement: edge.id.clone(),
                rate,
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
                        Attachment::ExistingNetwork { component, .. } => Some(component.id.clone()),
                        _ => None,
                    })
                    .collect(),
            });
        crossings::install(&mut next, transport, &edge.item, &path, &choices);
        if let Some(result) = search(
            &next,
            edge,
            remaining.checked_sub(rate).ok()?,
            lane + 1,
            transports,
            components,
            workspace,
            stats,
            limit,
            nearest.as_deref_mut(),
            trial_index,
            stop,
            accept,
        ) {
            if let (Some(trace), Some(index)) = (nearest.as_deref_mut(), trial_index) {
                trace.trials[index].connected = true;
            }
            if shared {
                stats.network_attachment_candidates_validated += 1;
            }
            return Some(result);
        }
    }
    if crate::work_control::stopped() {
        stop.interrupted();
    }
    None
}
