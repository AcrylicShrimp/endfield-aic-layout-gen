//! Bounded first-feasible connection search. No area incumbent or completed-bundle collection.
use super::*;

/// Constructive reservation policy, not a required game clearance.
pub(in crate::layouts::constructive) const FUTURE_PORT_ESCAPE_CELLS: i64 = 3;

pub(in crate::layouts::constructive) fn connect_first(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workspace: &mut RouteWorkspace,
    stats: &mut ConstructiveCompositionStatistics,
    limit: u64,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    search(
        node, edge, edge.rate, 0, transports, components, workspace, stats, limit, accept,
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
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
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
    if stats.astar_searches >= limit {
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
    for source in &sources {
        for target in &targets {
            if stats.astar_searches >= limit {
                return None;
            }
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
            let mut blocked = occupancy.facilities.clone();
            blocked.extend(&occupancy.layers[&transport]);
            // Concrete assigned future port domains, including short outward escape corridors.
            for b in node
                .boundary_requirements
                .iter()
                .filter(|b| b.transport == transport)
            {
                for a in &b.attachments {
                    let Attachment::NewPort { port } = a else {
                        continue;
                    };
                    if [source, target].iter().any(|selected| {
                        selected.port().instance == port.instance
                            && selected.port().port == port.port
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
            stats.port_pairs_considered += 1;
            let shared = matches!(source, Attachment::ExistingNetwork { .. })
                || matches!(target, Attachment::ExistingNetwork { .. });
            if shared {
                stats.existing_network_attempts += 1;
            } else {
                stats.new_port_attempts += 1;
            }
            stats.astar_searches += 1;
            let choices = crossings::options(node, transport, &edge.item, rate, components);
            let axes = choices.iter().map(|(p, (axis, _))| (*p, *axis)).collect();
            let Some(path) =
                workspace.route_crossings(&blocked, &axes, endpoint(source), endpoint(target))
            else {
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
                            Attachment::ExistingNetwork { component, .. } => {
                                Some(component.id.clone())
                            }
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
                accept,
            ) {
                if shared {
                    stats.network_attachment_candidates_validated += 1;
                }
                return Some(result);
            }
        }
    }
    None
}
