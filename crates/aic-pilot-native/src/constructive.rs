//! Direct, router-free construction of facility poses and physical paths.
//!
//! This first research boundary intentionally supports pairwise routes only. The
//! policy owns every pose, port and path segment; this module applies commands,
//! performs local checks and invokes independent validation only at completion.

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Point, layer},
    physical::{Layout, Metrics, Pose, Problem, Route, SCHEMA, Solution},
    projection::{Port, rotate},
    stable_id::is_stable_id,
    validation::{Catalogs, validate, within_capacity},
};
use serde::{Deserialize, Serialize};

const COORDINATE_MAX: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_turns: u64,
    pub max_extension: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    East,
    South,
    West,
    North,
}

impl Direction {
    fn offset(self) -> (i64, i64) {
        match self {
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
            Self::North => (0, -1),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Command {
    Place {
        block: String,
        position: Point,
        rotation: i64,
    },
    BeginRoute {
        connection: String,
        source_port: String,
        target_port: String,
    },
    Extend {
        direction: Direction,
        length: i64,
    },
    Connect,
    AbortRoute,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActiveRoute {
    pub connection: usize,
    pub source_port: String,
    pub target_port: String,
    pub target: Point,
    pub cells: Vec<Point>,
}

/// Explicit Markov focus for the policy decoder. It survives route completion
/// so the next query can attend to work related to the most recent command.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Focus {
    Block {
        block: usize,
        position: Point,
    },
    Route {
        connection: usize,
        endpoint: Point,
        target: Point,
        active: bool,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Observation {
    pub revision: u64,
    pub turns: u64,
    pub max_turns: u64,
    pub poses: Vec<Option<Pose>>,
    pub routes: Vec<Option<Route>>,
    pub active: Option<ActiveRoute>,
    pub focus: Option<Focus>,
    pub completed: usize,
    pub total: usize,
    pub completion_ratio: f64,
    pub fully_validated: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Step {
    pub accepted: bool,
    pub turn: u64,
    pub revision: u64,
    pub progress_before: f64,
    pub progress_after: f64,
    pub progress_delta: f64,
    pub terminal: bool,
    pub fully_validated: bool,
    pub diagnostic: Option<Diagnostic>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: &'static str,
    pub stage: &'static str,
    pub message: String,
    pub entity: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} [{}]: {}",
            self.code, self.entity, self.message
        )
    }
}

impl std::error::Error for Diagnostic {}

fn error(code: &str, entity: impl Into<String>, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: format!("constructive.{code}"),
        severity: "error",
        stage: "direct-construction",
        message: message.into(),
        entity: entity.into(),
    }
}

/// Router-free construction state. All indices are stable problem order.
pub struct Session {
    problem: Problem,
    catalogs: Arc<Catalogs>,
    limits: Limits,
    blocks: BTreeMap<String, usize>,
    connections: BTreeMap<String, usize>,
    poses: Vec<Option<Pose>>,
    routes: Vec<Option<Route>>,
    active: Option<ActiveRoute>,
    active_cells: HashSet<Point>,
    focus: Option<Focus>,
    bodies: HashSet<Point>,
    occupied: [HashSet<Point>; 2],
    used_ports: HashSet<(usize, String)>,
    turns: u64,
    revision: u64,
    metrics: Option<Metrics>,
    final_failure: Option<Diagnostic>,
}

impl Session {
    pub fn new(
        problem: Problem,
        catalogs: Arc<Catalogs>,
        limits: Limits,
    ) -> Result<Self, Diagnostic> {
        if !(1..=10_000_000).contains(&limits.max_turns)
            || !(1..=COORDINATE_MAX).contains(&limits.max_extension)
        {
            return Err(error(
                "limits",
                "limits",
                "turn and extension limits are outside the supported work domain",
            ));
        }
        if catalogs.identity.len() != 64
            || !catalogs
                .identity
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(error(
                "catalog-identity",
                "catalogs",
                "catalog identity must be a 64-digit hexadecimal digest",
            ));
        }
        if problem.blocks.is_empty()
            || problem.blocks.len() > 4096
            || problem.connections.is_empty()
            || problem.connections.len() > 32_768
        {
            return Err(error(
                "problem-size",
                "problem",
                "the first constructive domain requires blocks and connections within guards",
            ));
        }
        let mut blocks = BTreeMap::new();
        for (index, block) in problem.blocks.iter().enumerate() {
            if !is_stable_id(&block.id)
                || blocks.insert(block.id.clone(), index).is_some()
                || catalogs.facilities.facility(&block.facility).is_none()
            {
                return Err(error(
                    "block",
                    &block.id,
                    "block ID is invalid or repeated, or its facility is absent",
                ));
            }
        }
        let mut connections = BTreeMap::new();
        for (index, connection) in problem.connections.iter().enumerate() {
            let source = blocks.get(&connection.source).copied();
            let target = blocks.get(&connection.target).copied();
            let capacity = catalogs.transports.capacity(connection.transport);
            if !is_stable_id(&connection.id)
                || !is_stable_id(&connection.commodity)
                || connections.insert(connection.id.clone(), index).is_some()
                || source.is_none()
                || target.is_none()
                || source == target
                || !within_capacity(connection.rate, capacity.quantity, capacity.duration_ms)
            {
                return Err(error(
                    "connection",
                    &connection.id,
                    "connection identity, endpoints, commodity or rate are invalid",
                ));
            }
            for (block, direction) in [
                (source.unwrap(), FacilityPortDirection::Output),
                (target.unwrap(), FacilityPortDirection::Input),
            ] {
                let definition = catalogs
                    .facilities
                    .facility(&problem.blocks[block].facility)
                    .expect("blocks checked above");
                if !definition.ports.iter().any(|port| {
                    port.direction == direction && port.transport == connection.transport
                }) {
                    return Err(error(
                        "connection-domain",
                        &connection.id,
                        "an endpoint has no compatible facility port",
                    ));
                }
            }
        }
        let block_count = problem.blocks.len();
        let connection_count = problem.connections.len();
        Ok(Self {
            problem,
            catalogs,
            limits,
            blocks,
            connections,
            poses: vec![None; block_count],
            routes: vec![None; connection_count],
            active: None,
            active_cells: HashSet::new(),
            focus: None,
            bodies: HashSet::new(),
            occupied: [HashSet::new(), HashSet::new()],
            used_ports: HashSet::new(),
            turns: 0,
            revision: 0,
            metrics: None,
            final_failure: None,
        })
    }

    pub fn observe(&self) -> Observation {
        Observation {
            revision: self.revision,
            turns: self.turns,
            max_turns: self.limits.max_turns,
            poses: self.poses.clone(),
            routes: self.routes.clone(),
            active: self.active.clone(),
            focus: self.focus.clone(),
            completed: self.completed(),
            total: self.routes.len(),
            completion_ratio: self.progress(),
            fully_validated: self.metrics.is_some(),
        }
    }

    pub fn metrics(&self) -> Option<&Metrics> {
        self.metrics.as_ref()
    }

    pub fn problem(&self) -> &Problem {
        &self.problem
    }

    pub fn layout(&self) -> Option<Layout> {
        self.metrics.as_ref().map(|_| self.complete_layout())
    }

    pub fn done(&self) -> bool {
        self.metrics.is_some()
            || self.final_failure.is_some()
            || self.turns >= self.limits.max_turns
    }

    pub fn step(&mut self, command: Command) -> Step {
        let before = self.progress();
        if self.done() {
            return Step {
                accepted: false,
                turn: self.turns,
                revision: self.revision,
                progress_before: before,
                progress_after: before,
                progress_delta: 0.0,
                terminal: true,
                fully_validated: self.metrics.is_some(),
                diagnostic: Some(error(
                    "episode-closed",
                    "episode",
                    "no command is accepted after completion, failure or turn exhaustion",
                )),
            };
        }
        self.turns += 1;
        let outcome = match command {
            Command::Place {
                block,
                position,
                rotation,
            } => self.place(&block, position, rotation),
            Command::BeginRoute {
                connection,
                source_port,
                target_port,
            } => self.begin_route(&connection, &source_port, &target_port),
            Command::Extend { direction, length } => self.extend(direction, length),
            Command::Connect => self.connect(),
            Command::AbortRoute => self.abort_route(),
        };
        let accepted = outcome.is_ok();
        if accepted {
            self.revision += 1;
            if self.is_complete() {
                match validate(&self.complete_layout(), &self.catalogs) {
                    Ok(metrics) => self.metrics = Some(metrics),
                    Err(failure) => {
                        self.final_failure = Some(error(
                            "final-validation",
                            failure.entity.to_string(),
                            failure.message,
                        ));
                    }
                }
            }
        }
        let after = self.progress();
        let diagnostic = outcome.err().or_else(|| self.final_failure.clone());
        Step {
            accepted,
            turn: self.turns,
            revision: self.revision,
            progress_before: before,
            progress_after: after,
            progress_delta: after - before,
            terminal: self.done(),
            fully_validated: self.metrics.is_some(),
            diagnostic,
        }
    }

    fn progress(&self) -> f64 {
        self.completed() as f64 / self.routes.len() as f64
    }

    fn completed(&self) -> usize {
        self.routes.iter().filter(|route| route.is_some()).count()
    }

    fn is_complete(&self) -> bool {
        self.active.is_none()
            && self.poses.iter().all(Option::is_some)
            && self.routes.iter().all(Option::is_some)
    }

    fn complete_layout(&self) -> Layout {
        Layout {
            schema: SCHEMA.into(),
            catalog_sha256: self.catalogs.identity.clone(),
            problem: self.problem.clone(),
            solution: Solution {
                poses: self.poses.iter().flatten().cloned().collect(),
                routes: self.routes.iter().flatten().cloned().collect(),
                bridges: vec![],
            },
        }
    }

    fn place(&mut self, block: &str, position: Point, rotation: i64) -> Result<(), Diagnostic> {
        if self.active.is_some() {
            return Err(error(
                "active-route",
                block,
                "finish or abort the active route before placing another facility",
            ));
        }
        let Some(&index) = self.blocks.get(block) else {
            return Err(error(
                "unknown-block",
                block,
                "place references an unknown block",
            ));
        };
        if self.poses[index].is_some() {
            return Err(error("placed-block", block, "block is already placed"));
        }
        let definition = self
            .catalogs
            .facilities
            .facility(&self.problem.blocks[index].facility)
            .expect("problem admission checked facilities");
        if !definition.allowed_rotations.contains(&rotation) {
            return Err(error(
                "rotation",
                block,
                "rotation is outside the facility catalog domain",
            ));
        }
        let (width, height, _) = rotate(definition, rotation, position);
        if width > 1000
            || height > 1000
            || width.checked_mul(height).is_none_or(|area| area > 10_000)
        {
            return Err(error(
                "body-guard",
                block,
                "facility body exceeds the first constructive materialization guard",
            ));
        }
        let maximum = position
            .x
            .checked_add(width - 1)
            .zip(position.y.checked_add(height - 1));
        if position.x < 0
            || position.y < 0
            || maximum.is_none_or(|(x, y)| x > COORDINATE_MAX || y > COORDINATE_MAX)
        {
            return Err(error(
                "coordinate",
                block,
                "facility body leaves the supported coordinate domain",
            ));
        }
        let mut cells = Vec::with_capacity((width * height) as usize);
        for y in position.y..position.y + height {
            for x in position.x..position.x + width {
                let cell = Point { x, y };
                if self.bodies.contains(&cell)
                    || self
                        .occupied
                        .iter()
                        .any(|occupied| occupied.contains(&cell))
                {
                    return Err(error(
                        "placement-collision",
                        block,
                        "facility body intersects existing physical geometry",
                    ));
                }
                cells.push(cell);
            }
        }
        self.bodies.extend(cells);
        self.poses[index] = Some(Pose {
            block: block.into(),
            position,
            rotation,
        });
        self.focus = Some(Focus::Block {
            block: index,
            position,
        });
        Ok(())
    }

    fn projected_ports(&self, block: usize) -> Result<Vec<Port>, Diagnostic> {
        let pose = self.poses[block].as_ref().ok_or_else(|| {
            error(
                "unplaced-endpoint",
                &self.problem.blocks[block].id,
                "route endpoint facility is not placed",
            )
        })?;
        let definition = self
            .catalogs
            .facilities
            .facility(&self.problem.blocks[block].facility)
            .expect("problem admission checked facilities");
        Ok(rotate(definition, pose.rotation, pose.position).2)
    }

    fn selected_port(
        &self,
        block: usize,
        id: &str,
        expected: FacilityPortDirection,
        transport: aic_layout::logistics::TransportKind,
        connection: &str,
    ) -> Result<Port, Diagnostic> {
        let port = self
            .projected_ports(block)?
            .into_iter()
            .find(|port| port.id == id)
            .ok_or_else(|| error("unknown-port", connection, "selected port does not exist"))?;
        if port.direction != expected || port.transport != transport {
            return Err(error(
                "incompatible-port",
                connection,
                "selected port has the wrong direction or transport kind",
            ));
        }
        if self.used_ports.contains(&(block, id.to_owned())) {
            return Err(error(
                "used-port",
                connection,
                "selected facility port already belongs to a completed route",
            ));
        }
        Ok(port)
    }

    fn begin_route(
        &mut self,
        connection: &str,
        source_port: &str,
        target_port: &str,
    ) -> Result<(), Diagnostic> {
        if self.active.is_some() {
            return Err(error(
                "active-route",
                connection,
                "only one route may be active in the first constructive domain",
            ));
        }
        let Some(&index) = self.connections.get(connection) else {
            return Err(error(
                "unknown-connection",
                connection,
                "begin-route references an unknown logical connection",
            ));
        };
        if self.routes[index].is_some() {
            return Err(error(
                "completed-connection",
                connection,
                "logical connection already has a completed route",
            ));
        }
        let obligation = &self.problem.connections[index];
        let source_block = self.blocks[&obligation.source];
        let target_block = self.blocks[&obligation.target];
        let source = self.selected_port(
            source_block,
            source_port,
            FacilityPortDirection::Output,
            obligation.transport,
            connection,
        )?;
        let target = self.selected_port(
            target_block,
            target_port,
            FacilityPortDirection::Input,
            obligation.transport,
            connection,
        )?;
        if self.bodies.contains(&source.connection) || self.bodies.contains(&target.connection) {
            return Err(error(
                "blocked-port",
                connection,
                "a selected port connection cell is covered by a facility body",
            ));
        }
        let occupied = &self.occupied[layer(obligation.transport)];
        if occupied.contains(&source.connection) || occupied.contains(&target.connection) {
            return Err(error(
                "occupied-port",
                connection,
                "a selected port connection cell is occupied on the same layer",
            ));
        }
        self.active = Some(ActiveRoute {
            connection: index,
            source_port: source_port.into(),
            target_port: target_port.into(),
            target: target.connection,
            cells: vec![source.connection],
        });
        self.active_cells.clear();
        self.active_cells.insert(source.connection);
        self.focus = Some(Focus::Route {
            connection: index,
            endpoint: source.connection,
            target: target.connection,
            active: true,
        });
        Ok(())
    }

    fn extend(&mut self, direction: Direction, length: i64) -> Result<(), Diagnostic> {
        if !(1..=self.limits.max_extension).contains(&length) {
            return Err(error(
                "extension-length",
                "active-route",
                "straight extension length is outside the configured action domain",
            ));
        }
        let active = self.active.as_ref().ok_or_else(|| {
            error(
                "missing-active-route",
                "active-route",
                "extend requires an active route",
            )
        })?;
        let transport = self.problem.connections[active.connection].transport;
        let occupied = &self.occupied[layer(transport)];
        let focus_connection = active.connection;
        let focus_target = active.target;
        let (dx, dy) = direction.offset();
        let mut point = *active
            .cells
            .last()
            .expect("active route always has a source");
        let mut added = Vec::with_capacity(length as usize);
        let mut new_cells = HashSet::with_capacity(length as usize);
        for _ in 0..length {
            point = Point {
                x: point.x.checked_add(dx).ok_or_else(|| {
                    error(
                        "coordinate",
                        "active-route",
                        "extension coordinate overflows",
                    )
                })?,
                y: point.y.checked_add(dy).ok_or_else(|| {
                    error(
                        "coordinate",
                        "active-route",
                        "extension coordinate overflows",
                    )
                })?,
            };
            if !(0..=COORDINATE_MAX).contains(&point.x) || !(0..=COORDINATE_MAX).contains(&point.y)
            {
                return Err(error(
                    "coordinate",
                    "active-route",
                    "extension leaves the supported coordinate domain",
                ));
            }
            if self.bodies.contains(&point) {
                return Err(error(
                    "path-body",
                    "active-route",
                    "extension intersects a facility body",
                ));
            }
            if self.active_cells.contains(&point) || !new_cells.insert(point) {
                return Err(error(
                    "path-repeat",
                    "active-route",
                    "extension revisits its active route",
                ));
            }
            if occupied.contains(&point) {
                return Err(error(
                    "same-layer-intersection",
                    "active-route",
                    "same-layer crossings are outside the first constructive domain",
                ));
            }
            added.push(point);
        }
        self.active
            .as_mut()
            .expect("active route checked above")
            .cells
            .extend(added.iter().copied());
        self.active_cells.extend(added);
        self.focus = Some(Focus::Route {
            connection: focus_connection,
            endpoint: point,
            target: focus_target,
            active: true,
        });
        Ok(())
    }

    fn connect(&mut self) -> Result<(), Diagnostic> {
        let active = self.active.as_ref().ok_or_else(|| {
            error(
                "missing-active-route",
                "active-route",
                "connect requires an active route",
            )
        })?;
        if active.cells.last() != Some(&active.target) {
            return Err(error(
                "wrong-endpoint",
                &self.problem.connections[active.connection].id,
                "active path does not end at its selected target port",
            ));
        }
        let active = self.active.take().expect("active route checked above");
        self.active_cells.clear();
        let obligation = &self.problem.connections[active.connection];
        let source_block = self.blocks[&obligation.source];
        let target_block = self.blocks[&obligation.target];
        let route = Route {
            connection: obligation.id.clone(),
            source_port: active.source_port.clone(),
            target_port: active.target_port.clone(),
            cells: active.cells,
        };
        self.occupied[layer(obligation.transport)].extend(route.cells.iter().copied());
        self.used_ports.insert((source_block, active.source_port));
        self.used_ports.insert((target_block, active.target_port));
        self.routes[active.connection] = Some(route);
        self.focus = Some(Focus::Route {
            connection: active.connection,
            endpoint: active.target,
            target: active.target,
            active: false,
        });
        Ok(())
    }

    fn abort_route(&mut self) -> Result<(), Diagnostic> {
        let Some(active) = self.active.take() else {
            return Err(error(
                "missing-active-route",
                "active-route",
                "abort-route requires an active route",
            ));
        };
        self.active_cells.clear();
        self.focus = Some(Focus::Route {
            connection: active.connection,
            endpoint: *active.cells.last().expect("active route has a source"),
            target: active.target,
            active: false,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_layout::{
        facilities::{FacilityCatalog, ValidatedFacilityCatalog},
        logistics::{
            LogisticsComponentCatalog, TransportKind, ValidatedLogisticsComponentCatalog,
            ValidatedTransportCatalog,
        },
        physical::{Block, Connection},
        rate::Rate,
    };
    use serde::de::DeserializeOwned;
    use std::{fs::File, path::Path};

    fn read<T: DeserializeOwned>(root: &Path, name: &str) -> T {
        serde_json::from_reader(File::open(root.join(name)).unwrap()).unwrap()
    }

    fn catalogs() -> Arc<Catalogs> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized");
        Arc::new(Catalogs {
            identity: "1".repeat(64),
            facilities: ValidatedFacilityCatalog::try_from_catalog(read::<FacilityCatalog>(
                &root,
                "facilities.json",
            ))
            .unwrap(),
            transports: ValidatedTransportCatalog::try_from_catalog(read(&root, "transports.json"))
                .unwrap(),
            components: ValidatedLogisticsComponentCatalog::try_from_catalog(read::<
                LogisticsComponentCatalog,
            >(
                &root,
                "logistics-components.json",
            ))
            .unwrap(),
        })
    }

    fn problem() -> Problem {
        Problem {
            blocks: ["source", "target"]
                .into_iter()
                .map(|id| Block {
                    id: id.into(),
                    facility: "grinder-1-mode-normal".into(),
                })
                .collect(),
            connections: vec![Connection {
                id: "connection-0".into(),
                source: "source".into(),
                target: "target".into(),
                commodity: "test-item".into(),
                transport: TransportKind::Belt,
                rate: Rate {
                    numerator: 1,
                    denominator: 2,
                },
            }],
        }
    }

    fn session() -> Session {
        Session::new(
            problem(),
            catalogs(),
            Limits {
                max_turns: 32,
                max_extension: 16,
            },
        )
        .unwrap()
    }

    fn place_pair(session: &mut Session) {
        assert!(
            session
                .step(Command::Place {
                    block: "source".into(),
                    position: Point { x: 5, y: 8 },
                    rotation: 0,
                })
                .accepted
        );
        assert!(
            session
                .step(Command::Place {
                    block: "target".into(),
                    position: Point { x: 5, y: 2 },
                    rotation: 0,
                })
                .accepted
        );
    }

    fn begin(session: &mut Session) -> Step {
        session.step(Command::BeginRoute {
            connection: "connection-0".into(),
            source_port: "output-belt-1".into(),
            target_port: "input-belt-1".into(),
        })
    }

    #[test]
    fn authored_commands_complete_without_a_router_and_pay_progress_once() {
        let mut session = session();
        place_pair(&mut session);
        assert!(begin(&mut session).accepted);
        let observation = session.observe();
        let active = observation.active.unwrap();
        assert_eq!(active.connection, 0);
        assert_eq!(active.cells, vec![Point { x: 6, y: 7 }]);
        assert_eq!(active.target, Point { x: 6, y: 5 });
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 2,
                })
                .accepted
        );
        let completed = session.step(Command::Connect);
        assert!(completed.accepted && completed.terminal && completed.fully_validated);
        assert_eq!(completed.progress_before, 0.0);
        assert_eq!(completed.progress_after, 1.0);
        assert_eq!(completed.progress_delta, 1.0);
        assert_eq!(
            session.observe().focus,
            Some(Focus::Route {
                connection: 0,
                endpoint: Point { x: 6, y: 5 },
                target: Point { x: 6, y: 5 },
                active: false,
            })
        );
        assert!(session.metrics().is_some());
        assert!(session.layout().is_some());
    }

