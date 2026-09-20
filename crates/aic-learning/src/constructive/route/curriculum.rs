//! Bounded self-generated frontier curriculum and exploration controller.

use std::collections::BTreeSet;

use aic_pilot_native::constructive::{Command, Session};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{TaskBuilder, TaskSpec, rollout::Episode};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub capacity_prefixes: usize,
    pub minimum_sessions_per_update: usize,
    pub maximum_sessions_per_update: usize,
    pub unique_success_target: usize,
    pub consolidation_success_target: usize,
    pub probability_floor: f64,
    pub discovery_temperature: f64,
    pub consolidation_temperature: f64,
    pub mastery_temperature: f64,
    pub fresh_greedy_success_target: usize,
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            capacity_prefixes: 0,
            minimum_sessions_per_update: 0,
            maximum_sessions_per_update: 0,
            unique_success_target: 0,
            consolidation_success_target: 0,
            probability_floor: 0.02,
            discovery_temperature: 1.5,
            consolidation_temperature: 1.,
            mastery_temperature: 0.5,
            fresh_greedy_success_target: 0,
            seed: 0,
        }
    }
}

impl Config {
    pub fn validate(&self, sessions_per_update: usize) -> Result<()> {
        if !self.enabled() {
            ensure!(
                self.capacity_prefixes == 0
                    && self.minimum_sessions_per_update == 0
                    && self.maximum_sessions_per_update == 0
                    && self.unique_success_target == 0
                    && self.consolidation_success_target == 0
                    && self.fresh_greedy_success_target == 0,
                "aic-learning.constructive-route-frontier: partially enabled configuration"
            );
            return Ok(());
        }
        ensure!(
            (1..=4096).contains(&self.capacity_prefixes)
                && (1..=self.maximum_sessions_per_update)
                    .contains(&self.minimum_sessions_per_update)
                && self.maximum_sessions_per_update + 2 <= sessions_per_update
                && (1..=4096).contains(&self.unique_success_target)
                && (1..=4096).contains(&self.consolidation_success_target)
                && self.probability_floor.is_finite()
                && (0.0001..=1.).contains(&self.probability_floor)
                && [
                    self.discovery_temperature,
                    self.consolidation_temperature,
                    self.mastery_temperature,
                ]
                .into_iter()
                .all(|temperature| temperature.is_finite() && (0.05..=20.).contains(&temperature))
                && self.fresh_greedy_success_target > 0,
            "aic-learning.constructive-route-frontier: invalid enabled configuration"
        );
        Ok(())
    }

    pub fn enabled(&self) -> bool {
        self.capacity_prefixes > 0
    }
}

#[derive(Clone)]
pub struct Candidate {
    pub task: TaskSpec,
    pub commands: Vec<Command>,
}

struct Entry {
    identity: [u8; 32],
    task: TaskSpec,
    commands: Vec<Command>,
    admitted_update: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    AwaitingPrefix,
    Discover,
    Consolidate,
    Master,
}

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub phase: Phase,
    pub temperature: f64,
    pub sessions: usize,
    pub probability_lower_95: f64,
    pub budget_probability: f64,
    pub remaining_success_target: usize,
}

#[derive(Default, Serialize)]
pub struct AdmissionReport {
    pub candidates: usize,
    pub admitted: usize,
    pub duplicates: usize,
    pub evicted: usize,
}

#[derive(Serialize)]
pub struct Report {
    pub plan: Plan,
    pub prefixes: usize,
    pub targeted_attempts: usize,
    pub targeted_successes: usize,
    pub unique_successes: usize,
    pub success_rate: f64,
    pub probability_lower_95: f64,
    pub probability_upper_95: f64,
    pub fresh_greedy_crossed_depth_one_successes: usize,
    pub admissions: AdmissionReport,
}

pub struct Buffer {
    config: Config,
    entries: Vec<Entry>,
    cursor: usize,
    targeted_attempts: usize,
    targeted_successes: usize,
    success_identities: BTreeSet<[u8; 32]>,
}

