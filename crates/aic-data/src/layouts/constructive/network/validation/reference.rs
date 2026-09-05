//! Independent validation of the committed grid witness, without route-search state.
use super::*;
use std::collections::VecDeque;

pub(in crate::layouts::constructive::network) fn validate(
    node: &ConstructiveNode,
    facilities: &ValidatedFacilityCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
) -> Result<(), ConstructiveFrontierDiagnostic> {
    let fail = |entity: String, message: &str| {
        ConstructiveFrontierDiagnostic::error(
            "constructive-network-attachment-geometry-invalid",
            "/composite",
            Some(entity),
            message,
        )
    };
    let mut occupied = HashSet::new();
    let mut members = BTreeSet::new();
    let mut ports = BTreeMap::<(String, String), PlacedFacilityPort>::new();
    for p in &node.placements {
        if !members.insert(p.instance.clone()) {
            return Err(fail(p.instance.clone(), "duplicate facility instance"));
        }
        let Some(d) = facilities.facility(&p.facility) else {
            return Err(fail(p.instance.clone(), "unknown facility definition"));
        };
        let size = if p.rotation % 180 == 0 {
            (d.footprint.width, d.footprint.height)
        } else {
            (d.footprint.height, d.footprint.width)
        };
        if !d.allowed_rotations.contains(&p.rotation) || (p.width, p.height) != size {
            return Err(fail(
                p.instance.clone(),
                "invalid facility rotation or footprint",
            ));
        }
        for x in p.x..p.x + p.width {
            for y in p.y..p.y + p.height {
                if !occupied.insert((x, y)) {
                    return Err(fail(p.instance.clone(), "facility overlap"));
                }
            }
        }
        // Projection is a neutral catalog geometry operation, with no exact model or search.
        let shifted = crate::layouts::FacilityPlacement {
            x: 1,
            y: 1,
            ..p.clone()
        };
        let projection = crate::layouts::project_facility_ports(
            &crate::layouts::FacilityPlacementReport {
                success: true,
                status: crate::layouts::FacilityPlacementStatus::Feasible,
                bounds: Some(crate::layouts::FacilityPlacementBounds {
                    width: p.width + 2,
                    height: p.height + 2,
                }),
                placements: vec![shifted],
                diagnostics: Vec::new(),
            },
            facilities,
            &crate::layouts::FacilityPlacementRequest {
                schema_version: crate::layouts::SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
                max_width: p.width + 2,
                max_height: p.height + 2,
            },
        );
        if !projection.success {
            return Err(fail(p.instance.clone(), "facility port projection failed"));
        }
        for mut port in projection.ports {
            port.position.x += p.x - 1;
            port.position.y += p.y - 1;
            port.connection.x += p.x - 1;
            port.connection.y += p.y - 1;
            ports.insert((p.instance.clone(), port.port.clone()), port);
        }
    }
    let mut physical = BTreeSet::new();
    if members != node.member_instances.iter().cloned().collect()
        || members.len() != node.member_instances.len()
    {
        return Err(fail(
            node.id.clone(),
            "facility membership does not match placements",
        ));
    }
    let requirements = node
        .transport_networks
        .iter()
        .flat_map(|n| n.requirement_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    if requirements
        != node
            .internal_requirements
            .iter()
            .chain(&node.external_requirements)
            .cloned()
            .collect()
    {
        return Err(fail(
            node.id.clone(),
            "routed requirement identities do not match the covered subgraph",
        ));
    }
    let commodities = node
        .transport_networks
        .iter()
        .map(|n| (n.transport, n.item.clone()))
        .collect::<BTreeSet<_>>();
    if commodities.len() != node.transport_networks.len() {
        return Err(fail(
            node.id.clone(),
            "commodity networks are not normalized",
        ));
    }
    let mut used_ports = BTreeSet::new();
    let mut component_ids = BTreeSet::new();
    let mut bridge_axes = BTreeMap::<String, Vec<bool>>::new();
    for n in &node.transport_networks {
        let mut network_components = BTreeSet::new();
        let Some(cap) = line_capacity(n.transport, transports) else {
            return Err(fail(n.id.clone(), "invalid line capacity"));
        };
        let cells = n.cells.iter().map(xy).collect::<BTreeSet<_>>();
        if cells.is_empty() || cells.len() != n.cells.len() {
            return Err(fail(n.id.clone(), "empty or duplicate network cells"));
        }
        let mut flow = BTreeMap::<
            (i64, i64),
            (Rate, Rate, Vec<CardinalDirection>, Vec<CardinalDirection>),
        >::new();
        for p in &cells {
            let crossing = node.logistics_components.iter().any(|c| {
                c.transport == n.transport
                    && xy(&c.position) == *p
                    && c.kind == LogisticsComponentKind::Bridge
            });
            if occupied.contains(p) || (!physical.insert((n.transport, *p)) && !crossing) {
                return Err(fail(n.id.clone(), "transport collision"));
            }
            flow.insert(*p, (Rate::zero(), Rate::zero(), Vec::new(), Vec::new()));
        }
        let mut segments = BTreeSet::new();
        for s in &n.segments {
            if s.rate <= Rate::zero()
                || s.rate > cap
                || !cells.contains(&xy(&s.from))
                || !cells.contains(&xy(&s.to))
                || !segments.insert((xy(&s.from), xy(&s.to)))
            {
                return Err(fail(
                    n.id.clone(),
                    "invalid segment rate, membership, or duplicate arc",
                ));
            }
            let Some(out) = direction(&s.from, &s.to) else {
                return Err(fail(n.id.clone(), "non-adjacent segment"));
            };
            let f = flow.get_mut(&xy(&s.from)).unwrap();
            f.1 =
                f.1.checked_add(s.rate)
                    .map_err(|_| fail(n.id.clone(), "flow arithmetic overflow"))?;
            f.3.push(out);
            let f = flow.get_mut(&xy(&s.to)).unwrap();
            f.0 =
                f.0.checked_add(s.rate)
                    .map_err(|_| fail(n.id.clone(), "flow arithmetic overflow"))?;
            f.2.push(rotate(out, 180));
        }
        for t in &n.terminals {
            if let TransportNetworkEndpoint::External {
                node: external_node,
                side,
            } = &t.endpoint
            {
                use crate::facilities::FacilityPortEdge;
                let xs = node
                    .placements
                    .iter()
                    .flat_map(|p| [p.x, p.x + p.width - 1])
                    .chain(
                        node.transport_networks
                            .iter()
                            .flat_map(|n| n.cells.iter().map(|p| p.x)),
                    )
                    .collect::<Vec<_>>();
                let ys = node
                    .placements
                    .iter()
                    .flat_map(|p| [p.y, p.y + p.height - 1])
                    .chain(
                        node.transport_networks
                            .iter()
                            .flat_map(|n| n.cells.iter().map(|p| p.y)),
                    )
                    .collect::<Vec<_>>();
                let (on_boundary, d) = match side {
                    FacilityPortEdge::North => (
                        Some(&t.position.y) == ys.iter().min(),
                        CardinalDirection::North,
                    ),
                    FacilityPortEdge::East => (
                        Some(&t.position.x) == xs.iter().max(),
                        CardinalDirection::East,
                    ),
                    FacilityPortEdge::South => (
                        Some(&t.position.y) == ys.iter().max(),
                        CardinalDirection::South,
                    ),
                    FacilityPortEdge::West => (
                        Some(&t.position.x) == xs.iter().min(),
                        CardinalDirection::West,
                    ),
                };
                if !on_boundary
                    || t.node != *external_node
                    || t.rate <= Rate::zero()
                    || t.rate > cap
                {
                    return Err(fail(
                        t.id.clone(),
                        "invalid external terminal identity, used-boundary position or capacity",
                    ));
                }
                let Some(f) = flow.get_mut(&xy(&t.position)) else {
                    return Err(fail(t.id.clone(), "external terminal missing from network"));
                };
                if t.direction == FacilityPortDirection::Output {
                    f.0 =
                        f.0.checked_add(t.rate)
                            .map_err(|_| fail(t.id.clone(), "external flow overflow"))?;
                    f.2.push(d);
                } else {
                    f.1 =
                        f.1.checked_add(t.rate)
                            .map_err(|_| fail(t.id.clone(), "external flow overflow"))?;
                    f.3.push(d);
                }
                continue;
            }
            let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint else {
                return Err(fail(
                    n.id.clone(),
                    "unexpected external terminal in constructive node",
                ));
            };
            let Some(p) = ports.get(&(instance.clone(), port.clone())) else {
                return Err(fail(t.id.clone(), "unknown terminal port"));
            };
            if t.node != *instance
                || t.position != p.connection
                || t.direction != p.direction
                || n.transport != p.transport
                || t.rate <= Rate::zero()
                || t.rate > cap
                || !used_ports.insert((instance, port))
            {
                return Err(fail(
                    t.id.clone(),
                    "terminal direction, geometry, capacity, or port reuse mismatch",
                ));
            }
            let Some(f) = flow.get_mut(&xy(&t.position)) else {
                return Err(fail(t.id.clone(), "terminal missing from network"));
            };
            let d = direction(&p.connection, &p.position).unwrap();
            if t.direction == FacilityPortDirection::Output {
                f.0 =
                    f.0.checked_add(t.rate)
                        .map_err(|_| fail(t.id.clone(), "terminal flow overflow"))?;
                f.2.push(d);
            } else {
                f.1 =
                    f.1.checked_add(t.rate)
                        .map_err(|_| fail(t.id.clone(), "terminal flow overflow"))?;
                f.3.push(d);
            }
        }
        for (p, (incoming, outgoing, inputs, outputs)) in &flow {
            if incoming != outgoing || incoming.is_zero() || *incoming > cap {
                return Err(fail(
                    n.id.clone(),
                    "cell flow conservation or capacity failure",
                ));
            }
            let unique = inputs
                .iter()
                .chain(outputs)
                .copied()
                .collect::<BTreeSet<_>>();
            if unique.len() != inputs.len() + outputs.len() {
                return Err(fail(n.id.clone(), "duplicate or opposing use of an arm"));
            }
            let placed = node
                .logistics_components
                .iter()
                .filter(|c| c.transport == n.transport && xy(&c.position) == *p)
                .collect::<Vec<_>>();
            if placed.len() > 1 {
                return Err(fail(n.id.clone(), "overlapping branch components"));
            }
            if let Some(c) = placed.first() {
                let Some(def) = components.component(&c.component) else {
                    return Err(fail(c.id.clone(), "unknown component"));
                };
                if (!component_ids.insert(c.id.clone()) && c.kind != LogisticsComponentKind::Bridge)
                    || !n.component_ids.contains(&c.id)
                    || def.kind != c.kind
                    || def.transport != c.transport
                    || !def.allowed_rotations.contains(&c.rotation)
                    || component_capacity(def).is_none_or(|cap| cap < *incoming)
                {
                    return Err(fail(
                        c.id.clone(),
                        "component identity, rotation, or capacity mismatch",
                    ));
                }
                network_components.insert(c.id.clone());
                let allowed_in = def
                    .input_directions
                    .iter()
                    .map(|&d| rotate(d, c.rotation))
                    .collect::<Vec<_>>();
                let allowed_out = def
                    .output_directions
                    .iter()
                    .map(|&d| rotate(d, c.rotation))
                    .collect::<Vec<_>>();
                let topology = match c.kind {
                    LogisticsComponentKind::Splitter => inputs.len() == 1 && outputs.len() >= 2,
                    LogisticsComponentKind::Converger => inputs.len() >= 2 && outputs.len() == 1,
                    LogisticsComponentKind::Bridge => {
                        let straight = inputs.len() == 1
                            && outputs.len() == 1
                            && rotate(inputs[0], 180) == outputs[0]
                            && !n.terminals.iter().any(|t| xy(&t.position) == *p);
                        if straight {
                            bridge_axes.entry(c.id.clone()).or_default().push(matches!(
                                inputs[0],
                                CardinalDirection::East | CardinalDirection::West
                            ));
                        }
                        straight
                    }
                };
                if !topology
                    || !inputs.iter().all(|d| allowed_in.contains(d))
                    || !outputs.iter().all(|d| allowed_out.contains(d))
                {
                    return Err(fail(c.id.clone(), "component arm topology mismatch"));
                }
            } else if inputs.len() != 1 || outputs.len() != 1 {
                return Err(fail(
                    n.id.clone(),
                    "ordinary transport cell branches or merges",
                ));
            }
        }
        if n.component_ids.iter().cloned().collect::<BTreeSet<_>>() != network_components
            || n.component_ids.len() != network_components.len()
        {
            return Err(fail(n.id.clone(), "network references missing component"));
        }
        // No disconnected circulation-only geometry: each physical cell must be reachable from
        // a supply and able to reach a demand. This constructor emits no circulation.
        for reverse in [false, true] {
            let mut seen = BTreeSet::new();
            let mut queue = VecDeque::new();
            for t in &n.terminals {
                if (t.direction == FacilityPortDirection::Input) == reverse {
                    seen.insert(xy(&t.position));
                    queue.push_back(xy(&t.position));
                }
            }
            while let Some(p) = queue.pop_front() {
                for s in &n.segments {
                    let (a, b) = if reverse {
                        (&s.to, &s.from)
                    } else {
                        (&s.from, &s.to)
                    };
                    if xy(a) == p && seen.insert(xy(b)) {
                        queue.push_back(xy(b));
                    }
                }
            }
            if seen != cells {
                return Err(fail(n.id.clone(), "unreachable network geometry"));
            }
        }
    }
    if bridge_axes
        .values()
        .any(|axes| axes.len() != 2 || axes[0] == axes[1])
    {
        return Err(fail(
            node.id.clone(),
            "bridge requires two independent perpendicular straight channels",
        ));
    }
    if component_ids.len() != node.logistics_components.len() {
        return Err(fail(node.id.clone(), "unowned logistics component"));
    }
    Ok(())
}
