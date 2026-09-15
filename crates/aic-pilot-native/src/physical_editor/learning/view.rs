//! Physical graph authority. Names are lookup keys only; equality is represented by edges.
use super::*;
use aic_layout::{
    editor::{CompiledEdge, CompiledProblemV1, CompiledTerminal, PhysicalDraftV1},
    facilities::FacilityPortDirection,
    logistics::{CardinalDirection, LogisticsComponentKind, TransportCapacity, TransportKind},
    rate::Rate,
};
use schema::{edge as e, node as n, state as st};

pub(super) struct Tables {
    pub facilities: Vec<usize>,
    pub nodes: Vec<usize>,
    components: Vec<usize>,
    ports: Vec<BTreeMap<String, usize>>,
    networks: Vec<usize>,
    fixed: Input,
    pub capacities: [f32; 2],
}
#[derive(Clone)]
pub(super) struct PhysicalView {
    pub input: Arc<Input>,
    pub frame: frame::PhysicalFrame,
    pub conflict_area: f64,
    pub attempts: u64,
    pub route_work: usize,
}
pub(super) fn transport(kind: TransportKind) -> usize {
    usize::from(kind == TransportKind::Pipe)
}
fn rate(value: Rate) -> f32 {
    (value.numerator as f64 / value.denominator as f64).ln_1p() as f32
}
fn capacity(value: &TransportCapacity) -> Result<Rate> {
    Rate::from_quantity_per_duration_ms(value.quantity, value.duration_ms)
        .map_err(|d| error("learning-capacity", "catalog.capacity", d.message))
}
fn cardinal(d: CardinalDirection) -> usize {
    match d {
        CardinalDirection::North => 0,
        CardinalDirection::East => 1,
        CardinalDirection::South => 2,
        CardinalDirection::West => 3,
    }
}
fn node(value: &mut Input, kind: usize) -> usize {
    let id = value.node_static.len();
    let mut row = [0.; NODE_STATIC];
    row[kind] = 1.;
    value.node_static.push(row);
    value.node_state.push([0.; NODE_STATE]);
    id
}
fn relation(value: &mut Input, source: usize, target: usize, kind: usize) {
    value.edge_index.push([source as i64, target as i64]);
    let mut row = [0.; EDGE_STATIC];
    row[kind] = 1.;
    value.edge_static.push(row);
    value.edge_state.push([0.; EDGE_STATE]);
}
fn rotations(row: &mut [f32; NODE_STATIC], values: &[i64]) {
    for &rotation in values {
        row[n::ROTATIONS + (rotation / 90) as usize] = 1.;
    }
}
impl Tables {
    pub fn new(problem: &CompiledProblemV1, facilities: &[Facility]) -> Result<Self> {
        let exact_capacities = [TransportKind::Belt, TransportKind::Pipe]
            .map(|kind| {
                problem
                    .used_catalog
                    .transports
                    .iter()
                    .find(|definition| definition.kind == kind)
                    .ok_or_else(|| {
                        error(
                            "learning-catalog",
                            format!("{kind:?}"),
                            "Missing transport definition",
                        )
                    })
                    .and_then(|definition| capacity(&definition.capacity))
            })
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let capacities = [rate(exact_capacities[0]), rate(exact_capacities[1])];
        let mut fixed = Input::default();
        let mut facility_map = vec![];
        for f in &problem.blocks {
            let id = node(&mut fixed, n::FACILITY);
            facility_map.push(id);
            let d = super::facility(problem, &f.facility);
            let row = &mut fixed.node_static[id];
            row[n::WIDTH] = d.footprint.width as f32;
            row[n::HEIGHT] = d.footprint.height as f32;
            rotations(row, &d.allowed_rotations);
        }
        let mut nodes = vec![];
        let mut network_transport = vec![];
        for network in &problem.networks {
            network_transport.push(transport(network.transport));
        }
        for original in &problem.nodes {
            let kind = match original.terminal.as_ref() {
                Some(CompiledTerminal::Facility { .. }) => n::TERMINAL,
                Some(CompiledTerminal::External { .. }) => n::EXTERNAL,
                None => n::CHANNEL,
            };
            let id = node(&mut fixed, kind);
            nodes.push(id);
            let row = &mut fixed.node_static[id];
            row[n::TRANSPORT + network_transport[original.network]] = 1.;
            if let Some(t) = &original.terminal {
                let (direction, demand) = match t {
                    CompiledTerminal::Facility {
                        direction, rate, ..
                    }
                    | CompiledTerminal::External {
                        direction, rate, ..
                    } => (*direction, *rate),
                };
                row[n::DIRECTION + usize::from(direction == FacilityPortDirection::Output)] = 1.;
                row[n::DEMAND] = rate(demand);
            }
        }
        let mut components = Vec::with_capacity(problem.components.len());
        let mut component_capacities = Vec::with_capacity(problem.components.len());
        for c in &problem.components {
            let id = node(&mut fixed, n::COMPONENT);
            components.push(id);
            let d = super::component(problem, &c.definition);
            let exact = capacity(&d.capacity)?;
            component_capacities.push(exact);
            let row = &mut fixed.node_static[id];
            row[n::WIDTH] = d.footprint.width as f32;
            row[n::HEIGHT] = d.footprint.height as f32;
            rotations(row, &d.allowed_rotations);
            row[n::TRANSPORT + transport(d.transport)] = 1.;
            row[n::CAPACITY] = rate(exact);
            row[n::COMPONENT_KIND
                + match d.kind {
                    LogisticsComponentKind::Splitter => 0,
                    LogisticsComponentKind::Converger => 1,
                    LogisticsComponentKind::Bridge => 2,
                }] = 1.;
            for (offset, directions) in [
                (n::INPUT_MASK, &d.input_directions),
                (n::OUTPUT_MASK, &d.output_directions),
            ] {
                for &direction in directions {
                    row[offset + (cardinal(direction) + (c.rotation / 90) as usize) % 4] = 1.;
                }
            }
        }
        for original in &problem.nodes {
            let mut cap = exact_capacities[network_transport[original.network]];
            for &component in &original.components {
                let other = component_capacities.get(component).ok_or_else(|| {
                    error(
                        "learning-catalog",
                        component.to_string(),
                        "Missing endpoint component",
                    )
                })?;
                if i128::from(other.numerator) * i128::from(cap.denominator)
                    < i128::from(cap.numerator) * i128::from(other.denominator)
                {
                    cap = *other;
                }
            }
            fixed.node_static[nodes[original.id]][n::CAPACITY] = rate(cap);
        }
        let mut ports = vec![];
        for f in facilities {
            let mut table = BTreeMap::new();
            // Catalog order is retained independently of spelling or current use.
            for p in &f.ports[&f.rotations[0]] {
                let id = node(&mut fixed, n::PORT);
                table.insert(p.id.clone(), id);
                let row = &mut fixed.node_static[id];
                row[n::TRANSPORT + transport(p.transport)] = 1.;
                row[n::DIRECTION + usize::from(p.direction == FacilityPortDirection::Output)] = 1.;
                row[n::CAPACITY] = capacities[transport(p.transport)];
            }
            ports.push(table);
        }
        let mut networks = vec![];
        for _ in &problem.networks {
            networks.push(node(&mut fixed, n::NETWORK));
        }
        let mut commodities = BTreeMap::<String, usize>::new();
        for network in &problem.networks {
            if !commodities.contains_key(&network.commodity) {
                commodities.insert(network.commodity.clone(), node(&mut fixed, n::COMMODITY));
            }
        }
        // Transport edges retain native edge indices for path ownership.
        for edge in &problem.edges {
            relation(
                &mut fixed,
                nodes[edge.source],
                nodes[edge.target],
                e::TRANSPORT,
            );
            let row = &mut fixed.edge_static[edge.id];
            let medium = network_transport[edge.network];
            row[e::MEDIUM + medium] = 1.;
            row[e::RATE] = rate(edge.rate);
            let mut cap = exact_capacities[medium];
            for &component in [edge.source, edge.target]
                .iter()
                .flat_map(|&id| &problem.nodes[id].components)
            {
                let other = component_capacities.get(component).ok_or_else(|| {
                    error(
                        "learning-catalog",
                        component.to_string(),
                        "Missing endpoint component",
                    )
                })?;
                if i128::from(other.numerator) * i128::from(cap.denominator)
                    < i128::from(cap.numerator) * i128::from(other.denominator)
                {
                    cap = *other;
                }
            }
            row[e::CAPACITY] = rate(cap);
            row[e::ZERO_LENGTH] = f32::from(edge.zero_length);
        }
        for (owner, table) in ports.iter().enumerate() {
            for &id in table.values() {
                relation(&mut fixed, facility_map[owner], id, e::FACILITY_PORT);
            }
        }
        for original in &problem.nodes {
            for &component in &original.components {
                relation(
                    &mut fixed,
                    components[component],
                    nodes[original.id],
                    e::COMPONENT_CHANNEL,
                );
            }
            relation(
                &mut fixed,
                nodes[original.id],
                networks[original.network],
                e::NETWORK_MEMBER,
            );
        }
        for (i, network) in problem.networks.iter().enumerate() {
            relation(
                &mut fixed,
                networks[i],
                commodities[&network.commodity],
                e::COMMODITY_MEMBER,
            );
        }
        Ok(Self {
            facilities: facility_map,
            nodes,
            components,
            ports,
            networks,
            fixed,
            capacities,
        })
    }
}
pub(super) fn path_rows(
    path: &[Point],
    frame: frame::PhysicalFrame,
) -> impl Iterator<Item = [f32; PATH_FEATURES]> + '_ {
    path.iter().enumerate().map(move |(i, p)| {
        let incoming = i.checked_sub(1).map_or(Point::default(), |j| Point {
            x: p.x - path[j].x,
            y: p.y - path[j].y,
        });
        let outgoing = path.get(i + 1).map_or(Point::default(), |q| Point {
            x: q.x - p.x,
            y: q.y - p.y,
        });
        floats([
            (i128::from(p.x) - frame.origin[0]) as f64 / frame.scale(),
            (i128::from(p.y) - frame.origin[1]) as f64 / frame.scale(),
            incoming.x as f64,
            incoming.y as f64,
            outgoing.x as f64,
            outgoing.y as f64,
            f64::from(i == 0),
            f64::from(i + 1 == path.len()),
        ])
    })
}
impl Learning {
    pub(super) fn physical(
        &self,
        problem: &CompiledProblemV1,
        draft: &PhysicalDraftV1,
        positions: &[Point],
        frame: frame::PhysicalFrame,
        collision_counts: &[f32],
        conflicts: usize,
    ) -> Result<Input> {
        let scale = frame.scale() as f32;
        let mut value = self.tables.fixed.clone();
        value.conflicts = conflicts;
        value.pending = draft.paths.iter().filter(|p| p.is_none()).count();
        for row in &mut value.node_static {
            row[n::WIDTH] /= scale;
            row[n::HEIGHT] /= scale;
        }
        let position = |row: &mut [f32; NODE_STATE], p: Point| {
            row[st::X] = (i128::from(p.x) - frame.origin[0]) as f32 / scale;
            row[st::Y] = (i128::from(p.y) - frame.origin[1]) as f32 / scale;
        };
        let pose = |row: &mut [f32; NODE_STATE], w: i64, h: i64, rotation| {
            row[st::WIDTH] = w as f32 / scale;
            row[st::HEIGHT] = h as f32 / scale;
            let (sin, cos) = angle(rotation);
            row[st::SIN] = sin as f32;
            row[st::COS] = cos as f32;
        };
        for (owner, pose_value) in draft.poses.iter().enumerate() {
            let definition = super::facility(problem, &problem.blocks[owner].facility);
            let (width, height) = if pose_value.rotation % 180 == 0 {
                (definition.footprint.width, definition.footprint.height)
            } else {
                (definition.footprint.height, definition.footprint.width)
            };
            let row = &mut value.node_state[self.tables.facilities[owner]];
            position(row, pose_value.position);
            pose(row, width, height, pose_value.rotation);
            row[st::CONFLICTS] = collision_counts[owner];
            for p in &self.facilities[owner].ports[&pose_value.rotation] {
                let row = &mut value.node_state[self.tables.ports[owner][&p.id]];
                position(
                    row,
                    p.connection
                        .shifted(pose_value.position.x, pose_value.position.y),
                );
                row[st::NORMAL_X] = (p.connection.x - p.position.x) as f32;
                row[st::NORMAL_Y] = (p.connection.y - p.position.y) as f32;
            }
        }
        for (index, (component, point)) in problem
            .components
            .iter()
            .zip(&draft.component_positions)
            .enumerate()
        {
            let id = self.tables.components[index];
            let d = super::component(problem, &component.definition);
            let (w, h) = if component.rotation % 180 == 0 {
                (d.footprint.width, d.footprint.height)
            } else {
                (d.footprint.height, d.footprint.width)
            };
            position(&mut value.node_state[id], *point);
            pose(&mut value.node_state[id], w, h, component.rotation);
        }
        for original in &problem.nodes {
            let id = self.tables.nodes[original.id];
            position(&mut value.node_state[id], positions[original.id]);
            if let Some(CompiledTerminal::External {
                side: Some(side), ..
            }) = &original.terminal
            {
                let [x, y] = [[0., -1.], [1., 0.], [0., 1.], [-1., 0.]]
                    [aic_layout::projection::edge_index(*side)];
                value.node_state[id][st::NORMAL_X] = x;
                value.node_state[id][st::NORMAL_Y] = y;
            }
            if let Some(CompiledTerminal::Facility {
                block: owner,
                original_port,
                ..
            }) = &original.terminal
            {
                let selected = draft.selected_ports[original.id].as_ref().ok_or_else(|| {
                    error(
                        "learning-port",
                        original.id.to_string(),
                        "Missing selected port",
                    )
                })?;
                let selected_id = self.tables.ports[*owner][selected];
                let normal = value.node_state[selected_id];
                let row = &mut value.node_state[id];
                row[st::NORMAL_X] = normal[st::NORMAL_X];
                row[st::NORMAL_Y] = normal[st::NORMAL_Y];
                row[st::SELECTED] = 1.;
                value.node_state[selected_id][st::SELECTED] = 1.;
                relation(
                    &mut value,
                    self.tables.facilities[*owner],
                    id,
                    e::FACILITY_TERMINAL,
                );
                relation(&mut value, id, selected_id, e::SELECTED_PORT);
                relation(
                    &mut value,
                    id,
                    self.tables.ports[*owner][original_port],
                    e::ORIGINAL_PORT,
                );
            }
        }
        for edge in &problem.edges {
            let (current, baseline) = self.endpoint_rules(problem, draft, edge)?;
            let (first, last) = crate::physical_editor::query_directions(
                edge.zero_length,
                positions[edge.source],
                positions[edge.target],
                (current[0], current[1]),
            );
            let row = &mut value.edge_static[edge.id];
            for (offset, heading) in [
                (e::CURRENT_FIRST, first),
                (e::CURRENT_LAST, last),
                (e::BASELINE_FIRST, baseline[0]),
                (e::BASELINE_LAST, baseline[1]),
            ] {
                row[offset + heading.unwrap_or(4)] = 1.;
            }
            let path = &draft.paths[edge.id];
            let (a, b) = (positions[edge.source], positions[edge.target]);
            value.edge_state[edge.id] = [
                f32::from(path.is_none()),
                path.as_ref()
                    .map_or(0., |p| p.len().saturating_sub(1) as f32 / scale),
                (b.x - a.x) as f32 / scale,
                (b.y - a.y) as f32 / scale,
            ];
            if let Some(path) = path {
                value.path_cells.extend(path_rows(path, frame));
                value
                    .path_edges
                    .extend(std::iter::repeat_n(edge.id as i64, path.len()));
            }
        }
        debug_assert!(
            self.tables
                .networks
                .iter()
                .all(|&id| value.node_state[id] == [0.; NODE_STATE])
        );
        Ok(value)
    }
    fn endpoint_rules(
        &self,
        problem: &CompiledProblemV1,
        draft: &PhysicalDraftV1,
        edge: &CompiledEdge,
    ) -> Result<([Option<usize>; 2], [Option<usize>; 2])> {
        let mut current = [None; 2];
        let mut baseline = [None; 2];
        for (side, (id, recorded)) in [(edge.source, edge.first), (edge.target, edge.last)]
            .into_iter()
            .enumerate()
        {
            let mut facility = None;
            let mut reassigned = false;
            if let Some(CompiledTerminal::Facility {
                block: owner,
                original_port,
                ..
            }) = &problem.nodes[id].terminal
            {
                let placement = draft.poses[*owner];
                let selected = draft.selected_ports[id].as_ref().ok_or_else(|| {
                    error("learning-port", id.to_string(), "Missing selected port")
                })?;
                let p = self.facilities[*owner].ports[&placement.rotation]
                    .iter()
                    .find(|p| &p.id == selected)
                    .ok_or_else(|| {
                        error("learning-port", id.to_string(), "Missing catalog port")
                    })?;
                facility = Some((
                    problem.blocks[*owner].baseline_rotation,
                    placement.rotation,
                    aic_layout::projection::edge_index(p.edge),
                ));
                reassigned = selected != original_port;
            }
            (current[side], baseline[side]) =
                crate::physical_editor::endpoint_rule(recorded, facility, side, reassigned);
        }
        Ok((current, baseline))
    }
}
