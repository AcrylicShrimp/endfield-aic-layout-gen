//! Explicit offline diagnostic. Never called by the connected production constructor.
#[cfg(test)]
mod tests;
use std::collections::{BTreeMap, BTreeSet};
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
    FacilityPlacementStatus, project_facility_ports,
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
    pub placement_index: usize,
    pub nearest_port_distance: Option<i64>,
    pub port_trials: Vec<network::PortPairTrial>,
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
    pub per_connection_astar_limit: u64,
    pub per_pair_path_variant_limit: usize,
    pub connection_order: &'static str,
    pub required_internal: usize,
    pub unresolved_internal: Vec<String>,
    pub unresolved_external: Vec<String>,
    pub initial_empty_domains: Vec<String>,
    pub attempts: Vec<FixedRoutingAttempt>,
    pub first_failure_state: Option<ConstructiveNode>,
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
    let mut first_failure_state = None;
    let indices = input
        .placements
        .iter()
        .enumerate()
        .map(|(i, p)| (p.instance.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    let mut order = input
        .internal_requirements
        .iter()
        .map(|edge| {
            let source = boundaries
                .iter()
                .find(|b| b.requirement == edge.id && b.direction == FacilityPortDirection::Output)
                .unwrap();
            let target = boundaries
                .iter()
                .find(|b| b.requirement == edge.id && b.direction == FacilityPortDirection::Input)
                .unwrap();
            let distance = source
                .attachments
                .iter()
                .flat_map(|a| {
                    target.attachments.iter().map(move |b| {
                        (a.port().connection.x - b.port().connection.x).abs()
                            + (a.port().connection.y - b.port().connection.y).abs()
                    })
                })
                .min();
            (
                edge,
                indices[edge.source.as_str()].max(indices[edge.target.as_str()]),
                distance,
            )
        })
        .collect::<Vec<_>>();
    order.sort_by_key(|(e, index, distance)| {
        (
            *index,
            distance.unwrap_or(i64::MAX),
            usize::from(items.item(&e.item).unwrap().transport == TransportKind::Pipe),
            e.id.clone(),
        )
    });
    for (edge, placement_index, nearest_port_distance) in order {
        let step_start = Instant::now();
        let domains = refreshed(&node, &boundaries, transports, components);
        let live = domains
            .iter()
            .filter(|b| b.requirement != edge.id && !b.attachments.is_empty())
            .map(key)
            .collect::<BTreeSet<_>>();
        let mut nearest = network::NearestPortSearch {
            guards: domains
                .iter()
                .filter(|b| {
                    b.requirement != edge.id
                        && b.inside_instance != edge.source
                        && b.inside_instance != edge.target
                        && b.transport == items.item(&edge.item).unwrap().transport
                        && !b.attachments.is_empty()
                        && b.attachments
                            .iter()
                            .all(|a| matches!(a, Attachment::NewPort { .. }))
                })
                .map(|b| network::PortDomainGuard {
                    requirement: b.requirement.clone(),
                    cells: b
                        .attachments
                        .iter()
                        .map(|a| a.port().connection.clone())
                        .collect(),
                })
                .collect(),
            ..Default::default()
        };
        let mut trial = node.clone();
        // Retain every compatible port for the remaining lanes of this requirement.
        trial.boundary_requirements = domains
            .into_iter()
            .filter(|b| b.requirement == edge.id)
            .collect();
        let mut stats = ConstructiveCompositionStatistics::default();
        let mut invalid = 0;
        let mut lost = 0;
        let mut rejected_future = BTreeSet::new();
        let mut validation_examples = Vec::new();
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
        let result = network::connect_nearest(
            &trial,
            edge,
            transports,
            components,
            &mut workspace,
            &mut stats,
            limit,
            &mut nearest,
            &mut accept,
        );
        let connected = result.is_some();
        let stage = if !connected {
            "unresolved"
        } else if nearest
            .trials
            .iter()
            .filter(|t| t.connected)
            .all(|t| t.straight)
        {
            "straight-pairs"
        } else {
            "alternate-ports-astar"
        };
        if let Some(candidate) = result {
            node = candidate;
        } else if first_failure_state.is_none() {
            let mut snapshot = node.clone();
            snapshot.boundary_requirements =
                refreshed(&snapshot, &boundaries, transports, components);
            network::report_capacity(&mut snapshot, transports)?;
            first_failure_state = Some(snapshot);
        }
        attempts.push(FixedRoutingAttempt {
            requirement: edge.id.clone(),
            placement_index,
            nearest_port_distance,
            port_pairs: nearest.trials.len(),
            port_trials: nearest.trials,
            stage,
            connected,
            astar_searches: stats.astar_searches,
            witness_rejections: invalid,
            future_domain_rejections: lost,
            rejected_future_requirements: rejected_future.into_iter().collect(),
            validation_examples,
            elapsed_ms: step_start.elapsed().as_secs_f64() * 1000.0,
        });
    }
    node.boundary_requirements = refreshed(&node, &boundaries, transports, components);
    network::report_capacity(&mut node, transports)?;
    network::validate(&node, facilities, transports, components)?;
    Ok(FixedRoutingReport {
        schema_version: 3,
        kind: "fixed-placement-routing-probe",
        factory_validated: false,
        covered_routes_validated: true,
        coordinates_frozen: true,
        search_translation: padding,
        search_canvas: canvas,
        per_connection_astar_limit: limit,
        per_pair_path_variant_limit: network::PORT_PRESERVATION_VARIANTS,
        connection_order: "placement-index/nearest-port-distance/transport-tie/stable-id",
        required_internal: input.internal_requirements.len(),
        unresolved_internal: attempts
            .iter()
            .filter(|a| !a.connected)
            .map(|a| a.requirement.clone())
            .collect(),
        unresolved_external: input
            .external_requirements
            .iter()
            .map(|e| e.id.clone())
            .collect(),
        initial_empty_domains,
        attempts,
        first_failure_state,
        final_node: node,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}
