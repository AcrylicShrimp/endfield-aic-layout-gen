//! Source-bound analytic scoring for native placement commands.

use super::{
    ACTION_FEATURES, AllowanceStop, Bounds, Command, MacroAction, Point, Result,
    SearchMemoryBudget, Session, TreeReservation, action_bytes, placement_host_stop, state_value,
};
use anyhow::ensure;

#[derive(Clone, Copy)]
struct Incident {
    connection: usize,
    peer: Option<Point>,
    same_entity: bool,
}

pub(super) struct PlacementScore {
    pub(super) features: [f32; ACTION_FEATURES],
    pub(super) entity_index: i64,
    pub(super) anchor_score: f64,
    pub(super) child_value: f64,
}

pub(super) enum PreparedPlacement<'s> {
    Ready(PlacementActionScorer<'s>),
    MemoryStop(AllowanceStop),
}

pub(super) enum ScoredPlacement {
    Ready(PlacementScore),
    MemoryStop(AllowanceStop),
}

pub(super) struct PlacementActionScorer<'s> {
    source_instance_id: u64,
    source_revision: u64,
    bounds: Bounds,
    before_progress: f64,
    after_progress: f64,
    completion_may_publish_metrics: bool,
    entities_by_id: Vec<(&'s str, usize)>,
    fixed_connections: Vec<Option<f64>>,
    incidents_by_entity: Vec<Vec<Incident>>,
    _reservation: TreeReservation,
}

pub(super) struct PlacementPageScorer<'p, 's> {
    scorer: &'p PlacementActionScorer<'s>,
    source: &'p Session,
}

impl<'s> PlacementActionScorer<'s> {
    pub(super) fn prepare(
        source: &'s Session,
        bounds: Bounds,
        budget: &SearchMemoryBudget,
    ) -> Result<PreparedPlacement<'s>> {
        let problem = source.problem();
        let entity_count = problem.blocks.len() + problem.terminals.len();
        let connections = problem.connections.len();
        let estimated = entity_count
            .checked_mul(
                std::mem::size_of::<(&str, usize)>()
                    + std::mem::size_of::<Vec<Incident>>()
                    + std::mem::size_of::<usize>(),
            )
            .and_then(|value| {
                connections
                    .checked_mul(
                        std::mem::size_of::<Option<f64>>() + 2 * std::mem::size_of::<Incident>(),
                    )
                    .and_then(|connections| value.checked_add(connections))
            })
            .and_then(|value| value.checked_add(4096));
        let Some(estimated) = estimated else {
            return Ok(PreparedPlacement::MemoryStop(placement_host_stop(
                budget,
                usize::MAX,
            )));
        };
        let reservation = match TreeReservation::new(budget, estimated) {
            Ok(reservation) => reservation,
            Err(stop) => return Ok(PreparedPlacement::MemoryStop(stop)),
        };
        let host_stop = || placement_host_stop(budget, estimated);

        let mut entities_by_id = Vec::new();
        if entities_by_id.try_reserve_exact(entity_count).is_err() {
            return Ok(PreparedPlacement::MemoryStop(host_stop()));
        }
        for (index, block) in problem.blocks.iter().enumerate() {
            entities_by_id.push((block.id.as_str(), index));
        }
        for (index, terminal) in problem.terminals.iter().enumerate() {
            entities_by_id.push((terminal.id.as_str(), problem.blocks.len() + index));
        }
        entities_by_id.sort_unstable_by(|left, right| left.0.cmp(right.0));
        let entity_index = |id: &str| {
            entities_by_id
                .binary_search_by(|candidate| candidate.0.cmp(id))
                .ok()
                .map(|index| entities_by_id[index].1)
                .expect("validated connection endpoint belongs to problem")
        };

