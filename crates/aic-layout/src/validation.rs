//! Independent validation of a complete physical problem and solution.
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hash};
use std::ops::Deref;

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::geometry::{Diagnostic, Point, direction};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::physical::{BridgeAxis, Junction, Layout, Metrics, SCHEMA, TerminalDirection};
use crate::projection::{
    Port, PortProjectionCursor, ProjectionAdmission, ProjectionAdvance, cardinal_index,
};
use crate::rate::Rate;
use crate::stable_id::{StableIdAdvance, StableIdCursor};

/// Loaded and validated once, then borrowed by generation and validation workers.
pub struct Catalogs {
    pub identity: String,
    pub facilities: ValidatedFacilityCatalog,
    pub transports: ValidatedTransportCatalog,
    pub components: ValidatedLogisticsComponentCatalog,
}

pub type Result<T> = std::result::Result<T, Diagnostic>;

mod junction_options_cursor;
pub use junction_options_cursor::{JunctionAdvance, JunctionOptionsCursor, JunctionReservation};
mod resumable;
use resumable::Permit;
pub use resumable::{ValidationAdvance, ValidationCursor, ValidationReservation, ValidationVisit};

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
    junction_options_with_visit(transport, point, uses, catalogs, || true)
}

/// The visit hook is used by bounded native replay to stop inside catalog
/// enumeration. The ordinary validator passes a no-op hook.
pub fn junction_options_with_visit(
    transport: TransportKind,
    point: Point,
    uses: &[SiteArmUse<'_>],
    catalogs: &Catalogs,
    visit: impl FnMut() -> bool,
) -> Result<Option<Vec<(String, i64)>>> {
    let mut cursor = JunctionOptionsCursor::new(transport, point);
    match cursor.advance(
        uses,
        catalogs,
        || true,
        visit,
        |bytes| bytes,
        |_| JunctionReservation::Granted,
        |_| {},
    ) {
        JunctionAdvance::Complete(result) => result,
        JunctionAdvance::Pending => Err(error(
            "route-request-work-limit",
            format!("{},{}", point.x, point.y),
            "Catalog visit allowance or deadline reached",
        )),
        JunctionAdvance::Retry | JunctionAdvance::Denied => {
            unreachable!("unbounded catalog result allocation always admits")
        }
    }
}

async fn projected_ports(
    definition: &crate::facilities::FacilityDefinition,
    rotation: i64,
    origin: Point,
    permit: &Permit,
) -> Result<(i64, i64, Vec<Port>)> {
    let mut width = definition.footprint.width;
    let mut height = definition.footprint.height;
    for _ in 0..rotation / 90 {
        std::mem::swap(&mut width, &mut height);
    }
    let mut cursor = PortProjectionCursor::new();
    let mut grant = false;
    loop {
        let mut requested = None;
        match cursor.advance(definition, rotation, origin, |bytes| {
            if std::mem::take(&mut grant) {
                ProjectionAdmission::Granted
            } else {
                requested = Some(bytes);
                ProjectionAdmission::Pending
            }
        }) {
            ProjectionAdvance::Complete(ports) => return Ok((width, height, ports)),
            ProjectionAdvance::Pending => {
                let bytes = requested.expect("projection asks admission before yielding");
                permit.reserve(bytes).await?;
                permit.visit().await?;
                grant = true;
            }
            ProjectionAdvance::Denied => unreachable!("the validator grants admitted projection"),
        }
    }
}

async fn find_projected_port<'a>(
    ports: &'a [Port],
    id: &str,
    permit: &Permit,
) -> Result<Option<&'a Port>> {
    for port in ports {
        permit.visit().await?;
        if port.id == id {
            return Ok(Some(port));
        }
    }
    Ok(None)
}

async fn stable_id_resumable(value: &str, permit: &Permit) -> Result<bool> {
    let mut cursor = StableIdCursor::new();
    let mut grant = false;
    loop {
        match cursor.advance(value, || std::mem::take(&mut grant)) {
            StableIdAdvance::Complete(valid) => return Ok(valid),
            StableIdAdvance::Pending => {
                permit.visit().await?;
                grant = true;
            }
        }
    }
}

