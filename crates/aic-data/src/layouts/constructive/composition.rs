use std::collections::HashSet;

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, PlacedFacilityPort, SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
    TransportNetwork, TransportNetworkEndpoint, TransportNetworkSegment, TransportNetworkTerminal,
    WorldGridPosition, project_facility_ports,
};
use crate::logistics::{TransportKind, ValidatedItemCatalog};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport,
};

use super::first_pipe_frontier::{bounds_for, occupied_cells, rectangles_overlap};
use super::routing::{count_turns, route_shortest_path};
use super::{
    CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION, ConstructiveCompositionReport,
    ConstructiveCompositionScore, ConstructiveCompositionStatistics,
    ConstructiveFrontierDiagnostic, ConstructiveNode, ConstructiveProcessModuleBoundary,
    ConstructiveProcessModuleReport,
};

pub fn constructive_node_from_process_module(
    report: &ConstructiveProcessModuleReport,
) -> Result<ConstructiveNode, ConstructiveFrontierDiagnostic> {
    if !report.success {
        return Err(ConstructiveFrontierDiagnostic::error(
            "upstream-process-module-failed",
            "/source_module",
            Some(report.root_instance.clone()),
            "constructive composition requires a successful source process module",
        ));
    }
    let Some(bounds) = report.growth.bounds.clone() else {
        return Err(ConstructiveFrontierDiagnostic::error(
            "missing-process-module-node-bounds",
            "/source_module/growth/bounds",
            Some(report.root_instance.clone()),
            "successful process module has no used bounds",
        ));
    };
    Ok(ConstructiveNode {
        id: format!(
            "process-module:{}:{}",
            report.root_instance, report.internal_item
        ),
        bounds,
        member_instances: report.member_instances.clone(),
        internal_requirements: report.internal_requirements.clone(),
        placements: report.growth.placements.clone(),
        transport_networks: report.growth.transport_networks.clone(),
        boundary_requirements: report.boundary_requirements.clone(),
    })
}