    #[test]
    fn rejected_extension_is_atomic_and_abort_releases_provisional_ports() {
        let mut session = session();
        place_pair(&mut session);
        assert!(begin(&mut session).accepted);
        let before = session.observe().active.unwrap().cells;
        let rejected = session.step(Command::Extend {
            direction: Direction::South,
            length: 1,
        });
        assert!(!rejected.accepted);
        assert_eq!(rejected.diagnostic.unwrap().code, "constructive.path-body");
        assert_eq!(session.observe().active.unwrap().cells, before);
        assert!(session.step(Command::AbortRoute).accepted);
        assert!(session.observe().active.is_none());
        assert!(begin(&mut session).accepted);
    }

    #[test]
    fn connect_rejects_wrong_endpoint_without_committing_progress() {
        let mut session = session();
        place_pair(&mut session);
        assert!(begin(&mut session).accepted);
        let rejected = session.step(Command::Connect);
        assert!(!rejected.accepted);
        assert_eq!(
            rejected.diagnostic.unwrap().code,
            "constructive.wrong-endpoint"
        );
        assert_eq!(session.observe().completion_ratio, 0.0);
        assert!(session.observe().active.is_some());
    }

    #[test]
    fn incompatible_port_is_rejected_without_starting_a_route() {
        let mut session = session();
        place_pair(&mut session);
        let rejected = session.step(Command::BeginRoute {
            connection: "connection-0".into(),
            source_port: "input-belt-1".into(),
            target_port: "input-belt-1".into(),
        });
        assert!(!rejected.accepted);
        assert_eq!(
            rejected.diagnostic.unwrap().code,
            "constructive.incompatible-port"
        );
        assert!(session.observe().active.is_none());
    }

