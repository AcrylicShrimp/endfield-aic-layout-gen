use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::time::Instant;

use crate::facilities::FacilityPortEdge;
use crate::layouts::FacilityPlacement;
use crate::logistics::{LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog};

use super::{
    EndpointInput, EndpointPortSelection, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    IntegratedLayoutStatus, IntegratedRoute, IntegratedRouteEndpoint, ModelInput,
    PlacedLogisticsComponent, RetainedRoutingResult, RetainedRoutingState, RoutingConflict,
    RoutingOrderPolicy, candidate_port_connections, grid_index, world_position,
};

const PLACEMENT_GAPS: [i32; 5] = [12, 8, 4, 2, 1];
const ACTIVE_ROUTING_MARGIN: i32 = 10;

struct SparsePlacement {
    placement: FacilityPlacement,
    port_connections: BTreeMap<String, usize>,
}

#[derive(Clone)]
struct FixedEndpoint {
    endpoint: IntegratedRouteEndpoint,
    cell: usize,
    external_side: FacilityPortEdge,
}

#[derive(Clone)]
struct AssignedRoute {
    edge_index: usize,
    source: FixedEndpoint,
    target: FixedEndpoint,
}

pub(super) fn construct(
    input: ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
) -> IntegratedLayoutReport {
    construct_with_deadline(input, components, None)
}

fn construct_with_deadline(
    input: ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    deadline: Option<Instant>,
) -> IntegratedLayoutReport {
    let mut port_failure = None;
    let mut best_routing_failure = None;
    'search: for gap in PLACEMENT_GAPS {
        let Some(placements) = place_on_shelves(&input, gap) else {
            continue;
        };
        for routing_height in active_routing_heights(&input, &placements) {
            for order in route_orders(&input, None) {
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    break 'search;
                }
                let assigned = match assign_facility_ports(&input, &placements, &order) {
                    Ok(assigned) => assigned,
                    Err(failure) => {
                        port_failure = Some(failure);
                        continue;
                    }
                };
                match route_all(&input, &placements, &assigned, routing_height, deadline) {
                    Ok((routes, bridges)) => {
                        let report = success_report(
                            &input,
                            components,
                            placements,
                            routes,
                            bridges,
                            "sparse-integrated-layout-feasible",
                            "sparse construction produced a feasible placement and routing witness; optimality is not proven",
                        );
                        return validate_success_report(&input, components, report);
                    }
                    Err(failure) => {
                        if best_routing_failure
                            .as_ref()
                            .is_none_or(|best: &RoutingFailure| failure.routed > best.routed)
                        {
                            best_routing_failure = Some(failure);
                        }
                    }
                }
            }
        }
    }

    let diagnostic = if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        IntegratedLayoutDiagnostic::error(
            "sparse-construction-time-limit",
            "/routes",
            None,
            format!(
                "sparse construction exhausted its absolute deadline after routing at most {} of {} capacity-split routes; this is not proof of infeasibility",
                best_routing_failure
                    .as_ref()
                    .map_or(0, |failure: &RoutingFailure| failure.routed),
                input.edges.len(),
            ),
        )
    } else if let Some(failure) = best_routing_failure {
        let edge = &input.edges[failure.edge_index].edge;
        IntegratedLayoutDiagnostic::error(
            "sparse-routing-construction-failed",
            format!("/edges/{}", failure.edge_index),
            Some(edge.item.clone()),
            format!(
                "sparse routing constructed {} of {} capacity-split routes before failing from '{}' to '{}'; this is not proof of infeasibility",
                failure.routed,
                input.edges.len(),
                edge.source,
                edge.target
            ),
        )
    } else if let Some(failure) = port_failure {
        IntegratedLayoutDiagnostic::error(
            "sparse-port-assignment-failed",
            format!("/edges/{}", failure.edge_index),
            Some(failure.instance.clone()),
            format!(
                "facility instance '{}' has no unused compatible connection cell for the {} endpoint of capacity-split route {}; this is not proof of infeasibility",
                failure.instance, failure.endpoint_kind, failure.edge_index
            ),
        )
    } else {
        IntegratedLayoutDiagnostic::error(
            "sparse-placement-construction-failed",
            "/",
            None,
            "sparse shelf placement did not fit within the hard layout bounds; this is not proof of infeasibility",
        )
    };
    IntegratedLayoutReport::failure(IntegratedLayoutStatus::Unknown, diagnostic)
}

pub(super) fn construct_from_placements(
    input: ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    placements: Vec<FacilityPlacement>,
    deadline: Instant,
) -> IntegratedLayoutReport {
    construct_from_placements_with_policy(input, components, placements, None, deadline)
}

