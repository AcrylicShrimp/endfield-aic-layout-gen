use std::collections::{BTreeMap, BTreeSet};

use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::ProblemSolution;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use super::{DIRECTIONS, post_presence};
use crate::facilities::{FacilityPortDirection, FacilityPortEdge};
use crate::layouts::integrated::{
    EdgeInput, EndpointInput, ExactModelMetrics, ExternalBoundaryConnector,
    ExternalConnectorTemplate, IntegratedLayoutDiagnostic, IntegratedLayoutReport, ModelInput,
    TransportKind, WorldGridPosition, candidate_port_connections, world_position,
};
use crate::logistics::CardinalDirection;

#[derive(Clone)]
pub(super) struct ExternalRequirement {
    pub(super) edge: EdgeInput,
    pub(super) external_node: String,
    pub(super) facility_endpoint: EndpointInput,
    pub(super) direction: FacilityPortDirection,
}

#[derive(Clone)]
pub(super) struct ConnectorSelector {
    pub(super) geometry_key: DomainId,
    pub(super) port_choice: DomainId,
    pub(super) port_ids: Vec<String>,
    pub(super) facility_instance: String,
    pub(super) reachable_geometry_keys: Vec<i32>,
}

#[derive(Clone, Copy)]
pub(super) struct UsedBoundsVariables {
    pub(super) width: DomainId,
    pub(super) height: DomainId,
}

pub(super) struct ModelExternalConnector {
    requirement: ExternalRequirement,
    selector: ConnectorSelector,
    template: DomainId,
    cells: Vec<DomainId>,
    turn: DomainId,
}

#[derive(Clone, Copy)]
struct ConnectorOption {
    geometry_key: i32,
    template: i32,
    side: CardinalDirection,
    selected: DomainId,
}

pub(super) fn partition_external_requirements(
    input: &ModelInput,
) -> Result<(ModelInput, Vec<ExternalRequirement>), IntegratedLayoutDiagnostic> {
    let mut internal_edges = Vec::new();
    let mut external = Vec::new();
    for edge in &input.edges {
        match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => {
                internal_edges.push(edge.clone());
            }
            (EndpointInput::External { node }, facility @ EndpointInput::Facility { .. }) => {
                external.push(ExternalRequirement {
                    edge: edge.clone(),
                    external_node: node.clone(),
                    facility_endpoint: facility.clone(),
                    direction: FacilityPortDirection::Input,
                });
            }
            (facility @ EndpointInput::Facility { .. }, EndpointInput::External { node }) => {
                external.push(ExternalRequirement {
                    edge: edge.clone(),
                    external_node: node.clone(),
                    facility_endpoint: facility.clone(),
                    direction: FacilityPortDirection::Output,
                });
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => unreachable!(),
        }
    }
    let mut routing_input = input.clone();
    routing_input.edges = internal_edges;
    routing_input.networks = super::super::networks::normalize(&routing_input.edges)?;
    Ok((routing_input, external))
}

