use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::facilities::FacilityPortDirection;
use crate::layouts::FacilityPlacement;
use crate::logistics::{LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog};
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
    let mut forward = BTreeMap::<usize, BTreeSet<usize>>::new();
    let mut reverse = BTreeMap::<usize, BTreeSet<usize>>::new();
    let mut incident_neighbors = BTreeMap::<usize, BTreeSet<usize>>::new();
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
        forward.entry(from).or_default().insert(to);
        reverse.entry(to).or_default().insert(from);
        incident_neighbors.entry(from).or_default().insert(to);
        incident_neighbors.entry(to).or_default().insert(from);
    }

    let expected_terminals = expected_terminals(input, expected.route_indices(), network_index)?;
    let mut actual_terminal_rates = BTreeMap::<(String, bool), Rate>::new();
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
        let key = (
            terminal.node.clone(),
            terminal.direction == FacilityPortDirection::Output,
        );
        let (expected_endpoint, _) = expected_terminals.get(&key).ok_or_else(|| {
            invalid(
                format!("/transport_networks/{network_index}/terminals/{terminal_index}"),
                "transport terminal does not match a prepared supply or demand",
            )
        })?;
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
            expected_endpoint,
            &terminal.endpoint,
            cell,
            network_index,
            terminal_index,
        )?;
        add_named_rate(
            &mut actual_terminal_rates,
            key,
            terminal.rate,
            network_index,
        )?;
        if terminal.direction == FacilityPortDirection::Output {
            add_rate(&mut supply, cell, terminal.rate, network_index)?;
        } else {
            add_rate(&mut demand, cell, terminal.rate, network_index)?;
        }
    }
    let expected_rates = expected_terminals
        .iter()
        .map(|(key, (_, rate))| (key.clone(), *rate))
        .collect::<BTreeMap<_, _>>();
    if actual_terminal_rates != expected_rates {
        return Err(invalid(
            format!("/transport_networks/{network_index}/terminals"),
            "transport terminal rates do not exactly match prepared supply and demand",
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
        let incoming_arms =
            reverse.get(cell).map_or(0, BTreeSet::len) + usize::from(supply.contains_key(cell));
        let outgoing_arms =
            forward.get(cell).map_or(0, BTreeSet::len) + usize::from(demand.contains_key(cell));
        if incoming_arms > 1 || outgoing_arms > 1 {
            return Err(invalid(
                format!("/transport_networks/{network_index}/cells"),
                format!(
                    "plain transport cell {cell} splits or converges without a modeled logistics component"
                ),
            ));
        }
    }
    validate_reachability(network_index, &cells, &supply, &demand, &forward, &reverse)?;

    let layer = layer_index(network.transport);
    for cell in &cells {
        for shape in network_cell_shapes(*cell, &incident_neighbors, input.width) {
            layer_cells[layer][*cell].push(shape);
        }
    }
    Ok(())
}

fn expected_terminals(
    input: &ModelInput,
    route_indices: &[usize],
    network_index: usize,
) -> Result<BTreeMap<(String, bool), (EndpointInput, Rate)>, IntegratedLayoutDiagnostic> {
    let mut terminals = BTreeMap::new();
    for route_index in route_indices {
        let edge = &input.edges[*route_index];
        merge_expected_terminal(
            &mut terminals,
            endpoint_node(&edge.source),
            true,
            &edge.source,
            edge.edge.rate,
            network_index,
        )?;
        merge_expected_terminal(
            &mut terminals,
            endpoint_node(&edge.target),
            false,
            &edge.target,
            edge.edge.rate,
            network_index,
        )?;
    }
    Ok(terminals)
}

fn merge_expected_terminal(
    terminals: &mut BTreeMap<(String, bool), (EndpointInput, Rate)>,
    node: &str,
    is_output: bool,
    endpoint: &EndpointInput,
    rate: Rate,
    network_index: usize,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let (_, total) = terminals
        .entry((node.to_string(), is_output))
        .or_insert_with(|| (endpoint.clone(), Rate::zero()));
    *total = total
        .checked_add(rate)
        .map_err(|error| arithmetic_invalid(network_index, error.message))?;
    Ok(())
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

fn validate_reachability(
    network_index: usize,
    cells: &BTreeSet<usize>,
    supply: &BTreeMap<usize, Rate>,
    demand: &BTreeMap<usize, Rate>,
    forward: &BTreeMap<usize, BTreeSet<usize>>,
    reverse: &BTreeMap<usize, BTreeSet<usize>>,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let from_supply = reachable(supply.keys().copied(), forward);
    let to_demand = reachable(demand.keys().copied(), reverse);
    if !cells.is_subset(&from_supply) || !cells.is_subset(&to_demand) {
        return Err(invalid(
            format!("/transport_networks/{network_index}/cells"),
            "every active transport cell must lie on a directed supply-to-demand flow path",
        ));
    }
    Ok(())
}

fn reachable(
    starts: impl IntoIterator<Item = usize>,
    adjacency: &BTreeMap<usize, BTreeSet<usize>>,
) -> BTreeSet<usize> {
    let mut reached = BTreeSet::new();
    let mut queue = VecDeque::new();
    for start in starts {
        if reached.insert(start) {
            queue.push_back(start);
        }
    }
    while let Some(cell) = queue.pop_front() {
        for next in adjacency.get(&cell).into_iter().flatten() {
            if reached.insert(*next) {
                queue.push_back(*next);
            }
        }
    }
    reached
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
                    expected_bridges.insert((transport, cell));
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
    for (index, component) in report.logistics_components.iter().enumerate() {
        if component.kind != LogisticsComponentKind::Bridge {
            return Err(invalid(
                format!("/logistics_components/{index}/kind"),
                "exact baseline witness currently emits only bridge logistics components",
            ));
        }
        let definition = components
            .component_by_kind(component.transport, component.kind)
            .expect("validated catalog has every bridge capability");
        if component.component != definition.id
            || !definition.allowed_rotations.contains(&component.rotation)
            || component.position.x < 0
            || component.position.y < 0
            || component.position.x >= i64::from(input.width)
            || component.position.y >= i64::from(input.height)
        {
            return Err(invalid(
                format!("/logistics_components/{index}"),
                "placed logistics bridge does not match its catalog definition or hard bounds",
            ));
        }
        let cell = grid_index(
            component.position.x as i32,
            component.position.y as i32,
            input.width,
        );
        if placements.occupied[cell] || !actual_bridges.insert((component.transport, cell)) {
            return Err(invalid(
                format!("/logistics_components/{index}"),
                "placed logistics bridge overlaps a facility or duplicates another bridge",
            ));
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

fn add_named_rate(
    rates: &mut BTreeMap<(String, bool), Rate>,
    key: (String, bool),
    rate: Rate,
    network_index: usize,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let total = rates.entry(key).or_insert(Rate::zero());
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
