//! The single source-independent physical mutation and routing engine.

use std::collections::{BTreeMap, BTreeSet};

use aic_layout::{
    editor::{
        CompiledNode, CompiledProblemV1, CompiledTerminal, PhysicalDraftV1, occupied_extents,
        validate_compiled,
    },
    geometry::{Bounds, Point, direction},
    projection::{edge_index, rotate},
};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::{Result, compiled_error, error};

const GUARD: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    fn get(self, point: Point) -> i64 {
        match self {
            Self::X => point.x,
            Self::Y => point.y,
        }
    }

    fn set(self, point: &mut Point, value: i64) {
        match self {
            Self::X => point.x = value,
            Self::Y => point.y = value,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PortSelection {
    pub node: usize,
    pub port: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PortDomain {
    pub node: usize,
    pub ports: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhysicalEdit {
    Place {
        block: String,
        position: Point,
        rotation: i64,
        ports: Vec<PortSelection>,
    },
    InsertBand {
        axis: Axis,
        at: i64,
        count: i64,
    },
    DeleteBand {
        axis: Axis,
        at: i64,
        count: i64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalEditLimits {
    pub route_expansions: usize,
    pub search_padding: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct EditWork {
    pub expanded: usize,
    pub preflight_expanded: usize,
    pub route_calls: usize,
    pub pending: usize,
    pub routes: Vec<RouteWork>,
    pub committed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RouteWork {
    pub edge: usize,
    pub status: String,
    pub expanded: usize,
    pub preflight_expanded: usize,
    pub preflight_status: String,
}

fn facility<'a>(
    problem: &'a CompiledProblemV1,
    id: &str,
) -> &'a aic_layout::facilities::FacilityDefinition {
    problem
        .used_catalog
        .facilities
        .iter()
        .find(|definition| definition.id == id)
        .expect("validated compiled facility")
}

fn component_definition<'a>(
    problem: &'a CompiledProblemV1,
    id: &str,
) -> &'a aic_layout::logistics::LogisticsComponentDefinition {
    problem
        .used_catalog
        .components
        .iter()
        .find(|definition| definition.id == id)
        .expect("validated compiled component")
}

fn facility_nodes(problem: &CompiledProblemV1, block: usize) -> Vec<usize> {
    problem
        .nodes
        .iter()
        .filter_map(|node| match node.terminal {
            Some(CompiledTerminal::Facility { block: owner, .. }) if owner == block => {
                Some(node.id)
            }
            _ => None,
        })
        .collect()
}

pub fn block_port_domains(problem: &CompiledProblemV1, block: usize) -> Result<Vec<PortDomain>> {
    let owner = problem.blocks.get(block).ok_or_else(|| {
        error(
            "block",
            block.to_string(),
            "port domain references an unknown block",
        )
    })?;
    let definition = facility(problem, &owner.facility);
    facility_nodes(problem, block)
        .into_iter()
        .map(|node_id| {
            let node = &problem.nodes[node_id];
            let Some(CompiledTerminal::Facility { direction, .. }) = node.terminal else {
                unreachable!("facility_nodes filters terminal kind")
            };
            let ports = definition
                .ports
                .iter()
                .filter(|port| {
                    port.direction == direction
                        && port.transport == problem.networks[node.network].transport
                })
                .map(|port| port.id.clone())
                .collect();
            Ok(PortDomain {
                node: node_id,
                ports,
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalPortKey {
    physical: [i128; 5],
    original_membership: Vec<bool>,
}

fn baseline_block_origin(problem: &CompiledProblemV1, block: usize) -> Result<[i128; 2]> {
    let owner = &problem.blocks[block];
    let definition = facility(problem, &owner.facility);
    let node = facility_nodes(problem, block)
        .into_iter()
        .next()
        .ok_or_else(|| {
            error(
                "port-domain",
                &owner.id,
                "block has no terminal to anchor ordering",
            )
        })?;
    let CompiledTerminal::Facility { original_port, .. } =
        &problem.nodes[node].terminal.as_ref().unwrap()
    else {
        unreachable!()
    };
    let projected = rotate(definition, owner.baseline_rotation, Point::default())
        .2
        .into_iter()
        .find(|port| port.id == *original_port)
        .expect("validated original port");
    Ok([
        i128::from(problem.nodes[node].baseline_position.x) - i128::from(projected.connection.x),
        i128::from(problem.nodes[node].baseline_position.y) - i128::from(projected.connection.y),
    ])
}

/// Canonical terminal and port order shared by every source adapter.
pub fn canonical_block_port_domains(
    problem: &CompiledProblemV1,
    block: usize,
) -> Result<Vec<PortDomain>> {
    let owner = problem.blocks.get(block).ok_or_else(|| {
        error(
            "block",
            block.to_string(),
            "port domain references an unknown block",
        )
    })?;
    let definition = facility(problem, &owner.facility);
    let base_ports = rotate(definition, 0, Point::default()).2;
    let base_by_id: BTreeMap<_, _> = base_ports
        .iter()
        .map(|port| (port.id.as_str(), port))
        .collect();
    let mut domains = block_port_domains(problem, block)?;
    if domains.is_empty() {
        return Ok(domains);
    }
    let origin = baseline_block_origin(problem, block)?;
    let original = |node: usize| match &problem.nodes[node].terminal {
        Some(CompiledTerminal::Facility { original_port, .. }) => original_port.as_str(),
        _ => unreachable!(),
    };
    let mut terminal_keys = BTreeMap::new();
    for domain in &domains {
        let node = &problem.nodes[domain.node];
        let Some(CompiledTerminal::Facility { rate, .. }) = &node.terminal else {
            unreachable!()
        };
        let port = base_by_id[original(domain.node)];
        let mut key = vec![
            i128::from(port.position.x),
            i128::from(port.position.y),
            edge_index(port.edge) as i128,
            i128::from(port.direction == aic_layout::facilities::FacilityPortDirection::Output),
            i128::from(port.transport == aic_layout::logistics::TransportKind::Pipe),
            i128::from(rate.numerator),
            i128::from(rate.denominator),
        ];
        let mut incidence = Vec::new();
        for edge in &problem.edges {
            let side = if edge.source == domain.node {
                Some(0usize)
            } else if edge.target == domain.node {
                Some(1usize)
            } else {
                None
            };
            let Some(side) = side else { continue };
            let mut row = vec![
                side as i128,
                i128::from(edge.rate.numerator),
                i128::from(edge.rate.denominator),
                i128::from(edge.zero_length),
            ];
            for (side, (endpoint, heading)) in [edge.source, edge.target]
                .into_iter()
                .zip([edge.first, edge.last])
                .enumerate()
            {
                let endpoint = &problem.nodes[endpoint];
                let heading = heading.map_or_else(
                    || {
                        if matches!(endpoint.terminal, Some(CompiledTerminal::Facility { .. })) {
                            (side * 2) as i128
                        } else {
                            -1
                        }
                    },
                    |value| value as i128,
                );
                row.extend([
                    i128::from(endpoint.baseline_position.x) - origin[0],
                    i128::from(endpoint.baseline_position.y) - origin[1],
                    heading,
                ]);
            }
            incidence.push(row);
        }
        incidence.sort();
        for row in incidence {
            key.push(row.len() as i128);
            key.extend(row);
        }
        terminal_keys.insert(domain.node, key);
    }
    domains.sort_by(|left, right| terminal_keys[&left.node].cmp(&terminal_keys[&right.node]));
    let port_keys: BTreeMap<_, _> = base_ports
        .iter()
        .map(|port| {
            (
                port.id.as_str(),
                CanonicalPortKey {
                    physical: [
                        i128::from(port.position.x),
                        i128::from(port.position.y),
                        edge_index(port.edge) as i128,
                        i128::from(
                            port.direction == aic_layout::facilities::FacilityPortDirection::Output,
                        ),
                        i128::from(port.transport == aic_layout::logistics::TransportKind::Pipe),
                    ],
                    original_membership: domains
                        .iter()
                        .map(|domain| original(domain.node) == port.id)
                        .collect(),
                },
            )
        })
        .collect();
    for domain in &mut domains {
        domain
            .ports
            .sort_by(|left, right| port_keys[left.as_str()].cmp(&port_keys[right.as_str()]));
    }
    Ok(domains)
}

pub fn node_position(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    node: &CompiledNode,
) -> Result<Point> {
    if let Some(CompiledTerminal::Facility { block, .. }) = &node.terminal {
        let owner = *block;
        let block = &problem.blocks[owner];
        let pose = draft.poses[owner];
        let selected = draft.selected_ports[node.id].as_deref().ok_or_else(|| {
            error(
                "missing-port",
                node.source_id.clone(),
                "facility terminal has no selected port",
            )
        })?;
        return rotate(
            facility(problem, &block.facility),
            pose.rotation,
            pose.position,
        )
        .2
        .into_iter()
        .find(|port| port.id == selected)
        .map(|port| port.connection)
        .ok_or_else(|| {
            error(
                "missing-port",
                node.source_id.clone(),
                "selected port is outside the used catalog",
            )
        });
    }
    Ok(draft.anchors[node.id])
}

pub fn endpoint_rule(
    recorded: Option<usize>,
    facility: Option<(i64, i64, usize)>,
    side: usize,
    reassigned: bool,
) -> (Option<usize>, Option<usize>) {
    let baseline_heading = match (recorded, facility) {
        (Some(value), Some((baseline, _, _))) => {
            Some((value as i64 - baseline / 90).rem_euclid(4) as usize)
        }
        _ => recorded,
    };
    let current = if reassigned {
        None
    } else if let Some((baseline, rotation, normal)) = facility {
        Some(recorded.map_or((normal + side * 2) % 4, |direction| {
            (direction as i64 + (rotation - baseline) / 90).rem_euclid(4) as usize
        }))
    } else {
        recorded
    };
    (current, baseline_heading)
}

fn directions(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    edge: &aic_layout::editor::CompiledEdge,
) -> (Option<usize>, Option<usize>) {
    let mut result = [edge.first, edge.last];
    for (side, node_id) in [edge.source, edge.target].into_iter().enumerate() {
        let node = &problem.nodes[node_id];
        if let Some(CompiledTerminal::Facility {
            block,
            original_port,
            ..
        }) = &node.terminal
        {
            let spec = facility(problem, &problem.blocks[*block].facility);
            let pose = draft.poses[*block];
            let selected = draft.selected_ports[node_id].as_deref().unwrap();
            let normal = rotate(spec, pose.rotation, Point::default())
                .2
                .iter()
                .find(|port| port.id == selected)
                .map(|port| edge_index(port.edge))
                .unwrap_or(0);
            result[side] = endpoint_rule(
                result[side],
                Some((
                    problem.blocks[*block].baseline_rotation,
                    pose.rotation,
                    normal,
                )),
                side,
                selected != original_port,
            )
            .0;
        }
    }
    (result[0], result[1])
}

pub fn edge_headings(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    edge: &aic_layout::editor::CompiledEdge,
) -> Result<(Option<usize>, Option<usize>)> {
    let start = node_position(problem, draft, &problem.nodes[edge.source])?;
    let end = node_position(problem, draft, &problem.nodes[edge.target])?;
    let headings = directions(problem, draft, edge);
    Ok(query_directions(edge.zero_length, start, end, headings))
}

pub fn query_directions(
    zero_length: bool,
    start: Point,
    end: Point,
    headings: (Option<usize>, Option<usize>),
) -> (Option<usize>, Option<usize>) {
    if zero_length && start == end && headings.0 == headings.1 {
        (None, None)
    } else {
        headings
    }
}

fn compatible(a: &CompiledNode, b: &CompiledNode) -> bool {
    a.id == b.id || a.components.iter().any(|id| b.components.contains(id))
}

fn bodies<'a>(
    problem: &'a CompiledProblemV1,
    draft: &'a PhysicalDraftV1,
) -> impl Iterator<Item = Point> + 'a {
    problem
        .blocks
        .iter()
        .zip(&draft.poses)
        .flat_map(|(block, pose)| {
            let (width, height, _) = rotate(
                facility(problem, &block.facility),
                pose.rotation,
                pose.position,
            );
            (pose.position.y..pose.position.y + height).flat_map(move |y| {
                (pose.position.x..pose.position.x + width).map(move |x| Point { x, y })
            })
        })
}

fn blocked(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    edge: &aic_layout::editor::CompiledEdge,
) -> Result<IndexSet<Point>> {
    let positions: Vec<_> = problem
        .nodes
        .iter()
        .map(|node| node_position(problem, draft, node))
        .collect::<Result<_>>()?;
    let permits = |other: usize, point: Point| {
        [edge.source, edge.target].iter().any(|&endpoint| {
            positions[endpoint] == point
                && compatible(&problem.nodes[endpoint], &problem.nodes[other])
        })
    };
    let mut result: IndexSet<_> = bodies(problem, draft).collect();
    let transport = problem.networks[edge.network].transport;
    for node in &problem.nodes {
        if problem.networks[node.network].transport == transport
            && !permits(node.id, positions[node.id])
        {
            result.insert(positions[node.id]);
        }
    }
    for (index, component) in problem.components.iter().enumerate() {
        if component.transport == transport {
            let point = draft.component_positions[index];
            if ![edge.source, edge.target].iter().any(|&endpoint| {
                positions[endpoint] == point && problem.nodes[endpoint].components.contains(&index)
            }) {
                result.insert(point);
            }
        }
    }
    for other in &problem.edges {
        if other.id == edge.id || problem.networks[other.network].transport != transport {
            continue;
        }
        if let Some(cells) = &draft.paths[other.id] {
            for (index, &point) in cells.iter().enumerate() {
                if !((index == 0 && permits(other.source, point))
                    || (index + 1 == cells.len() && permits(other.target, point)))
                {
                    result.insert(point);
                }
            }
        }
    }
    Ok(result)
}

fn check_path(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    edge: &aic_layout::editor::CompiledEdge,
    cells: &[Point],
) -> Result<()> {
    if cells.is_empty() || cells.len() > 250_000 {
        return Err(error(
            "route-geometry",
            &edge.source_id,
            "path is empty or too large",
        ));
    }
    let start = node_position(problem, draft, &problem.nodes[edge.source])?;
    let end = node_position(problem, draft, &problem.nodes[edge.target])?;
    if cells[0] != start || cells[cells.len() - 1] != end {
        return Err(error(
            "route-endpoint",
            &edge.source_id,
            "path endpoints do not match its nodes",
        ));
    }
    let headings = directions(problem, draft, edge);
    if (cells.len() == 1 && (!edge.zero_length || headings.0 != headings.1))
        || (cells.len() > 1
            && (headings
                .0
                .is_some_and(|value| direction(cells[0], cells[1]) != Some(value))
                || headings
                    .1
                    .is_some_and(|value| direction(cells[cells.len() - 2], end) != Some(value))))
    {
        return Err(error(
            "route-direction",
            &edge.source_id,
            "path violates an endpoint heading",
        ));
    }
    if cells.iter().collect::<IndexSet<_>>().len() != cells.len()
        || cells
            .windows(2)
            .any(|pair| direction(pair[0], pair[1]).is_none())
    {
        return Err(error(
            "route-geometry",
            &edge.source_id,
            "path must be simple and adjacent",
        ));
    }
    let blocked = blocked(problem, draft, edge)?;
    if let Some(point) = cells.iter().find(|point| blocked.contains(*point)) {
        return Err(error(
            "route-intersection",
            &edge.source_id,
            format!("path intersects occupied cell ({}, {})", point.x, point.y),
        ));
    }
    Ok(())
}

fn extents(problem: &CompiledProblemV1, draft: &PhysicalDraftV1) -> Result<(Point, Point)> {
    occupied_extents(problem, draft).map_err(compiled_error)
}

fn canonicalize(problem: &CompiledProblemV1, draft: &mut PhysicalDraftV1) -> Result<()> {
    let (origin, _) = extents(problem, draft)?;
    if origin != Point::default() {
        let translate = |point: &mut Point| {
            point.x -= origin.x;
            point.y -= origin.y;
        };
        for pose in &mut draft.poses {
            translate(&mut pose.position);
        }
        for point in &mut draft.anchors {
            translate(point);
        }
        for point in &mut draft.component_positions {
            translate(point);
        }
        for point in draft.paths.iter_mut().flatten().flatten() {
            translate(point);
        }
    }
    Ok(())
}

pub fn validate_draft(problem: &CompiledProblemV1, draft: &PhysicalDraftV1) -> Result<()> {
    validate_compiled(problem, draft).map_err(compiled_error)?;
    validate_draft_validated(problem, draft)
}

fn validate_draft_validated(problem: &CompiledProblemV1, draft: &PhysicalDraftV1) -> Result<()> {
    for edge in &problem.edges {
        if let Some(path) = &draft.paths[edge.id] {
            check_path(problem, draft, edge, path)?;
        }
    }
    Ok(())
}

pub fn conflicts(problem: &CompiledProblemV1, draft: &PhysicalDraftV1) -> Result<usize> {
    validate_compiled(problem, draft).map_err(compiled_error)?;
    Ok(conflicts_validated(problem, draft))
}

pub(crate) fn conflicts_validated(problem: &CompiledProblemV1, draft: &PhysicalDraftV1) -> usize {
    let bodies: Vec<BTreeSet<_>> = problem
        .blocks
        .iter()
        .zip(&draft.poses)
        .map(|(block, pose)| {
            let (width, height, _) = rotate(
                facility(problem, &block.facility),
                pose.rotation,
                pose.position,
            );
            (pose.position.y..pose.position.y + height)
                .flat_map(|y| {
                    (pose.position.x..pose.position.x + width).map(move |x| Point { x, y })
                })
                .collect()
        })
        .collect();
    let mut count = 0;
    for (index, body) in bodies.iter().enumerate() {
        count += bodies[index + 1..]
            .iter()
            .filter(|other| !body.is_disjoint(other))
            .count();
        count += draft
            .component_positions
            .iter()
            .filter(|point| body.contains(point))
            .count();
    }
    count
}

pub fn apply_edit(
    problem: &CompiledProblemV1,
    draft: &mut PhysicalDraftV1,
    action: PhysicalEdit,
) -> Result<()> {
    validate_compiled(problem, draft).map_err(compiled_error)?;
    let mut candidate = draft.clone();
    apply_edit_in_place(problem, &mut candidate, action)?;
    validate_compiled(problem, &candidate).map_err(compiled_error)?;
    *draft = candidate;
    Ok(())
}

pub(crate) fn apply_edit_in_place(
    problem: &CompiledProblemV1,
    draft: &mut PhysicalDraftV1,
    action: PhysicalEdit,
) -> Result<()> {
    match action {
        PhysicalEdit::Place {
            block,
            position: next,
            rotation,
            ports,
        } => {
            let block_index = problem
                .blocks
                .iter()
                .position(|candidate| candidate.id == block)
                .ok_or_else(|| {
                    error(
                        "block",
                        block.clone(),
                        "physical action references an unknown block",
                    )
                })?;
            let nodes = facility_nodes(problem, block_index);
            if nodes.iter().copied().collect::<BTreeSet<_>>()
                != ports
                    .iter()
                    .map(|selection| selection.node)
                    .collect::<BTreeSet<_>>()
                || ports.len() != nodes.len()
            {
                return Err(error(
                    "port-assignment",
                    block,
                    "place requires the block's complete terminal-to-port mapping",
                ));
            }
            if !facility(problem, &problem.blocks[block_index].facility)
                .allowed_rotations
                .contains(&rotation)
            {
                return Err(error(
                    "rotation",
                    block,
                    "rotation is outside the used catalog",
                ));
            }
            let (width, height, projected_ports) = rotate(
                facility(problem, &problem.blocks[block_index].facility),
                rotation,
                Point::default(),
            );
            let safe = next.x.checked_add(width - 1).is_some()
                && next.y.checked_add(height - 1).is_some()
                && projected_ports.iter().all(|port| {
                    next.x.checked_add(port.position.x).is_some()
                        && next.y.checked_add(port.position.y).is_some()
                        && next.x.checked_add(port.connection.x).is_some()
                        && next.y.checked_add(port.connection.y).is_some()
                });
            if !safe {
                return Err(error(
                    "coordinate-overflow",
                    block,
                    "block pose exceeds integer representation",
                ));
            }
            let domains: std::collections::BTreeMap<_, _> =
                block_port_domains(problem, block_index)?
                    .into_iter()
                    .map(|domain| (domain.node, domain.ports))
                    .collect();
            let mut selected = BTreeSet::new();
            if ports.iter().any(|selection| {
                !selected.insert(selection.port.as_str())
                    || !domains[&selection.node].contains(&selection.port)
            }) {
                return Err(error(
                    "port-assignment",
                    block,
                    "place assigns a duplicate or incompatible facility port",
                ));
            }
            draft.poses[block_index].position = next;
            draft.poses[block_index].rotation = rotation;
            for selection in ports {
                draft.selected_ports[selection.node] = Some(selection.port);
            }
        }
        PhysicalEdit::InsertBand { axis, at, count } => {
            band(problem, draft, axis, at, count, false)?
        }
        PhysicalEdit::DeleteBand { axis, at, count } => {
            band(problem, draft, axis, at, count, true)?
        }
    }
    canonicalize(problem, draft)?;
    let detached: Vec<_> = problem
        .edges
        .iter()
        .filter(|edge| {
            draft.paths[edge.id]
                .as_ref()
                .is_some_and(|path| check_path(problem, draft, edge, path).is_err())
        })
        .map(|edge| edge.id)
        .collect();
    for edge in detached {
        draft.paths[edge] = None;
    }
    Ok(())
}

fn band(
    problem: &CompiledProblemV1,
    draft: &mut PhysicalDraftV1,
    axis: Axis,
    at: i64,
    count: i64,
    delete: bool,
) -> Result<()> {
    if let Some(obstruction) = band_obstruction(problem, draft, axis, at, count, delete) {
        return Err(match obstruction {
            BandObstruction::Invalid => error(
                "band",
                "action",
                "Band coordinate/count exceeds the coordinate work guard",
            ),
            BandObstruction::Body(id) => error(
                "band-cuts-body",
                id,
                "Band would remove or split a rigid facility body",
            ),
            BandObstruction::Component(id) => error(
                "band-cuts-component",
                id,
                "Band would remove or split an original logistics component",
            ),
            BandObstruction::Anchor(id) => error(
                "band-removes-anchor",
                id,
                "Band would remove an original external or component anchor",
            ),
        });
    }
    let transform = |mut point: Point| -> Result<Option<Point>> {
        let value = axis.get(point);
        if delete && (at..at + count).contains(&value) {
            return Ok(None);
        }
        if value >= at {
            axis.set(
                &mut point,
                value
                    .checked_add(if delete { -count } else { count })
                    .ok_or_else(|| {
                        error("coordinate-overflow", "band", "band translation overflows")
                    })?,
            );
        }
        Ok(Some(point))
    };
    for pose in &mut draft.poses {
        pose.position = transform(pose.position)?.expect("block cuts rejected");
    }
    for point in &mut draft.anchors {
        *point = transform(*point)?.unwrap_or_else(|| {
            let mut retained = *point;
            axis.set(&mut retained, at);
            retained
        });
    }
    for point in &mut draft.component_positions {
        *point = transform(*point)?.expect("component cuts rejected");
    }
    for path in &mut draft.paths {
        if let Some(cells) = path {
            let mut transformed = Vec::with_capacity(cells.len());
            for &point in cells.iter() {
                if let Some(point) = transform(point)? {
                    transformed.push(point);
                }
            }
            *path = (!transformed.is_empty()).then_some(transformed);
        }
    }
    Ok(())
}

/// Structural mask authority shared by enumerators and the mutating band edit.
pub fn band_allowed(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    axis: Axis,
    at: i64,
    count: i64,
    delete: bool,
) -> Result<bool> {
    Ok(band_obstruction(problem, draft, axis, at, count, delete).is_none())
}

enum BandObstruction {
    Invalid,
    Body(String),
    Component(String),
    Anchor(String),
}

fn band_obstruction(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    axis: Axis,
    at: i64,
    count: i64,
    delete: bool,
) -> Option<BandObstruction> {
    if !(1..=GUARD).contains(&count) || at.checked_add(count).is_none() {
        return Some(BandObstruction::Invalid);
    }
    let cuts = |origin: i64, size: i64| {
        if delete {
            origin < at + count && origin + size > at
        } else {
            origin < at && origin + size > at
        }
    };
    for (block, pose) in problem.blocks.iter().zip(&draft.poses) {
        let (width, height, _) = rotate(
            facility(problem, &block.facility),
            pose.rotation,
            pose.position,
        );
        let (origin, size) = match axis {
            Axis::X => (pose.position.x, width),
            Axis::Y => (pose.position.y, height),
        };
        if cuts(origin, size) {
            return Some(BandObstruction::Body(block.id.clone()));
        }
    }
    for (index, component) in problem.components.iter().enumerate() {
        let definition = component_definition(problem, &component.definition);
        let (width, height) = if component.rotation % 180 == 0 {
            (definition.footprint.width, definition.footprint.height)
        } else {
            (definition.footprint.height, definition.footprint.width)
        };
        if cuts(
            axis.get(draft.component_positions[index]),
            match axis {
                Axis::X => width,
                Axis::Y => height,
            },
        ) {
            return Some(BandObstruction::Component(component.id.clone()));
        }
    }
    if delete
        && let Some(node) = problem.nodes.iter().find(|node| {
            !matches!(node.terminal, Some(CompiledTerminal::Facility { .. }))
                && (at..at + count).contains(&axis.get(draft.anchors[node.id]))
        })
    {
        return Some(BandObstruction::Anchor(node.source_id.clone()));
    }
    None
}

pub fn repair_pending(
    problem: &CompiledProblemV1,
    draft: &mut PhysicalDraftV1,
    limits: PhysicalEditLimits,
) -> Result<EditWork> {
    validate_compiled(problem, draft).map_err(compiled_error)?;
    repair_pending_validated(problem, draft, limits)
}

pub(crate) fn repair_pending_validated(
    problem: &CompiledProblemV1,
    draft: &mut PhysicalDraftV1,
    limits: PhysicalEditLimits,
) -> Result<EditWork> {
    repair_pending_validated_until(problem, draft, limits, || false)
}

pub(crate) fn repair_pending_validated_until(
    problem: &CompiledProblemV1,
    draft: &mut PhysicalDraftV1,
    limits: PhysicalEditLimits,
    mut expired: impl FnMut() -> bool,
) -> Result<EditWork> {
    let mut work = EditWork::default();
    let mut remaining = limits.route_expansions;
    let mut candidate = draft.clone();
    let mut pending: Vec<_> = problem
        .edges
        .iter()
        .filter(|edge| candidate.paths[edge.id].is_none())
        .collect();
    pending.sort_by_key(|edge| {
        (
            problem.networks[edge.network].transport != aic_layout::logistics::TransportKind::Pipe,
            edge.id,
        )
    });
    for edge in pending {
        if expired() {
            work.routes.push(RouteWork {
                edge: edge.id,
                status: "elapsed".into(),
                expanded: 0,
                preflight_expanded: 0,
                preflight_status: "not-run".into(),
            });
            work.pending = candidate.paths.iter().filter(|path| path.is_none()).count();
            return Ok(work);
        }
        let start = node_position(problem, &candidate, &problem.nodes[edge.source])?;
        let end = node_position(problem, &candidate, &problem.nodes[edge.target])?;
        let (low, high) = extents(problem, &candidate)?;
        let padding = limits.search_padding;
        if !(0..=1000).contains(&padding) {
            return Err(error(
                "coordinate-guard",
                "routing",
                "invalid search padding",
            ));
        }
        let bounds = Bounds {
            min_x: low.x.checked_sub(padding).ok_or_else(|| {
                error("coordinate-overflow", "routing", "routing bound overflows")
            })?,
            min_y: low.y.checked_sub(padding).ok_or_else(|| {
                error("coordinate-overflow", "routing", "routing bound overflows")
            })?,
            max_x: high.x.checked_add(padding).ok_or_else(|| {
                error("coordinate-overflow", "routing", "routing bound overflows")
            })?,
            max_y: high.y.checked_add(padding).ok_or_else(|| {
                error("coordinate-overflow", "routing", "routing bound overflows")
            })?,
        };
        if i128::from(bounds.width()) * i128::from(bounds.height()) > 250_000 {
            return Err(error(
                "routing-area-guard",
                &edge.source_id,
                "routing region exceeds 250000 cells",
            ));
        }
        let headings = edge_headings(problem, &candidate, edge)?;
        let result = aic_routing::shortest_path(
            start,
            end,
            &blocked(problem, &candidate, edge)?,
            bounds,
            aic_routing::PathOptions {
                first: headings.0,
                last: headings.1,
                work_budget: remaining,
                crossings: &IndexMap::new(),
                crossing_limit: 0,
                allow_zero_length: edge.zero_length,
                cell_penalties: None,
            },
        )
        .map_err(|failure| error("routing", failure.entity.to_string(), failure.message))?;
        work.route_calls += 1;
        work.expanded += result.expanded;
        work.preflight_expanded += result.preflight_expanded;
        remaining = remaining.saturating_sub(result.expanded);
        work.routes.push(RouteWork {
            edge: edge.id,
            status: result.status.to_string(),
            expanded: result.expanded,
            preflight_expanded: result.preflight_expanded,
            preflight_status: result.preflight_status.to_string(),
        });
        let Some(cells) = result.cells else {
            break;
        };
        check_path(problem, &candidate, edge, &cells)?;
        candidate.paths[edge.id] = Some(cells);
    }
    work.pending = candidate.paths.iter().filter(|path| path.is_none()).count();
    if work.pending == 0 {
        canonicalize(problem, &mut candidate)?;
        validate_draft_validated(problem, &candidate)?;
        *draft = candidate;
        work.committed = true;
    }
    Ok(work)
}