        let mut degree = Vec::<usize>::new();
        if degree.try_reserve_exact(entity_count).is_err() {
            return Ok(PreparedPlacement::MemoryStop(host_stop()));
        }
        degree.resize(entity_count, 0);
        for connection in &problem.connections {
            degree[entity_index(&connection.source)] += 1;
            if connection.source != connection.target {
                degree[entity_index(&connection.target)] += 1;
            }
        }
        let mut incidents_by_entity = Vec::<Vec<Incident>>::new();
        if incidents_by_entity.try_reserve_exact(entity_count).is_err() {
            return Ok(PreparedPlacement::MemoryStop(host_stop()));
        }
        for count in degree {
            let mut incidents = Vec::new();
            if incidents.try_reserve_exact(count).is_err() {
                return Ok(PreparedPlacement::MemoryStop(host_stop()));
            }
            incidents_by_entity.push(incidents);
        }
        let mut fixed_connections = Vec::new();
        if fixed_connections.try_reserve_exact(connections).is_err() {
            return Ok(PreparedPlacement::MemoryStop(host_stop()));
        }
        let span = bounds.width().max(bounds.height()).max(1) as f64;
        for (index, connection) in problem.connections.iter().enumerate() {
            let source_position = source.placed_entity_position(&connection.source);
            let target_position = source.placed_entity_position(&connection.target);
            fixed_connections.push(
                source_position
                    .zip(target_position)
                    .map(|(source, target)| source.distance(target) as f64 / span),
            );
            let source_entity = entity_index(&connection.source);
            let target_entity = entity_index(&connection.target);
            incidents_by_entity[source_entity].push(Incident {
                connection: index,
                peer: target_position,
                same_entity: source_entity == target_entity,
            });
            if source_entity != target_entity {
                incidents_by_entity[target_entity].push(Incident {
                    connection: index,
                    peer: source_position,
                    same_entity: false,
                });
            }
        }

        let placed = problem
            .blocks
            .iter()
            .filter(|block| source.placed_entity_position(&block.id).is_some())
            .count()
            + problem
                .terminals
                .iter()
                .filter(|terminal| source.placed_entity_position(&terminal.id).is_some())
                .count();
        let (routed, total_routes) = source.connection_progress_counts();
        let total = entity_count + total_routes;
        Ok(PreparedPlacement::Ready(Self {
            source_instance_id: source.in_memory_instance_id(),
            source_revision: source.revision(),
            bounds,
            before_progress: source.construction_progress(),
            after_progress: (placed + routed + 1) as f64 / total.max(1) as f64,
            completion_may_publish_metrics: entity_count.saturating_sub(placed) == 1
                && routed == total_routes,
            entities_by_id,
            fixed_connections,
            incidents_by_entity,
            _reservation: reservation,
        }))
    }

    pub(super) fn begin_page<'p>(
        &'p self,
        source: &'p Session,
    ) -> Result<PlacementPageScorer<'p, 's>> {
        ensure!(
            source.in_memory_instance_id() == self.source_instance_id
                && source.revision() == self.source_revision,
            "aic-learning.placement-scorer-source: prepared source instance or revision changed"
        );
        Ok(PlacementPageScorer {
            scorer: self,
            source,
        })
    }
}

