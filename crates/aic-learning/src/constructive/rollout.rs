//! Batched native episode collection and certified-progress reward accounting.

use aic_pilot_native::constructive::{
    Session, Step,
    learning::{Config as DomainConfig, Observation},
};
use aic_torch::Generator;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::collection::credit;

use super::{policy, ppo::Learner};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RewardConfig {
    pub progress_scale: f64,
    pub full_validation_bonus: f64,
    pub rejected_penalty: f64,
    pub unfinished_penalty: f64,
}

impl RewardConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.progress_scale.is_finite()
                && self.progress_scale >= 0.
                && self.full_validation_bonus.is_finite()
                && self.full_validation_bonus > self.progress_scale
                && self.rejected_penalty.is_finite()
                && self.rejected_penalty <= 0.
                && self.unfinished_penalty.is_finite()
                && self.unfinished_penalty <= 0.,
            "aic-learning.constructive-reward: invalid coefficients"
        );
        Ok(())
    }

    fn reward(&self, step: &Step) -> f64 {
        self.progress_scale * step.progress_delta
            + if step.fully_validated {
                self.full_validation_bonus
            } else {
                0.
            }
            + if step.accepted {
                0.
            } else {
                self.rejected_penalty
            }
            + if step.terminal && !step.fully_validated {
                self.unfinished_penalty
            } else {
                0.
            }
    }
}

pub struct Transition {
    pub observation: Observation,
    pub choice: policy::Choice,
    pub reward: f64,
    pub terminal: bool,
    pub accepted: bool,
    pub progress_delta: f64,
    pub fully_validated: bool,
}

#[derive(Default)]
pub struct Episode {
    pub transitions: Vec<Transition>,
    pub fully_validated: bool,
    pub dead_end: bool,
    /// Set by the route-option collector when this trajectory committed its
    /// selected connection. Full-construction collection leaves it false.
    pub route_succeeded: bool,
    /// Number of routes already committed by the same policy in this native
    /// session before the option began.
    pub rollin_depth: usize,
    /// Whether the route task reverses source and target row order.
    pub route_crossed: bool,
    pub completed_connections: usize,
    pub total_connections: usize,
}

pub struct Collection {
    pub episodes: Vec<Episode>,
    pub turns: usize,
    pub accepted: usize,
    pub completed: usize,
    pub seconds: f64,
}