pub fn construct_facility_node(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    instance_id: &str,
) -> Result<ConstructiveNode, ConstructiveFrontierDiagnostic> {
    let Some((recipe, facility)) = wiring.nodes.iter().find_map(|node| match node {
        FacilityInstanceWiringNode::Facility {
            id,
            recipe,
            facility,
            ..
        } if id == instance_id => Some((recipe, facility)),
        _ => None,
    }) else {
        return Err(ConstructiveFrontierDiagnostic::error(
            "missing-composition-facility-instance",
            "/target_instance",
            Some(instance_id.to_string()),
            format!("composition target facility instance '{instance_id}' does not exist"),
        ));
    };
    let Some(definition) = facilities.facility(facility) else {
        return Err(ConstructiveFrontierDiagnostic::error(
            "missing-composition-facility-definition",
            "/target_instance",
            Some(instance_id.to_string()),
            format!("composition target references missing facility '{facility}'"),
        ));
    };
    let Some(&rotation) = definition.allowed_rotations.first() else {
        return Err(ConstructiveFrontierDiagnostic::error(
            "missing-composition-facility-rotation",
            "/target_instance",
            Some(instance_id.to_string()),
            "composition target facility has no legal rotation",
        ));
    };
    let (width, height) = match rotation {
        0 | 180 => (definition.footprint.width, definition.footprint.height),
        90 | 270 => (definition.footprint.height, definition.footprint.width),
        _ => unreachable!("facility rotations are validated"),
    };
    let placement = FacilityPlacement {
        instance: instance_id.to_string(),
        recipe: recipe.clone(),
        facility: facility.clone(),
        x: 0,
        y: 0,
        width,
        height,
        rotation,
    };
    let translated = FacilityPlacement {
        x: 1,
        y: 1,
        ..placement.clone()
    };
    let request = FacilityPlacementRequest {
        schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
        max_width: width + 2,
        max_height: height + 2,
    };
    let projection = project_facility_ports(
        &FacilityPlacementReport {
            success: true,
            status: FacilityPlacementStatus::Feasible,
            bounds: Some(FacilityPlacementBounds {
                width: width + 2,
                height: height + 2,
            }),
            placements: vec![translated],
            diagnostics: Vec::new(),
        },
        facilities,
        &request,
    );
    if !projection.success {
        return Err(ConstructiveFrontierDiagnostic::error(
            "composition-facility-port-projection-failed",
            "/target_instance",
            Some(instance_id.to_string()),
            "failed to project composition target facility ports",
        ));
    }
    let ports = projection
        .ports
        .into_iter()
        .map(|mut port| {
            translate_port(&mut port, -1, -1);
            port
        })
        .collect::<Vec<_>>();
    let mut boundaries = Vec::new();
    for edge in &wiring.edges {
        let (direction, inside) = if edge.source == instance_id && edge.target != instance_id {
            (FacilityPortDirection::Output, edge.source.as_str())
        } else if edge.target == instance_id && edge.source != instance_id {
            (FacilityPortDirection::Input, edge.target.as_str())
        } else {
            continue;
        };
        let Some(item) = items.item(&edge.item) else {
            return Err(ConstructiveFrontierDiagnostic::error(
                "missing-composition-boundary-item",
                "/edges",
                Some(edge.item.clone()),
                format!(
                    "composition boundary references missing item '{}'",
                    edge.item
                ),
            ));
        };
        let port_options = ports
            .iter()
            .filter(|port| port.direction == direction && port.transport == item.transport)
            .cloned()
            .collect::<Vec<_>>();
        if port_options.is_empty() {
            return Err(ConstructiveFrontierDiagnostic::error(
                "composition-facility-boundary-blocked",
                "/target_instance",
                Some(edge.id.clone()),
                format!(
                    "facility node has no compatible port for boundary requirement '{}'",
                    edge.id
                ),
            ));
        }
        boundaries.push(ConstructiveProcessModuleBoundary {
            requirement: edge.id.clone(),
            item: edge.item.clone(),
            transport: item.transport,
            rate: edge.rate,
            direction,
            inside_instance: inside.to_string(),
            port_options,
        });
    }
    boundaries.sort_by(|left, right| left.requirement.cmp(&right.requirement));
    Ok(ConstructiveNode {
        id: format!("facility-node:{instance_id}"),
        bounds: FacilityPlacementBounds { width, height },
        member_instances: vec![instance_id.to_string()],
        internal_requirements: Vec::new(),
        placements: vec![placement],
        transport_networks: Vec::new(),
        boundary_requirements: boundaries,
    })
}

pub fn compose_process_module_with_facility(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    source_module: &ConstructiveProcessModuleReport,
    target_instance: &str,
    requirement: &str,
) -> ConstructiveCompositionReport {
    let source = match constructive_node_from_process_module(source_module) {
        Ok(source) => source,
        Err(diagnostic) => {
            return failed_report(
                requirement,
                "source-process-module",
                target_instance,
                diagnostic,
            );
        }
    };
    let target = match construct_facility_node(wiring, facilities, items, target_instance) {
        Ok(target) => target,
        Err(diagnostic) => {
            return failed_report(requirement, &source.id, target_instance, diagnostic);
        }
    };
    let Some(edge) = wiring.edges.iter().find(|edge| edge.id == requirement) else {
        return failed_report(
            requirement,
            &source.id,
            &target.id,
            ConstructiveFrontierDiagnostic::error(
                "missing-composition-requirement",
                "/requirement",
                Some(requirement.to_string()),
                format!("composition requirement '{requirement}' does not exist"),
            ),
        );
    };
    compose_constructive_nodes(&source, &target, edge, facilities)
}

