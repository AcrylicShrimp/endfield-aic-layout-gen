//! Retained construction of one route-guidance input for a stable native state.

mod raster;

use raster::{RasterCursor, RasterKeys};

use aic_layout::{geometry::Bounds, physical::Bridge};
use std::sync::Arc;

use super::{
    Config, EndpointPair, EndpointPairCursor, GUIDANCE_GRID_CHANNELS, GUIDANCE_SCHEMA,
    GuidanceInput, Request, RequestAllowance, RequestFailure, Session, WorkKind, error,
};
use crate::constructive::learning::{Config as LearningConfig, Input, RouteGraphCursor};

/// The selected guidance input stays owned across temporary first-page stops.
pub struct RouteGuidancePreparation {
    request: Request,
    bounds: Bounds,
    connection: usize,
    source_address: usize,
    source_instance_id: u64,
    source_turns: u64,
    candidate: Config,
    learning: LearningConfig,
    pending: Vec<usize>,
    scan_index: usize,
    selected_done: bool,
    active_connection: Option<usize>,
    pair_cursor: Option<EndpointPairCursor>,
    selected_pairs: Option<Vec<EndpointPair>>,
    remaining_pairs: Vec<Vec<EndpointPair>>,
    raster: Option<RasterCursor>,
    raster_keys: Option<RasterKeys>,
    grid: Option<Vec<f32>>,
    bridge_resources: Option<Vec<Bridge>>,
    graph_cursor: Option<RouteGraphCursor>,
    graph: Option<Arc<Input>>,
    graph_bytes: usize,
    output_bytes: usize,
    pending_connections: Option<Vec<String>>,
    pending_index: usize,
    catalog_identity: Option<String>,
    final_bytes: usize,
    pair_bytes: usize,
    owner_bytes: usize,
}

impl RouteGuidancePreparation {
    pub fn new(
        source: &Session,
        request: &Request,
        candidate: &Config,
        learning: &LearningConfig,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        candidate.validate()?;
        allowance.check_deadline()?;
        if learning.placement != candidate.bounds {
            return Err(error(
                "route-guidance-bounds",
                &request.connection,
                "candidate and learning bounds must match",
            )
            .into());
        }
        if request.state_revision != source.revision() || source.active.is_some() {
            return Err(error(
                "route-guidance-state",
                &request.connection,
                "route guidance input requires the exact stable state revision",
            )
            .into());
        }
        let Some(&connection) = source.connections.get(&request.connection) else {
            return Err(error(
                "unknown-connection",
                &request.connection,
                "route guidance references an unknown logical connection",
            )
            .into());
        };
        if source.routes[connection].is_some() {
            return Err(error(
                "completed-connection",
                &request.connection,
                "route guidance references a completed logical connection",
            )
            .into());
        }
        let selected = &source.problem.connections[connection];
        let pending_endpoint = |endpoint: &str| {
            if let Some(&block) = source.blocks.get(endpoint) {
                source.poses[block].is_none()
            } else {
                source.terminal_poses[source.terminals[endpoint]].is_none()
            }
        };
        let source_pending = pending_endpoint(&selected.source);
        let target_pending = pending_endpoint(&selected.target);
        if source_pending || target_pending {
            // Rejected-input diagnostics leave this preparation boundary and
            // are not retained search state in the request memory ledger.
            let mut selected_pending = Vec::with_capacity(2);
            if source_pending {
                selected_pending.push(selected.source.clone());
            }
            if target_pending && selected.target != selected.source {
                selected_pending.push(selected.target.clone());
            }
            return Err(RequestFailure::PendingPlacement {
                endpoints: selected_pending,
            });
        }

        let obligations = source.problem.connections.len();
        let owner_bytes = std::mem::size_of::<Self>()
            .saturating_add(request.connection.len())
            .saturating_add(
                learning
                    .extension_lengths
                    .len()
                    .saturating_mul(std::mem::size_of::<i64>()),
            )
            .saturating_add(obligations.saturating_mul(
                std::mem::size_of::<usize>() + std::mem::size_of::<Vec<EndpointPair>>(),
            ));
        allowance.reserve_peak(owner_bytes)?;
        Ok(Self {
            request: request.clone(),
            bounds: candidate.bounds,
            connection,
            source_address: source as *const Session as usize,
            source_instance_id: source.instance_id.0,
            source_turns: source.turns,
            candidate: candidate.clone(),
            learning: learning.clone(),
            pending: Vec::with_capacity(obligations),
            scan_index: 0,
            selected_done: false,
            active_connection: None,
            pair_cursor: None,
            selected_pairs: None,
            remaining_pairs: Vec::with_capacity(obligations),
            raster: None,
            raster_keys: None,
            grid: None,
            bridge_resources: None,
            graph_cursor: None,
            graph: None,
            graph_bytes: 0,
            output_bytes: 0,
            pending_connections: None,
            pending_index: 0,
            catalog_identity: None,
            final_bytes: 0,
            pair_bytes: 0,
            owner_bytes,
        })
    }