impl Buffer {
    pub fn new(config: Config) -> Self {
        Self {
            cursor: config.seed as usize,
            config,
            entries: Vec::new(),
            targeted_attempts: 0,
            targeted_successes: 0,
            success_identities: BTreeSet::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled()
    }

    pub fn plan(&self, fresh_greedy_successes: usize) -> Plan {
        if !self.enabled() || self.entries.is_empty() {
            return Plan {
                phase: Phase::AwaitingPrefix,
                temperature: 1.,
                sessions: 0,
                probability_lower_95: 0.,
                budget_probability: self.config.probability_floor,
                remaining_success_target: self.config.unique_success_target,
            };
        }
        let (phase, temperature, remaining) =
            if self.success_identities.len() < self.config.unique_success_target {
                (
                    Phase::Discover,
                    self.config.discovery_temperature,
                    self.config
                        .unique_success_target
                        .saturating_sub(self.success_identities.len()),
                )
            } else if fresh_greedy_successes < self.config.fresh_greedy_success_target {
                (
                    Phase::Consolidate,
                    self.config.consolidation_temperature,
                    self.config.consolidation_success_target,
                )
            } else {
                (Phase::Master, self.config.mastery_temperature, 1)
            };
        let (lower, _) = wilson(self.targeted_successes, self.targeted_attempts);
        let budget_probability = lower.max(self.config.probability_floor);
        let estimated = ((remaining as f64) / budget_probability).ceil() as usize;
        let sessions = if matches!(phase, Phase::Master) {
            self.config.minimum_sessions_per_update
        } else {
            estimated.clamp(
                self.config.minimum_sessions_per_update,
                self.config.maximum_sessions_per_update,
            )
        };
        Plan {
            phase,
            temperature,
            sessions,
            probability_lower_95: lower,
            budget_probability,
            remaining_success_target: remaining,
        }
    }

    pub fn sessions(
        &mut self,
        builder: &TaskBuilder,
        count: usize,
    ) -> Result<Vec<(Session, TaskSpec)>> {
        ensure!(
            count == 0 || !self.entries.is_empty(),
            "aic-learning.constructive-route-frontier: requested sessions without prefixes"
        );
        let mut sessions = Vec::with_capacity(count);
        for _ in 0..count {
            let index = self.cursor % self.entries.len();
            self.cursor = self.cursor.wrapping_add(1);
            let entry = &self.entries[index];
            let session = reconstruct(builder, &entry.task, &entry.commands)?;
            sessions.push((session, entry.task.clone()));
        }
        Ok(sessions)
    }

    pub fn admit(
        &mut self,
        builder: &TaskBuilder,
        candidates: Vec<Candidate>,
        update: u64,
    ) -> Result<AdmissionReport> {
        let mut report = AdmissionReport::default();
        if !self.enabled() {
            return Ok(report);
        }
        for candidate in candidates {
            report.candidates += 1;
            reconstruct(builder, &candidate.task, &candidate.commands)?;
            let identity = prefix_identity(&candidate)?;
            if self.entries.iter().any(|entry| entry.identity == identity) {
                report.duplicates += 1;
                continue;
            }
            self.entries.push(Entry {
                identity,
                task: candidate.task,
                commands: candidate.commands,
                admitted_update: update,
            });
            report.admitted += 1;
        }
        while self.entries.len() > self.config.capacity_prefixes {
            let index = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.admitted_update)
                .map(|(index, _)| index)
                .expect("frontier eviction requires an entry");
            self.entries.remove(index);
            report.evicted += 1;
        }
        Ok(report)
    }

    pub fn observe(&mut self, episodes: &[Episode], blocked: usize) -> Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.targeted_attempts += episodes.len() + blocked;
        for episode in episodes.iter().filter(|episode| episode.route_succeeded) {
            self.targeted_successes += 1;
            let encoded = serde_json::to_vec(
                &episode
                    .transitions
                    .iter()
                    .map(|transition| {
                        (
                            transition.observation.input.as_ref(),
                            &transition.choice.factors,
                            &transition.choice.command.command,
                        )
                    })
                    .collect::<Vec<_>>(),
            )?;
            self.success_identities
                .insert(Sha256::digest(encoded).into());
        }
        Ok(())
    }

    pub fn report(
        &self,
        plan: Plan,
        admissions: AdmissionReport,
        fresh_greedy_successes: usize,
    ) -> Report {
        let (lower, upper) = wilson(self.targeted_successes, self.targeted_attempts);
        Report {
            plan,
            prefixes: self.entries.len(),
            targeted_attempts: self.targeted_attempts,
            targeted_successes: self.targeted_successes,
            unique_successes: self.success_identities.len(),
            success_rate: ratio(self.targeted_successes, self.targeted_attempts),
            probability_lower_95: lower,
            probability_upper_95: upper,
            fresh_greedy_crossed_depth_one_successes: fresh_greedy_successes,
            admissions,
        }
    }
}