pub struct Credit {
    pub episode: usize,
    pub transition: usize,
    pub advantage: f32,
    pub target: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampling {
    Training,
    GreedyEvaluation,
    StochasticEvaluation { seed: u64 },
}

pub fn collect(
    sessions: &mut [Session],
    domain: &DomainConfig,
    reward: &RewardConfig,
    learner: &mut Learner,
    sampling: Sampling,
) -> Result<Collection> {
    ensure!(
        !sessions.is_empty(),
        "aic-learning.constructive-collection: no sessions"
    );
    reward.validate()?;
    let started = Instant::now();
    let mut evaluation_sampler = match sampling {
        Sampling::StochasticEvaluation { seed } => Some(Generator::new(seed)?),
        _ => None,
    };
    let mut episodes: Vec<_> = sessions.iter().map(|_| Episode::default()).collect();
    let mut active: Vec<_> = (0..sessions.len()).collect();
    while !active.is_empty() {
        let mut owners = Vec::new();
        let mut observations = Vec::new();
        for &owner in &active {
            if sessions[owner].done() {
                continue;
            }
            match sessions[owner].learning_observation(domain) {
                Ok(observation) => {
                    owners.push(owner);
                    observations.push(observation);
                }
                Err(diagnostic) if diagnostic.code == "constructive.empty-learning-domain" => {
                    episodes[owner].dead_end = true;
                    if let Some(last) = episodes[owner].transitions.last_mut() {
                        last.terminal = true;
                        last.reward += reward.unfinished_penalty;
                    }
                }
                Err(diagnostic) => return Err(diagnostic.into()),
            }
        }
        if owners.is_empty() {
            break;
        }
        let rows = observations.iter().collect::<Vec<_>>();
        let choices = match sampling {
            Sampling::Training => learner.sample(&rows)?,
            Sampling::GreedyEvaluation => learner.greedy(&rows)?,
            Sampling::StochasticEvaluation { .. } => policy::sample(
                &learner.model,
                &rows,
                evaluation_sampler
                    .as_mut()
                    .expect("stochastic evaluation initialized generator"),
                learner.device,
            )?,
        };
        for ((owner, observation), choice) in owners.iter().copied().zip(observations).zip(choices)
        {
            let step = sessions[owner].step_selected(choice.command.clone());
            let step_reward = reward.reward(&step);
            episodes[owner].transitions.push(Transition {
                observation,
                choice,
                reward: step_reward,
                terminal: step.terminal,
                accepted: step.accepted,
                progress_delta: step.progress_delta,
                fully_validated: step.fully_validated,
            });
        }
        active.retain(|&owner| !sessions[owner].done() && !episodes[owner].dead_end);
    }
    for (episode, session) in episodes.iter_mut().zip(sessions) {
        let observation = session.observe();
        episode.fully_validated = observation.fully_validated;
        episode.completed_connections = observation.completed;
        episode.total_connections = observation.total;
        ensure!(
            !episode.transitions.is_empty()
                && episode.transitions.last().is_some_and(|step| step.terminal)
                && episode
                    .transitions
                    .iter()
                    .all(|step| step.reward.is_finite()),
            "aic-learning.constructive-collection: incomplete episode accounting"
        );
    }
    let turns = episodes
        .iter()
        .map(|episode| episode.transitions.len())
        .sum();
    let accepted = episodes
        .iter()
        .flat_map(|episode| &episode.transitions)
        .filter(|transition| transition.accepted)
        .count();
    let completed = episodes
        .iter()
        .filter(|episode| episode.fully_validated)
        .count();
    Ok(Collection {
        episodes,
        turns,
        accepted,
        completed,
        seconds: started.elapsed().as_secs_f64(),
    })
}

pub fn gae(collection: &Collection, gamma: f64, lambda: f64) -> Result<Vec<Credit>> {
    let mut credited = Vec::with_capacity(collection.turns);
    for (episode, trajectory) in collection.episodes.iter().enumerate() {
        let rewards = trajectory
            .transitions
            .iter()
            .map(|transition| transition.reward)
            .collect::<Vec<_>>();
        let values = trajectory
            .transitions
            .iter()
            .map(|transition| transition.choice.old_value)
            .collect::<Vec<_>>();
        let terminals = trajectory
            .transitions
            .iter()
            .map(|transition| transition.terminal)
            .collect::<Vec<_>>();
        let (advantages, targets) = credit::gae(&rewards, &values, &terminals, 0., gamma, lambda)?;
        credited.extend(advantages.into_iter().zip(targets).enumerate().map(
            |(transition, (advantage, target))| Credit {
                episode,
                transition,
                advantage,
                target,
            },
        ));
    }
    Ok(credited)
}

pub fn ppo_samples<'a>(
    collection: &'a Collection,
    credit: &'a [Credit],
) -> Result<Vec<super::ppo::Sample<'a>>> {
    ensure!(
        credit.len() == collection.turns,
        "aic-learning.constructive-credit: transition count mismatch"
    );
    credit
        .iter()
        .map(|credit| {
            let transition = collection
                .episodes
                .get(credit.episode)
                .and_then(|episode| episode.transitions.get(credit.transition))
                .ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.constructive-credit: invalid transition")
                })?;
            Ok(super::ppo::Sample {
                observation: &transition.observation,
                choice: &transition.choice,
                advantage: credit.advantage,
                target: credit.target,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ConstructiveModelConfig, UpdateConfig};
    use aic_torch::tch::Device;

    #[test]
    fn batched_collection_terminates_and_preserves_credit_identity() {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let model = ConstructiveModelConfig {
            schema: ConstructiveModelConfig::SCHEMA.into(),
            width: 16,
            message_rounds: 1,
        };
        let update = UpdateConfig {
            adam_backend: "foreach".into(),
            learning_rate: 0.0003,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            clip_epsilon: 0.2,
            entropy_coefficient: 0.01,
            imitation_coefficient: 0.,
            gradient_clip: 0.5,
        };
        let (_, mut learner) = Learner::cold(&model, &update, 91, Device::Cpu).unwrap();
        let mut sessions = vec![
            super::super::test_support::session(6),
            super::super::test_support::session(6),
        ];
        let collection = collect(
            &mut sessions,
            &super::super::test_support::domain(),
            &RewardConfig {
                progress_scale: 1.,
                full_validation_bonus: 5.,
                rejected_penalty: -0.05,
                unfinished_penalty: -1.,
            },
            &mut learner,
            Sampling::Training,
        )
        .unwrap();
        assert_eq!(collection.episodes.len(), 2);
        assert!(collection.turns > 0 && collection.turns <= 12);
        assert_eq!(
            collection.accepted,
            collection
                .episodes
                .iter()
                .flat_map(|episode| &episode.transitions)
                .filter(|transition| transition.accepted)
                .count()
        );
        let credit = gae(&collection, 0.99, 0.95).unwrap();
        assert_eq!(credit.len(), collection.turns);
        let samples = ppo_samples(&collection, &credit).unwrap();
        assert_eq!(samples.len(), collection.turns);
        assert!(samples.iter().all(|sample| {
            sample.advantage.is_finite()
                && sample.target.is_finite()
                && sample.choice.command.revision == sample.observation.domain.revision()
        }));
    }
}