    pub fn advance_pairs(
        &mut self,
        source: &Session,
        candidate: &Config,
        learning: &LearningConfig,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        if source as *const Session as usize != self.source_address
            || source.instance_id.0 != self.source_instance_id
            || source.revision() != self.request.state_revision
            || source.turns != self.source_turns
            || candidate != &self.candidate
            || learning != &self.learning
        {
            return Err(error(
                "route-guidance-state",
                &self.request.connection,
                "route guidance preparation source changed",
            )
            .into());
        }
        while !self.selected_done
            || self.active_connection.is_some()
            || self.scan_index < source.problem.connections.len()
        {
            if self.active_connection.is_none() {
                if !self.selected_done {
                    self.active_connection = Some(self.connection);
                } else {
                    while self.scan_index < source.problem.connections.len() {
                        if self.scan_index % 64 == 0 {
                            allowance.check_deadline()?;
                        }
                        let other = self.scan_index;
                        self.scan_index += 1;
                        if other == self.connection || source.routes[other].is_some() {
                            continue;
                        }
                        let obligation = &source.problem.connections[other];
                        let pending_endpoint = |endpoint: &str| {
                            if let Some(&block) = source.blocks.get(endpoint) {
                                source.poses[block].is_none()
                            } else {
                                source.terminal_poses[source.terminals[endpoint]].is_none()
                            }
                        };
                        if pending_endpoint(&obligation.source)
                            || pending_endpoint(&obligation.target)
                        {
                            self.pending.push(other);
                        } else {
                            self.active_connection = Some(other);
                            break;
                        }
                    }
                    if self.active_connection.is_none() {
                        break;
                    }
                }
            }
            let connection = self
                .active_connection
                .expect("selected or scanned ready route");
            if self.pair_cursor.is_none() {
                let pair_bytes = source.endpoint_pair_reservation_bytes(connection);
                allowance.reserve_peak(pair_bytes)?;
                self.pair_bytes = self.pair_bytes.saturating_add(pair_bytes);
                self.pair_cursor = Some(EndpointPairCursor::new(source, connection));
            }
            let cursor = self
                .pair_cursor
                .as_mut()
                .expect("just prepared pair cursor");
            let pairs = cursor.advance(source, connection, self.bounds, allowance)?;
            if pairs.is_empty() {
                // This diagnostic also leaves the retained preparation owner.
                return Err(RequestFailure::NoCompatibleEndpoints {
                    connection: source.problem.connections[connection].id.clone(),
                });
            }
            if connection == self.connection {
                self.selected_pairs = Some(pairs);
                self.selected_done = true;
            } else {
                self.remaining_pairs.push(pairs);
            }
            self.pair_cursor = None;
            self.active_connection = None;
        }
        Ok(())
    }

