//! Physical used-boundary realization, preserving real external node identities and rates.
use super::*;
use crate::facilities::FacilityPortEdge;

pub(in crate::layouts::constructive) fn connect_external(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    width: i64,
    height: i64,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    stats: &mut ConstructiveCompositionStatistics,
    limit: u64,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    let mut workspace = RouteWorkspace::new(width, height);
    search(
        node,
        edge,
        edge.rate,
        0,
        width,
        height,
        transports,
        components,
        &mut workspace,
        stats,
        limit,
        accept,
    )
}

fn search(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    remaining: Rate,
    lane: usize,
    width: i64,
    height: i64,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workspace: &mut RouteWorkspace,
    stats: &mut ConstructiveCompositionStatistics,
    limit: u64,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    if remaining.is_zero() {
        let mut n = node.clone();
        n.external_requirements.push(edge.id.clone());
        n.external_requirements.sort();
        n.boundary_requirements.retain(|b| b.requirement != edge.id);
        if refresh(&mut n, transports, components, stats) && accept(&n) {
            return Some(n);
        }
        return None;
    }
    if stats.astar_searches >= limit {
        return None;
    }
    let boundary = node
        .boundary_requirements
        .iter()
        .find(|b| b.requirement == edge.id)?;
    let transport = boundary.transport;
    let capacity = line_capacity(transport, transports)?;
    let supply = boundary.direction == FacilityPortDirection::Input;
    let occupancy = Occupancy::new(node);
    for attachment in attachments(node, boundary, transports, components, &occupancy, stats) {
        let rate = remaining.min(match &attachment {
            Attachment::NewPort { .. } => capacity,
            Attachment::ExistingNetwork {
                residual_capacity, ..
            } => *residual_capacity,
        });
        let point = endpoint(&attachment);
        let mut endpoints = Vec::new();
        for x in 0..width {
            endpoints.push((WorldGridPosition { x, y: 0 }, FacilityPortEdge::North));
            endpoints.push((
                WorldGridPosition { x, y: height - 1 },
                FacilityPortEdge::South,
            ));
        }
        for y in 1..height - 1 {
            endpoints.push((WorldGridPosition { x: 0, y }, FacilityPortEdge::West));
            endpoints.push((
                WorldGridPosition { x: width - 1, y },
                FacilityPortEdge::East,
            ));
        }
        endpoints.sort_by_key(|(p, _)| (p.x.abs_diff(point.x) + p.y.abs_diff(point.y), p.y, p.x));
        let mut blocked = occupancy.facilities.clone();
        blocked.extend(&occupancy.layers[&transport]);
        for b in node
            .boundary_requirements
            .iter()
            .filter(|b| b.transport == transport)
        {
            for a in &b.attachments {
                if let Attachment::NewPort { port } = a {
                    if port.instance == attachment.port().instance
                        && port.port == attachment.port().port
                    {
                        continue;
                    }
                    let dx = port.connection.x - port.position.x;
                    let dy = port.connection.y - port.position.y;
                    for k in 0..3 {
                        blocked.insert((port.connection.x + dx * k, port.connection.y + dy * k));
                    }
                }
            }
        }
        let choices = crossings::options(node, transport, &edge.item, rate, components);
        let axes = choices.iter().map(|(p, (a, _))| (*p, *a)).collect();
        for (external, side) in endpoints {
            if blocked.contains(&xy(&external)) {
                continue;
            }
            if stats.astar_searches >= limit {
                return None;
            }
            stats.astar_searches += 1;
            let (start, goal) = if supply {
                (&external, point)
            } else {
                (point, &external)
            };
            let Some(path) = workspace.route_crossings(&blocked, &axes, start, goal) else {
                stats.astar_failures += 1;
                continue;
            };
            let mut n = node.clone();
            if !n
                .transport_networks
                .iter()
                .any(|n| n.transport == transport && n.item == edge.item)
            {
                n.transport_networks.push(TransportNetwork {
                    id: format!("commodity:{transport:?}:{}", edge.item).to_lowercase(),
                    item: edge.item.clone(),
                    transport,
                    cells: vec![],
                    segments: vec![],
                    terminals: vec![],
                    component_ids: vec![],
                    requirement_ids: vec![],
                });
            }
            let network = n
                .transport_networks
                .iter_mut()
                .find(|n| n.transport == transport && n.item == edge.item)?;
            attach(
                network,
                &attachment,
                rate,
                !supply,
                edge,
                lane,
                &mut n.logistics_components,
            )?;
            let external_node = if supply { &edge.source } else { &edge.target };
            network.terminals.push(TransportNetworkTerminal {
                id: format!("{}:external:{lane}", edge.id),
                node: external_node.clone(),
                direction: if supply {
                    FacilityPortDirection::Output
                } else {
                    FacilityPortDirection::Input
                },
                endpoint: TransportNetworkEndpoint::External {
                    node: external_node.clone(),
                    side,
                },
                position: external,
                rate,
            });
            network.cells.extend(path.iter().cloned());
            network.cells.sort_by_key(xy);
            network.cells.dedup();
            network
                .segments
                .extend(path.windows(2).map(|p| TransportNetworkSegment {
                    from: p[0].clone(),
                    to: p[1].clone(),
                    rate,
                }));
            network.requirement_ids.push(edge.id.clone());
            network.requirement_ids.sort();
            network.requirement_ids.dedup();
            let kind = if matches!(attachment, Attachment::NewPort { .. }) {
                "new-port"
            } else {
                "existing-network"
            };
            n.attachment_events
                .push(super::super::ConstructiveAttachmentEvent {
                    requirement: edge.id.clone(),
                    rate,
                    network: network.id.clone(),
                    source_kind: if supply { "external" } else { kind },
                    target_kind: if supply { kind } else { "external" },
                    component_ids: vec![],
                });
            crossings::install(&mut n, transport, &edge.item, &path, &choices);
            if let Some(n) = search(
                &n,
                edge,
                remaining.checked_sub(rate).ok()?,
                lane + 1,
                width,
                height,
                transports,
                components,
                workspace,
                stats,
                limit,
                accept,
            ) {
                return Some(n);
            }
        }
    }
    None
}
