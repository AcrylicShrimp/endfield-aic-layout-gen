//! Independent validation of a complete physical problem and solution.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::geometry::{Diagnostic, Point, direction};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::physical::{Layout, Metrics, SCHEMA, TerminalDirection};
use crate::projection::{cardinal_index, rotate};
use crate::rate::Rate;
use crate::stable_id::is_stable_id;

/// Loaded and validated once, then borrowed by generation and validation workers.
pub struct Catalogs {
    pub identity: String,
    pub facilities: ValidatedFacilityCatalog,
    pub transports: ValidatedTransportCatalog,
    pub components: ValidatedLogisticsComponentCatalog,
}

pub type Result<T> = std::result::Result<T, Diagnostic>;

pub fn error(
    code: &str,
    entity: impl Into<serde_json::Value>,
    message: impl Into<String>,
) -> Diagnostic {
    let mut d = Diagnostic::new(format!("aic-layout.{code}"), message, entity);
    d.stage = "physical-layout-validation";
    d
}

fn valid_point(p: Point) -> bool {
    (0..=1_000_000).contains(&p.x) && (0..=1_000_000).contains(&p.y)
}

pub fn within_capacity(rate: Rate, quantity: i64, duration_ms: i64) -> bool {
    if rate.numerator <= 0 || rate.denominator <= 0 || quantity <= 0 || duration_ms <= 0 {
        return false;
    }
    // Divide with ceiling before the comparison: multiplying three i64 factors
    // could overflow i128 for otherwise well-formed external catalog values.
    let demand = i128::from(rate.numerator) * i128::from(duration_ms);
    let per_millisecond = demand / 1000 + i128::from(demand % 1000 != 0);
    per_millisecond <= i128::from(quantity) * i128::from(rate.denominator)
}