impl PlacementPageScorer<'_, '_> {
    pub(super) fn score(
        &self,
        command: &Command,
        budget: &SearchMemoryBudget,
    ) -> Result<ScoredPlacement> {
        let (entity, position, rotation, kind) = match command {
            Command::Place {
                block,
                position,
                rotation,
            } => (block.as_str(), *position, Some(*rotation), 0),
            Command::PlaceTerminal { terminal, position } => {
                (terminal.as_str(), *position, None, 1)
            }
            _ => anyhow::bail!("aic-learning.placement-scorer-command: expected placement"),
        };
        if self.scorer.completion_may_publish_metrics {
            let action = MacroAction::Command(command.clone());
            let estimate = self
                .source
                .clone_reservation_bytes()
                .saturating_add(action_bytes(&action))
                .saturating_add(8192);
            let mut reservation = match TreeReservation::new(budget, estimate) {
                Ok(reservation) => reservation,
                Err(stop) => return Ok(ScoredPlacement::MemoryStop(stop)),
            };
            let mut child = self.source.clone();
            let step = child.step(command.clone());
            if let Some(diagnostic) = step.diagnostic {
                return Err(anyhow::Error::new(diagnostic));
            }
            ensure!(
                step.accepted && !child.has_active_route(),
                "aic-learning.placement-scorer-native: native placement did not complete"
            );
            if let Err(stop) = reservation.resize(child.clone_reservation_bytes()) {
                return Ok(ScoredPlacement::MemoryStop(stop));
            }
            return Ok(ScoredPlacement::Ready(PlacementScore {
                features: action.features(self.source, &child, self.scorer.bounds),
                entity_index: action.entity_index(self.source.problem())?,
                anchor_score: action.prior_score(self.source, &child, self.scorer.bounds),
                child_value: state_value(&child, self.scorer.bounds),
            }));
        }

        let index = self
            .scorer
            .entities_by_id
            .binary_search_by(|candidate| candidate.0.cmp(entity))
            .map(|index| self.scorer.entities_by_id[index].1)
            .map_err(|_| {
                anyhow::anyhow!(
                    "aic-learning.placement-scorer-entity: command entity is absent from source"
                )
            })?;
        let mut features = [0.0f32; ACTION_FEATURES];
        features[kind] = 1.0;
        features[3] = self.scorer.before_progress as f32;
        features[4] = self.scorer.after_progress as f32;
        features[5] = features[4] - features[3];
        let scale = self
            .scorer
            .bounds
            .width()
            .max(self.scorer.bounds.height())
            .max(1) as f32;
        features[6] = position.x as f32 / scale;
        features[7] = position.y as f32 / scale;
        if let Some(rotation) = rotation {
            features[8] = (rotation as f32).to_radians().sin();
            features[9] = (rotation as f32).to_radians().cos();
        }

        let mut incident_index = 0usize;
        let incidents = &self.scorer.incidents_by_entity[index];
        let span = self
            .scorer
            .bounds
            .width()
            .max(self.scorer.bounds.height())
            .max(1) as f64;
        let connected_distance = self
            .scorer
            .fixed_connections
            .iter()
            .enumerate()
            .filter_map(|(connection, fixed)| {
                if incidents
                    .get(incident_index)
                    .map(|incident| incident.connection)
                    != Some(connection)
                {
                    return *fixed;
                }
                let incident = incidents[incident_index];
                incident_index += 1;
                if incident.same_entity {
                    Some(0.0)
                } else {
                    incident
                        .peer
                        .map(|peer| position.distance(peer) as f64 / span)
                }
            })
            .sum::<f64>();
        let center_x = (self.scorer.bounds.min_x + self.scorer.bounds.max_x) as f64 * 0.5;
        let center_y = (self.scorer.bounds.min_y + self.scorer.bounds.max_y) as f64 * 0.5;
        let center_distance =
            ((position.x as f64 - center_x).abs() + (position.y as f64 - center_y).abs()) / span;
        let progress = (self.scorer.after_progress - self.scorer.before_progress) * 8.0;
        let anchor_score = progress + (-center_distance * 0.08 - connected_distance * 0.12);
        Ok(ScoredPlacement::Ready(PlacementScore {
            features,
            entity_index: index as i64,
            anchor_score,
            child_value: self.scorer.after_progress,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_layout::{
        logistics::TransportKind,
        physical::{Terminal, TerminalDirection},
    };
    use aic_pilot_native::constructive::{
        Diagnostic, Direction, learning::CommandKind, routing_candidates::Config as CandidateConfig,
    };

    fn candidate(bounds: Bounds) -> CandidateConfig {
        CandidateConfig {
            bounds,
            max_candidates: 8,
            max_searches: 8,
            work_per_search: 100,
            max_expanded: 100,
            overlap_penalty: 8,
            max_crossings_per_route: 0,
            request_allowance: Default::default(),
        }
    }

    fn compare_page(session: &Session, kind: CommandKind, offset: usize) -> Result<usize> {
        let domain_config = crate::constructive::test_support::domain();
        let domain = session.learning_placement_domain(&domain_config)?;
        let page = domain.command_page(kind, offset, 16)?;
        let budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        let scorer =
            match PlacementActionScorer::prepare(session, domain_config.placement, &budget)? {
                PreparedPlacement::Ready(scorer) => scorer,
                PreparedPlacement::MemoryStop(stop) => {
                    anyhow::bail!("placement scorer preparation stopped: {stop:?}")
                }
            };
        let source_clone = session.clone();
        ensure!(
            scorer.begin_page(&source_clone).is_err(),
            "distinct session at same revision must not use scorer"
        );
        let page_scorer = scorer.begin_page(session)?;
        let candidate = candidate(domain_config.placement);
        let mut scoring_seconds = 0.0f64;
        for factored in &page.commands {
            let command = &factored.command.command;
            let started = std::time::Instant::now();
            let score = match page_scorer.score(command, &budget)? {
                ScoredPlacement::Ready(score) => score,
                ScoredPlacement::MemoryStop(stop) => {
                    anyhow::bail!("placement scorer stopped: {stop:?}")
                }
            };
            scoring_seconds += started.elapsed().as_secs_f64();
            let action = MacroAction::Command(command.clone());
            let child = action.apply(session, &candidate, 0)?;
            assert_eq!(
                score.features,
                action.features(session, &child, candidate.bounds)
            );
            assert_eq!(score.entity_index, action.entity_index(session.problem())?);
            assert_eq!(
                score.anchor_score.to_bits(),
                action
                    .prior_score(session, &child, candidate.bounds)
                    .to_bits()
            );
            assert_eq!(score.child_value, state_value(&child, candidate.bounds));
        }
        drop(page_scorer);
        drop(scorer);
        assert_eq!(budget.live_bytes(), 0);
        if std::env::var("AIC_PLAN01_MEASURE").as_deref() == Ok("1") {
            eprintln!(
                "plan01 ordinary kind={kind:?} count={} scoring_seconds={scoring_seconds:.6}",
                page.commands.len()
            );
        }
        Ok(page.commands.len())
    }

    #[test]
    fn prepared_placement_scores_match_native_children() -> Result<()> {
        let partial = crate::constructive::test_support::session(64);
        assert!(compare_page(&partial, CommandKind::Place, 0)? > 0);
        assert!(compare_page(&partial, CommandKind::Place, 64)? > 0);

        let mut placed_peer = partial.clone();
        ensure!(
            placed_peer
                .step(Command::Place {
                    block: "source".into(),
                    position: Point { x: 3, y: 3 },
                    rotation: 0,
                })
                .accepted,
            "placed-peer fixture rejected"
        );
        assert!(compare_page(&placed_peer, CommandKind::Place, 0)? > 0);
        assert!(compare_page(&placed_peer, CommandKind::Place, 32)? > 0);

        let mut terminal_problem = partial.problem().clone();
        terminal_problem.terminals.push(Terminal {
            id: "unconnected-terminal".into(),
            direction: TerminalDirection::Input,
            commodity: "test-item".into(),
            transport: TransportKind::Belt,
        });
        let with_terminal = Session::new(
            terminal_problem,
            crate::constructive::test_support::catalogs(),
            aic_pilot_native::constructive::Limits {
                max_turns: 64,
                max_extension: 8,
            },
        )?;
        assert!(compare_page(&with_terminal, CommandKind::PlaceTerminal, 0)? > 0);
        Ok(())
    }

    #[test]
    fn final_placement_scores_native_completion_and_rejects_stale_page() -> Result<()> {
        let mut problem = crate::constructive::test_support::session(64)
            .problem()
            .clone();
        problem.terminals.push(Terminal {
            id: "unconnected-terminal".into(),
            direction: TerminalDirection::Input,
            commodity: "test-item".into(),
            transport: TransportKind::Belt,
        });
        let mut session = Session::new(
            problem,
            crate::constructive::test_support::catalogs(),
            aic_pilot_native::constructive::Limits {
                max_turns: 64,
                max_extension: 16,
            },
        )?;
        for command in [
            Command::Place {
                block: "source".into(),
                position: Point { x: 5, y: 8 },
                rotation: 0,
            },
            Command::Place {
                block: "target".into(),
                position: Point { x: 5, y: 2 },
                rotation: 0,
            },
            Command::BeginRoute {
                connection: "connection-0".into(),
                source_port: Some("output-belt-1".into()),
                target_port: Some("input-belt-1".into()),
            },
            Command::Extend {
                direction: Direction::North,
                length: 2,
            },
            Command::Connect,
        ] {
            ensure!(session.step(command).accepted, "fixture command rejected");
        }
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 15,
            max_y: 15,
        };
        let domain_config = aic_pilot_native::constructive::learning::Config {
            placement: bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        let scorer = match PlacementActionScorer::prepare(&session, bounds, &budget)? {
            PreparedPlacement::Ready(scorer) => scorer,
            PreparedPlacement::MemoryStop(stop) => anyhow::bail!("scorer stopped: {stop:?}"),
        };
        let page = session
            .learning_placement_domain(&domain_config)?
            .command_page(CommandKind::PlaceTerminal, 0, 16)?;
        ensure!(!page.commands.is_empty(), "missing final terminal poses");
        let page_scorer = scorer.begin_page(&session)?;
        let candidate = candidate(bounds);
        let invalid = Command::PlaceTerminal {
            terminal: "missing-terminal".into(),
            position: Point { x: 0, y: 0 },
        };
        let error = match page_scorer.score(&invalid, &budget) {
            Ok(_) => anyhow::bail!("invalid final placement was accepted"),
            Err(error) => error,
        };
        assert_eq!(
            error
                .downcast_ref::<Diagnostic>()
                .map(|error| error.code.as_str()),
            Some("constructive.unknown-terminal")
        );
        let mut completed = 0;
        let mut scoring_seconds = 0.0f64;
        for factored in &page.commands {
            let command = &factored.command.command;
            let started = std::time::Instant::now();
            let ScoredPlacement::Ready(score) = page_scorer.score(command, &budget)? else {
                anyhow::bail!("final placement scorer stopped");
            };
            scoring_seconds += started.elapsed().as_secs_f64();
            let action = MacroAction::Command(command.clone());
            let child = action.apply(&session, &candidate, 0)?;
            assert_eq!(score.features, action.features(&session, &child, bounds));
            assert_eq!(score.entity_index, action.entity_index(session.problem())?);
            assert_eq!(
                score.anchor_score.to_bits(),
                action.prior_score(&session, &child, bounds).to_bits()
            );
            assert_eq!(score.child_value, state_value(&child, bounds));
            completed += usize::from(child.metrics().is_some());
        }
        assert!(completed > 0, "fixture never published completion metrics");
        if std::env::var("AIC_PLAN01_MEASURE").as_deref() == Ok("1") {
            eprintln!(
                "plan01 final-terminal count={} completed={completed} scoring_seconds={scoring_seconds:.6}",
                page.commands.len()
            );
        }
        drop(page_scorer);
        let mut changed = session.clone();
        let extra = page.commands[0].command.command.clone();
        assert!(changed.step(extra).accepted);
        assert!(scorer.begin_page(&changed).is_err());
        drop(scorer);
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }

    #[test]
    #[ignore = "bounded local sparse production-scorer throughput probe"]
    fn sparse_prepared_placement_scoring_probe() -> Result<()> {
        let session = crate::constructive::test_support::session(64);
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 319,
            max_y: 319,
        };
        let domain_config = aic_pilot_native::constructive::learning::Config {
            placement: bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let domain = session.learning_placement_domain(&domain_config)?;
        let budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        let scorer = match PlacementActionScorer::prepare(&session, bounds, &budget)? {
            PreparedPlacement::Ready(scorer) => scorer,
            PreparedPlacement::MemoryStop(stop) => anyhow::bail!("sparse scorer stopped: {stop:?}"),
        };
        let page_scorer = scorer.begin_page(&session)?;
        let started = std::time::Instant::now();
        let mut offset = 0usize;
        let mut count = 0usize;
        let mut checksum = 0.0f64;
        loop {
            let page = domain.command_page(CommandKind::Place, offset, 256)?;
            for factored in &page.commands {
                match page_scorer.score(&factored.command.command, &budget)? {
                    ScoredPlacement::Ready(score) => {
                        checksum += score.anchor_score;
                        count += 1;
                    }
                    ScoredPlacement::MemoryStop(stop) => {
                        anyhow::bail!("sparse scorer stopped: {stop:?}")
                    }
                }
            }
            offset = page.next_offset;
            if page.exhausted {
                break;
            }
        }
        eprintln!(
            "plan01 sparse scorer count={count} seconds={:.3} retained_bytes={} checksum={checksum:.9}",
            started.elapsed().as_secs_f64(),
            budget.live_bytes(),
        );
        assert_eq!(count, 808_992);
        Ok(())
    }
}