pub fn compose_constructive_nodes(
    source: &ConstructiveNode,
    target: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    facilities: &ValidatedFacilityCatalog,
) -> ConstructiveCompositionReport {
    if !source.member_instances.contains(&edge.source)
        || !target.member_instances.contains(&edge.target)
    {
        return failed_report(
            &edge.id,
            &source.id,
            &target.id,
            ConstructiveFrontierDiagnostic::error(
                "composition-edge-node-mismatch",
                "/requirement",
                Some(edge.id.clone()),
                "composition edge source and target do not belong to the supplied nodes",
            ),
        );
    }
    let Some(source_boundary) = source.boundary_requirements.iter().find(|boundary| {
        boundary.requirement == edge.id && boundary.direction == FacilityPortDirection::Output
    }) else {
        return failed_report(
            &edge.id,
            &source.id,
            &target.id,
            missing_boundary(&edge.id, "source output"),
        );
    };
    let Some(target_boundary) = target.boundary_requirements.iter().find(|boundary| {
        boundary.requirement == edge.id && boundary.direction == FacilityPortDirection::Input
    }) else {
        return failed_report(
            &edge.id,
            &source.id,
            &target.id,
            missing_boundary(&edge.id, "target input"),
        );
    };
    if source_boundary.transport != target_boundary.transport
        || source_boundary.item != target_boundary.item
    {
        return failed_report(
            &edge.id,
            &source.id,
            &target.id,
            ConstructiveFrontierDiagnostic::error(
                "composition-boundary-mismatch",
                "/requirement",
                Some(edge.id.clone()),
                "composition boundary item or transport kind does not match",
            ),
        );
    }

    let canvas_width = source.bounds.width + target.bounds.width + 10;
    let canvas_height = source.bounds.height + target.bounds.height + 10;
    let target_candidate = translate_node(target, 4, 4);
    let mut statistics = ConstructiveCompositionStatistics::default();
    let mut best: Option<(ConstructiveCompositionScore, usize, ConstructiveNode)> = None;
    let mut order = 0usize;
    for rotation in [0, 90, 180, 270] {
        statistics.rotations_considered += 1;
        let rotated = rotate_node(source, rotation);
        if !node_rotations_are_legal(&rotated, facilities) {
            continue;
        }
        for y in 1..canvas_height - rotated.bounds.height {
            for x in 1..canvas_width - rotated.bounds.width {
                statistics.placements_considered += 1;
                let source_candidate = translate_node(&rotated, x, y);
                if nodes_collide(&source_candidate, &target_candidate) {
                    statistics.colliding_placements_rejected += 1;
                    continue;
                }
                let blocked = routing_blocked_cells(
                    &source_candidate,
                    &target_candidate,
                    source_boundary.transport,
                );
                let source_options = transformed_boundary(&source_candidate, &edge.id)
                    .map(|boundary| &boundary.port_options)
                    .into_iter()
                    .flatten();
                let target_options = transformed_boundary(&target_candidate, &edge.id)
                    .map(|boundary| &boundary.port_options)
                    .into_iter()
                    .flatten();
                for source_port in source_options {
                    for target_port in target_options.clone() {
                        statistics.port_pairs_considered += 1;
                        if blocked.contains(&(source_port.connection.x, source_port.connection.y))
                            || blocked
                                .contains(&(target_port.connection.x, target_port.connection.y))
                        {
                            statistics.blocked_port_pairs_rejected += 1;
                            continue;
                        }
                        statistics.astar_searches += 1;
                        let Some(path) = route_shortest_path(
                            canvas_width,
                            canvas_height,
                            &blocked,
                            &source_port.connection,
                            &target_port.connection,
                        ) else {
                            statistics.astar_failures += 1;
                            continue;
                        };
                        let route = connection_network(
                            edge,
                            source_boundary.transport,
                            source_port,
                            target_port,
                            path,
                        );
                        let Some((composite, blocked_options)) = combine_nodes(
                            &source_candidate,
                            &target_candidate,
                            edge,
                            source_port,
                            target_port,
                            route,
                        ) else {
                            statistics.boundary_dead_ends_rejected += 1;
                            continue;
                        };
                        if !validate_node_geometry(&composite) {
                            continue;
                        }
                        statistics.valid_candidates_scored += 1;
                        let score = composition_score(&composite, blocked_options);
                        let candidate_order = order;
                        order += 1;
                        if best.as_ref().is_none_or(|(current, current_order, _)| {
                            (score, candidate_order) < (*current, *current_order)
                        }) {
                            best = Some((score, candidate_order, composite));
                        }
                    }
                }
            }
        }
    }

    let Some((score, _, composite)) = best else {
        return ConstructiveCompositionReport {
            schema_version: CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION,
            success: false,
            requirement: edge.id.clone(),
            source_node: source.id.clone(),
            target_node: target.id.clone(),
            score: None,
            composite: None,
            statistics,
            diagnostics: vec![ConstructiveFrontierDiagnostic::error(
                "constructive-composition-exhausted",
                "/composition",
                Some(edge.id.clone()),
                "exhausted block placement, rotation, port, and route candidates",
            )],
        };
    };
    ConstructiveCompositionReport {
        schema_version: CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION,
        success: true,
        requirement: edge.id.clone(),
        source_node: source.id.clone(),
        target_node: target.id.clone(),
        score: Some(score),
        composite: Some(composite),
        statistics,
        diagnostics: vec![ConstructiveFrontierDiagnostic::info(
            "constructive-composition-constructed",
            "placed and routed two immutable constructive nodes as one composite node",
        )],
    }
}

