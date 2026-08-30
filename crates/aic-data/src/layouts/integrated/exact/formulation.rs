use crate::facilities::FacilityPortDefinition;
use crate::facilities::FacilityPortEdge;
use crate::logistics::{
    CardinalDirection, LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog,
};
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::super::{
    EndpointInput, InstanceInput, TransportNetworkEndpoint, candidate_port_connections, grid_index,
};
use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use super::{Arc, Candidate, EndpointOption, ModelBranchComponent, ModelInstance, ModelNetwork};

pub(in crate::layouts::integrated) type FlowTerms = Vec<(DomainId, i32)>;

pub(in crate::layouts::integrated) const DIRECTIONS: [CardinalDirection; 4] = [
    CardinalDirection::North,
    CardinalDirection::East,
    CardinalDirection::South,
    CardinalDirection::West,
];

pub(in crate::layouts::integrated) fn generate_candidates(
    solver: &mut RecordedModel,
    instance: &InstanceInput,
    max_width: i32,
    max_height: i32,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for rotation in &instance.definition.allowed_rotations {
        let source_width = instance.definition.footprint.width as i32;
        let source_height = instance.definition.footprint.height as i32;
        let (width, height) = if matches!(rotation, 90 | 270) {
            (source_height, source_width)
        } else {
            (source_width, source_height)
        };
        if width > max_width || height > max_height {
            continue;
        }
        for y in 0..=(max_height - height) {
            for x in 0..=(max_width - width) {
                let port_connections = candidate_port_connections(
                    &instance.definition,
                    *rotation,
                    x,
                    y,
                    max_width,
                    max_height,
                );
                let occupied_cells = (y..(y + height))
                    .flat_map(|occupied_y| {
                        (x..(x + width))
                            .map(move |occupied_x| grid_index(occupied_x, occupied_y, max_width))
                    })
                    .collect();
                candidates.push(Candidate {
                    rotation: *rotation,
                    x,
                    y,
                    width,
                    height,
                    occupied_cells,
                    port_connections,
                    selected: solver.new_variable(
                        VariableFamily::Placement,
                        0,
                        1,
                        format!("place-{}-{rotation}-{x}-{y}", instance.id),
                    ),
                });
            }
        }
    }
    candidates
}

