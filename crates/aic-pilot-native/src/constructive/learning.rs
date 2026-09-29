//! Versioned numerical state and conditional command domains for direct construction.

use std::sync::Arc;

use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Bounds, Point, layer},
    physical::{Bridge, BridgeAxis, BridgeChannel},
    projection::rotate,
};
use serde::{Deserialize, Serialize};

use super::routing_candidates::{AllowanceStop, RequestAllowance, RequestFailure, WorkKind};
use super::{Command, Diagnostic, Direction, Session, error};

mod pose_index;
mod route_graph_cursor;
use pose_index::PoseIndex;
pub(crate) use route_graph_cursor::RouteGraphCursor;

pub const SCHEMA: &str = "aic-constructive-learning-v3";
pub const ENTITY_FEATURES: usize = 18;
pub const RELATION_FEATURES: usize = 6;
pub const SPATIAL_FEATURES: usize = 14;
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
    /// Exact installed components supplement the fixed-size numerical rows.
    pub bridge_resources: Vec<Bridge>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Observation {
    pub input: Arc<Input>,
    pub domain: Arc<Domain>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum CommandKind {
    Place,
    PlaceTerminal,
    BeginRoute,
    Extend,
    Connect,
    AbortRoute,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedFactor {
    pub cursor: Cursor,
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactoredCommand {
    pub factors: Vec<SelectedFactor>,
    pub command: SelectedCommand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandPage {
    pub commands: Vec<FactoredCommand>,
    pub next_offset: usize,
    pub exhausted: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Domain {
    revision: u64,
    scale: f32,
    placement: Bounds,
    pose_index: Option<PoseIndex>,
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
            .occupied
            .iter()
            .try_fold(session.bodies.len(), |count, cells| {
                count.checked_add(cells.len())
            })
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
            || area.is_none()
            || pose_upper_bound.is_none()
            || spatial_upper_bound.is_none()
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
    pub fn learning_domain(&self, config: &Config) -> Result<Arc<Domain>, Diagnostic> {
        config.validate(self)?;
        Ok(Arc::new(Domain::new(self, config)?))
    }

    /// A placement-only view preserves the Place and PlaceTerminal factor ranks
    /// while avoiding unrelated route-port collections in a paged search.
    pub fn learning_placement_domain(&self, config: &Config) -> Result<Arc<Domain>, Diagnostic> {
        config.validate(self)?;
        Ok(Arc::new(Domain::new_placement(self, config)?))
    }

    /// Close checked construction peak for the placement-only domain. The
    /// caller reserves this before the occupancy prefix or shape rows exist.
    pub fn learning_placement_domain_peak_bytes(
        &self,
        config: &Config,
    ) -> Result<usize, Diagnostic> {
        config.validate(self)?;
        let add =
            |left: usize, right: usize| left.checked_add(right).ok_or_else(learning_size_error);
        let multiply =
            |left: usize, right: usize| left.checked_mul(right).ok_or_else(learning_size_error);
        let blocked_count = self
            .occupied
            .iter()
            .try_fold(self.bodies.len(), |count, layer| add(count, layer.len()))?;
        let blocked_count = add(blocked_count, self.terminal_poses.len())?;
        let mut bytes = add(
            std::mem::size_of::<Domain>() + 2 * std::mem::size_of::<usize>(),
            multiply(blocked_count, std::mem::size_of::<Point>())?,
        )?;
        bytes = add(
            bytes,
            multiply(
                self.problem.blocks.len(),
                std::mem::size_of::<PlacementDomain>(),
            )?,
        )?;
        let mut rotation_count = 0usize;
        for (block, problem) in self.problem.blocks.iter().enumerate() {
            if self.poses[block].is_some() {
                continue;
            }
            let definition = self
                .catalogs
                .facilities
                .facility(&problem.facility)
                .expect("constructive problem admitted facilities");
            rotation_count = add(rotation_count, definition.allowed_rotations.len())?;
            bytes = add(bytes, problem.id.len())?;
        }
        bytes = add(bytes, multiply(rotation_count, std::mem::size_of::<i64>())?)?;
        if rotation_count > 0 {
            bytes = add(
                bytes,
                PoseIndex::construction_upper_bytes(config.placement, rotation_count)?,
            )?;
        }
        let width = usize::try_from(config.placement.width()).map_err(|_| learning_size_error())?;
        let height =
            usize::try_from(config.placement.height()).map_err(|_| learning_size_error())?;
        let perimeter = if width == 1 || height == 1 {
            width.max(height)
        } else {
            add(multiply(width, 2)?, multiply(height - 2, 2)?)?
        };
        bytes = add(bytes, multiply(perimeter, std::mem::size_of::<Point>())?)?;
        bytes = add(
            bytes,
            multiply(
                self.problem.terminals.len(),
                std::mem::size_of::<TerminalDomain>(),
            )?,
        )?;
        for (terminal, problem) in self.problem.terminals.iter().enumerate() {
            if self.terminal_poses[terminal].is_none() {
                bytes = add(bytes, problem.id.len())?;
            }
        }
        // `try_reserve_exact` may receive an allocator-rounded capacity.
        add(add(bytes, bytes / 8)?, 4096)
    }

    pub fn learning_observation(&self, config: &Config) -> Result<Observation, Diagnostic> {
        let domain = self.learning_domain(config)?;
        if domain.choices(&Cursor::Command)?.is_empty() && !self.done() {
            return Err(error(
                "empty-learning-domain",
                "learning-domain",
                "the bounded learning domain has no locally legal command",
            ));
        }
        Ok(Observation {
            input: Arc::new(Input::new(self, config)?),
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
        let domain = Arc::new(Domain::new_route(self, config, connection, allow_abort)?);
        if domain.choices(&Cursor::Command)?.is_empty() {
            return Err(error(
                "empty-route-learning-domain",
                &self.problem.connections[connection].id,
                "the route option has no locally legal command",
            ));
        }
        Ok(Observation {
            input: Arc::new(Input::new(self, config)?),
            domain,
        })
    }

    /// Encodes a stable state with an unresolved connection as the explicit
    /// focus. Candidate guidance uses this instead of inheriting the most
    /// recent constructive command as its attention query.
    pub fn route_guidance_learning_input(
        &self,
        config: &Config,
        connection: usize,
    ) -> Result<Arc<Input>, Diagnostic> {
        config.validate(self)?;
        if self.active.is_some()
            || connection >= self.problem.connections.len()
            || self.routes[connection].is_some()
        {
            return Err(error(
                "route-guidance-target",
                "learning-domain",
                "route guidance requires an unresolved connection in a stable state",
            ));
        }
        Ok(Arc::new(Input::new_for_route(self, config, connection)?))
    }

    /// Returns the retained byte charge with the input. The caller releases
    /// that charge after the input is no longer live.
    pub fn route_guidance_learning_input_with_allowance(
        &self,
        config: &Config,
        connection: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<(Arc<Input>, usize), RequestFailure> {
        config.validate(self)?;
        if self.active.is_some()
            || connection >= self.problem.connections.len()
            || self.routes[connection].is_some()
        {
            return Err(error(
                "route-guidance-target",
                "learning-domain",
                "route guidance requires an unresolved connection in a stable state",
            )
            .into());
        }
        let (input, reserved_bytes) =
            Input::new_for_route_bounded(self, config, connection, allowance)?;
        Ok((Arc::new(input), reserved_bytes))
    }

    /// Encodes a stable macro-search state without inheriting the most recent
    /// primitive command as the policy query.
    pub fn macro_learning_input(&self, config: &Config) -> Result<Arc<Input>, Diagnostic> {
        config.validate(self)?;
        if self.active.is_some() {
            return Err(error(
                "macro-learning-state",
                "learning-domain",
                "macro-search encoding requires a stable state",
            ));
        }
        Ok(Arc::new(Input::new_for_macro(self, config)?))
    }
}

impl Domain {
    /// Retained host bytes for the source-bound placement and command view.
    pub fn retained_bytes(&self) -> usize {
        fn vector_bytes<T>(items: &Vec<T>) -> usize {
            items.capacity().saturating_mul(std::mem::size_of::<T>())
        }
        let mut bytes = std::mem::size_of::<Self>()
            .saturating_add(2 * std::mem::size_of::<usize>())
            .saturating_add(vector_bytes(&self.placements))
            .saturating_add(vector_bytes(&self.terminals))
            .saturating_add(vector_bytes(&self.terminal_positions))
            .saturating_add(vector_bytes(&self.routes))
            .saturating_add(vector_bytes(&self.extensions))
            .saturating_add(self.pose_index.as_ref().map_or(0, PoseIndex::heap_bytes));
        for placement in &self.placements {
            bytes = bytes
                .saturating_add(placement.id.capacity())
                .saturating_add(vector_bytes(&placement.rotations));
        }
        for terminal in &self.terminals {
            bytes = bytes.saturating_add(terminal.id.capacity());
        }
        for route in &self.routes {
            bytes = bytes
                .saturating_add(route.id.capacity())
                .saturating_add(vector_bytes(&route.sources));
            for source in &route.sources {
                bytes = bytes
                    .saturating_add(source.id.as_ref().map_or(0, String::capacity))
                    .saturating_add(vector_bytes(&source.targets));
                for target in &source.targets {
                    bytes = bytes.saturating_add(target.as_ref().map_or(0, String::capacity));
                }
            }
        }
        for extension in &self.extensions {
            bytes = bytes.saturating_add(vector_bytes(&extension.lengths));
        }
        bytes
    }

    pub fn source_revision(&self) -> u64 {
        self.revision
    }

    pub fn placement_bounds(&self) -> Bounds {
        self.placement
    }

    /// Count the complete native placement families without materializing pages.
    /// This uses the same pose counts and terminal product as `command_page`.
    pub fn placement_command_count(&self) -> Result<usize, Diagnostic> {
        let blocks = checked_total(
            self.placements
                .iter()
                .map(|placement| self.pose_count(placement)),
        )?;
        let terminals = self
            .terminals
            .len()
            .checked_mul(self.terminal_positions.len())
            .ok_or_else(learning_size_error)?;
        blocks
            .checked_add(terminals)
            .ok_or_else(learning_size_error)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn choices(&self, cursor: &Cursor) -> Result<Vec<Choice>, Diagnostic> {
        match *cursor {
            Cursor::Command => self.command_choices(),
            Cursor::PlaceBlock => self
                .placements
                .iter()
                .map(|placement| {
                    choice(6, placement.block as i64, 0., [placement.block as i64, -1])
                })
                .checked_collect(),
            Cursor::PlacePose { block } => self
                .placements
                .get(block)
                .map(|domain| {
                    (0..self.pose_count(domain))
                        .map(|rank| {
                            let (point, rotation) = self.pose_at(domain, rank);
                            let mut value = choice(7, rotation / 90, 0., [domain.block as i64, -1]);
                            value.features[17] = point.x as f32 / self.scale;
                            value.features[18] = point.y as f32 / self.scale;
                            value.features[19] = (rotation as f32).to_radians().sin();
                            value.features[20] = (rotation as f32).to_radians().cos();
                            value
                        })
                        .checked_collect()
                })
                .transpose()
                .map(|choices| choices.unwrap_or_default()),
            Cursor::PlaceTerminal => self
                .terminals
                .iter()
                .map(|terminal| choice(8, terminal.terminal as i64, 0., [terminal.entity, -1]))
                .checked_collect(),
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
                        .checked_collect()
                })
                .transpose()
                .map(|choices| choices.unwrap_or_default()),
            Cursor::RouteConnection => self
                .routes
                .iter()
                .map(|route| choice(10, route.connection as i64, 0., [route.entity, -1]))
                .checked_collect(),
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
                        .checked_collect()
                })
                .transpose()
                .map(|choices| choices.unwrap_or_default()),
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
                        .checked_collect()
                })
                .transpose()
                .map(|choices| choices.unwrap_or_default()),
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
                .checked_collect(),
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
                        .checked_collect()
                })
                .transpose()
                .map(|choices| choices.unwrap_or_default()),
        }
    }

    pub fn advance(&self, cursor: &Cursor, selected: usize) -> Result<Advance, Diagnostic> {
        self.advance_many(cursor, &[selected])
            .map(|mut advances| advances.pop().expect("one selection produces one advance"))
    }

    /// Decode several choices from the same conditional factor while
    /// materializing large placement banks only once. Policy search uses this
    /// for its bounded beam; ordinary one-choice decoding keeps the same
    /// contract through `advance`.
    pub fn advance_many(
        &self,
        cursor: &Cursor,
        selected: &[usize],
    ) -> Result<Vec<Advance>, Diagnostic> {
        let command_kinds =
            matches!(cursor, Cursor::Command).then(|| self.available_command_kinds());
        let choice_count = match *cursor {
            Cursor::Command => command_kinds.as_ref().unwrap().len(),
            Cursor::PlaceBlock => self.placements.len(),
            Cursor::PlacePose { block } => self
                .placements
                .get(block)
                .map_or(0, |domain| self.pose_count(domain)),
            Cursor::PlaceTerminal => self.terminals.len(),
            Cursor::TerminalPosition { .. } => self.terminal_positions.len(),
            Cursor::RouteConnection => self.routes.len(),
            Cursor::RouteSource { connection } => self
                .routes
                .get(connection)
                .map_or(0, |route| route.sources.len()),
            Cursor::RouteTarget { connection, source } => self
                .routes
                .get(connection)
                .and_then(|route| route.sources.get(source))
                .map_or(0, |ports| ports.targets.len()),
            Cursor::ExtendDirection => self.extensions.len(),
            Cursor::ExtendLength { direction } => self
                .extensions
                .get(direction)
                .map_or(0, |extension| extension.lengths.len()),
        };
        if selected.iter().any(|&index| index >= choice_count) {
            return Err(error(
                "learning-choice",
                "learning-domain",
                "choice index is outside the current conditional domain",
            ));
        }
        let mut advances = Vec::new();
        advances
            .try_reserve_exact(selected.len())
            .map_err(|_| learning_size_error())?;
        for &selected in selected {
            advances.push(match *cursor {
                Cursor::Command => match command_kinds.as_ref().unwrap()[selected] {
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
                    let (position, rotation) = self.pose_at(domain, selected);
                    Advance::Command(self.selected(Command::Place {
                        block: checked_string(&domain.id)?,
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
                        terminal: checked_string(&domain.id)?,
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
                        connection: checked_string(&route.id)?,
                        source_port: checked_optional_string(&source.id)?,
                        target_port: checked_optional_string(&source.targets[selected])?,
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
            });
        }
        Ok(advances)
    }

    fn selected(&self, command: Command) -> SelectedCommand {
        SelectedCommand {
            revision: self.revision,
            command,
        }
    }

    pub fn available_command_kinds(&self) -> Vec<CommandKind> {
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

    /// Return a deterministic slice of every locally legal complete command in
    /// one command family. The factor trace uses the exact local indices that
    /// `choices` and `advance` expose, so a policy can score these commands
    /// without deciding whether they are eligible for search.
    pub fn command_page(
        &self,
        kind: CommandKind,
        offset: usize,
        limit: usize,
    ) -> Result<CommandPage, Diagnostic> {
        let kinds = self.available_command_kinds();
        let Some(root_selected) = kinds.iter().position(|candidate| *candidate == kind) else {
            return Ok(CommandPage {
                commands: vec![],
                next_offset: offset,
                exhausted: true,
            });
        };
        let root = SelectedFactor {
            cursor: Cursor::Command,
            selected: root_selected,
        };
        if kind == CommandKind::Place {
            let mut lengths = Vec::new();
            lengths
                .try_reserve_exact(self.placements.len())
                .map_err(|_| learning_size_error())?;
            lengths.extend(
                self.placements
                    .iter()
                    .map(|placement| self.pose_count(placement)),
            );
            let total = checked_total(lengths.iter().copied())?;
            let page = interleaved_page(&lengths, offset, limit)?;
            let mut commands = Vec::new();
            reserve_commands(&mut commands, page.len())?;
            for (block, pose_rank) in page {
                let pose = stratified_index(pose_rank, lengths[block], block);
                let (position, rotation) = self.pose_at(&self.placements[block], pose);
                commands.push(FactoredCommand {
                    factors: checked_factors([
                        root.clone(),
                        SelectedFactor {
                            cursor: Cursor::PlaceBlock,
                            selected: block,
                        },
                        SelectedFactor {
                            cursor: Cursor::PlacePose { block },
                            selected: pose,
                        },
                    ])?,
                    command: self.selected(Command::Place {
                        block: checked_string(&self.placements[block].id)?,
                        position,
                        rotation,
                    }),
                });
            }
            let next_offset = offset.saturating_add(commands.len()).min(total);
            return Ok(CommandPage {
                next_offset,
                exhausted: next_offset == total,
                commands,
            });
        }
        let total = match kind {
            CommandKind::Place => unreachable!("place pages return above"),
            CommandKind::PlaceTerminal => self
                .terminals
                .len()
                .checked_mul(self.terminal_positions.len())
                .ok_or_else(learning_size_error)?,
            CommandKind::BeginRoute => checked_total(
                self.routes
                    .iter()
                    .flat_map(|route| &route.sources)
                    .map(|source| source.targets.len()),
            )?,
            CommandKind::Extend => checked_total(
                self.extensions
                    .iter()
                    .map(|extension| extension.lengths.len()),
            )?,
            CommandKind::Connect | CommandKind::AbortRoute => 1,
        };
        let start = offset.min(total);
        let end = start.saturating_add(limit).min(total);
        let mut commands = Vec::new();
        reserve_commands(&mut commands, end - start)?;
        match kind {
            CommandKind::Place => unreachable!("place pages return above"),
            CommandKind::PlaceTerminal => {
                let positions = self.terminal_positions.len();
                for flat in start..end {
                    let terminal = flat % self.terminals.len();
                    let pose_rank = flat / self.terminals.len();
                    let position = stratified_index(pose_rank, positions, terminal);
                    commands.push(FactoredCommand {
                        factors: checked_factors([
                            root.clone(),
                            SelectedFactor {
                                cursor: Cursor::PlaceTerminal,
                                selected: terminal,
                            },
                            SelectedFactor {
                                cursor: Cursor::TerminalPosition { terminal },
                                selected: position,
                            },
                        ])?,
                        command: self.selected(Command::PlaceTerminal {
                            terminal: checked_string(&self.terminals[terminal].id)?,
                            position: self.terminal_positions[position],
                        }),
                    });
                }
            }
            CommandKind::BeginRoute => {
                let mut lengths = Vec::new();
                lengths
                    .try_reserve_exact(self.routes.len())
                    .map_err(|_| learning_size_error())?;
                for route in &self.routes {
                    lengths.push(checked_total(
                        route.sources.iter().map(|ports| ports.targets.len()),
                    )?);
                }
                for (connection, pair_rank) in interleaved_page(&lengths, start, end - start)? {
                    let route = &self.routes[connection];
                    let mut remaining = pair_rank;
                    let (source, target) = route
                        .sources
                        .iter()
                        .enumerate()
                        .find_map(|(source, ports)| {
                            if remaining < ports.targets.len() {
                                Some((source, remaining))
                            } else {
                                remaining -= ports.targets.len();
                                None
                            }
                        })
                        .expect("pair rank stays within route source bank");
                    let ports = &route.sources[source];
                    commands.push(FactoredCommand {
                        factors: checked_factors([
                            root.clone(),
                            SelectedFactor {
                                cursor: Cursor::RouteConnection,
                                selected: connection,
                            },
                            SelectedFactor {
                                cursor: Cursor::RouteSource { connection },
                                selected: source,
                            },
                            SelectedFactor {
                                cursor: Cursor::RouteTarget { connection, source },
                                selected: target,
                            },
                        ])?,
                        command: self.selected(Command::BeginRoute {
                            connection: checked_string(&route.id)?,
                            source_port: checked_optional_string(&ports.id)?,
                            target_port: checked_optional_string(&ports.targets[target])?,
                        }),
                    });
                }
            }
            CommandKind::Extend => {
                let mut lengths = Vec::new();
                lengths
                    .try_reserve_exact(self.extensions.len())
                    .map_err(|_| learning_size_error())?;
                lengths.extend(
                    self.extensions
                        .iter()
                        .map(|extension| extension.lengths.len()),
                );
                for (direction, length) in interleaved_page(&lengths, start, end - start)? {
                    let extension = &self.extensions[direction];
                    commands.push(FactoredCommand {
                        factors: checked_factors([
                            root.clone(),
                            SelectedFactor {
                                cursor: Cursor::ExtendDirection,
                                selected: direction,
                            },
                            SelectedFactor {
                                cursor: Cursor::ExtendLength { direction },
                                selected: length,
                            },
                        ])?,
                        command: self.selected(Command::Extend {
                            direction: extension.direction,
                            length: extension.lengths[length],
                        }),
                    });
                }
            }
            CommandKind::Connect => {
                if start == 0 && end == 1 {
                    commands.push(FactoredCommand {
                        factors: checked_factors([root])?,
                        command: self.selected(Command::Connect),
                    });
                }
            }
            CommandKind::AbortRoute => {
                if start == 0 && end == 1 {
                    commands.push(FactoredCommand {
                        factors: checked_factors([root])?,
                        command: self.selected(Command::AbortRoute),
                    });
                }
            }
        }
        Ok(CommandPage {
            commands,
            next_offset: end,
            exhausted: end >= total,
        })
    }

    fn command_choices(&self) -> Result<Vec<Choice>, Diagnostic> {
        self.available_command_kinds()
            .into_iter()
            .map(|kind| choice(kind as usize, 0, 0., [-1, -1]))
            .checked_collect()
    }

    fn new(session: &Session, config: &Config) -> Result<Self, Diagnostic> {
        Self::new_with_routes(session, config, true)
    }

    fn new_placement(session: &Session, config: &Config) -> Result<Self, Diagnostic> {
        Self::new_with_routes(session, config, false)
    }

    fn new_with_routes(
        session: &Session,
        config: &Config,
        include_routes: bool,
    ) -> Result<Self, Diagnostic> {
        let scale = config
            .placement
            .width()
            .max(config.placement.height())
            .max(1) as f32;
        let blocked = blocked_cells(session)?;
        let mut placements = placement_domains(session)?;
        let pose_index = (!placements.is_empty())
            .then(|| PoseIndex::new(config.placement, &blocked, &placements))
            .transpose()?;
        if let Some(index) = &pose_index {
            placements.retain(|placement| index.pose_count(placement) > 0);
        }
        let terminal_positions = terminal_positions(session, config.placement, &blocked)?;
        let terminals = terminal_domains(session, !terminal_positions.is_empty())?;
        let routes = if include_routes {
            route_domains(session)?
        } else {
            Vec::new()
        };
        let extensions = extension_domains(session, config);
        let connect = session
            .active
            .as_ref()
            .is_some_and(|active| active.cells.last() == Some(&active.target));
        let focus_entity = session.active.as_ref().map_or(-1, |active| {
            (session.problem.blocks.len() + session.problem.terminals.len() + active.connection)
                as i64
        });
        Ok(Self {
            revision: session.revision,
            scale,
            placement: config.placement,
            pose_index,
            placements,
            terminals,
            terminal_positions,
            routes,
            extensions,
            focus_entity,
            connect,
            abort: session.active.is_some(),
        })
    }

    fn new_route(
        session: &Session,
        config: &Config,
        connection: usize,
        allow_abort: bool,
    ) -> Result<Self, Diagnostic> {
        let scale = config
            .placement
            .width()
            .max(config.placement.height())
            .max(1) as f32;
        let mut routes = Vec::new();
        if session.active.is_none() {
            if let Some(route) = route_domain_for(session, connection)? {
                routes
                    .try_reserve_exact(1)
                    .map_err(|_| learning_size_error())?;
                routes.push(route);
            }
        }
        let extensions = extension_domains(session, config);
        let connect = session
            .active
            .as_ref()
            .is_some_and(|active| active.cells.last() == Some(&active.target));
        let focus_entity = session.active.as_ref().map_or(-1, |active| {
            (session.problem.blocks.len() + session.problem.terminals.len() + active.connection)
                as i64
        });
        Ok(Self {
            revision: session.revision,
            scale,
            placement: config.placement,
            pose_index: None,
            placements: Vec::new(),
            terminals: Vec::new(),
            terminal_positions: Vec::new(),
            routes,
            extensions,
            focus_entity,
            connect,
            abort: session.active.is_some() && allow_abort,
        })
    }

    fn pose_count(&self, domain: &PlacementDomain) -> usize {
        self.pose_index
            .as_ref()
            .expect("placement domain has its index")
            .pose_count(domain)
    }

    fn pose_at(&self, domain: &PlacementDomain, rank: usize) -> (Point, i64) {
        self.pose_index
            .as_ref()
            .expect("placement domain has its index")
            .pose_at(domain, rank)
            .expect("legal pose rank remains in indexed domain")
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

trait CheckedChoiceCollect: Iterator<Item = Choice> + Sized {
    fn checked_collect(self) -> Result<Vec<Choice>, Diagnostic> {
        let mut choices = Vec::new();
        let (lower, upper) = self.size_hint();
        choices
            .try_reserve_exact(upper.unwrap_or(lower))
            .map_err(|_| {
                error(
                    "learning-resource",
                    "learning-domain",
                    "conditional choice allocation failed",
                )
            })?;
        for choice in self {
            if choices.len() == choices.capacity() {
                choices.try_reserve(1).map_err(|_| {
                    error(
                        "learning-resource",
                        "learning-domain",
                        "conditional choice allocation failed",
                    )
                })?;
            }
            choices.push(choice);
        }
        Ok(choices)
    }
}

impl<T: Iterator<Item = Choice>> CheckedChoiceCollect for T {}

fn interleaved_page(
    lengths: &[usize],
    offset: usize,
    limit: usize,
) -> Result<Vec<(usize, usize)>, Diagnostic> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let maximum = lengths.iter().copied().max().unwrap_or(0);
    let covered =
        |rank: usize| -> u128 { lengths.iter().map(|&length| length.min(rank) as u128).sum() };
    if covered(maximum) <= offset as u128 {
        return Ok(Vec::new());
    }
    let mut low = 0usize;
    let mut high = maximum;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if covered(middle) <= offset as u128 {
            low = middle;
        } else {
            high = middle;
        }
    }
    let mut seen = covered(low) as usize;
    let mut page = Vec::new();
    let available = (covered(maximum) - offset as u128).min(limit as u128) as usize;
    page.try_reserve_exact(available)
        .map_err(|_| learning_size_error())?;
    'ranks: for rank in low..maximum {
        for (entity, &length) in lengths.iter().enumerate() {
            if rank >= length {
                continue;
            }
            if seen >= offset {
                page.push((entity, rank));
                if page.len() == limit {
                    break 'ranks;
                }
            }
            seen += 1;
        }
    }
    Ok(page)
}

fn learning_size_error() -> Diagnostic {
    error(
        "learning-resource",
        "learning-domain",
        "command output size or allocation exceeds available resources",
    )
}

fn checked_total(mut items: impl Iterator<Item = usize>) -> Result<usize, Diagnostic> {
    items.try_fold(0usize, |total, item| {
        total.checked_add(item).ok_or_else(learning_size_error)
    })
}

fn reserve_commands(commands: &mut Vec<FactoredCommand>, count: usize) -> Result<(), Diagnostic> {
    commands
        .try_reserve_exact(count)
        .map_err(|_| learning_size_error())
}

fn checked_factors<const N: usize>(
    items: [SelectedFactor; N],
) -> Result<Vec<SelectedFactor>, Diagnostic> {
    let mut factors = Vec::new();
    factors
        .try_reserve_exact(N)
        .map_err(|_| learning_size_error())?;
    factors.extend(items);
    Ok(factors)
}

fn checked_string(value: &str) -> Result<String, Diagnostic> {
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| learning_size_error())?;
    output.push_str(value);
    Ok(output)
}

fn checked_optional_string(value: &Option<String>) -> Result<Option<String>, Diagnostic> {
    value.as_deref().map(checked_string).transpose()
}

#[cfg(test)]
#[test]
fn interleaved_pages_keep_original_factor_order_at_large_offsets() {
    for lengths in [vec![], vec![0, 0], vec![1, 5, 2, 0, 8], vec![97, 3, 41]] {
        let expected = (0..lengths.iter().copied().max().unwrap_or(0))
            .flat_map(|rank| {
                lengths
                    .iter()
                    .enumerate()
                    .filter_map(move |(entity, &length)| (rank < length).then_some((entity, rank)))
            })
            .collect::<Vec<_>>();
        for offset in 0..=expected.len() + 1 {
            for limit in [0, 1, 2, 7, expected.len() + 1] {
                assert_eq!(
                    interleaved_page(&lengths, offset, limit).unwrap(),
                    expected
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .copied()
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

/// A deterministic full-cycle permutation used to make early pages cover a
/// large conditional bank instead of one adjacent row. Because the stride is
/// coprime to `count`, every original factor index appears exactly once.
fn stratified_index(rank: usize, count: usize, salt: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let mut stride = ((count as u128 * 618_033_989u128) / 1_000_000_000u128) as usize;
    stride = stride.max(1) % count;
    while greatest_common_divisor(stride, count) != 1 {
        stride += 1;
        if stride == count {
            stride = 1;
        }
    }
    let start = (count / 8 + salt.wrapping_mul(104_729)) % count;
    (start + ((rank as u128 * stride as u128) % count as u128) as usize) % count
}

fn greatest_common_divisor(mut left: usize, mut right: usize) -> usize {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn blocked_cells(session: &Session) -> Result<Vec<Point>, Diagnostic> {
    blocked_cells_bounded(session, false).map_err(|failure| match failure {
        RequestFailure::Invalid(diagnostic) => diagnostic,
        _ => unreachable!("unbounded blocked-cell builder returns diagnostics"),
    })
}

fn blocked_cells_bounded(session: &Session, bounded: bool) -> Result<Vec<Point>, RequestFailure> {
    let capacity = session
        .occupied
        .iter()
        .try_fold(session.bodies.len(), |count, layer| {
            count.checked_add(layer.len())
        })
        .and_then(|count| count.checked_add(session.terminal_poses.len()))
        .ok_or_else(|| {
            RequestFailure::Invalid(error(
                "learning-resource",
                "learning-domain",
                "blocked-cell count is not representable",
            ))
        })?;
    let mut blocked = Vec::new();
    blocked.try_reserve_exact(capacity).map_err(|_| {
        if bounded {
            host_allocation_stop(capacity.saturating_mul(std::mem::size_of::<Point>()))
        } else {
            RequestFailure::Invalid(error(
                "learning-resource",
                "learning-domain",
                "blocked-cell allocation exceeds available memory",
            ))
        }
    })?;
    blocked.extend(session.bodies.iter().copied());
    for layer in &session.occupied {
        blocked.extend(layer.keys().copied());
    }
    blocked.extend(
        session
            .terminal_poses
            .iter()
            .flatten()
            .map(|pose| pose.position),
    );
    blocked.sort_unstable();
    blocked.dedup();
    Ok(blocked)
}

fn placement_domains(session: &Session) -> Result<Vec<PlacementDomain>, Diagnostic> {
    if session.active.is_some() {
        return Ok(vec![]);
    }
    let mut domains = Vec::new();
    domains
        .try_reserve_exact(session.problem.blocks.len())
        .map_err(|_| {
            error(
                "learning-resource",
                "learning-domain",
                "placement domain allocation failed",
            )
        })?;
    for (block, problem) in session.problem.blocks.iter().enumerate() {
        if session.poses[block].is_none() {
            let definition = session
                .catalogs
                .facilities
                .facility(&problem.facility)
                .expect("constructive problem admitted facilities");
            domains.push(PlacementDomain {
                block,
                id: checked_string(&problem.id)?,
                width: definition.footprint.width,
                height: definition.footprint.height,
                rotations: {
                    let mut rotations = Vec::new();
                    rotations
                        .try_reserve_exact(definition.allowed_rotations.len())
                        .map_err(|_| learning_size_error())?;
                    rotations.extend_from_slice(&definition.allowed_rotations);
                    rotations
                },
            });
        }
    }
    Ok(domains)
}

fn terminal_positions(
    session: &Session,
    placement: Bounds,
    blocked: &[Point],
) -> Result<Vec<Point>, Diagnostic> {
    if session.active.is_some() {
        return Ok(vec![]);
    }
    let mut positions = Vec::new();
    let width = usize::try_from(placement.width()).map_err(|_| {
        error(
            "learning-resource",
            "learning-domain",
            "terminal boundary width overflows",
        )
    })?;
    let height = usize::try_from(placement.height()).map_err(|_| {
        error(
            "learning-resource",
            "learning-domain",
            "terminal boundary height overflows",
        )
    })?;
    let perimeter = if height == 1 {
        width
    } else if width == 1 {
        height
    } else {
        width
            .checked_mul(2)
            .and_then(|count| count.checked_add((height - 2).checked_mul(2)?))
            .ok_or_else(|| {
                error(
                    "learning-resource",
                    "learning-domain",
                    "terminal boundary count overflows",
                )
            })?
    };
    positions.try_reserve_exact(perimeter).map_err(|_| {
        error(
            "learning-resource",
            "learning-domain",
            "terminal boundary allocation failed",
        )
    })?;
    for y in placement.min_y..=placement.max_y {
        if y == placement.min_y || y == placement.max_y {
            for x in placement.min_x..=placement.max_x {
                let point = Point { x, y };
                if blocked.binary_search(&point).is_err() {
                    positions.push(point);
                }
            }
        } else {
            for x in [placement.min_x, placement.max_x] {
                if x == placement.max_x && x == placement.min_x {
                    continue;
                }
                let point = Point { x, y };
                if blocked.binary_search(&point).is_err() {
                    positions.push(point);
                }
            }
        }
    }
    Ok(positions)
}

fn terminal_domains(
    session: &Session,
    have_positions: bool,
) -> Result<Vec<TerminalDomain>, Diagnostic> {
    if !have_positions {
        return Ok(vec![]);
    }
    let mut domains = Vec::new();
    domains
        .try_reserve_exact(session.problem.terminals.len())
        .map_err(|_| {
            error(
                "learning-resource",
                "learning-domain",
                "terminal domain allocation failed",
            )
        })?;
    for (terminal, problem) in session.problem.terminals.iter().enumerate() {
        if session.terminal_poses[terminal].is_none() {
            domains.push(TerminalDomain {
                terminal,
                entity: (session.problem.blocks.len() + terminal) as i64,
                id: checked_string(&problem.id)?,
            });
        }
    }
    Ok(domains)
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
) -> Result<Vec<Option<String>>, Diagnostic> {
    let obligation = &session.problem.connections[connection];
    if let Some(&block) = session.blocks.get(endpoint) {
        let projected = session.projected_ports(block)?;
        let mut available = Vec::new();
        available
            .try_reserve_exact(projected.len())
            .map_err(|_| learning_size_error())?;
        for port in projected {
            if port.direction != direction
                || port.transport != obligation.transport
                || session.bodies.contains(&port.connection)
                || !session.shared_cell_available(port.connection, connection)
            {
                continue;
            }
            match session.selected_port(
                block,
                &port.id,
                direction,
                obligation.transport,
                &obligation.commodity,
                obligation.rate,
                &obligation.id,
            ) {
                Ok(_) => available.push(Some(port.id)),
                Err(diagnostic)
                    if diagnostic.code.starts_with("constructive.route-request-")
                        || diagnostic.code == "constructive.port-resource" =>
                {
                    return Err(diagnostic);
                }
                Err(_) => {}
            }
        }
        return Ok(available);
    }
    let terminal = session.terminals[endpoint];
    Ok(session.terminal_poses[terminal]
        .as_ref()
        .filter(|pose| session.shared_cell_available(pose.position, connection))
        .map(|_| vec![None])
        .unwrap_or_default())
}

fn route_domains(session: &Session) -> Result<Vec<RouteDomain>, Diagnostic> {
    if session.active.is_some() {
        return Ok(vec![]);
    }
    let mut domains = Vec::new();
    domains
        .try_reserve_exact(session.problem.connections.len())
        .map_err(|_| learning_size_error())?;
    for connection in 0..session.problem.connections.len() {
        if let Some(domain) = route_domain_for(session, connection)? {
            domains.push(domain);
        }
    }
    Ok(domains)
}

fn route_domain_for(
    session: &Session,
    connection: usize,
) -> Result<Option<RouteDomain>, Diagnostic> {
    if session.routes[connection].is_some() {
        return Ok(None);
    }
    let obligation = &session.problem.connections[connection];
    let placed = |endpoint: &str| {
        if let Some(&block) = session.blocks.get(endpoint) {
            session.poses[block].is_some()
        } else {
            session.terminal_poses[session.terminals[endpoint]].is_some()
        }
    };
    if !placed(&obligation.source) || !placed(&obligation.target) {
        return Ok(None);
    }
    let targets = endpoint_ports(
        session,
        &obligation.target,
        FacilityPortDirection::Input,
        connection,
    )?;
    let sources = endpoint_ports(
        session,
        &obligation.source,
        FacilityPortDirection::Output,
        connection,
    )?;
    let mut ports = Vec::new();
    if !targets.is_empty() {
        ports
            .try_reserve_exact(sources.len())
            .map_err(|_| learning_size_error())?;
        for id in sources {
            let mut copied_targets = Vec::new();
            copied_targets
                .try_reserve_exact(targets.len())
                .map_err(|_| learning_size_error())?;
            for target in &targets {
                copied_targets.push(checked_optional_string(target)?);
            }
            ports.push(PortDomain {
                id,
                targets: copied_targets,
            });
        }
    }
    let id = checked_string(&obligation.id)?;
    Ok((!ports.is_empty()).then(|| RouteDomain {
        connection,
        entity: (session.problem.blocks.len() + session.problem.terminals.len() + connection)
            as i64,
        endpoints: [
            endpoint_entity(session, &obligation.source),
            endpoint_entity(session, &obligation.target),
        ],
        id,
        sources: ports,
    }))
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
    fn new_for_macro(session: &Session, config: &Config) -> Result<Self, Diagnostic> {
        let mut input = Self::new(session, config)?;
        for row in &mut input.entity_features {
            row[7] = 0.;
        }
        input.focus_features = [0.; FOCUS_FEATURES];
        input.focus_entity = -1;
        Ok(input)
    }

    fn new_for_route(
        session: &Session,
        config: &Config,
        connection: usize,
    ) -> Result<Self, Diagnostic> {
        let mut input = Self::new(session, config)?;
        for row in &mut input.entity_features {
            row[7] = 0.;
        }
        let entity = session.problem.blocks.len() + session.problem.terminals.len() + connection;
        input.entity_features[entity][7] = 1.;
        input.focus_features = [0.; FOCUS_FEATURES];
        input.focus_features[2] = 1.;
        input.focus_entity = entity as i64;
        Ok(input)
    }

    fn new_for_route_bounded(
        session: &Session,
        config: &Config,
        connection: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<(Self, usize), RequestFailure> {
        let (mut input, reserved_bytes) = Self::new_bounded(session, config, Some(allowance))?;
        for row in &mut input.entity_features {
            if let Err(stop) = allowance.charge(WorkKind::ValidationRecords, 1) {
                allowance.release_peak(reserved_bytes);
                return Err(stop.into());
            }
            row[7] = 0.;
        }
        let entity = session.problem.blocks.len() + session.problem.terminals.len() + connection;
        input.entity_features[entity][7] = 1.;
        input.focus_features = [0.; FOCUS_FEATURES];
        input.focus_features[2] = 1.;
        input.focus_entity = entity as i64;
        Ok((input, reserved_bytes))
    }

    fn new(session: &Session, config: &Config) -> Result<Self, Diagnostic> {
        match Self::new_bounded(session, config, None) {
            Ok((input, _)) => Ok(input),
            Err(RequestFailure::Invalid(diagnostic)) => Err(diagnostic),
            Err(_) => unreachable!("unbounded graph construction has no allowance stop"),
        }
    }

    fn new_bounded(
        session: &Session,
        config: &Config,
        mut allowance: Option<&mut RequestAllowance>,
    ) -> Result<(Self, usize), RequestFailure> {
        let scale = config
            .placement
            .width()
            .max(config.placement.height())
            .max(1) as f32;
        let blocks = session.problem.blocks.len();
        let terminals = session.problem.terminals.len();
        let connections = session.problem.connections.len();
        let size_error = || {
            RequestFailure::Invalid(error(
                "learning-resource",
                "learning-input",
                "graph input size is not representable",
            ))
        };
        let connection_offset = blocks.checked_add(terminals).ok_or_else(size_error)?;
        let entities = connection_offset
            .checked_add(connections)
            .ok_or_else(size_error)?;
        let relations = connections.checked_mul(2).ok_or_else(size_error)?;
        let mut spatial_count = 0usize;
        for (block, pose) in session.poses.iter().enumerate() {
            if let Some(pose) = pose {
                let definition = session
                    .catalogs
                    .facilities
                    .facility(&session.problem.blocks[block].facility)
                    .expect("constructive problem admitted facilities");
                let (width, height, _) = rotate(definition, pose.rotation, pose.position);
                let width = usize::try_from(width).map_err(|_| size_error())?;
                let height = usize::try_from(height).map_err(|_| size_error())?;
                spatial_count = spatial_count
                    .checked_add(width.checked_mul(height).ok_or_else(size_error)?)
                    .ok_or_else(size_error)?;
            }
        }
        for route in session.routes.iter().flatten() {
            spatial_count = spatial_count
                .checked_add(route.cells.len())
                .ok_or_else(size_error)?;
        }
        for bridge in session.bridges.values() {
            for channel in &bridge.channels {
                spatial_count = spatial_count
                    .checked_add(channel.connections.len())
                    .ok_or_else(size_error)?;
            }
        }
        spatial_count = spatial_count
            .checked_add(session.terminal_poses.iter().flatten().count())
            .and_then(|count| {
                session.active.as_ref().map_or(Some(count), |active| {
                    count.checked_add(active.cells.len())?.checked_add(2)
                })
            })
            .ok_or_else(size_error)?;
        let counted_bytes =
            |count: usize, element: usize| count.checked_mul(element).ok_or_else(size_error);
        let occupied_sites = session
            .occupied
            .iter()
            .try_fold(session.bodies.len(), |count, layer| {
                count.checked_add(layer.len()).ok_or_else(size_error)
            })?
            .checked_add(terminals)
            .ok_or_else(size_error)?;
        let scratch_bytes = counted_bytes(occupied_sites, std::mem::size_of::<Point>())?;
        let output_bytes = [
            std::mem::size_of::<Input>(),
            counted_bytes(entities, std::mem::size_of::<[f32; ENTITY_FEATURES]>())?,
            counted_bytes(relations, std::mem::size_of::<[i64; 2]>())?,
            counted_bytes(relations, std::mem::size_of::<[f32; RELATION_FEATURES]>())?,
            counted_bytes(
                spatial_count,
                std::mem::size_of::<[f32; SPATIAL_FEATURES]>(),
            )?,
            counted_bytes(spatial_count, std::mem::size_of::<i64>())?,
            session.bridge_copy_bytes(),
        ]
        .into_iter()
        .try_fold(0usize, |sum, bytes| {
            sum.checked_add(bytes).ok_or_else(size_error)
        })?;
        let reserved_bytes = output_bytes
            .checked_add(scratch_bytes)
            .ok_or_else(size_error)?;
        if let Some(allowance) = allowance.as_deref_mut() {
            allowance.reserve_peak(reserved_bytes)?;
        }
        let result = (|| -> Result<Self, RequestFailure> {
            let mut entity_features = Vec::new();
            reserve_graph(&mut entity_features, entities, allowance.is_some())?;
            for (index, block) in session.problem.blocks.iter().enumerate() {
                charge_graph(&mut allowance, WorkKind::GraphRows, 1)?;
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
                if matches!(session.focus, Some(super::Focus::Block { block, .. }) if block == index)
                {
                    row[7] = 1.;
                }
                entity_features.push(row);
            }
            for (index, terminal) in session.problem.terminals.iter().enumerate() {
                charge_graph(&mut allowance, WorkKind::GraphRows, 1)?;
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
            let mut relation_index = Vec::new();
            reserve_graph(&mut relation_index, relations, allowance.is_some())?;
            let mut relation_features = Vec::new();
            reserve_graph(&mut relation_features, relations, allowance.is_some())?;
            for (index, connection) in session.problem.connections.iter().enumerate() {
                charge_graph(&mut allowance, WorkKind::GraphRows, 3)?;
                let entity = connection_offset + index;
                let mut row = [0.; ENTITY_FEATURES];
                row[1] = 1.;
                row[3] = session.routes[index].is_some() as u8 as f32;
                row[4] = session
                    .active
                    .as_ref()
                    .is_some_and(|route| route.connection == index) as u8
                    as f32;
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
            reserve_graph(&mut spatial_features, spatial_count, allowance.is_some())?;
            let mut spatial_owners = Vec::new();
            reserve_graph(&mut spatial_owners, spatial_count, allowance.is_some())?;
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
                        charge_graph(&mut allowance, WorkKind::GraphRows, 1)?;
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
                    charge_graph(&mut allowance, WorkKind::GraphRows, 1)?;
                    charge_graph(&mut allowance, WorkKind::RouteCells, 1)?;
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
            let mut bridge_resources = Vec::new();
            reserve_graph(
                &mut bridge_resources,
                session.bridges.len(),
                allowance.is_some(),
            )?;
            for bridge in session.bridges.values() {
                bridge_resources.push(try_clone_bridge(bridge).map_err(|diagnostic| {
                    if allowance.is_some() {
                        host_allocation_stop(super::routing_candidates::bridge_copy_bytes(bridge))
                    } else {
                        RequestFailure::Invalid(diagnostic)
                    }
                })?);
            }
            bridge_resources.sort_by(|left, right| left.id.cmp(&right.id));
            for bridge in &bridge_resources {
                charge_graph(
                    &mut allowance,
                    WorkKind::CatalogVisits,
                    session.catalogs.components.catalog().components.len(),
                )?;
                charge_graph(
                    &mut allowance,
                    WorkKind::ValidationRecords,
                    bridge
                        .channels
                        .iter()
                        .map(|channel| channel.connections.len())
                        .sum(),
                )?;
                let (component_code, loads) = session.bridge_component_features(bridge);
                for (axis, channel) in bridge.channels.iter().enumerate() {
                    let kind = match channel.axis {
                        BridgeAxis::Horizontal => 9,
                        BridgeAxis::Vertical => 10,
                    };
                    for connection in &channel.connections {
                        charge_graph(&mut allowance, WorkKind::GraphRows, 1)?;
                        let owner = session.connections[connection] + connection_offset;
                        push_spatial(
                            &mut spatial_features,
                            &mut spatial_owners,
                            bridge.position,
                            scale,
                            kind,
                            owner as i64,
                        );
                        let row = spatial_features.last_mut().expect("bridge row was pushed");
                        row[11] = component_code;
                        row[12] = loads[axis];
                        row[13] = bridge.rotation as f32 / 270.0;
                    }
                }
            }
            for (terminal, pose) in session.terminal_poses.iter().enumerate() {
                let Some(pose) = pose else { continue };
                charge_graph(&mut allowance, WorkKind::GraphRows, 1)?;
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
                    charge_graph(&mut allowance, WorkKind::GraphRows, 1)?;
                    charge_graph(&mut allowance, WorkKind::RouteCells, 1)?;
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
                charge_graph(&mut allowance, WorkKind::GraphRows, 2)?;
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
            let occupied = blocked_cells_bounded(session, allowance.is_some())?;
            for _ in &session.bodies {
                charge_graph(&mut allowance, WorkKind::ValidationRecords, 1)?;
            }
            for layer in &session.occupied {
                for _ in layer.keys() {
                    charge_graph(&mut allowance, WorkKind::ValidationRecords, 1)?;
                }
            }
            for _ in session.terminal_poses.iter().flatten() {
                charge_graph(&mut allowance, WorkKind::ValidationRecords, 1)?;
            }
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
                occupied.len() as f32
                    / (config.placement.width() * config.placement.height()) as f32,
                config.placement.width() as f32 / scale,
                config.placement.height() as f32 / scale,
            ];
            Ok(Self {
                schema: SCHEMA,
                entity_features,
                relation_index,
                relation_features,
                spatial_features,
                spatial_owners,
                global_features,
                focus_features,
                focus_entity,
                bridge_resources,
            })
        })();
        if result.is_err() {
            if let Some(allowance) = allowance.as_deref_mut() {
                allowance.release_peak(reserved_bytes);
            }
        } else if let Some(allowance) = allowance.as_deref_mut() {
            allowance.release_peak(scratch_bytes);
        }
        result.map(|input| (input, if allowance.is_some() { output_bytes } else { 0 }))
    }
}

fn reserve_graph<T>(
    values: &mut Vec<T>,
    count: usize,
    bounded: bool,
) -> Result<(), RequestFailure> {
    values.try_reserve_exact(count).map_err(|_| {
        if bounded {
            host_allocation_stop(count.saturating_mul(std::mem::size_of::<T>()))
        } else {
            RequestFailure::Invalid(error(
                "learning-resource",
                "learning-input",
                "graph input host allocation failed",
            ))
        }
    })
}

fn host_allocation_stop(bytes: usize) -> RequestFailure {
    RequestFailure::Stopped(AllowanceStop {
        reason: "host-allocation",
        category: None,
        limit: 0,
        used: 0,
        deadline_overshoot_ms: 0,
        attempted_bytes: Some(bytes),
        aggregate_peak_bytes: None,
    })
}

fn try_clone_bridge(bridge: &Bridge) -> Result<Bridge, Diagnostic> {
    fn copy_channel(channel: &BridgeChannel) -> Result<BridgeChannel, Diagnostic> {
        let mut connections = Vec::new();
        connections
            .try_reserve_exact(channel.connections.len())
            .map_err(|_| learning_size_error())?;
        for connection in &channel.connections {
            connections.push(checked_string(connection)?);
        }
        Ok(BridgeChannel {
            axis: channel.axis,
            commodity: checked_string(&channel.commodity)?,
            connections,
        })
    }
    Ok(Bridge {
        id: checked_string(&bridge.id)?,
        component: checked_string(&bridge.component)?,
        position: bridge.position,
        rotation: bridge.rotation,
        channels: [
            copy_channel(&bridge.channels[0])?,
            copy_channel(&bridge.channels[1])?,
        ],
    })
}

fn charge_graph(
    allowance: &mut Option<&mut RequestAllowance>,
    kind: WorkKind,
    count: usize,
) -> Result<(), AllowanceStop> {
    if let Some(allowance) = allowance.as_deref_mut() {
        allowance.charge(kind, count)?;
    }
    Ok(())
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
