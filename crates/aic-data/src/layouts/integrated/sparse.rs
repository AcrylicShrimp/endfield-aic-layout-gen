use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::layouts::{FacilityPlacement, FacilityPlacementBounds};
use crate::logistics::{LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog};

use super::{
    BoundarySide, EndpointInput, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    IntegratedLayoutStatus, IntegratedRoute, IntegratedRouteEndpoint, ModelInput,
    PlacedLogisticsComponent, candidate_port_connections, grid_index, world_position,
};

const PLACEMENT_GAPS: [i32; 5] = [20, 14, 10, 6, 2];
const ACTIVE_ROUTING_MARGIN: i32 = 10;

struct SparsePlacement {
    placement: FacilityPlacement,
    port_connections: BTreeMap<String, usize>,
}

#[derive(Clone)]
struct FixedEndpoint {
    endpoint: IntegratedRouteEndpoint,
    cell: usize,
}

#[derive(Clone)]
enum AssignedEndpoint {
    Fixed(FixedEndpoint),
    Boundary { node: String },
}

#[derive(Clone)]
struct AssignedRoute {
    edge_index: usize,
    source: AssignedEndpoint,
    target: AssignedEndpoint,
}

pub(super) fn construct(
    input: ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
) -> IntegratedLayoutReport {
    let mut port_failure = None;
    let mut best_routing_failure = None;
    for gap in PLACEMENT_GAPS {
        let Some(placements) = place_on_shelves(&input, gap) else {
            continue;
        };
        for routing_height in active_routing_heights(&input, &placements) {
            for order in route_orders(&input) {
                let assigned = match assign_facility_ports(&input, &placements, &order) {
                    Ok(assigned) => assigned,
                    Err(failure) => {
                        port_failure = Some(failure);
                        continue;
                    }
                };
                match route_all(&input, &placements, &assigned, routing_height) {
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
                        return match super::witness::validate(&input, components, &report) {
                            Ok(()) => report,
                            Err(diagnostic) => IntegratedLayoutReport::failure(
                                IntegratedLayoutStatus::Unknown,
                                diagnostic,
                            ),
                        };
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

    let diagnostic = if let Some(failure) = best_routing_failure {
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
) -> IntegratedLayoutReport {
    let placements = match index_placements(&input, placements) {
        Ok(placements) => placements,
        Err(diagnostic) => {
            return IntegratedLayoutReport::failure(IntegratedLayoutStatus::Unknown, diagnostic);
        }
    };
    let mut port_failure = None;
    let mut best_routing_failure = None;

    for routing_height in active_routing_heights(&input, &placements) {
        for order in route_orders(&input) {
            let assigned = match assign_facility_ports(&input, &placements, &order) {
                Ok(assigned) => assigned,
                Err(failure) => {
                    port_failure = Some(failure);
                    continue;
                }
            };
            match route_all(&input, &placements, &assigned, routing_height) {
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
                    return match super::witness::validate(&input, components, &report) {
                        Ok(()) => report,
                        Err(diagnostic) => IntegratedLayoutReport::failure(
                            IntegratedLayoutStatus::Unknown,
                            diagnostic,
                        ),
                    };
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

    let diagnostic = if let Some(failure) = best_routing_failure {
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
        let Some(port_connections) = candidate_port_connections(
            &instance.definition,
            placement.rotation,
            x,
            y,
            input.width,
            input.height,
        ) else {
            return Err(IntegratedLayoutDiagnostic::error(
                "coordinate-placement-port-projection-failed",
                "/placements",
                Some(placement.instance.clone()),
                "coordinate placement has a port connection outside the routing grid",
            ));
        };
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
        )?;
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

fn route_orders(input: &ModelInput) -> Vec<Vec<usize>> {
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
            std::cmp::Reverse(network.boundary_terminal_count()),
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
        let boundaries = usize::from(matches!(edge.source, EndpointInput::Boundary { .. }))
            + usize::from(matches!(edge.target, EndpointInput::Boundary { .. }));
        (boundaries, *index)
    });
    let mut boundary_first = facility_first.clone();
    boundary_first.reverse();
    let mut orders = vec![
        original,
        reversed,
        network_first,
        terminal_first,
        facility_first,
        boundary_first,
    ];
    for seed in 1_u64..=96 {
        let mut shuffled = (0..input.edges.len()).collect::<Vec<_>>();
        shuffled.sort_by_key(|index| deterministic_order_key(*index as u64, seed));
        orders.push(shuffled);
    }
    orders
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
        let source = assign_endpoint(
            *edge_index,
            "source",
            &edge.source,
            placements,
            &mut reserved[layer],
        )?;
        let target = assign_endpoint(
            *edge_index,
            "target",
            &edge.target,
            placements,
            &mut reserved[layer],
        )?;
        assigned.push(AssignedRoute {
            edge_index: *edge_index,
            source,
            target,
        });
    }

    Ok(assigned)
}

fn assign_endpoint(
    edge_index: usize,
    endpoint_kind: &'static str,
    endpoint: &EndpointInput,
    placements: &BTreeMap<String, SparsePlacement>,
    reserved: &mut BTreeSet<usize>,
) -> Result<AssignedEndpoint, PortAssignmentFailure> {
    match endpoint {
        EndpointInput::Facility { instance, ports } => {
            let placement = placements
                .get(instance)
                .expect("prepared facility endpoint has a sparse placement");
            let (port, cell) = ports
                .iter()
                .filter_map(|port| {
                    placement
                        .port_connections
                        .get(&port.id)
                        .map(|cell| (port, *cell))
                })
                .find(|(_, cell)| !reserved.contains(cell))
                .ok_or_else(|| PortAssignmentFailure {
                    edge_index,
                    endpoint_kind,
                    instance: instance.clone(),
                })?;
            reserved.insert(cell);
            Ok(AssignedEndpoint::Fixed(FixedEndpoint {
                endpoint: IntegratedRouteEndpoint::Facility {
                    instance: instance.clone(),
                    port: port.id.clone(),
                },
                cell,
            }))
        }
        EndpointInput::Boundary { node } => Ok(AssignedEndpoint::Boundary { node: node.clone() }),
    }
}

fn route_all(
    input: &ModelInput,
    placements: &BTreeMap<String, SparsePlacement>,
    assigned: &[AssignedRoute],
    routing_height: i32,
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
        let edge = &input.edges[route.edge_index];
        let layer = layer_index(edge.transport);
        let source_options = endpoint_options(
            &route.source,
            input.width,
            routing_height,
            input.height,
            &facility_cells,
            &used[layer],
            &reserved[layer],
        );
        let target_options = endpoint_options(
            &route.target,
            input.width,
            routing_height,
            input.height,
            &facility_cells,
            &used[layer],
            &reserved[layer],
        );
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
            if let AssignedEndpoint::Fixed(endpoint) = endpoint {
                reserved[layer].insert(endpoint.cell);
            }
        }
    }
    reserved
}

fn endpoint_options(
    endpoint: &AssignedEndpoint,
    width: i32,
    routing_height: i32,
    hard_height: i32,
    facility_cells: &[bool],
    used: &[Option<RouteCellShape>],
    reserved: &BTreeSet<usize>,
) -> Vec<FixedEndpoint> {
    match endpoint {
        AssignedEndpoint::Fixed(endpoint) => vec![endpoint.clone()],
        AssignedEndpoint::Boundary { node } => boundary_cells(width, routing_height, hard_height)
            .into_iter()
            .filter(|(_, cell)| {
                !facility_cells[*cell] && used[*cell].is_none() && !reserved.contains(cell)
            })
            .map(|(side, cell)| FixedEndpoint {
                endpoint: IntegratedRouteEndpoint::Boundary {
                    node: node.clone(),
                    side,
                },
                cell,
            })
            .collect(),
    }
}

fn boundary_cells(width: i32, routing_height: i32, hard_height: i32) -> Vec<(BoundarySide, usize)> {
    let mut cells = Vec::new();
    cells.extend((0..width).map(|x| (BoundarySide::North, grid_index(x, 0, width))));
    cells
        .extend((1..routing_height).map(|y| (BoundarySide::East, grid_index(width - 1, y, width))));
    if routing_height == hard_height && hard_height > 1 {
        cells.extend(
            (0..(width - 1))
                .rev()
                .map(|x| (BoundarySide::South, grid_index(x, hard_height - 1, width))),
        );
        if width > 1 {
            cells.extend(
                (1..(hard_height - 1))
                    .rev()
                    .map(|y| (BoundarySide::West, grid_index(0, y, width))),
            );
        }
    } else if width > 1 {
        cells.extend(
            (1..routing_height)
                .rev()
                .map(|y| (BoundarySide::West, grid_index(0, y, width))),
        );
    }
    cells
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
    let state_count = cell_count * 5;
    let mut parent = vec![usize::MAX; state_count];
    let mut root = vec![usize::MAX; state_count];
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
    let mut queue = VecDeque::new();
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
            queue.push_back(state);
        }
    }

    while let Some(state) = queue.pop_front() {
        let cell = state / 5;
        let incoming = match state % 5 {
            0 => Some(StepDirection::North),
            1 => Some(StepDirection::East),
            2 => Some(StepDirection::South),
            3 => Some(StepDirection::West),
            4 => None,
            _ => unreachable!(),
        };
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
            if parent[next_state] != usize::MAX || facility_cells[next] {
                continue;
            }
            if reserved.contains(&next) && !target_cells.contains(&next) {
                continue;
            }
            parent[next_state] = state;
            root[next_state] = root[state];
            if let Some(target_index) = target_by_cell.get(&next).copied()
                && !source_cells.contains(&next)
                && used[next].is_none()
            {
                let mut path = vec![next];
                let mut current = next_state;
                while parent[current] != current {
                    current = parent[current];
                    path.push(current / 5);
                }
                path.reverse();
                if path.iter().copied().collect::<BTreeSet<_>>().len() != path.len() {
                    continue;
                }
                return Some((
                    sources[root[next_state]].clone(),
                    targets[target_index].clone(),
                    path,
                ));
            }
            queue.push_back(next_state);
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
            PlacedLogisticsComponent {
                id: format!("bridge:{transport:?}:{}:{}", position.x, position.y).to_lowercase(),
                component: definition.id.clone(),
                kind: definition.kind,
                transport,
                position,
                rotation: 0,
            }
        })
        .collect::<Vec<_>>();
    let used_width = placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .chain(
            routes
                .iter()
                .flat_map(|route| route.cells.iter().map(|cell| cell.x + 1)),
        )
        .max()
        .unwrap_or(0);
    let used_height = placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .chain(
            routes
                .iter()
                .flat_map(|route| route.cells.iter().map(|cell| cell.y + 1)),
        )
        .max()
        .unwrap_or(0);
    debug_assert!(used_width <= i64::from(input.width));
    debug_assert!(used_height <= i64::from(input.height));

    IntegratedLayoutReport {
        success: true,
        status: IntegratedLayoutStatus::Feasible,
        bounds: Some(FacilityPlacementBounds {
            width: used_width,
            height: used_height,
        }),
        placements,
        logistics_components,
        routes,
        diagnostics: vec![IntegratedLayoutDiagnostic::info(
            diagnostic_code,
            diagnostic_message,
        )],
    }
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
    fn enumerates_each_boundary_cell_once() {
        let cells = boundary_cells(5, 4, 4)
            .into_iter()
            .map(|(_, cell)| cell)
            .collect::<Vec<_>>();
        assert_eq!(cells.len(), 14);
        assert_eq!(cells.iter().copied().collect::<BTreeSet<_>>().len(), 14);
    }

    #[test]
    fn active_boundary_omits_the_unsearched_south_side() {
        let cells = boundary_cells(5, 2, 4);
        assert_eq!(cells.len(), 7);
        assert!(
            cells
                .iter()
                .all(|(side, _)| !matches!(side, BoundarySide::South))
        );
        assert_eq!(
            cells
                .iter()
                .map(|(_, cell)| *cell)
                .collect::<BTreeSet<_>>()
                .len(),
            7
        );
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
}