    pub fn advance_raster(
        &mut self,
        source: &Session,
        candidate: &Config,
        learning: &LearningConfig,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        self.advance_pairs(source, candidate, learning, allowance)?;
        if self.grid.is_some() {
            return Ok(());
        }
        if self.raster.is_none() {
            self.raster = Some(RasterCursor::new(
                source,
                self.connection,
                self.bounds,
                self.pending
                    .len()
                    .saturating_add(self.remaining_pairs.len()),
                allowance,
            )?);
        }
        self.raster.as_mut().expect("raster was prepared").advance(
            source,
            self.connection,
            self.selected_pairs.as_ref().expect("pairs were prepared"),
            &self.remaining_pairs,
            allowance,
        )?;
        let (grid, bridge_resources, raster_keys, output_bytes) = self
            .raster
            .take()
            .expect("raster completed")
            .finish(allowance);
        self.grid = Some(grid);
        self.bridge_resources = Some(bridge_resources);
        self.raster_keys = Some(raster_keys);
        self.output_bytes = output_bytes;
        self.selected_pairs = None;
        self.remaining_pairs.clear();
        allowance.release_peak(self.pair_bytes);
        self.pair_bytes = 0;
        Ok(())
    }

    pub fn advance_graph(
        &mut self,
        source: &Session,
        candidate: &Config,
        learning: &LearningConfig,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        self.advance_raster(source, candidate, learning, allowance)?;
        if self.graph.is_some() {
            return Ok(());
        }
        let keys = self.raster_keys.as_ref().expect("raster keys are retained");
        if self.graph_cursor.is_none() {
            let occupied_sites = keys
                .bodies
                .len()
                .saturating_add(keys.occupied_same.len())
                .saturating_add(keys.occupied_other.len())
                .saturating_add(source.terminal_poses.iter().flatten().count());
            self.graph_cursor = Some(RouteGraphCursor::new(
                source,
                learning,
                self.connection,
                occupied_sites,
                allowance,
            )?);
        }
        let occupied_layers: [&[_]; 2] = if keys.transport_layer == 0 {
            [&keys.occupied_same, &keys.occupied_other]
        } else {
            [&keys.occupied_other, &keys.occupied_same]
        };
        self.graph_cursor
            .as_mut()
            .expect("graph was prepared")
            .advance(
                source,
                learning,
                self.connection,
                self.bridge_resources
                    .as_ref()
                    .expect("raster bridges are retained"),
                &keys.bodies,
                occupied_layers,
                allowance,
            )?;
        let (graph, graph_bytes) = self
            .graph_cursor
            .take()
            .expect("graph completed")
            .finish(allowance);
        self.graph = Some(graph);
        self.graph_bytes = graph_bytes;
        let keys = self.raster_keys.take().expect("graph consumed raster keys");
        let key_bytes = keys.reserved_bytes();
        drop(keys);
        allowance.release_peak(key_bytes);
        Ok(())
    }

    /// A temporary stop returns the same owner, including any final string
    /// copies already completed. The caller retains it for the next page.
    pub fn into_input(
        mut self,
        source: &Session,
        candidate: &Config,
        learning: &LearningConfig,
        allowance: &mut RequestAllowance,
    ) -> Result<GuidanceInput, (Self, RequestFailure)> {
        if let Err(failure) = self.advance_graph(source, candidate, learning, allowance) {
            return Err((self, failure));
        }
        if self.pending_connections.is_none() {
            let final_struct_bytes = std::mem::size_of::<GuidanceInput>()
                .saturating_add(
                    self.pending
                        .len()
                        .saturating_mul(std::mem::size_of::<String>()),
                )
                .saturating_add(source.catalogs.identity.len());
            if let Err(stop) = allowance.reserve_peak(final_struct_bytes) {
                return Err((self, RequestFailure::Stopped(stop)));
            }
            self.final_bytes = final_struct_bytes;
            self.pending_connections = Some(Vec::with_capacity(self.pending.len()));
        }
        while self.pending_index < self.pending.len() {
            let id = &source.problem.connections[self.pending[self.pending_index]].id;
            if let Err(stop) = allowance.reserve_peak(id.len()) {
                return Err((self, RequestFailure::Stopped(stop)));
            }
            if let Err(stop) = allowance.charge(WorkKind::CopiedBytes, id.len()) {
                allowance.release_peak(id.len());
                return Err((self, RequestFailure::Stopped(stop)));
            }
            self.pending_connections
                .as_mut()
                .expect("pending output was reserved")
                .push(id.clone());
            self.final_bytes = self.final_bytes.saturating_add(id.len());
            self.pending_index += 1;
        }
        if self.catalog_identity.is_none() {
            if let Err(stop) =
                allowance.charge(WorkKind::CopiedBytes, source.catalogs.identity.len())
            {
                return Err((self, RequestFailure::Stopped(stop)));
            }
            self.catalog_identity = Some(source.catalogs.identity.clone());
        }
        let connection_bytes = self.request.connection.len();
        let reserved_bytes = self
            .output_bytes
            .saturating_add(self.graph_bytes)
            .saturating_add(self.final_bytes)
            .saturating_add(connection_bytes);
        let input = GuidanceInput {
            schema: GUIDANCE_SCHEMA,
            state_revision: self.request.state_revision,
            connection: std::mem::take(&mut self.request.connection),
            catalog_identity: self.catalog_identity.take().expect("catalog was copied"),
            bounds: self.bounds,
            channels: GUIDANCE_GRID_CHANNELS,
            bridge_resources: self.bridge_resources.take().expect("raster was completed"),
            grid: self.grid.take().expect("raster was completed"),
            graph: self.graph.take().expect("graph was completed"),
            pending_connections: self
                .pending_connections
                .take()
                .expect("pending output was copied"),
            reserved_bytes,
        };
        let owner_release = self.owner_bytes - connection_bytes;
        drop(self);
        allowance.release_peak(owner_release);
        Ok(input)
    }