pub(super) fn new_used_bounds(
    solver: &mut RecordedModel,
    input: &ModelInput,
) -> UsedBoundsVariables {
    UsedBoundsVariables {
        width: solver.new_variable(
            VariableFamily::Objective,
            1,
            input.width,
            "used-bounding-box-width",
        ),
        height: solver.new_variable(
            VariableFamily::Objective,
            1,
            input.height,
            "used-bounding-box-height",
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    solver: &mut RecordedModel,
    input: &ModelInput,
    requirements: &[ExternalRequirement],
    selectors: Vec<ConnectorSelector>,
    used_bounds: UsedBoundsVariables,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<ModelExternalConnector> {
    let mut bound_literals = BTreeMap::new();
    requirements
        .iter()
        .cloned()
        .zip(selectors)
        .enumerate()
        .map(|(connector_index, (requirement, selector))| {
            let template = solver.new_variable(
                VariableFamily::ExternalConnector,
                0,
                2,
                format!("external-connector-{connector_index}-template"),
            );
            metrics.external_connector_variables += 1;
            let options = build_options(solver, connector_index, &selector, template, metrics, tag);
            let cells = (0..input.cell_count as usize)
                .map(|cell| {
                    let x = i32::try_from(cell).expect("grid cell fits i32") % input.width;
                    let y = i32::try_from(cell).expect("grid cell fits i32") / input.width;
                    let contributors = options
                        .iter()
                        .filter_map(|option| {
                            option_cell_literal(
                                solver,
                                connector_index,
                                option,
                                x,
                                y,
                                input.width,
                                used_bounds,
                                &mut bound_literals,
                                metrics,
                                tag,
                            )
                        })
                        .collect::<Vec<_>>();
                    let used = post_presence(
                        solver,
                        VariableFamily::ExternalConnector,
                        ConstraintFamily::ExternalConnector,
                        format!("external-connector-{connector_index}-cell-{cell}"),
                        contributors.into_iter(),
                        tag,
                    );
                    metrics.external_connector_variables += 1;
                    used
                })
                .collect::<Vec<_>>();
            let turn = solver.new_variable(
                VariableFamily::ExternalConnector,
                0,
                1,
                format!("external-connector-{connector_index}-turn"),
            );
            solver.post_table(
                ConstraintFamily::ExternalConnector,
                vec![template, turn],
                vec![vec![0, 0], vec![1, 1], vec![2, 1]],
                tag,
            );
            metrics.external_connector_variables += 1;
            ModelExternalConnector {
                requirement,
                selector,
                template,
                cells,
                turn,
            }
        })
        .collect()
}

fn build_options(
    solver: &mut RecordedModel,
    connector_index: usize,
    selector: &ConnectorSelector,
    template: DomainId,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<ConnectorOption> {
    let template_literals = (0..3)
        .map(|template_index| {
            let literal = solver.new_named_literal_for_predicate(
                VariableFamily::ExternalConnector,
                template.equality_predicate(template_index),
                tag,
                format!("external-connector-{connector_index}-template-{template_index}"),
            );
            metrics.external_connector_variables += 1;
            *literal.get_integer_variable().inner()
        })
        .collect::<Vec<_>>();
    let mut options = Vec::with_capacity(selector.reachable_geometry_keys.len() * 3);
    for geometry_key in &selector.reachable_geometry_keys {
        let geometry_literal = solver.new_named_literal_for_predicate(
            VariableFamily::ExternalConnector,
            selector.geometry_key.equality_predicate(*geometry_key),
            tag,
            format!("external-connector-{connector_index}-geometry-{geometry_key}"),
        );
        metrics.external_connector_variables += 1;
        let geometry_selected = *geometry_literal.get_integer_variable().inner();
        for (template_index, template_selected) in template_literals.iter().enumerate() {
            let selected = post_connector_and(
                solver,
                format!(
                    "external-connector-{connector_index}-option-{geometry_key}-{template_index}"
                ),
                geometry_selected,
                *template_selected,
                tag,
            );
            metrics.external_connector_variables += 1;
            options.push(ConnectorOption {
                geometry_key: *geometry_key,
                template: template_index as i32,
                side: option_side(*geometry_key, template_index as i32),
                selected,
            });
        }
    }
    options
}

#[allow(clippy::too_many_arguments)]
fn option_cell_literal(
    solver: &mut RecordedModel,
    connector_index: usize,
    option: &ConnectorOption,
    x: i32,
    y: i32,
    width: i32,
    used_bounds: UsedBoundsVariables,
    bound_literals: &mut BTreeMap<(bool, i32), DomainId>,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Option<DomainId> {
    let connection_cell = option.geometry_key / 4;
    let connection_x = connection_cell % width;
    let connection_y = connection_cell / width;
    match option.side {
        CardinalDirection::West if y == connection_y && x <= connection_x => Some(option.selected),
        CardinalDirection::North if x == connection_x && y <= connection_y => Some(option.selected),
        CardinalDirection::East if y == connection_y && x >= connection_x => {
            Some(option_with_bound(
                solver,
                connector_index,
                option,
                used_bounds.width,
                true,
                x + 1,
                x == connection_x,
                bound_literals,
                metrics,
                tag,
            ))
        }
        CardinalDirection::South if x == connection_x && y >= connection_y => {
            Some(option_with_bound(
                solver,
                connector_index,
                option,
                used_bounds.height,
                false,
                y + 1,
                y == connection_y,
                bound_literals,
                metrics,
                tag,
            ))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn option_with_bound(
    solver: &mut RecordedModel,
    connector_index: usize,
    option: &ConnectorOption,
    bound: DomainId,
    horizontal: bool,
    threshold: i32,
    require_bound: bool,
    bound_literals: &mut BTreeMap<(bool, i32), DomainId>,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> DomainId {
    let bound_selected = *bound_literals
        .entry((horizontal, threshold))
        .or_insert_with(|| {
            let literal = solver.new_named_literal_for_predicate(
                VariableFamily::ExternalConnector,
                bound.lower_bound_predicate(threshold),
                tag,
                format!(
                    "used-{}-at-least-{threshold}",
                    if horizontal { "width" } else { "height" }
                ),
            );
            metrics.external_connector_variables += 1;
            *literal.get_integer_variable().inner()
        });
    if require_bound {
        solver.post_less_than_or_equals(
            ConstraintFamily::ExternalConnector,
            vec![option.selected.scaled(1), bound_selected.scaled(-1)],
            0,
            1,
            tag,
        );
    }
    let selected = post_connector_and(
        solver,
        format!(
            "external-connector-{connector_index}-option-{}-{}-reaches-{threshold}",
            option.geometry_key, option.template
        ),
        option.selected,
        bound_selected,
        tag,
    );
    metrics.external_connector_variables += 1;
    selected
}

fn post_connector_and(
    solver: &mut RecordedModel,
    name: String,
    left: DomainId,
    right: DomainId,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> DomainId {
    let conjunction = solver.new_variable(VariableFamily::ExternalConnector, 0, 1, name);
    solver.post_less_than_or_equals(
        ConstraintFamily::ExternalConnector,
        vec![conjunction.scaled(1), left.scaled(-1)],
        0,
        1,
        tag,
    );
    solver.post_less_than_or_equals(
        ConstraintFamily::ExternalConnector,
        vec![conjunction.scaled(1), right.scaled(-1)],
        0,
        1,
        tag,
    );
    solver.post_greater_than_or_equals(
        ConstraintFamily::ExternalConnector,
        vec![conjunction.scaled(1), left.scaled(-1), right.scaled(-1)],
        -1,
        1,
        tag,
    );
    conjunction
}

pub(super) fn post_collisions(
    solver: &mut RecordedModel,
    input: &ModelInput,
    facility_occupancy: &[Vec<DomainId>],
    internal_cells: impl Fn(TransportKind, usize) -> Option<DomainId>,
    connectors: &[ModelExternalConnector],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        for cell in 0..input.cell_count as usize {
            let terms = facility_occupancy[cell]
                .iter()
                .copied()
                .chain(internal_cells(transport, cell))
                .chain(
                    connectors
                        .iter()
                        .filter(|connector| connector.requirement.edge.transport == transport)
                        .map(|connector| connector.cells[cell]),
                )
                .map(|variable| variable.scaled(1))
                .collect::<Vec<_>>();
            if terms.len() > 1 {
                solver.post_less_than_or_equals(
                    ConstraintFamily::TransportCollision,
                    terms,
                    1,
                    1,
                    tag,
                );
            }
        }
    }
}

pub(super) fn cells(connector: &ModelExternalConnector) -> &[DomainId] {
    &connector.cells
}

pub(super) fn turn(connector: &ModelExternalConnector) -> DomainId {
    connector.turn
}

pub(super) fn extract(
    solution: &impl ProblemSolution,
    input: &ModelInput,
    connectors: &[ModelExternalConnector],
) -> Vec<ExternalBoundaryConnector> {
    connectors
        .iter()
        .map(|connector| {
            let geometry_key = solution.get_integer_value(connector.selector.geometry_key);
            let template_index = solution.get_integer_value(connector.template);
            let connection_cell =
                usize::try_from(geometry_key / 4).expect("selected connector cell is non-negative");
            let connection = world_position(connection_cell, input.width);
            let side = option_side(geometry_key, template_index);
            let mut cells = connector
                .cells
                .iter()
                .enumerate()
                .filter_map(|(cell, selected)| {
                    (solution.get_integer_value(*selected) == 1)
                        .then(|| world_position(cell, input.width))
                })
                .collect::<Vec<_>>();
            sort_cells(&mut cells, side, connector.requirement.direction);
            let exit = cells
                .iter()
                .cloned()
                .find(|cell| is_exit(cell.clone(), side, &cells))
                .expect("an external connector contains its boundary exit");
            let port_index =
                usize::try_from(solution.get_integer_value(connector.selector.port_choice))
                    .expect("selected connector port is non-negative");
            ExternalBoundaryConnector {
                id: format!(
                    "external-connector:{}",
                    connector.requirement.edge.requirement_id
                ),
                requirement_id: connector.requirement.edge.requirement_id.clone(),
                external_node: connector.requirement.external_node.clone(),
                facility_instance: connector.selector.facility_instance.clone(),
                port: connector.selector.port_ids[port_index].clone(),
                item: connector.requirement.edge.edge.item.clone(),
                transport: connector.requirement.edge.transport,
                direction: connector.requirement.direction,
                rate: connector.requirement.edge.edge.rate,
                template: match template_index {
                    0 => ExternalConnectorTemplate::Forward,
                    1 => ExternalConnectorTemplate::Left,
                    2 => ExternalConnectorTemplate::Right,
                    _ => unreachable!("template domain is 0..2"),
                },
                turn: (template_index != 0).then_some(connection),
                cells,
                boundary_side: direction_edge(side),
                exit,
            }
        })
        .collect()
}

fn option_side(geometry_key: i32, template: i32) -> CardinalDirection {
    let facility_direction =
        usize::try_from(geometry_key % 4).expect("geometry direction is non-negative");
    let outward = (facility_direction + 2) % 4;
    let side = match template {
        0 => outward,
        1 => (outward + 3) % 4,
        2 => (outward + 1) % 4,
        _ => unreachable!("connector template domain is 0..2"),
    };
    DIRECTIONS[side]
}

fn direction_edge(direction: CardinalDirection) -> FacilityPortEdge {
    match direction {
        CardinalDirection::North => FacilityPortEdge::North,
        CardinalDirection::East => FacilityPortEdge::East,
        CardinalDirection::South => FacilityPortEdge::South,
        CardinalDirection::West => FacilityPortEdge::West,
    }
}

fn sort_cells(
    cells: &mut [WorldGridPosition],
    side: CardinalDirection,
    direction: FacilityPortDirection,
) {
    cells.sort_by_key(|cell| match side {
        CardinalDirection::West => -cell.x,
        CardinalDirection::East => cell.x,
        CardinalDirection::North => -cell.y,
        CardinalDirection::South => cell.y,
    });
    if direction == FacilityPortDirection::Input {
        cells.reverse();
    }
}

fn is_exit(cell: WorldGridPosition, side: CardinalDirection, cells: &[WorldGridPosition]) -> bool {
    match side {
        CardinalDirection::West => cell.x == cells.iter().map(|cell| cell.x).min().unwrap(),
        CardinalDirection::East => cell.x == cells.iter().map(|cell| cell.x).max().unwrap(),
        CardinalDirection::North => cell.y == cells.iter().map(|cell| cell.y).min().unwrap(),
        CardinalDirection::South => cell.y == cells.iter().map(|cell| cell.y).max().unwrap(),
    }
}

pub(super) fn validate_unique_ports(
    solver: &mut RecordedModel,
    connectors: &[ModelExternalConnector],
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let mut by_port = BTreeMap::<(String, String), Vec<DomainId>>::new();
    for connector in connectors {
        for (port_index, port) in connector.selector.port_ids.iter().enumerate() {
            let literal = solver.new_named_literal_for_predicate(
                VariableFamily::ExternalConnector,
                connector
                    .selector
                    .port_choice
                    .equality_predicate(i32::try_from(port_index).expect("port index fits i32")),
                tag,
                format!(
                    "external-connector-{}-uses-port-{port}",
                    connector.requirement.edge.requirement_id
                ),
            );
            metrics.external_connector_variables += 1;
            by_port
                .entry((connector.selector.facility_instance.clone(), port.clone()))
                .or_default()
                .push(*literal.get_integer_variable().inner());
        }
    }
    for variables in by_port
        .into_values()
        .filter(|variables| variables.len() > 1)
    {
        solver.post_less_than_or_equals(
            ConstraintFamily::ExternalConnector,
            variables
                .into_iter()
                .map(|variable| variable.scaled(1))
                .collect(),
            1,
            1,
            tag,
        );
    }
}

pub(super) fn validate_witness(
    input: &ModelInput,
    requirements: &[ExternalRequirement],
    report: &IntegratedLayoutReport,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let expected = requirements
        .iter()
        .map(|requirement| (requirement.edge.requirement_id.as_str(), requirement))
        .collect::<BTreeMap<_, _>>();
    if report.external_connectors.len() != expected.len() {
        return Err(invalid_witness(
            "/external_connectors",
            format!(
                "witness has {} external connectors for {} external requirements",
                report.external_connectors.len(),
                expected.len()
            ),
        ));
    }
    let bounds = report.bounds.as_ref().ok_or_else(|| {
        invalid_witness(
            "/bounds",
            "external connector witness requires exact used bounds",
        )
    })?;
    let mut occupied_by_transport = BTreeMap::<(TransportKind, i64, i64), String>::new();
    for network in &report.transport_networks {
        for cell in &network.cells {
            occupied_by_transport.insert(
                (network.transport, cell.x, cell.y),
                format!("network {}", network.id),
            );
        }
    }
    let mut seen = BTreeSet::new();
    for (index, connector) in report.external_connectors.iter().enumerate() {
        if !seen.insert(connector.requirement_id.as_str()) {
            return Err(invalid_witness(
                format!("/external_connectors/{index}/requirement_id"),
                "external connector requirement appears more than once",
            ));
        }
        let requirement = expected
            .get(connector.requirement_id.as_str())
            .ok_or_else(|| {
                invalid_witness(
                    format!("/external_connectors/{index}/requirement_id"),
                    "external connector does not match a prepared external requirement",
                )
            })?;
        if connector.external_node != requirement.external_node
            || connector.item != requirement.edge.edge.item
            || connector.transport != requirement.edge.transport
            || connector.direction != requirement.direction
            || connector.rate != requirement.edge.edge.rate
        {
            return Err(invalid_witness(
                format!("/external_connectors/{index}"),
                "external connector identity, material, direction, or rate does not match its requirement",
            ));
        }
        let placement = report
            .placements
            .iter()
            .find(|placement| placement.instance == connector.facility_instance)
            .ok_or_else(|| {
                invalid_witness(
                    format!("/external_connectors/{index}/facility_instance"),
                    "external connector references an unplaced facility",
                )
            })?;
        let instance = input
            .instances
            .iter()
            .find(|instance| instance.id == connector.facility_instance)
            .expect("validated placement belongs to a prepared instance");
        let port = instance
            .definition
            .ports
            .iter()
            .find(|port| port.id == connector.port)
            .ok_or_else(|| {
                invalid_witness(
                    format!("/external_connectors/{index}/port"),
                    "external connector references an unknown facility port",
                )
            })?;
        if port.direction != connector.direction || port.transport != connector.transport {
            return Err(invalid_witness(
                format!("/external_connectors/{index}/port"),
                "external connector port kind or direction is incompatible",
            ));
        }
        let connection_cell = candidate_port_connections(
            &instance.definition,
            placement.rotation,
            i32::try_from(placement.x).expect("validated placement x fits i32"),
            i32::try_from(placement.y).expect("validated placement y fits i32"),
            input.width,
            input.height,
        )
        .get(&connector.port)
        .copied()
        .ok_or_else(|| {
            invalid_witness(
                format!("/external_connectors/{index}/port"),
                "selected external connector port has no in-bounds adjacent connection cell",
            )
        })?;
        let connection = world_position(connection_cell, input.width);
        let outward = edge_direction(port.edge.rotated_clockwise(placement.rotation));
        let expected_side = match connector.template {
            ExternalConnectorTemplate::Forward => outward,
            ExternalConnectorTemplate::Left => rotate_left(outward),
            ExternalConnectorTemplate::Right => rotate_right(outward),
        };
        if connector.boundary_side != direction_edge(expected_side) {
            return Err(invalid_witness(
                format!("/external_connectors/{index}/boundary_side"),
                "external connector boundary side does not match its selected template",
            ));
        }
        let mut expected_cells = ray_to_boundary(
            connection.clone(),
            expected_side,
            bounds.width,
            bounds.height,
        );
        sort_cells(&mut expected_cells, expected_side, connector.direction);
        if connector.cells != expected_cells {
            return Err(invalid_witness(
                format!("/external_connectors/{index}/cells"),
                "external connector cells do not exactly match its deterministic boundary template",
            ));
        }
        let expected_turn =
            (connector.template != ExternalConnectorTemplate::Forward).then_some(connection);
        if connector.turn != expected_turn {
            return Err(invalid_witness(
                format!("/external_connectors/{index}/turn"),
                "external connector turn does not match its selected template",
            ));
        }
        let expected_exit = expected_cells
            .iter()
            .cloned()
            .find(|cell| is_exit(cell.clone(), expected_side, &expected_cells))
            .expect("deterministic connector has an exit");
        if connector.exit != expected_exit {
            return Err(invalid_witness(
                format!("/external_connectors/{index}/exit"),
                "external connector exit is not on the selected used-bounds side",
            ));
        }
        for cell in &connector.cells {
            if report.placements.iter().any(|placement| {
                cell.x >= placement.x
                    && cell.x < placement.x + placement.width
                    && cell.y >= placement.y
                    && cell.y < placement.y + placement.height
            }) {
                return Err(invalid_witness(
                    format!("/external_connectors/{index}/cells"),
                    "external connector overlaps a production facility",
                ));
            }
            if let Some(owner) = occupied_by_transport
                .insert((connector.transport, cell.x, cell.y), connector.id.clone())
            {
                return Err(invalid_witness(
                    format!("/external_connectors/{index}/cells"),
                    format!("external connector overlaps {owner} on its transport layer"),
                ));
            }
        }
    }
    if let Some(missing) = expected.keys().find(|id| !seen.contains(**id)) {
        return Err(invalid_witness(
            "/external_connectors",
            format!("external requirement '{missing}' has no connector witness"),
        ));
    }
    Ok(())
}

fn ray_to_boundary(
    connection: WorldGridPosition,
    side: CardinalDirection,
    width: i64,
    height: i64,
) -> Vec<WorldGridPosition> {
    match side {
        CardinalDirection::West => (0..=connection.x)
            .map(|x| WorldGridPosition { x, y: connection.y })
            .collect(),
        CardinalDirection::East => (connection.x..width)
            .map(|x| WorldGridPosition { x, y: connection.y })
            .collect(),
        CardinalDirection::North => (0..=connection.y)
            .map(|y| WorldGridPosition { x: connection.x, y })
            .collect(),
        CardinalDirection::South => (connection.y..height)
            .map(|y| WorldGridPosition { x: connection.x, y })
            .collect(),
    }
}

fn edge_direction(edge: FacilityPortEdge) -> CardinalDirection {
    match edge {
        FacilityPortEdge::North => CardinalDirection::North,
        FacilityPortEdge::East => CardinalDirection::East,
        FacilityPortEdge::South => CardinalDirection::South,
        FacilityPortEdge::West => CardinalDirection::West,
    }
}

fn rotate_left(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::West,
        CardinalDirection::West => CardinalDirection::South,
        CardinalDirection::South => CardinalDirection::East,
        CardinalDirection::East => CardinalDirection::North,
    }
}

fn rotate_right(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::East,
        CardinalDirection::East => CardinalDirection::South,
        CardinalDirection::South => CardinalDirection::West,
        CardinalDirection::West => CardinalDirection::North,
    }
}

fn invalid_witness(
    path: impl Into<String>,
    message: impl Into<String>,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error("invalid-external-connector-witness", path, None, message)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn three_templates_cover_every_non_rear_side_once() {
        for facility_direction in 0..4 {
            let geometry_key = facility_direction;
            let outward = DIRECTIONS[(facility_direction as usize + 2) % 4];
            let rear = DIRECTIONS[facility_direction as usize];
            let sides = (0..3)
                .map(|template| option_side(geometry_key, template))
                .collect::<BTreeSet<_>>();
            assert_eq!(sides.len(), 3);
            assert!(sides.contains(&outward));
            assert!(!sides.contains(&rear));
        }
    }

    #[test]
    fn ray_ends_at_the_selected_used_boundary() {
        let connection = WorldGridPosition { x: 2, y: 3 };
        assert_eq!(
            ray_to_boundary(connection.clone(), CardinalDirection::West, 7, 8),
            vec![
                WorldGridPosition { x: 0, y: 3 },
                WorldGridPosition { x: 1, y: 3 },
                WorldGridPosition { x: 2, y: 3 },
            ]
        );
        assert_eq!(
            ray_to_boundary(connection, CardinalDirection::South, 7, 8)
                .last()
                .cloned(),
            Some(WorldGridPosition { x: 2, y: 7 })
        );
    }
}
