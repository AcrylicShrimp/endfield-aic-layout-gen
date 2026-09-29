//! Physical used-boundary realization, preserving real external node identities and rates.
use super::*;
use crate::facilities::FacilityPortEdge;
use std::{borrow::Cow, collections::VecDeque};

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_CHILD_ADMISSION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(in crate::layouts::constructive) fn fail_next_child_admission_for_test() {
    FAIL_NEXT_CHILD_ADMISSION.with(|fail| fail.set(true));
}

fn reserve_child<T>(queue: &mut VecDeque<T>) -> Result<(), ()> {
    #[cfg(test)]
    if FAIL_NEXT_CHILD_ADMISSION.with(|fail| fail.replace(false)) {
        return Err(());
    }
    queue.try_reserve(1).map_err(|_| ())
}

pub(in crate::layouts::constructive) fn connect_external(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    width: i64,
    height: i64,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    stats: &mut ConstructiveCompositionStatistics,
    limit: u64,
    stop: &mut ConnectionSearchStop,
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
        stop,
        accept,
    )
}

type CrossingChoices = BTreeMap<(i64, i64), (bool, PlacedLogisticsComponent)>;

struct PreparedEndpoints {
    endpoints: Vec<(WorldGridPosition, FacilityPortEdge)>,
    next: usize,
    blocked: HashSet<(i64, i64)>,
    choices: CrossingChoices,
    axes: BTreeMap<(i64, i64), bool>,
}

struct ExternalCandidate {
    attachment: Attachment,
    rate: Rate,
    prepared: Option<PreparedEndpoints>,
}

struct PartialConnection<'a> {
    node: Cow<'a, ConstructiveNode>,
    remaining: Rate,
    lane: usize,
    occupancy: Occupancy,
    transport: TransportKind,
    supply: bool,
    candidates: Vec<ExternalCandidate>,
    next_candidate: usize,
    first_round_left: usize,
}

impl<'a> PartialConnection<'a> {
    fn new(
        node: Cow<'a, ConstructiveNode>,
        edge: &FacilityInstanceWiringEdge,
        remaining: Rate,
        lane: usize,
        transports: &ValidatedTransportCatalog,
        components: &ValidatedLogisticsComponentCatalog,
        stats: &mut ConstructiveCompositionStatistics,
        root: bool,
    ) -> Result<Option<Self>, ()> {
        let Some(boundary) = node
            .boundary_requirements
            .iter()
            .find(|b| b.requirement == edge.id)
        else {
            return Ok(None);
        };
        let Some(capacity) = line_capacity(boundary.transport, transports) else {
            return Ok(None);
        };
        let transport = boundary.transport;
        let supply = boundary.direction == FacilityPortDirection::Input;
        let occupancy = Occupancy::new(node.as_ref());
        let attachments = attachments(
            node.as_ref(),
            boundary,
            transports,
            components,
            &occupancy,
            stats,
        );
        let mut candidates = Vec::new();
        candidates.try_reserve(attachments.len()).map_err(|_| ())?;
        for attachment in attachments {
            let available = match &attachment {
                Attachment::NewPort { .. } => capacity,
                Attachment::ExistingNetwork {
                    residual_capacity, ..
                } => *residual_capacity,
            };
            candidates.push(ExternalCandidate {
                attachment,
                rate: remaining.min(available),
                prepared: None,
            });
        }
        let first_round_left = if root { candidates.len() } else { 0 };
        Ok(Some(Self {
            node,
            remaining,
            lane,
            occupancy,
            transport,
            supply,
            candidates,
            next_candidate: 0,
            first_round_left,
        }))
    }

    fn next_trial(
        &mut self,
        width: i64,
        height: i64,
        components: &ValidatedLogisticsComponentCatalog,
    ) -> Result<Option<(usize, WorldGridPosition, FacilityPortEdge)>, ()> {
        let count = self.candidates.len();
        for _ in 0..count {
            let index = self.next_candidate;
            self.next_candidate = (index + 1) % count;
            self.first_round_left = self.first_round_left.saturating_sub(1);
            let candidate = &mut self.candidates[index];
            if candidate.prepared.is_none() {
                candidate.prepared = Some(prepare_endpoints(
                    self.node.as_ref(),
                    &self.occupancy,
                    &candidate.attachment,
                    candidate.rate,
                    self.transport,
                    width,
                    height,
                    components,
                )?);
            }
            let prepared = candidate.prepared.as_mut().unwrap();
            if let Some((external, side)) = prepared.endpoints.get(prepared.next).cloned() {
                prepared.next += 1;
                return Ok(Some((index, external, side)));
            }
        }
        Ok(None)
    }
}

fn prepare_endpoints(
    node: &ConstructiveNode,
    occupancy: &Occupancy,
    attachment: &Attachment,
    rate: Rate,
    transport: TransportKind,
    width: i64,
    height: i64,
    components: &ValidatedLogisticsComponentCatalog,
) -> Result<PreparedEndpoints, ()> {
    let point = endpoint(attachment);
    let perimeter = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_add(h)))
        .and_then(|sum| sum.checked_mul(2))
        .ok_or(())?;
    let mut endpoints = Vec::new();
    endpoints.try_reserve(perimeter).map_err(|_| ())?;
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
        .filter(|b| b.transport == transport && b.attachments.len() == 1)
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
    endpoints.retain(|(p, _)| !blocked.contains(&xy(p)));
    let choices = crossings::options(node, transport, rate, components);
    let axes = choices.iter().map(|(p, (axis, _))| (*p, *axis)).collect();
    Ok(PreparedEndpoints {
        endpoints,
        next: 0,
        blocked,
        choices,
        axes,
    })
}