/// Validate actual geometry and obligations, never caller-supplied metrics/flags.
/// Count/materialization guards are operational limits, not game limits.
pub fn validate(layout: &Layout, catalogs: &Catalogs) -> Result<Metrics> {
    let problem = &layout.problem;
    let solution = &layout.solution;
    if layout.schema != SCHEMA
        || layout.catalog_sha256 != catalogs.identity
        || catalogs.identity.len() != 64
        || !catalogs.identity.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(error(
            "identity",
            "layout",
            "Unsupported schema or mismatched external catalog identity",
        ));
    }
    if problem.blocks.is_empty()
        || problem.blocks.len() > 4096
        || problem.terminals.len() > 32768
        || solution.poses.len() != problem.blocks.len()
        || solution.terminal_poses.len() != problem.terminals.len()
        || problem.connections.len() > 32768
        || solution.routes.len() != problem.connections.len()
        || solution.bridges.len() > 32768
    {
        return Err(error(
            "counts",
            "layout",
            "Missing poses/routes or counts exceed physical input guards",
        ));
    }
    let mut blocks = BTreeMap::new();
    for (i, block) in problem.blocks.iter().enumerate() {
        if !is_stable_id(&block.id)
            || blocks.insert(block.id.as_str(), i).is_some()
            || catalogs.facilities.facility(&block.facility).is_none()
        {
            return Err(error(
                "block",
                block.id.clone(),
                "Duplicate/invalid block identity or unknown catalog facility",
            ));
        }
    }
    let mut terminals = BTreeMap::new();
    for (i, terminal) in problem.terminals.iter().enumerate() {
        if !is_stable_id(&terminal.id)
            || !is_stable_id(&terminal.commodity)
            || blocks.contains_key(terminal.id.as_str())
            || terminals.insert(terminal.id.as_str(), i).is_some()
        {
            return Err(error(
                "terminal",
                terminal.id.clone(),
                "Duplicate/invalid terminal identity or commodity",
            ));
        }
    }
    let mut body = HashSet::new();
    let mut ports = vec![None; problem.blocks.len()];
    let mut lo = Point {
        x: i64::MAX,
        y: i64::MAX,
    };
    let mut hi = Point { x: 0, y: 0 };
    let mut include = |p: Point| {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    };
    for pose in &solution.poses {
        let Some(&i) = blocks.get(pose.block.as_str()) else {
            return Err(error(
                "pose",
                pose.block.clone(),
                "Pose references an unknown block",
            ));
        };
        let definition = catalogs
            .facilities
            .facility(&problem.blocks[i].facility)
            .unwrap();
        if ports[i].is_some()
            || !valid_point(pose.position)
            || !definition.allowed_rotations.contains(&pose.rotation)
        {
            return Err(error(
                "pose",
                pose.block.clone(),
                "Duplicate pose, invalid coordinate or disallowed rotation",
            ));
        }
        // Reject oversized external geometry before adding origins to its ports.
        let footprint = &definition.footprint;
        if footprint.width > 1000
            || footprint.height > 1000
            || footprint.width * footprint.height > 10_000
        {
            return Err(error(
                "body-guard",
                pose.block.clone(),
                "Block geometry exceeds the materialization guard",
            ));
        }
        let (w, h, projected) = rotate(definition, pose.rotation, pose.position);
        if w > 1000
            || h > 1000
            || w * h > 10_000
            || body.len() as i64 + w * h > 2_000_000
            || !valid_point(pose.position.shifted(w - 1, h - 1))
        {
            return Err(error(
                "body-guard",
                pose.block.clone(),
                "Block geometry exceeds the materialization guard",
            ));
        }
        include(pose.position);
        include(pose.position.shifted(w - 1, h - 1));
        for y in pose.position.y..pose.position.y + h {
            for x in pose.position.x..pose.position.x + w {
                if !body.insert(Point { x, y }) {
                    return Err(error("overlap", pose.block.clone(), "Block bodies overlap"));
                }
            }
        }
        ports[i] = Some(projected);
    }
    let mut terminal_positions = vec![None; problem.terminals.len()];
    let mut occupied_terminal_positions = HashSet::new();
    for pose in &solution.terminal_poses {
        let Some(&i) = terminals.get(pose.terminal.as_str()) else {
            return Err(error(
                "terminal-pose",
                pose.terminal.clone(),
                "Terminal pose references an unknown boundary terminal",
            ));
        };
        if terminal_positions[i].is_some()
            || !valid_point(pose.position)
            || !occupied_terminal_positions.insert(pose.position)
            || body.contains(&pose.position)
        {
            return Err(error(
                "terminal-pose",
                pose.terminal.clone(),
                "Duplicate terminal, invalid coordinate or occupied boundary cell",
            ));
        }
        include(pose.position);
        terminal_positions[i] = Some(pose.position);
    }
    let mut connection_ids = BTreeMap::new();
    let mut parent: Vec<_> = (0..blocks.len() + terminals.len()).collect();
    let mut incoming = vec![0; blocks.len()];
    let mut outgoing = vec![0; blocks.len()];
    let mut transports = [0; 2];
    for (i, connection) in problem.connections.iter().enumerate() {
        let source_block = blocks.get(connection.source.as_str()).copied();
        let target_block = blocks.get(connection.target.as_str()).copied();
        let source_terminal = terminals.get(connection.source.as_str()).copied();
        let target_terminal = terminals.get(connection.target.as_str()).copied();
        let source = source_block.or_else(|| source_terminal.map(|index| blocks.len() + index));
        let target = target_block.or_else(|| target_terminal.map(|index| blocks.len() + index));
        if !is_stable_id(&connection.id)
            || !is_stable_id(&connection.commodity)
            || connection_ids.insert(connection.id.as_str(), i).is_some()
            || source.is_none()
            || target.is_none()
            || source == target
            || source_terminal.is_some() && target_terminal.is_some()
        {
            return Err(error(
                "connection",
                connection.id.clone(),
                "Invalid/duplicate connection or missing/self endpoint",
            ));
        }
        for (terminal, expected) in [
            (source_terminal, TerminalDirection::Input),
            (target_terminal, TerminalDirection::Output),
        ] {
            if terminal.is_some_and(|index| {
                let terminal = &problem.terminals[index];
                terminal.direction != expected
                    || terminal.commodity != connection.commodity
                    || terminal.transport != connection.transport
            }) {
                return Err(error(
                    "terminal-connection",
                    connection.id.clone(),
                    "Boundary terminal role, commodity or transport does not match the connection",
                ));
            }
        }
        let capacity = catalogs.transports.capacity(connection.transport);
        if !within_capacity(connection.rate, capacity.quantity, capacity.duration_ms) {
            return Err(error(
                "capacity",
                connection.id.clone(),
                "Nonpositive or excessive transport demand",
            ));
        }
        let (s, t) = (source.unwrap(), target.unwrap());
        let (a, b) = (root(&parent, s), root(&parent, t));
        parent[a] = b;
        if let Some(s) = source_block {
            outgoing[s] += 1;
        }
        if let Some(t) = target_block {
            incoming[t] += 1;
        }
        transports[crate::geometry::layer(connection.transport)] += 1;
    }
    let mut routed = HashSet::new();
    let mut selected_ports = HashMap::new();
    let mut visits: HashMap<(TransportKind, Point), Vec<(usize, usize)>> = HashMap::new();
    let mut route_indices = vec![0; problem.connections.len()];
    let mut total_cells = 0usize;
    let mut turns = 0u64;
    let mut distance_sum = 0i64;
    let mut detour_sum = 0f64;
    for (ri, route) in solution.routes.iter().enumerate() {
        let Some(&ci) = connection_ids.get(route.connection.as_str()) else {
            return Err(error(
                "route",
                route.connection.clone(),
                "Route references an unknown obligation",
            ));
        };
        if !routed.insert(ci) || route.cells.is_empty() || route.cells.len() > 250_000 {
            return Err(error(
                "route",
                route.connection.clone(),
                "Duplicate route or invalid path length",
            ));
        }
        total_cells = total_cells.saturating_add(route.cells.len());
        if total_cells > 2_000_000 {
            return Err(error(
                "route-guard",
                "routes",
                "Total path cells exceed materialization guard",
            ));
        }
        route_indices[ci] = ri;
        let connection = &problem.connections[ci];
        for (endpoint_id, port_id, expected, endpoint) in [
            (
                &connection.source,
                &route.source_port,
                FacilityPortDirection::Output,
                route.cells[0],
            ),
            (
                &connection.target,
                &route.target_port,
                FacilityPortDirection::Input,
                *route.cells.last().unwrap(),
            ),
        ] {
            if let Some(&bi) = blocks.get(endpoint_id.as_str()) {
                let Some(port_id) = port_id.as_deref() else {
                    return Err(error(
                        "port",
                        route.connection.clone(),
                        "Facility route endpoint omitted its selected port",
                    ));
                };
                let port = ports[bi].as_ref().unwrap().iter().find(|p| p.id == port_id);
                if port.is_none_or(|p| {
                    p.direction != expected
                        || p.transport != connection.transport
                        || p.connection != endpoint
                }) {
                    return Err(error(
                        "port",
                        route.connection.clone(),
                        "Wrong port, transport, direction or endpoint position",
                    ));
                }
                let key = (bi, port_id.to_owned());
                let entry = selected_ports.entry(key).or_insert_with(|| {
                    (
                        connection.commodity.clone(),
                        connection.transport,
                        expected,
                        Rate::zero(),
                    )
                });
                if entry.0 != connection.commodity
                    || entry.1 != connection.transport
                    || entry.2 != expected
                {
                    return Err(error(
                        "port",
                        route.connection.clone(),
                        "A facility port is shared by incompatible material flows",
                    ));
                }
                entry.3 = entry.3.checked_add(connection.rate).map_err(|_| {
                    error(
                        "port-capacity",
                        route.connection.clone(),
                        "Shared port rate overflow",
                    )
                })?;
                let capacity = catalogs.transports.capacity(connection.transport);
                if !within_capacity(entry.3, capacity.quantity, capacity.duration_ms) {
                    return Err(error(
                        "port-capacity",
                        route.connection.clone(),
                        "Aggregate shared port demand exceeds one transport line",
                    ));
                }
            } else {
                let ti = terminals[endpoint_id.as_str()];
                if port_id.is_some() || terminal_positions[ti] != Some(endpoint) {
                    return Err(error(
                        "terminal-route",
                        route.connection.clone(),
                        "Boundary route endpoint has a port or misses its terminal position",
                    ));
                }
            }
        }
        let mut local = HashSet::new();
        for (pi, &p) in route.cells.iter().enumerate() {
            if terminal_positions
                .iter()
                .enumerate()
                .any(|(terminal, position)| {
                    *position == Some(p)
                        && problem.terminals[terminal].id != connection.source
                        && problem.terminals[terminal].id != connection.target
                })
            {
                return Err(error(
                    "terminal-intersection",
                    route.connection.clone(),
                    "Route crosses an unrelated boundary terminal",
                ));
            }
            if !valid_point(p)
                || body.contains(&p)
                || !local.insert(p)
                || pi > 0 && direction(route.cells[pi - 1], p).is_none()
            {
                return Err(error(
                    "path",
                    route.connection.clone(),
                    "Path leaves bounds, intersects a body, repeats or skips cells",
                ));
            }
            visits
                .entry((connection.transport, p))
                .or_default()
                .push((ci, pi));
            include(p);
        }
        turns += route
            .cells
            .windows(3)
            .filter(|p| direction(p[0], p[1]) != direction(p[1], p[2]))
            .count() as u64;
        let distance = route.cells[0].distance(*route.cells.last().unwrap());
        distance_sum += distance;
        detour_sum += (route.cells.len() - 1) as f64 / distance.max(1) as f64;
    }
    let mut shared_sites = HashSet::new();
    for (&(transport, point), users) in &visits {
        if users.len() <= 1 {
            continue;
        }
        let commodities = users
            .iter()
            .map(|&(connection, _)| problem.connections[connection].commodity.as_str())
            .collect::<BTreeSet<_>>();
        if commodities.len() == 1 {
            let mut load = Rate::zero();
            for &(connection, _) in users {
                load = load
                    .checked_add(problem.connections[connection].rate)
                    .map_err(|_| {
                        error("shared-capacity", "routes", "Shared route rate overflow")
                    })?;
            }
            let capacity = catalogs.transports.capacity(transport);
            if !within_capacity(load, capacity.quantity, capacity.duration_ms) {
                return Err(error(
                    "shared-capacity",
                    "routes",
                    "Aggregate same-commodity route demand exceeds line capacity",
                ));
            }
            shared_sites.insert((transport, point));
        }
    }
    let mut bridge_sites = HashSet::new();
    let mut bridge_ids = HashSet::new();
    for bridge in &solution.bridges {
        let Some(definition) = catalogs.components.component(&bridge.component) else {
            return Err(error(
                "bridge",
                bridge.id.clone(),
                "Unknown bridge definition",
            ));
        };
        if !is_stable_id(&bridge.id)
            || !bridge_ids.insert(bridge.id.as_str())
            || definition.kind != LogisticsComponentKind::Bridge
            || definition.footprint.width != 1
            || definition.footprint.height != 1
            || !definition.allowed_rotations.contains(&bridge.rotation)
            || !bridge_sites.insert((definition.transport, bridge.position))
        {
            return Err(error(
                "bridge",
                bridge.id.clone(),
                "Duplicate, unsupported or incorrectly rotated bridge",
            ));
        }
        let Some(users) = visits.get(&(definition.transport, bridge.position)) else {
            return Err(error(
                "bridge",
                bridge.id.clone(),
                "Bridge is not used by a route",
            ));
        };
        let named: BTreeSet<_> = bridge.connections.iter().map(String::as_str).collect();
        let actual: BTreeSet<_> = users
            .iter()
            .map(|&(ci, _)| problem.connections[ci].id.as_str())
            .collect();
        if users.len() != 2 || named.len() != 2 || named != actual {
            return Err(error(
                "bridge",
                bridge.id.clone(),
                "Bridge must carry exactly its two declared connections",
            ));
        }
        let mut axes = HashSet::new();
        for &(ci, pi) in users {
            let route = &solution.routes[route_indices[ci]];
            if pi == 0 || pi + 1 == route.cells.len() {
                return Err(error(
                    "bridge",
                    bridge.id.clone(),
                    "A bridge cannot terminate a route",
                ));
            }
            let d = direction(route.cells[pi - 1], route.cells[pi]).unwrap();
            let rotated = |v| (cardinal_index(v) + bridge.rotation as usize / 90) % 4;
            if direction(route.cells[pi], route.cells[pi + 1]) != Some(d)
                || !axes.insert(d % 2)
                || !definition
                    .input_directions
                    .iter()
                    .any(|&v| rotated(v) == (d + 2) % 4)
                || !definition
                    .output_directions
                    .iter()
                    .any(|&v| rotated(v) == d)
                || !within_capacity(
                    problem.connections[ci].rate,
                    definition.capacity.quantity,
                    definition.capacity.duration_ms,
                )
            {
                return Err(error(
                    "bridge",
                    bridge.id.clone(),
                    "Bridge turns, shares an axis, violates direction or exceeds capacity",
                ));
            }
        }
    }
    if visits.iter().any(|(tag, users)| {
        users.len() > 1 && !bridge_sites.contains(tag) && !shared_sites.contains(tag)
    }) {
        return Err(error(
            "intersection",
            "routes",
            "Same-layer paths intersect without a valid catalog bridge",
        ));
    }
    let width = hi.x - lo.x + 1;
    let height = hi.y - lo.y + 1;
    if terminal_positions.iter().flatten().any(|position| {
        position.x != lo.x && position.x != hi.x && position.y != lo.y && position.y != hi.y
    }) {
        return Err(error(
            "terminal-boundary",
            "terminals",
            "Every external terminal must lie on the final used layout boundary",
        ));
    }
    let area = (width as u64) * (height as u64);
    let unique_transport: HashSet<_> = visits.keys().map(|(_, p)| *p).collect();
    Ok(Metrics {
        width,
        height,
        area,
        body_cells: body.len() as u64,
        transport_cells: visits.len() as u64,
        turns,
        occupancy: (body.len() + unique_transport.len()) as f64 / area as f64,
        weak_components: (0..parent.len())
            .map(|i| root(&parent, i))
            .collect::<HashSet<_>>()
            .len(),
        branch_blocks: outgoing.iter().filter(|&&v| v > 1).count(),
        merge_blocks: incoming.iter().filter(|&&v| v > 1).count(),
        max_degree: incoming
            .iter()
            .zip(&outgoing)
            .map(|(a, b)| a + b)
            .max()
            .unwrap_or(0),
        mean_port_distance: distance_sum as f64 / problem.connections.len().max(1) as f64,
        mean_detour: detour_sum / problem.connections.len().max(1) as f64,
        belt_connections: transports[0],
        pipe_connections: transports[1],
        bridges: solution.bridges.len(),
    })
}

fn root(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i {
        i = parent[i];
    }
    i
}
