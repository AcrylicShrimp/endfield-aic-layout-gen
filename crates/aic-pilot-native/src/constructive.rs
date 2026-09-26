//! Direct, router-free construction of facility poses and physical paths.
//!
//! This first research boundary intentionally supports pairwise routes only. The
//! policy owns every pose, port and path segment; this module applies commands,
//! performs local checks and invokes independent validation only at completion.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
    time::Instant,
};

use aic_layout::rate::Rate;
use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Point, direction as point_direction, layer},
    logistics::LogisticsComponentKind,
    physical::{
        Bridge, BridgeAxis, BridgeChannel, Junction, Layout, Metrics, Pose, Problem, Route, SCHEMA,
        Solution, TerminalDirection, TerminalPose,
    },
    projection::{Port, cardinal_index, project_port, rotate},
    stable_id::is_stable_id,
    validation::{Catalogs, SiteArmUse, junction_options_with_visit, validate, within_capacity},
};
use serde::{Deserialize, Serialize};

pub mod learning;
pub mod routing_candidates;

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
    ReserveBridge {
        position: Point,
        component: String,
        rotation: i64,
        crossed_axis: BridgeAxis,
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
    pub bridge_reservations: Vec<BridgeReservation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BridgeReservation {
    pub position: Point,
    pub component: String,
    pub rotation: i64,
    pub crossed_axis: BridgeAxis,
    pub direction: usize,
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
    pub junctions: Vec<Junction>,
    pub bridges: Vec<Bridge>,
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
    terminal_at: HashMap<Point, usize>,
    routes: Vec<Option<Route>>,
    junctions: HashMap<(aic_layout::logistics::TransportKind, Point), Junction>,
    bridges: HashMap<(aic_layout::logistics::TransportKind, Point), Bridge>,
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
    replay_deadline: Option<Instant>,
    replay_meter: Option<Arc<ReplayMeter>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplayWork {
    RouteCells = 0,
    Users = 1,
    Catalog = 2,
    Validation = 3,
}

struct ReplayMeter {
    limits: [usize; 4],
    used: [AtomicUsize; 4],
    stopped: AtomicU8,
    visits: AtomicUsize,
}

impl ReplayMeter {
    fn new(limits: [usize; 4]) -> Self {
        Self {
            limits,
            used: std::array::from_fn(|_| AtomicUsize::new(0)),
            stopped: AtomicU8::new(0),
            visits: AtomicUsize::new(0),
        }
    }
    fn usage(&self) -> [usize; 4] {
        std::array::from_fn(|index| self.used[index].load(Ordering::Relaxed))
    }
    fn stopped(&self) -> Option<ReplayWork> {
        match self.stopped.load(Ordering::Relaxed) {
            1 => Some(ReplayWork::RouteCells),
            2 => Some(ReplayWork::Users),
            3 => Some(ReplayWork::Catalog),
            4 => Some(ReplayWork::Validation),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct UsedPort {
    commodity: String,
    transport: aic_layout::logistics::TransportKind,
    direction: FacilityPortDirection,
    rate: Rate,
}

impl Session {
    /// Conservative reservation for one independently mutable search fork.
    /// Catalog tables are shared through Arc and are not copied.
    pub fn clone_reservation_bytes(&self) -> usize {
        let route_cells = self
            .routes
            .iter()
            .flatten()
            .map(|route| route.cells.len())
            .sum::<usize>();
        let occupied_users = self
            .occupied
            .iter()
            .flat_map(|layer| layer.values())
            .map(Vec::len)
            .sum::<usize>();
        let records = self
            .problem
            .blocks
            .len()
            .saturating_add(self.problem.terminals.len())
            .saturating_add(self.problem.connections.len().saturating_mul(3))
            .saturating_add(self.bodies.len())
            .saturating_add(self.terminal_at.len())
            .saturating_add(self.junctions.len().saturating_mul(4))
            .saturating_add(self.bridges.len().saturating_mul(8))
            .saturating_add(route_cells.saturating_mul(2))
            .saturating_add(occupied_users.saturating_mul(2))
            .saturating_add(self.used_ports.len().saturating_mul(3));
        let string_bytes = self
            .problem
            .blocks
            .iter()
            .map(|block| block.id.len() + block.facility.len())
            .sum::<usize>()
            .saturating_add(
                self.problem
                    .terminals
                    .iter()
                    .map(|terminal| terminal.id.len())
                    .sum::<usize>(),
            )
            .saturating_add(
                self.problem
                    .connections
                    .iter()
                    .map(|connection| {
                        connection.id.len()
                            + connection.source.len()
                            + connection.target.len()
                            + connection.commodity.len()
                    })
                    .sum::<usize>(),
            );
        std::mem::size_of::<Self>()
            .saturating_add(records.saturating_mul(512))
            .saturating_add(string_bytes.saturating_mul(8))
    }

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
            terminal_at: HashMap::new(),
            routes: vec![None; connection_count],
            junctions: HashMap::new(),
            bridges: HashMap::new(),
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
            replay_deadline: None,
            replay_meter: None,
        })
    }

    fn check_replay_deadline(&self) -> Result<(), Diagnostic> {
        if self
            .replay_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(error(
                "route-request-deadline",
                "route-candidate",
                "request deadline reached during native replay",
            ));
        }
        Ok(())
    }

    fn charge_replay(&self, kind: ReplayWork, count: usize) -> Result<(), Diagnostic> {
        let Some(meter) = &self.replay_meter else {
            return Ok(());
        };
        let index = kind as usize;
        let used = meter.used[index]
            .load(Ordering::Relaxed)
            .saturating_add(count);
        if used > meter.limits[index] {
            meter.stopped.store(index as u8 + 1, Ordering::Relaxed);
            return Err(error(
                "route-request-work-limit",
                "route-candidate",
                format!("native replay {:?} visit limit reached", kind),
            ));
        }
        meter.used[index].store(used, Ordering::Relaxed);
        let visits = meter
            .visits
            .fetch_add(count, Ordering::Relaxed)
            .saturating_add(count);
        if visits / 256 != visits.saturating_sub(count) / 256 {
            self.check_replay_deadline()?;
        }
        Ok(())
    }

    pub fn observe(&self) -> Observation {
        Observation {
            revision: self.revision,
            turns: self.turns,
            max_turns: self.limits.max_turns,
            poses: self.poses.clone(),
            terminal_poses: self.terminal_poses.clone(),
            routes: self.routes.clone(),
            junctions: {
                let mut junctions = self.junctions.values().cloned().collect::<Vec<_>>();
                junctions.sort_by(|a, b| a.id.cmp(&b.id));
                junctions
            },
            bridges: {
                let mut bridges = self.bridges.values().cloned().collect::<Vec<_>>();
                bridges.sort_by(|a, b| a.id.cmp(&b.id));
                bridges
            },
            active: self.active.clone(),
            focus: self.focus.clone(),
            completed: self.completed(),
            total: self.routes.len(),
            completion_ratio: self.progress(),
            fully_validated: self.metrics.is_some(),
        }
    }

    /// Read the immutable state identity without cloning a public observation.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Cheap macro-state checks used before preparing route requests.
    pub fn has_active_route(&self) -> bool {
        self.active.is_some()
    }

    pub fn route_is_complete(&self, index: usize) -> bool {
        self.routes[index].is_some()
    }

    pub fn has_unplaced_entities(&self) -> bool {
        self.poses.iter().any(Option::is_none) || self.terminal_poses.iter().any(Option::is_none)
    }

    pub fn metrics(&self) -> Option<&Metrics> {
        self.metrics.as_ref()
    }

    pub fn problem(&self) -> &Problem {
        &self.problem
    }

    /// Stable catalog code and exact per-axis load for a committed bridge.
    /// The code is meaningful only with the input's catalog identity.
    pub fn bridge_component_features(&self, bridge: &Bridge) -> (f32, [f32; 2]) {
        let mut total = 0usize;
        let mut ordinal = 0usize;
        for component in &self.catalogs.components.catalog().components {
            if component.kind == LogisticsComponentKind::Bridge {
                total += 1;
                if component.id == bridge.component {
                    ordinal = total;
                }
            }
        }
        let definition = self
            .catalogs
            .components
            .component(&bridge.component)
            .expect("committed bridge has a catalog component");
        let mut loads = [0.0; 2];
        for (axis, channel) in bridge.channels.iter().enumerate() {
            let mut rate = Rate::zero();
            for owner in &channel.connections {
                let ci = self.connections[owner];
                rate = rate
                    .checked_add(self.problem.connections[ci].rate)
                    .expect("committed bridge axis has a representable rate");
            }
            loads[axis] = ((rate.numerator as f64 / rate.denominator as f64)
                * definition.capacity.duration_ms as f64
                / (1000.0 * definition.capacity.quantity as f64))
                .min(1.0) as f32;
        }
        (ordinal as f32 / (total + 1) as f32, loads)
    }

    pub fn bridge_at(
        &self,
        transport: aic_layout::logistics::TransportKind,
        position: Point,
    ) -> Option<&Bridge> {
        self.bridges.get(&(transport, position))
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
            Command::ReserveBridge {
                position,
                component,
                rotation,
                crossed_axis,
            } => self.reserve_bridge(position, &component, rotation, crossed_axis),
            Command::Connect => self.connect(),
            Command::AbortRoute => self.abort_route(),
        };
        let accepted = outcome.is_ok();
        if accepted {
            self.revision += 1;
            if self.is_complete() {
                let layout = if self.replay_meter.is_some() {
                    self.complete_layout_bounded()
                } else {
                    Ok(self.complete_layout())
                };
                match layout {
                    Ok(layout) => {
                        let validation = if self.replay_meter.is_some() {
                            aic_layout::validation::validate_with_visit(
                                &layout,
                                &self.catalogs,
                                || {
                                    self.charge_replay(ReplayWork::Validation, 1).map_err(
                                        |failure| {
                                            aic_layout::validation::error(
                                                "request-work-limit",
                                                "final-validation",
                                                failure.message,
                                            )
                                        },
                                    )
                                },
                            )
                        } else {
                            validate(&layout, &self.catalogs)
                        };
                        match validation {
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
                    Err(failure) => self.final_failure = Some(failure),
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
                bridges: {
                    let mut bridges = self.bridges.values().cloned().collect::<Vec<_>>();
                    bridges.sort_by(|a, b| a.id.cmp(&b.id));
                    bridges
                },
                junctions: {
                    let mut junctions = self.junctions.values().cloned().collect::<Vec<_>>();
                    junctions.sort_by(|a, b| a.id.cmp(&b.id));
                    junctions
                },
            },
        }
    }

    fn complete_layout_bounded(&self) -> Result<Layout, Diagnostic> {
        let mut blocks = Vec::with_capacity(self.problem.blocks.len());
        for block in &self.problem.blocks {
            self.charge_replay(ReplayWork::Validation, 1)?;
            blocks.push(block.clone());
        }
        let mut terminals = Vec::with_capacity(self.problem.terminals.len());
        for terminal in &self.problem.terminals {
            self.charge_replay(ReplayWork::Validation, 1)?;
            terminals.push(terminal.clone());
        }
        let mut connections = Vec::with_capacity(self.problem.connections.len());
        for connection in &self.problem.connections {
            self.charge_replay(ReplayWork::Validation, 1)?;
            connections.push(connection.clone());
        }
        let mut poses = Vec::with_capacity(self.poses.len());
        for pose in self.poses.iter().flatten() {
            self.charge_replay(ReplayWork::Validation, 1)?;
            poses.push(pose.clone());
        }
        let mut terminal_poses = Vec::with_capacity(self.terminal_poses.len());
        for pose in self.terminal_poses.iter().flatten() {
            self.charge_replay(ReplayWork::Validation, 1)?;
            terminal_poses.push(pose.clone());
        }
        let mut routes = Vec::with_capacity(self.routes.len());
        for route in self.routes.iter().flatten() {
            self.charge_replay(ReplayWork::Validation, 1)?;
            let mut cells = Vec::with_capacity(route.cells.len());
            for chunk in route.cells.chunks(256) {
                self.charge_replay(ReplayWork::Validation, chunk.len())?;
                cells.extend_from_slice(chunk);
            }
            routes.push(Route {
                connection: route.connection.clone(),
                source_port: route.source_port.clone(),
                target_port: route.target_port.clone(),
                cells,
            });
        }
        let mut bridges = Vec::with_capacity(self.bridges.len());
        for bridge in self.bridges.values() {
            self.charge_replay(
                ReplayWork::Validation,
                1 + bridge
                    .channels
                    .iter()
                    .map(|channel| channel.connections.len())
                    .sum::<usize>(),
            )?;
            bridges.push(bridge.clone());
        }
        self.check_replay_deadline()?;
        bridges.sort_by(|a, b| a.id.cmp(&b.id));
        let mut junctions = Vec::with_capacity(self.junctions.len());
        for junction in self.junctions.values() {
            self.charge_replay(ReplayWork::Validation, 1 + junction.connections.len())?;
            junctions.push(junction.clone());
        }
        self.check_replay_deadline()?;
        junctions.sort_by(|a, b| a.id.cmp(&b.id));
        self.check_replay_deadline()?;
        Ok(Layout {
            schema: SCHEMA.into(),
            catalog_sha256: self.catalogs.identity.clone(),
            problem: Problem {
                blocks,
                terminals,
                connections,
            },
            solution: Solution {
                poses,
                terminal_poses,
                routes,
                bridges,
                junctions,
            },
        })
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
                    || self.terminal_at.contains_key(&cell)
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
            || self.terminal_at.contains_key(&position)
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
        self.terminal_at.insert(position, index);
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
        let mut ports = Vec::with_capacity(definition.ports.len());
        for port in &definition.ports {
            self.charge_replay(ReplayWork::Catalog, 1)?;
            ports.push(project_port(definition, port, pose.rotation, pose.position));
        }
        Ok(ports)
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
        let mut selected = None;
        for port in &definition.ports {
            self.charge_replay(ReplayWork::Catalog, 1)?;
            if port.id == id {
                selected = Some(project_port(definition, port, pose.rotation, pose.position));
                break;
            }
        }
        let port = selected
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
        self.shared_cell_available_checked(point, connection, || Ok(()))
            .expect("unchecked availability has no fallible visits")
    }

    fn shared_cell_available_replay(
        &self,
        point: Point,
        connection: usize,
    ) -> Result<bool, Diagnostic> {
        self.shared_cell_available_checked(point, connection, || {
            self.charge_replay(ReplayWork::Users, 1)
        })
    }

    fn shared_cell_available_checked(
        &self,
        point: Point,
        connection: usize,
        mut visit: impl FnMut() -> Result<(), Diagnostic>,
    ) -> Result<bool, Diagnostic> {
        let obligation = &self.problem.connections[connection];
        if let Some(&terminal) = self.terminal_at.get(&point) {
            if self.problem.terminals[terminal].id != obligation.source
                && self.problem.terminals[terminal].id != obligation.target
            {
                return Ok(false);
            }
        }
        let users = self.occupied[layer(obligation.transport)].get(&point);
        let Some(users) = users else { return Ok(true) };
        let mut load = obligation.rate;
        for &other in users {
            visit()?;
            let existing = &self.problem.connections[other];
            if existing.commodity != obligation.commodity {
                return Ok(false);
            }
            let Some(total) = load.checked_add(existing.rate).ok() else {
                return Ok(false);
            };
            load = total;
        }
        let capacity = self.catalogs.transports.capacity(obligation.transport);
        Ok(within_capacity(
            load,
            capacity.quantity,
            capacity.duration_ms,
        ))
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
        if !self.shared_cell_available_replay(source, index)?
            || !self.shared_cell_available_replay(target, index)?
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
            bridge_reservations: vec![],
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

    fn reserve_bridge(
        &mut self,
        position: Point,
        component: &str,
        rotation: i64,
        crossed_axis: BridgeAxis,
    ) -> Result<(), Diagnostic> {
        let active = self.active.as_ref().ok_or_else(|| {
            error(
                "missing-active-route",
                "active-route",
                "bridge reservation requires an active route",
            )
        })?;
        let obligation = &self.problem.connections[active.connection];
        let previous = *active.cells.last().expect("active route has a source");
        let Some(direction) = point_direction(previous, position) else {
            return Err(error(
                "bridge-reservation",
                &obligation.id,
                "bridge must be reserved immediately before entering its adjacent cell",
            ));
        };
        let expected_axis = if direction % 2 == 0 {
            BridgeAxis::Vertical
        } else {
            BridgeAxis::Horizontal
        };
        if crossed_axis != expected_axis
            || self.bodies.contains(&position)
            || self.terminal_at.contains_key(&position)
            || self.active_cells.contains(&position)
            || self
                .junctions
                .contains_key(&(obligation.transport, position))
            || active
                .bridge_reservations
                .iter()
                .any(|reserved| reserved.position == position)
        {
            return Err(error(
                "bridge-reservation",
                &obligation.id,
                "bridge axis, site or reservation conflicts with occupied geometry",
            ));
        }
        let users = self.occupied[layer(obligation.transport)]
            .get(&position)
            .ok_or_else(|| {
                error(
                    "bridge-reservation",
                    &obligation.id,
                    "bridge site has no committed carrier axis",
                )
            })?;
        if let Some(installed) = self.bridges.get(&(obligation.transport, position)) {
            let axis = direction % 2;
            let channel = &installed.channels[axis];
            let definition = self
                .catalogs
                .components
                .component(&installed.component)
                .expect("installed bridge has a catalog component");
            let mut load = obligation.rate;
            let mut matches_axis = channel.commodity == obligation.commodity
                && installed.component == component
                && installed.rotation == rotation;
            for owner in &channel.connections {
                self.charge_replay(ReplayWork::Users, 1)?;
                let ci = self.connections[owner];
                let route = self.routes[ci]
                    .as_ref()
                    .expect("bridge owner route is committed");
                let mut pi = None;
                for (index, &cell) in route.cells.iter().enumerate() {
                    self.charge_replay(ReplayWork::RouteCells, 1)?;
                    if cell == position {
                        pi = Some(index);
                        break;
                    }
                }
                let pi = pi.expect("bridge owner visits its component");
                matches_axis &= pi > 0
                    && pi + 1 < route.cells.len()
                    && point_direction(route.cells[pi - 1], position) == Some(direction)
                    && point_direction(position, route.cells[pi + 1]) == Some(direction);
                load = load
                    .checked_add(self.problem.connections[ci].rate)
                    .map_err(|_| {
                        error(
                            "bridge-reservation",
                            &obligation.id,
                            "bridge axis rate overflows",
                        )
                    })?;
            }
            let line = self.catalogs.transports.capacity(obligation.transport);
            if !matches_axis
                || !within_capacity(
                    load,
                    definition.capacity.quantity,
                    definition.capacity.duration_ms,
                )
                || !within_capacity(load, line.quantity, line.duration_ms)
            {
                return Err(error(
                    "bridge-reservation",
                    &obligation.id,
                    "installed bridge axis has incompatible flow or insufficient residual capacity",
                ));
            }
            self.active
                .as_mut()
                .expect("active route checked above")
                .bridge_reservations
                .push(BridgeReservation {
                    position,
                    component: component.to_owned(),
                    rotation,
                    crossed_axis,
                    direction,
                });
            return Ok(());
        }
        let mut carrier_direction = None;
        let mut carrier_rate = Rate::zero();
        let carrier_commodity = &self.problem.connections[users[0]].commodity;
        for &ci in users {
            self.charge_replay(ReplayWork::Users, 1)?;
            let existing = &self.problem.connections[ci];
            let route = self.routes[ci]
                .as_ref()
                .expect("occupied route is committed");
            let mut pi = None;
            for (index, &cell) in route.cells.iter().enumerate() {
                self.charge_replay(ReplayWork::RouteCells, 1)?;
                if cell == position {
                    pi = Some(index);
                    break;
                }
            }
            let Some(pi) = pi else {
                return Err(error(
                    "bridge-reservation",
                    &obligation.id,
                    "carrier route does not visit the bridge site",
                ));
            };
            let heading = if pi > 0 && pi + 1 < route.cells.len() {
                point_direction(route.cells[pi - 1], position)
            } else {
                None
            };
            if existing.commodity != *carrier_commodity
                || heading.is_none()
                || point_direction(position, route.cells[pi + 1]) != heading
                || carrier_direction.is_some_and(|old| heading != Some(old))
            {
                return Err(error(
                    "bridge-reservation",
                    &obligation.id,
                    "carrier routes must form one same-commodity straight directed axis",
                ));
            }
            carrier_direction = heading;
            carrier_rate = carrier_rate.checked_add(existing.rate).map_err(|_| {
                error(
                    "bridge-reservation",
                    &obligation.id,
                    "carrier axis rate overflows",
                )
            })?;
        }
        let definition = self.catalogs.components.component(component);
        let legal = definition.is_some_and(|definition| {
            let Some(carrier_direction) = carrier_direction else {
                return false;
            };
            let rotated = |arm| (cardinal_index(arm) + rotation as usize / 90) % 4;
            definition.kind == LogisticsComponentKind::Bridge
                && definition.transport == obligation.transport
                && carrier_direction % 2 != direction % 2
                && definition.footprint.width == 1
                && definition.footprint.height == 1
                && definition.allowed_rotations.contains(&rotation)
                && within_capacity(
                    carrier_rate,
                    definition.capacity.quantity,
                    definition.capacity.duration_ms,
                )
                && within_capacity(
                    obligation.rate,
                    definition.capacity.quantity,
                    definition.capacity.duration_ms,
                )
                && [carrier_direction, direction].iter().all(|&heading| {
                    definition
                        .input_directions
                        .iter()
                        .any(|&arm| rotated(arm) == (heading + 2) % 4)
                        && definition
                            .output_directions
                            .iter()
                            .any(|&arm| rotated(arm) == heading)
                })
        });
        if !legal {
            return Err(error(
                "bridge-reservation",
                &obligation.id,
                "catalog bridge does not fit the carrier, new route, rotation or capacity",
            ));
        }
        self.active
            .as_mut()
            .expect("active route checked above")
            .bridge_reservations
            .push(BridgeReservation {
                position,
                component: component.to_owned(),
                rotation,
                crossed_axis,
                direction,
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
        let heading = match direction {
            Direction::East => 0,
            Direction::South => 1,
            Direction::West => 2,
            Direction::North => 3,
        };
        let mut point = *active
            .cells
            .last()
            .expect("active route always has a source");
        let mut added = Vec::with_capacity(length as usize);
        let mut new_cells = HashSet::with_capacity(length as usize);
        for step_index in 0..length {
            self.charge_replay(ReplayWork::RouteCells, 1)?;
            if step_index % 256 == 0 {
                self.check_replay_deadline()?;
            }
            if active
                .bridge_reservations
                .iter()
                .any(|reserved| reserved.position == point && reserved.direction != heading)
            {
                return Err(error(
                    "bridge-turn",
                    "active-route",
                    "route must leave a reserved bridge in its entry direction",
                ));
            }
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
            let pending = active.bridge_reservations.iter().find(|reserved| {
                !self.active_cells.contains(&reserved.position)
                    && !new_cells.contains(&reserved.position)
                    && reserved.position != point
            });
            if pending.is_some() {
                return Err(error(
                    "bridge-reservation",
                    "active-route",
                    "the next extension must enter the reserved bridge cell",
                ));
            }
            let reserved = active
                .bridge_reservations
                .iter()
                .find(|reserved| reserved.position == point);
            if reserved.is_some_and(|reserved| reserved.direction != heading)
                || reserved.is_none()
                    && (self.bridges.contains_key(&(
                        self.problem.connections[focus_connection].transport,
                        point,
                    )) || !self.shared_cell_available_replay(point, focus_connection)?)
            {
                return Err(error(
                    "same-layer-intersection",
                    "active-route",
                    "same-layer cell is incompatible, reserved in another direction or lacks capacity",
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

    fn site_arm_use<'a>(
        &'a self,
        connection_index: usize,
        route: &'a Route,
        path_index: usize,
    ) -> Result<SiteArmUse<'a>, Diagnostic> {
        let connection = &self.problem.connections[connection_index];
        let point = route.cells[path_index];
        let mut external_incoming = false;
        let mut external_outgoing = false;
        let incoming = if path_index > 0 {
            aic_layout::geometry::direction(point, route.cells[path_index - 1])
        } else if let Some(&block) = self.blocks.get(&connection.source) {
            let port_id = route
                .source_port
                .as_deref()
                .ok_or_else(|| error("topology", &connection.id, "source route port is missing"))?;
            let port = self
                .projected_ports(block)?
                .into_iter()
                .find(|p| p.id == port_id)
                .ok_or_else(|| error("topology", &connection.id, "source route port is unknown"))?;
            aic_layout::geometry::direction(point, port.position)
        } else {
            external_incoming = true;
            None
        };
        let outgoing = if path_index + 1 < route.cells.len() {
            aic_layout::geometry::direction(point, route.cells[path_index + 1])
        } else if let Some(&block) = self.blocks.get(&connection.target) {
            let port_id = route
                .target_port
                .as_deref()
                .ok_or_else(|| error("topology", &connection.id, "target route port is missing"))?;
            let port = self
                .projected_ports(block)?
                .into_iter()
                .find(|p| p.id == port_id)
                .ok_or_else(|| error("topology", &connection.id, "target route port is unknown"))?;
            aic_layout::geometry::direction(point, port.position)
        } else {
            external_outgoing = true;
            None
        };
        if incoming.is_none() && !external_incoming || outgoing.is_none() && !external_outgoing {
            return Err(error(
                "topology",
                &connection.id,
                "route endpoint arm is not adjacent",
            ));
        }
        Ok(SiteArmUse {
            connection: &connection.id,
            commodity: &connection.commodity,
            rate: connection.rate,
            incoming,
            outgoing,
            external_incoming,
            external_outgoing,
        })
    }

    fn junction_updates(
        &self,
        pending_index: usize,
        pending: &Route,
    ) -> Result<
        Vec<(
            (aic_layout::logistics::TransportKind, Point),
            Option<Junction>,
        )>,
        Diagnostic,
    > {
        let transport = self.problem.connections[pending_index].transport;
        let mut touched = HashSet::new();
        for &point in &pending.cells {
            self.charge_replay(ReplayWork::RouteCells, 1)?;
            touched.insert(point);
        }
        let mut existing_connections = HashSet::new();
        for point in &touched {
            self.check_replay_deadline()?;
            if let Some(users) = self.occupied[layer(transport)].get(point) {
                self.charge_replay(ReplayWork::Users, users.len())?;
                existing_connections.extend(users.iter().copied());
            }
        }
        let mut old_indices = HashMap::new();
        for ci in existing_connections {
            self.check_replay_deadline()?;
            let route = self.routes[ci]
                .as_ref()
                .expect("occupied route is committed");
            for (pi, &point) in route.cells.iter().enumerate() {
                self.charge_replay(ReplayWork::RouteCells, 1)?;
                if pi % 256 == 0 {
                    self.check_replay_deadline()?;
                }
                if touched.contains(&point) {
                    old_indices.insert((ci, point), pi);
                }
            }
        }
        let mut updates = Vec::with_capacity(pending.cells.len());
        for (pi, &point) in pending.cells.iter().enumerate() {
            self.charge_replay(ReplayWork::RouteCells, 1)?;
            if pi % 256 == 0 {
                self.check_replay_deadline()?;
            }
            if self.active.as_ref().is_some_and(|active| {
                active
                    .bridge_reservations
                    .iter()
                    .any(|bridge| bridge.position == point)
            }) {
                continue;
            }
            let users = self.occupied[layer(transport)].get(&point);
            if users.is_none() && pi > 0 && pi + 1 < pending.cells.len() {
                continue;
            }
            let mut uses = Vec::new();
            if let Some(users) = users {
                for &ci in users {
                    self.charge_replay(ReplayWork::Users, 1)?;
                    self.check_replay_deadline()?;
                    let route = self.routes[ci]
                        .as_ref()
                        .expect("occupied route is committed");
                    let old_pi = old_indices[&(ci, point)];
                    uses.push(self.site_arm_use(ci, route, old_pi)?);
                }
            }
            uses.push(self.site_arm_use(pending_index, pending, pi)?);
            let options =
                junction_options_with_visit(transport, point, &uses, &self.catalogs, || {
                    self.charge_replay(ReplayWork::Catalog, 1).is_ok()
                })
                .map_err(|failure| {
                    error(
                        if failure.code == "route-request-work-limit" {
                            "route-request-work-limit"
                        } else {
                            "topology"
                        },
                        failure.entity.to_string(),
                        failure.message,
                    )
                })?;
            self.check_replay_deadline()?;
            let key = (transport, point);
            let junction = options.map(|options| {
                let selected = self
                    .junctions
                    .get(&key)
                    .filter(|prior| options.contains(&(prior.component.clone(), prior.rotation)))
                    .map(|prior| (prior.component.clone(), prior.rotation))
                    .unwrap_or_else(|| options[0].clone());
                let mut connections = uses
                    .iter()
                    .map(|use_site| use_site.connection.to_owned())
                    .collect::<Vec<_>>();
                connections.sort();
                connections.dedup();
                let transport_id = match transport {
                    aic_layout::logistics::TransportKind::Belt => "belt",
                    aic_layout::logistics::TransportKind::Pipe => "pipe",
                };
                Junction {
                    id: format!("junction-{transport_id}-{}-{}", point.x, point.y),
                    component: selected.0,
                    position: point,
                    rotation: selected.1,
                    connections,
                }
            });
            updates.push((key, junction));
        }
        Ok(updates)
    }

    fn bridge_updates(
        &self,
        pending_index: usize,
        pending: &Route,
    ) -> Result<Vec<Bridge>, Diagnostic> {
        let active = self
            .active
            .as_ref()
            .expect("bridge updates require an active route");
        let transport = self.problem.connections[pending_index].transport;
        let mut bridges = Vec::with_capacity(active.bridge_reservations.len());
        for reserved in &active.bridge_reservations {
            self.check_replay_deadline()?;
            let mut found = None;
            for (pi, &point) in pending.cells.iter().enumerate() {
                self.charge_replay(ReplayWork::RouteCells, 1)?;
                if pi % 256 == 0 {
                    self.check_replay_deadline()?;
                }
                if point == reserved.position {
                    found = Some(pi);
                    break;
                }
            }
            let Some(pi) = found else {
                return Err(error(
                    "bridge-reservation",
                    &pending.connection,
                    "reserved bridge was not traversed",
                ));
            };
            if pi == 0
                || pi + 1 == pending.cells.len()
                || point_direction(pending.cells[pi - 1], reserved.position)
                    != Some(reserved.direction)
                || point_direction(reserved.position, pending.cells[pi + 1])
                    != Some(reserved.direction)
            {
                return Err(error(
                    "bridge-turn",
                    &pending.connection,
                    "route must enter and leave each bridge straight",
                ));
            }
            if let Some(installed) = self.bridges.get(&(transport, reserved.position)) {
                self.charge_replay(
                    ReplayWork::Users,
                    installed.channels[reserved.direction % 2].connections.len(),
                )?;
                let mut bridge = installed.clone();
                let owners = &mut bridge.channels[reserved.direction % 2].connections;
                owners.push(pending.connection.clone());
                owners.sort();
                owners.dedup();
                bridges.push(bridge);
                continue;
            }
            let users = self.occupied[layer(transport)]
                .get(&reserved.position)
                .expect("reserved bridge has committed carrier users");
            self.charge_replay(ReplayWork::Users, users.len())?;
            let mut owners = users
                .iter()
                .map(|&ci| self.problem.connections[ci].id.clone())
                .collect::<Vec<_>>();
            owners.sort();
            owners.dedup();
            let carrier_commodity = self.problem.connections[users[0]].commodity.clone();
            let new_commodity = self.problem.connections[pending_index].commodity.clone();
            let (horizontal, vertical) = if reserved.crossed_axis == BridgeAxis::Horizontal {
                (
                    BridgeChannel {
                        axis: BridgeAxis::Horizontal,
                        commodity: carrier_commodity,
                        connections: owners,
                    },
                    BridgeChannel {
                        axis: BridgeAxis::Vertical,
                        commodity: new_commodity,
                        connections: vec![pending.connection.clone()],
                    },
                )
            } else {
                (
                    BridgeChannel {
                        axis: BridgeAxis::Horizontal,
                        commodity: new_commodity,
                        connections: vec![pending.connection.clone()],
                    },
                    BridgeChannel {
                        axis: BridgeAxis::Vertical,
                        commodity: carrier_commodity,
                        connections: owners,
                    },
                )
            };
            let transport_id = match transport {
                aic_layout::logistics::TransportKind::Belt => "belt",
                aic_layout::logistics::TransportKind::Pipe => "pipe",
            };
            bridges.push(Bridge {
                id: format!(
                    "bridge-{transport_id}-{}-{}",
                    reserved.position.x, reserved.position.y
                ),
                component: reserved.component.clone(),
                position: reserved.position,
                rotation: reserved.rotation,
                channels: [horizontal, vertical],
            });
        }
        Ok(bridges)
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
        let obligation = &self.problem.connections[active.connection];
        let route = Route {
            connection: obligation.id.clone(),
            source_port: active.source_port.clone(),
            target_port: active.target_port.clone(),
            cells: active.cells.clone(),
        };
        let updates = self.junction_updates(active.connection, &route)?;
        let bridges = self.bridge_updates(active.connection, &route)?;
        let active = self.active.take().expect("active route checked above");
        self.active_cells.clear();
        let obligation = &self.problem.connections[active.connection];
        for &cell in &route.cells {
            self.charge_replay(ReplayWork::RouteCells, 1)?;
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
        for (site, junction) in updates {
            if let Some(junction) = junction {
                self.junctions.insert(site, junction);
            } else {
                self.junctions.remove(&site);
            }
        }
        for bridge in bridges {
            self.bridges
                .insert((obligation.transport, bridge.position), bridge);
        }
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
        physical::{Block, Connection, Terminal, TerminalDirection},
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

    fn route_candidate_config() -> routing_candidates::Config {
        routing_candidates::Config {
            bounds: aic_layout::geometry::Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 15,
                max_y: 15,
            },
            max_candidates: 3,
            max_searches: 12,
            work_per_search: 10_000,
            max_expanded: 100_000,
            overlap_penalty: 8,
            max_crossings_per_route: 0,
            request_allowance: Default::default(),
        }
    }

    #[test]
    fn repair_replaces_committed_route_atomically() {
        let panel: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-network/panel.json"
        )))
        .unwrap();
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone()).unwrap(),
            catalogs(),
            serde_json::from_value(panel["limits"].clone()).unwrap(),
        )
        .unwrap();
        for command in case["prefix"].as_array().unwrap() {
            assert!(
                session
                    .step(serde_json::from_value(command.clone()).unwrap())
                    .accepted
            );
        }
        let mut config = route_candidate_config();
        config.bounds.max_x = 20;
        config.bounds.max_y = 20;
        config.max_candidates = 6;
        config.max_searches = 48;
        config.max_expanded = 400_000;
        let first = session
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: session.revision(),
                    connection: "output-a".into(),
                },
                &config,
            )
            .unwrap()
            .candidates[0]
            .clone();
        session.apply_route_candidate(&first).unwrap();
        let original = session.observe();
        let request = session
            .repair_request(&["output-b".into()], &["output-a".into()], 1, &config)
            .unwrap();
        let mut allowance =
            routing_candidates::RequestAllowance::new(request.allowance_caps.clone());
        let (mut draft, reserved_bytes) = session
            .prepare_repair_draft_with_allowance(&request, &["output-a".into()], &mut allowance)
            .unwrap();
        allowance.release_peak(reserved_bytes);
        assert!(!draft.route_is_complete(draft.connections["output-a"]));
        let next = draft
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: draft.revision(),
                    connection: "output-b".into(),
                },
                &config,
            )
            .unwrap()
            .candidates[0]
            .clone();
        draft.apply_route_candidate(&next).unwrap();
        let restored = draft
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: draft.revision(),
                    connection: "output-a".into(),
                },
                &config,
            )
            .unwrap()
            .candidates[0]
            .clone();
        let candidate = routing_candidates::RepairCandidate {
            schema: routing_candidates::REPAIR_CANDIDATE_SCHEMA.into(),
            state_revision: request.state_revision,
            removed: vec!["output-a".into()],
            replacements: vec![next, restored],
            expanded: 0,
        };
        let original_bytes = serde_json::to_vec(&original).unwrap();
        let mut invalid = candidate.clone();
        invalid.replacements.pop();
        assert_eq!(
            session
                .apply_repair_candidate(&request, &invalid)
                .unwrap_err()
                .code,
            "constructive.repair-coverage"
        );
        assert_eq!(
            serde_json::to_vec(&session.observe()).unwrap(),
            original_bytes
        );
        let mut unauthorized = candidate.clone();
        unauthorized.removed = vec!["supply-flow".into()];
        assert_eq!(
            session
                .apply_repair_candidate(&request, &unauthorized)
                .unwrap_err()
                .code,
            "constructive.repair-removals"
        );
        let mut stale = request.clone();
        stale.state_revision += 1;
        assert_eq!(
            session
                .apply_repair_candidate(&stale, &candidate)
                .unwrap_err()
                .code,
            "constructive.repair-stale-request"
        );
        let mut bad_member = candidate.clone();
        bad_member.replacements[1].commands.pop();
        assert!(
            session
                .apply_repair_candidate(&request, &bad_member)
                .is_err()
        );
        let mut wrong_revision = candidate.clone();
        wrong_revision.replacements[1].state_revision += 1;
        assert_eq!(
            session
                .apply_repair_candidate(&request, &wrong_revision)
                .unwrap_err()
                .code,
            "constructive.repair-revision"
        );
        assert_eq!(
            serde_json::to_vec(&session.observe()).unwrap(),
            original_bytes
        );
        session
            .apply_repair_candidate(&request, &candidate)
            .unwrap();
        assert!(session.observe().fully_validated);
        assert_eq!(session.observe().completed, 3);
        assert_eq!(session.observe().bridges, original.bridges);
    }

    #[test]
    fn repair_unblocks_frozen_corridor() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-repair/fixture.json"
        )))
        .unwrap();
        let mut session = Session::new(
            serde_json::from_value(fixture["problem"].clone()).unwrap(),
            catalogs(),
            serde_json::from_value(fixture["limits"].clone()).unwrap(),
        )
        .unwrap();
        let config: routing_candidates::Config =
            serde_json::from_value(fixture["candidate"].clone()).unwrap();
        for command in fixture["prefix"].as_array().unwrap() {
            let step = session.step(serde_json::from_value(command.clone()).unwrap());
            assert!(step.accepted, "{step:?}");
        }
        let first_route = session.observe().routes[0].clone().unwrap();
        let blocked = session
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: session.revision(),
                    connection: "second".into(),
                },
                &config,
            )
            .unwrap();
        assert!(blocked.candidates.is_empty());
        assert_eq!(blocked.expanded, 0);
        assert_eq!(
            blocked.termination,
            routing_candidates::Termination::FrontierExhausted
        );
        assert!(blocked.fixed_request_proven_infeasible);
        let request = session
            .repair_request(&["second".into()], &["first".into()], 1, &config)
            .unwrap();
        let mut allowance =
            routing_candidates::RequestAllowance::new(request.allowance_caps.clone());
        let (mut draft, reserved_bytes) = session
            .prepare_repair_draft_with_allowance(&request, &["first".into()], &mut allowance)
            .unwrap();
        allowance.release_peak(reserved_bytes);
        let second = draft
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: draft.revision(),
                    connection: "second".into(),
                },
                &config,
            )
            .unwrap()
            .candidates[0]
            .clone();
        draft.apply_route_candidate(&second).unwrap();
        let replacement = draft
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: draft.revision(),
                    connection: "first".into(),
                },
                &config,
            )
            .unwrap()
            .candidates[0]
            .clone();
        let candidate = routing_candidates::RepairCandidate {
            schema: routing_candidates::REPAIR_CANDIDATE_SCHEMA.into(),
            state_revision: request.state_revision,
            removed: vec!["first".into()],
            expanded: second.expanded + replacement.expanded,
            replacements: vec![second, replacement],
        };
        session
            .apply_repair_candidate(&request, &candidate)
            .unwrap();
        assert!(session.observe().fully_validated);
        assert_ne!(session.observe().routes[0], Some(first_route));
    }

    #[test]
    fn route_candidates_are_diverse_native_valid_and_revision_atomic() {
        let mut session = session();
        for command in [
            Command::Place {
                block: "source".into(),
                position: Point { x: 5, y: 12 },
                rotation: 0,
            },
            Command::Place {
                block: "target".into(),
                position: Point { x: 5, y: 2 },
                rotation: 0,
            },
        ] {
            assert!(session.step(command).accepted);
        }
        let revision = session.observe().revision;
        let report = session
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: revision,
                    connection: "connection-0".into(),
                },
                &route_candidate_config(),
            )
            .unwrap();

        assert_eq!(session.observe().revision, revision);
        assert!(session.observe().active.is_none());
        assert!(report.candidates.len() >= 2, "{report:#?}");
        assert!(report.native_rejections.is_empty());
        assert!(report.minimum_pairwise_dissimilarity.unwrap() > 0.0);
        assert_ne!(report.candidates[0].cells, report.candidates[1].cells);
        assert!(matches!(
            report.candidates[0].commands.first(),
            Some(Command::BeginRoute { .. })
        ));
        assert_eq!(
            report.candidates[0].commands.last(),
            Some(&Command::Connect)
        );

        let mut tiny_publication = route_candidate_config();
        tiny_publication.request_allowance.published_bytes = 16;
        let stopped = session
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: revision,
                    connection: "connection-0".into(),
                },
                &tiny_publication,
            )
            .unwrap();
        assert!(stopped.candidates.is_empty());
        assert_eq!(
            stopped.termination,
            routing_candidates::Termination::WorkLimit
        );
        assert_eq!(
            stopped
                .allowance_stop
                .as_ref()
                .and_then(|stop| stop.category),
            Some(routing_candidates::WorkKind::PublishedBytes)
        );

        let mut capped = session.clone();
        let mut caps = route_candidate_config().request_allowance;
        caps.replay_route_cells = 1;
        let mut allowance = routing_candidates::RequestAllowance::new(caps);
        let failure = capped
            .apply_route_candidate_with_allowance(&report.candidates[0], &mut allowance)
            .unwrap_err();
        assert!(matches!(
            failure,
            routing_candidates::RequestFailure::Stopped(stop)
                if stop.category == Some(routing_candidates::WorkKind::ReplayRouteCells)
        ));
        assert_eq!(capped.observe().revision, revision);
        assert!(capped.observe().active.is_none());

        let mut committed = session.clone();
        let steps = committed
            .apply_route_candidate(&report.candidates[0])
            .unwrap();
        assert!(steps.iter().all(|step| step.accepted));
        assert!(committed.layout().is_some());
        let stale = committed
            .apply_route_candidate(&report.candidates[1])
            .unwrap_err();
        assert_eq!(stale.code, "constructive.stale-route-candidate");
    }

    #[test]
    fn route_readiness_and_endpoint_allowance_preserve_unknown() {
        use routing_candidates::{Request, RouteReadiness, Termination, WorkKind};

        let mut session = session();
        let request = Request {
            state_revision: session.observe().revision,
            connection: "connection-0".into(),
        };
        assert_eq!(
            session.route_request_readiness(&request).unwrap(),
            RouteReadiness::PendingPlacement {
                endpoints: vec!["source".into(), "target".into()]
            }
        );
        place_pair(&mut session);
        let request = Request {
            state_revision: session.observe().revision,
            ..request
        };
        assert_eq!(
            session.route_request_readiness(&request).unwrap(),
            RouteReadiness::Ready
        );

        let mut config = route_candidate_config();
        config.request_allowance.endpoint_visits = 1;
        let report = session.route_candidates(&request, &config).unwrap();
        assert_eq!(report.termination, Termination::WorkLimit);
        assert!(!report.fixed_request_proven_infeasible);
        assert!(report.candidates.is_empty());
        assert_eq!(
            report.allowance_stop.unwrap().category,
            Some(WorkKind::EndpointVisits)
        );
    }

    #[test]
    fn guidance_raster_cap_stops_before_input_allocation() {
        use routing_candidates::{Request, RequestAllowance, RequestFailure, WorkKind};

        let mut session = session();
        place_pair(&mut session);
        let request = Request {
            state_revision: session.observe().revision,
            connection: "connection-0".into(),
        };
        let mut config = route_candidate_config();
        config.request_allowance.raster_cells = 15;
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let failure = session
            .route_guidance_input_with_allowance(
                &request,
                &config,
                &learning_config(),
                &mut allowance,
            )
            .unwrap_err();
        assert!(
            matches!(failure, RequestFailure::Stopped(stop) if stop.category == Some(WorkKind::RasterCells))
        );
        assert!(allowance.usage().peak_live_bytes > 0);
        assert_eq!(
            allowance
                .usage()
                .categories
                .get(&WorkKind::RasterCells)
                .copied()
                .unwrap_or(0),
            0
        );
        let mut tiny_peak = route_candidate_config();
        tiny_peak.request_allowance.peak_live_bytes = 1;
        let mut peak_allowance = RequestAllowance::new(tiny_peak.request_allowance.clone());
        let failure = session
            .route_guidance_input_with_allowance(
                &request,
                &tiny_peak,
                &learning_config(),
                &mut peak_allowance,
            )
            .unwrap_err();
        assert!(matches!(
            failure,
            RequestFailure::Stopped(stop) if stop.reason == "peak-memory-limit"
        ));
        assert_eq!(peak_allowance.usage().peak_live_bytes, 0);
    }

    #[test]
    fn sequential_frontier_peaks_share_one_live_limit() {
        let mut caps = routing_candidates::AllowanceCaps::default();
        caps.frontier_entries = 3;
        let mut allowance = routing_candidates::RequestAllowance::new(caps);
        allowance
            .record_peak(routing_candidates::WorkKind::FrontierEntries, 3)
            .unwrap();
        allowance
            .record_peak(routing_candidates::WorkKind::FrontierEntries, 3)
            .unwrap();
        assert_eq!(
            allowance.usage().categories[&routing_candidates::WorkKind::FrontierEntries],
            3
        );
        assert_eq!(
            allowance
                .record_peak(routing_candidates::WorkKind::FrontierEntries, 4)
                .unwrap_err()
                .used,
            4
        );
    }

    #[test]
    fn route_guidance_is_task_focused_and_preserves_an_anchor_candidate() {
        let mut session = session();
        for command in [
            Command::Place {
                block: "source".into(),
                position: Point { x: 5, y: 12 },
                rotation: 0,
            },
            Command::Place {
                block: "target".into(),
                position: Point { x: 5, y: 2 },
                rotation: 0,
            },
        ] {
            assert!(session.step(command).accepted);
        }
        let request = routing_candidates::Request {
            state_revision: session.observe().revision,
            connection: "connection-0".into(),
        };
        let config = route_candidate_config();
        let input = session
            .route_guidance_input(&request, &config, &learning_config())
            .unwrap();
        assert_eq!(
            input.grid.len(),
            routing_candidates::GUIDANCE_GRID_CHANNELS * 16 * 16
        );
        assert_eq!(input.graph.focus_entity, 2);
        assert_eq!(input.graph.entity_features[2][7], 1.0);

        let anchor = session
            .route_candidates(&request, &config)
            .unwrap()
            .candidates
            .remove(0);
        let directed_costs: Vec<_> = anchor
            .cells
            .windows(2)
            .map(|edge| {
                let direction = match (edge[1].x - edge[0].x, edge[1].y - edge[0].y) {
                    (1, 0) => 0,
                    (0, 1) => 1,
                    (-1, 0) => 2,
                    (0, -1) => 3,
                    _ => unreachable!(),
                };
                routing_candidates::DirectedCost {
                    point: edge[1],
                    direction,
                    cost: 100,
                }
            })
            .collect();
        let guidance = routing_candidates::Guidance {
            schema: routing_candidates::GUIDANCE_SCHEMA.into(),
            state_revision: request.state_revision,
            connection: request.connection.clone(),
            model_version: "test-guidance-v1".into(),
            heads: vec![routing_candidates::GuidanceHead {
                id: "preserve-anchor-corridor".into(),
                directed_costs,
                predicted_success: 0.75,
                uncertainty: 0.25,
            }],
        };
        let mut tiny_map = config.clone();
        tiny_map.request_allowance.peak_live_bytes =
            guidance.heads[0].directed_costs.len() * 96 - 1;
        let stopped = session
            .route_candidates_with_guidance(&request, &tiny_map, &guidance)
            .unwrap();
        assert_eq!(
            stopped.termination,
            routing_candidates::Termination::WorkLimit
        );
        assert_eq!(stopped.allowance_stop.unwrap().reason, "peak-memory-limit");
        let report = session
            .route_candidates_with_guidance(&request, &config, &guidance)
            .unwrap();

        assert!(matches!(
            report.candidates[0].provenance,
            routing_candidates::Provenance::Anchor
        ));
        assert_eq!(report.candidates[0].cells, anchor.cells);
        assert!(report.candidates.iter().any(|candidate| matches!(
            candidate.provenance,
            routing_candidates::Provenance::Guided { .. }
        ) && candidate.cells != anchor.cells));
        assert!(report.native_rejections.is_empty());
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
        let placement_ids = first
            .commands
            .iter()
            .chain(&second.commands)
            .map(|factored| match &factored.command.command {
                Command::Place { block, .. } => block.as_str(),
                command => panic!("unexpected placement page command: {command:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            placement_ids,
            vec!["source", "target", "source", "target", "source", "target"]
        );
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
        let network_revision = session.observe().revision;
        let mut staged = session.clone();
        let mut routes = Vec::new();
        for (connection, horizontal) in
            [("output-a", Direction::West), ("output-b", Direction::East)]
        {
            let state_revision = staged.observe().revision;
            let commands = vec![
                Command::BeginRoute {
                    connection: connection.into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: None,
                },
                Command::Extend {
                    direction: Direction::North,
                    length: 3,
                },
                Command::Extend {
                    direction: horizontal,
                    length: 2,
                },
                Command::Extend {
                    direction: Direction::North,
                    length: 4,
                },
                Command::Connect,
            ];
            for command in &commands {
                assert!(staged.step(command.clone()).accepted);
            }
            let route = staged
                .observe()
                .routes
                .into_iter()
                .flatten()
                .find(|route| route.connection == connection)
                .unwrap();
            routes.push(routing_candidates::Candidate {
                id: routes.len(),
                state_revision,
                connection: connection.into(),
                source_port: Some("output-belt-1".into()),
                target_port: None,
                cells: route.cells,
                commands,
                crossings: vec![],
                steps: 9,
                turns: 2,
                expanded: 0,
                overlap_with_previous: 0,
                learned_cost: 0,
                predicted_success: None,
                guidance_uncertainty: None,
                provenance: routing_candidates::Provenance::Anchor,
            });
        }
        let network = routing_candidates::NetworkCandidate {
            id: 0,
            state_revision: network_revision,
            commodity: "test-item".into(),
            transport: TransportKind::Belt,
            steps: routes.iter().map(|route| route.steps).sum(),
            turns: routes.iter().map(|route| route.turns).sum(),
            expanded: 0,
            shared_cells: routing_candidates::network_shared_cells(&routes),
            routes,
        };
        assert_eq!(network.shared_cells, 4);
        let mut tampered = network.clone();
        tampered.shared_cells += 1;
        assert!(session.apply_network_candidate(&tampered).is_err());
        assert_eq!(session.observe().revision, network_revision);
        session.apply_network_candidate(&network).unwrap();
        assert!(session.done());
        assert!(session.metrics().is_some());
        assert_eq!(session.observe().completed, 3);
        let layout = session.layout().unwrap();
        assert_eq!(layout.solution.junctions.len(), 1);
        assert_eq!(layout.solution.junctions[0].connections.len(), 2);
        let mut missing_junction = layout.clone();
        missing_junction.solution.junctions.clear();
        assert_eq!(
            validate(&missing_junction, &catalogs()).unwrap_err().code,
            "aic-layout.junction"
        );
    }

    #[test]
    fn belt_and_pipe_merges_materialize_catalog_convergers() {
        for (transport, facility, port, center_x, source_y) in [
            (
                TransportKind::Belt,
                "grinder-1-mode-normal",
                "input-belt-1",
                6,
                15,
            ),
            (
                TransportKind::Pipe,
                "transmuter-1-mode-gastrans",
                "input-pipe-2",
                7,
                17,
            ),
        ] {
            let problem = Problem {
                blocks: vec![Block {
                    id: "machine".into(),
                    facility: facility.into(),
                }],
                terminals: ["left", "right"]
                    .into_iter()
                    .map(|id| Terminal {
                        id: id.into(),
                        direction: TerminalDirection::Input,
                        commodity: "merge-item".into(),
                        transport,
                    })
                    .collect(),
                connections: ["left", "right"]
                    .into_iter()
                    .map(|source| Connection {
                        id: format!("flow-{source}"),
                        source: source.into(),
                        target: "machine".into(),
                        commodity: "merge-item".into(),
                        transport,
                        rate: Rate {
                            numerator: 1,
                            denominator: 4,
                        },
                    })
                    .collect(),
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
                    terminal: "left".into(),
                    position: Point {
                        x: center_x - 2,
                        y: source_y,
                    },
                },
                Command::PlaceTerminal {
                    terminal: "right".into(),
                    position: Point {
                        x: center_x + 2,
                        y: source_y,
                    },
                },
            ] {
                assert!(session.step(command).accepted);
            }
            for (source, direction) in [("left", Direction::East), ("right", Direction::West)] {
                for command in [
                    Command::BeginRoute {
                        connection: format!("flow-{source}"),
                        source_port: None,
                        target_port: Some(port.into()),
                    },
                    Command::Extend {
                        direction,
                        length: 2,
                    },
                    Command::Extend {
                        direction: Direction::North,
                        length: 4,
                    },
                    Command::Connect,
                ] {
                    let step = session.step(command);
                    assert!(step.accepted, "{}", step.diagnostic.unwrap());
                }
            }
            let layout = session.layout().expect("complete merge validates");
            assert_eq!(layout.solution.junctions.len(), 1);
            assert_eq!(layout.solution.junctions[0].connections.len(), 2);
            let definition = session
                .catalogs
                .components
                .component(&layout.solution.junctions[0].component)
                .unwrap();
            assert_eq!(
                definition.kind,
                aic_layout::logistics::LogisticsComponentKind::Converger
            );
        }
    }

    #[test]
    fn pipe_split_materializes_rotated_catalog_splitter() {
        let problem = Problem {
            blocks: vec![Block {
                id: "machine".into(),
                facility: "furnance-1-mode-liquid".into(),
            }],
            terminals: ["left", "right"]
                .into_iter()
                .map(|id| Terminal {
                    id: id.into(),
                    direction: TerminalDirection::Output,
                    commodity: "split-item".into(),
                    transport: TransportKind::Pipe,
                })
                .collect(),
            connections: ["left", "right"]
                .into_iter()
                .map(|target| Connection {
                    id: format!("flow-{target}"),
                    source: "machine".into(),
                    target: target.into(),
                    commodity: "split-item".into(),
                    transport: TransportKind::Pipe,
                    rate: Rate {
                        numerator: 1,
                        denominator: 4,
                    },
                })
                .collect(),
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
                rotation: 270,
            },
            Command::PlaceTerminal {
                terminal: "left".into(),
                position: Point { x: 4, y: 0 },
            },
            Command::PlaceTerminal {
                terminal: "right".into(),
                position: Point { x: 8, y: 0 },
            },
        ] {
            let step = session.step(command);
            assert!(step.accepted, "{}", step.diagnostic.unwrap());
        }
        for (target, direction) in [("left", Direction::West), ("right", Direction::East)] {
            for command in [
                Command::BeginRoute {
                    connection: format!("flow-{target}"),
                    source_port: Some("output-pipe-3".into()),
                    target_port: None,
                },
                Command::Extend {
                    direction: Direction::North,
                    length: 3,
                },
                Command::Extend {
                    direction,
                    length: 2,
                },
                Command::Extend {
                    direction: Direction::North,
                    length: 4,
                },
                Command::Connect,
            ] {
                let step = session.step(command);
                assert!(step.accepted, "{}", step.diagnostic.unwrap());
            }
        }
        let layout = session.layout().expect("complete pipe split validates");
        assert_eq!(layout.solution.junctions.len(), 1);
        let definition = session
            .catalogs
            .components
            .component(&layout.solution.junctions[0].component)
            .unwrap();
        assert_eq!(
            definition.kind,
            aic_layout::logistics::LogisticsComponentKind::Splitter
        );
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
    fn same_layer_crossing_requires_a_reserved_bridge_and_commits_atomically() {
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
        let stable_before_vertical = session.clone();
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
        let horizontal = session.routes[0].as_ref().unwrap();
        let crossing = horizontal.cells[1];
        let option = aic_routing::crossing_options(
            &[aic_routing::Carrier {
                id: 0,
                network: 0,
                transport: TransportKind::Belt,
                rate: session.problem.connections[0].rate,
                cells: &horizontal.cells,
            }],
            &aic_routing::RouteJob {
                id: 1,
                transport: TransportKind::Belt,
                rate: session.problem.connections[1].rate,
            },
            &session.catalogs.components,
            &indexmap::IndexSet::new(),
        )
        .get(&(crossing, 3))
        .cloned()
        .expect("catalog has a matching northbound bridge");
        let start = before[0];
        assert_eq!(start.x, crossing.x);
        let to_bridge = start.y - crossing.y;
        assert!(to_bridge >= 2);
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: to_bridge - 1,
                })
                .accepted
        );
        assert!(
            session
                .step(Command::ReserveBridge {
                    position: crossing,
                    component: option.component,
                    rotation: option.rotation,
                    crossed_axis: BridgeAxis::Horizontal,
                })
                .accepted
        );
        let mut aborted = session.clone();
        assert!(aborted.step(Command::AbortRoute).accepted);
        assert!(aborted.observe().bridges.is_empty());
        assert_eq!(aborted.observe().routes[0], session.observe().routes[0]);
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 1,
                })
                .accepted
        );
        let wrong_turn = session.step(Command::Extend {
            direction: Direction::East,
            length: 1,
        });
        assert_eq!(
            wrong_turn.diagnostic.unwrap().code,
            "constructive.bridge-turn"
        );
        let target = session.active.as_ref().unwrap().target;
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: crossing.y - target.y,
                })
                .accepted
        );
        assert!(session.step(Command::Connect).accepted);
        let layout = session.layout().expect("bridge-bearing layout validates");
        assert_eq!(layout.solution.bridges.len(), 1);
        assert_eq!(
            layout.solution.bridges[0].channels[0].connections,
            vec!["horizontal"]
        );
        assert_eq!(
            layout.solution.bridges[0].channels[1].connections,
            vec!["vertical"]
        );
        assert_eq!(session.observe().bridges, layout.solution.bridges);
        let mut repair_base = session.clone();
        repair_base.problem.connections.push(Connection {
            id: "extra".into(),
            source: "south".into(),
            target: "north".into(),
            commodity: "vertical-item".into(),
            transport: TransportKind::Belt,
            rate: Rate {
                numerator: 1,
                denominator: 4,
            },
        });
        repair_base.connections.insert("extra".into(), 2);
        repair_base.routes.push(None);
        repair_base.metrics = None;
        let mut repair_config = route_candidate_config();
        repair_config.bounds.max_x = 20;
        repair_config.bounds.max_y = 20;
        repair_config.max_crossings_per_route = 441;
        repair_config.max_searches = 512;
        let repair_request = repair_base
            .repair_request(&["extra".into()], &["vertical".into()], 1, &repair_config)
            .unwrap();
        let mut repair_allowance =
            routing_candidates::RequestAllowance::new(repair_request.allowance_caps.clone());
        let (mut repair_draft, repair_bytes) = repair_base
            .prepare_repair_draft_with_allowance(
                &repair_request,
                &["vertical".into()],
                &mut repair_allowance,
            )
            .unwrap();
        repair_allowance.release_peak(repair_bytes);
        assert!(repair_draft.observe().bridges.is_empty());
        let vertical_probe = repair_draft
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: repair_draft.revision(),
                    connection: "vertical".into(),
                },
                &repair_config,
            )
            .unwrap();
        let restored_vertical = vertical_probe
            .candidates
            .into_iter()
            .find(|candidate| {
                candidate
                    .crossings
                    .iter()
                    .any(|bridge_use| bridge_use.position == crossing)
            })
            .expect("restored route crosses the original bridge");
        repair_draft
            .apply_route_candidate(&restored_vertical)
            .unwrap();
        let extra = repair_draft
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: repair_draft.revision(),
                    connection: "extra".into(),
                },
                &repair_config,
            )
            .unwrap()
            .candidates
            .remove(0);
        let repair_candidate = routing_candidates::RepairCandidate {
            schema: routing_candidates::REPAIR_CANDIDATE_SCHEMA.into(),
            state_revision: repair_request.state_revision,
            removed: vec!["vertical".into()],
            expanded: restored_vertical.expanded + extra.expanded,
            replacements: vec![restored_vertical, extra],
        };
        repair_base
            .apply_repair_candidate(&repair_request, &repair_candidate)
            .unwrap();
        assert!(repair_base.observe().fully_validated);
        let retained_bridge = repair_base
            .observe()
            .bridges
            .into_iter()
            .find(|bridge| bridge.position == crossing)
            .expect("the original bridge survives replacement of its vertical route");
        assert_eq!(retained_bridge.id, layout.solution.bridges[0].id);
        assert_eq!(
            retained_bridge.component,
            layout.solution.bridges[0].component
        );
        assert_eq!(
            retained_bridge.rotation,
            layout.solution.bridges[0].rotation
        );
        assert_eq!(retained_bridge.channels[0].connections, vec!["horizontal"]);
        assert_eq!(
            retained_bridge.channels[1].connections,
            vec!["extra", "vertical"]
        );
        let mut retained = session.clone();
        let mut allowance = routing_candidates::RequestAllowance::new(Default::default());
        retained
            .rebuild_after_route_removal(&mut allowance)
            .unwrap();
        assert_eq!(retained.observe().bridges, layout.solution.bridges);
        retained.routes[1] = None;
        retained
            .rebuild_after_route_removal(&mut allowance)
            .unwrap();
        assert!(retained.observe().bridges.is_empty());
        assert_eq!(retained.observe().routes[0], session.observe().routes[0]);
        let learning = session.macro_learning_input(&learning_config()).unwrap();
        assert_eq!(learning.bridge_resources, layout.solution.bridges);
        assert!(learning.spatial_features.iter().any(|row| row[9] == 1.0));
        assert!(learning.spatial_features.iter().any(|row| row[10] == 1.0));
        assert!(
            learning
                .spatial_features
                .iter()
                .any(|row| row[9] == 1.0 && row[11] > 0.0 && row[12] > 0.0)
        );
        let mut config = route_candidate_config();
        config.bounds = aic_layout::geometry::Bounds {
            min_x: crossing.x,
            max_x: crossing.x,
            min_y: target.y,
            max_y: start.y,
        };
        config.max_candidates = 1;
        config.max_crossings_per_route = 1;
        let report = stable_before_vertical
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: stable_before_vertical.revision,
                    connection: "vertical".into(),
                },
                &config,
            )
            .unwrap();
        assert_eq!(report.crossing_limit, 1);
        assert_eq!(report.candidates.len(), 1, "{report:?}");
        assert_eq!(report.candidates[0].crossings.len(), 1);
        config.max_crossings_per_route = 0;
        let clipped = stable_before_vertical
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: stable_before_vertical.revision,
                    connection: "vertical".into(),
                },
                &config,
            )
            .unwrap();
        assert!(clipped.candidates.is_empty());
        assert_eq!(
            clipped.termination,
            routing_candidates::Termination::ResourceLimit
        );
        assert!(!clipped.fixed_request_proven_infeasible);
        let mut occupied_probe = stable_before_vertical.clone();
        occupied_probe.replay_meter = Some(Arc::new(ReplayMeter::new([
            usize::MAX,
            0,
            usize::MAX,
            usize::MAX,
        ])));
        assert_eq!(
            occupied_probe
                .shared_cell_available_replay(crossing, occupied_probe.connections["vertical"])
                .unwrap_err()
                .code,
            "constructive.route-request-work-limit"
        );
        let mut port_capped = stable_before_vertical.clone();
        let mut caps = route_candidate_config().request_allowance;
        caps.catalog_visits = 0;
        let mut allowance = routing_candidates::RequestAllowance::new(caps);
        let stopped = port_capped
            .apply_route_candidate_with_allowance(&report.candidates[0], &mut allowance)
            .unwrap_err();
        assert!(matches!(
            stopped,
            routing_candidates::RequestFailure::Stopped(stop)
                if stop.category == Some(routing_candidates::WorkKind::CatalogVisits)
        ));
        assert_eq!(port_capped.revision(), stable_before_vertical.revision());
        let mut metered = stable_before_vertical.clone();
        let mut caps = route_candidate_config().request_allowance;
        caps.validation_records = 1;
        let mut allowance = routing_candidates::RequestAllowance::new(caps);
        let before_revision = metered.revision();
        let stopped = metered
            .apply_route_candidate_with_allowance(&report.candidates[0], &mut allowance)
            .unwrap_err();
        assert!(matches!(
            stopped,
            routing_candidates::RequestFailure::Stopped(stop)
                if stop.category == Some(routing_candidates::WorkKind::ValidationRecords)
        ));
        assert_eq!(metered.revision(), before_revision);
        let mut replay = stable_before_vertical;
        let mut tampered = report.candidates[0].clone();
        tampered.crossings[0].position.x += 1;
        if let Some(Command::ReserveBridge { position, .. }) = tampered
            .commands
            .iter_mut()
            .find(|command| matches!(command, Command::ReserveBridge { .. }))
        {
            position.x += 1;
        }
        let before_revision = replay.revision;
        assert!(replay.apply_route_candidate(&tampered).is_err());
        assert_eq!(replay.revision, before_revision);
        assert!(replay.observe().bridges.is_empty());
        let mut wrong_direction = report.candidates[0].clone();
        wrong_direction.crossings[0].direction = 1;
        assert_eq!(
            replay
                .apply_route_candidate(&wrong_direction)
                .unwrap_err()
                .code,
            "constructive.route-candidate-bridge-witness"
        );
        let mut wrong_carrier = report.candidates[0].clone();
        wrong_carrier.crossings[0].carrier_chain = usize::MAX;
        assert_eq!(
            replay
                .apply_route_candidate(&wrong_carrier)
                .unwrap_err()
                .code,
            "constructive.route-candidate-bridge-witness"
        );
        replay.apply_route_candidate(&report.candidates[0]).unwrap();
        assert_eq!(replay.layout().unwrap().solution.bridges.len(), 1);
    }

    #[test]
    fn pipe_crossing_candidate_needs_a_catalog_bridge_in_the_fixed_corridor() {
        let problem = Problem {
            blocks: ["west", "east", "south", "north"]
                .into_iter()
                .map(|id| Block {
                    id: id.into(),
                    facility: "furnance-1-mode-liquid".into(),
                })
                .collect(),
            terminals: vec![],
            connections: vec![
                Connection {
                    id: "horizontal".into(),
                    source: "west".into(),
                    target: "east".into(),
                    commodity: "pipe-a".into(),
                    transport: TransportKind::Pipe,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                },
                Connection {
                    id: "vertical".into(),
                    source: "south".into(),
                    target: "north".into(),
                    commodity: "pipe-b".into(),
                    transport: TransportKind::Pipe,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                },
            ],
        };
        let prefix = |catalogs: Arc<Catalogs>, problem: Problem| {
            let mut session = Session::new(
                problem,
                catalogs,
                Limits {
                    max_turns: 32,
                    max_extension: 16,
                },
            )
            .unwrap();
            for (block, position, rotation) in [
                ("west", Point { x: 2, y: 5 }, 0),
                ("east", Point { x: 8, y: 5 }, 0),
                ("south", Point { x: 5, y: 9 }, 270),
                ("north", Point { x: 5, y: 1 }, 270),
            ] {
                assert!(
                    session
                        .step(Command::Place {
                            block: block.into(),
                            position,
                            rotation
                        })
                        .accepted
                );
            }
            for command in [
                Command::BeginRoute {
                    connection: "horizontal".into(),
                    source_port: Some("output-pipe-3".into()),
                    target_port: Some("input-pipe-3".into()),
                },
                Command::Extend {
                    direction: Direction::East,
                    length: 2,
                },
                Command::Connect,
            ] {
                let step = session.step(command);
                assert!(
                    step.accepted,
                    "{}",
                    step.diagnostic
                        .unwrap_or_else(|| error("test", "pipe", "unknown"))
                );
            }
            session
        };
        let mut config = route_candidate_config();
        config.bounds = aic_layout::geometry::Bounds {
            min_x: 6,
            max_x: 6,
            min_y: 4,
            max_y: 8,
        };
        config.max_candidates = 1;
        config.max_crossings_per_route = 1;
        let mut session = prefix(catalogs(), problem.clone());
        let request = routing_candidates::Request {
            state_revision: session.revision,
            connection: "vertical".into(),
        };
        let report = session.route_candidates(&request, &config).unwrap();
        assert_eq!(report.candidates.len(), 1, "{report:?}");
        assert_eq!(report.candidates[0].crossings.len(), 1);
        session
            .apply_route_candidate(&report.candidates[0])
            .unwrap();
        assert_eq!(session.layout().unwrap().solution.bridges.len(), 1);
        let mut same_item = problem.clone();
        same_item.connections[1].commodity = "pipe-a".into();
        let shared = prefix(catalogs(), same_item);
        let shared_report = shared
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: shared.revision,
                    connection: "vertical".into(),
                },
                &config,
            )
            .unwrap();
        assert_eq!(shared_report.candidates.len(), 1, "{shared_report:?}");
        assert_eq!(shared_report.candidates[0].crossings.len(), 1);

        let mut parallel_problem = problem.clone();
        parallel_problem.blocks.push(Block {
            id: "east-2".into(),
            facility: "furnance-1-mode-liquid".into(),
        });
        parallel_problem.terminals.push(Terminal {
            id: "west-2".into(),
            direction: TerminalDirection::Input,
            commodity: "pipe-a".into(),
            transport: TransportKind::Pipe,
        });
        parallel_problem.connections.push(Connection {
            id: "horizontal-2".into(),
            source: "west-2".into(),
            target: "east-2".into(),
            commodity: "pipe-a".into(),
            transport: TransportKind::Pipe,
            rate: Rate {
                numerator: 1,
                denominator: 2,
            },
        });
        let mut parallel = prefix(catalogs(), parallel_problem);
        for command in [
            Command::Place {
                block: "east-2".into(),
                position: Point { x: 12, y: 5 },
                rotation: 0,
            },
            Command::PlaceTerminal {
                terminal: "west-2".into(),
                position: Point { x: 5, y: 4 },
            },
            Command::BeginRoute {
                connection: "horizontal-2".into(),
                source_port: None,
                target_port: Some("input-pipe-3".into()),
            },
            Command::Extend {
                direction: Direction::South,
                length: 2,
            },
        ] {
            let step = parallel.step(command);
            assert!(step.accepted, "{step:?}");
        }
        let parallel_rejection = parallel.step(Command::ReserveBridge {
            position: Point { x: 6, y: 6 },
            component: "log-pipe-connector".into(),
            rotation: 0,
            crossed_axis: BridgeAxis::Vertical,
        });
        assert_eq!(
            parallel_rejection.diagnostic.unwrap().code,
            "constructive.bridge-reservation"
        );
        assert!(parallel.observe().bridges.is_empty());

        let mut multiowner_problem = problem.clone();
        multiowner_problem.blocks.push(Block {
            id: "north-2".into(),
            facility: "furnance-1-mode-liquid".into(),
        });
        multiowner_problem.terminals.push(Terminal {
            id: "south-2".into(),
            direction: TerminalDirection::Input,
            commodity: "pipe-b".into(),
            transport: TransportKind::Pipe,
        });
        multiowner_problem.connections.push(Connection {
            id: "vertical-2".into(),
            source: "south-2".into(),
            target: "north-2".into(),
            commodity: "pipe-b".into(),
            transport: TransportKind::Pipe,
            rate: Rate {
                numerator: 1,
                denominator: 2,
            },
        });
        let mut multiowner = prefix(catalogs(), multiowner_problem);
        for command in [
            Command::Place {
                block: "north-2".into(),
                position: Point { x: 10, y: 2 },
                rotation: 0,
            },
            Command::PlaceTerminal {
                terminal: "south-2".into(),
                position: Point { x: 7, y: 12 },
            },
        ] {
            let step = multiowner.step(command);
            assert!(step.accepted, "{step:?}");
        }
        // Probe the candidate filter while the third obligation keeps this
        // state incomplete: a component on the other transport layer must not
        // suppress a pipe bridge candidate at the same coordinate.
        let mut other_layer = multiowner.clone();
        other_layer.junctions.insert(
            (TransportKind::Belt, Point { x: 6, y: 6 }),
            Junction {
                id: "other-layer-probe".into(),
                component: "log-converger".into(),
                position: Point { x: 6, y: 6 },
                rotation: 0,
                connections: vec![],
            },
        );
        let cross_layer = other_layer
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: other_layer.revision,
                    connection: "vertical".into(),
                },
                &config,
            )
            .unwrap();
        assert_eq!(cross_layer.candidates.len(), 1, "{cross_layer:?}");
        assert_eq!(
            cross_layer.candidates[0].crossings[0].position,
            Point { x: 6, y: 6 }
        );
        let first = multiowner
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: multiowner.revision,
                    connection: "vertical".into(),
                },
                &config,
            )
            .unwrap();
        assert_eq!(first.candidates.len(), 1, "{first:?}");
        multiowner
            .apply_route_candidate(&first.candidates[0])
            .unwrap();
        let mut second_config = config.clone();
        second_config.bounds.max_x = 9;
        second_config.bounds.min_y = 3;
        second_config.bounds.max_y = 12;
        let second = multiowner
            .route_candidates(
                &routing_candidates::Request {
                    state_revision: multiowner.revision,
                    connection: "vertical-2".into(),
                },
                &second_config,
            )
            .unwrap();
        assert_eq!(second.candidates.len(), 1, "{second:?}");
        assert_eq!(second.candidates[0].crossings.len(), 1);
        assert_eq!(
            second.candidates[0].crossings[0].position,
            Point { x: 6, y: 6 }
        );
        multiowner
            .apply_route_candidate(&second.candidates[0])
            .unwrap();
        let layout = multiowner.layout().unwrap();
        assert_eq!(layout.solution.bridges.len(), 1);
        assert_eq!(
            layout.solution.bridges[0].channels[1].connections,
            ["vertical", "vertical-2"]
        );

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized");
        let full = catalogs();
        let mut components: LogisticsComponentCatalog = read(&root, "logistics-components.json");
        for component in &mut components.components {
            if component.kind == aic_layout::logistics::LogisticsComponentKind::Bridge
                && component.transport == TransportKind::Pipe
            {
                component.capacity.duration_ms = 10_000;
            }
        }
        let without_bridges = Arc::new(Catalogs {
            identity: full.identity.clone(),
            facilities: ValidatedFacilityCatalog::try_from_catalog(read(&root, "facilities.json"))
                .unwrap(),
            transports: ValidatedTransportCatalog::try_from_catalog(read(&root, "transports.json"))
                .unwrap(),
            components: ValidatedLogisticsComponentCatalog::try_from_catalog(components).unwrap(),
        });
        let mut no_bridge = prefix(without_bridges, problem);
        let request = routing_candidates::Request {
            state_revision: no_bridge.revision,
            connection: "vertical".into(),
        };
        let report = no_bridge.route_candidates(&request, &config).unwrap();
        assert!(report.candidates.is_empty());
        assert_eq!(
            report.termination,
            routing_candidates::Termination::FrontierExhausted
        );
        assert!(report.fixed_request_proven_infeasible);
        assert!(
            no_bridge
                .step(Command::BeginRoute {
                    connection: "vertical".into(),
                    source_port: Some("output-pipe-3".into()),
                    target_port: Some("input-pipe-3".into()),
                })
                .accepted
        );
        assert!(
            no_bridge
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 1
                })
                .accepted
        );
        let rejected = no_bridge.step(Command::ReserveBridge {
            position: Point { x: 6, y: 6 },
            component: "log-pipe-connector".into(),
            rotation: 0,
            crossed_axis: BridgeAxis::Horizontal,
        });
        assert_eq!(
            rejected.diagnostic.unwrap().code,
            "constructive.bridge-reservation"
        );
        assert!(no_bridge.observe().bridges.is_empty());
    }
}