fn combine_nodes(
    source: &ConstructiveNode,
    target: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    source_port: &PlacedFacilityPort,
    target_port: &PlacedFacilityPort,
    route: TransportNetwork,
) -> Option<(ConstructiveNode, usize)> {
    let mut placements = target.placements.clone();
    placements.extend(source.placements.clone());
    let mut networks = target.transport_networks.clone();
    networks.extend(source.transport_networks.clone());
    networks.push(route);
    let mut member_instances = target.member_instances.clone();
    member_instances.extend(source.member_instances.clone());
    member_instances.sort();
    member_instances.dedup();
    let mut internal_requirements = target.internal_requirements.clone();
    internal_requirements.extend(source.internal_requirements.clone());
    internal_requirements.push(edge.id.clone());
    internal_requirements.sort();
    internal_requirements.dedup();
    let mut boundaries = target
        .boundary_requirements
        .iter()
        .chain(&source.boundary_requirements)
        .filter(|boundary| boundary.requirement != edge.id)
        .cloned()
        .collect::<Vec<_>>();
    let options_before = boundaries
        .iter()
        .map(|boundary| boundary.port_options.len())
        .sum::<usize>();
    let facility_cells = occupied_cells(&placements);
    let transport_cells = networks
        .iter()
        .flat_map(|network| {
            network
                .cells
                .iter()
                .map(move |cell| (layer_key(network.transport), cell.x, cell.y))
        })
        .collect::<HashSet<_>>();
    for boundary in &mut boundaries {
        boundary.port_options.retain(|port| {
            (port.instance != source_port.instance || port.port != source_port.port)
                && (port.instance != target_port.instance || port.port != target_port.port)
                && !facility_cells.contains(&(port.connection.x, port.connection.y))
                && !transport_cells.contains(&(
                    layer_key(port.transport),
                    port.connection.x,
                    port.connection.y,
                ))
        });
        if boundary.port_options.is_empty() {
            return None;
        }
    }
    boundaries.sort_by(|left, right| left.requirement.cmp(&right.requirement));
    let options_after = boundaries
        .iter()
        .map(|boundary| boundary.port_options.len())
        .sum::<usize>();
    let node = ConstructiveNode {
        id: format!("composite:{}+{}", target.id, source.id),
        bounds: bounds_for(
            &placements,
            &networks
                .iter()
                .flat_map(|network| network.cells.iter().cloned())
                .collect::<Vec<_>>(),
        ),
        member_instances,
        internal_requirements,
        placements,
        transport_networks: networks,
        boundary_requirements: boundaries,
    };
    Some((canonicalize_node(&node), options_before - options_after))
}