pub(super) fn construct_from_placements_with_policy(
    input: ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    placements: Vec<FacilityPlacement>,
    routing_order_policy: Option<RoutingOrderPolicy>,
    deadline: Instant,
) -> IntegratedLayoutReport {
    let placements = match index_placements(&input, placements) {
        Ok(placements) => placements,
        Err(diagnostic) => {
            return IntegratedLayoutReport::failure(IntegratedLayoutStatus::Unknown, diagnostic);
        }
    };
    let mut port_failure = None;
    let mut best_routing_failure = None;

    'search: for routing_height in active_routing_heights(&input, &placements) {
        for order in route_orders(&input, routing_order_policy) {
            if Instant::now() >= deadline {
                break 'search;
            }
            let assigned = match assign_facility_ports(&input, &placements, &order) {
                Ok(assigned) => assigned,
                Err(failure) => {
                    port_failure = Some(failure);
                    continue;
                }
            };
            match route_all(
                &input,
                &placements,
                &assigned,
                routing_height,
                Some(deadline),
            ) {
                Ok((routes, bridges)) => {
                    let report = success_report(
                        &input,
                        components,
                        placements,
                        routes,
                        bridges,
                        "coordinate-integrated-layout-feasible",
                        "coordinate CP placement and sparse routing produced a validated feasible witness; optimality is not proven",
                    );
                    return validate_success_report(&input, components, report);
                }
                Err(failure) => {
                    if best_routing_failure
                        .as_ref()
                        .is_none_or(|best: &RoutingFailure| failure.routed > best.routed)
                    {
                        best_routing_failure = Some(failure);
                    }
                    if Instant::now() >= deadline {
                        break 'search;
                    }
                }
            }
        }
    }

    let diagnostic = if Instant::now() >= deadline {
        IntegratedLayoutDiagnostic::error(
            "coordinate-routing-time-limit",
            "/routes",
            None,
            format!(
                "coordinate routing reached its worker deadline after constructing at most {} of {} capacity-split routes; this is not proof of infeasibility",
                best_routing_failure
                    .as_ref()
                    .map(|failure| failure.routed)
                    .unwrap_or(0),
                input.edges.len(),
            ),
        )
    } else if let Some(failure) = best_routing_failure {
        let edge = &input.edges[failure.edge_index].edge;
        IntegratedLayoutDiagnostic::error(
            "coordinate-routing-construction-failed",
            format!("/edges/{}", failure.edge_index),
            Some(edge.item.clone()),
            format!(
                "coordinate placement routed {} of {} capacity-split routes before failing from '{}' to '{}'; this is not proof of infeasibility",
                failure.routed,
                input.edges.len(),
                edge.source,
                edge.target
            ),
        )
    } else if let Some(failure) = port_failure {
        IntegratedLayoutDiagnostic::error(
            "coordinate-port-assignment-failed",
            format!("/edges/{}", failure.edge_index),
            Some(failure.instance.clone()),
            format!(
                "facility instance '{}' has no unused compatible connection cell for the {} endpoint of capacity-split route {}; this is not proof of infeasibility",
                failure.instance, failure.endpoint_kind, failure.edge_index
            ),
        )
    } else {
        IntegratedLayoutDiagnostic::error(
            "coordinate-placement-projection-failed",
            "/",
            None,
            "coordinate placement could not be projected into the routing grid",
        )
    };
    IntegratedLayoutReport::failure(IntegratedLayoutStatus::Unknown, diagnostic)
}

pub(super) fn construct_from_retained(
    input: ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    placements: Vec<FacilityPlacement>,
    retained: &RetainedRoutingState,
    explicit_invalidated_requirement_ids: &BTreeSet<String>,
    deadline: Instant,
) -> RetainedRoutingResult {
    construct_from_retained_with_policy(
        input,
        components,
        placements,
        retained,
        explicit_invalidated_requirement_ids,
        None,
        deadline,
    )
}

