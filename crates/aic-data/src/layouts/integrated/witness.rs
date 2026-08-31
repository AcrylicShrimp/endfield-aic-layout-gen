use std::collections::{BTreeMap, BTreeSet};

use crate::facilities::{FacilityPortDirection, FacilityPortEdge};
use crate::layouts::FacilityPlacement;
use crate::logistics::{
    CardinalDirection, LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog,
};
use crate::recipes::Rate;

use super::{
    EndpointInput, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutReport, ModelInput, TransportNetwork, TransportNetworkEndpoint,
    WorldGridPosition, candidate_port_connections, grid_index,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SegmentShape {
    Horizontal,
    Vertical,
    Other,
}

pub(super) fn validate(
    input: &ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    report: &IntegratedLayoutReport,
) -> Result<(), IntegratedLayoutDiagnostic> {
    if report.schema_version != INTEGRATED_LAYOUT_SCHEMA_VERSION {
        return Err(invalid(
            "/schema_version",
            format!(
                "witness schema version {} does not match supported version {}",
                report.schema_version, INTEGRATED_LAYOUT_SCHEMA_VERSION
            ),
        ));
    }
    if report.placements.len() != input.instances.len() {
        return Err(invalid(
            "/placements",
            format!(
                "witness has {} placements for {} facility instances",
                report.placements.len(),
                input.instances.len()
            ),
        ));
    }
    let cell_count = input.width as usize * input.height as usize;
    let placements = validate_placements(input, report, cell_count)?;
    let mut layer_cells = [vec![Vec::new(); cell_count], vec![Vec::new(); cell_count]];
    let expected_networks = input
        .networks
        .iter()
        .map(|network| (network.id(), network))
        .collect::<BTreeMap<_, _>>();
    let mut actual_ids = BTreeSet::new();

    for (network_index, network) in report.transport_networks.iter().enumerate() {
        if !actual_ids.insert(network.id.as_str()) {
            return Err(invalid(
                format!("/transport_networks/{network_index}/id"),
                format!(
                    "transport network ID '{}' appears more than once",
                    network.id
                ),
            ));
        }
        let expected = expected_networks.get(network.id.as_str()).ok_or_else(|| {
            invalid(
                format!("/transport_networks/{network_index}/id"),
                format!("transport network ID '{}' is not expected", network.id),
            )
        })?;
        if network.item != expected.item() || network.transport != expected.transport() {
            return Err(invalid(
                format!("/transport_networks/{network_index}"),
                "transport network item or transport kind does not match the prepared model",
            ));
        }
        let expected_requirements = expected
            .route_indices()
            .iter()
            .map(|index| input.edges[*index].requirement_id.as_str())
            .collect::<BTreeSet<_>>();
        let actual_requirements = network
            .requirement_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if expected_requirements != actual_requirements
            || actual_requirements.len() != network.requirement_ids.len()
        {
            return Err(invalid(
                format!("/transport_networks/{network_index}/requirement_ids"),
                "transport network requirement references do not exactly match the prepared logical flow",
            ));
        }
        validate_network(
            input,
            network_index,
            network,
            expected,
            &placements,
            components,
            report,
            &mut layer_cells,
        )?;
    }
    if let Some(missing) = expected_networks
        .keys()
        .find(|network_id| !actual_ids.contains(**network_id))
    {
        return Err(invalid(
            "/transport_networks",
            format!("required transport network '{missing}' is missing from the witness"),
        ));
    }

    validate_crossings(input, components, report, &placements, &layer_cells)?;
    let (minimum_x, minimum_y, used_width, used_height) = used_geometry_bounds(report);
    if minimum_x != 0 || minimum_y != 0 {
        return Err(invalid(
            "/bounds",
            format!(
                "used geometry must be canonicalized to origin (0, 0), found ({minimum_x}, {minimum_y})"
            ),
        ));
    }
    let bounds = report.bounds.as_ref().ok_or_else(|| {
        invalid(
            "/bounds",
            "successful witness must report its exact used bounds",
        )
    })?;
    if bounds.width != used_width || bounds.height != used_height {
        return Err(invalid(
            "/bounds",
            format!(
                "reported bounds {}x{} do not equal used bounds {used_width}x{used_height}",
                bounds.width, bounds.height
            ),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_network(
    input: &ModelInput,
    network_index: usize,
    network: &TransportNetwork,
    expected: &super::networks::RoutingNetworkInput,
    placements: &ValidatedPlacements<'_>,
    components: &ValidatedLogisticsComponentCatalog,
    report: &IntegratedLayoutReport,
    layer_cells: &mut [Vec<Vec<SegmentShape>>; 2],
) -> Result<(), IntegratedLayoutDiagnostic> {
    if network.cells.is_empty() {
        return Err(invalid(
            format!("/transport_networks/{network_index}/cells"),
            "transport network must occupy at least one cell",
        ));
    }
    let mut cells = BTreeSet::new();
    for (cell_index, position) in network.cells.iter().enumerate() {
        let cell = checked_cell(
            input,
            position,
            &format!("/transport_networks/{network_index}/cells/{cell_index}"),
        )?;
        if !cells.insert(cell) {
            return Err(invalid(
                format!("/transport_networks/{network_index}/cells/{cell_index}"),
                "transport network contains a duplicate occupied cell",
            ));
        }
        if placements.occupied[cell] {
            return Err(invalid(
                format!("/transport_networks/{network_index}/cells/{cell_index}"),
                "transport network occupies a production-facility footprint cell",
            ));
        }
    }

    let mut incoming = BTreeMap::<usize, Rate>::new();
    let mut outgoing = BTreeMap::<usize, Rate>::new();
    let mut incident_neighbors = BTreeMap::<usize, BTreeSet<usize>>::new();
    let mut incoming_directions = BTreeMap::<usize, BTreeSet<CardinalDirection>>::new();
    let mut outgoing_directions = BTreeMap::<usize, BTreeSet<CardinalDirection>>::new();
    let mut incoming_direction_rates = BTreeMap::<usize, BTreeMap<CardinalDirection, Rate>>::new();
    let mut outgoing_direction_rates = BTreeMap::<usize, BTreeMap<CardinalDirection, Rate>>::new();
    for (segment_index, segment) in network.segments.iter().enumerate() {
        if segment.rate.numerator <= 0 || segment.rate.denominator <= 0 {
            return Err(invalid(
                format!("/transport_networks/{network_index}/segments/{segment_index}/rate"),
                "active transport segment must carry a positive rate",
            ));
        }
        if segment.rate > expected.line_capacity_rate() {
            return Err(invalid(
                format!("/transport_networks/{network_index}/segments/{segment_index}/rate"),
                "transport segment exceeds one line of catalog capacity",
            ));
        }
        let from = checked_cell(
            input,
            &segment.from,
            &format!("/transport_networks/{network_index}/segments/{segment_index}/from"),
        )?;
        let to = checked_cell(
            input,
            &segment.to,
            &format!("/transport_networks/{network_index}/segments/{segment_index}/to"),
        )?;
        if !cells.contains(&from) || !cells.contains(&to) {
            return Err(invalid(
                format!("/transport_networks/{network_index}/segments/{segment_index}"),
                "transport segment endpoints must both be occupied network cells",
            ));
        }
        let from_x = from % input.width as usize;
        let from_y = from / input.width as usize;
        let to_x = to % input.width as usize;
        let to_y = to / input.width as usize;
        if from_x.abs_diff(to_x) + from_y.abs_diff(to_y) != 1 {
            return Err(invalid(
                format!("/transport_networks/{network_index}/segments/{segment_index}"),
                "transport segment endpoints must be orthogonally adjacent",
            ));
        }
        add_rate(&mut outgoing, from, segment.rate, network_index)?;
        add_rate(&mut incoming, to, segment.rate, network_index)?;
        incident_neighbors.entry(from).or_default().insert(to);
        incident_neighbors.entry(to).or_default().insert(from);
        let outgoing_direction = direction_between(from, to, input.width);
        let incoming_direction = direction_between(to, from, input.width);
        outgoing_directions
            .entry(from)
            .or_default()
            .insert(outgoing_direction);
        incoming_directions
            .entry(to)
            .or_default()
            .insert(incoming_direction);
        add_direction_rate(
            &mut outgoing_direction_rates,
            from,
            outgoing_direction,
            segment.rate,
            network_index,
        )?;
        add_direction_rate(
            &mut incoming_direction_rates,
            to,
            incoming_direction,
            segment.rate,
            network_index,
        )?;
    }

    let mut expected_terminals = expected
        .terminals()
        .iter()
        .map(|terminal| (terminal.id(), terminal))
        .collect::<BTreeMap<_, _>>();
    let mut supply = BTreeMap::<usize, Rate>::new();
    let mut demand = BTreeMap::<usize, Rate>::new();
    let mut terminal_ids = BTreeSet::new();
    for (terminal_index, terminal) in network.terminals.iter().enumerate() {
        if !terminal_ids.insert(terminal.id.as_str()) {
            return Err(invalid(
                format!("/transport_networks/{network_index}/terminals/{terminal_index}/id"),
                "transport terminal ID appears more than once in its network",
            ));
        }
        if terminal.rate.numerator <= 0 || terminal.rate.denominator <= 0 {
            return Err(invalid(
                format!("/transport_networks/{network_index}/terminals/{terminal_index}/rate"),
                "transport terminal must carry a positive rate",
            ));
        }
        let expected_terminal =
            expected_terminals
                .remove(terminal.id.as_str())
                .ok_or_else(|| {
                    invalid(
                        format!(
                            "/transport_networks/{network_index}/terminals/{terminal_index}/id"
                        ),
                        "transport terminal ID does not match a prepared supply or demand lane",
                    )
                })?;
        if terminal.node != endpoint_node(expected_terminal.endpoint())
            || terminal.direction != expected_terminal.direction()
            || terminal.rate != expected_terminal.rate()
        {
            return Err(invalid(
                format!("/transport_networks/{network_index}/terminals/{terminal_index}"),
                "transport terminal rates or identity do not exactly match the prepared supply or demand lane",
            ));
        }
        let cell = checked_cell(
            input,
            &terminal.position,
            &format!("/transport_networks/{network_index}/terminals/{terminal_index}/position"),
        )?;
        if !cells.contains(&cell) {
            return Err(invalid(
                format!("/transport_networks/{network_index}/terminals/{terminal_index}/position"),
                "transport terminal must occupy a network cell",
            ));
        }
        validate_terminal_endpoint(
            input,
            placements,
            expected_terminal.endpoint(),
            &terminal.endpoint,
            cell,
            network_index,
            terminal_index,
        )?;
        if terminal.direction == FacilityPortDirection::Output {
            add_rate(&mut supply, cell, terminal.rate, network_index)?;
            let direction = terminal_arm_direction(input, placements, &terminal.endpoint)?;
            incoming_directions
                .entry(cell)
                .or_default()
                .insert(direction);
            add_direction_rate(
                &mut incoming_direction_rates,
                cell,
                direction,
                terminal.rate,
                network_index,
            )?;
        } else {
            add_rate(&mut demand, cell, terminal.rate, network_index)?;
            let direction = terminal_arm_direction(input, placements, &terminal.endpoint)?;
            outgoing_directions
                .entry(cell)
                .or_default()
                .insert(direction);
            add_direction_rate(
                &mut outgoing_direction_rates,
                cell,
                direction,
                terminal.rate,
                network_index,
            )?;
        }
    }
    if !expected_terminals.is_empty() {
        return Err(invalid(
            format!("/transport_networks/{network_index}/terminals"),
            "transport terminals do not exactly match all prepared supply and demand lanes",
        ));
    }

    for (cell, rate) in supply.iter().chain(&demand) {
        if *rate > expected.line_capacity_rate() {
            return Err(invalid(
                format!("/transport_networks/{network_index}/cells/{cell}"),
                "transport terminal flow exceeds one line of catalog capacity",
            ));
        }
    }

    for cell in &cells {
        let left = rate_at(&incoming, *cell)
            .checked_add(rate_at(&supply, *cell))
            .map_err(|error| arithmetic_invalid(network_index, error.message))?;
        let right = rate_at(&outgoing, *cell)
            .checked_add(rate_at(&demand, *cell))
            .map_err(|error| arithmetic_invalid(network_index, error.message))?;
        if left != right {
            return Err(invalid(
                format!("/transport_networks/{network_index}/cells"),
                format!("transport flow is not conserved at grid cell {cell}"),
            ));
        }
        let has_terminal = supply.contains_key(cell) || demand.contains_key(cell);
        if !has_terminal && !incident_neighbors.contains_key(cell) {
            return Err(invalid(
                format!("/transport_networks/{network_index}/cells"),
                format!("transport cell {cell} has no terminal or active segment"),
            ));
        }
        validate_branch_topology(
            input,
            components,
            report,
            network_index,
            network,
            *cell,
            incoming_directions.get(cell),
            outgoing_directions.get(cell),
            incoming_direction_rates.get(cell),
            outgoing_direction_rates.get(cell),
            left,
        )?;
    }
    let layer = layer_index(network.transport);
    for cell in &cells {
        for shape in network_cell_shapes(*cell, &incident_neighbors, input.width) {
            layer_cells[layer][*cell].push(shape);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_branch_topology(
    input: &ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    report: &IntegratedLayoutReport,
    network_index: usize,
    network: &TransportNetwork,
    cell: usize,
    incoming: Option<&BTreeSet<CardinalDirection>>,
    outgoing: Option<&BTreeSet<CardinalDirection>>,
    incoming_rates: Option<&BTreeMap<CardinalDirection, Rate>>,
    outgoing_rates: Option<&BTreeMap<CardinalDirection, Rate>>,
    flow: Rate,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let incoming = incoming.cloned().unwrap_or_default();
    let outgoing = outgoing.cloned().unwrap_or_default();
    let incoming_rates = incoming_rates.cloned().unwrap_or_default();
    let outgoing_rates = outgoing_rates.cloned().unwrap_or_default();
    let bridge = report.logistics_components.iter().find(|component| {
        network.component_ids.contains(&component.id)
            && component.kind == LogisticsComponentKind::Bridge
            && component.position.x >= 0
            && component.position.y >= 0
            && component.position.x < i64::from(input.width)
            && component.position.y < i64::from(input.height)
            && grid_index(
                component.position.x as i32,
                component.position.y as i32,
                input.width,
            ) == cell
    });
    if let Some(bridge) = bridge {
        if incoming.is_empty()
            || incoming.len() > 2
            || outgoing.len() != incoming.len()
            || incoming.iter().any(|direction| {
                let opposite = opposite(*direction);
                !outgoing.contains(&opposite)
                    || incoming_rates.get(direction) != outgoing_rates.get(&opposite)
            })
            || (incoming.len() == 2
                && (incoming.iter().all(|direction| {
                    matches!(direction, CardinalDirection::East | CardinalDirection::West)
                }) || incoming.iter().all(|direction| {
                    matches!(
                        direction,
                        CardinalDirection::North | CardinalDirection::South
                    )
                })))
        {
            return Err(invalid(
                format!("/transport_networks/{network_index}/component_ids"),
                format!("bridge at cell {cell} does not preserve independent straight channels"),
            ));
        }
        let definition = components
            .component(&bridge.component)
            .expect("validated component");
        let capacity = Rate::from_quantity_per_duration_ms(
            definition.capacity.quantity,
            definition.capacity.duration_ms,
        )
        .map_err(|error| arithmetic_invalid(network_index, error.message))?;
        if incoming_rates.values().any(|rate| *rate > capacity) {
            return Err(invalid(
                format!("/transport_networks/{network_index}/component_ids"),
                format!("bridge channel at cell {cell} exceeds catalog capacity"),
            ));
        }
        return Ok(());
    }
    let branch_components = report
        .logistics_components
        .iter()
        .filter(|component| {
            network.component_ids.contains(&component.id)
                && component.position.x >= 0
                && component.position.y >= 0
                && matches!(
                    component.kind,
                    LogisticsComponentKind::Splitter | LogisticsComponentKind::Converger
                )
                && component.position.x < i64::from(input.width)
                && component.position.y < i64::from(input.height)
                && grid_index(
                    component.position.x as i32,
                    component.position.y as i32,
                    input.width,
                ) == cell
        })
        .collect::<Vec<_>>();

    let expected_kind = match (incoming.len(), outgoing.len()) {
        (0 | 1, 0 | 1) => None,
        (1, 2 | 3) => Some(LogisticsComponentKind::Splitter),
        (2 | 3, 1) => Some(LogisticsComponentKind::Converger),
        _ => {
            return Err(invalid(
                format!("/transport_networks/{network_index}/cells"),
                format!(
                    "transport cell {cell} has unsupported {}-input/{}-output topology",
                    incoming.len(),
                    outgoing.len()
                ),
            ));
        }
    };
    let Some(expected_kind) = expected_kind else {
        if !branch_components.is_empty() {
            return Err(invalid(
                format!("/transport_networks/{network_index}/component_ids"),
                format!("plain transport cell {cell} has an unnecessary branch component"),
            ));
        }
        return Ok(());
    };
    if branch_components.len() != 1 || branch_components[0].kind != expected_kind {
        return Err(invalid(
            format!("/transport_networks/{network_index}/component_ids"),
            format!("transport cell {cell} requires exactly one {expected_kind:?} component"),
        ));
    }
    let placed = branch_components[0];
    if placed.transport != network.transport {
        return Err(invalid(
            format!("/transport_networks/{network_index}/component_ids"),
            "branch component transport does not match its network",
        ));
    }
    let definition = components.component(&placed.component).ok_or_else(|| {
        invalid(
            "/logistics_components",
            "unknown branch component definition",
        )
    })?;
    let allowed_inputs = definition
        .input_directions
        .iter()
        .map(|direction| rotate_direction(*direction, placed.rotation))
        .collect::<BTreeSet<_>>();
    let allowed_outputs = definition
        .output_directions
        .iter()
        .map(|direction| rotate_direction(*direction, placed.rotation))
        .collect::<BTreeSet<_>>();
    if !incoming.is_subset(&allowed_inputs) || !outgoing.is_subset(&allowed_outputs) {
        return Err(invalid(
            format!("/transport_networks/{network_index}/component_ids"),
            format!("branch component at cell {cell} does not match channel directions"),
        ));
    }
    let capacity = Rate::from_quantity_per_duration_ms(
        definition.capacity.quantity,
        definition.capacity.duration_ms,
    )
    .map_err(|error| arithmetic_invalid(network_index, error.message))?;
    if flow > capacity {
        return Err(invalid(
            format!("/transport_networks/{network_index}/component_ids"),
            format!("branch component at cell {cell} exceeds catalog capacity"),
        ));
    }
    Ok(())
}

fn terminal_arm_direction(
    input: &ModelInput,
    placements: &ValidatedPlacements<'_>,
    endpoint: &TransportNetworkEndpoint,
) -> Result<CardinalDirection, IntegratedLayoutDiagnostic> {
    match endpoint {
        TransportNetworkEndpoint::External { side, .. } => Ok(edge_direction(*side)),
        TransportNetworkEndpoint::Facility { instance, port } => {
            let placement = placements
                .by_instance
                .get(instance.as_str())
                .ok_or_else(|| {
                    invalid(
                        "/transport_networks",
                        "terminal references an unplaced facility",
                    )
                })?;
            let definition = &input
                .instances
                .iter()
                .find(|candidate| candidate.id == *instance)
                .expect("validated placement instance exists")
                .definition;
            let port = definition
                .ports
                .iter()
                .find(|candidate| candidate.id == *port)
                .expect("validated terminal port exists");
            Ok(opposite(edge_direction(
                port.edge.rotated_clockwise(placement.rotation),
            )))
        }
    }
}

fn direction_between(cell: usize, neighbor: usize, width: i32) -> CardinalDirection {
    if neighbor + width as usize == cell {
        CardinalDirection::North
    } else if neighbor == cell + 1 {
        CardinalDirection::East
    } else if neighbor == cell + width as usize {
        CardinalDirection::South
    } else if neighbor + 1 == cell {
        CardinalDirection::West
    } else {
        panic!("validated segment cells are orthogonal neighbors")
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

fn opposite(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::South,
        CardinalDirection::East => CardinalDirection::West,
        CardinalDirection::South => CardinalDirection::North,
        CardinalDirection::West => CardinalDirection::East,
    }
}

fn rotate_direction(direction: CardinalDirection, rotation: i64) -> CardinalDirection {
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
fn validate_terminal_endpoint(
    input: &ModelInput,
    placements: &ValidatedPlacements<'_>,
    expected: &EndpointInput,
    actual: &TransportNetworkEndpoint,
    terminal_cell: usize,
    network_index: usize,
    terminal_index: usize,
) -> Result<(), IntegratedLayoutDiagnostic> {
    match (expected, actual) {
        (
            EndpointInput::Facility { instance, ports },
            TransportNetworkEndpoint::Facility {
                instance: actual_instance,
                port,
            },
        ) if instance == actual_instance && ports.iter().any(|candidate| candidate.id == *port) => {
            let placement = placements.by_instance[instance.as_str()];
            let definition = &input
                .instances
                .iter()
                .find(|candidate| candidate.id == *instance)
                .expect("prepared endpoint instance exists")
                .definition;
            let connections = candidate_port_connections(
                definition,
                placement.rotation,
                placement.x as i32,
                placement.y as i32,
                input.width,
                input.height,
            );
            if connections.get(port).copied() != Some(terminal_cell) {
                return Err(invalid(
                    format!("/transport_networks/{network_index}/terminals/{terminal_index}"),
                    "selected facility port does not connect to the terminal position",
                ));
            }
            Ok(())
        }
        (
            EndpointInput::External { node },
            TransportNetworkEndpoint::External {
                node: actual_node, ..
            },
        ) if node == actual_node => Ok(()),
        _ => Err(invalid(
            format!("/transport_networks/{network_index}/terminals/{terminal_index}/endpoint"),
            "transport terminal endpoint does not match the prepared endpoint",
        )),
    }
}

fn network_cell_shapes(
    cell: usize,
    neighbors: &BTreeMap<usize, BTreeSet<usize>>,
    width: i32,
) -> Vec<SegmentShape> {
    let Some(neighbors) = neighbors.get(&cell) else {
        return vec![SegmentShape::Other];
    };
    let x = cell % width as usize;
    let has_horizontal = neighbors
        .iter()
        .any(|neighbor| neighbor % width as usize != x);
    let has_vertical = neighbors
        .iter()
        .any(|neighbor| neighbor % width as usize == x);
    match (has_horizontal, has_vertical, neighbors.len()) {
        (true, false, _) => vec![SegmentShape::Horizontal],
        (false, true, _) => vec![SegmentShape::Vertical],
        (true, true, 4) => vec![SegmentShape::Horizontal, SegmentShape::Vertical],
        _ => vec![SegmentShape::Other],
    }
}

struct ValidatedPlacements<'a> {
    by_instance: BTreeMap<&'a str, &'a FacilityPlacement>,
    occupied: Vec<bool>,
}

fn validate_placements<'a>(
    input: &ModelInput,
    report: &'a IntegratedLayoutReport,
    cell_count: usize,
) -> Result<ValidatedPlacements<'a>, IntegratedLayoutDiagnostic> {
    let mut by_instance = BTreeMap::new();
    let mut occupied = vec![false; cell_count];
    for (index, placement) in report.placements.iter().enumerate() {
        let instance = input
            .instances
            .iter()
            .find(|instance| instance.id == placement.instance)
            .ok_or_else(|| {
                invalid(
                    format!("/placements/{index}/instance"),
                    format!("unknown facility instance '{}'", placement.instance),
                )
            })?;
        if by_instance
            .insert(placement.instance.as_str(), placement)
            .is_some()
        {
            return Err(invalid(
                format!("/placements/{index}/instance"),
                format!("duplicate facility instance '{}'", placement.instance),
            ));
        }
        if placement.recipe != instance.recipe
            || placement.facility != instance.facility
            || !instance
                .definition
                .allowed_rotations
                .contains(&placement.rotation)
        {
            return Err(invalid(
                format!("/placements/{index}"),
                "placement identity or rotation does not match its prepared facility instance",
            ));
        }
        let source_width = instance.definition.footprint.width;
        let source_height = instance.definition.footprint.height;
        let (width, height) = if matches!(placement.rotation, 90 | 270) {
            (source_height, source_width)
        } else {
            (source_width, source_height)
        };
        if placement.width != width
            || placement.height != height
            || placement.x < 0
            || placement.y < 0
            || placement.x + placement.width > i64::from(input.width)
            || placement.y + placement.height > i64::from(input.height)
        {
            return Err(invalid(
                format!("/placements/{index}"),
                "placement dimensions or origin violate its facility definition or hard bounds",
            ));
        }
        for y in placement.y..(placement.y + placement.height) {
            for x in placement.x..(placement.x + placement.width) {
                let cell = grid_index(x as i32, y as i32, input.width);
                if occupied[cell] {
                    return Err(invalid(
                        format!("/placements/{index}"),
                        "production-facility footprints overlap",
                    ));
                }
                occupied[cell] = true;
            }
        }
    }
    Ok(ValidatedPlacements {
        by_instance,
        occupied,
    })
}

fn validate_crossings(
    input: &ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    report: &IntegratedLayoutReport,
    placements: &ValidatedPlacements<'_>,
    layer_cells: &[Vec<Vec<SegmentShape>>; 2],
) -> Result<(), IntegratedLayoutDiagnostic> {
    let branch_cells = report
        .logistics_components
        .iter()
        .filter(|component| {
            matches!(
                component.kind,
                LogisticsComponentKind::Splitter | LogisticsComponentKind::Converger
            ) && component.position.x >= 0
                && component.position.y >= 0
                && component.position.x < i64::from(input.width)
                && component.position.y < i64::from(input.height)
        })
        .map(|component| {
            (
                component.transport,
                grid_index(
                    component.position.x as i32,
                    component.position.y as i32,
                    input.width,
                ),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut expected_bridges = BTreeSet::new();
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        for (cell, shapes) in layer_cells[layer_index(transport)].iter().enumerate() {
            match shapes.as_slice() {
                [] | [_] => {}
                [first, second]
                    if matches!(
                        (*first, *second),
                        (SegmentShape::Horizontal, SegmentShape::Vertical)
                            | (SegmentShape::Vertical, SegmentShape::Horizontal)
                    ) =>
                {
                    if !branch_cells.contains(&(transport, cell)) {
                        expected_bridges.insert((transport, cell));
                    }
                }
                _ => {
                    return Err(invalid(
                        "/transport_networks",
                        format!(
                            "same-layer cell {cell} has {} channels without a valid perpendicular bridge crossing",
                            shapes.len()
                        ),
                    ));
                }
            }
        }
    }

    let known_component_ids = report
        .logistics_components
        .iter()
        .map(|component| component.id.as_str())
        .collect::<BTreeSet<_>>();
    for (network_index, network) in report.transport_networks.iter().enumerate() {
        if network
            .component_ids
            .iter()
            .any(|id| !known_component_ids.contains(id.as_str()))
        {
            return Err(invalid(
                format!("/transport_networks/{network_index}/component_ids"),
                "transport network references an unknown logistics component",
            ));
        }
    }

    let mut actual_bridges = BTreeSet::new();
    let mut occupied_components = BTreeSet::new();
    for (index, component) in report.logistics_components.iter().enumerate() {
        let definition = components
            .component_by_kind(component.transport, component.kind)
            .expect("validated catalog has every logistics component capability");
        if component.component != definition.id
            || !definition.allowed_rotations.contains(&component.rotation)
            || component.position.x < 0
            || component.position.y < 0
            || component.position.x >= i64::from(input.width)
            || component.position.y >= i64::from(input.height)
        {
            return Err(invalid(
                format!("/logistics_components/{index}"),
                "placed logistics component does not match its catalog definition or hard bounds",
            ));
        }
        let cell = grid_index(
            component.position.x as i32,
            component.position.y as i32,
            input.width,
        );
        if placements.occupied[cell] || !occupied_components.insert((component.transport, cell)) {
            return Err(invalid(
                format!("/logistics_components/{index}"),
                "placed logistics component overlaps a facility or another same-layer component",
            ));
        }
        let owners = report
            .transport_networks
            .iter()
            .filter(|network| network.component_ids.contains(&component.id))
            .collect::<Vec<_>>();
        if owners.is_empty()
            || owners.iter().any(|network| {
                network.transport != component.transport
                    || !network.cells.contains(&component.position)
            })
        {
            return Err(invalid(
                format!("/logistics_components/{index}"),
                "placed logistics component is not owned by a matching network at its cell",
            ));
        }
        match component.kind {
            LogisticsComponentKind::Bridge => {
                if !(1..=2).contains(&owners.len()) {
                    return Err(invalid(
                        format!("/logistics_components/{index}"),
                        "bridge must carry two perpendicular channels owned by one or two transport networks",
                    ));
                }
                actual_bridges.insert((component.transport, cell));
            }
            LogisticsComponentKind::Splitter | LogisticsComponentKind::Converger => {
                if owners.len() != 1 {
                    return Err(invalid(
                        format!("/logistics_components/{index}"),
                        "splitter or converger must belong to exactly one transport network",
                    ));
                }
            }
        }
    }
    if actual_bridges != expected_bridges {
        return Err(invalid(
            "/logistics_components",
            "placed logistics bridges do not exactly cover same-layer perpendicular crossings",
        ));
    }
    Ok(())
}

fn used_geometry_bounds(report: &IntegratedLayoutReport) -> (i64, i64, i64, i64) {
    let mut minimum_x = i64::MAX;
    let mut minimum_y = i64::MAX;
    let mut maximum_x = i64::MIN;
    let mut maximum_y = i64::MIN;
    for placement in &report.placements {
        minimum_x = minimum_x.min(placement.x);
        minimum_y = minimum_y.min(placement.y);
        maximum_x = maximum_x.max(placement.x + placement.width - 1);
        maximum_y = maximum_y.max(placement.y + placement.height - 1);
    }
    for position in report
        .transport_networks
        .iter()
        .flat_map(|network| network.cells.iter())
        .chain(
            report
                .logistics_components
                .iter()
                .map(|component| &component.position),
        )
    {
        minimum_x = minimum_x.min(position.x);
        minimum_y = minimum_y.min(position.y);
        maximum_x = maximum_x.max(position.x);
        maximum_y = maximum_y.max(position.y);
    }
    if minimum_x == i64::MAX {
        (0, 0, 0, 0)
    } else {
        (
            minimum_x,
            minimum_y,
            maximum_x - minimum_x + 1,
            maximum_y - minimum_y + 1,
        )
    }
}

fn checked_cell(
    input: &ModelInput,
    position: &WorldGridPosition,
    path: &str,
) -> Result<usize, IntegratedLayoutDiagnostic> {
    if position.x < 0
        || position.y < 0
        || position.x >= i64::from(input.width)
        || position.y >= i64::from(input.height)
    {
        return Err(invalid(
            path,
            "transport geometry is outside the hard layout bounds",
        ));
    }
    Ok(grid_index(
        position.x as i32,
        position.y as i32,
        input.width,
    ))
}

fn add_rate(
    rates: &mut BTreeMap<usize, Rate>,
    cell: usize,
    rate: Rate,
    network_index: usize,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let total = rates.entry(cell).or_insert(Rate::zero());
    *total = total
        .checked_add(rate)
        .map_err(|error| arithmetic_invalid(network_index, error.message))?;
    Ok(())
}

fn add_direction_rate(
    rates: &mut BTreeMap<usize, BTreeMap<CardinalDirection, Rate>>,
    cell: usize,
    direction: CardinalDirection,
    rate: Rate,
    network_index: usize,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let total = rates
        .entry(cell)
        .or_default()
        .entry(direction)
        .or_insert(Rate::zero());
    *total = total
        .checked_add(rate)
        .map_err(|error| arithmetic_invalid(network_index, error.message))?;
    Ok(())
}

fn rate_at(rates: &BTreeMap<usize, Rate>, cell: usize) -> Rate {
    rates.get(&cell).copied().unwrap_or_else(Rate::zero)
}

fn endpoint_node(endpoint: &EndpointInput) -> &str {
    match endpoint {
        EndpointInput::Facility { instance, .. } => instance,
        EndpointInput::External { node } => node,
    }
}

fn arithmetic_invalid(network_index: usize, message: String) -> IntegratedLayoutDiagnostic {
    invalid(
        format!("/transport_networks/{network_index}"),
        format!("transport network rate arithmetic failed: {message}"),
    )
}

fn layer_index(transport: TransportKind) -> usize {
    match transport {
        TransportKind::Belt => 0,
        TransportKind::Pipe => 1,
    }
}

fn invalid(path: impl Into<String>, message: impl Into<String>) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error("invalid-integrated-layout-witness", path, None, message)
}
