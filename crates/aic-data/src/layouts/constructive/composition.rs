use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, PlacedFacilityPort, SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
    WorldGridPosition, project_facility_ports,
};
use crate::logistics::{TransportKind, ValidatedItemCatalog, ValidatedTransportCatalog};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport,
};

use super::capacity::split_rate_into_lanes;
use super::first_pipe_frontier::{bounds_for, occupied_cells, rectangles_overlap};
use super::routing::RouteWorkspace;
use super::{
    CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION, ConstructiveCompositionReport,
    ConstructiveCompositionScore, ConstructiveCompositionStatistics,
    ConstructiveFrontierDiagnostic, ConstructiveNode, ConstructiveProcessModuleBoundary,
    ConstructiveProcessModuleReport,
};
use super::{ConstructiveBoundaryAttachment as Attachment, network};
use crate::logistics::ValidatedLogisticsComponentCatalog;

const TARGET_OFFSET: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateOrder {
    rotation: usize,
    y: i64,
    x: i64,
    bundle: usize,
}

#[derive(Debug, Clone, Copy)]
struct PlacementCandidate {
    x: i64,
    y: i64,
    area_lower_bound: usize,
    additive: bool,
}

impl PlacementCandidate {
    fn new(
        x: i64,
        y: i64,
        source: &FacilityPlacementBounds,
        target: &FacilityPlacementBounds,
    ) -> Self {
        let source_right = x + source.width;
        let source_bottom = y + source.height;
        let target_right = TARGET_OFFSET + target.width;
        let target_bottom = TARGET_OFFSET + target.height;
        let width = source_right.max(target_right) - x.min(TARGET_OFFSET);
        let height = source_bottom.max(target_bottom) - y.min(TARGET_OFFSET);
        let horizontal_gap = axis_gap(x, source_right, TARGET_OFFSET, target_right);
        let vertical_gap = axis_gap(y, source_bottom, TARGET_OFFSET, target_bottom);
        let vertical_overlap = ranges_overlap(y, source_bottom, TARGET_OFFSET, target_bottom);
        let horizontal_overlap = ranges_overlap(x, source_right, TARGET_OFFSET, target_right);
        let additive = horizontal_gap.is_some_and(|gap| gap <= 1) && vertical_overlap
            || vertical_gap.is_some_and(|gap| gap <= 1) && horizontal_overlap;
        Self {
            x,
            y,
            area_lower_bound: usize::try_from(width.saturating_mul(height)).unwrap_or(usize::MAX),
            additive,
        }
    }
}

fn axis_gap(left_start: i64, left_end: i64, right_start: i64, right_end: i64) -> Option<i64> {
    if left_end <= right_start {
        Some(right_start - left_end)
    } else if right_end <= left_start {
        Some(left_start - right_end)
    } else {
        None
    }
}

fn ranges_overlap(left_start: i64, left_end: i64, right_start: i64, right_end: i64) -> bool {
    left_start < right_end && right_start < left_end
}

pub fn constructive_node_from_process_module(
    report: &ConstructiveProcessModuleReport,
    transports: &ValidatedTransportCatalog,
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
    let mut node = ConstructiveNode {
        id: format!(
            "process-module:{}:{}",
            report.root_instance, report.internal_item
        ),
        bounds,
        member_instances: report.member_instances.clone(),
        internal_requirements: report.internal_requirements.clone(),
        placements: report.growth.placements.clone(),
        transport_networks: {
            let mut networks = report.growth.transport_networks.clone();
            network::normalize(&mut networks);
            networks
        },
        logistics_components: Vec::new(),
        network_capacity: Vec::new(),
        attachment_events: Vec::new(),
        boundary_requirements: report.boundary_requirements.clone(),
    };
    network::report_capacity(&mut node, transports)?;
    Ok(node)
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
            attachments: port_options
                .into_iter()
                .map(|port| Attachment::NewPort { port })
                .collect(),
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
        logistics_components: Vec::new(),
        network_capacity: Vec::new(),
        attachment_events: Vec::new(),
        boundary_requirements: boundaries,
    })
}

