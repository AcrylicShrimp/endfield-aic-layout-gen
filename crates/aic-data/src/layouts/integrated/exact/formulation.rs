use crate::facilities::FacilityPortDefinition;
use pumpkin_solver::Solver;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::super::{
    Arc, Candidate, EndpointInput, EndpointOption, InstanceInput, IntegratedRouteEndpoint,
    ModelInstance, candidate_port_connections, grid_index,
};

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
                endpoint: IntegratedRouteEndpoint::Facility {
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
            endpoint: IntegratedRouteEndpoint::External {
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
    edge_index: usize,
    width: i32,
    height: i32,
) -> (Vec<Arc>, Vec<Vec<DomainId>>, Vec<Vec<DomainId>>) {
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
                    format!("route-{edge_index}-arc-{from}-{to}"),
                );
                arcs.push(Arc { from, to, selected });
                outgoing[from].push(selected);
                incoming[to].push(selected);
            }
        }
    }
    (arcs, incoming, outgoing)
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
