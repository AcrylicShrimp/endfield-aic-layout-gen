//! Independent validation of a complete physical problem and solution.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::geometry::{Diagnostic, Point, direction};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::physical::{Layout, Metrics, SCHEMA};
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
        || solution.poses.len() != problem.blocks.len()
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
    let mut connection_ids = BTreeMap::new();
    let mut parent: Vec<_> = (0..blocks.len()).collect();
    let mut incoming = vec![0; blocks.len()];
    let mut outgoing = vec![0; blocks.len()];
    let mut transports = [0; 2];
    for (i, connection) in problem.connections.iter().enumerate() {
        let source = blocks.get(connection.source.as_str()).copied();
        let target = blocks.get(connection.target.as_str()).copied();
        if !is_stable_id(&connection.id)
            || !is_stable_id(&connection.commodity)
            || connection_ids.insert(connection.id.as_str(), i).is_some()
            || source.is_none()
            || target.is_none()
            || source == target
        {
            return Err(error(
                "connection",
                connection.id.clone(),
                "Invalid/duplicate connection or missing/self endpoint",
            ));
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
        outgoing[s] += 1;
        incoming[t] += 1;
        transports[crate::geometry::layer(connection.transport)] += 1;
    }
    let mut routed = HashSet::new();
    let mut selected_ports = HashSet::new();
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
        for (block, port_id, expected, endpoint) in [
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
            let bi = blocks[block.as_str()];
            let port = ports[bi]
                .as_ref()
                .unwrap()
                .iter()
                .find(|p| p.id == *port_id);
            if port.is_none_or(|p| {
                p.direction != expected
                    || p.transport != connection.transport
                    || p.connection != endpoint
            }) || !selected_ports.insert((bi, port_id.as_str()))
            {
                return Err(error(
                    "port",
                    route.connection.clone(),
                    "Wrong/reused port, transport, direction or endpoint position",
                ));
            }
        }
        let mut local = HashSet::new();
        for (pi, &p) in route.cells.iter().enumerate() {
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
    if visits
        .iter()
        .any(|(tag, users)| users.len() > 1 && !bridge_sites.contains(tag))
    {
        return Err(error(
            "intersection",
            "routes",
            "Same-layer paths intersect without a valid catalog bridge",
        ));
    }
    let width = hi.x - lo.x + 1;
    let height = hi.y - lo.y + 1;
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