fn hash_table_bytes(capacity: usize, entry_bytes: usize) -> usize {
    if capacity == 0 {
        0
    } else {
        let buckets = capacity.saturating_add(1).next_power_of_two();
        buckets
            .saturating_mul(entry_bytes.saturating_add(1))
            .saturating_add(32)
    }
}

fn next_hash_table_bytes(capacity: usize, entry_bytes: usize) -> usize {
    let buckets = if capacity == 0 {
        4
    } else {
        capacity
            .saturating_add(1)
            .next_power_of_two()
            .saturating_mul(2)
    };
    buckets
        .saturating_mul(entry_bytes.saturating_add(1))
        .saturating_add(32)
}

async fn prepare_set_insert<T: Eq + Hash, S: BuildHasher>(
    values: &mut HashSet<T, S>,
    key: &T,
    permit: &Permit,
) -> Result<()> {
    if values.len() == values.capacity() && !values.contains(key) {
        let old = hash_table_bytes(values.capacity(), std::mem::size_of::<T>());
        permit
            .reserve(next_hash_table_bytes(
                values.capacity(),
                std::mem::size_of::<T>(),
            ))
            .await?;
        values.reserve(1);
        permit.release(old).await;
    }
    Ok(())
}

async fn prepare_map_insert<K: Eq + Hash, V, S: BuildHasher>(
    values: &mut HashMap<K, V, S>,
    key: &K,
    permit: &Permit,
) -> Result<()> {
    if values.len() == values.capacity() && !values.contains_key(key) {
        let entry_bytes = std::mem::size_of::<(K, V)>();
        let old = hash_table_bytes(values.capacity(), entry_bytes);
        permit
            .reserve(next_hash_table_bytes(values.capacity(), entry_bytes))
            .await?;
        values.reserve(1);
        permit.release(old).await;
    }
    Ok(())
}

async fn prepare_vec_push<T>(values: &mut Vec<T>, permit: &Permit) -> Result<()> {
    if values.len() == values.capacity() {
        let old = values.capacity().saturating_mul(std::mem::size_of::<T>());
        let next_capacity = values.capacity().saturating_mul(2).max(4);
        permit
            .reserve(next_capacity.saturating_mul(std::mem::size_of::<T>()))
            .await?;
        values.reserve(1);
        permit.release(old).await;
    }
    Ok(())
}