fn connection_network(
    edge: &FacilityInstanceWiringEdge,
    transport: TransportKind,
    source: &PlacedFacilityPort,
    target: &PlacedFacilityPort,
    cells: Vec<WorldGridPosition>,
) -> TransportNetwork {
    TransportNetwork {
        id: format!("constructive-composition:{}", edge.id),
        requirement_ids: vec![edge.id.clone()],
        item: edge.item.clone(),
        transport,
        segments: cells
            .windows(2)
            .map(|pair| TransportNetworkSegment {
                from: pair[0].clone(),
                to: pair[1].clone(),
                rate: edge.rate,
            })
            .collect(),
        cells,
        terminals: vec![
            TransportNetworkTerminal {
                id: format!("{}:source", edge.id),
                node: source.instance.clone(),
                direction: FacilityPortDirection::Output,
                endpoint: TransportNetworkEndpoint::Facility {
                    instance: source.instance.clone(),
                    port: source.port.clone(),
                },
                position: source.connection.clone(),
                rate: edge.rate,
            },
            TransportNetworkTerminal {
                id: format!("{}:target", edge.id),
                node: target.instance.clone(),
                direction: FacilityPortDirection::Input,
                endpoint: TransportNetworkEndpoint::Facility {
                    instance: target.instance.clone(),
                    port: target.port.clone(),
                },
                position: target.connection.clone(),
                rate: edge.rate,
            },
        ],
        component_ids: Vec::new(),
    }
}

fn rotate_node(node: &ConstructiveNode, rotation: i64) -> ConstructiveNode {
    let mut rotated = node.clone();
    for placement in &mut rotated.placements {
        let (x, y, width, height) = rotate_rectangle(
            placement.x,
            placement.y,
            placement.width,
            placement.height,
            node.bounds.width,
            node.bounds.height,
            rotation,
        );
        placement.x = x;
        placement.y = y;
        placement.width = width;
        placement.height = height;
        placement.rotation = (placement.rotation + rotation) % 360;
    }
    for network in &mut rotated.transport_networks {
        for cell in &mut network.cells {
            *cell = rotate_point(cell, node.bounds.width, node.bounds.height, rotation);
        }
        for segment in &mut network.segments {
            segment.from = rotate_point(
                &segment.from,
                node.bounds.width,
                node.bounds.height,
                rotation,
            );
            segment.to = rotate_point(&segment.to, node.bounds.width, node.bounds.height, rotation);
        }
        for terminal in &mut network.terminals {
            terminal.position = rotate_point(
                &terminal.position,
                node.bounds.width,
                node.bounds.height,
                rotation,
            );
        }
    }
    for boundary in &mut rotated.boundary_requirements {
        for port in &mut boundary.port_options {
            port.position = rotate_point(
                &port.position,
                node.bounds.width,
                node.bounds.height,
                rotation,
            );
            port.connection = rotate_point(
                &port.connection,
                node.bounds.width,
                node.bounds.height,
                rotation,
            );
            port.edge = port.edge.rotated_clockwise(rotation);
        }
    }
    if matches!(rotation, 90 | 270) {
        rotated.bounds = FacilityPlacementBounds {
            width: node.bounds.height,
            height: node.bounds.width,
        };
    }
    rotated
}

fn rotate_rectangle(
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    node_width: i64,
    node_height: i64,
    rotation: i64,
) -> (i64, i64, i64, i64) {
    match rotation {
        0 => (x, y, width, height),
        90 => (node_height - y - height, x, height, width),
        180 => (
            node_width - x - width,
            node_height - y - height,
            width,
            height,
        ),
        270 => (y, node_width - x - width, height, width),
        _ => unreachable!("composition rotations are quarter turns"),
    }
}