    #[test]
    fn completed_port_cannot_be_reused_by_another_obligation() {
        let mut problem = problem();
        problem.blocks.push(Block {
            id: "other-target".into(),
            facility: "grinder-1-mode-normal".into(),
        });
        problem.connections.push(Connection {
            id: "connection-1".into(),
            source: "source".into(),
            target: "other-target".into(),
            commodity: "other-item".into(),
            transport: TransportKind::Belt,
            rate: Rate {
                numerator: 1,
                denominator: 2,
            },
        });
        let mut session = Session::new(
            problem,
            catalogs(),
            Limits {
                max_turns: 32,
                max_extension: 16,
            },
        )
        .unwrap();
        place_pair(&mut session);
        assert!(
            session
                .step(Command::Place {
                    block: "other-target".into(),
                    position: Point { x: 10, y: 2 },
                    rotation: 0,
                })
                .accepted
        );
        assert!(begin(&mut session).accepted);
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 2,
                })
                .accepted
        );
        assert!(session.step(Command::Connect).accepted);
        let rejected = session.step(Command::BeginRoute {
            connection: "connection-1".into(),
            source_port: "output-belt-1".into(),
            target_port: "input-belt-1".into(),
        });
        assert!(!rejected.accepted);
        assert_eq!(rejected.diagnostic.unwrap().code, "constructive.used-port");
        assert_eq!(session.observe().completion_ratio, 0.5);
    }

    #[test]
    fn same_layer_crossing_is_rejected_atomically() {
        let problem = Problem {
            blocks: ["west", "east", "south", "north"]
                .into_iter()
                .map(|id| Block {
                    id: id.into(),
                    facility: "grinder-1-mode-normal".into(),
                })
                .collect(),
            connections: vec![
                Connection {
                    id: "horizontal".into(),
                    source: "west".into(),
                    target: "east".into(),
                    commodity: "horizontal-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                },
                Connection {
                    id: "vertical".into(),
                    source: "south".into(),
                    target: "north".into(),
                    commodity: "vertical-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                },
            ],
        };
        let mut session = Session::new(
            problem,
            catalogs(),
            Limits {
                max_turns: 32,
                max_extension: 16,
            },
        )
        .unwrap();
        for (block, position, rotation) in [
            ("west", Point { x: 2, y: 5 }, 90),
            ("east", Point { x: 8, y: 5 }, 90),
            ("south", Point { x: 5, y: 9 }, 0),
            ("north", Point { x: 5, y: 1 }, 0),
        ] {
            assert!(
                session
                    .step(Command::Place {
                        block: block.into(),
                        position,
                        rotation,
                    })
                    .accepted
            );
        }
        assert!(
            session
                .step(Command::BeginRoute {
                    connection: "horizontal".into(),
                    source_port: "output-belt-1".into(),
                    target_port: "input-belt-1".into(),
                })
                .accepted
        );
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::East,
                    length: 2,
                })
                .accepted
        );
        assert!(session.step(Command::Connect).accepted);
        assert!(
            session
                .step(Command::BeginRoute {
                    connection: "vertical".into(),
                    source_port: "output-belt-1".into(),
                    target_port: "input-belt-1".into(),
                })
                .accepted
        );
        let before = session.observe().active.unwrap().cells;
        let rejected = session.step(Command::Extend {
            direction: Direction::North,
            length: 4,
        });
        assert!(!rejected.accepted);
        assert_eq!(
            rejected.diagnostic.unwrap().code,
            "constructive.same-layer-intersection"
        );
        assert_eq!(session.observe().active.unwrap().cells, before);
        assert_eq!(session.observe().completion_ratio, 0.5);
    }
}