fn endpoint_options(
    solver: &mut RecordedModel,
    edge_index: usize,
    endpoint_kind: &str,
    instance: &ModelInstance,
    ports: &[FacilityPortDefinition],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EndpointOption> {
    let mut options = Vec::new();
    for (candidate_index, candidate) in instance.candidates.iter().enumerate() {
        let mut candidate_options = Vec::new();
        for port in ports {
            let Some(cell) = candidate.port_connections.get(&port.id).copied() else {
                continue;
            };
            let selected = solver.new_variable(
                VariableFamily::Endpoint,
                0,
                1,
                format!(
                    "edge-{edge_index}-{endpoint_kind}-{}-{}-{candidate_index}",
                    instance.input.id, port.id
                ),
            );
            candidate_options.push(selected);
            options.push(EndpointOption {
                endpoint: TransportNetworkEndpoint::Facility {
                    instance: instance.input.id.clone(),
                    port: port.id.clone(),
                },
                cell,
                selected,
                external_side: Some(port.edge.rotated_clockwise(candidate.rotation)),
                arm_direction: opposite(edge_direction(
                    port.edge.rotated_clockwise(candidate.rotation),
                )),
            });
        }
        let mut definition = candidate_options
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        definition.push(candidate.selected.scaled(-1));
        solver.post_equals(ConstraintFamily::EndpointLink, definition, 0, 1, tag);
    }
    post_equals_one(
        solver,
        ConstraintFamily::EndpointChoice,
        options.iter().map(|option| option.selected),
        tag,
    );
    options
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn model_facility_endpoint_options(
    solver: &mut RecordedModel,
    edge_index: usize,
    endpoint_kind: &str,
    endpoint: &EndpointInput,
    instances: &[ModelInstance],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EndpointOption> {
    match endpoint {
        EndpointInput::Facility { instance, ports } => {
            let instance = instances
                .iter()
                .find(|model_instance| model_instance.input.id == *instance)
                .expect("prepared endpoint instance exists");
            endpoint_options(solver, edge_index, endpoint_kind, instance, ports, tag)
        }
        EndpointInput::External { .. } => unreachable!("expected a facility endpoint"),
    }
}

pub(in crate::layouts::integrated) fn external_endpoint_options(
    node: &str,
    facility_options: &[EndpointOption],
) -> Vec<EndpointOption> {
    facility_options
        .iter()
        .map(|option| EndpointOption {
            endpoint: TransportNetworkEndpoint::External {
                node: node.to_string(),
                side: option
                    .external_side
                    .expect("facility endpoint option records its outward side"),
            },
            cell: option.cell,
            selected: option.selected,
            external_side: option.external_side,
            arm_direction: edge_direction(
                option
                    .external_side
                    .expect("facility endpoint option records its outward side"),
            ),
        })
        .collect()
}

fn edge_direction(edge: FacilityPortEdge) -> CardinalDirection {
    match edge {
        FacilityPortEdge::North => CardinalDirection::North,
        FacilityPortEdge::East => CardinalDirection::East,
        FacilityPortEdge::South => CardinalDirection::South,
        FacilityPortEdge::West => CardinalDirection::West,
    }
}

fn opposite(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::South,
        CardinalDirection::East => CardinalDirection::West,
        CardinalDirection::South => CardinalDirection::North,
        CardinalDirection::West => CardinalDirection::East,
    }
}

pub(in crate::layouts::integrated) fn grid_arcs(
    solver: &mut RecordedModel,
    network_index: usize,
    width: i32,
    height: i32,
    line_capacity: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (Vec<Arc>, Vec<Vec<Arc>>, Vec<Vec<Arc>>) {
    let cell_count = (width as usize) * (height as usize);
    let mut arcs = Vec::new();
    let mut incoming = vec![Vec::new(); cell_count];
    let mut outgoing = vec![Vec::new(); cell_count];
    for y in 0..height {
        for x in 0..width {
            let from = grid_index(x, y, width);
            for (to_x, to_y) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if to_x < 0 || to_y < 0 || to_x >= width || to_y >= height {
                    continue;
                }
                let to = grid_index(to_x, to_y, width);
                let selected = solver.new_variable(
                    VariableFamily::RouteArc,
                    0,
                    1,
                    format!("network-{network_index}-arc-{from}-{to}-used"),
                );
                let flow = solver.new_variable(
                    VariableFamily::Flow,
                    0,
                    line_capacity,
                    format!("network-{network_index}-arc-{from}-{to}-flow"),
                );
                solver.post_greater_than_or_equals(
                    ConstraintFamily::ArcActivation,
                    vec![flow.scaled(1), selected.scaled(-1)],
                    0,
                    1,
                    tag,
                );
                solver.post_less_than_or_equals(
                    ConstraintFamily::ArcActivation,
                    vec![flow.scaled(1), selected.scaled(-line_capacity)],
                    0,
                    line_capacity.unsigned_abs() as u64,
                    tag,
                );
                let arc = Arc {
                    from,
                    to,
                    flow,
                    selected,
                };
                arcs.push(arc);
                outgoing[from].push(arc);
                incoming[to].push(arc);
            }
        }
    }
    (arcs, incoming, outgoing)
}

pub(in crate::layouts::integrated) fn incident_arcs_by_axis(
    arcs: &[Arc],
    cell_count: usize,
    width: i32,
) -> (Vec<Vec<DomainId>>, Vec<Vec<DomainId>>) {
    let mut horizontal = vec![Vec::new(); cell_count];
    let mut vertical = vec![Vec::new(); cell_count];
    for arc in arcs {
        let axis = if arc.from / width as usize == arc.to / width as usize {
            &mut horizontal
        } else {
            &mut vertical
        };
        axis[arc.from].push(arc.selected);
        axis[arc.to].push(arc.selected);
    }
    (horizontal, vertical)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn post_branch_component_topology(
    solver: &mut RecordedModel,
    network_index: usize,
    cell: usize,
    transport: TransportKind,
    incoming_arms: &[DomainId; 4],
    outgoing_arms: &[DomainId; 4],
    incoming_flow: &[FlowTerms; 4],
    line_capacity: i32,
    splitter_capacity: i32,
    converger_capacity: i32,
    components: &ValidatedLogisticsComponentCatalog,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<ModelBranchComponent> {
    let mut modeled = Vec::new();
    for kind in [
        LogisticsComponentKind::Splitter,
        LogisticsComponentKind::Converger,
    ] {
        let definition = components
            .component_by_kind(transport, kind)
            .expect("validated catalog contains every branch component capability");
        for rotation in &definition.allowed_rotations {
            let selected = solver.new_variable(
                VariableFamily::BranchComponent,
                0,
                1,
                format!(
                    "network-{network_index}-cell-{cell}-{:?}-rotation-{rotation}",
                    kind
                )
                .to_lowercase(),
            );
            let allowed_inputs = definition
                .input_directions
                .iter()
                .map(|direction| rotate_direction(*direction, *rotation))
                .collect::<Vec<_>>();
            let allowed_outputs = definition
                .output_directions
                .iter()
                .map(|direction| rotate_direction(*direction, *rotation))
                .collect::<Vec<_>>();
            for (direction_index, direction) in DIRECTIONS.iter().enumerate() {
                if !allowed_inputs.contains(direction) {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::BranchTopology,
                        vec![incoming_arms[direction_index].scaled(1), selected.scaled(1)],
                        1,
                        1,
                        tag,
                    );
                }
                if !allowed_outputs.contains(direction) {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::BranchTopology,
                        vec![outgoing_arms[direction_index].scaled(1), selected.scaled(1)],
                        1,
                        1,
                        tag,
                    );
                }
            }

            let capacity = match kind {
                LogisticsComponentKind::Splitter => splitter_capacity,
                LogisticsComponentKind::Converger => converger_capacity,
                LogisticsComponentKind::Bridge => unreachable!(),
            };
            let maximum_flow = line_capacity
                .checked_mul(4)
                .expect("validated solver flow domain fits component big-M bound");
            let mut capacity_constraint = incoming_flow
                .iter()
                .flatten()
                .map(|(variable, coefficient)| variable.scaled(*coefficient))
                .collect::<Vec<_>>();
            capacity_constraint.push(selected.scaled(maximum_flow));
            solver.post_less_than_or_equals(
                ConstraintFamily::BranchTopology,
                capacity_constraint,
                capacity + maximum_flow,
                maximum_flow.unsigned_abs() as u64,
                tag,
            );

            modeled.push(ModelBranchComponent {
                network_index,
                transport,
                cell,
                component: definition.id.clone(),
                kind,
                rotation: *rotation,
                selected,
            });
        }
    }

    post_at_most_one(
        solver,
        ConstraintFamily::BranchTopology,
        modeled.iter().map(|component| component.selected),
        tag,
    );
    let splitters = modeled
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Splitter)
        .map(|component| component.selected)
        .collect::<Vec<_>>();
    let convergers = modeled
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Converger)
        .map(|component| component.selected)
        .collect::<Vec<_>>();
    let incoming_count = incoming_arms
        .iter()
        .map(|arm| arm.scaled(1))
        .collect::<Vec<_>>();
    let outgoing_count = outgoing_arms
        .iter()
        .map(|arm| arm.scaled(1))
        .collect::<Vec<_>>();

    let mut incoming_maximum = incoming_count.clone();
    incoming_maximum.extend(convergers.iter().map(|selected| selected.scaled(-2)));
    solver.post_less_than_or_equals(
        ConstraintFamily::BranchTopology,
        incoming_maximum,
        1,
        2,
        tag,
    );
    let mut outgoing_maximum = outgoing_count.clone();
    outgoing_maximum.extend(splitters.iter().map(|selected| selected.scaled(-2)));
    solver.post_less_than_or_equals(
        ConstraintFamily::BranchTopology,
        outgoing_maximum,
        1,
        2,
        tag,
    );

    let mut splitter_minimum_outputs = outgoing_count;
    splitter_minimum_outputs.extend(splitters.iter().map(|selected| selected.scaled(-2)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        splitter_minimum_outputs,
        0,
        2,
        tag,
    );
    let mut splitter_input = incoming_count.clone();
    splitter_input.extend(splitters.iter().map(|selected| selected.scaled(-1)));
    solver.post_greater_than_or_equals(ConstraintFamily::BranchTopology, splitter_input, 0, 1, tag);

    let mut converger_minimum_inputs = incoming_count;
    converger_minimum_inputs.extend(convergers.iter().map(|selected| selected.scaled(-2)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        converger_minimum_inputs,
        0,
        2,
        tag,
    );
    let mut converger_output = outgoing_arms
        .iter()
        .map(|arm| arm.scaled(1))
        .collect::<Vec<_>>();
    converger_output.extend(convergers.iter().map(|selected| selected.scaled(-1)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        converger_output,
        0,
        1,
        tag,
    );

    modeled
}

pub(in crate::layouts::integrated) fn direction_index(direction: CardinalDirection) -> usize {
    match direction {
        CardinalDirection::North => 0,
        CardinalDirection::East => 1,
        CardinalDirection::South => 2,
        CardinalDirection::West => 3,
    }
}

pub(in crate::layouts::integrated) fn direction_between(
    cell: usize,
    neighbor: usize,
    width: i32,
) -> CardinalDirection {
    if neighbor + width as usize == cell {
        CardinalDirection::North
    } else if neighbor == cell + 1 {
        CardinalDirection::East
    } else if neighbor == cell + width as usize {
        CardinalDirection::South
    } else if neighbor + 1 == cell {
        CardinalDirection::West
    } else {
        panic!("grid cells {cell} and {neighbor} are not orthogonal neighbors")
    }
}

pub(in crate::layouts::integrated) fn rotate_direction(
    direction: CardinalDirection,
    rotation: i64,
) -> CardinalDirection {
    let mut direction = direction;
    for _ in 0..(rotation / 90) {
        direction = match direction {
            CardinalDirection::North => CardinalDirection::East,
            CardinalDirection::East => CardinalDirection::South,
            CardinalDirection::South => CardinalDirection::West,
            CardinalDirection::West => CardinalDirection::North,
        };
    }
    direction
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn post_bridge_crossing(
    solver: &mut RecordedModel,
    transport_name: &str,
    cell: usize,
    bridge: DomainId,
    occupancy: &[DomainId],
    networks: &[(usize, &ModelNetwork)],
    branch_components: &[DomainId],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (usize, usize) {
    let mut terminal_constraints = 0_usize;
    for component in branch_components {
        solver.post_less_than_or_equals(
            ConstraintFamily::BridgeCrossing,
            vec![bridge.scaled(1), component.scaled(1)],
            1,
            1,
            tag,
        );
    }
    let mut horizontal_owners = Vec::with_capacity(networks.len());
    let mut vertical_owners = Vec::with_capacity(networks.len());
    for (network_index, network) in networks {
        let horizontal_owner = solver.new_variable(
            VariableFamily::CrossingOwner,
            0,
            1,
            format!("{transport_name}-bridge-{cell}-horizontal-network-{network_index}"),
        );
        let vertical_owner = solver.new_variable(
            VariableFamily::CrossingOwner,
            0,
            1,
            format!("{transport_name}-bridge-{cell}-vertical-network-{network_index}"),
        );
        let mut horizontal_straight = network.horizontal_incident[cell]
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        horizontal_straight.push(horizontal_owner.scaled(-2));
        solver.post_greater_than_or_equals(
            ConstraintFamily::BridgeCrossing,
            horizontal_straight,
            0,
            2,
            tag,
        );
        let mut vertical_straight = network.vertical_incident[cell]
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        vertical_straight.push(vertical_owner.scaled(-2));
        solver.post_greater_than_or_equals(
            ConstraintFamily::BridgeCrossing,
            vertical_straight,
            0,
            2,
            tag,
        );
        for (owner, incident) in [
            (horizontal_owner, &network.horizontal_incident[cell]),
            (vertical_owner, &network.vertical_incident[cell]),
        ] {
            let incoming_flow = network
                .arcs
                .iter()
                .filter(|arc| arc.to == cell && incident.contains(&arc.selected))
                .map(|arc| arc.flow)
                .collect::<Vec<_>>();
            let outgoing_flow = network
                .arcs
                .iter()
                .filter(|arc| arc.from == cell && incident.contains(&arc.selected))
                .map(|arc| arc.flow)
                .collect::<Vec<_>>();
            let mut capacity = incoming_flow
                .iter()
                .map(|flow| flow.scaled(1))
                .collect::<Vec<_>>();
            capacity.push(owner.scaled(network.line_capacity_units));
            solver.post_less_than_or_equals(
                ConstraintFamily::BridgeCrossing,
                capacity,
                network.bridge_capacity_units + network.line_capacity_units,
                network.line_capacity_units.unsigned_abs() as u64,
                tag,
            );
            let mut forward_balance = incoming_flow
                .iter()
                .map(|flow| flow.scaled(1))
                .chain(outgoing_flow.iter().map(|flow| flow.scaled(-1)))
                .collect::<Vec<_>>();
            forward_balance.push(owner.scaled(network.line_capacity_units));
            solver.post_less_than_or_equals(
                ConstraintFamily::BridgeCrossing,
                forward_balance,
                network.line_capacity_units,
                network.line_capacity_units.unsigned_abs() as u64,
                tag,
            );
            let mut reverse_balance = outgoing_flow
                .iter()
                .map(|flow| flow.scaled(1))
                .chain(incoming_flow.iter().map(|flow| flow.scaled(-1)))
                .collect::<Vec<_>>();
            reverse_balance.push(owner.scaled(network.line_capacity_units));
            solver.post_less_than_or_equals(
                ConstraintFamily::BridgeCrossing,
                reverse_balance,
                network.line_capacity_units,
                network.line_capacity_units.unsigned_abs() as u64,
                tag,
            );
        }
        for option in network
            .terminals
            .iter()
            .flat_map(|terminal| terminal.options.iter())
            .filter(|option| option.cell == cell)
        {
            terminal_constraints += 1;
            solver.post_less_than_or_equals(
                ConstraintFamily::BridgeCrossing,
                vec![bridge.scaled(1), option.selected.scaled(1)],
                1,
                1,
                tag,
            );
        }
        horizontal_owners.push(horizontal_owner);
        vertical_owners.push(vertical_owner);
    }

    let mut horizontal_definition = horizontal_owners
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    horizontal_definition.push(bridge.scaled(-1));
    solver.post_equals(
        ConstraintFamily::BridgeCrossing,
        horizontal_definition,
        0,
        1,
        tag,
    );
    let mut vertical_definition = vertical_owners
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    vertical_definition.push(bridge.scaled(-1));
    solver.post_equals(
        ConstraintFamily::BridgeCrossing,
        vertical_definition,
        0,
        1,
        tag,
    );

    let route_cells = networks
        .iter()
        .map(|(_, network)| network.route_cells[cell])
        .collect::<Vec<_>>();
    let mut maximum_occupancy = occupancy
        .iter()
        .chain(route_cells.iter())
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    maximum_occupancy.push(bridge.scaled(-1));
    solver.post_less_than_or_equals(
        ConstraintFamily::TransportCollision,
        maximum_occupancy,
        1,
        1,
        tag,
    );

    (
        networks.len() * 2,
        networks.len() * 8 + terminal_constraints + branch_components.len() + 3,
    )
}

pub(in crate::layouts::integrated) fn post_equals_one(
    solver: &mut RecordedModel,
    family: ConstraintFamily,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    solver.post_equals(
        family,
        variables
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>(),
        1,
        1,
        tag,
    );
}

pub(in crate::layouts::integrated) fn post_at_most_one(
    solver: &mut RecordedModel,
    family: ConstraintFamily,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let terms = variables
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    if terms.len() > 1 {
        solver.post_less_than_or_equals(family, terms, 1, 1, tag);
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::results::SatisfactionResult;
    use pumpkin_solver::core::termination::Indefinite;

    use super::post_bridge_crossing;
    use crate::layouts::integrated::exact::ModelNetwork;
    use crate::layouts::integrated::exact::recorder::{RecordedModel, VariableFamily};

    fn fixed_network(
        solver: &mut RecordedModel,
        tag: pumpkin_solver::core::proof::ConstraintTag,
        name: &str,
        horizontal: bool,
    ) -> ModelNetwork {
        let route_cell =
            solver.new_variable(VariableFamily::RouteCell, 0, 1, format!("{name}-cell"));
        solver.add_clause([route_cell.equality_predicate(1)], tag);
        let first = solver.new_variable(VariableFamily::RouteArc, 0, 1, format!("{name}-first"));
        let second = solver.new_variable(VariableFamily::RouteArc, 0, 1, format!("{name}-second"));
        solver.add_clause([first.equality_predicate(1)], tag);
        solver.add_clause([second.equality_predicate(1)], tag);
        ModelNetwork {
            input_index: 0,
            line_capacity_units: 1,
            bridge_capacity_units: 1,
            terminals: Vec::new(),
            arcs: Vec::new(),
            route_cells: vec![route_cell],
            horizontal_incident: vec![if horizontal {
                vec![first, second]
            } else {
                Vec::new()
            }],
            vertical_incident: vec![if horizontal {
                Vec::new()
            } else {
                vec![first, second]
            }],
        }
    }

    fn crossing_is_satisfiable(horizontal_routes: usize, vertical_routes: usize) -> bool {
        let mut solver = RecordedModel::default();
        let tag = solver.new_constraint_tag();
        let bridge = solver.new_variable(VariableFamily::Bridge, 0, 1, "bridge");
        solver.add_clause([bridge.equality_predicate(1)], tag);
        let mut networks = Vec::new();
        for index in 0..horizontal_routes {
            networks.push(fixed_network(
                &mut solver,
                tag,
                &format!("horizontal-{index}"),
                true,
            ));
        }
        for index in 0..vertical_routes {
            networks.push(fixed_network(
                &mut solver,
                tag,
                &format!("vertical-{index}"),
                false,
            ));
        }
        let indexed = networks.iter().enumerate().collect::<Vec<_>>();
        post_bridge_crossing(&mut solver, "belt", 0, bridge, &[], &indexed, &[], tag);

        let mut brancher = solver.default_brancher();
        let mut resolver = ResolutionResolver::default();
        matches!(
            solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver),
            SatisfactionResult::Satisfiable(_)
        )
    }

    fn same_network_crossing_is_satisfiable() -> bool {
        let mut solver = RecordedModel::default();
        let tag = solver.new_constraint_tag();
        let bridge = solver.new_variable(VariableFamily::Bridge, 0, 1, "bridge");
        solver.add_clause([bridge.equality_predicate(1)], tag);
        let horizontal = fixed_network(&mut solver, tag, "shared-horizontal", true);
        let vertical = fixed_network(&mut solver, tag, "shared-vertical", false);
        let network = ModelNetwork {
            input_index: 0,
            line_capacity_units: 1,
            bridge_capacity_units: 1,
            terminals: Vec::new(),
            arcs: Vec::new(),
            route_cells: horizontal.route_cells,
            horizontal_incident: horizontal.horizontal_incident,
            vertical_incident: vertical.vertical_incident,
        };
        post_bridge_crossing(
            &mut solver,
            "belt",
            0,
            bridge,
            &[],
            &[(0, &network)],
            &[],
            tag,
        );

        let mut brancher = solver.default_brancher();
        let mut resolver = ResolutionResolver::default();
        matches!(
            solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver),
            SatisfactionResult::Satisfiable(_)
        )
    }

    #[test]
    fn bridge_accepts_exactly_one_horizontal_and_one_vertical_route() {
        assert!(crossing_is_satisfiable(1, 1));
    }

    #[test]
    fn bridge_rejects_two_parallel_routes() {
        assert!(!crossing_is_satisfiable(2, 0));
    }

    #[test]
    fn bridge_accepts_two_independent_axes_from_one_network() {
        assert!(same_network_crossing_is_satisfiable());
    }
}