    pub fn release(self, allowance: &mut RequestAllowance) {
        let reserved = self
            .pair_bytes
            .saturating_add(self.owner_bytes)
            .saturating_add(self.output_bytes)
            .saturating_add(self.graph_bytes)
            .saturating_add(self.final_bytes)
            .saturating_add(
                self.raster_keys
                    .as_ref()
                    .map_or(0, RasterKeys::reserved_bytes),
            )
            .saturating_add(self.raster.as_ref().map_or(0, RasterCursor::reserved_bytes))
            .saturating_add(
                self.graph_cursor
                    .as_ref()
                    .map_or(0, RouteGraphCursor::reserved_bytes),
            );
        drop(self);
        allowance.release_peak(reserved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructive::routing_candidates::{
        CUMULATIVE_WORK_KINDS, GUIDANCE_GRID_CHANNELS, WorkKind,
    };
    use crate::constructive::{Command, Direction, Limits};
    use aic_layout::{
        geometry::Point,
        logistics::TransportKind,
        physical::{Block, BridgeAxis, Connection, Problem},
        rate::Rate,
    };
    use aic_routing::{Carrier, RouteJob};
    use indexmap::IndexSet;
    use std::time::Duration;

    fn bridge_and_open_route() -> Session {
        let blocks = ["west", "east", "south", "north", "extra-west", "extra-east"]
            .into_iter()
            .map(|id| Block {
                id: id.into(),
                facility: "grinder-1-mode-normal".into(),
            })
            .collect();
        let connection = |id: &str, source: &str, target: &str, rate: Rate| Connection {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            commodity: format!("{id}-item"),
            transport: TransportKind::Belt,
            rate,
        };
        let problem = Problem {
            blocks,
            terminals: vec![],
            connections: vec![
                connection(
                    "horizontal",
                    "west",
                    "east",
                    Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                ),
                connection(
                    "vertical",
                    "south",
                    "north",
                    Rate {
                        numerator: 1,
                        denominator: 4,
                    },
                ),
                connection(
                    "selected",
                    "extra-west",
                    "extra-east",
                    Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                ),
            ],
        };
        let mut source = Session::new(
            problem,
            crate::constructive::tests::catalogs(),
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
            ("extra-west", Point { x: 2, y: 12 }, 90),
            ("extra-east", Point { x: 8, y: 12 }, 90),
        ] {
            assert!(
                source
                    .step(Command::Place {
                        block: block.into(),
                        position,
                        rotation
                    })
                    .accepted
            );
        }
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "horizontal".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::East,
                    length: 2
                })
                .accepted
        );
        assert!(source.step(Command::Connect).accepted);
        let horizontal = source.routes[0].as_ref().unwrap();
        let crossing = horizontal.cells[1];
        let option = aic_routing::crossing_options(
            &[Carrier {
                id: 0,
                network: 0,
                transport: TransportKind::Belt,
                rate: source.problem.connections[0].rate,
                cells: &horizontal.cells,
            }],
            &RouteJob {
                id: 1,
                transport: TransportKind::Belt,
                rate: source.problem.connections[1].rate,
            },
            &source.catalogs.components,
            &IndexSet::new(),
        )
        .get(&(crossing, 3))
        .cloned()
        .unwrap();
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "vertical".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let start = source.active.as_ref().unwrap().cells[0];
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length: start.y - crossing.y - 1,
                })
                .accepted
        );
        assert!(
            source
                .step(Command::ReserveBridge {
                    position: crossing,
                    component: option.component,
                    rotation: option.rotation,
                    crossed_axis: BridgeAxis::Horizontal,
                })
                .accepted
        );
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 1
                })
                .accepted
        );
        let target = source.active.as_ref().unwrap().target;
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length: crossing.y - target.y,
                })
                .accepted
        );
        assert!(source.step(Command::Connect).accepted);
        assert_eq!(source.bridges.len(), 1);
        source
    }

    #[test]
    fn endpoint_preparation_keeps_pairs_and_work_through_a_page_yield() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        let candidate = crate::constructive::tests::route_candidate_config();
        let learning = crate::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut baseline_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let expected = source
            .endpoint_pairs(0, candidate.bounds, &mut baseline_allowance)
            .unwrap();
        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut preparation =
            RouteGuidancePreparation::new(&source, &request, &candidate, &learning, &mut allowance)
                .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let endpoint_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::EndpointVisits)
            .unwrap();
        quotas[endpoint_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            preparation.advance_pairs(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        allowance.end_page_window();
        preparation
            .advance_pairs(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        let signature = |pairs: &[EndpointPair]| {
            pairs
                .iter()
                .map(|pair| {
                    (
                        pair.source.port.clone(),
                        pair.source.point,
                        pair.target.port.clone(),
                        pair.target.point,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            signature(preparation.selected_pairs.as_ref().unwrap()),
            signature(&expected)
        );
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
        preparation.release(&mut allowance);
        assert_eq!(allowance.reserved_bytes, 0);
    }

    #[test]
    fn other_ready_and_pending_connections_survive_a_pair_page_yield() {
        let original = crate::constructive::tests::session();
        let mut problem = original.problem.as_ref().clone();
        let mut unplaced = problem.blocks[0].clone();
        unplaced.id = "unplaced".into();
        problem.blocks.push(unplaced);
        let mut ready = problem.connections[0].clone();
        ready.id = "connection-2".into();
        let mut pending = problem.connections[0].clone();
        pending.id = "connection-1".into();
        pending.source = "unplaced".into();
        problem.connections.push(pending);
        problem.connections.push(ready);
        let mut source = Session::new(problem, original.catalogs.clone(), original.limits).unwrap();
        crate::constructive::tests::place_pair(&mut source);
        let candidate = crate::constructive::tests::route_candidate_config();
        let learning = crate::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut baseline = RequestAllowance::new(candidate.request_allowance.clone());
        let selected = source
            .endpoint_pairs(0, candidate.bounds, &mut baseline)
            .unwrap();
        let selected_visits = baseline.usage().categories[&WorkKind::EndpointVisits];
        let other = source
            .endpoint_pairs(2, candidate.bounds, &mut baseline)
            .unwrap();
        let signature = |pairs: &[EndpointPair]| {
            pairs
                .iter()
                .map(|pair| {
                    (
                        pair.source.port.clone(),
                        pair.source.point,
                        pair.source.direction,
                        pair.target.port.clone(),
                        pair.target.point,
                        pair.target.direction,
                    )
                })
                .collect::<Vec<_>>()
        };

        let mut uninterrupted_allowance =
            RequestAllowance::new(candidate.request_allowance.clone());
        let mut uninterrupted = RouteGuidancePreparation::new(
            &source,
            &request,
            &candidate,
            &learning,
            &mut uninterrupted_allowance,
        )
        .unwrap();
        uninterrupted
            .advance_pairs(&source, &candidate, &learning, &mut uninterrupted_allowance)
            .unwrap();

        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut preparation =
            RouteGuidancePreparation::new(&source, &request, &candidate, &learning, &mut allowance)
                .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let endpoint_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::EndpointVisits)
            .unwrap();
        quotas[endpoint_slot] = selected_visits;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            preparation.advance_pairs(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        assert!(preparation.selected_done);
        assert_eq!(preparation.pending, vec![1]);
        allowance.end_page_window();
        preparation
            .advance_pairs(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        assert_eq!(
            signature(preparation.selected_pairs.as_ref().unwrap()),
            signature(&selected)
        );
        assert_eq!(preparation.remaining_pairs.len(), 1);
        assert_eq!(
            signature(&preparation.remaining_pairs[0]),
            signature(&other)
        );
        assert_eq!(preparation.pending, vec![1]);
        assert_eq!(allowance.usage().categories, baseline.usage().categories);
        assert_eq!(
            signature(preparation.selected_pairs.as_ref().unwrap()),
            signature(uninterrupted.selected_pairs.as_ref().unwrap())
        );
        assert_eq!(
            signature(&preparation.remaining_pairs[0]),
            signature(&uninterrupted.remaining_pairs[0])
        );
        assert_eq!(
            allowance.usage().categories,
            uninterrupted_allowance.usage().categories
        );
        preparation.release(&mut allowance);
        uninterrupted.release(&mut uninterrupted_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(uninterrupted_allowance.reserved_bytes, 0);
    }

    #[test]
    fn same_slot_different_session_or_changed_config_is_rejected() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        let mut candidate = crate::constructive::tests::route_candidate_config();
        let learning = crate::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut preparation =
            RouteGuidancePreparation::new(&source, &request, &candidate, &learning, &mut allowance)
                .unwrap();
        let original_bounds = candidate.bounds;
        candidate.bounds.max_x -= 1;
        assert!(matches!(
            preparation.advance_pairs(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Invalid(_))
        ));
        candidate.bounds = original_bounds;
        let mut fork = source.clone();
        std::mem::swap(&mut source, &mut fork);
        assert_eq!(source.revision(), request.state_revision);
        assert!(matches!(
            preparation.advance_pairs(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Invalid(_))
        ));
        preparation.release(&mut allowance);
        assert_eq!(allowance.reserved_bytes, 0);
    }

    #[test]
    fn raster_page_yield_preserves_exact_grid_and_charges() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        let candidate = crate::constructive::tests::route_candidate_config();
        let learning = crate::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let legacy = source
            .route_guidance_input(&request, &candidate, &learning)
            .unwrap();

        let mut uninterrupted_allowance =
            RequestAllowance::new(candidate.request_allowance.clone());
        let mut uninterrupted = RouteGuidancePreparation::new(
            &source,
            &request,
            &candidate,
            &learning,
            &mut uninterrupted_allowance,
        )
        .unwrap();
        uninterrupted
            .advance_raster(&source, &candidate, &learning, &mut uninterrupted_allowance)
            .unwrap();

        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut preparation =
            RouteGuidancePreparation::new(&source, &request, &candidate, &learning, &mut allowance)
                .unwrap();
        preparation
            .advance_pairs(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let raster_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::RasterCells)
            .unwrap();
        quotas[raster_slot] = GUIDANCE_GRID_CHANNELS * 5;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            preparation.advance_raster(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        assert!(preparation.raster.is_some());
        allowance.end_page_window();
        preparation
            .advance_raster(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        assert_eq!(preparation.grid.as_ref().unwrap(), &legacy.grid);
        assert_eq!(preparation.grid, uninterrupted.grid);
        assert_eq!(preparation.bridge_resources, uninterrupted.bridge_resources);
        assert_eq!(
            allowance.usage().categories,
            uninterrupted_allowance.usage().categories
        );
        assert_eq!(
            allowance.reserved_bytes,
            uninterrupted_allowance.reserved_bytes
        );
        preparation.release(&mut allowance);
        uninterrupted.release(&mut uninterrupted_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(uninterrupted_allowance.reserved_bytes, 0);
    }

    #[test]
    fn graph_focus_page_yield_matches_legacy_input_and_charges() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        let candidate = crate::constructive::tests::route_candidate_config();
        let learning = crate::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let legacy = source
            .route_guidance_input(&request, &candidate, &learning)
            .unwrap();
        let mut uninterrupted_allowance =
            RequestAllowance::new(candidate.request_allowance.clone());
        let mut uninterrupted = RouteGuidancePreparation::new(
            &source,
            &request,
            &candidate,
            &learning,
            &mut uninterrupted_allowance,
        )
        .unwrap();
        uninterrupted
            .advance_graph(&source, &candidate, &learning, &mut uninterrupted_allowance)
            .unwrap();

        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut preparation =
            RouteGuidancePreparation::new(&source, &request, &candidate, &learning, &mut allowance)
                .unwrap();
        preparation
            .advance_raster(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let validation_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::ValidationRecords)
            .unwrap();
        quotas[validation_slot] = source.bodies.len() + 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            preparation.advance_graph(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        assert!(preparation.graph_cursor.is_some());
        allowance.end_page_window();
        preparation
            .advance_graph(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        assert_eq!(preparation.graph.as_deref(), Some(legacy.graph.as_ref()));
        assert_eq!(preparation.graph, uninterrupted.graph);
        assert_eq!(
            allowance.usage().categories,
            uninterrupted_allowance.usage().categories
        );
        assert_eq!(
            allowance.reserved_bytes,
            uninterrupted_allowance.reserved_bytes
        );
        preparation.release(&mut allowance);
        uninterrupted.release(&mut uninterrupted_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(uninterrupted_allowance.reserved_bytes, 0);
    }

    #[test]
    fn final_copy_page_yield_preserves_input_and_memory_ownership() {
        let original = crate::constructive::tests::session();
        let mut problem = original.problem.as_ref().clone();
        let mut unplaced = problem.blocks[0].clone();
        unplaced.id = "unplaced".into();
        problem.blocks.push(unplaced);
        let mut pending = problem.connections[0].clone();
        pending.id = "pending-connection".into();
        pending.source = "unplaced".into();
        problem.connections.push(pending);
        let mut second_pending = problem.connections[0].clone();
        second_pending.id = "second-pending-connection".into();
        second_pending.target = "unplaced".into();
        problem.connections.push(second_pending);
        let mut source = Session::new(problem, original.catalogs.clone(), original.limits).unwrap();
        crate::constructive::tests::place_pair(&mut source);
        let candidate = crate::constructive::tests::route_candidate_config();
        let learning = crate::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut direct_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let direct = RouteGuidancePreparation::new(
            &source,
            &request,
            &candidate,
            &learning,
            &mut direct_allowance,
        )
        .unwrap()
        .into_input(&source, &candidate, &learning, &mut direct_allowance)
        .unwrap_or_else(|(_, failure)| panic!("direct preparation failed: {failure:?}"));

        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut preparation =
            RouteGuidancePreparation::new(&source, &request, &candidate, &learning, &mut allowance)
                .unwrap();
        preparation
            .advance_graph(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let copied_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::CopiedBytes)
            .unwrap();
        quotas[copied_slot] = "pending-connection".len();
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let (preparation, failure) = preparation
            .into_input(&source, &candidate, &learning, &mut allowance)
            .unwrap_err();
        assert!(matches!(
            failure,
            RequestFailure::Stopped(stop) if stop.reason == "page-quantum"
        ));
        assert_eq!(preparation.pending_index, 1);
        assert_eq!(
            preparation.pending_connections.as_deref(),
            Some(["pending-connection".into()].as_slice())
        );
        allowance.end_page_window();
        let input = preparation
            .into_input(&source, &candidate, &learning, &mut allowance)
            .unwrap_or_else(|(_, failure)| panic!("resumed preparation failed: {failure:?}"));
        assert_eq!(
            input.pending_connections,
            vec!["pending-connection", "second-pending-connection"]
        );
        assert_eq!(
            serde_json::to_value(&input).unwrap(),
            serde_json::to_value(&direct).unwrap()
        );
        assert_eq!(
            allowance.usage().categories,
            direct_allowance.usage().categories
        );
        assert_eq!(input.reserved_bytes, direct.reserved_bytes);
        assert_eq!(allowance.reserved_bytes, direct_allowance.reserved_bytes);
        allowance.release_peak(input.reserved_bytes);
        direct_allowance.release_peak(direct.reserved_bytes);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(direct_allowance.reserved_bytes, 0);
    }

    #[test]
    fn installed_bridge_and_completed_routes_resume_without_duplicate_work() {
        let source = bridge_and_open_route();
        let candidate = crate::constructive::tests::route_candidate_config();
        let learning = crate::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "selected".into(),
        };
        let mut direct_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let direct = RouteGuidancePreparation::new(
            &source,
            &request,
            &candidate,
            &learning,
            &mut direct_allowance,
        )
        .unwrap()
        .into_input(&source, &candidate, &learning, &mut direct_allowance)
        .unwrap_or_else(|(_, failure)| panic!("direct preparation failed: {failure:?}"));
        assert_eq!(direct.bridge_resources.len(), 1);
        assert_eq!(direct.graph.bridge_resources.len(), 1);

        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut preparation =
            RouteGuidancePreparation::new(&source, &request, &candidate, &learning, &mut allowance)
                .unwrap();
        preparation
            .advance_pairs(&source, &candidate, &learning, &mut allowance)
            .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let validation_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::ValidationRecords)
            .unwrap();
        quotas[validation_slot] = 0;
        let copied_before_bridge = allowance
            .usage()
            .categories
            .get(&WorkKind::CopiedBytes)
            .copied()
            .unwrap_or(0);
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            preparation.advance_raster(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Stopped(stop))
                if stop.reason == "page-quantum"
                    && stop.category == Some(WorkKind::ValidationRecords)
        ));
        assert!(
            allowance.usage().categories[&WorkKind::CopiedBytes] > copied_before_bridge,
            "the bridge copy must happen before the validation yield"
        );
        allowance.end_page_window();
        preparation
            .advance_raster(&source, &candidate, &learning, &mut allowance)
            .unwrap();

        let route_cells_before_graph = allowance.usage().categories[&WorkKind::RouteCells];
        let graph_rows_before = allowance
            .usage()
            .categories
            .get(&WorkKind::GraphRows)
            .copied()
            .unwrap_or(0);
        quotas[validation_slot] = usize::MAX;
        let route_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::RouteCells)
            .unwrap();
        quotas[route_slot] = 0;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            preparation.advance_graph(&source, &candidate, &learning, &mut allowance),
            Err(RequestFailure::Stopped(stop))
                if stop.reason == "page-quantum"
                    && stop.category == Some(WorkKind::RouteCells)
        ));
        assert_eq!(
            allowance.usage().categories[&WorkKind::RouteCells],
            route_cells_before_graph
        );
        assert!(allowance.usage().categories[&WorkKind::GraphRows] > graph_rows_before);
        allowance.end_page_window();
        let resumed = preparation
            .into_input(&source, &candidate, &learning, &mut allowance)
            .unwrap_or_else(|(_, failure)| panic!("resumed preparation failed: {failure:?}"));
        assert_eq!(
            serde_json::to_value(&resumed).unwrap(),
            serde_json::to_value(&direct).unwrap()
        );
        assert_eq!(
            allowance.usage().categories,
            direct_allowance.usage().categories
        );
        assert_eq!(allowance.reserved_bytes, direct_allowance.reserved_bytes);
        allowance.release_peak(resumed.reserved_bytes);
        direct_allowance.release_peak(direct.reserved_bytes);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(direct_allowance.reserved_bytes, 0);
    }
}
