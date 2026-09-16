//! Complete fixed-placement construction through the production network legalizer.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::Serialize;

use super::network::{self, NearestPortSearch, PortDomainGuard};
use super::routing::RouteWorkspace;
use super::{
    ConstructiveBoundaryAttachment as Attachment, ConstructiveCompositionStatistics,
    ConstructiveFrontierDiagnostic, ConstructiveNode, ConstructiveProcessModuleBoundary,
    FactoryEditCatalogs, FactoryEditRequest, FactoryLayoutAcceptance, WitnessProbeInput,
    validate_factory_layout,
};
use crate::facilities::FacilityPortDirection;
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, project_facility_ports,
};
use crate::logistics::TransportKind;
use crate::recipes::{FacilityInstanceWiringEdge, FacilityInstanceWiringReport};

#[derive(Debug, Clone, Serialize)]
pub struct FixedFactoryInput {
    pub schema_version: u32,
    pub placements: Vec<FacilityPlacement>,
    pub wiring: FacilityInstanceWiringReport,
    pub routing_seed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixedFactoryAttempt {
    pub order: &'static str,
    pub routed_internal: usize,
    pub routed_external: usize,
    pub astar_searches: u64,
    pub status: &'static str,
    pub failed_requirement: Option<String>,
    pub elapsed_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct FixedFactoryReport {
    pub schema_version: u32,
    pub success: bool,
    pub factory_validated: bool,
    pub padding: i64,
    pub canvas: FacilityPlacementBounds,
    pub attempts: Vec<FixedFactoryAttempt>,
    pub witness: Option<WitnessProbeInput>,
    pub acceptance: Option<FactoryLayoutAcceptance>,
    pub diagnostics: Vec<ConstructiveFrontierDiagnostic>,
    pub elapsed_ms: f64,
}

fn error(entity: impl Into<String>, message: impl Into<String>) -> ConstructiveFrontierDiagnostic {
    ConstructiveFrontierDiagnostic::error(
        "fixed-factory-input-invalid",
        "/fixed_factory",
        Some(entity.into()),
        message,
    )
}

fn key(boundary: &ConstructiveProcessModuleBoundary) -> String {
    format!(
        "{}:{}:{:?}",
        boundary.requirement, boundary.inside_instance, boundary.direction
    )
}

fn refreshed(
    node: &ConstructiveNode,
    original: &[ConstructiveProcessModuleBoundary],
    catalogs: &FactoryEditCatalogs,
) -> Vec<ConstructiveProcessModuleBoundary> {
    let resolved = node
        .internal_requirements
        .iter()
        .chain(&node.external_requirements)
        .collect::<BTreeSet<_>>();
    let mut trial = node.clone();
    trial.boundary_requirements = original
        .iter()
        .filter(|boundary| !resolved.contains(&boundary.requirement))
        .cloned()
        .collect();
    network::refresh(
        &mut trial,
        &catalogs.transports,
        &catalogs.components,
        &mut Default::default(),
    );
    trial.boundary_requirements
}

fn domains_survive(
    candidate: &ConstructiveNode,
    before: &BTreeSet<String>,
    original: &[ConstructiveProcessModuleBoundary],
    catalogs: &FactoryEditCatalogs,
) -> bool {
    refreshed(candidate, original, catalogs)
        .iter()
        .all(|boundary| !before.contains(&key(boundary)) || !boundary.attachments.is_empty())
}

fn order_edges<'a>(
    edges: &[&'a FacilityInstanceWiringEdge],
    boundaries: &[ConstructiveProcessModuleBoundary],
    mode: usize,
    seed: u64,
) -> Vec<&'a FacilityInstanceWiringEdge> {
    let mut result = edges.to_vec();
    let score = |edge: &FacilityInstanceWiringEdge| {
        let sides = boundaries
            .iter()
            .filter(|boundary| boundary.requirement == edge.id)
            .collect::<Vec<_>>();
        let pairs = sides
            .iter()
            .map(|boundary| boundary.attachments.len().max(1))
            .product::<usize>();
        let distance = sides
            .first()
            .into_iter()
            .flat_map(|source| {
                sides.get(1).into_iter().flat_map(move |target| {
                    source.attachments.iter().flat_map(move |a| {
                        target.attachments.iter().map(move |b| {
                            (a.port().connection.x - b.port().connection.x).abs()
                                + (a.port().connection.y - b.port().connection.y).abs()
                        })
                    })
                })
            })
            .min()
            .unwrap_or(i64::MAX);
        (pairs, distance)
    };
    result.sort_by_key(|edge| {
        let (pairs, distance) = score(edge);
        let pipe = usize::from(
            boundaries
                .iter()
                .find(|boundary| boundary.requirement == edge.id)
                .is_some_and(|boundary| boundary.transport == TransportKind::Pipe),
        );
        let stable_mix = edge
            .id
            .bytes()
            .fold(seed, |value, byte| value.rotate_left(5) ^ u64::from(byte));
        let item_mix = edge.item.bytes().fold(0_u64, |value, byte| {
            value.wrapping_mul(1_099_511_628_211) ^ u64::from(byte)
        });
        let position = boundaries
            .iter()
            .filter(|boundary| boundary.requirement == edge.id)
            .flat_map(|boundary| boundary.attachments.iter())
            .map(|attachment| {
                let point = &attachment.port().connection;
                (point.x.max(0) as u64).saturating_mul(1_000_001) + point.y.max(0) as u64
            })
            .min()
            .unwrap_or(u64::MAX);
        match mode {
            0 => (
                pairs as u64,
                u64::MAX - distance as u64,
                pipe as u64,
                stable_mix,
            ),
            1 => (
                u64::MAX - distance as u64,
                pairs as u64,
                pipe as u64,
                stable_mix,
            ),
            2 => (
                1 - pipe as u64,
                pairs as u64,
                u64::MAX - distance as u64,
                stable_mix,
            ),
            4 => (item_mix, position, pairs as u64, stable_mix),
            _ => (
                stable_mix,
                pairs as u64,
                u64::MAX - distance as u64,
                pipe as u64,
            ),
        }
    });
    result
}

fn route_internal(
    mut node: ConstructiveNode,
    edges: &[&FacilityInstanceWiringEdge],
    boundaries: &[ConstructiveProcessModuleBoundary],
    catalogs: &FactoryEditCatalogs,
    route_limit: u64,
    stats: &mut ConstructiveCompositionStatistics,
) -> Result<ConstructiveNode, String> {
    let mut workspace = RouteWorkspace::new(node.bounds.width, node.bounds.height);
    for edge in edges {
        let domains = refreshed(&node, boundaries, catalogs);
        let live = domains
            .iter()
            .filter(|boundary| boundary.requirement != edge.id && !boundary.attachments.is_empty())
            .map(key)
            .collect::<BTreeSet<_>>();
        let transport = catalogs
            .items
            .item(&edge.item)
            .expect("validated wiring item")
            .transport;
        let mut search = NearestPortSearch {
            guards: domains
                .iter()
                .filter(|boundary| {
                    boundary.requirement != edge.id
                        && boundary.inside_instance != edge.source
                        && boundary.inside_instance != edge.target
                        && boundary.transport == transport
                        && !boundary.attachments.is_empty()
                        && boundary
                            .attachments
                            .iter()
                            .all(|attachment| matches!(attachment, Attachment::NewPort { .. }))
                })
                .map(|boundary| PortDomainGuard {
                    requirement: boundary.requirement.clone(),
                    cells: boundary
                        .attachments
                        .iter()
                        .map(|attachment| attachment.port().connection.clone())
                        .collect(),
                })
                .collect(),
            ..Default::default()
        };
        let mut trial = node.clone();
        trial.boundary_requirements = domains
            .into_iter()
            .filter(|boundary| boundary.requirement == edge.id)
            .collect();
        let Some(next) = network::connect_nearest(
            &trial,
            edge,
            &catalogs.transports,
            &catalogs.components,
            &mut workspace,
            stats,
            route_limit,
            &mut search,
            &mut |candidate| {
                network::validate(
                    candidate,
                    &catalogs.facilities,
                    &catalogs.transports,
                    &catalogs.components,
                )
                .is_ok()
                    && domains_survive(candidate, &live, boundaries, catalogs)
            },
        ) else {
            return Err(edge.id.clone());
        };
        node = next;
    }
    Ok(node)
}

fn route_external(
    mut node: ConstructiveNode,
    edges: &[&FacilityInstanceWiringEdge],
    boundaries: &[ConstructiveProcessModuleBoundary],
    catalogs: &FactoryEditCatalogs,
    route_limit: u64,
    stats: &mut ConstructiveCompositionStatistics,
) -> Result<ConstructiveNode, String> {
    for edge in edges {
        node.boundary_requirements = refreshed(&node, boundaries, catalogs);
        let live = node
            .boundary_requirements
            .iter()
            .filter(|boundary| boundary.requirement != edge.id && !boundary.attachments.is_empty())
            .map(key)
            .collect::<BTreeSet<_>>();
        let Some(next) = network::connect_external(
            &node,
            edge,
            node.bounds.width,
            node.bounds.height,
            &catalogs.transports,
            &catalogs.components,
            stats,
            route_limit,
            &mut |candidate| {
                network::validate(
                    candidate,
                    &catalogs.facilities,
                    &catalogs.transports,
                    &catalogs.components,
                )
                .is_ok()
                    && domains_survive(candidate, &live, boundaries, catalogs)
            },
        ) else {
            return Err(edge.id.clone());
        };
        node = next;
    }
    Ok(node)
}

pub fn construct_fixed_factory(
    input: &FixedFactoryInput,
    request: &FactoryEditRequest,
    catalogs: &FactoryEditCatalogs,
    padding: i64,
    route_limit: u64,
) -> FixedFactoryReport {
    let started = Instant::now();
    let mut diagnostics = Vec::new();
    let mut attempts = Vec::new();
    let mut right = 0;
    let mut bottom = 0;
    let members = input
        .placements
        .iter()
        .map(|placement| placement.instance.clone())
        .collect::<BTreeSet<_>>();
    let invalid = input.schema_version != 1
        || !input.wiring.success
        || input.placements.is_empty()
        || input.placements.len() > 256
        || members.len() != input.placements.len()
        || !(1..=128).contains(&padding)
        || !(1..=1_000_000).contains(&route_limit);
    if invalid {
        diagnostics.push(error(
            "request",
            "Expected successful wiring, unique nonempty placements, padding 1..128 and route limit 1..1000000",
        ));
    }
    for placement in &input.placements {
        if placement.x < 0
            || placement.y < 0
            || placement.width < 1
            || placement.height < 1
            || placement.x > 1_000_000
            || placement.y > 1_000_000
        {
            diagnostics.push(error(&placement.instance, "Unsafe placement geometry"));
        }
        right = right.max(placement.x.saturating_add(placement.width));
        bottom = bottom.max(placement.y.saturating_add(placement.height));
    }
    let canvas = FacilityPlacementBounds {
        width: right.saturating_add(2 * padding),
        height: bottom.saturating_add(2 * padding),
    };
    if canvas.width.saturating_mul(canvas.height) > 2_000_000 {
        diagnostics.push(error("canvas", "Routing canvas exceeds 2000000 cells"));
    }
    let facility_nodes = input
        .wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            crate::recipes::FacilityInstanceWiringNode::Facility { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if members != facility_nodes {
        diagnostics.push(error(
            "placements",
            "Placement membership differs from the wiring facility inventory",
        ));
    }
    if !diagnostics.is_empty() {
        return FixedFactoryReport {
            schema_version: 1,
            success: false,
            factory_validated: false,
            padding,
            canvas,
            attempts,
            witness: None,
            acceptance: None,
            diagnostics,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        };
    }
    let placements = input
        .placements
        .iter()
        .map(|placement| FacilityPlacement {
            x: placement.x + padding,
            y: placement.y + padding,
            ..placement.clone()
        })
        .collect::<Vec<_>>();
    let projection = project_facility_ports(
        &FacilityPlacementReport {
            success: true,
            status: FacilityPlacementStatus::Feasible,
            bounds: Some(canvas.clone()),
            placements: placements.clone(),
            diagnostics: Vec::new(),
        },
        &catalogs.facilities,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: canvas.width,
            max_height: canvas.height,
        },
    );
    if !projection.success {
        diagnostics.push(error("placements", "Facility port projection failed"));
    }
    let mut boundaries = Vec::new();
    let mut internal = Vec::new();
    let mut external = Vec::new();
    for edge in &input.wiring.edges {
        let source_inside = members.contains(&edge.source);
        let target_inside = members.contains(&edge.target);
        if source_inside && target_inside {
            internal.push(edge);
        } else if source_inside || target_inside {
            external.push(edge);
        } else {
            diagnostics.push(error(&edge.id, "Wiring edge has no facility endpoint"));
            continue;
        }
        let Some(item) = catalogs.items.item(&edge.item) else {
            diagnostics.push(error(&edge.item, "Unknown wiring item"));
            continue;
        };
        for (inside, direction) in [
            (&edge.source, FacilityPortDirection::Output),
            (&edge.target, FacilityPortDirection::Input),
        ] {
            if !members.contains(inside) {
                continue;
            }
            let attachments = projection
                .ports
                .iter()
                .filter(|port| {
                    port.instance == *inside
                        && port.direction == direction
                        && port.transport == item.transport
                })
                .cloned()
                .map(|port| Attachment::NewPort { port })
                .collect::<Vec<_>>();
            if attachments.is_empty() {
                diagnostics.push(error(&edge.id, "Facility endpoint has no compatible port"));
            }
            boundaries.push(ConstructiveProcessModuleBoundary {
                requirement: edge.id.clone(),
                item: edge.item.clone(),
                transport: item.transport,
                rate: edge.rate,
                direction,
                inside_instance: inside.clone(),
                attachments,
            });
        }
    }
    if !diagnostics.is_empty() {
        return FixedFactoryReport {
            schema_version: 1,
            success: false,
            factory_validated: false,
            padding,
            canvas,
            attempts,
            witness: None,
            acceptance: None,
            diagnostics,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        };
    }
    let base = ConstructiveNode {
        id: "fixed-recipe-factory".into(),
        bounds: canvas.clone(),
        member_instances: members.into_iter().collect(),
        placements,
        internal_requirements: Vec::new(),
        external_requirements: Vec::new(),
        transport_networks: Vec::new(),
        logistics_components: Vec::new(),
        network_capacity: Vec::new(),
        attachment_events: Vec::new(),
        boundary_requirements: boundaries.clone(),
    };
    if let Err(diagnostic) = network::validate(
        &base,
        &catalogs.facilities,
        &catalogs.transports,
        &catalogs.components,
    ) {
        diagnostics.push(diagnostic);
    }
    let initial = refreshed(&base, &boundaries, catalogs);
    if let Some(boundary) = initial
        .iter()
        .find(|boundary| boundary.attachments.is_empty())
    {
        diagnostics.push(error(
            &boundary.requirement,
            "Placement blocks every compatible port escape for a required endpoint",
        ));
    }
    if !diagnostics.is_empty() {
        return FixedFactoryReport {
            schema_version: 1,
            success: false,
            factory_validated: false,
            padding,
            canvas,
            attempts,
            witness: None,
            acceptance: None,
            diagnostics,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        };
    }

    let orders = [
        (3, "seeded"),
        (4, "commodity-neighbor-first"),
        (0, "constrained-first"),
        (2, "pipe-first"),
    ];
    let mut last_validation = None;
    for (mode, name) in orders {
        let attempt_started = Instant::now();
        let mut stats = ConstructiveCompositionStatistics::default();
        let ordered_internal = order_edges(&internal, &initial, mode, input.routing_seed);
        let routed_internal = match route_internal(
            base.clone(),
            &ordered_internal,
            &boundaries,
            catalogs,
            route_limit,
            &mut stats,
        ) {
            Ok(node) => node,
            Err(requirement) => {
                let routed = usize::try_from(stats.completed_lane_bundles).unwrap_or(usize::MAX);
                attempts.push(FixedFactoryAttempt {
                    order: name,
                    routed_internal: routed,
                    routed_external: 0,
                    astar_searches: stats.astar_searches,
                    status: "internal-routing-exhausted",
                    failed_requirement: Some(requirement),
                    elapsed_ms: attempt_started.elapsed().as_secs_f64() * 1000.0,
                });
                continue;
            }
        };
        let internal_count = routed_internal.internal_requirements.len();
        let live = refreshed(&routed_internal, &boundaries, catalogs);
        let ordered_external = order_edges(&external, &live, mode, input.routing_seed ^ 1);
        let routed = match route_external(
            routed_internal,
            &ordered_external,
            &boundaries,
            catalogs,
            route_limit,
            &mut stats,
        ) {
            Ok(node) => node,
            Err(requirement) => {
                attempts.push(FixedFactoryAttempt {
                    order: name,
                    routed_internal: internal_count,
                    routed_external: usize::try_from(stats.completed_lane_bundles)
                        .unwrap_or(usize::MAX)
                        .saturating_sub(internal_count),
                    astar_searches: stats.astar_searches,
                    status: "external-routing-exhausted",
                    failed_requirement: Some(requirement),
                    elapsed_ms: attempt_started.elapsed().as_secs_f64() * 1000.0,
                });
                continue;
            }
        };
        let witness = WitnessProbeInput {
            schema_version: 1,
            placements: routed.placements.clone(),
            utilizations: request
                .instances
                .iter()
                .map(|instance| (instance.instance.clone(), instance.utilization))
                .collect::<BTreeMap<_, _>>(),
            transport_networks: routed.transport_networks.clone(),
            logistics_components: routed.logistics_components.clone(),
        };
        let acceptance = validate_factory_layout(request, &witness, catalogs);
        attempts.push(FixedFactoryAttempt {
            order: name,
            routed_internal: routed.internal_requirements.len(),
            routed_external: routed.external_requirements.len(),
            astar_searches: stats.astar_searches,
            status: if acceptance.factory_validated {
                "complete"
            } else {
                "factory-validation-failed"
            },
            failed_requirement: None,
            elapsed_ms: attempt_started.elapsed().as_secs_f64() * 1000.0,
        });
        if acceptance.factory_validated {
            return FixedFactoryReport {
                schema_version: 1,
                success: true,
                factory_validated: true,
                padding,
                canvas,
                attempts,
                witness: Some(witness),
                acceptance: Some(acceptance),
                diagnostics,
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
            };
        }
        last_validation = Some((witness, acceptance));
    }
    diagnostics.push(error(
        "routing",
        "All bounded legal routing orders were exhausted",
    ));
    let (witness, acceptance) = last_validation
        .map(|(witness, acceptance)| (Some(witness), Some(acceptance)))
        .unwrap_or((None, None));
    FixedFactoryReport {
        schema_version: 1,
        success: false,
        factory_validated: false,
        padding,
        canvas,
        attempts,
        witness,
        acceptance,
        diagnostics,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
}