fn complete(
    mut node: ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    stats: &mut ConstructiveCompositionStatistics,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    node.external_requirements.push(edge.id.clone());
    node.external_requirements.sort();
    node.boundary_requirements
        .retain(|b| b.requirement != edge.id);
    (refresh(&mut node, transports, components, stats) && accept(&node)).then_some(node)
}

#[allow(clippy::too_many_arguments)]
fn install_lane(
    node: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    attachment: &Attachment,
    rate: Rate,
    lane: usize,
    external: WorldGridPosition,
    side: FacilityPortEdge,
    path: &[WorldGridPosition],
    transport: TransportKind,
    supply: bool,
    choices: &CrossingChoices,
) -> Option<ConstructiveNode> {
    let mut next = node.clone();
    if !next
        .transport_networks
        .iter()
        .any(|network| network.transport == transport && network.item == edge.item)
    {
        next.transport_networks.push(TransportNetwork {
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
    let network = next
        .transport_networks
        .iter_mut()
        .find(|network| network.transport == transport && network.item == edge.item)?;
    attach(
        network,
        attachment,
        rate,
        !supply,
        edge,
        lane,
        &mut next.logistics_components,
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
    next.attachment_events
        .push(super::super::ConstructiveAttachmentEvent {
            requirement: edge.id.clone(),
            rate,
            network: network.id.clone(),
            source_kind: if supply { "external" } else { kind },
            target_kind: if supply { kind } else { "external" },
            component_ids: vec![],
        });
    crossings::install(&mut next, transport, &edge.item, path, choices);
    Some(next)
}

#[allow(clippy::too_many_arguments)]
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
    stop: &mut ConnectionSearchStop,
    accept: &mut impl FnMut(&ConstructiveNode) -> bool,
) -> Option<ConstructiveNode> {
    if crate::work_control::stopped() {
        stop.interrupted();
        return None;
    }
    if remaining.is_zero() {
        return complete(node.clone(), edge, transports, components, stats, accept);
    }
    if limit == 0 {
        stop.budget();
        return None;
    }
    let root = match PartialConnection::new(
        Cow::Borrowed(node),
        edge,
        remaining,
        lane,
        transports,
        components,
        stats,
        true,
    ) {
        Ok(Some(root)) => root,
        Ok(None) => return None,
        Err(()) => {
            stop.resource();
            return None;
        }
    };
    let mut queue = VecDeque::new();
    if queue.try_reserve(1).is_err() {
        stop.resource();
        return None;
    }
    queue.push_back(root);
    while let Some(mut partial) = queue.pop_front() {
        if crate::work_control::stopped() {
            stop.interrupted();
            return None;
        }
        let trial = match partial.next_trial(width, height, components) {
            Ok(Some(trial)) => trial,
            Ok(None) => continue,
            Err(()) => {
                stop.resource();
                return None;
            }
        };
        if stats.astar_searches >= limit {
            stop.budget();
            return None;
        }
        let (index, external, side) = trial;
        let candidate = &partial.candidates[index];
        let prepared = candidate.prepared.as_ref().unwrap();
        let point = endpoint(&candidate.attachment);
        stats.astar_searches += 1;
        let (start, goal) = if partial.supply {
            (&external, point)
        } else {
            (point, &external)
        };
        let path = workspace.route_crossings(&prepared.blocked, &prepared.axes, start, goal);
        let mut child = None;
        if let Some(path) = path {
            if let Some(next) = install_lane(
                partial.node.as_ref(),
                edge,
                &candidate.attachment,
                candidate.rate,
                partial.lane,
                external,
                side,
                &path,
                partial.transport,
                partial.supply,
                &prepared.choices,
            ) {
                if let Ok(remaining) = partial.remaining.checked_sub(candidate.rate) {
                    if remaining.is_zero() {
                        if let Some(done) =
                            complete(next, edge, transports, components, stats, accept)
                        {
                            return Some(done);
                        }
                    } else {
                        child = match PartialConnection::new(
                            Cow::Owned(next),
                            edge,
                            remaining,
                            partial.lane + 1,
                            transports,
                            components,
                            stats,
                            false,
                        ) {
                            Ok(child) => child,
                            Err(()) => {
                                stop.resource();
                                return None;
                            }
                        };
                    }
                }
            }
        } else {
            stats.astar_failures += 1;
        }
        if partial.first_round_left > 0 {
            queue.push_front(partial);
        } else {
            queue.push_back(partial);
        }
        if let Some(child) = child {
            if reserve_child(&mut queue).is_err() {
                stop.resource();
                return None;
            }
            queue.push_back(child);
        }
    }
    if crate::work_control::stopped() {
        stop.interrupted();
    }
    None
}