pub(super) fn construct_from_retained_with_policy(
    input: ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    placements: Vec<FacilityPlacement>,
    retained: &RetainedRoutingState,
    explicit_invalidated_requirement_ids: &BTreeSet<String>,
    routing_order_policy: Option<RoutingOrderPolicy>,
    deadline: Instant,
) -> RetainedRoutingResult {
    if let Some(unknown) = explicit_invalidated_requirement_ids
        .iter()
        .find(|requirement_id| {
            !input
                .edges
                .iter()
                .any(|edge| edge.requirement_id == requirement_id.as_str())
        })
    {
        return retained_failure(
            IntegratedLayoutDiagnostic::error(
                "unknown-retained-route-requirement",
                "/invalidated_requirement_ids",
                Some(unknown.clone()),
                format!(
                    "invalidated route requirement '{unknown}' is absent from the current graph"
                ),
            ),
            BTreeSet::new(),
            BTreeSet::new(),
            None,
        );
    }
    let placements = match index_placements(&input, placements) {
        Ok(placements) => placements,
        Err(diagnostic) => {
            return retained_failure(diagnostic, BTreeSet::new(), BTreeSet::new(), None);
        }
    };
    let invalidated = invalidation_closure(
        &input,
        &placements,
        retained,
        explicit_invalidated_requirement_ids,
    );
    let current_index_by_id = input
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| (edge.requirement_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let reused = retained
        .retained_routes
        .keys()
        .filter(|requirement_id| {
            current_index_by_id.contains_key(requirement_id.as_str())
                && !invalidated.contains(requirement_id.as_str())
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let route_order = preferred_route_order(&input, routing_order_policy)
        .into_iter()
        .filter(|index| invalidated.contains(&input.edges[*index].requirement_id))
        .collect::<Vec<_>>();
    let assigned = match assign_subset_ports(&input, &placements, &route_order, retained, &reused) {
        Ok(assigned) => assigned,
        Err(failure) => {
            let requirement_id = input.edges[failure.edge_index].requirement_id.clone();
            let conflict = RoutingConflict {
                code: "retained-subset-port-assignment-failed".to_string(),
                failed_requirement_ids: vec![requirement_id.clone()],
                related_facility_ids: vec![failure.instance.clone()],
                related_scc_ids: Vec::new(),
                blocked_cells: Vec::new(),
                blocking_requirement_ids: reused.iter().cloned().collect(),
                blocking_component_ids: Vec::new(),
                message: format!(
                    "facility '{}' has no available compatible port for retained subset route '{}'",
                    failure.instance, requirement_id
                ),
            };
            return retained_failure(
                IntegratedLayoutDiagnostic::error(
                    "retained-subset-port-assignment-failed",
                    format!("/routes/{requirement_id}"),
                    Some(requirement_id),
                    conflict.message.clone(),
                ),
                invalidated,
                reused,
                Some(conflict),
            );
        }
    };

    let mut best_failure = None;
    for routing_height in active_routing_heights(&input, &placements) {
        if Instant::now() >= deadline {
            break;
        }
        match route_subset(
            &input,
            &placements,
            &assigned,
            retained,
            &reused,
            routing_height,
            deadline,
        ) {
            Ok((routes, bridges)) => {
                let report = success_report(
                    &input,
                    components,
                    placements,
                    routes,
                    bridges,
                    "retained-subset-routing-feasible",
                    "retained routes were preserved while the invalidated subset was rerouted and the complete witness was validated",
                );
                let report = validate_success_report(&input, components, report);
                if report.success {
                    return RetainedRoutingResult {
                        report,
                        invalidated_requirement_ids: invalidated.into_iter().collect(),
                        reused_requirement_ids: reused.into_iter().collect(),
                        conflict: None,
                    };
                }
                return retained_failure(
                    report.diagnostics.first().cloned().unwrap_or_else(|| {
                        IntegratedLayoutDiagnostic::error(
                            "retained-subset-validation-failed",
                            "/",
                            None,
                            "retained subset witness validation failed without a diagnostic",
                        )
                    }),
                    invalidated,
                    reused,
                    None,
                );
            }
            Err(failure) => best_failure = Some(failure),
        }
    }

    let failure = best_failure;
    let failed_requirement_id = failure
        .as_ref()
        .map(|failure| input.edges[failure.edge_index].requirement_id.clone());
    let blocking_requirement_ids = reused
        .iter()
        .filter(|requirement_id| {
            failed_requirement_id.as_ref().is_none_or(|failed| {
                let failed_transport = input.edges[current_index_by_id[failed.as_str()]].transport;
                retained.retained_routes[*requirement_id].transport == failed_transport
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let blocking_component_ids = retained
        .retained_components
        .values()
        .filter(|component| {
            component
                .owner_requirement_ids
                .iter()
                .any(|owner| reused.contains(owner))
        })
        .map(|component| component.id.clone())
        .collect::<Vec<_>>();
    let related_facility_ids = failure
        .as_ref()
        .map(|failure| edge_facility_ids(&input.edges[failure.edge_index]))
        .unwrap_or_default();
    let failed_requirement_ids = failed_requirement_id.into_iter().collect::<Vec<_>>();
    let conflict = RoutingConflict {
        code: "retained-subset-routing-failed".to_string(),
        failed_requirement_ids: failed_requirement_ids.clone(),
        related_facility_ids,
        related_scc_ids: Vec::new(),
        blocked_cells: Vec::new(),
        blocking_requirement_ids,
        blocking_component_ids,
        message: "retained subset routing exhausted the available route search without a complete witness; this is not proof of infeasibility".to_string(),
    };
    retained_failure(
        IntegratedLayoutDiagnostic::error(
            if Instant::now() >= deadline {
                "retained-subset-routing-time-limit"
            } else {
                "retained-subset-routing-failed"
            },
            "/routes",
            failed_requirement_ids.first().cloned(),
            conflict.message.clone(),
        ),
        invalidated,
        reused,
        Some(conflict),
    )
}

fn index_placements(
    input: &ModelInput,
    placements: Vec<FacilityPlacement>,
) -> Result<BTreeMap<String, SparsePlacement>, IntegratedLayoutDiagnostic> {
    let mut indexed = BTreeMap::new();
    for placement in placements {
        let Some(instance) = input
            .instances
            .iter()
            .find(|instance| instance.id == placement.instance)
        else {
            return Err(IntegratedLayoutDiagnostic::error(
                "coordinate-placement-instance-mismatch",
                "/placements",
                Some(placement.instance),
                "coordinate placement contains an instance absent from integrated input",
            ));
        };
        let x = i32::try_from(placement.x).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "coordinate-placement-out-of-range",
                "/placements",
                Some(placement.instance.clone()),
                "coordinate placement x does not fit the routing grid domain",
            )
        })?;
        let y = i32::try_from(placement.y).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "coordinate-placement-out-of-range",
                "/placements",
                Some(placement.instance.clone()),
                "coordinate placement y does not fit the routing grid domain",
            )
        })?;
        let port_connections = candidate_port_connections(
            &instance.definition,
            placement.rotation,
            x,
            y,
            input.width,
            input.height,
        );
        indexed.insert(
            placement.instance.clone(),
            SparsePlacement {
                placement,
                port_connections,
            },
        );
    }
    Ok(indexed)
}

struct PortAssignmentFailure {
    edge_index: usize,
    endpoint_kind: &'static str,
    instance: String,
}

struct RoutingFailure {
    routed: usize,
    edge_index: usize,
}

type RoutedWitness = (
    Vec<(usize, IntegratedRoute)>,
    BTreeSet<(TransportKind, usize)>,
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum RouteCellShape {
    Horizontal,
    Vertical,
    Blocked,
    Crossed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepDirection {
    North,
    East,
    South,
    West,
}

impl StepDirection {
    fn index(self) -> usize {
        match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }

    fn orientation(self) -> RouteCellShape {
        match self {
            Self::North | Self::South => RouteCellShape::Vertical,
            Self::East | Self::West => RouteCellShape::Horizontal,
        }
    }
}

fn place_on_shelves(input: &ModelInput, gap: i32) -> Option<BTreeMap<String, SparsePlacement>> {
    let margin = 1;
    let mut x = margin;
    let mut y = margin;
    let mut row_height = 0;
    let mut placements = BTreeMap::new();

    for instance in &input.instances {
        let rotation = *instance.definition.allowed_rotations.first()?;
        let source_width = i32::try_from(instance.definition.footprint.width).ok()?;
        let source_height = i32::try_from(instance.definition.footprint.height).ok()?;
        let (width, height) = if matches!(rotation, 90 | 270) {
            (source_height, source_width)
        } else {
            (source_width, source_height)
        };

        if x + width + margin > input.width {
            x = margin;
            y += row_height + gap;
            row_height = 0;
        }
        if y + height + margin > input.height {
            return None;
        }
        let port_connections = candidate_port_connections(
            &instance.definition,
            rotation,
            x,
            y,
            input.width,
            input.height,
        );
        placements.insert(
            instance.id.clone(),
            SparsePlacement {
                placement: FacilityPlacement {
                    instance: instance.id.clone(),
                    recipe: instance.recipe.clone(),
                    facility: instance.facility.clone(),
                    x: i64::from(x),
                    y: i64::from(y),
                    width: i64::from(width),
                    height: i64::from(height),
                    rotation,
                },
                port_connections,
            },
        );
        x += width + gap;
        row_height = row_height.max(height);
    }

    Some(placements)
}

fn route_orders(
    input: &ModelInput,
    preferred_policy: Option<RoutingOrderPolicy>,
) -> Vec<Vec<usize>> {
    let original = (0..input.edges.len()).collect::<Vec<_>>();
    let mut reversed = original.clone();
    reversed.reverse();
    let network_first = input
        .networks
        .iter()
        .flat_map(|network| network.route_indices().iter().copied())
        .collect::<Vec<_>>();
    let mut terminal_first_networks = input.networks.iter().collect::<Vec<_>>();
    terminal_first_networks.sort_by_key(|network| {
        (
            std::cmp::Reverse(network.external_terminal_count()),
            std::cmp::Reverse(network.terminal_count()),
            network.id(),
        )
    });
    let terminal_first = terminal_first_networks
        .into_iter()
        .flat_map(|network| network.route_indices().iter().copied())
        .collect::<Vec<_>>();
    let mut facility_first = original.clone();
    facility_first.sort_by_key(|index| {
        let edge = &input.edges[*index];
        let boundaries = usize::from(matches!(edge.source, EndpointInput::External { .. }))
            + usize::from(matches!(edge.target, EndpointInput::External { .. }));
        (boundaries, *index)
    });
    let mut external_first = facility_first.clone();
    external_first.reverse();
    let mut orders = vec![
        original,
        reversed,
        network_first,
        terminal_first,
        facility_first,
        external_first,
    ];
    if let Some(preferred_policy) = preferred_policy {
        let preferred = preferred_route_order(input, Some(preferred_policy));
        orders.retain(|order| order != &preferred);
        orders.insert(0, preferred);
    }
    for seed in 1_u64..=96 {
        let mut shuffled = (0..input.edges.len()).collect::<Vec<_>>();
        shuffled.sort_by_key(|index| deterministic_order_key(*index as u64, seed));
        orders.push(shuffled);
    }
    orders
}

fn preferred_route_order(input: &ModelInput, policy: Option<RoutingOrderPolicy>) -> Vec<usize> {
    let mut order = (0..input.edges.len()).collect::<Vec<_>>();
    match policy {
        Some(RoutingOrderPolicy::FacilityFirst) => order.sort_by_key(|index| {
            let edge = &input.edges[*index];
            let boundaries = usize::from(matches!(edge.source, EndpointInput::External { .. }))
                + usize::from(matches!(edge.target, EndpointInput::External { .. }));
            (boundaries, *index)
        }),
        Some(RoutingOrderPolicy::ExternalFirst) => order.sort_by_key(|index| {
            let edge = &input.edges[*index];
            let boundaries = usize::from(matches!(edge.source, EndpointInput::External { .. }))
                + usize::from(matches!(edge.target, EndpointInput::External { .. }));
            (std::cmp::Reverse(boundaries), *index)
        }),
        Some(RoutingOrderPolicy::NetworkFirst) => {
            order = input
                .networks
                .iter()
                .flat_map(|network| network.route_indices().iter().copied())
                .collect();
        }
        None => {}
    }
    order
}

fn deterministic_order_key(index: u64, seed: u64) -> u64 {
    let mut value = index ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn assign_facility_ports(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
    order: &[usize],
) -> Result<Vec<AssignedRoute>, PortAssignmentFailure> {
    let mut reserved = [BTreeSet::new(), BTreeSet::new()];
    let mut assigned = Vec::with_capacity(input.edges.len());

    for edge_index in order {
        let edge = &input.edges[*edge_index];
        let layer = layer_index(edge.transport);
        let (source, target) = match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => {
                let source = assign_facility_endpoint(
                    *edge_index,
                    "source",
                    &edge.source,
                    placements,
                    &mut reserved[layer],
                    None,
                    None,
                )?;
                let target = assign_facility_endpoint(
                    *edge_index,
                    "target",
                    &edge.target,
                    placements,
                    &mut reserved[layer],
                    Some(source.cell),
                    None,
                )?;
                (source, target)
            }
            (EndpointInput::External { node }, EndpointInput::Facility { .. }) => {
                let target = assign_facility_endpoint(
                    *edge_index,
                    "target",
                    &edge.target,
                    placements,
                    &mut reserved[layer],
                    None,
                    None,
                )?;
                (external_endpoint(node, &target), target)
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { node }) => {
                let source = assign_facility_endpoint(
                    *edge_index,
                    "source",
                    &edge.source,
                    placements,
                    &mut reserved[layer],
                    None,
                    None,
                )?;
                let target = external_endpoint(node, &source);
                (source, target)
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => unreachable!(
                "external-to-external requirements are rejected during model preparation"
            ),
        };
        assigned.push(AssignedRoute {
            edge_index: *edge_index,
            source,
            target,
        });
    }

    Ok(assigned)
}

fn assign_subset_ports(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
    order: &[usize],
    retained: &RetainedRoutingState,
    reused: &BTreeSet<String>,
) -> Result<Vec<AssignedRoute>, PortAssignmentFailure> {
    let mut reserved = [BTreeSet::new(), BTreeSet::new()];
    for requirement_id in reused {
        let route = &retained.retained_routes[requirement_id];
        let layer = layer_index(route.transport);
        let assignment = &retained.selected_ports[requirement_id];
        for selection in [&assignment.source, &assignment.target] {
            if let Some(cell) = selection_connection_cell(selection, placements) {
                reserved[layer].insert(cell);
            }
        }
    }

    let mut assigned = Vec::with_capacity(order.len());
    for edge_index in order {
        let edge = &input.edges[*edge_index];
        let layer = layer_index(edge.transport);
        let preferred = retained.selected_ports.get(&edge.requirement_id);
        let (source, target) = match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => {
                let source = assign_facility_endpoint(
                    *edge_index,
                    "source",
                    &edge.source,
                    placements,
                    &mut reserved[layer],
                    None,
                    preferred.map(|assignment| selection_port_id(&assignment.source)),
                )?;
                let target = assign_facility_endpoint(
                    *edge_index,
                    "target",
                    &edge.target,
                    placements,
                    &mut reserved[layer],
                    Some(source.cell),
                    preferred.map(|assignment| selection_port_id(&assignment.target)),
                )?;
                (source, target)
            }
            (EndpointInput::External { node }, EndpointInput::Facility { .. }) => {
                let target = assign_facility_endpoint(
                    *edge_index,
                    "target",
                    &edge.target,
                    placements,
                    &mut reserved[layer],
                    None,
                    preferred.map(|assignment| selection_port_id(&assignment.target)),
                )?;
                (external_endpoint(node, &target), target)
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { node }) => {
                let source = assign_facility_endpoint(
                    *edge_index,
                    "source",
                    &edge.source,
                    placements,
                    &mut reserved[layer],
                    None,
                    preferred.map(|assignment| selection_port_id(&assignment.source)),
                )?;
                let target = external_endpoint(node, &source);
                (source, target)
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => unreachable!(
                "external-to-external requirements are rejected during model preparation"
            ),
        };
        assigned.push(AssignedRoute {
            edge_index: *edge_index,
            source,
            target,
        });
    }
    Ok(assigned)
}

fn selection_connection_cell(
    selection: &EndpointPortSelection,
    placements: &BTreeMap<String, SparsePlacement>,
) -> Option<usize> {
    let (instance, port) = selection_identity(selection);
    placements
        .get(instance)?
        .port_connections
        .get(port)
        .copied()
}

fn selection_port_id(selection: &EndpointPortSelection) -> &str {
    selection_identity(selection).1
}

fn selection_identity(selection: &EndpointPortSelection) -> (&str, &str) {
    match selection {
        EndpointPortSelection::FacilityPort {
            facility_instance_id,
            port_id,
        }
        | EndpointPortSelection::ExternalDangling {
            facility_instance_id,
            port_id,
            ..
        } => (facility_instance_id, port_id),
    }
}

fn invalidation_closure(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
    retained: &RetainedRoutingState,
    explicit: &BTreeSet<String>,
) -> BTreeSet<String> {
    let current_by_id = input
        .edges
        .iter()
        .map(|edge| (edge.requirement_id.as_str(), edge))
        .collect::<BTreeMap<_, _>>();
    let mut invalidated = explicit.clone();
    for edge in &input.edges {
        match retained.retained_routes.get(&edge.requirement_id) {
            Some(route) if route.requirement_fingerprint == edge.requirement_fingerprint => {}
            _ => {
                invalidated.insert(edge.requirement_id.clone());
            }
        }
    }
    let moved_facilities = placements
        .iter()
        .filter(|(instance, placement)| {
            let facility_definition_changed = retained
                .graph_key
                .facilities
                .iter()
                .find(|record| record.facility_instance_id == instance.as_str())
                .is_none_or(|record| record.flattened_facility_id != placement.placement.facility);
            retained
                .retained_placements
                .get(instance.as_str())
                .is_none_or(|prior| {
                    facility_definition_changed
                        || prior.x != placement.placement.x
                        || prior.y != placement.placement.y
                        || prior.rotation != placement.placement.rotation
                })
        })
        .map(|(instance, _)| instance.as_str())
        .collect::<BTreeSet<_>>();
    for edge in &input.edges {
        if endpoint_in_facilities(&edge.source, &moved_facilities)
            || endpoint_in_facilities(&edge.target, &moved_facilities)
        {
            invalidated.insert(edge.requirement_id.clone());
        }
    }

    for (requirement_id, route) in &retained.retained_routes {
        let Some(edge) = current_by_id.get(requirement_id.as_str()) else {
            continue;
        };
        if route.requirement_fingerprint != edge.requirement_fingerprint
            || retained
                .selected_ports
                .get(requirement_id)
                .is_none_or(|assignment| {
                    [&assignment.source, &assignment.target]
                        .into_iter()
                        .any(|selection| selection_connection_cell(selection, placements).is_none())
                })
        {
            invalidated.insert(requirement_id.clone());
        }
    }

    loop {
        let before = invalidated.len();
        for component in retained.retained_components.values() {
            if component
                .owner_requirement_ids
                .iter()
                .any(|owner| invalidated.contains(owner))
            {
                invalidated.extend(
                    component
                        .owner_requirement_ids
                        .iter()
                        .filter(|owner| current_by_id.contains_key(owner.as_str()))
                        .cloned(),
                );
            }
        }
        if invalidated.len() == before {
            break;
        }
    }
    invalidated
}

fn endpoint_in_facilities(endpoint: &EndpointInput, facilities: &BTreeSet<&str>) -> bool {
    matches!(endpoint, EndpointInput::Facility { instance, .. } if facilities.contains(instance.as_str()))
}

fn assign_facility_endpoint(
    edge_index: usize,
    endpoint_kind: &'static str,
    endpoint: &EndpointInput,
    placements: &BTreeMap<String, SparsePlacement>,
    reserved: &mut BTreeSet<usize>,
    allowed_reserved: Option<usize>,
    preferred_port_id: Option<&str>,
) -> Result<FixedEndpoint, PortAssignmentFailure> {
    match endpoint {
        EndpointInput::Facility { instance, ports } => {
            let placement = placements
                .get(instance)
                .expect("prepared facility endpoint has a sparse placement");
            let preferred = preferred_port_id.and_then(|preferred| {
                ports
                    .iter()
                    .filter(|port| port.id == preferred)
                    .find_map(|port| {
                        placement
                            .port_connections
                            .get(&port.id)
                            .map(|cell| (port, *cell))
                            .filter(|(_, cell)| {
                                !reserved.contains(cell) || allowed_reserved == Some(*cell)
                            })
                    })
            });
            let fallback = || {
                ports.iter().find_map(|port| {
                    placement
                        .port_connections
                        .get(&port.id)
                        .map(|cell| (port, *cell))
                        .filter(|(_, cell)| {
                            !reserved.contains(cell) || allowed_reserved == Some(*cell)
                        })
                })
            };
            let (port, cell) =
                preferred
                    .or_else(fallback)
                    .ok_or_else(|| PortAssignmentFailure {
                        edge_index,
                        endpoint_kind,
                        instance: instance.clone(),
                    })?;
            reserved.insert(cell);
            Ok(FixedEndpoint {
                endpoint: IntegratedRouteEndpoint::Facility {
                    instance: instance.clone(),
                    port: port.id.clone(),
                },
                cell,
                external_side: port.edge.rotated_clockwise(placement.placement.rotation),
            })
        }
        EndpointInput::External { .. } => unreachable!("expected a facility endpoint"),
    }
}

fn external_endpoint(node: &str, facility: &FixedEndpoint) -> FixedEndpoint {
    FixedEndpoint {
        endpoint: IntegratedRouteEndpoint::External {
            node: node.to_string(),
            side: facility.external_side,
        },
        cell: facility.cell,
        external_side: facility.external_side,
    }
}

fn route_all(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
    assigned: &[AssignedRoute],
    routing_height: i32,
    deadline: Option<Instant>,
) -> Result<RoutedWitness, RoutingFailure> {
    let cell_count = usize::try_from(input.width).expect("validated width is positive")
        * usize::try_from(routing_height).expect("routing height is positive");
    let facility_cells = facility_cells(input, placements, cell_count)
        .expect("sparse shelf placements are non-overlapping and in bounds");
    let reserved = reserved_cells(input, assigned);
    let mut used = [vec![None; cell_count], vec![None; cell_count]];
    let mut bridges = BTreeSet::new();
    let mut routes = Vec::with_capacity(assigned.len());

    for route in assigned {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(RoutingFailure {
                routed: routes.len(),
                edge_index: route.edge_index,
            });
        }
        let edge = &input.edges[route.edge_index];
        let layer = layer_index(edge.transport);
        let source_options = vec![route.source.clone()];
        let target_options = vec![route.target.clone()];
        let Some((source, target, cells)) = find_path(
            input.width,
            routing_height,
            &source_options,
            &target_options,
            &facility_cells,
            &used[layer],
            &reserved[layer],
        ) else {
            return Err(RoutingFailure {
                routed: routes.len(),
                edge_index: route.edge_index,
            });
        };
        for (path_index, cell) in cells.iter().enumerate() {
            let shape = route_cell_shape(&cells, path_index, input.width);
            match used[layer][*cell] {
                None => used[layer][*cell] = Some(shape),
                Some(existing) if crossing_allowed(existing, shape) => {
                    used[layer][*cell] = Some(RouteCellShape::Crossed);
                    bridges.insert((edge.transport, *cell));
                }
                Some(_) => unreachable!("path search only returns valid crossings"),
            }
        }
        routes.push((
            route.edge_index,
            IntegratedRoute {
                requirement_id: edge.requirement_id.clone(),
                requirement_fingerprint: edge.requirement_fingerprint.clone(),
                source: source.endpoint,
                target: target.endpoint,
                item: edge.edge.item.clone(),
                rate: edge.edge.rate,
                transport: edge.transport,
                cells: cells
                    .into_iter()
                    .map(|cell| world_position(cell, input.width))
                    .collect(),
            },
        ));
    }

    Ok((routes, bridges))
}

fn route_subset(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
    assigned: &[AssignedRoute],
    retained: &RetainedRoutingState,
    reused: &BTreeSet<String>,
    routing_height: i32,
    deadline: Instant,
) -> Result<RoutedWitness, RoutingFailure> {
    let cell_count = usize::try_from(input.width).expect("validated width is positive")
        * usize::try_from(routing_height).expect("routing height is positive");
    let facility_cells = facility_cells(input, placements, cell_count)
        .expect("retained placements are non-overlapping and in bounds");
    let reserved = reserved_cells_with_retained(input, assigned, retained, reused, placements);
    let current_index_by_id = input
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| (edge.requirement_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut used = [vec![None; cell_count], vec![None; cell_count]];
    let mut bridges = BTreeSet::new();
    let retained_component_cells = retained
        .retained_components
        .values()
        .filter(|component| {
            component
                .owner_requirement_ids
                .iter()
                .all(|owner| reused.contains(owner))
        })
        .map(|component| {
            (
                component.transport,
                grid_index(
                    component.cell.x as i32,
                    component.cell.y as i32,
                    input.width,
                ),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut routes = Vec::with_capacity(reused.len() + assigned.len());
    for requirement_id in reused {
        let edge_index = current_index_by_id[requirement_id.as_str()];
        let route = retained.retained_routes[requirement_id].clone();
        let layer = layer_index(route.transport);
        let cells = route
            .cells
            .iter()
            .map(|cell| {
                if cell.y >= i64::from(routing_height) {
                    return None;
                }
                Some(grid_index(cell.x as i32, cell.y as i32, input.width))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(RoutingFailure {
                routed: routes.len(),
                edge_index,
            })?;
        for (path_index, cell) in cells.iter().enumerate() {
            if facility_cells[*cell] {
                return Err(RoutingFailure {
                    routed: routes.len(),
                    edge_index,
                });
            }
            if retained_component_cells.contains(&(route.transport, *cell)) {
                used[layer][*cell] = Some(RouteCellShape::Crossed);
                bridges.insert((route.transport, *cell));
            } else if used[layer][*cell].is_none() {
                used[layer][*cell] = Some(route_cell_shape(&cells, path_index, input.width));
            } else {
                return Err(RoutingFailure {
                    routed: routes.len(),
                    edge_index,
                });
            }
        }
        routes.push((edge_index, route));
    }

    for route in assigned {
        if Instant::now() >= deadline {
            return Err(RoutingFailure {
                routed: routes.len(),
                edge_index: route.edge_index,
            });
        }
        let edge = &input.edges[route.edge_index];
        let layer = layer_index(edge.transport);
        let Some((source, target, cells)) = find_path(
            input.width,
            routing_height,
            std::slice::from_ref(&route.source),
            std::slice::from_ref(&route.target),
            &facility_cells,
            &used[layer],
            &reserved[layer],
        ) else {
            return Err(RoutingFailure {
                routed: routes.len(),
                edge_index: route.edge_index,
            });
        };
        for (path_index, cell) in cells.iter().enumerate() {
            let shape = route_cell_shape(&cells, path_index, input.width);
            match used[layer][*cell] {
                None => used[layer][*cell] = Some(shape),
                Some(existing) if crossing_allowed(existing, shape) => {
                    used[layer][*cell] = Some(RouteCellShape::Crossed);
                    bridges.insert((edge.transport, *cell));
                }
                Some(_) => unreachable!("path search only returns valid crossings"),
            }
        }
        routes.push((
            route.edge_index,
            IntegratedRoute {
                requirement_id: edge.requirement_id.clone(),
                requirement_fingerprint: edge.requirement_fingerprint.clone(),
                source: source.endpoint,
                target: target.endpoint,
                item: edge.edge.item.clone(),
                rate: edge.edge.rate,
                transport: edge.transport,
                cells: cells
                    .into_iter()
                    .map(|cell| world_position(cell, input.width))
                    .collect(),
            },
        ));
    }
    Ok((routes, bridges))
}

fn reserved_cells_with_retained(
    input: &ModelInput,
    assigned: &[AssignedRoute],
    retained: &RetainedRoutingState,
    reused: &BTreeSet<String>,
    placements: &BTreeMap<String, SparsePlacement>,
) -> [BTreeSet<usize>; 2] {
    let mut reserved = reserved_cells(input, assigned);
    for requirement_id in reused {
        let route = &retained.retained_routes[requirement_id];
        let layer = layer_index(route.transport);
        let assignment = &retained.selected_ports[requirement_id];
        for selection in [&assignment.source, &assignment.target] {
            if let Some(cell) = selection_connection_cell(selection, placements) {
                reserved[layer].insert(cell);
            }
        }
    }
    reserved
}

fn retained_failure(
    diagnostic: IntegratedLayoutDiagnostic,
    invalidated: BTreeSet<String>,
    reused: BTreeSet<String>,
    conflict: Option<RoutingConflict>,
) -> RetainedRoutingResult {
    RetainedRoutingResult {
        report: IntegratedLayoutReport::failure(IntegratedLayoutStatus::Unknown, diagnostic),
        invalidated_requirement_ids: invalidated.into_iter().collect(),
        reused_requirement_ids: reused.into_iter().collect(),
        conflict,
    }
}

fn edge_facility_ids(edge: &super::EdgeInput) -> Vec<String> {
    [&edge.source, &edge.target]
        .into_iter()
        .filter_map(|endpoint| match endpoint {
            EndpointInput::Facility { instance, .. } => Some(instance.clone()),
            EndpointInput::External { .. } => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn active_routing_heights(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
) -> Vec<i32> {
    let placement_height = placements
        .values()
        .filter_map(|placement| {
            i32::try_from(placement.placement.y + placement.placement.height).ok()
        })
        .max()
        .unwrap_or(1);
    let mut heights = [1, 2, 4, 8]
        .into_iter()
        .map(|multiplier| {
            placement_height
                .saturating_add(ACTIVE_ROUTING_MARGIN.saturating_mul(multiplier))
                .clamp(1, input.height)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if heights.last().copied() != Some(input.height) {
        heights.push(input.height);
    }
    heights
}

fn facility_cells(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
    cell_count: usize,
) -> Option<Vec<bool>> {
    let mut occupied = vec![false; cell_count];
    for placement in placements.values() {
        let placement = &placement.placement;
        for y in placement.y..(placement.y + placement.height) {
            for x in placement.x..(placement.x + placement.width) {
                let cell = grid_index(i32::try_from(x).ok()?, i32::try_from(y).ok()?, input.width);
                if occupied[cell] {
                    return None;
                }
                occupied[cell] = true;
            }
        }
    }
    Some(occupied)
}

fn reserved_cells(input: &ModelInput, assigned: &[AssignedRoute]) -> [BTreeSet<usize>; 2] {
    let mut reserved = [BTreeSet::new(), BTreeSet::new()];
    for route in assigned {
        let layer = layer_index(input.edges[route.edge_index].transport);
        for endpoint in [&route.source, &route.target] {
            reserved[layer].insert(endpoint.cell);
        }
    }
    reserved
}

#[allow(clippy::too_many_arguments)]
fn find_path(
    width: i32,
    height: i32,
    sources: &[FixedEndpoint],
    targets: &[FixedEndpoint],
    facility_cells: &[bool],
    used: &[Option<RouteCellShape>],
    reserved: &BTreeSet<usize>,
) -> Option<(FixedEndpoint, FixedEndpoint, Vec<usize>)> {
    let cell_count = facility_cells.len();
    for source in sources {
        if let Some(target) = targets.iter().find(|target| target.cell == source.cell)
            && !facility_cells[source.cell]
            && used[source.cell].is_none()
        {
            return Some((source.clone(), target.clone(), vec![source.cell]));
        }
    }
    let state_count = cell_count * 5;
    let mut parent = vec![usize::MAX; state_count];
    let mut root = vec![usize::MAX; state_count];
    let mut distance = vec![(usize::MAX, usize::MAX); state_count];
    let mut target_by_cell = BTreeMap::new();
    for (index, target) in targets.iter().enumerate() {
        target_by_cell.entry(target.cell).or_insert(index);
    }
    let target_cells = targets
        .iter()
        .map(|target| target.cell)
        .collect::<BTreeSet<_>>();
    let source_cells = sources
        .iter()
        .map(|source| source.cell)
        .collect::<BTreeSet<_>>();
    let mut queue = BinaryHeap::new();
    for (index, source) in sources.iter().enumerate() {
        if facility_cells[source.cell]
            || used[source.cell].is_some()
            || target_cells.contains(&source.cell)
        {
            continue;
        }
        let state = source.cell * 5 + 4;
        if parent[state] == usize::MAX {
            parent[state] = state;
            root[state] = index;
            distance[state] = (0, 0);
            queue.push(Reverse((0, 0, state)));
        }
    }

    while let Some(Reverse((steps, turns, state))) = queue.pop() {
        if distance[state] != (steps, turns) {
            continue;
        }
        let cell = state / 5;
        let incoming = match state % 5 {
            0 => Some(StepDirection::North),
            1 => Some(StepDirection::East),
            2 => Some(StepDirection::South),
            3 => Some(StepDirection::West),
            4 => None,
            _ => unreachable!(),
        };
        if let Some(target_index) = target_by_cell.get(&cell).copied()
            && incoming.is_some()
            && !source_cells.contains(&cell)
            && used[cell].is_none()
        {
            let mut path = vec![cell];
            let mut current = state;
            while parent[current] != current {
                current = parent[current];
                path.push(current / 5);
            }
            path.reverse();
            if path.iter().copied().collect::<BTreeSet<_>>().len() == path.len() {
                return Some((
                    sources[root[state]].clone(),
                    targets[target_index].clone(),
                    path,
                ));
            }
        }
        let x = (cell % width as usize) as i32;
        let y = (cell / width as usize) as i32;
        for (direction, next_x, next_y) in [
            (StepDirection::West, x - 1, y),
            (StepDirection::East, x + 1, y),
            (StepDirection::North, x, y - 1),
            (StepDirection::South, x, y + 1),
        ] {
            if next_x < 0 || next_y < 0 || next_x >= width || next_y >= height {
                continue;
            }
            if let Some(existing) = used[cell] {
                let new_shape = match incoming {
                    Some(incoming) if incoming == direction => direction.orientation(),
                    _ => RouteCellShape::Blocked,
                };
                if !crossing_allowed(existing, new_shape) {
                    continue;
                }
            }
            let next = grid_index(next_x, next_y, width);
            let next_state = next * 5 + direction.index();
            if facility_cells[next] {
                continue;
            }
            if reserved.contains(&next) && !target_cells.contains(&next) {
                continue;
            }
            let next_distance = (
                steps + 1,
                turns + usize::from(incoming.is_some_and(|incoming| incoming != direction)),
            );
            if next_distance < distance[next_state] {
                distance[next_state] = next_distance;
                parent[next_state] = state;
                root[next_state] = root[state];
                queue.push(Reverse((next_distance.0, next_distance.1, next_state)));
            }
        }
    }
    None
}

fn success_report(
    input: &ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    placements: BTreeMap<String, SparsePlacement>,
    mut indexed_routes: Vec<(usize, IntegratedRoute)>,
    bridges: BTreeSet<(TransportKind, usize)>,
    diagnostic_code: &'static str,
    diagnostic_message: &'static str,
) -> IntegratedLayoutReport {
    let mut placements = placements
        .into_values()
        .map(|placement| placement.placement)
        .collect::<Vec<_>>();
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));
    indexed_routes.sort_by_key(|(index, _)| *index);
    let routes = indexed_routes
        .into_iter()
        .map(|(_, route)| route)
        .collect::<Vec<_>>();
    let logistics_components = bridges
        .into_iter()
        .map(|(transport, cell)| {
            let definition = components
                .component_by_kind(transport, LogisticsComponentKind::Bridge)
                .expect("validated catalog has every transport bridge capability");
            let position = world_position(cell, input.width);
            let owners = routes
                .iter()
                .filter(|route| {
                    route.transport == transport
                        && route.cells.iter().any(|route_cell| route_cell == &position)
                })
                .map(|route| route.requirement_id.clone())
                .collect::<BTreeSet<_>>();
            PlacedLogisticsComponent {
                id: super::retained::logistics_component_id(
                    LogisticsComponentKind::Bridge,
                    transport,
                    position.x,
                    position.y,
                    &owners,
                ),
                component: definition.id.clone(),
                kind: definition.kind,
                transport,
                position,
                rotation: 0,
            }
        })
        .collect::<Vec<_>>();
    let mut report = IntegratedLayoutReport {
        schema_version: super::INTEGRATED_LAYOUT_SCHEMA_VERSION,
        success: true,
        status: IntegratedLayoutStatus::Feasible,
        bounds: None,
        placements,
        logistics_components,
        routes,
        phases: Vec::new(),
        diagnostics: vec![IntegratedLayoutDiagnostic::info(
            diagnostic_code,
            diagnostic_message,
        )],
    };
    super::canonicalize_report_geometry(&mut report);
    report
}

fn validate_success_report(
    input: &ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    report: IntegratedLayoutReport,
) -> IntegratedLayoutReport {
    if let Err(diagnostic) = super::witness::validate(input, components, &report) {
        return IntegratedLayoutReport::failure(IntegratedLayoutStatus::Unknown, diagnostic);
    }
    if let Err(diagnostic) =
        super::retained::RetainedRoutingState::from_validated_report(input, &report)
    {
        return IntegratedLayoutReport::failure(IntegratedLayoutStatus::Unknown, diagnostic);
    }
    report
}

fn route_cell_shape(path: &[usize], index: usize, width: i32) -> RouteCellShape {
    if index == 0 || index + 1 == path.len() {
        return RouteCellShape::Blocked;
    }
    let previous = path[index - 1];
    let current = path[index];
    let next = path[index + 1];
    let previous_x = previous % width as usize;
    let current_x = current % width as usize;
    let next_x = next % width as usize;
    if previous_x == current_x && current_x == next_x {
        RouteCellShape::Vertical
    } else if previous / width as usize == current / width as usize
        && current / width as usize == next / width as usize
    {
        RouteCellShape::Horizontal
    } else {
        RouteCellShape::Blocked
    }
}

fn crossing_allowed(existing: RouteCellShape, new: RouteCellShape) -> bool {
    matches!(
        (existing, new),
        (RouteCellShape::Horizontal, RouteCellShape::Vertical)
            | (RouteCellShape::Vertical, RouteCellShape::Horizontal)
    )
}

fn layer_index(transport: TransportKind) -> usize {
    match transport {
        TransportKind::Belt => 0,
        TransportKind::Pipe => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_only_perpendicular_straight_bridge_crossings() {
        assert!(crossing_allowed(
            RouteCellShape::Horizontal,
            RouteCellShape::Vertical
        ));
        assert!(crossing_allowed(
            RouteCellShape::Vertical,
            RouteCellShape::Horizontal
        ));
        assert!(!crossing_allowed(
            RouteCellShape::Horizontal,
            RouteCellShape::Horizontal
        ));
        assert!(!crossing_allowed(
            RouteCellShape::Blocked,
            RouteCellShape::Vertical
        ));
        assert!(!crossing_allowed(
            RouteCellShape::Crossed,
            RouteCellShape::Horizontal
        ));
    }

    #[test]
    fn deterministic_route_order_keys_change_with_seed() {
        let first = (0..16)
            .map(|index| deterministic_order_key(index, 1))
            .collect::<Vec<_>>();
        let repeated = (0..16)
            .map(|index| deterministic_order_key(index, 1))
            .collect::<Vec<_>>();
        let second = (0..16)
            .map(|index| deterministic_order_key(index, 2))
            .collect::<Vec<_>>();
        assert_eq!(first, repeated);
        assert_ne!(first, second);
    }

    #[test]
    fn shortest_path_breaks_distance_ties_with_fewer_turns() {
        let width = 3;
        let height = 3;
        let source = FixedEndpoint {
            endpoint: IntegratedRouteEndpoint::External {
                node: "source".to_string(),
                side: FacilityPortEdge::North,
            },
            cell: grid_index(0, 0, width),
            external_side: FacilityPortEdge::North,
        };
        let target = FixedEndpoint {
            endpoint: IntegratedRouteEndpoint::External {
                node: "target".to_string(),
                side: FacilityPortEdge::South,
            },
            cell: grid_index(2, 2, width),
            external_side: FacilityPortEdge::South,
        };
        let facility_cells = vec![false; (width * height) as usize];
        let used = vec![None; (width * height) as usize];

        let (_, _, path) = find_path(
            width,
            height,
            &[source],
            &[target],
            &facility_cells,
            &used,
            &BTreeSet::new(),
        )
        .expect("empty grid should have a route");

        assert_eq!(path.len(), 5);
        assert_eq!(grid_path_turns(&path, width), 1);
    }

    fn grid_path_turns(path: &[usize], width: i32) -> usize {
        path.windows(3)
            .filter(|cells| {
                let first_horizontal = cells[0] / width as usize == cells[1] / width as usize;
                let second_horizontal = cells[1] / width as usize == cells[2] / width as usize;
                first_horizontal != second_horizontal
            })
            .count()
    }
}
