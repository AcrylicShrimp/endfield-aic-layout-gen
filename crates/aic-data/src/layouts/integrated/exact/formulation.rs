use crate::facilities::FacilityPortDefinition;
use pumpkin_solver::Solver;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::super::{
    EndpointInput, InstanceInput, TransportNetworkEndpoint, candidate_port_connections, grid_index,
};
use super::{Arc, Candidate, EndpointOption, ModelInstance, ModelNetwork};

pub(in crate::layouts::integrated) fn generate_candidates(
    solver: &mut Solver,
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
                    selected: solver.new_named_bounded_integer(
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
    solver: &mut Solver,
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
            let selected = solver.new_named_bounded_integer(
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
            });
        }
        let mut definition = candidate_options
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        definition.push(candidate.selected.scaled(-1));
        solver
            .add_constraint(pumpkin_solver::equals(definition, 0, tag))
            .post();
    }
    post_equals_one(solver, options.iter().map(|option| option.selected), tag);
    options
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn model_facility_endpoint_options(
    solver: &mut Solver,
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
        })
        .collect()
}

pub(in crate::layouts::integrated) fn grid_arcs(
    solver: &mut Solver,
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
                let selected = solver.new_named_bounded_integer(
                    0,
                    1,
                    format!("network-{network_index}-arc-{from}-{to}-used"),
                );
                let flow = solver.new_named_bounded_integer(
                    0,
                    line_capacity,
                    format!("network-{network_index}-arc-{from}-{to}-flow"),
                );
                solver
                    .add_constraint(pumpkin_solver::greater_than_or_equals(
                        [flow.scaled(1), selected.scaled(-1)],
                        0,
                        tag,
                    ))
                    .post();
                solver
                    .add_constraint(pumpkin_solver::less_than_or_equals(
                        [flow.scaled(1), selected.scaled(-line_capacity)],
                        0,
                        tag,
                    ))
                    .post();
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
pub(in crate::layouts::integrated) fn post_bridge_crossing(
    solver: &mut Solver,
    transport_name: &str,
    cell: usize,
    bridge: DomainId,
    occupancy: &[DomainId],
    networks: &[(usize, &ModelNetwork)],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (usize, usize) {
    let mut horizontal_owners = Vec::with_capacity(networks.len());
    let mut vertical_owners = Vec::with_capacity(networks.len());
    for (network_index, network) in networks {
        let horizontal_owner = solver.new_named_bounded_integer(
            0,
            1,
            format!("{transport_name}-bridge-{cell}-horizontal-network-{network_index}"),
        );
        let vertical_owner = solver.new_named_bounded_integer(
            0,
            1,
            format!("{transport_name}-bridge-{cell}-vertical-network-{network_index}"),
        );
        let mut horizontal_straight = network.horizontal_incident[cell]
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        horizontal_straight.push(horizontal_owner.scaled(-2));
        solver
            .add_constraint(pumpkin_solver::greater_than_or_equals(
                horizontal_straight,
                0,
                tag,
            ))
            .post();
        let mut vertical_straight = network.vertical_incident[cell]
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        vertical_straight.push(vertical_owner.scaled(-2));
        solver
            .add_constraint(pumpkin_solver::greater_than_or_equals(
                vertical_straight,
                0,
                tag,
            ))
            .post();
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                [horizontal_owner.scaled(1), vertical_owner.scaled(1)],
                1,
                tag,
            ))
            .post();
        for option in network
            .terminals
            .iter()
            .flat_map(|terminal| terminal.options.iter())
            .filter(|option| option.cell == cell)
        {
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    [bridge.scaled(1), option.selected.scaled(1)],
                    1,
                    tag,
                ))
                .post();
        }
        horizontal_owners.push(horizontal_owner);
        vertical_owners.push(vertical_owner);
    }

    let mut horizontal_definition = horizontal_owners
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    horizontal_definition.push(bridge.scaled(-1));
    solver
        .add_constraint(pumpkin_solver::equals(horizontal_definition, 0, tag))
        .post();
    let mut vertical_definition = vertical_owners
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    vertical_definition.push(bridge.scaled(-1));
    solver
        .add_constraint(pumpkin_solver::equals(vertical_definition, 0, tag))
        .post();

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
    solver
        .add_constraint(pumpkin_solver::less_than_or_equals(
            maximum_occupancy,
            1,
            tag,
        ))
        .post();

    (networks.len() * 2, networks.len() * 3 + 3)
}