fn rotate_point(
    point: &WorldGridPosition,
    width: i64,
    height: i64,
    rotation: i64,
) -> WorldGridPosition {
    match rotation {
        0 => point.clone(),
        90 => WorldGridPosition {
            x: height - 1 - point.y,
            y: point.x,
        },
        180 => WorldGridPosition {
            x: width - 1 - point.x,
            y: height - 1 - point.y,
        },
        270 => WorldGridPosition {
            x: point.y,
            y: width - 1 - point.x,
        },
        _ => unreachable!("composition rotations are quarter turns"),
    }
}

fn translate_node(node: &ConstructiveNode, dx: i64, dy: i64) -> ConstructiveNode {
    let mut translated = node.clone();
    for placement in &mut translated.placements {
        placement.x += dx;
        placement.y += dy;
    }
    for network in &mut translated.transport_networks {
        for cell in &mut network.cells {
            cell.x += dx;
            cell.y += dy;
        }
        for segment in &mut network.segments {
            segment.from.x += dx;
            segment.from.y += dy;
            segment.to.x += dx;
            segment.to.y += dy;
        }
        for terminal in &mut network.terminals {
            terminal.position.x += dx;
            terminal.position.y += dy;
        }
    }
    for boundary in &mut translated.boundary_requirements {
        for port in &mut boundary.port_options {
            translate_port(port, dx, dy);
        }
    }
    translated
}

fn translate_port(port: &mut PlacedFacilityPort, dx: i64, dy: i64) {
    port.position.x += dx;
    port.position.y += dy;
    port.connection.x += dx;
    port.connection.y += dy;
}

fn canonicalize_node(node: &ConstructiveNode) -> ConstructiveNode {
    let minimum_x = node
        .placements
        .iter()
        .map(|placement| placement.x)
        .chain(
            node.transport_networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.x)),
        )
        .min()
        .unwrap_or(0);
    let minimum_y = node
        .placements
        .iter()
        .map(|placement| placement.y)
        .chain(
            node.transport_networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.y)),
        )
        .min()
        .unwrap_or(0);
    let mut canonical = translate_node(node, -minimum_x, -minimum_y);
    let cells = canonical
        .transport_networks
        .iter()
        .flat_map(|network| network.cells.iter().cloned())
        .collect::<Vec<_>>();
    canonical.bounds = bounds_for(&canonical.placements, &cells);
    canonical
}

fn nodes_collide(left: &ConstructiveNode, right: &ConstructiveNode) -> bool {
    if left.placements.iter().any(|left_placement| {
        right
            .placements
            .iter()
            .any(|right_placement| rectangles_overlap(left_placement, right_placement))
    }) {
        return true;
    }
    let left_facilities = occupied_cells(&left.placements);
    let right_facilities = occupied_cells(&right.placements);
    if left.transport_networks.iter().any(|network| {
        network
            .cells
            .iter()
            .any(|cell| right_facilities.contains(&(cell.x, cell.y)))
    }) || right.transport_networks.iter().any(|network| {
        network
            .cells
            .iter()
            .any(|cell| left_facilities.contains(&(cell.x, cell.y)))
    }) {
        return true;
    }
    let left_transport = left
        .transport_networks
        .iter()
        .flat_map(|network| {
            network
                .cells
                .iter()
                .map(move |cell| (layer_key(network.transport), cell.x, cell.y))
        })
        .collect::<HashSet<_>>();
    right.transport_networks.iter().any(|network| {
        network
            .cells
            .iter()
            .any(|cell| left_transport.contains(&(layer_key(network.transport), cell.x, cell.y)))
    })
}

fn routing_blocked_cells(
    source: &ConstructiveNode,
    target: &ConstructiveNode,
    transport: TransportKind,
) -> HashSet<(i64, i64)> {
    let mut blocked = occupied_cells(&source.placements);
    blocked.extend(occupied_cells(&target.placements));
    blocked.extend(
        source
            .transport_networks
            .iter()
            .chain(&target.transport_networks)
            .filter(|network| network.transport == transport)
            .flat_map(|network| network.cells.iter().map(|cell| (cell.x, cell.y))),
    );
    blocked
}

