//! Independent validation of a complete physical problem and solution.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::geometry::{Diagnostic, Point, direction};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::physical::{BridgeAxis, Junction, Layout, Metrics, SCHEMA, TerminalDirection};
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

/// The directed arm used by one route at one transport cell. A boundary
/// terminal contributes an external arm with no fixed cardinal direction.
pub struct SiteArmUse<'a> {
    pub connection: &'a str,
    pub commodity: &'a str,
    pub rate: Rate,
    pub incoming: Option<usize>,
    pub outgoing: Option<usize>,
    pub external_incoming: bool,
    pub external_outgoing: bool,
}

/// Return every catalog component/rotation that can carry this site's exact
/// directed flow. An ordinary cell returns an empty list without requiring a
/// component. This is also used at native route-commit boundaries.
pub fn junction_options(
    transport: TransportKind,
    point: Point,
    uses: &[SiteArmUse<'_>],
    catalogs: &Catalogs,
) -> Result<Option<Vec<(String, i64)>>> {
    if uses.is_empty() {
        return Err(error(
            "topology",
            format!("{},{}", point.x, point.y),
            "Empty transport site",
        ));
    }
    let commodity = uses[0].commodity;
    let mut incoming = [Rate::zero(); 4];
    let mut outgoing = [Rate::zero(); 4];
    let mut external_incoming = Rate::zero();
    let mut external_outgoing = Rate::zero();
    for use_site in uses {
        if use_site.commodity != commodity
            || use_site.incoming.is_some() == use_site.external_incoming
            || use_site.outgoing.is_some() == use_site.external_outgoing
        {
            return Err(error(
                "topology",
                format!("{},{}", point.x, point.y),
                "Mixed commodity or incomplete directed route arm",
            ));
        }
        if let Some(d) = use_site.incoming {
            incoming[d] = incoming[d].checked_add(use_site.rate).map_err(|_| {
                error(
                    "topology",
                    use_site.connection,
                    "Incoming arm rate overflow",
                )
            })?;
        } else {
            external_incoming = external_incoming.checked_add(use_site.rate).map_err(|_| {
                error(
                    "topology",
                    use_site.connection,
                    "External incoming rate overflow",
                )
            })?;
        }
        if let Some(d) = use_site.outgoing {
            outgoing[d] = outgoing[d].checked_add(use_site.rate).map_err(|_| {
                error(
                    "topology",
                    use_site.connection,
                    "Outgoing arm rate overflow",
                )
            })?;
        } else {
            external_outgoing = external_outgoing.checked_add(use_site.rate).map_err(|_| {
                error(
                    "topology",
                    use_site.connection,
                    "External outgoing rate overflow",
                )
            })?;
        }
    }
    let capacity = catalogs.transports.capacity(transport);
    let mut total_in = external_incoming;
    let mut total_out = external_outgoing;
    for d in 0..4 {
        if !incoming[d].is_zero() && !outgoing[d].is_zero() {
            return Err(error(
                "topology",
                format!("{},{}", point.x, point.y),
                "A physical arm carries opposing directions",
            ));
        }
        total_in = total_in
            .checked_add(incoming[d])
            .map_err(|_| error("topology", "routes", "Incoming flow overflow"))?;
        total_out = total_out
            .checked_add(outgoing[d])
            .map_err(|_| error("topology", "routes", "Outgoing flow overflow"))?;
    }
    if total_in != total_out || !within_capacity(total_in, capacity.quantity, capacity.duration_ms)
    {
        return Err(error(
            "topology",
            format!("{},{}", point.x, point.y),
            "Site flow is unbalanced or exceeds line capacity",
        ));
    }
    let in_dirs = (0..4)
        .filter(|&d| !incoming[d].is_zero())
        .collect::<Vec<_>>();
    let out_dirs = (0..4)
        .filter(|&d| !outgoing[d].is_zero())
        .collect::<Vec<_>>();
    let in_degree = in_dirs.len() + usize::from(!external_incoming.is_zero());
    let out_degree = out_dirs.len() + usize::from(!external_outgoing.is_zero());
    if in_degree <= 1 && out_degree <= 1 {
        return Ok(None);
    }
    let kind = match (in_degree, out_degree) {
        (1, 2..=3) => LogisticsComponentKind::Splitter,
        (2..=3, 1) => LogisticsComponentKind::Converger,
        _ => {
            return Err(error(
                "topology",
                format!("{},{}", point.x, point.y),
                "No catalog junction topology supports these directed arms",
            ));
        }
    };
    if !external_incoming.is_zero() || !external_outgoing.is_zero() {
        return Err(error(
            "topology",
            format!("{},{}", point.x, point.y),
            "A boundary terminal cannot host a junction",
        ));
    }
    let mut options = Vec::new();
    for definition in &catalogs.components.catalog().components {
        if definition.transport != transport
            || definition.kind != kind
            || definition.footprint.width != 1
            || definition.footprint.height != 1
            || !within_capacity(
                total_in,
                definition.capacity.quantity,
                definition.capacity.duration_ms,
            )
        {
            continue;
        }
        for &rotation in &definition.allowed_rotations {
            let rotated = |direction: &crate::logistics::CardinalDirection| {
                (cardinal_index(*direction) + rotation as usize / 90) % 4
            };
            let valid_in = in_dirs.iter().all(|&d| {
                definition
                    .input_directions
                    .iter()
                    .any(|arm| rotated(arm) == d)
            });
            let valid_out = out_dirs.iter().all(|&d| {
                definition
                    .output_directions
                    .iter()
                    .any(|arm| rotated(arm) == d)
            });
            if valid_in && valid_out {
                options.push((definition.id.clone(), rotation));
            }
        }
    }
    if options.is_empty() {
        return Err(error(
            "junction",
            format!("{},{}", point.x, point.y),
            "No catalog junction supports the directed arms, flow and rotation",
        ));
    }
    Ok(Some(options))
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
        || solution.junctions.len() > 32768
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
    let mut terminal_at = HashMap::new();
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
        terminal_at.insert(pose.position, i);
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
            if let Some(&terminal) = terminal_at.get(&p) {
                if problem.terminals[terminal].id != connection.source
                    && problem.terminals[terminal].id != connection.target
                {
                    return Err(error(
                        "terminal-intersection",
                        route.connection.clone(),
                        "Route crosses an unrelated boundary terminal",
                    ));
                }
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
        if bridge.channels[0].axis != BridgeAxis::Horizontal
            || bridge.channels[1].axis != BridgeAxis::Vertical
        {
            return Err(error(
                "bridge",
                bridge.id.clone(),
                "Bridge channels must declare horizontal and vertical axes",
            ));
        }
        let mut actual = [BTreeSet::new(), BTreeSet::new()];
        let mut load = [Rate::zero(), Rate::zero()];
        let mut directions = [None, None];
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
            let axis = d % 2;
            let channel = &bridge.channels[axis];
            if channel.commodity != problem.connections[ci].commodity
                || directions[axis].is_some_and(|previous| previous != d)
            {
                return Err(error(
                    "bridge",
                    bridge.id.clone(),
                    "One bridge axis cannot mix commodities or opposing flow directions",
                ));
            }
            directions[axis] = Some(d);
            actual[axis].insert(problem.connections[ci].id.as_str());
            load[axis] = load[axis]
                .checked_add(problem.connections[ci].rate)
                .map_err(|_| error("bridge", bridge.id.clone(), "Bridge axis rate overflow"))?;
            let rotated = |v| (cardinal_index(v) + bridge.rotation as usize / 90) % 4;
            if direction(route.cells[pi], route.cells[pi + 1]) != Some(d)
                || !definition
                    .input_directions
                    .iter()
                    .any(|&v| rotated(v) == (d + 2) % 4)
                || !definition
                    .output_directions
                    .iter()
                    .any(|&v| rotated(v) == d)
            {
                return Err(error(
                    "bridge",
                    bridge.id.clone(),
                    "Bridge route turns or violates rotated channel direction",
                ));
            }
        }
        for axis in 0..2 {
            let channel = &bridge.channels[axis];
            let named = channel
                .connections
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let line = catalogs.transports.capacity(definition.transport);
            if actual[axis].is_empty()
                || named.len() != channel.connections.len()
                || named != actual[axis]
                || !within_capacity(
                    load[axis],
                    definition.capacity.quantity,
                    definition.capacity.duration_ms,
                )
                || !within_capacity(load[axis], line.quantity, line.duration_ms)
            {
                return Err(error(
                    "bridge",
                    bridge.id.clone(),
                    "Bridge axis owners or aggregate capacity do not match route flow",
                ));
            }
        }
    }
    let mut junction_sites = HashMap::new();
    let mut junction_ids = HashSet::new();
    for junction in &solution.junctions {
        let definition = catalogs
            .components
            .component(&junction.component)
            .ok_or_else(|| {
                error(
                    "junction",
                    junction.id.clone(),
                    "Unknown junction component",
                )
            })?;
        if !is_stable_id(&junction.id)
            || !junction_ids.insert(junction.id.as_str())
            || bridge_ids.contains(junction.id.as_str())
            || !matches!(
                definition.kind,
                LogisticsComponentKind::Splitter | LogisticsComponentKind::Converger
            )
            || definition.footprint.width != 1
            || definition.footprint.height != 1
            || !definition.allowed_rotations.contains(&junction.rotation)
            || body.contains(&junction.position)
            || occupied_terminal_positions.contains(&junction.position)
            || bridge_sites.contains(&(definition.transport, junction.position))
            || junction_sites
                .insert((definition.transport, junction.position), junction)
                .is_some()
        {
            return Err(error(
                "junction",
                junction.id.clone(),
                "Invalid or overlapping catalog junction",
            ));
        }
    }
    for (&(transport, point), users) in &visits {
        if bridge_sites.contains(&(transport, point)) {
            if junction_sites.contains_key(&(transport, point)) {
                return Err(error("junction", "routes", "Junction overlaps bridge"));
            }
            continue;
        }
        if users.len() == 1 && !junction_sites.contains_key(&(transport, point)) {
            continue;
        }
        let mut arm_uses = Vec::with_capacity(users.len());
        for &(ci, pi) in users {
            let connection = &problem.connections[ci];
            let route = &solution.routes[route_indices[ci]];
            let mut external_incoming = false;
            let mut external_outgoing = false;
            let incoming = if pi > 0 {
                direction(point, route.cells[pi - 1])
            } else if let Some(&bi) = blocks.get(connection.source.as_str()) {
                let port = ports[bi]
                    .as_ref()
                    .unwrap()
                    .iter()
                    .find(|port| Some(port.id.as_str()) == route.source_port.as_deref())
                    .expect("route port checked before topology");
                direction(point, port.position)
            } else {
                external_incoming = true;
                None
            };
            let outgoing = if pi + 1 < route.cells.len() {
                direction(point, route.cells[pi + 1])
            } else if let Some(&bi) = blocks.get(connection.target.as_str()) {
                let port = ports[bi]
                    .as_ref()
                    .unwrap()
                    .iter()
                    .find(|port| Some(port.id.as_str()) == route.target_port.as_deref())
                    .expect("route port checked before topology");
                direction(point, port.position)
            } else {
                external_outgoing = true;
                None
            };
            arm_uses.push(SiteArmUse {
                connection: &connection.id,
                commodity: &connection.commodity,
                rate: connection.rate,
                incoming,
                outgoing,
                external_incoming,
                external_outgoing,
            });
        }
        let required = junction_options(transport, point, &arm_uses, catalogs)?;
        let selected: Option<&&Junction> = junction_sites.get(&(transport, point));
        match (required, selected) {
            (None, None) => {}
            (Some(options), Some(junction)) => {
                let named = junction
                    .connections
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                let actual = users
                    .iter()
                    .map(|&(ci, _)| problem.connections[ci].id.as_str())
                    .collect::<BTreeSet<_>>();
                if named.len() != junction.connections.len()
                    || named != actual
                    || !options.contains(&(junction.component.clone(), junction.rotation))
                {
                    return Err(error(
                        "junction",
                        junction.id.clone(),
                        "Junction ownership, catalog geometry or flow does not match route arms",
                    ));
                }
            }
            (Some(_), None) => {
                return Err(error(
                    "junction",
                    "routes",
                    "Branch or merge lacks a catalog junction",
                ));
            }
            (None, Some(junction)) => {
                return Err(error(
                    "junction",
                    junction.id.clone(),
                    "Unneeded junction on an ordinary line",
                ));
            }
        }
    }
    if junction_sites.len() != solution.junctions.len()
        || junction_sites.keys().any(|site| !visits.contains_key(site))
    {
        return Err(error(
            "junction",
            "layout",
            "Junction has no matching routed site",
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
