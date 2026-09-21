//! Direct, router-free construction of facility poses and physical paths.
//!
//! This first research boundary intentionally supports pairwise routes only. The
//! policy owns every pose, port and path segment; this module applies commands,
//! performs local checks and invokes independent validation only at completion.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use aic_layout::rate::Rate;
use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Point, layer},
    physical::{
        Layout, Metrics, Pose, Problem, Route, SCHEMA, Solution, TerminalDirection, TerminalPose,
    },
    projection::{Port, rotate},
    stable_id::is_stable_id,
    validation::{Catalogs, validate, within_capacity},
};
use serde::{Deserialize, Serialize};

pub mod learning;

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
    PlaceTerminal {
        terminal: String,
        position: Point,
    },
    BeginRoute {
        connection: String,
        source_port: Option<String>,
        target_port: Option<String>,
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
    pub source_port: Option<String>,
    pub target_port: Option<String>,
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
    Terminal {
        terminal: usize,
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
    pub terminal_poses: Vec<Option<TerminalPose>>,
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
///
/// Search forks this state at a decision boundary and then advances the fork
/// through a deterministic command path. Catalog data remains shared through
/// its `Arc`; mutable geometry is copied so simulations cannot affect the
/// committed session.
#[derive(Clone)]
pub struct Session {
    problem: Problem,
    catalogs: Arc<Catalogs>,
    limits: Limits,
    blocks: BTreeMap<String, usize>,
    terminals: BTreeMap<String, usize>,
    connections: BTreeMap<String, usize>,
    poses: Vec<Option<Pose>>,
    terminal_poses: Vec<Option<TerminalPose>>,
    routes: Vec<Option<Route>>,
    active: Option<ActiveRoute>,
    active_cells: HashSet<Point>,
    focus: Option<Focus>,
    bodies: HashSet<Point>,
    occupied: [HashMap<Point, Vec<usize>>; 2],
    used_ports: HashMap<(usize, String), UsedPort>,
    turns: u64,
    revision: u64,
    metrics: Option<Metrics>,
    final_failure: Option<Diagnostic>,
}

#[derive(Clone)]
struct UsedPort {
    commodity: String,
    transport: aic_layout::logistics::TransportKind,
    direction: FacilityPortDirection,
    rate: Rate,
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
            || problem.terminals.len() > 32_768
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
        let mut terminals = BTreeMap::new();
        for (index, terminal) in problem.terminals.iter().enumerate() {
            if !is_stable_id(&terminal.id)
                || !is_stable_id(&terminal.commodity)
                || blocks.contains_key(&terminal.id)
                || terminals.insert(terminal.id.clone(), index).is_some()
            {
                return Err(error(
                    "terminal",
                    &terminal.id,
                    "terminal ID or commodity is invalid/repeated or collides with a block",
                ));
            }
        }
        let mut connections = BTreeMap::new();
        for (index, connection) in problem.connections.iter().enumerate() {
            let source_block = blocks.get(&connection.source).copied();
            let target_block = blocks.get(&connection.target).copied();
            let source_terminal = terminals.get(&connection.source).copied();
            let target_terminal = terminals.get(&connection.target).copied();
            let capacity = catalogs.transports.capacity(connection.transport);
            if !is_stable_id(&connection.id)
                || !is_stable_id(&connection.commodity)
                || connections.insert(connection.id.clone(), index).is_some()
                || source_block.is_none() && source_terminal.is_none()
                || target_block.is_none() && target_terminal.is_none()
                || source_terminal.is_some() && target_terminal.is_some()
                || connection.source == connection.target
                || !within_capacity(connection.rate, capacity.quantity, capacity.duration_ms)
            {
                return Err(error(
                    "connection",
                    &connection.id,
                    "connection identity, endpoints, commodity or rate are invalid",
                ));
            }
            for (block, direction) in [
                (source_block, FacilityPortDirection::Output),
                (target_block, FacilityPortDirection::Input),
            ] {
                let Some(block) = block else { continue };
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
            for (terminal, direction) in [
                (source_terminal, TerminalDirection::Input),
                (target_terminal, TerminalDirection::Output),
            ] {
                if terminal.is_some_and(|terminal| {
                    let terminal = &problem.terminals[terminal];
                    terminal.direction != direction
                        || terminal.commodity != connection.commodity
                        || terminal.transport != connection.transport
                }) {
                    return Err(error(
                        "terminal-connection",
                        &connection.id,
                        "terminal role, commodity or transport does not match its connection",
                    ));
                }
            }
        }
        let block_count = problem.blocks.len();
        let terminal_count = problem.terminals.len();
        let connection_count = problem.connections.len();
        Ok(Self {
            problem,
            catalogs,
            limits,
            blocks,
            terminals,
            connections,
            poses: vec![None; block_count],
            terminal_poses: vec![None; terminal_count],
            routes: vec![None; connection_count],
            active: None,
            active_cells: HashSet::new(),
            focus: None,
            bodies: HashSet::new(),
            occupied: std::array::from_fn(|_| HashMap::new()),
            used_ports: HashMap::new(),
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
            terminal_poses: self.terminal_poses.clone(),
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

    /// Manhattan distance from the open end of the provisional route to its
    /// fixed target. Search uses this lightweight signal without cloning the
    /// full public observation.
    pub fn active_route_manhattan_distance(&self) -> Option<u64> {
        self.active.as_ref().map(|active| {
            let endpoint = active
                .cells
                .last()
                .expect("active route always contains its source cell");
            endpoint.x.abs_diff(active.target.x) + endpoint.y.abs_diff(active.target.y)
        })
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
        let before = self.construction_progress();
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
            Command::PlaceTerminal { terminal, position } => {
                self.place_terminal(&terminal, position)
            }
            Command::BeginRoute {
                connection,
                source_port,
                target_port,
            } => self.begin_route(&connection, source_port.as_deref(), target_port.as_deref()),
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
        let after = self.construction_progress();
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

    /// Applies a command produced by a learning domain only while that domain
    /// still describes this exact state.
    pub fn step_selected(&mut self, selected: learning::SelectedCommand) -> Step {
        if selected.revision != self.revision {
            let progress = self.progress();
            return Step {
                accepted: false,
                turn: self.turns,
                revision: self.revision,
                progress_before: progress,
                progress_after: progress,
                progress_delta: 0.0,
                terminal: self.done(),
                fully_validated: self.metrics.is_some(),
                diagnostic: Some(error(
                    "stale-learning-domain",
                    "learning-domain",
                    "selected command belongs to an earlier constructive revision",
                )),
            };
        }
        self.step(selected.command)
    }

    fn progress(&self) -> f64 {
        self.completed() as f64 / self.routes.len() as f64
    }

    fn construction_progress(&self) -> f64 {
        let completed = self.poses.iter().filter(|pose| pose.is_some()).count()
            + self
                .terminal_poses
                .iter()
                .filter(|pose| pose.is_some())
                .count()
            + self.routes.iter().filter(|route| route.is_some()).count();
        let total = self.poses.len() + self.terminal_poses.len() + self.routes.len();
        completed as f64 / total as f64
    }

    fn completed(&self) -> usize {
        self.routes.iter().filter(|route| route.is_some()).count()
    }

    fn is_complete(&self) -> bool {
        self.active.is_none()
            && self.poses.iter().all(Option::is_some)
            && self.terminal_poses.iter().all(Option::is_some)
            && self.routes.iter().all(Option::is_some)
    }

    fn complete_layout(&self) -> Layout {
        Layout {
            schema: SCHEMA.into(),
            catalog_sha256: self.catalogs.identity.clone(),
            problem: self.problem.clone(),
            solution: Solution {
                poses: self.poses.iter().flatten().cloned().collect(),
                terminal_poses: self.terminal_poses.iter().flatten().cloned().collect(),
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
                        .any(|occupied| occupied.contains_key(&cell))
                    || self
                        .terminal_poses
                        .iter()
                        .flatten()
                        .any(|pose| pose.position == cell)
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

    fn place_terminal(&mut self, terminal: &str, position: Point) -> Result<(), Diagnostic> {
        if self.active.is_some() {
            return Err(error(
                "active-route",
                terminal,
                "finish or abort the active route before placing a boundary terminal",
            ));
        }
        let Some(&index) = self.terminals.get(terminal) else {
            return Err(error(
                "unknown-terminal",
                terminal,
                "place-terminal references an unknown boundary terminal",
            ));
        };
        if self.terminal_poses[index].is_some() {
            return Err(error(
                "placed-terminal",
                terminal,
                "boundary terminal is already placed",
            ));
        }
        if position.x < 0
            || position.y < 0
            || position.x > COORDINATE_MAX
            || position.y > COORDINATE_MAX
            || self.bodies.contains(&position)
            || self
                .occupied
                .iter()
                .any(|occupied| occupied.contains_key(&position))
            || self
                .terminal_poses
                .iter()
                .flatten()
                .any(|pose| pose.position == position)
        {
            return Err(error(
                "terminal-position",
                terminal,
                "boundary terminal coordinate is invalid or occupied",
            ));
        }
        self.terminal_poses[index] = Some(TerminalPose {
            terminal: terminal.into(),
            position,
        });
        self.focus = Some(Focus::Terminal {
            terminal: index,
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
        commodity: &str,
        rate: Rate,
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
        if let Some(used) = self.used_ports.get(&(block, id.to_owned())) {
            let total = used.rate.checked_add(rate).map_err(|_| {
                error(
                    "port-capacity",
                    connection,
                    "shared facility port rate overflow",
                )
            })?;
            let capacity = self.catalogs.transports.capacity(transport);
            if used.commodity != commodity
                || used.transport != transport
                || used.direction != expected
                || !within_capacity(total, capacity.quantity, capacity.duration_ms)
            {
                return Err(error(
                    "used-port",
                    connection,
                    "selected facility port cannot carry this additional material flow",
                ));
            }
        }
        Ok(port)
    }

    fn route_endpoint(
        &self,
        endpoint: &str,
        selected_port: Option<&str>,
        expected: FacilityPortDirection,
        obligation: &aic_layout::physical::Connection,
    ) -> Result<(Point, Option<(usize, String)>), Diagnostic> {
        if let Some(&block) = self.blocks.get(endpoint) {
            let id = selected_port.ok_or_else(|| {
                error(
                    "missing-port",
                    &obligation.id,
                    "facility route endpoint requires a selected port",
                )
            })?;
            let port = self.selected_port(
                block,
                id,
                expected,
                obligation.transport,
                &obligation.commodity,
                obligation.rate,
                &obligation.id,
            )?;
            return Ok((port.connection, Some((block, id.to_owned()))));
        }
        let terminal = self.terminals[endpoint];
        if selected_port.is_some() {
            return Err(error(
                "terminal-port",
                &obligation.id,
                "boundary terminal route endpoint must not select a facility port",
            ));
        }
        let position = self.terminal_poses[terminal]
            .as_ref()
            .ok_or_else(|| {
                error(
                    "unplaced-terminal",
                    endpoint,
                    "route endpoint boundary terminal is not placed",
                )
            })?
            .position;
        Ok((position, None))
    }

    fn shared_cell_available(&self, point: Point, connection: usize) -> bool {
        let obligation = &self.problem.connections[connection];
        if self
            .terminal_poses
            .iter()
            .enumerate()
            .any(|(terminal, pose)| {
                pose.as_ref().is_some_and(|pose| pose.position == point)
                    && self.problem.terminals[terminal].id != obligation.source
                    && self.problem.terminals[terminal].id != obligation.target
            })
        {
            return false;
        }
        let users = self.occupied[layer(obligation.transport)].get(&point);
        let Some(users) = users else { return true };
        let mut load = obligation.rate;
        for &other in users {
            let existing = &self.problem.connections[other];
            if existing.commodity != obligation.commodity {
                return false;
            }
            let Some(total) = load.checked_add(existing.rate).ok() else {
                return false;
            };
            load = total;
        }
        let capacity = self.catalogs.transports.capacity(obligation.transport);
        within_capacity(load, capacity.quantity, capacity.duration_ms)
    }

    fn begin_route(
        &mut self,
        connection: &str,
        source_port: Option<&str>,
        target_port: Option<&str>,
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
        let (source, _) = self.route_endpoint(
            &obligation.source,
            source_port,
            FacilityPortDirection::Output,
            obligation,
        )?;
        let (target, _) = self.route_endpoint(
            &obligation.target,
            target_port,
            FacilityPortDirection::Input,
            obligation,
        )?;
        if self.bodies.contains(&source) || self.bodies.contains(&target) {
            return Err(error(
                "blocked-port",
                connection,
                "a selected port connection cell is covered by a facility body",
            ));
        }
        if !self.shared_cell_available(source, index) || !self.shared_cell_available(target, index)
        {
            return Err(error(
                "occupied-port",
                connection,
                "a selected port connection cell is occupied on the same layer",
            ));
        }
        self.active = Some(ActiveRoute {
            connection: index,
            source_port: source_port.map(str::to_owned),
            target_port: target_port.map(str::to_owned),
            target,
            cells: vec![source],
        });
        self.active_cells.clear();
        self.active_cells.insert(source);
        self.focus = Some(Focus::Route {
            connection: index,
            endpoint: source,
            target,
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
            if !self.shared_cell_available(point, focus_connection) {
                return Err(error(
                    "same-layer-intersection",
                    "active-route",
                    "same-layer cell is incompatible or lacks residual shared capacity",
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
        let route = Route {
            connection: obligation.id.clone(),
            source_port: active.source_port.clone(),
            target_port: active.target_port.clone(),
            cells: active.cells,
        };
        for &cell in &route.cells {
            self.occupied[layer(obligation.transport)]
                .entry(cell)
                .or_default()
                .push(active.connection);
        }
        for (endpoint, port, direction) in [
            (
                &obligation.source,
                active.source_port.as_ref(),
                FacilityPortDirection::Output,
            ),
            (
                &obligation.target,
                active.target_port.as_ref(),
                FacilityPortDirection::Input,
            ),
        ] {
            let (Some(&block), Some(port)) = (self.blocks.get(endpoint), port) else {
                continue;
            };
            let entry = self
                .used_ports
                .entry((block, port.clone()))
                .or_insert_with(|| UsedPort {
                    commodity: obligation.commodity.clone(),
                    transport: obligation.transport,
                    direction,
                    rate: Rate::zero(),
                });
            entry.rate = entry
                .rate
                .checked_add(obligation.rate)
                .expect("begin-route checked shared port rate");
        }
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
        physical::{Block, Connection, Terminal},
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
            terminals: vec![],
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

    fn learning_config() -> learning::Config {
        learning::Config {
            placement: aic_layout::geometry::Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 15,
                max_y: 15,
            },
            extension_lengths: vec![1, 2, 4],
        }
    }

    fn begin(session: &mut Session) -> Step {
        session.step(Command::BeginRoute {
            connection: "connection-0".into(),
            source_port: Some("output-belt-1".into()),
            target_port: Some("input-belt-1".into()),
        })
    }

    #[test]
    fn route_learning_domain_exposes_only_the_selected_connection() {
        let mut session = session();
        place_pair(&mut session);
        let observation = session
            .route_learning_observation(&learning_config(), 0)
            .unwrap();
        let command_choices = observation.domain.choices(&learning::Cursor::Command);
        assert_eq!(command_choices.len(), 1);
        assert_eq!(
            command_choices[0].features[learning::CommandKind::BeginRoute as usize],
            1.0
        );

        let learning::Advance::Continue(learning::Cursor::RouteConnection) = observation
            .domain
            .advance(&learning::Cursor::Command, 0)
            .unwrap()
        else {
            panic!("route option did not begin with connection selection");
        };
        let connections = observation
            .domain
            .choices(&learning::Cursor::RouteConnection);
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].references[0], 2);

        assert!(begin(&mut session).accepted);
        let active = session
            .route_learning_observation_with_abort(&learning_config(), 0, false)
            .unwrap();
        let active_choices = active.domain.choices(&learning::Cursor::Command);
        assert!(
            active_choices
                .iter()
                .any(|choice| choice.features[learning::CommandKind::Extend as usize] == 1.0)
        );
        assert!(
            active_choices
                .iter()
                .all(|choice| choice.features[learning::CommandKind::AbortRoute as usize] == 0.0)
        );
    }

    #[test]
    fn learning_command_pages_preserve_native_factor_traces() {
        let session = session();
        let observation = session.learning_observation(&learning_config()).unwrap();
        assert_eq!(
            observation.domain.available_command_kinds(),
            vec![learning::CommandKind::Place]
        );
        let first = observation
            .domain
            .command_page(learning::CommandKind::Place, 0, 3);
        let second =
            observation
                .domain
                .command_page(learning::CommandKind::Place, first.next_offset, 3);
        assert_eq!(first.commands.len(), 3);
        assert_eq!(second.commands.len(), 3);
        assert_eq!(first.next_offset, 3);
        assert_eq!(second.next_offset, 6);
        assert!(first.commands.iter().all(|left| {
            second
                .commands
                .iter()
                .all(|right| left.command.command != right.command.command)
        }));

        for factored in first.commands.into_iter().chain(second.commands) {
            let mut expected = learning::Cursor::Command;
            let mut completed = None;
            for factor in factored.factors {
                assert_eq!(factor.cursor, expected);
                match observation
                    .domain
                    .advance(&factor.cursor, factor.selected)
                    .unwrap()
                {
                    learning::Advance::Continue(cursor) => expected = cursor,
                    learning::Advance::Command(command) => completed = Some(command),
                }
            }
            assert_eq!(completed, Some(factored.command.clone()));
            let mut fork = session.clone();
            assert!(fork.step_selected(factored.command).accepted);
        }
    }

    #[test]
    fn authored_commands_complete_without_a_router_and_finish_construction_progress() {
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
        assert!((completed.progress_before - 2.0 / 3.0).abs() < f64::EPSILON);
        assert_eq!(completed.progress_after, 1.0);
        assert!((completed.progress_delta - 1.0 / 3.0).abs() < f64::EPSILON);
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
    fn external_terminals_and_capacity_bounded_shared_flow_validate() {
        let problem = Problem {
            blocks: vec![Block {
                id: "machine".into(),
                facility: "grinder-1-mode-normal".into(),
            }],
            terminals: vec![
                Terminal {
                    id: "supply".into(),
                    direction: TerminalDirection::Input,
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                },
                Terminal {
                    id: "sink-a".into(),
                    direction: TerminalDirection::Output,
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                },
                Terminal {
                    id: "sink-b".into(),
                    direction: TerminalDirection::Output,
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                },
            ],
            connections: vec![
                Connection {
                    id: "supply-flow".into(),
                    source: "supply".into(),
                    target: "machine".into(),
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                },
                Connection {
                    id: "output-a".into(),
                    source: "machine".into(),
                    target: "sink-a".into(),
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 4,
                    },
                },
                Connection {
                    id: "output-b".into(),
                    source: "machine".into(),
                    target: "sink-b".into(),
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 4,
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
        for command in [
            Command::Place {
                block: "machine".into(),
                position: Point { x: 5, y: 8 },
                rotation: 0,
            },
            Command::PlaceTerminal {
                terminal: "supply".into(),
                position: Point { x: 6, y: 15 },
            },
            Command::PlaceTerminal {
                terminal: "sink-a".into(),
                position: Point { x: 4, y: 0 },
            },
            Command::PlaceTerminal {
                terminal: "sink-b".into(),
                position: Point { x: 8, y: 0 },
            },
        ] {
            assert!(session.step(command).accepted);
        }
        assert!(
            session
                .step(Command::BeginRoute {
                    connection: "supply-flow".into(),
                    source_port: None,
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 4,
                })
                .accepted
        );
        assert!(session.step(Command::Connect).accepted);
        for (connection, horizontal) in
            [("output-a", Direction::West), ("output-b", Direction::East)]
        {
            assert!(
                session
                    .step(Command::BeginRoute {
                        connection: connection.into(),
                        source_port: Some("output-belt-1".into()),
                        target_port: None,
                    })
                    .accepted
            );
            for (direction, length) in [
                (Direction::North, 3),
                (horizontal, 2),
                (Direction::North, 4),
            ] {
                assert!(session.step(Command::Extend { direction, length }).accepted);
            }
            assert!(session.step(Command::Connect).accepted);
        }
        assert!(session.done());
        assert!(session.metrics().is_some());
        assert_eq!(session.observe().completed, 3);
    }

    #[test]
    fn learning_domain_factorizes_commands_and_rejects_stale_selections() {
        let mut session = session();
        let observation = session.learning_observation(&learning_config()).unwrap();
        let command_choices = observation.domain.choices(&learning::Cursor::Command);
        assert_eq!(command_choices.len(), 1);
        assert_eq!(command_choices[0].features[0], 1.0);
        assert_eq!(command_choices[0].features[6], 0.0);
        let learning::Advance::Continue(blocks) = observation
            .domain
            .advance(&learning::Cursor::Command, 0)
            .unwrap()
        else {
            panic!("place command should continue to its block factor")
        };
        let block_choices = observation.domain.choices(&blocks);
        assert_eq!(block_choices.len(), 2);
        assert_eq!(block_choices[0].features[6], 1.0);
        assert_eq!(block_choices[0].features[5], 0.0);
        let learning::Advance::Continue(poses) = observation.domain.advance(&blocks, 0).unwrap()
        else {
            panic!("block choice should continue to its pose factor")
        };
        let learning::Advance::Command(selected) = observation.domain.advance(&poses, 0).unwrap()
        else {
            panic!("pose choice should complete a command")
        };
        assert!(matches!(&selected.command, Command::Place { block, .. } if block == "source"));
        assert!(session.step_selected(selected.clone()).accepted);
        let stale = session.step_selected(selected);
        assert!(!stale.accepted);
        assert_eq!(
            stale.diagnostic.unwrap().code,
            "constructive.stale-learning-domain"
        );
    }

    #[test]
    fn learning_input_keeps_exact_route_cells_and_persistent_focus() {
        let mut session = session();
        place_pair(&mut session);
        assert!(begin(&mut session).accepted);
        let observation = session.learning_observation(&learning_config()).unwrap();
        assert_eq!(observation.input.focus_entity, 2);
        assert_eq!(observation.input.focus_features[2], 1.0);
        assert_eq!(observation.input.focus_features[3], 1.0);
        assert!(
            observation
                .input
                .spatial_features
                .iter()
                .any(|row| row[6] == 1.0)
        );
        assert!(
            observation
                .input
                .spatial_features
                .iter()
                .any(|row| row[7] == 1.0)
        );
        // Extend and abort are the only locally legal command families here.
        assert_eq!(
            observation.domain.choices(&learning::Cursor::Command).len(),
            2
        );
        let learning::Advance::Command(abort) = observation
            .domain
            .advance(&learning::Cursor::Command, 1)
            .unwrap()
        else {
            panic!("abort has no conditional factor")
        };
        assert!(session.step_selected(abort).accepted);
        assert!(matches!(
            session.observe().focus,
            Some(Focus::Route { active: false, .. })
        ));
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
            source_port: Some("input-belt-1".into()),
            target_port: Some("input-belt-1".into()),
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
            source_port: Some("output-belt-1".into()),
            target_port: Some("input-belt-1".into()),
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
            terminals: vec![],
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
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
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
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
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
