//! Versioned numerical state and conditional command domains for direct construction.

use std::{collections::BTreeSet, sync::Arc};

use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Bounds, Point, layer},
    projection::rotate,
};
use serde::{Deserialize, Serialize};

use super::{Command, Diagnostic, Direction, Session, error};

pub const SCHEMA: &str = "aic-constructive-learning-v2";
pub const ENTITY_FEATURES: usize = 18;
pub const RELATION_FEATURES: usize = 6;
pub const SPATIAL_FEATURES: usize = 12;
pub const GLOBAL_FEATURES: usize = 8;
pub const FOCUS_FEATURES: usize = 12;
pub const CHOICE_FEATURES: usize = 22;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Inclusive placement canvas. A facility body must fit inside it.
    pub placement: Bounds,
    pub extension_lengths: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Input {
    pub schema: &'static str,
    pub entity_features: Vec<[f32; ENTITY_FEATURES]>,
    pub relation_index: Vec<[i64; 2]>,
    pub relation_features: Vec<[f32; RELATION_FEATURES]>,
    /// One row per occupied cell or explicit endpoint/target landmark. Rows at
    /// the same coordinate remain separate, preserving belt/pipe coexistence.
    pub spatial_features: Vec<[f32; SPATIAL_FEATURES]>,
    /// Owning block/connection entity, or -1 for a global landmark.
    pub spatial_owners: Vec<i64>,
    pub global_features: [f32; GLOBAL_FEATURES],
    pub focus_features: [f32; FOCUS_FEATURES],
    pub focus_entity: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Observation {
    pub input: Arc<Input>,
    pub domain: Arc<Domain>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CommandKind {
    Place,
    PlaceTerminal,
    BeginRoute,
    Extend,
    Connect,
    AbortRoute,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "stage", rename_all = "kebab-case")]
pub enum Cursor {
    Command,
    PlaceBlock,
    PlacePose { block: usize },
    PlaceTerminal,
    TerminalPosition { terminal: usize },
    RouteConnection,
    RouteSource { connection: usize },
    RouteTarget { connection: usize, source: usize },
    ExtendDirection,
    ExtendLength { direction: usize },
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Choice {
    pub features: [f32; CHOICE_FEATURES],
    pub references: [i64; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Advance {
    Continue(Cursor),
    Command(SelectedCommand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedCommand {
    pub revision: u64,
    pub command: Command,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Domain {
    revision: u64,
    scale: f32,
    placement: Bounds,
    blocked: BTreeSet<Point>,
    placements: Vec<PlacementDomain>,
    terminals: Vec<TerminalDomain>,
    terminal_positions: Vec<Point>,
    routes: Vec<RouteDomain>,
    extensions: Vec<ExtensionDomain>,
    focus_entity: i64,
    connect: bool,
    abort: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct PlacementDomain {
    block: usize,
    id: String,
    width: i64,
    height: i64,
    rotations: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct TerminalDomain {
    terminal: usize,
    entity: i64,
    id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct RouteDomain {
    connection: usize,
    entity: i64,
    endpoints: [i64; 2],
    id: String,
    sources: Vec<PortDomain>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct PortDomain {
    id: Option<String>,
    targets: Vec<Option<String>>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct ExtensionDomain {
    direction: Direction,
    lengths: Vec<i64>,
}

impl Config {
    fn validate(&self, session: &Session) -> Result<(), Diagnostic> {
        let width = self
            .placement
            .max_x
            .checked_sub(self.placement.min_x)
            .and_then(|value| value.checked_add(1));
        let height = self
            .placement
            .max_y
            .checked_sub(self.placement.min_y)
            .and_then(|value| value.checked_add(1));
        let (Some(width), Some(height)) = (width, height) else {
            return Err(error(
                "learning-config",
                "learning-config",
                "placement bounds overflow the bounded pilot domain",
            ));
        };
        let area = width.checked_mul(height);
        let pose_upper_bound = area.and_then(|area| {
            let rotations = session
                .problem
                .blocks
                .iter()
                .map(|block| {
                    session
                        .catalogs
                        .facilities
                        .facility(&block.facility)
                        .expect("constructive problem admitted facilities")
                        .allowed_rotations
                        .len() as i64
                })
                .max()
                .unwrap_or(0);
            area.checked_mul(rotations)
        });
        let spatial_upper_bound = session
            .bodies
            .len()
            .checked_add(
                session
                    .occupied
                    .iter()
                    .map(|cells| cells.len())
                    .sum::<usize>(),
            )
            .and_then(|count| count.checked_add(session.active_cells.len()))
            .and_then(|count| count.checked_add(session.terminal_poses.len()))
            .and_then(|count| count.checked_add(2));
        let ordered = self
            .extension_lengths
            .windows(2)
            .all(|pair| pair[0] < pair[1]);
        if self.placement.min_x < 0
            || self.placement.min_y < 0
            || width <= 0
            || height <= 0
            || area.is_none_or(|area| area > 65_536)
            || pose_upper_bound.is_none_or(|count| count > 1_000_000)
            || spatial_upper_bound.is_none_or(|count| count > 1_000_000)
            || self.extension_lengths.is_empty()
            || self.extension_lengths.len() > 256
            || !ordered
            || self
                .extension_lengths
                .iter()
                .any(|&length| length <= 0 || length > session.limits.max_extension)
        {
            return Err(error(
                "learning-config",
                "learning-config",
                "placement bounds or extension lengths are invalid for the bounded pilot",
            ));
        }
        Ok(())
    }
}

impl Session {
    pub fn learning_observation(&self, config: &Config) -> Result<Observation, Diagnostic> {
        config.validate(self)?;
        let domain = Arc::new(Domain::new(self, config));
        if domain.choices(&Cursor::Command).is_empty() && !self.done() {
            return Err(error(
                "empty-learning-domain",
                "learning-domain",
                "the bounded learning domain has no locally legal command",
            ));
        }
        Ok(Observation {
            input: Arc::new(Input::new(self, config)),
            domain,
        })
    }

    /// Builds the policy domain for one learned route option. The problem-order
    /// connection index is stable for the lifetime of the session. Facility and
    /// terminal placement, other unresolved connections and committed routes
    /// are deliberately unavailable to this option.
    pub fn route_learning_observation(
        &self,
        config: &Config,
        connection: usize,
    ) -> Result<Observation, Diagnostic> {
        self.route_learning_observation_with_abort(config, connection, true)
    }

    pub fn route_learning_observation_with_abort(
        &self,
        config: &Config,
        connection: usize,
        allow_abort: bool,
    ) -> Result<Observation, Diagnostic> {
        config.validate(self)?;
        if connection >= self.problem.connections.len()
            || self.routes[connection].is_some()
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.connection != connection)
        {
            return Err(error(
                "route-learning-target",
                "learning-domain",
                "route option target is missing, completed or differs from the active route",
            ));
        }
        let domain = Arc::new(Domain::new_route(self, config, connection, allow_abort));
        if domain.choices(&Cursor::Command).is_empty() {
            return Err(error(
                "empty-route-learning-domain",
                &self.problem.connections[connection].id,
                "the route option has no locally legal command",
            ));
        }
        Ok(Observation {
            input: Arc::new(Input::new(self, config)),
            domain,
        })
    }
}

impl Domain {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn choices(&self, cursor: &Cursor) -> Vec<Choice> {
        match *cursor {
            Cursor::Command => self.command_choices(),
            Cursor::PlaceBlock => self
                .placements
                .iter()
                .map(|placement| {
                    choice(6, placement.block as i64, 0., [placement.block as i64, -1])
                })
                .collect(),
            Cursor::PlacePose { block } => self
                .placements
                .get(block)
                .map(|domain| {
                    self.placement_poses(domain)
                        .into_iter()
                        .map(|(point, rotation)| {
                            let mut value = choice(7, rotation / 90, 0., [domain.block as i64, -1]);
                            value.features[17] = point.x as f32 / self.scale;
                            value.features[18] = point.y as f32 / self.scale;
                            value.features[19] = (rotation as f32).to_radians().sin();
                            value.features[20] = (rotation as f32).to_radians().cos();
                            value
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Cursor::PlaceTerminal => self
                .terminals
                .iter()
                .map(|terminal| choice(8, terminal.terminal as i64, 0., [terminal.entity, -1]))
                .collect(),
            Cursor::TerminalPosition { terminal } => self
                .terminals
                .get(terminal)
                .map(|domain| {
                    self.terminal_positions
                        .iter()
                        .map(|&point| {
                            let mut value = choice(9, 0, 0., [domain.entity, -1]);
                            value.features[17] = point.x as f32 / self.scale;
                            value.features[18] = point.y as f32 / self.scale;
                            value
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Cursor::RouteConnection => self
                .routes
                .iter()
                .map(|route| choice(10, route.connection as i64, 0., [route.entity, -1]))
                .collect(),
            Cursor::RouteSource { connection } => self
                .routes
                .get(connection)
                .map(|route| {
                    route
                        .sources
                        .iter()
                        .enumerate()
                        .map(|(index, _)| {
                            choice(11, index as i64, 0., [route.entity, route.endpoints[0]])
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Cursor::RouteTarget { connection, source } => self
                .routes
                .get(connection)
                .and_then(|route| route.sources.get(source).map(|ports| (route, ports)))
                .map(|(route, ports)| {
                    ports
                        .targets
                        .iter()
                        .enumerate()
                        .map(|(index, _)| {
                            choice(12, index as i64, 0., [route.entity, route.endpoints[1]])
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Cursor::ExtendDirection => self
                .extensions
                .iter()
                .enumerate()
                .map(|(index, domain)| {
                    choice(
                        13,
                        direction_index(domain.direction),
                        index as f32,
                        [self.focus_entity, -1],
                    )
                })
                .collect(),
            Cursor::ExtendLength { direction } => self
                .extensions
                .get(direction)
                .map(|domain| {
                    domain
                        .lengths
                        .iter()
                        .map(|&length| {
                            choice(
                                14,
                                length,
                                length as f32 / self.scale,
                                [self.focus_entity, -1],
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn advance(&self, cursor: &Cursor, selected: usize) -> Result<Advance, Diagnostic> {
        if selected >= self.choices(cursor).len() {
            return Err(error(
                "learning-choice",
                "learning-domain",
                "choice index is outside the current conditional domain",
            ));
        }
        let next = match *cursor {
            Cursor::Command => match self.command_kinds()[selected] {
                CommandKind::Place => Advance::Continue(Cursor::PlaceBlock),
                CommandKind::PlaceTerminal => Advance::Continue(Cursor::PlaceTerminal),
                CommandKind::BeginRoute => Advance::Continue(Cursor::RouteConnection),
                CommandKind::Extend => Advance::Continue(Cursor::ExtendDirection),
                CommandKind::Connect => Advance::Command(self.selected(Command::Connect)),
                CommandKind::AbortRoute => Advance::Command(self.selected(Command::AbortRoute)),
            },
            Cursor::PlaceBlock => Advance::Continue(Cursor::PlacePose { block: selected }),
            Cursor::PlacePose { block } => {
                let domain = &self.placements[block];
                let (position, rotation) = self.placement_poses(domain)[selected];
                Advance::Command(self.selected(Command::Place {
                    block: domain.id.clone(),
                    position,
                    rotation,
                }))
            }
            Cursor::PlaceTerminal => {
                Advance::Continue(Cursor::TerminalPosition { terminal: selected })
            }
            Cursor::TerminalPosition { terminal } => {
                let domain = &self.terminals[terminal];
                Advance::Command(self.selected(Command::PlaceTerminal {
                    terminal: domain.id.clone(),
                    position: self.terminal_positions[selected],
                }))
            }
            Cursor::RouteConnection => Advance::Continue(Cursor::RouteSource {
                connection: selected,
            }),
            Cursor::RouteSource { connection } => Advance::Continue(Cursor::RouteTarget {
                connection,
                source: selected,
            }),
            Cursor::RouteTarget { connection, source } => {
                let route = &self.routes[connection];
                let source = &route.sources[source];
                Advance::Command(self.selected(Command::BeginRoute {
                    connection: route.id.clone(),
                    source_port: source.id.clone(),
                    target_port: source.targets[selected].clone(),
                }))
            }
            Cursor::ExtendDirection => Advance::Continue(Cursor::ExtendLength {
                direction: selected,
            }),
            Cursor::ExtendLength { direction } => {
                let extension = &self.extensions[direction];
                Advance::Command(self.selected(Command::Extend {
                    direction: extension.direction,
                    length: extension.lengths[selected],
                }))
            }
        };
        Ok(next)
    }

    fn selected(&self, command: Command) -> SelectedCommand {
        SelectedCommand {
            revision: self.revision,
            command,
        }
    }

    fn command_kinds(&self) -> Vec<CommandKind> {
        let mut kinds = Vec::with_capacity(6);
        if !self.placements.is_empty() {
            kinds.push(CommandKind::Place);
        }
        if !self.terminals.is_empty() {
            kinds.push(CommandKind::PlaceTerminal);
        }
        if !self.routes.is_empty() {
            kinds.push(CommandKind::BeginRoute);
        }
        if !self.extensions.is_empty() {
            kinds.push(CommandKind::Extend);
        }
        if self.connect {
            kinds.push(CommandKind::Connect);
        }
        if self.abort {
            kinds.push(CommandKind::AbortRoute);
        }
        kinds
    }

    fn command_choices(&self) -> Vec<Choice> {
        self.command_kinds()
            .into_iter()
            .map(|kind| choice(kind as usize, 0, 0., [-1, -1]))
            .collect()
    }

    fn new(session: &Session, config: &Config) -> Self {
        let scale = config
            .placement
            .width()
            .max(config.placement.height())
            .max(1) as f32;
        let blocked = blocked_cells(session);
        let placement = config.placement;
        let placements = placement_domains(session, placement, &blocked);
        let terminal_positions = terminal_positions(session, placement, &blocked);
        let terminals = terminal_domains(session, !terminal_positions.is_empty());
        let routes = route_domains(session);
        let extensions = extension_domains(session, config);
        let connect = session
            .active
            .as_ref()
            .is_some_and(|active| active.cells.last() == Some(&active.target));
        let focus_entity = session.active.as_ref().map_or(-1, |active| {
            (session.problem.blocks.len() + session.problem.terminals.len() + active.connection)
                as i64
        });
        Self {
            revision: session.revision,
            scale,
            placement,
            blocked,
            placements,
            terminals,
            terminal_positions,
            routes,
            extensions,
            focus_entity,
            connect,
            abort: session.active.is_some(),
        }
    }

    fn new_route(session: &Session, config: &Config, connection: usize, allow_abort: bool) -> Self {
        let mut domain = Self::new(session, config);
        domain.placements.clear();
        domain.terminals.clear();
        domain.terminal_positions.clear();
        domain.routes.retain(|route| route.connection == connection);
        domain.abort &= allow_abort;
        domain
    }

    fn placement_poses(&self, domain: &PlacementDomain) -> Vec<(Point, i64)> {
        placement_poses(self.placement, &self.blocked, domain, false)
    }
}

fn choice(kind: usize, value: i64, scalar: f32, references: [i64; 2]) -> Choice {
    let mut features = [0.; CHOICE_FEATURES];
    features[kind] = 1.;
    features[15] = (value as f32).ln_1p();
    features[16] = scalar;
    Choice {
        features,
        references,
    }
}

fn blocked_cells(session: &Session) -> BTreeSet<Point> {
    session
        .bodies
        .iter()
        .copied()
        .chain(
            session
                .occupied
                .iter()
                .flat_map(|occupied| occupied.keys().copied()),
        )
        .chain(
            session
                .terminal_poses
                .iter()
                .flatten()
                .map(|pose| pose.position),
        )
        .collect()
}

fn placement_domains(
    session: &Session,
    placement: Bounds,
    blocked: &BTreeSet<Point>,
) -> Vec<PlacementDomain> {
    if session.active.is_some() {
        return vec![];
    }
    session
        .problem
        .blocks
        .iter()
        .enumerate()
        .filter(|(block, _)| session.poses[*block].is_none())
        .map(|(block, problem)| {
            let definition = session
                .catalogs
                .facilities
                .facility(&problem.facility)
                .expect("constructive problem admitted facilities");
            PlacementDomain {
                block,
                id: problem.id.clone(),
                width: definition.footprint.width,
                height: definition.footprint.height,
                rotations: definition.allowed_rotations.clone(),
            }
        })
        .filter(|domain| !placement_poses(placement, blocked, domain, true).is_empty())
        .collect()
}

fn placement_poses(
    placement: Bounds,
    blocked: &BTreeSet<Point>,
    domain: &PlacementDomain,
    first_only: bool,
) -> Vec<(Point, i64)> {
    let mut poses = Vec::new();
    for &rotation in &domain.rotations {
        let (width, height) = if rotation.rem_euclid(180) == 0 {
            (domain.width, domain.height)
        } else {
            (domain.height, domain.width)
        };
        for y in placement.min_y..=placement.max_y {
            for x in placement.min_x..=placement.max_x {
                let end = x.checked_add(width - 1).zip(y.checked_add(height - 1));
                if end.is_some_and(|(max_x, max_y)| {
                    max_x <= placement.max_x
                        && max_y <= placement.max_y
                        && (y..=max_y).all(|cell_y| {
                            (x..=max_x).all(|cell_x| {
                                !blocked.contains(&Point {
                                    x: cell_x,
                                    y: cell_y,
                                })
                            })
                        })
                }) {
                    poses.push((Point { x, y }, rotation));
                    if first_only {
                        return poses;
                    }
                }
            }
        }
    }
    poses
}

fn terminal_positions(
    session: &Session,
    placement: Bounds,
    blocked: &BTreeSet<Point>,
) -> Vec<Point> {
    if session.active.is_some() {
        return vec![];
    }
    let mut positions = Vec::new();
    for y in placement.min_y..=placement.max_y {
        for x in placement.min_x..=placement.max_x {
            if x != placement.min_x
                && x != placement.max_x
                && y != placement.min_y
                && y != placement.max_y
            {
                continue;
            }
            let point = Point { x, y };
            if !blocked.contains(&point) {
                positions.push(point);
            }
        }
    }
    positions
}

fn terminal_domains(session: &Session, have_positions: bool) -> Vec<TerminalDomain> {
    if !have_positions {
        return vec![];
    }
    session
        .problem
        .terminals
        .iter()
        .enumerate()
        .filter(|(terminal, _)| session.terminal_poses[*terminal].is_none())
        .map(|(terminal, problem)| TerminalDomain {
            terminal,
            entity: (session.problem.blocks.len() + terminal) as i64,
            id: problem.id.clone(),
        })
        .collect()
}

fn endpoint_entity(session: &Session, endpoint: &str) -> i64 {
    if let Some(&block) = session.blocks.get(endpoint) {
        block as i64
    } else {
        (session.problem.blocks.len() + session.terminals[endpoint]) as i64
    }
}

fn endpoint_ports(
    session: &Session,
    endpoint: &str,
    direction: FacilityPortDirection,
    connection: usize,
) -> Vec<Option<String>> {
    let obligation = &session.problem.connections[connection];
    if let Some(&block) = session.blocks.get(endpoint) {
        return session
            .projected_ports(block)
            .into_iter()
            .flatten()
            .filter(|port| {
                port.direction == direction
                    && port.transport == obligation.transport
                    && !session.bodies.contains(&port.connection)
                    && session.shared_cell_available(port.connection, connection)
                    && session
                        .selected_port(
                            block,
                            &port.id,
                            direction,
                            obligation.transport,
                            &obligation.commodity,
                            obligation.rate,
                            &obligation.id,
                        )
                        .is_ok()
            })
            .map(|port| Some(port.id))
            .collect();
    }
    let terminal = session.terminals[endpoint];
    session.terminal_poses[terminal]
        .as_ref()
        .filter(|pose| session.shared_cell_available(pose.position, connection))
        .map(|_| vec![None])
        .unwrap_or_default()
}

fn route_domains(session: &Session) -> Vec<RouteDomain> {
    if session.active.is_some() {
        return vec![];
    }
    session
        .problem
        .connections
        .iter()
        .enumerate()
        .filter(|(connection, _)| session.routes[*connection].is_none())
        .filter_map(|(connection, obligation)| {
            let targets = endpoint_ports(
                session,
                &obligation.target,
                FacilityPortDirection::Input,
                connection,
            );
            let sources = endpoint_ports(
                session,
                &obligation.source,
                FacilityPortDirection::Output,
                connection,
            )
            .into_iter()
            .map(|id| PortDomain {
                id,
                targets: targets.clone(),
            })
            .filter(|source| !source.targets.is_empty())
            .collect::<Vec<_>>();
            (!sources.is_empty()).then_some(RouteDomain {
                connection,
                entity: (session.problem.blocks.len()
                    + session.problem.terminals.len()
                    + connection) as i64,
                endpoints: [
                    endpoint_entity(session, &obligation.source),
                    endpoint_entity(session, &obligation.target),
                ],
                id: obligation.id.clone(),
                sources,
            })
        })
        .collect()
}

fn extension_domains(session: &Session, config: &Config) -> Vec<ExtensionDomain> {
    let Some(active) = session.active.as_ref() else {
        return vec![];
    };
    [
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::North,
    ]
    .into_iter()
    .filter_map(|direction| {
        let (dx, dy) = direction.offset();
        let start = *active.cells.last().expect("active route contains source");
        let lengths = config
            .extension_lengths
            .iter()
            .copied()
            .filter(|&length| {
                (1..=length).all(|step| {
                    let point = start
                        .x
                        .checked_add(dx * step)
                        .zip(start.y.checked_add(dy * step))
                        .map(|(x, y)| Point { x, y });
                    point.is_some_and(|point| {
                        point.x >= config.placement.min_x
                            && point.y >= config.placement.min_y
                            && point.x <= config.placement.max_x
                            && point.y <= config.placement.max_y
                            && !session.bodies.contains(&point)
                            && session.shared_cell_available(point, active.connection)
                            && !session.active_cells.contains(&point)
                    })
                })
            })
            .collect::<Vec<_>>();
        (!lengths.is_empty()).then_some(ExtensionDomain { direction, lengths })
    })
    .collect()
}

fn direction_index(direction: Direction) -> i64 {
    match direction {
        Direction::East => 0,
        Direction::South => 1,
        Direction::West => 2,
        Direction::North => 3,
    }
}

impl Input {
    fn new(session: &Session, config: &Config) -> Self {
        let scale = config
            .placement
            .width()
            .max(config.placement.height())
            .max(1) as f32;
        let blocks = session.problem.blocks.len();
        let terminals = session.problem.terminals.len();
        let connection_offset = blocks + terminals;
        let mut entity_features =
            Vec::with_capacity(connection_offset + session.problem.connections.len());
        for (index, block) in session.problem.blocks.iter().enumerate() {
            let definition = session
                .catalogs
                .facilities
                .facility(&block.facility)
                .expect("constructive problem admitted facilities");
            let mut row = [0.; ENTITY_FEATURES];
            row[0] = 1.;
            row[8] = definition.footprint.width as f32 / scale;
            row[9] = definition.footprint.height as f32 / scale;
            row[10] = definition.allowed_rotations.len() as f32;
            if let Some(pose) = &session.poses[index] {
                row[2] = 1.;
                row[11] = pose.position.x as f32 / scale;
                row[12] = pose.position.y as f32 / scale;
                row[13] = (pose.rotation as f32).to_radians().sin();
                row[14] = (pose.rotation as f32).to_radians().cos();
            }
            if matches!(session.focus, Some(super::Focus::Block { block, .. }) if block == index) {
                row[7] = 1.;
            }
            entity_features.push(row);
        }
        for (index, terminal) in session.problem.terminals.iter().enumerate() {
            let mut row = [0.; ENTITY_FEATURES];
            row[16
                + matches!(
                    terminal.direction,
                    aic_layout::physical::TerminalDirection::Output
                ) as usize] = 1.;
            row[5 + layer(terminal.transport)] = 1.;
            if let Some(pose) = &session.terminal_poses[index] {
                row[2] = 1.;
                row[11] = pose.position.x as f32 / scale;
                row[12] = pose.position.y as f32 / scale;
            }
            if matches!(session.focus, Some(super::Focus::Terminal { terminal, .. }) if terminal == index)
            {
                row[7] = 1.;
            }
            entity_features.push(row);
        }
        let mut relation_index = Vec::with_capacity(session.problem.connections.len() * 2);
        let mut relation_features = Vec::with_capacity(session.problem.connections.len() * 2);
        for (index, connection) in session.problem.connections.iter().enumerate() {
            let entity = connection_offset + index;
            let mut row = [0.; ENTITY_FEATURES];
            row[1] = 1.;
            row[3] = session.routes[index].is_some() as u8 as f32;
            row[4] = session
                .active
                .as_ref()
                .is_some_and(|route| route.connection == index) as u8 as f32;
            row[5 + layer(connection.transport)] = 1.;
            row[15] =
                (connection.rate.numerator as f32 / connection.rate.denominator as f32).ln_1p();
            if matches!(session.focus, Some(super::Focus::Route { connection, .. }) if connection == index)
            {
                row[7] = 1.;
            }
            entity_features.push(row);
            let source = endpoint_entity(session, &connection.source);
            let target = endpoint_entity(session, &connection.target);
            relation_index.extend([[source, entity as i64], [entity as i64, target]]);
            let mut source_row = [0.; RELATION_FEATURES];
            source_row[0] = 1.;
            source_row[2 + layer(connection.transport)] = 1.;
            source_row[4] = row[15];
            let mut target_row = source_row;
            target_row[0] = 0.;
            target_row[1] = 1.;
            relation_features.extend([source_row, target_row]);
        }
        let mut spatial_features = Vec::new();
        let mut spatial_owners = Vec::new();
        for (block, pose) in session.poses.iter().enumerate() {
            let Some(pose) = pose else { continue };
            let definition = session
                .catalogs
                .facilities
                .facility(&session.problem.blocks[block].facility)
                .expect("constructive problem admitted facilities");
            let (width, height, _) = rotate(definition, pose.rotation, pose.position);
            for y in pose.position.y..pose.position.y + height {
                for x in pose.position.x..pose.position.x + width {
                    push_spatial(
                        &mut spatial_features,
                        &mut spatial_owners,
                        Point { x, y },
                        scale,
                        2,
                        block as i64,
                    );
                }
            }
        }
        for (connection, route) in session.routes.iter().enumerate() {
            let Some(route) = route else { continue };
            let kind = 3 + layer(session.problem.connections[connection].transport);
            for &point in &route.cells {
                push_spatial(
                    &mut spatial_features,
                    &mut spatial_owners,
                    point,
                    scale,
                    kind,
                    (connection_offset + connection) as i64,
                );
            }
        }
        for (terminal, pose) in session.terminal_poses.iter().enumerate() {
            let Some(pose) = pose else { continue };
            push_spatial(
                &mut spatial_features,
                &mut spatial_owners,
                pose.position,
                scale,
                8,
                (blocks + terminal) as i64,
            );
        }
        if let Some(active) = &session.active {
            for &point in &active.cells {
                push_spatial(
                    &mut spatial_features,
                    &mut spatial_owners,
                    point,
                    scale,
                    5,
                    (connection_offset + active.connection) as i64,
                );
            }
            let endpoint = *active.cells.last().expect("active route contains source");
            push_spatial(
                &mut spatial_features,
                &mut spatial_owners,
                endpoint,
                scale,
                6,
                (connection_offset + active.connection) as i64,
            );
            push_spatial(
                &mut spatial_features,
                &mut spatial_owners,
                active.target,
                scale,
                7,
                (connection_offset + active.connection) as i64,
            );
        }
        let mut focus_features = [0.; FOCUS_FEATURES];
        let mut focus_entity = -1;
        match &session.focus {
            None => focus_features[0] = 1.,
            Some(super::Focus::Block { block, position }) => {
                focus_features[1] = 1.;
                focus_features[4] = position.x as f32 / scale;
                focus_features[5] = position.y as f32 / scale;
                focus_entity = *block as i64;
            }
            Some(super::Focus::Terminal { terminal, position }) => {
                focus_features[11] = 1.;
                focus_features[4] = position.x as f32 / scale;
                focus_features[5] = position.y as f32 / scale;
                focus_entity = (blocks + terminal) as i64;
            }
            Some(super::Focus::Route {
                connection,
                endpoint,
                target,
                active,
            }) => {
                focus_features[2] = 1.;
                focus_features[3] = *active as u8 as f32;
                focus_features[4] = endpoint.x as f32 / scale;
                focus_features[5] = endpoint.y as f32 / scale;
                focus_features[6] = target.x as f32 / scale;
                focus_features[7] = target.y as f32 / scale;
                focus_features[8] = (target.x - endpoint.x) as f32 / scale;
                focus_features[9] = (target.y - endpoint.y) as f32 / scale;
                focus_features[10] = endpoint.distance(*target) as f32 / scale;
                focus_entity = (connection_offset + connection) as i64;
            }
        }
        let occupied: BTreeSet<_> = session
            .bodies
            .iter()
            .copied()
            .chain(
                session
                    .occupied
                    .iter()
                    .flat_map(|occupied| occupied.keys().copied()),
            )
            .chain(
                session
                    .terminal_poses
                    .iter()
                    .flatten()
                    .map(|pose| pose.position),
            )
            .collect();
        let placed = session.poses.iter().filter(|pose| pose.is_some()).count()
            + session
                .terminal_poses
                .iter()
                .filter(|pose| pose.is_some())
                .count();
        let placeable = session.poses.len() + session.terminal_poses.len();
        let global_features = [
            session.turns as f32 / session.limits.max_turns as f32,
            (session.limits.max_turns - session.turns) as f32 / session.limits.max_turns as f32,
            session.completed() as f32 / session.routes.len() as f32,
            placed as f32 / placeable as f32,
            session.active.is_some() as u8 as f32,
            occupied.len() as f32 / (config.placement.width() * config.placement.height()) as f32,
            config.placement.width() as f32 / scale,
            config.placement.height() as f32 / scale,
        ];
        Self {
            schema: SCHEMA,
            entity_features,
            relation_index,
            relation_features,
            spatial_features,
            spatial_owners,
            global_features,
            focus_features,
            focus_entity,
        }
    }
}

fn push_spatial(
    rows: &mut Vec<[f32; SPATIAL_FEATURES]>,
    owners: &mut Vec<i64>,
    point: Point,
    scale: f32,
    kind: usize,
    owner: i64,
) {
    let mut row = [0.; SPATIAL_FEATURES];
    row[0] = point.x as f32 / scale;
    row[1] = point.y as f32 / scale;
    row[kind] = 1.;
    rows.push(row);
    owners.push(owner);
}