fn reconstruct(builder: &TaskBuilder, task: &TaskSpec, commands: &[Command]) -> Result<Session> {
    let mut session = builder.build(task)?;
    for command in commands {
        let step = session.step(command.clone());
        ensure!(
            step.accepted,
            "aic-learning.constructive-route-frontier: retained prefix command was rejected"
        );
    }
    let observation = session.observe();
    ensure!(
        task.connections == 2
            && task.crossed
            && observation.completed == 1
            && observation.active.is_none()
            && observation.routes.first().is_some_and(Option::is_some)
            && observation.routes.get(1).is_some_and(Option::is_none)
            && session
                .route_learning_observation_with_abort(
                    &builder.domain,
                    1,
                    builder.config.allow_abort,
                )
                .is_ok(),
        "aic-learning.constructive-route-frontier: retained prefix is not an actionable crossed depth-one state"
    );
    Ok(session)
}

fn prefix_identity(candidate: &Candidate) -> Result<[u8; 32]> {
    let encoded = serde_json::to_vec(&(
        candidate.task.seed & 1,
        candidate.task.rows,
        &candidate.commands,
    ))?;
    Ok(Sha256::digest(encoded).into())
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.
    } else {
        numerator as f64 / denominator as f64
    }
}

fn wilson(successes: usize, attempts: usize) -> (f64, f64) {
    if attempts == 0 {
        return (0., 1.);
    }
    let z = 1.959_963_984_540_054f64;
    let n = attempts as f64;
    let p = successes as f64 / n;
    let denominator = 1. + z * z / n;
    let center = (p + z * z / (2. * n)) / denominator;
    let margin = z * ((p * (1. - p) / n + z * z / (4. * n * n)).sqrt()) / denominator;
    ((center - margin).max(0.), (center + margin).min(1.))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            capacity_prefixes: 32,
            minimum_sessions_per_update: 8,
            maximum_sessions_per_update: 32,
            unique_success_target: 8,
            consolidation_success_target: 2,
            probability_floor: 0.02,
            discovery_temperature: 1.5,
            consolidation_temperature: 1.,
            mastery_temperature: 0.5,
            fresh_greedy_success_target: 4,
            seed: 7,
        }
    }

    #[test]
    fn controller_waits_for_a_prefix_and_bounds_zero_success_budget() {
        let mut buffer = Buffer::new(config());
        let waiting = buffer.plan(0);
        assert!(matches!(waiting.phase, Phase::AwaitingPrefix));
        assert_eq!(waiting.sessions, 0);
        buffer.entries.push(Entry {
            identity: [1; 32],
            task: TaskSpec {
                connections: 2,
                rows: 4,
                crossed: true,
                seed: 1,
            },
            commands: vec![],
            admitted_update: 1,
        });
        let discovery = buffer.plan(0);
        assert!(matches!(discovery.phase, Phase::Discover));
        assert_eq!(discovery.temperature, 1.5);
        assert_eq!(discovery.sessions, 32);
        assert_eq!(discovery.budget_probability, 0.02);
    }

    #[test]
    fn controller_reduces_temperature_after_diverse_success_and_mastery() {
        let mut buffer = Buffer::new(config());
        buffer.entries.push(Entry {
            identity: [1; 32],
            task: TaskSpec {
                connections: 2,
                rows: 4,
                crossed: true,
                seed: 1,
            },
            commands: vec![],
            admitted_update: 1,
        });
        for value in 0..8u8 {
            buffer.success_identities.insert([value; 32]);
        }
        let consolidation = buffer.plan(0);
        assert!(matches!(consolidation.phase, Phase::Consolidate));
        assert_eq!(consolidation.temperature, 1.);
        let mastery = buffer.plan(4);
        assert!(matches!(mastery.phase, Phase::Master));
        assert_eq!(mastery.temperature, 0.5);
        assert_eq!(mastery.sessions, 8);
    }
}