fn transformed_boundary<'a>(
    node: &'a ConstructiveNode,
    requirement: &str,
) -> Option<&'a ConstructiveProcessModuleBoundary> {
    node.boundary_requirements
        .iter()
        .find(|boundary| boundary.requirement == requirement)
}

fn node_rotations_are_legal(
    node: &ConstructiveNode,
    facilities: &ValidatedFacilityCatalog,
) -> bool {
    node.placements.iter().all(|placement| {
        facilities
            .facility(&placement.facility)
            .is_some_and(|definition| definition.allowed_rotations.contains(&placement.rotation))
    })
}

fn validate_node_geometry(node: &ConstructiveNode) -> bool {
    for (index, left) in node.placements.iter().enumerate() {
        if node.placements[index + 1..]
            .iter()
            .any(|right| rectangles_overlap(left, right))
        {
            return false;
        }
    }
    let facilities = occupied_cells(&node.placements);
    let mut transport = HashSet::new();
    node.transport_networks.iter().all(|network| {
        !network.cells.is_empty()
            && network
                .cells
                .iter()
                .all(|cell| !facilities.contains(&(cell.x, cell.y)))
            && network
                .cells
                .windows(2)
                .all(|pair| pair[0].x.abs_diff(pair[1].x) + pair[0].y.abs_diff(pair[1].y) == 1)
            && network
                .cells
                .iter()
                .all(|cell| transport.insert((layer_key(network.transport), cell.x, cell.y)))
            && network.terminals.first().map(|terminal| &terminal.position) == network.cells.first()
            && network.terminals.last().map(|terminal| &terminal.position) == network.cells.last()
    })
}

fn composition_score(
    node: &ConstructiveNode,
    blocked_boundary_port_options: usize,
) -> ConstructiveCompositionScore {
    let transport_tiles = node
        .transport_networks
        .iter()
        .flat_map(|network| {
            network
                .cells
                .iter()
                .map(move |cell| (layer_key(network.transport), cell.x, cell.y))
        })
        .collect::<HashSet<_>>()
        .len();
    ConstructiveCompositionScore {
        blocked_boundary_port_options,
        used_bounding_box_area: usize::try_from(node.bounds.width * node.bounds.height)
            .unwrap_or(usize::MAX),
        transport_tiles,
        route_turns: node
            .transport_networks
            .iter()
            .map(|network| count_turns(&network.cells))
            .sum(),
    }
}

fn layer_key(transport: TransportKind) -> u8 {
    match transport {
        TransportKind::Belt => 0,
        TransportKind::Pipe => 1,
    }
}

fn missing_boundary(requirement: &str, role: &str) -> ConstructiveFrontierDiagnostic {
    ConstructiveFrontierDiagnostic::error(
        "missing-composition-boundary",
        "/requirement",
        Some(requirement.to_string()),
        format!("composition requirement has no {role} boundary"),
    )
}