pub(in crate::layouts::integrated) fn post_acyclic_network_ordering(
    solver: &mut Solver,
    network_index: usize,
    arcs: &[Arc],
    cell_count: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let order = (0..cell_count)
        .map(|cell| {
            solver.new_named_bounded_integer(
                0,
                cell_count - 1,
                format!("network-{network_index}-order-{cell}"),
            )
        })
        .collect::<Vec<_>>();
    for arc in arcs {
        solver
            .add_constraint(pumpkin_solver::greater_than_or_equals(
                [
                    order[arc.to].scaled(1),
                    order[arc.from].scaled(-1),
                    arc.selected.scaled(-cell_count),
                ],
                1 - cell_count,
                tag,
            ))
            .post();
    }
}

pub(in crate::layouts::integrated) fn post_equals_one(
    solver: &mut Solver,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    solver
        .add_constraint(pumpkin_solver::equals(
            variables
                .map(|variable| variable.scaled(1))
                .collect::<Vec<_>>(),
            1,
            tag,
        ))
        .post();
}

pub(in crate::layouts::integrated) fn post_at_most_one(
    solver: &mut Solver,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let terms = variables
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    if terms.len() > 1 {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(terms, 1, tag))
            .post();
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::Solver;
    use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::results::SatisfactionResult;
    use pumpkin_solver::core::termination::Indefinite;

    use super::{Arc, post_acyclic_network_ordering, post_bridge_crossing};
    use crate::layouts::integrated::exact::ModelNetwork;

    fn fixed_network(
        solver: &mut Solver,
        tag: pumpkin_solver::core::proof::ConstraintTag,
        name: &str,
        horizontal: bool,
    ) -> ModelNetwork {
        let route_cell = solver.new_named_bounded_integer(0, 1, format!("{name}-cell"));
        solver.add_clause([route_cell.equality_predicate(1)], tag);
        let first = solver.new_named_bounded_integer(0, 1, format!("{name}-first"));
        let second = solver.new_named_bounded_integer(0, 1, format!("{name}-second"));
        solver.add_clause([first.equality_predicate(1)], tag);
        solver.add_clause([second.equality_predicate(1)], tag);
        ModelNetwork {
            input_index: 0,
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
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let bridge = solver.new_named_bounded_integer(0, 1, "bridge");
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
        post_bridge_crossing(&mut solver, "belt", 0, bridge, &[], &indexed, tag);

        let mut brancher = solver.default_brancher();
        let mut resolver = ResolutionResolver::default();
        matches!(
            solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver),
            SatisfactionResult::Satisfiable(_)
        )
    }

    #[test]
    fn route_ordering_rejects_a_disconnected_directed_cycle() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let forward = solver.new_named_bounded_integer(0, 1, "cycle-0-1");
        let backward = solver.new_named_bounded_integer(0, 1, "cycle-1-0");
        solver.add_clause([forward.equality_predicate(1)], tag);
        solver.add_clause([backward.equality_predicate(1)], tag);
        post_acyclic_network_ordering(
            &mut solver,
            0,
            &[
                Arc {
                    from: 0,
                    to: 1,
                    flow: forward,
                    selected: forward,
                },
                Arc {
                    from: 1,
                    to: 0,
                    flow: backward,
                    selected: backward,
                },
            ],
            2,
            tag,
        );

        let mut brancher = solver.default_brancher();
        let mut resolver = ResolutionResolver::default();
        let result = solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver);

        assert!(matches!(result, SatisfactionResult::Unsatisfiable(_, _, _)));
    }

    #[test]
    fn bridge_accepts_exactly_one_horizontal_and_one_vertical_route() {
        assert!(crossing_is_satisfiable(1, 1));
    }

    #[test]
    fn bridge_rejects_two_parallel_routes() {
        assert!(!crossing_is_satisfiable(2, 0));
    }
}