async fn junction_options_resumable(
    transport: TransportKind,
    point: Point,
    uses: &[SiteArmUse<'_>],
    catalogs: &Catalogs,
    permit: &Permit,
) -> Result<(Option<Vec<(String, i64)>>, usize)> {
    let mut cursor = JunctionOptionsCursor::new(transport, point);
    let visit_grant = std::cell::Cell::new(false);
    let copy_grant = std::cell::Cell::new(None);
    let mut reservation_grant = None;
    loop {
        let mut requested = None;
        let requested_copy = std::cell::Cell::new(None);
        let released_bytes = std::cell::Cell::new(0usize);
        let result = cursor.advance(
            uses,
            catalogs,
            || visit_grant.replace(false),
            || visit_grant.replace(false),
            |bytes| {
                if copy_grant.replace(None) == Some(bytes) {
                    bytes
                } else {
                    requested_copy.set(Some(bytes));
                    0
                }
            },
            |bytes| {
                if reservation_grant.take() == Some(bytes) {
                    JunctionReservation::Granted
                } else {
                    requested = Some(bytes);
                    JunctionReservation::Retry
                }
            },
            |bytes| released_bytes.set(released_bytes.get().saturating_add(bytes)),
        );
        permit.release(released_bytes.get()).await;
        match result {
            JunctionAdvance::Complete(result) => {
                let reserved_bytes = cursor.reserved_bytes();
                match result {
                    Ok(options) => return Ok((options, reserved_bytes)),
                    Err(failure) => {
                        permit.release(reserved_bytes).await;
                        return Err(failure);
                    }
                }
            }
            JunctionAdvance::Pending => {
                permit.visit().await?;
                if let Some(bytes) = requested_copy.get() {
                    copy_grant.set(Some(bytes));
                } else {
                    visit_grant.set(true);
                }
            }
            JunctionAdvance::Retry => {
                let bytes = requested.expect("catalog result asks reservation before retry");
                permit.reserve(bytes).await?;
                reservation_grant = Some(bytes);
            }
            JunctionAdvance::Denied => unreachable!("the validator grants admitted result memory"),
        }
    }
}

/// Validate actual geometry and obligations, never caller-supplied metrics/flags.
/// Count/materialization guards are operational limits, not game limits.
pub fn validate(layout: &Layout, catalogs: &Catalogs) -> Result<Metrics> {
    validate_with_visit(layout, catalogs, || Ok(()))
}

/// Validate while allowing a caller to meter and interrupt record visits.
pub fn validate_with_visit(
    layout: &Layout,
    catalogs: &Catalogs,
    mut visit: impl FnMut() -> Result<()>,
) -> Result<Metrics> {
    let mut cursor = ValidationCursor::new(layout, catalogs);
    match cursor.advance(
        || match visit() {
            Ok(()) => ValidationVisit::Granted,
            Err(failure) => ValidationVisit::Denied(failure),
        },
        |_| ValidationReservation::Granted,
        |_| {},
    ) {
        ValidationAdvance::Complete(result) => result,
        ValidationAdvance::Pending => unreachable!("one-shot validator cannot retry"),
    }
}

async fn validate_core<L, C>(layout: L, catalogs: C, permit: Permit) -> Result<Metrics>
where
    L: Deref<Target = Layout>,
    C: Deref<Target = Catalogs>,
{
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
    let mut blocks = HashMap::new();
    for (i, block) in problem.blocks.iter().enumerate() {
        permit.visit().await?;
        prepare_map_insert(&mut blocks, &block.id.as_str(), &permit).await?;
        if !stable_id_resumable(&block.id, &permit).await?
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
    let mut terminals = HashMap::new();
    for (i, terminal) in problem.terminals.iter().enumerate() {
        permit.visit().await?;
        prepare_map_insert(&mut terminals, &terminal.id.as_str(), &permit).await?;
        if !stable_id_resumable(&terminal.id, &permit).await?
            || !stable_id_resumable(&terminal.commodity, &permit).await?
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
    permit
        .reserve(
            problem
                .blocks
                .len()
                .saturating_mul(std::mem::size_of::<Option<Vec<Port>>>()),
        )
        .await?;
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
        permit.visit().await?;
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
        let (w, h, projected) =
            projected_ports(definition, pose.rotation, pose.position, &permit).await?;
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
                permit.visit().await?;
                let point = Point { x, y };
                prepare_set_insert(&mut body, &point, &permit).await?;
                if !body.insert(point) {
                    return Err(error("overlap", pose.block.clone(), "Block bodies overlap"));
                }
            }
        }
        ports[i] = Some(projected);
    }
    permit
        .reserve(
            problem
                .terminals
                .len()
                .saturating_mul(std::mem::size_of::<Option<Point>>()),
        )
        .await?;
    let mut terminal_positions = vec![None; problem.terminals.len()];
    let mut occupied_terminal_positions = HashSet::new();
    let mut terminal_at = HashMap::new();
    for pose in &solution.terminal_poses {
        permit.visit().await?;
        let Some(&i) = terminals.get(pose.terminal.as_str()) else {
            return Err(error(
                "terminal-pose",
                pose.terminal.clone(),
                "Terminal pose references an unknown boundary terminal",
            ));
        };
        prepare_set_insert(&mut occupied_terminal_positions, &pose.position, &permit).await?;
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
        prepare_map_insert(&mut terminal_at, &pose.position, &permit).await?;
        terminal_at.insert(pose.position, i);
    }
    let mut connection_ids = HashMap::new();
    permit
        .reserve((blocks.len() + terminals.len()).saturating_mul(std::mem::size_of::<usize>()))
        .await?;
    let mut parent: Vec<_> = (0..blocks.len() + terminals.len()).collect();
    permit
        .reserve((blocks.len() + terminals.len()).saturating_mul(std::mem::size_of::<usize>()))
        .await?;
    let mut component_size = vec![1usize; parent.len()];
    permit
        .reserve(
            blocks
                .len()
                .saturating_mul(2 * std::mem::size_of::<usize>()),
        )
        .await?;
    let mut incoming = vec![0; blocks.len()];
    let mut outgoing = vec![0; blocks.len()];
    let mut transports = [0; 2];
    for (i, connection) in problem.connections.iter().enumerate() {
        permit.visit().await?;
        prepare_map_insert(&mut connection_ids, &connection.id.as_str(), &permit).await?;
        let source_block = blocks.get(connection.source.as_str()).copied();
        let target_block = blocks.get(connection.target.as_str()).copied();
        let source_terminal = terminals.get(connection.source.as_str()).copied();
        let target_terminal = terminals.get(connection.target.as_str()).copied();
        let source = source_block.or_else(|| source_terminal.map(|index| blocks.len() + index));
        let target = target_block.or_else(|| target_terminal.map(|index| blocks.len() + index));
        if !stable_id_resumable(&connection.id, &permit).await?
            || !stable_id_resumable(&connection.commodity, &permit).await?
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
        let a = root_with_visit(&parent, s, &permit).await?;
        let b = root_with_visit(&parent, t, &permit).await?;
        if a != b {
            if component_size[a] < component_size[b] {
                parent[a] = b;
                component_size[b] += component_size[a];
            } else {
                parent[b] = a;
                component_size[a] += component_size[b];
            }
        }
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
    permit
        .reserve(
            problem
                .connections
                .len()
                .saturating_mul(std::mem::size_of::<usize>()),
        )
        .await?;
    let mut route_indices = vec![0; problem.connections.len()];
    let mut total_cells = 0usize;
    let mut turns = 0u64;
    let mut distance_sum = 0i64;
    let mut detour_sum = 0f64;
    for (ri, route) in solution.routes.iter().enumerate() {
        permit.visit().await?;
        let Some(&ci) = connection_ids.get(route.connection.as_str()) else {
            return Err(error(
                "route",
                route.connection.clone(),
                "Route references an unknown obligation",
            ));
        };
        prepare_set_insert(&mut routed, &ci, &permit).await?;
        if !routed.insert(ci) || route.cells.is_empty() {
            return Err(error(
                "route",
                route.connection.clone(),
                "Duplicate route or empty path",
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
                let port =
                    find_projected_port(ports[bi].as_ref().unwrap(), port_id, &permit).await?;
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
                let key = (bi, port_id);
                prepare_map_insert(&mut selected_ports, &key, &permit).await?;
                let entry = selected_ports.entry(key).or_insert_with(|| {
                    (
                        connection.commodity.as_str(),
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
            permit.visit().await?;
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
            prepare_set_insert(&mut local, &p, &permit).await?;
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
            let key = (connection.transport, p);
            if let Some(users) = visits.get_mut(&key) {
                prepare_vec_push(users, &permit).await?;
                users.push((ci, pi));
            } else {
                prepare_map_insert(&mut visits, &key, &permit).await?;
                let users = visits.entry(key).or_default();
                prepare_vec_push(users, &permit).await?;
                users.push((ci, pi));
            }
            include(p);
        }
        let local_bytes = hash_table_bytes(local.capacity(), std::mem::size_of::<Point>());
        drop(local);
        permit.release(local_bytes).await;
        for window in route.cells.windows(3) {
            permit.visit().await?;
            if direction(window[0], window[1]) != direction(window[1], window[2]) {
                turns += 1;
            }
        }
        let distance = route.cells[0].distance(*route.cells.last().unwrap());
        distance_sum += distance;
        detour_sum += (route.cells.len() - 1) as f64 / distance.max(1) as f64;
    }
    let mut bridge_sites = HashSet::new();
    let mut bridge_ids = HashSet::new();
    for bridge in &solution.bridges {
        permit.visit().await?;
        let Some(definition) = catalogs.components.component(&bridge.component) else {
            return Err(error(
                "bridge",
                bridge.id.clone(),
                "Unknown bridge definition",
            ));
        };
        prepare_set_insert(&mut bridge_ids, &bridge.id.as_str(), &permit).await?;
        let site = (definition.transport, bridge.position);
        prepare_set_insert(&mut bridge_sites, &site, &permit).await?;
        if !stable_id_resumable(&bridge.id, &permit).await?
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
        let mut actual = [HashSet::new(), HashSet::new()];
        let mut load = [Rate::zero(), Rate::zero()];
        let mut directions = [None, None];
        for &(ci, pi) in users {
            permit.visit().await?;
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
            prepare_set_insert(
                &mut actual[axis],
                &problem.connections[ci].id.as_str(),
                &permit,
            )
            .await?;
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
            let mut named = HashSet::new();
            for connection in &channel.connections {
                permit.visit().await?;
                prepare_set_insert(&mut named, &connection.as_str(), &permit).await?;
                named.insert(connection.as_str());
            }
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
            let named_bytes = hash_table_bytes(named.capacity(), std::mem::size_of::<&str>());
            drop(named);
            permit.release(named_bytes).await;
        }
        let actual_bytes = actual
            .iter()
            .map(|owners| hash_table_bytes(owners.capacity(), std::mem::size_of::<&str>()))
            .sum();
        drop(actual);
        permit.release(actual_bytes).await;
    }
    let mut junction_sites = HashMap::new();
    let mut junction_ids = HashSet::new();
    for junction in &solution.junctions {
        permit.visit().await?;
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
        prepare_set_insert(&mut junction_ids, &junction.id.as_str(), &permit).await?;
        let site = (definition.transport, junction.position);
        prepare_map_insert(&mut junction_sites, &site, &permit).await?;
        if !stable_id_resumable(&junction.id, &permit).await?
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
        permit.visit().await?;
        if bridge_sites.contains(&(transport, point)) {
            if junction_sites.contains_key(&(transport, point)) {
                return Err(error("junction", "routes", "Junction overlaps bridge"));
            }
            continue;
        }
        if users.len() == 1 && !junction_sites.contains_key(&(transport, point)) {
            continue;
        }
        let arm_bytes = users
            .len()
            .saturating_mul(std::mem::size_of::<SiteArmUse<'_>>());
        permit.reserve(arm_bytes).await?;
        let mut arm_uses = Vec::with_capacity(users.len());
        for &(ci, pi) in users {
            permit.visit().await?;
            let connection = &problem.connections[ci];
            let route = &solution.routes[route_indices[ci]];
            let mut external_incoming = false;
            let mut external_outgoing = false;
            let incoming = if pi > 0 {
                direction(point, route.cells[pi - 1])
            } else if let Some(&bi) = blocks.get(connection.source.as_str()) {
                let port = find_projected_port(
                    ports[bi].as_ref().unwrap(),
                    route.source_port.as_deref().unwrap(),
                    &permit,
                )
                .await?
                .expect("route port checked before topology");
                direction(point, port.position)
            } else {
                external_incoming = true;
                None
            };
            let outgoing = if pi + 1 < route.cells.len() {
                direction(point, route.cells[pi + 1])
            } else if let Some(&bi) = blocks.get(connection.target.as_str()) {
                let port = find_projected_port(
                    ports[bi].as_ref().unwrap(),
                    route.target_port.as_deref().unwrap(),
                    &permit,
                )
                .await?
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
        let (required, junction_bytes) =
            junction_options_resumable(transport, point, &arm_uses, &catalogs, &permit).await?;
        let selected: Option<&&Junction> = junction_sites.get(&(transport, point));
        match (required, selected) {
            (None, None) => {}
            (Some(options), Some(junction)) => {
                let mut named = HashSet::new();
                for connection in &junction.connections {
                    permit.visit().await?;
                    prepare_set_insert(&mut named, &connection.as_str(), &permit).await?;
                    named.insert(connection.as_str());
                }
                let mut actual = HashSet::new();
                for &(ci, _) in users {
                    permit.visit().await?;
                    prepare_set_insert(&mut actual, &problem.connections[ci].id.as_str(), &permit)
                        .await?;
                    actual.insert(problem.connections[ci].id.as_str());
                }
                let mut selected_option = false;
                for (component, rotation) in &options {
                    permit.visit().await?;
                    if component == &junction.component && *rotation == junction.rotation {
                        selected_option = true;
                        break;
                    }
                }
                if named.len() != junction.connections.len() || named != actual || !selected_option
                {
                    return Err(error(
                        "junction",
                        junction.id.clone(),
                        "Junction ownership, catalog geometry or flow does not match route arms",
                    ));
                }
                let owner_bytes = hash_table_bytes(named.capacity(), std::mem::size_of::<&str>())
                    + hash_table_bytes(actual.capacity(), std::mem::size_of::<&str>());
                drop(named);
                drop(actual);
                permit.release(owner_bytes).await;
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
        permit.release(junction_bytes).await;
        permit.release(arm_bytes).await;
    }
    let mut unmatched_junction = junction_sites.len() != solution.junctions.len();
    for site in junction_sites.keys() {
        permit.visit().await?;
        unmatched_junction |= !visits.contains_key(site);
    }
    if unmatched_junction {
        return Err(error(
            "junction",
            "layout",
            "Junction has no matching routed site",
        ));
    }
    let width = hi.x - lo.x + 1;
    let height = hi.y - lo.y + 1;
    for position in &terminal_positions {
        permit.visit().await?;
        if position.as_ref().is_some_and(|position| {
            position.x != lo.x && position.x != hi.x && position.y != lo.y && position.y != hi.y
        }) {
            return Err(error(
                "terminal-boundary",
                "terminals",
                "Every external terminal must lie on the final used layout boundary",
            ));
        }
    }
    let area = (width as u64) * (height as u64);
    let mut unique_transport = HashSet::new();
    for (_, point) in visits.keys() {
        permit.visit().await?;
        prepare_set_insert(&mut unique_transport, point, &permit).await?;
        unique_transport.insert(*point);
    }
    let mut roots = HashSet::new();
    for index in 0..parent.len() {
        permit.visit().await?;
        let root = root_with_visit(&parent, index, &permit).await?;
        prepare_set_insert(&mut roots, &root, &permit).await?;
        roots.insert(root);
    }
    let mut branch_blocks = 0;
    let mut merge_blocks = 0;
    let mut max_degree = 0;
    for (&incoming_count, &outgoing_count) in incoming.iter().zip(&outgoing) {
        permit.visit().await?;
        branch_blocks += usize::from(outgoing_count > 1);
        merge_blocks += usize::from(incoming_count > 1);
        max_degree = max_degree.max(incoming_count + outgoing_count);
    }
    Ok(Metrics {
        width,
        height,
        area,
        body_cells: body.len() as u64,
        transport_cells: visits.len() as u64,
        turns,
        occupancy: (body.len() + unique_transport.len()) as f64 / area as f64,
        weak_components: roots.len(),
        branch_blocks,
        merge_blocks,
        max_degree,
        mean_port_distance: distance_sum as f64 / problem.connections.len().max(1) as f64,
        mean_detour: detour_sum / problem.connections.len().max(1) as f64,
        belt_connections: transports[0],
        pipe_connections: transports[1],
        bridges: solution.bridges.len(),
    })
}

async fn root_with_visit(parent: &[usize], mut i: usize, permit: &Permit) -> Result<usize> {
    while parent[i] != i {
        permit.visit().await?;
        i = parent[i];
    }
    Ok(i)
}

#[cfg(test)]
mod reservation_tests {
    use super::*;

    #[test]
    fn hash_growth_reservation_covers_real_capacity_steps() {
        let mut values = HashSet::new();
        for index in 0..50_000_u64 {
            if values.len() == values.capacity() {
                let predicted =
                    next_hash_table_bytes(values.capacity(), std::mem::size_of::<u64>());
                values.reserve(1);
                let actual = hash_table_bytes(values.capacity(), std::mem::size_of::<u64>());
                assert!(predicted >= actual);
            }
            values.insert(index);
        }
    }
}
