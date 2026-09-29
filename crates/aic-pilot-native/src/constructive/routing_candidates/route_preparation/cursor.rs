use aic_layout::geometry::{Point, direction};
use aic_layout::logistics::{LogisticsComponentKind, TransportKind};
use aic_layout::rate::Rate;
use aic_routing::{Carrier, Crossing, CrossingOptionsCursor, CrossingVisit, Crossings, RouteJob};
use indexmap::{IndexMap, IndexSet};

use super::super::{Config, RequestAllowance, Session, WorkKind, within_capacity};

struct CarrierDescriptor {
    id: usize,
    network: usize,
    transport: TransportKind,
    rate: Rate,
}

struct InstalledChannel {
    owner_index: usize,
    cell_index: usize,
    heading: Option<usize>,
    load: Rate,
    first_owner: Option<usize>,
    valid: bool,
}

impl InstalledChannel {
    fn new(rate: Rate) -> Self {
        Self {
            owner_index: 0,
            cell_index: 0,
            heading: None,
            load: rate,
            first_owner: None,
            valid: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    ExcludedBodies,
    ExcludedTerminals,
    ExcludedInstalled,
    CatalogOptions,
    InstalledBridges,
    Done,
}

/// Owns every charge-bearing crossing stage for one immutable source session.
/// The caller reserves its estimated storage before constructing this value.
pub(crate) struct RouteCrossingCursor {
    phase: Phase,
    carriers: Vec<CarrierDescriptor>,
    networks: IndexMap<String, usize>,
    bodies: Vec<Point>,
    terminals: Vec<Point>,
    installed: Vec<(TransportKind, Point)>,
    bridges: Vec<(TransportKind, Point)>,
    excluded: IndexSet<Point>,
    excluded_index: usize,
    options_cursor: Option<CrossingOptionsCursor>,
    crossings: Crossings,
    bridge_index: usize,
    channel_index: usize,
    channel: Option<InstalledChannel>,
}

impl RouteCrossingCursor {
    pub(crate) fn new(session: &Session, config: &Config) -> Self {
        if config.max_crossings_per_route == 0 {
            return Self {
                phase: Phase::Done,
                carriers: Vec::new(),
                networks: IndexMap::new(),
                bodies: Vec::new(),
                terminals: Vec::new(),
                installed: Vec::new(),
                bridges: Vec::new(),
                excluded: IndexSet::new(),
                excluded_index: 0,
                options_cursor: None,
                crossings: Crossings::new(),
                bridge_index: 0,
                channel_index: 0,
                channel: None,
            };
        }
        let mut networks = IndexMap::<String, usize>::new();
        let carriers = session
            .routes
            .iter()
            .enumerate()
            .filter_map(|(ci, route)| {
                route.as_ref()?;
                let obligation = &session.problem.connections[ci];
                let network = match networks.get(&obligation.commodity) {
                    Some(&network) => network,
                    None => {
                        let network = networks.len();
                        networks.insert(obligation.commodity.clone(), network);
                        network
                    }
                };
                Some(CarrierDescriptor {
                    id: ci,
                    network,
                    transport: obligation.transport,
                    rate: obligation.rate,
                })
            })
            .collect();
        let bridges = session.bridges.keys().copied().collect::<Vec<_>>();
        Self {
            phase: Phase::ExcludedBodies,
            carriers,
            networks,
            bodies: session.bodies.iter().copied().collect(),
            terminals: session.terminal_at.keys().copied().collect(),
            installed: session
                .junctions
                .keys()
                .chain(session.bridges.keys())
                .copied()
                .collect(),
            bridges,
            excluded: IndexSet::new(),
            excluded_index: 0,
            options_cursor: Some(CrossingOptionsCursor::new()),
            crossings: Crossings::new(),
            bridge_index: 0,
            channel_index: 0,
            channel: None,
        }
    }

    pub(crate) fn advance(
        &mut self,
        session: &Session,
        connection: usize,
        config: &Config,
        allowance: &mut RequestAllowance,
    ) -> Result<bool, super::super::AllowanceStop> {
        let obligation = &session.problem.connections[connection];
        if self.phase == Phase::ExcludedBodies {
            while self.excluded_index < self.bodies.len() {
                allowance.charge(WorkKind::BlockedVisits, 1)?;
                self.excluded.insert(self.bodies[self.excluded_index]);
                self.excluded_index += 1;
            }
            self.excluded_index = 0;
            self.phase = Phase::ExcludedTerminals;
        }
        if self.phase == Phase::ExcludedTerminals {
            while self.excluded_index < self.terminals.len() {
                allowance.charge(WorkKind::BlockedVisits, 1)?;
                self.excluded.insert(self.terminals[self.excluded_index]);
                self.excluded_index += 1;
            }
            self.excluded_index = 0;
            self.phase = Phase::ExcludedInstalled;
        }
        if self.phase == Phase::ExcludedInstalled {
            while self.excluded_index < self.installed.len() {
                allowance.charge(WorkKind::BlockedVisits, 1)?;
                let (transport, point) = self.installed[self.excluded_index];
                if transport == obligation.transport {
                    self.excluded.insert(point);
                }
                self.excluded_index += 1;
            }
            self.phase = Phase::CatalogOptions;
        }
        if self.phase == Phase::CatalogOptions {
            let complete = self
                .options_cursor
                .as_mut()
                .expect("unfinished crossing options")
                .advance_with(
                    self.carriers.len(),
                    |index| {
                        let carrier = &self.carriers[index];
                        Carrier {
                            id: carrier.id,
                            network: carrier.network,
                            transport: carrier.transport,
                            rate: carrier.rate,
                            cells: &session.routes[carrier.id]
                                .as_ref()
                                .expect("retained carrier route")
                                .cells,
                        }
                    },
                    &RouteJob {
                        id: connection,
                        transport: obligation.transport,
                        rate: obligation.rate,
                    },
                    &session.catalogs.components,
                    &self.excluded,
                    |visit| {
                        allowance.charge(
                            match visit {
                                CrossingVisit::RouteCell => WorkKind::RouteCells,
                                CrossingVisit::CatalogEntry => WorkKind::CatalogVisits,
                                CrossingVisit::Candidate => WorkKind::RoutingScans,
                            },
                            1,
                        )
                    },
                )?;
            if complete {
                self.crossings = self
                    .options_cursor
                    .take()
                    .expect("completed crossing options")
                    .finish();
                self.phase = Phase::InstalledBridges;
            }
        }
        if self.phase == Phase::InstalledBridges {
            self.advance_installed(session, connection, config, allowance)?;
            self.phase = Phase::Done;
        }
        Ok(self.phase == Phase::Done)
    }

    pub(crate) fn finish(self) -> Crossings {
        self.crossings
    }

    fn advance_installed(
        &mut self,
        session: &Session,
        connection: usize,
        config: &Config,
        allowance: &mut RequestAllowance,
    ) -> Result<(), super::super::AllowanceStop> {
        let obligation = &session.problem.connections[connection];
        while self.bridge_index < self.bridges.len() {
            let (transport, position) = self.bridges[self.bridge_index];
            if transport != obligation.transport || !config.bounds.contains(position) {
                self.next_bridge();
                continue;
            }
            let bridge = &session.bridges[&(transport, position)];
            let Some(definition) = session.catalogs.components.component(&bridge.component) else {
                self.next_bridge();
                continue;
            };
            let line = session.catalogs.transports.capacity(transport);
            while self.channel_index < bridge.channels.len() {
                let channel = &bridge.channels[self.channel_index];
                if channel.commodity != obligation.commodity {
                    self.next_channel();
                    continue;
                }
                let progress = self
                    .channel
                    .get_or_insert_with(|| InstalledChannel::new(obligation.rate));
                while progress.owner_index < channel.connections.len() {
                    let owner = &channel.connections[progress.owner_index];
                    let Some(&ci) = session.connections.get(owner) else {
                        progress.valid = false;
                        break;
                    };
                    let Some(route) = session.routes[ci].as_ref() else {
                        progress.valid = false;
                        break;
                    };
                    let mut found = None;
                    while progress.cell_index < route.cells.len() {
                        allowance.charge(WorkKind::RouteCells, 1)?;
                        let index = progress.cell_index;
                        progress.cell_index += 1;
                        if route.cells[index] == position {
                            found = Some(index);
                            break;
                        }
                    }
                    let Some(index) = found else {
                        progress.valid = false;
                        break;
                    };
                    if index == 0 || index + 1 == route.cells.len() {
                        progress.valid = false;
                        break;
                    }
                    let Some(heading) = direction(route.cells[index - 1], position) else {
                        progress.valid = false;
                        break;
                    };
                    if direction(route.cells[index], route.cells[index + 1]) != Some(heading)
                        || progress.heading.is_some_and(|old| old != heading)
                    {
                        progress.valid = false;
                        break;
                    }
                    let Ok(load) = progress
                        .load
                        .checked_add(session.problem.connections[ci].rate)
                    else {
                        progress.valid = false;
                        break;
                    };
                    progress.load = load;
                    progress.heading = Some(heading);
                    progress.first_owner.get_or_insert(ci);
                    progress.owner_index += 1;
                    progress.cell_index = 0;
                }
                if progress.valid {
                    if let (Some(direction), Some(carrier_chain)) =
                        (progress.heading, progress.first_owner)
                    {
                        if within_capacity(
                            progress.load,
                            definition.capacity.quantity,
                            definition.capacity.duration_ms,
                        ) && within_capacity(progress.load, line.quantity, line.duration_ms)
                        {
                            if let Some(&carrier_network) =
                                self.networks.get(channel.commodity.as_str())
                            {
                                self.crossings.insert(
                                    (position, direction),
                                    Crossing {
                                        position,
                                        direction,
                                        component: bridge.component.clone(),
                                        rotation: bridge.rotation,
                                        transport,
                                        kind: LogisticsComponentKind::Bridge,
                                        carrier_chain,
                                        carrier_network,
                                    },
                                );
                            }
                        }
                    }
                }
                self.next_channel();
            }
            self.next_bridge();
        }
        Ok(())
    }

    fn next_channel(&mut self) {
        self.channel = None;
        self.channel_index += 1;
    }

    fn next_bridge(&mut self) {
        self.channel = None;
        self.channel_index = 0;
        self.bridge_index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_layout::{
        logistics::TransportKind,
        physical::{Block, BridgeAxis, Connection, Problem},
    };
    use std::time::Duration;

    use super::super::super::super::{Command, Direction, Limits};
    use super::super::super::{
        CUMULATIVE_WORK_KINDS, Request, RequestFailure, RouteCandidatePager,
        RouteCandidatePreparation, route_preparation,
    };

    fn horizontal_session() -> Session {
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
            super::super::super::super::tests::catalogs(),
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
        session
    }

    fn bridge_session() -> Session {
        let mut session = horizontal_session();
        let crossing = session.routes[0].as_ref().unwrap().cells[1];
        let option = aic_routing::crossing_options(
            &[Carrier {
                id: 0,
                network: 0,
                transport: TransportKind::Belt,
                rate: session.problem.connections[0].rate,
                cells: &session.routes[0].as_ref().unwrap().cells,
            }],
            &RouteJob {
                id: 1,
                transport: TransportKind::Belt,
                rate: session.problem.connections[1].rate,
            },
            &session.catalogs.components,
            &IndexSet::new(),
        )
        .get(&(crossing, 3))
        .cloned()
        .unwrap();
        assert!(
            session
                .step(Command::BeginRoute {
                    connection: "vertical".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let start = session.active.as_ref().unwrap().cells[0];
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: start.y - crossing.y - 1,
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
        assert!(
            session
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 1,
                })
                .accepted
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
        session
    }

    #[test]
    fn carrier_inventory_yield_retains_work_and_hard_cap() {
        let source = horizontal_session();
        let mut config = super::super::super::super::tests::route_candidate_config();
        config.max_crossings_per_route = 1;
        let request = Request {
            state_revision: source.revision(),
            connection: "vertical".into(),
        };
        let mut baseline_allowance = RequestAllowance::new(config.request_allowance.clone());
        let baseline = RouteCandidatePager::new(
            &source,
            request.clone(),
            config.clone(),
            None,
            vec![17],
            &mut baseline_allowance,
        )
        .unwrap();
        assert_eq!(
            baseline_allowance.usage().categories[&WorkKind::ValidationRecords],
            1
        );

        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut preparation = RouteCandidatePreparation::new(
            &source,
            request.clone(),
            config.clone(),
            None,
            vec![17],
            &mut allowance,
        )
        .unwrap();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        preparation.advance_guidance(&mut allowance).unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::ValidationRecords)
            .unwrap();
        quotas[slot] = 0;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let stop = preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap_err();
        assert!(matches!(stop, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum"));
        allowance.end_page_window();
        preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_crossings(&source, &mut allowance)
            .unwrap();
        let pager = preparation.finish(&source, &mut allowance).unwrap();
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
        assert_eq!(allowance.reserved_bytes, baseline_allowance.reserved_bytes);
        pager.release(&mut allowance);
        baseline.release(&mut baseline_allowance);
        assert_eq!(allowance.reserved_bytes, 0);

        let mut capped_config = config;
        capped_config.request_allowance.validation_records = 1;
        let mut capped = RequestAllowance::new(capped_config.request_allowance.clone());
        capped.charge(WorkKind::ValidationRecords, 1).unwrap();
        let stop =
            RouteCandidatePager::new(&source, request, capped_config, None, vec![17], &mut capped)
                .err()
                .expect("carrier inventory must obey its hard work cap");
        assert!(
            matches!(stop, RequestFailure::Stopped(ref stop)
            if stop.reason == "work-limit" && stop.category == Some(WorkKind::ValidationRecords)),
            "unexpected carrier cap result: {stop:?}"
        );
        assert_eq!(capped.reserved_bytes, 0);
    }

    #[test]
    fn installed_bridge_owner_scan_resumes_without_recharging_or_changing_options() {
        let source = bridge_session();
        let mut config = super::super::super::super::tests::route_candidate_config();
        config.max_crossings_per_route = 1;
        let connection = 1;
        let mut baseline_allowance = RequestAllowance::new(Default::default());
        let expected = route_preparation::route_crossings(
            &source,
            connection,
            &config,
            &mut baseline_allowance,
        )
        .unwrap();
        assert!(expected.iter().any(|(_, crossing)| {
            crossing.position == source.routes[0].as_ref().unwrap().cells[1]
        }));

        let mut allowance = RequestAllowance::new(Default::default());
        let mut cursor = RouteCrossingCursor::new(&source, &config);
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let route_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::RouteCells)
            .unwrap();
        quotas[route_slot] = 1;
        let mut stopped_inside_owner = false;
        let max_pages = baseline_allowance.usage().categories[&WorkKind::RouteCells] + 1;
        for _ in 0..max_pages {
            allowance.begin_page_window(quotas, Duration::from_secs(1));
            let result = cursor.advance(&source, connection, &config, &mut allowance);
            allowance.end_page_window();
            if cursor.phase == Phase::InstalledBridges
                && cursor
                    .channel
                    .as_ref()
                    .is_some_and(|channel| channel.cell_index > 0)
            {
                assert!(matches!(result, Err(ref stop) if stop.reason == "page-quantum"));
                stopped_inside_owner = true;
                break;
            }
            assert!(matches!(result, Err(ref stop) if stop.reason == "page-quantum"));
        }
        assert!(stopped_inside_owner);
        assert!(
            cursor
                .advance(&source, connection, &config, &mut allowance)
                .unwrap()
        );
        assert_eq!(cursor.finish(), expected);
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
    }
}