pub fn compose_process_module_with_facility(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    source_module: &ConstructiveProcessModuleReport,
    target_instance: &str,
    requirement: &str,
) -> ConstructiveCompositionReport {
    let source = match constructive_node_from_process_module(source_module, transports) {
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
    compose_constructive_nodes(&source, &target, edge, transports, facilities, components)
}

pub fn compose_constructive_nodes(
    source: &ConstructiveNode,
    target: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transports: &ValidatedTransportCatalog,
    facilities: &ValidatedFacilityCatalog,
    components: &ValidatedLogisticsComponentCatalog,
) -> ConstructiveCompositionReport {
    let best_area = AtomicUsize::new(usize::MAX);
    compose_constructive_nodes_with_area_incumbent(
        source, target, edge, transports, facilities, components, &best_area,
    )
}

pub(super) fn compose_constructive_nodes_with_area_incumbent(
    source: &ConstructiveNode,
    target: &ConstructiveNode,
    edge: &FacilityInstanceWiringEdge,
    transports: &ValidatedTransportCatalog,
    facilities: &ValidatedFacilityCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    best_area: &AtomicUsize,
) -> ConstructiveCompositionReport {
    for node in [source, target] {
        if let Err(diagnostic) = network::validate(node, facilities, transports, components) {
            return failed_report(&edge.id, &source.id, &target.id, diagnostic);
        }
    }
    if source
        .member_instances
        .iter()
        .any(|id| target.member_instances.contains(id))
    {
        return failed_report(
            &edge.id,
            &source.id,
            &target.id,
            ConstructiveFrontierDiagnostic::error(
                "overlapping-constructive-composition-members",
                "/composition",
                Some(edge.id.clone()),
                "nodes contain the same facility instance",
            ),
        );
    }
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
        || source_boundary.item != edge.item
        || source_boundary.rate != edge.rate
        || target_boundary.rate != edge.rate
        || source_boundary.inside_instance != edge.source
        || target_boundary.inside_instance != edge.target
    {
        return failed_report(
            &edge.id,
            &source.id,
            &target.id,
            ConstructiveFrontierDiagnostic::error(
                "composition-boundary-mismatch",
                "/requirement",
                Some(edge.id.clone()),
                "composition boundary instance, item, transport, or rate does not match",
            ),
        );
    }

    let lane_rates =
        match split_rate_into_lanes(edge.rate, source_boundary.transport, transports, &edge.id) {
            Ok(rates) => rates,
            Err(diagnostic) => {
                return failed_report(&edge.id, &source.id, &target.id, diagnostic);
            }
        };

    let canvas_width = source.bounds.width + target.bounds.width + 10;
    let canvas_height = source.bounds.height + target.bounds.height + 10;
    let target_candidate = translate_node(target, TARGET_OFFSET, TARGET_OFFSET);
    let mut route_workspace = RouteWorkspace::new(canvas_width, canvas_height);
    let mut statistics = ConstructiveCompositionStatistics::default();
    let mut best: Option<(
        ConstructiveCompositionScore,
        CandidateOrder,
        ConstructiveNode,
    )> = None;
    for (rotation_index, rotation) in [0, 90, 180, 270].into_iter().enumerate() {
        statistics.rotations_considered += 1;
        let rotated = rotate_node(source, rotation);
        if !node_rotations_are_legal(&rotated, facilities)
            || rotated.logistics_components.iter().any(|c| {
                components
                    .component(&c.component)
                    .is_none_or(|d| !d.allowed_rotations.contains(&c.rotation))
            })
        {
            continue;
        }
        let source_bounds = &rotated.bounds;
        let target_bounds = &target_candidate.bounds;
        let mut placements = (1..canvas_height - rotated.bounds.height)
            .flat_map(|y| {
                (1..canvas_width - rotated.bounds.width)
                    .map(move |x| PlacementCandidate::new(x, y, source_bounds, target_bounds))
            })
            .collect::<Vec<_>>();
        placements.sort_by_key(|candidate| {
            (
                !candidate.additive,
                candidate.area_lower_bound,
                candidate.y,
                candidate.x,
            )
        });
        for placement in placements {
            statistics.placements_considered += 1;
            statistics.additive_placements_considered += u64::from(placement.additive);
            if placement.area_lower_bound > best_area.load(AtomicOrdering::Relaxed) {
                statistics.area_lower_bound_rejections += 1;
                continue;
            }
            let source_candidate = translate_node(&rotated, placement.x, placement.y);
            if nodes_collide(&source_candidate, &target_candidate) {
                statistics.colliding_placements_rejected += 1;
                continue;
            }
            let mut merged = merge_nodes(&source_candidate, &target_candidate);
            network::normalize(&mut merged.transport_networks);
            let bundles = network::connect(
                &merged,
                edge,
                source_boundary.transport,
                &lane_rates,
                transports,
                components,
                &mut route_workspace,
                &mut statistics,
            );
            for (bundle_index, composite) in bundles.into_iter().enumerate() {
                let mut composite = canonicalize_node(&composite);
                if let Err(diagnostic) =
                    network::validate(&composite, facilities, transports, components)
                {
                    statistics.geometry_rejections += 1;
                    *statistics
                        .geometry_failure_reasons
                        .entry(diagnostic.message)
                        .or_default() += 1;
                    continue;
                }
                if composite.logistics_components.len()
                    > source.logistics_components.len() + target.logistics_components.len()
                {
                    statistics.network_attachment_candidates_validated += 1;
                }
                let options_before = source
                    .boundary_requirements
                    .iter()
                    .chain(&target.boundary_requirements)
                    .filter(|b| b.requirement != edge.id)
                    .flat_map(|b| &b.attachments)
                    .filter(|a| matches!(a, Attachment::NewPort { .. }))
                    .count();
                let options_after = composite
                    .boundary_requirements
                    .iter()
                    .flat_map(|b| &b.attachments)
                    .filter(|a| matches!(a, Attachment::NewPort { .. }))
                    .count();
                let blocked_options = options_before.saturating_sub(options_after);
                if network::report_capacity(&mut composite, transports).is_err() {
                    statistics.attachment_capacity_rejections += 1;
                    continue;
                }
                statistics.valid_candidates_scored += 1;
                let score = composition_score(&composite, blocked_options);
                best_area.fetch_min(score.used_bounding_box_area, AtomicOrdering::Relaxed);
                let candidate_order = CandidateOrder {
                    rotation: rotation_index,
                    y: placement.y,
                    x: placement.x,
                    bundle: bundle_index,
                };
                if best.as_ref().is_none_or(|(current, current_order, _)| {
                    (score, candidate_order) < (*current, *current_order)
                }) {
                    best = Some((score, candidate_order, composite));
                }
            }
        }
    }

    let Some((score, _, composite)) = best else {
        let mut diagnostics = Vec::new();
        for requirement in statistics.boundary_requirement_rejections.keys() {
            diagnostics.push(ConstructiveFrontierDiagnostic::error(
                "constructive-network-attachment-unavailable",
                "/boundary_requirements",
                Some(requirement.clone()),
                "no unoccupied compatible port or realizable catalog branch anchor remains",
            ));
        }
        for (count, code, message) in [
            (
                statistics.attachment_capacity_rejections
                    + statistics.boundary_capacity_dead_ends_rejected,
                "constructive-network-attachment-capacity-insufficient",
                "shared segment, component, or remaining port capacity rejected attachment candidates",
            ),
            (
                statistics.branch_route_failures,
                "constructive-network-branch-route-unavailable",
                "branch anchors were available but branch routing failed",
            ),
            (
                statistics.geometry_rejections,
                "constructive-network-attachment-geometry-invalid",
                "independent network validation rejected completed candidates",
            ),
        ] {
            if count > 0 {
                diagnostics.push(ConstructiveFrontierDiagnostic::error(
                    code,
                    "/composition",
                    Some(edge.id.clone()),
                    message,
                ));
            }
        }
        diagnostics.push(ConstructiveFrontierDiagnostic::error(
            "constructive-composition-exhausted",
            "/composition",
            Some(edge.id.clone()),
            "exhausted rigid placement, attachment, and shortest-path lane candidates",
        ));
        return ConstructiveCompositionReport {
            schema_version: CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION,
            success: false,
            requirement: edge.id.clone(),
            source_node: source.id.clone(),
            target_node: target.id.clone(),
            score: None,
            composite: None,
            statistics,
            diagnostics,
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

pub(super) fn merge_nodes(
    source: &ConstructiveNode,
    target: &ConstructiveNode,
) -> ConstructiveNode {
    let mut node = target.clone();
    node.id = format!("composite:{}+{}", target.id, source.id);
    node.placements.extend(source.placements.clone());
    node.member_instances
        .extend(source.member_instances.clone());
    node.member_instances.sort();
    node.internal_requirements
        .extend(source.internal_requirements.clone());
    node.internal_requirements.sort();
    node.transport_networks
        .extend(source.transport_networks.clone());
    node.logistics_components
        .extend(source.logistics_components.clone());
    node.attachment_events
        .extend(source.attachment_events.clone());
    node.network_capacity.clear();
    node.boundary_requirements
        .extend(source.boundary_requirements.clone());
    node
}

pub(super) fn rotate_node(node: &ConstructiveNode, rotation: i64) -> ConstructiveNode {
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
    for component in &mut rotated.logistics_components {
        component.position = rotate_point(
            &component.position,
            node.bounds.width,
            node.bounds.height,
            rotation,
        );
        component.rotation = (component.rotation + rotation) % 360;
    }
    for capacity in &mut rotated.network_capacity {
        for segment in &mut capacity.segments {
            segment.from = rotate_point(
                &segment.from,
                node.bounds.width,
                node.bounds.height,
                rotation,
            );
            segment.to = rotate_point(&segment.to, node.bounds.width, node.bounds.height, rotation);
        }
    }
    for boundary in &mut rotated.boundary_requirements {
        for attachment in &mut boundary.attachments {
            if let Attachment::ExistingNetwork {
                anchor,
                branch_cell,
                component,
                ..
            } = attachment
            {
                *anchor = rotate_point(anchor, node.bounds.width, node.bounds.height, rotation);
                *branch_cell =
                    rotate_point(branch_cell, node.bounds.width, node.bounds.height, rotation);
                component.position = anchor.clone();
                component.rotation = (component.rotation + rotation) % 360;
            }
            let port = attachment.port_mut();
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

pub(super) fn translate_node(node: &ConstructiveNode, dx: i64, dy: i64) -> ConstructiveNode {
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
    for component in &mut translated.logistics_components {
        component.position.x += dx;
        component.position.y += dy;
    }
    for capacity in &mut translated.network_capacity {
        for segment in &mut capacity.segments {
            segment.from.x += dx;
            segment.from.y += dy;
            segment.to.x += dx;
            segment.to.y += dy;
        }
    }
    for boundary in &mut translated.boundary_requirements {
        for attachment in &mut boundary.attachments {
            if let Attachment::ExistingNetwork {
                anchor,
                branch_cell,
                component,
                ..
            } = attachment
            {
                anchor.x += dx;
                anchor.y += dy;
                branch_cell.x += dx;
                branch_cell.y += dy;
                component.position = anchor.clone();
            }
            translate_port(attachment.port_mut(), dx, dy);
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

pub(super) fn canonicalize_node(node: &ConstructiveNode) -> ConstructiveNode {
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
        route_turns: node.transport_networks.iter().map(network::turns).sum(),
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
    use crate::logistics::{
        ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
        SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
        TransportDefinition, TransportKind, ValidatedTransportCatalog,
    };
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

    fn transports() -> ValidatedTransportCatalog {
        ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 1_000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        })
        .expect("transport catalog validates")
    }

    #[test]
    fn composes_an_immutable_process_module_with_a_facility_node() {
        let mut wiring = FacilityInstanceWiringReport {
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
        wiring
            .edges
            .iter_mut()
            .find(|edge| edge.id == "root-target")
            .expect("composition edge")
            .rate = Rate {
            numerator: 2,
            denominator: 1,
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

        let module = construct_process_module(
            &wiring,
            &facilities,
            &items,
            &transports(),
            &super::super::test_components(),
            "root",
            "internal",
        );
        assert!(module.success, "{:?}", module.growth.diagnostics);
        let report = compose_process_module_with_facility(
            &wiring,
            &facilities,
            &items,
            &transports(),
            &super::super::test_components(),
            &module,
            "target",
            "root-target",
        );

        assert!(report.success, "{:?}", report.diagnostics);
        assert!(report.statistics.additive_placements_considered > 0);
        assert!(report.statistics.area_lower_bound_rejections > 0);
        let composite = report.composite.as_ref().expect("composite node");
        assert_eq!(composite.placements.len(), 4);
        assert_eq!(composite.transport_networks.len(), 2);
        let product_network = composite
            .transport_networks
            .iter()
            .find(|n| n.item == "product")
            .unwrap();
        assert_eq!(product_network.terminals.len(), 4);
        assert!(product_network.terminals.iter().all(|t| t.rate
            == Rate {
                numerator: 1,
                denominator: 1
            }));
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
                .all(|boundary| !boundary.attachments.is_empty())
        );
        let html = render_constructive_composition_html(&report, None)
            .expect("constructive composition should render");
        assert!(html.contains("constructive-composition-boundary"));
        assert!(html.contains("FEASIBLE"));
    }

    #[test]
    fn placement_lower_bound_recognizes_adjacent_additions() {
        let target = FacilityPlacementBounds {
            width: 4,
            height: 4,
        };
        let source = FacilityPlacementBounds {
            width: 2,
            height: 2,
        };

        let adjacent = PlacementCandidate::new(8, 5, &source, &target);
        assert!(adjacent.additive);
        assert_eq!(adjacent.area_lower_bound, 24);

        let distant = PlacementCandidate::new(12, 5, &source, &target);
        assert!(!distant.additive);
        assert_eq!(distant.area_lower_bound, 40);
    }

    #[test]
    fn compact_geometry_precedes_unused_port_flexibility() {
        let compact = ConstructiveCompositionScore {
            used_bounding_box_area: 100,
            transport_tiles: 20,
            route_turns: 3,
            blocked_boundary_port_options: 8,
        };
        let flexible = ConstructiveCompositionScore {
            used_bounding_box_area: 101,
            transport_tiles: 1,
            route_turns: 0,
            blocked_boundary_port_options: 0,
        };

        assert!(compact < flexible);
    }
}
