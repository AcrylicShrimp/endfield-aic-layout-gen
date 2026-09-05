//! Explicit offline diagnostic. Never called by the connected production constructor.
#[cfg(test)]
mod tests;
use std::collections::{BTreeSet, HashSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::network;
use super::routing::RouteWorkspace;
use super::{
    ConstructiveBoundaryAttachment as Attachment, ConstructiveCompositionStatistics,
    ConstructiveFrontierDiagnostic, ConstructiveNode, ConstructiveProcessModuleBoundary,
};
use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, PlacedFacilityPort, project_facility_ports,
};
use crate::logistics::{
    TransportKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringEdge;

#[derive(Debug, Deserialize)]
pub struct FixedRoutingInput {
    pub schema_version: u32,
    pub kind: String,
    pub routing_performed: bool,
    pub factory_validated: bool,
    pub required_facilities: usize,
    pub placements: Vec<FacilityPlacement>,
    pub internal_requirements: Vec<FacilityInstanceWiringEdge>,
    pub external_requirements: Vec<FacilityInstanceWiringEdge>,
}

#[derive(Debug, Serialize)]
pub struct FixedRoutingAttempt {
    pub requirement: String,
    pub stage: &'static str,
    pub connected: bool,
    pub port_pairs: usize,
    pub astar_searches: u64,
    pub witness_rejections: usize,
    pub future_domain_rejections: usize,
    pub rejected_future_requirements: Vec<String>,
    pub validation_examples: Vec<ConstructiveFrontierDiagnostic>,
    pub elapsed_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct FixedRoutingReport {
    pub schema_version: u32,
    pub kind: &'static str,
    pub factory_validated: bool,
    pub covered_routes_validated: bool,
    pub coordinates_frozen: bool,
    pub search_translation: i64,
    pub search_canvas: FacilityPlacementBounds,
    pub per_stage_connection_astar_limit: u64,
    pub required_internal: usize,
    pub unresolved_internal: Vec<String>,
    pub unresolved_external: Vec<String>,
    pub initial_empty_domains: Vec<String>,
    pub attempts: Vec<FixedRoutingAttempt>,
    pub final_node: ConstructiveNode,
    pub elapsed_ms: f64,
}

fn error(entity: impl Into<String>, message: impl Into<String>) -> ConstructiveFrontierDiagnostic {
    ConstructiveFrontierDiagnostic::error(
        "fixed-routing-input-invalid",
        "/fixed_placement",
        Some(entity.into()),
        message,
    )
}

fn key(b: &ConstructiveProcessModuleBoundary) -> String {
    format!("{}:{}:{:?}", b.requirement, b.inside_instance, b.direction)
}

fn refreshed(
    node: &ConstructiveNode,
    boundaries: &[ConstructiveProcessModuleBoundary],
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
) -> Vec<ConstructiveProcessModuleBoundary> {
    let mut trial = node.clone();
    trial.boundary_requirements = boundaries
        .iter()
        .filter(|b| !node.internal_requirements.contains(&b.requirement))
        .cloned()
        .collect();
    network::refresh(&mut trial, transports, components, &mut Default::default());
    trial.boundary_requirements
}

fn straight_clear(
    source: &PlacedFacilityPort,
    target: &PlacedFacilityPort,
    node: &ConstructiveNode,
    blocked: &HashSet<(i64, i64)>,
) -> bool {
    // A shared attachment may branch away from the facility. Leave it to the general stage.
    if node
        .transport_networks
        .iter()
        .flat_map(|n| &n.terminals)
        .any(|t| match &t.endpoint {
            crate::layouts::TransportNetworkEndpoint::Facility { instance, port } => {
                [source, target]
                    .iter()
                    .any(|p| p.instance == *instance && p.port == *port)
            }
            _ => false,
        })
    {
        return false;
    }
    let dx = source.connection.x - source.position.x;
    let dy = source.connection.y - source.position.y;
    if target.connection.x - target.position.x != -dx
        || target.connection.y - target.position.y != -dy
    {
        return false;
    }
    let vx = target.connection.x - source.connection.x;
    let vy = target.connection.y - source.connection.y;
    let distance = vx * dx + vy * dy;
    if distance < 0 || vx * dy != vy * dx {
        return false;
    }
    (0..=distance)
        .all(|i| !blocked.contains(&(source.connection.x + i * dx, source.connection.y + i * dy)))
}

/// Search limits are diagnostic bounds, never production canvas constraints or infeasibility proofs.
#[allow(clippy::too_many_arguments)]
pub fn probe_fixed_routing(
    input: &FixedRoutingInput,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    padding: i64,
    limit: u64,
) -> Result<FixedRoutingReport, ConstructiveFrontierDiagnostic> {
    let start = Instant::now();
    if input.schema_version != 1
        || input.kind != "placement-only-visual-diagnostic"
        || input.routing_performed
        || input.factory_validated
        || input.placements.is_empty()
        || input.placements.len() != input.required_facilities
        || !(1..=128).contains(&padding)
        || !(1..=4096).contains(&limit)
    {
        return Err(error(
            "request",
            "Expected unrouted placement v1, padding 1..128 and A* limit 1..4096",
        ));
    }
    let ids = input
        .placements
        .iter()
        .map(|p| p.instance.clone())
        .collect::<BTreeSet<_>>();
    if ids.len() != input.placements.len() {
        return Err(error("placements", "Duplicate instances"));
    }
    let mut right = 0;
    let mut bottom = 0;
    for p in &input.placements {
        if p.x < 0
            || p.y < 0
            || p.width < 1
            || p.height < 1
            || p.x > 100_000
            || p.y > 100_000
            || p.width > 100_000
            || p.height > 100_000
        {
            return Err(error(
                &p.instance,
                "Unsafe or negative geometry in diagnostic input",
            ));
        }
        right = right.max(p.x + p.width);
        bottom = bottom.max(p.y + p.height);
    }
    let canvas = FacilityPlacementBounds {
        width: right + 2 * padding,
        height: bottom + 2 * padding,
    };
    if canvas.width * canvas.height > 250_000 {
        return Err(error(
            "search-canvas",
            "Diagnostic routing workspace exceeds 250000-cell safety limit",
        ));
    }
    let placements = input
        .placements
        .iter()
        .map(|p| FacilityPlacement {
            x: p.x + padding,
            y: p.y + padding,
            ..p.clone()
        })
        .collect::<Vec<_>>();
    let projection = project_facility_ports(
        &FacilityPlacementReport {
            success: true,
            status: FacilityPlacementStatus::Feasible,
            bounds: Some(canvas.clone()),
            placements: placements.clone(),
            diagnostics: vec![],
        },
        facilities,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: canvas.width,
            max_height: canvas.height,
        },
    );
    if !projection.success {
        return Err(error(
            "placements",
            "Runtime catalog port projection failed",
        ));
    }
    let mut boundaries = Vec::new();
    let mut edge_ids = BTreeSet::new();
    for (internal, edge) in input
        .internal_requirements
        .iter()
        .map(|e| (true, e))
        .chain(input.external_requirements.iter().map(|e| (false, e)))
    {
        if !edge_ids.insert(edge.id.clone())
            || edge.rate.numerator <= 0
            || edge.rate.denominator <= 0
            || internal != (ids.contains(&edge.source) && ids.contains(&edge.target))
            || !ids.contains(&edge.source) && !ids.contains(&edge.target)
        {
            return Err(error(
                &edge.id,
                "Duplicate requirement, invalid rate or inconsistent internal/external endpoints",
            ));
        }
        let item = items
            .item(&edge.item)
            .ok_or_else(|| error(&edge.item, "Unknown requirement item"))?;
        for (inside, direction) in [
            (&edge.source, FacilityPortDirection::Output),
            (&edge.target, FacilityPortDirection::Input),
        ] {
            if !ids.contains(inside) {
                continue;
            }
            boundaries.push(ConstructiveProcessModuleBoundary {
                requirement: edge.id.clone(),
                item: edge.item.clone(),
                transport: item.transport,
                rate: edge.rate,
                direction,
                inside_instance: inside.clone(),
                attachments: projection
                    .ports
                    .iter()
                    .filter(|p| {
                        p.instance == *inside
                            && p.direction == direction
                            && p.transport == item.transport
                    })
                    .cloned()
                    .map(|port| Attachment::NewPort { port })
                    .collect(),
            });
        }
    }
    let mut node = ConstructiveNode {
        id: "fixed-placement-probe".into(),
        bounds: canvas.clone(),
        member_instances: ids.into_iter().collect(),
        placements,
        internal_requirements: vec![],
        external_requirements: vec![],
        transport_networks: vec![],
        logistics_components: vec![],
        network_capacity: vec![],
        attachment_events: vec![],
        boundary_requirements: vec![],
    };
    network::validate(&node, facilities, transports, components)?;
    let initial = refreshed(&node, &boundaries, transports, components);
    let initial_empty_domains = initial
        .iter()
        .filter(|b| b.attachments.is_empty())
        .map(key)
        .collect();
    let mut workspace = RouteWorkspace::new(canvas.width, canvas.height);
    let mut attempts = Vec::new();
    let facilities_blocked = super::first_pipe_frontier::occupied_cells(&node.placements);
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        for direct in [true, false] {
            for edge in &input.internal_requirements {
                if items.item(&edge.item).unwrap().transport != transport
                    || node.internal_requirements.contains(&edge.id)
                {
                    continue;
                }
                let step_start = Instant::now();
                // A straight success must be one physical lane, not a shared multi-lane bundle.
                let capacity = transports.capacity(transport);
                let lane_rate = crate::recipes::Rate::from_quantity_per_duration_ms(
                    capacity.quantity,
                    capacity.duration_ms,
                )
                .map_err(|_| error(&edge.id, "Invalid runtime lane capacity"))?;
                if direct && edge.rate > lane_rate {
                    continue;
                }
                let domains = refreshed(&node, &boundaries, transports, components);
                let live = domains
                    .iter()
                    .filter(|b| b.requirement != edge.id && !b.attachments.is_empty())
                    .map(key)
                    .collect::<BTreeSet<_>>();
                let source = domains
                    .iter()
                    .find(|b| {
                        b.requirement == edge.id && b.direction == FacilityPortDirection::Output
                    })
                    .unwrap();
                let target = domains
                    .iter()
                    .find(|b| {
                        b.requirement == edge.id && b.direction == FacilityPortDirection::Input
                    })
                    .unwrap();
                let ports = |b: &ConstructiveProcessModuleBoundary| {
                    let mut seen = BTreeSet::new();
                    b.attachments
                        .iter()
                        .map(Attachment::port)
                        .filter(|p| seen.insert(p.port.clone()))
                        .cloned()
                        .collect::<Vec<_>>()
                };
                let sources = ports(source);
                let targets = ports(target);
                let mut blocked = facilities_blocked.clone();
                blocked.extend(
                    node.transport_networks
                        .iter()
                        .filter(|n| n.transport == transport)
                        .flat_map(|n| n.cells.iter().map(|p| (p.x, p.y))),
                );
                let mut pairs = sources
                    .iter()
                    .flat_map(|a| targets.iter().map(move |b| (a, b)))
                    .filter(|(a, b)| !direct || straight_clear(a, b, &node, &blocked))
                    .collect::<Vec<_>>();
                pairs.sort_by_key(|(a, b)| {
                    (
                        (a.connection.x - b.connection.x).abs()
                            + (a.connection.y - b.connection.y).abs(),
                        a.port.clone(),
                        b.port.clone(),
                    )
                });
                let mut stats = ConstructiveCompositionStatistics::default();
                let mut tried = 0;
                let mut invalid = 0;
                let mut lost = 0;
                let mut rejected_future = BTreeSet::new();
                let mut validation_examples = Vec::new();
                let mut result = None;
                for (a, b) in pairs {
                    if stats.astar_searches >= limit {
                        break;
                    }
                    tried += 1;
                    let mut trial = node.clone();
                    trial.boundary_requirements = vec![
                        ConstructiveProcessModuleBoundary {
                            attachments: vec![Attachment::NewPort { port: a.clone() }],
                            ..source.clone()
                        },
                        ConstructiveProcessModuleBoundary {
                            attachments: vec![Attachment::NewPort { port: b.clone() }],
                            ..target.clone()
                        },
                    ];
                    let mut accept = |candidate: &ConstructiveNode| {
                        if let Err(diagnostic) =
                            network::validate(candidate, facilities, transports, components)
                        {
                            invalid += 1;
                            if validation_examples.len() < 4 {
                                validation_examples.push(diagnostic);
                            }
                            return false;
                        }
                        let newly_empty = refreshed(candidate, &boundaries, transports, components)
                            .into_iter()
                            .filter(|b| live.contains(&key(b)) && b.attachments.is_empty())
                            .map(|b| b.requirement)
                            .collect::<Vec<_>>();
                        if !newly_empty.is_empty() {
                            lost += 1;
                            rejected_future.extend(newly_empty);
                            return false;
                        }
                        true
                    };
                    if let Some(candidate) = network::connect_first(
                        &trial,
                        edge,
                        transports,
                        components,
                        &mut workspace,
                        &mut stats,
                        limit,
                        &mut accept,
                    ) {
                        result = Some(candidate);
                        break;
                    }
                }
                let connected = result.is_some();
                if let Some(candidate) = result {
                    node = candidate;
                }
                attempts.push(FixedRoutingAttempt {
                    requirement: edge.id.clone(),
                    stage: if direct {
                        "straight-pairs"
                    } else {
                        "alternate-ports-astar"
                    },
                    connected,
                    port_pairs: tried,
                    astar_searches: stats.astar_searches,
                    witness_rejections: invalid,
                    future_domain_rejections: lost,
                    rejected_future_requirements: rejected_future.into_iter().collect(),
                    validation_examples,
                    elapsed_ms: step_start.elapsed().as_secs_f64() * 1000.0,
                });
            }
        }
    }
    node.boundary_requirements = refreshed(&node, &boundaries, transports, components);
    network::report_capacity(&mut node, transports)?;
    network::validate(&node, facilities, transports, components)?;
    Ok(FixedRoutingReport {
        schema_version: 1,
        kind: "fixed-placement-routing-probe",
        factory_validated: false,
        covered_routes_validated: true,
        coordinates_frozen: true,
        search_translation: padding,
        search_canvas: canvas,
        per_stage_connection_astar_limit: limit,
        required_internal: input.internal_requirements.len(),
        unresolved_internal: input
            .internal_requirements
            .iter()
            .filter(|e| !node.internal_requirements.contains(&e.id))
            .map(|e| e.id.clone())
            .collect(),
        unresolved_external: input
            .external_requirements
            .iter()
            .map(|e| e.id.clone())
            .collect(),
        initial_empty_domains,
        attempts,
        final_node: node,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}