fn failed_report(
    requirement: &str,
    source_node: &str,
    target_node: &str,
    diagnostic: ConstructiveFrontierDiagnostic,
) -> ConstructiveCompositionReport {
    ConstructiveCompositionReport {
        schema_version: CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION,
        success: false,
        requirement: requirement.to_string(),
        source_node: source_node.to_string(),
        target_node: target_node.to_string(),
        score: None,
        composite: None,
        statistics: ConstructiveCompositionStatistics::default(),
        diagnostics: vec![diagnostic],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{
        FacilityCatalog, FacilityDefinition, FacilityFootprint, FacilityPortDefinition,
        FacilityPortEdge, FacilityPortPosition, SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
    };
    use crate::layouts::{construct_process_module, render_constructive_composition_html};
    use crate::logistics::{ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION};
    use crate::recipes::{
        FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringProjection, Rate,
    };

    fn node(id: &str) -> FacilityInstanceWiringNode {
        let one = Rate {
            numerator: 1,
            denominator: 1,
        };
        FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("{id}-recipe"),
            facility: "machine".to_string(),
            index: 0,
            runs_per_second: one,
            work_seconds_per_second: one,
            unused_capacity: Rate::zero(),
        }
    }

    fn edge(id: &str, source: &str, target: &str, item: &str) -> FacilityInstanceWiringEdge {
        FacilityInstanceWiringEdge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            kind: "recipe-flow".to_string(),
            item: item.to_string(),
            rate: Rate {
                numerator: 1,
                denominator: 1,
            },
            projection: FacilityInstanceWiringProjection::Original,
        }
    }

    fn facility_catalog() -> ValidatedFacilityCatalog {
        ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
            facilities: vec![FacilityDefinition {
                id: "machine".to_string(),
                footprint: FacilityFootprint {
                    width: 2,
                    height: 2,
                },
                allowed_rotations: vec![0, 90, 180, 270],
                ports: vec![
                    FacilityPortDefinition {
                        id: "input-west".to_string(),
                        direction: FacilityPortDirection::Input,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 0, y: 0 },
                        edge: FacilityPortEdge::West,
                    },
                    FacilityPortDefinition {
                        id: "input-north".to_string(),
                        direction: FacilityPortDirection::Input,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 1, y: 0 },
                        edge: FacilityPortEdge::North,
                    },
                    FacilityPortDefinition {
                        id: "output-east".to_string(),
                        direction: FacilityPortDirection::Output,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 1, y: 1 },
                        edge: FacilityPortEdge::East,
                    },
                    FacilityPortDefinition {
                        id: "output-south".to_string(),
                        direction: FacilityPortDirection::Output,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 0, y: 1 },
                        edge: FacilityPortEdge::South,
                    },
                ],
            }],
        })
        .expect("facility catalog validates")
    }

    #[test]
    fn composes_an_immutable_process_module_with_a_facility_node() {
        let wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                node("supplier-a"),
                node("supplier-b"),
                node("root"),
                node("target"),
                node("outside-input"),
                node("outside-output"),
            ],
            edges: vec![
                edge("a-root", "supplier-a", "root", "internal"),
                edge("b-root", "supplier-b", "root", "internal"),
                edge("outside-a", "outside-input", "supplier-a", "raw"),
                edge("root-target", "root", "target", "product"),
                edge("target-outside", "target", "outside-output", "final"),
            ],
            diagnostics: Vec::new(),
        };
        let facilities = facility_catalog();
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: ["internal", "raw", "product", "final"]
                .into_iter()
                .map(|id| ItemDefinition {
                    id: id.to_string(),
                    transport: TransportKind::Belt,
                })
                .collect(),
        })
        .expect("item catalog validates");

        let module = construct_process_module(&wiring, &facilities, &items, "root", "internal");
        assert!(module.success, "{:?}", module.growth.diagnostics);
        let report = compose_process_module_with_facility(
            &wiring,
            &facilities,
            &items,
            &module,
            "target",
            "root-target",
        );

        assert!(report.success, "{:?}", report.diagnostics);
        let composite = report.composite.as_ref().expect("composite node");
        assert_eq!(composite.placements.len(), 4);
        assert_eq!(composite.transport_networks.len(), 3);
        assert_eq!(composite.internal_requirements.len(), 3);
        assert!(
            composite
                .boundary_requirements
                .iter()
                .all(|boundary| boundary.requirement != "root-target")
        );
        assert!(
            composite
                .boundary_requirements
                .iter()
                .all(|boundary| !boundary.port_options.is_empty())
        );
        let html = render_constructive_composition_html(&report, None)
            .expect("constructive composition should render");
        assert!(html.contains("constructive-composition-boundary"));
        assert!(html.contains("FEASIBLE"));
    }
}
