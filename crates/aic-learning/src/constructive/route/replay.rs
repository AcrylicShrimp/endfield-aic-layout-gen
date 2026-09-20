//! Bounded successful route-option replay for the self-roll-in experiment.

use std::{cmp::Ordering, collections::BTreeMap};

use aic_pilot_native::constructive::learning::Observation;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::super::{policy, ppo::ReplaySample, rollout::Episode};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub capacity_sequences: usize,
    pub capacity_labels: usize,
    pub sequences_per_epoch: usize,
    pub minimum_utility_draws: u64,
    pub new_entry_grace_updates: u64,
    pub stale_after_updates: u64,
    pub utility_ema_alpha: f64,
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            capacity_sequences: 0,
            capacity_labels: 0,
            sequences_per_epoch: 0,
            minimum_utility_draws: 0,
            new_entry_grace_updates: 0,
            stale_after_updates: 0,
            utility_ema_alpha: 0.25,
            seed: 0,
        }
    }
}

impl Config {
    pub fn validate(&self, imitation_coefficient: f64) -> Result<()> {
        let enabled =
            self.capacity_sequences > 0 || self.capacity_labels > 0 || self.sequences_per_epoch > 0;
        if enabled {
            ensure!(
                imitation_coefficient > 0.
                    && (1..=4096).contains(&self.capacity_sequences)
                    && (1..=262_144).contains(&self.capacity_labels)
                    && (1..=self.capacity_sequences).contains(&self.sequences_per_epoch)
                    && self.minimum_utility_draws > 0
                    && self.new_entry_grace_updates > 0
                    && self.stale_after_updates > 0
                    && self.utility_ema_alpha.is_finite()
                    && (0. ..=1.).contains(&self.utility_ema_alpha),
                "aic-learning.constructive-route-replay: invalid enabled configuration"
            );
        } else {
            ensure!(
                imitation_coefficient == 0.,
                "aic-learning.constructive-route-replay: imitation requires a replay buffer"
            );
        }
        Ok(())
    }

    fn enabled(&self) -> bool {
        self.capacity_sequences > 0
    }
}

struct Label {
    observation: Observation,
    choice: policy::Choice,
    returned: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Stratum {
    crossed: bool,
    rollin_depth: usize,
}

struct Entry {
    id: u64,
    identity: [u8; 32],
    labels: Vec<Label>,
    stratum: Stratum,
    observed_continuations: usize,
    full_session: bool,
    blocked_after: bool,
    terminal_return: f64,
    accepted_steps: usize,
    admitted_update: u64,
    last_draw_update: u64,
    draws: u64,
    utility_ema: Option<f64>,
    approximate_bytes: usize,
}

#[derive(Default, Serialize)]
pub struct AdmissionReport {
    pub candidates: usize,
    pub admitted: usize,
    pub duplicates: usize,
    pub upgraded_duplicates: usize,
    pub evicted: usize,
}

#[derive(Serialize)]
pub struct Report {
    pub sequences: usize,
    pub labels: usize,
    pub approximate_bytes: usize,
    pub aligned_sequences: usize,
    pub crossed_sequences: usize,
    pub full_session_sequences: usize,
    pub blocked_after_sequences: usize,
    pub mature_sequences: usize,
    pub total_draws: u64,
    pub sequences_by_stratum: BTreeMap<String, usize>,
    pub mean_utility_ema: Option<f64>,
    pub admissions: AdmissionReport,
}

pub struct Draw {
    indices: Vec<usize>,
}

pub struct Buffer {
    config: Config,
    entries: Vec<Entry>,
    next_id: u64,
    random: u64,
}

impl Buffer {
    pub fn new(config: Config) -> Self {
        let random = config.seed;
        Self {
            config,
            entries: Vec::new(),
            next_id: 0,
            random,
        }
    }

    pub fn draw(&mut self, update: u64) -> Draw {
        if !self.config.enabled() || self.entries.is_empty() {
            return Draw { indices: vec![] };
        }
        let mut strata = BTreeMap::<Stratum, Vec<usize>>::new();
        for (index, entry) in self.entries.iter().enumerate() {
            strata.entry(entry.stratum).or_default().push(index);
        }
        let mut selected = Vec::new();
        let target = self.config.sequences_per_epoch.min(self.entries.len());
        let mut round = 0usize;
        while selected.len() < target {
            let mut progressed = false;
            for indices in strata.values() {
                if round >= indices.len() || selected.len() == target {
                    continue;
                }
                let offset = self.random_index(indices.len());
                let index = indices[(offset + round) % indices.len()];
                if !selected.contains(&index) {
                    selected.push(index);
                    progressed = true;
                }
            }
            if !progressed {
                for index in 0..self.entries.len() {
                    if selected.len() == target {
                        break;
                    }
                    if !selected.contains(&index) {
                        selected.push(index);
                    }
                }
            }
            round += 1;
        }
        for &index in &selected {
            let entry = &mut self.entries[index];
            entry.draws += 1;
            entry.last_draw_update = update;
        }
        Draw { indices: selected }
    }

    pub fn samples<'a>(&'a self, draw: &Draw) -> Vec<ReplaySample<'a>> {
        let sequences = draw.indices.len() as f32;
        draw.indices
            .iter()
            .flat_map(|&index| {
                let entry = &self.entries[index];
                let weight = 1. / (sequences * entry.labels.len() as f32);
                entry.labels.iter().map(move |label| ReplaySample {
                    entry: entry.id,
                    observation: &label.observation,
                    choice: &label.choice,
                    returned: label.returned,
                    weight,
                })
            })
            .collect()
    }

    pub fn observe_utility(&mut self, values: &[(u64, f64)]) {
        for &(id, utility) in values {
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
                entry.utility_ema = Some(entry.utility_ema.map_or(utility, |previous| {
                    self.config.utility_ema_alpha * utility
                        + (1. - self.config.utility_ema_alpha) * previous
                }));
            }
        }
    }

    pub fn admit(
        &mut self,
        episodes: &[Episode],
        gamma: f64,
        update: u64,
    ) -> Result<AdmissionReport> {
        let mut report = AdmissionReport::default();
        if !self.config.enabled() {
            return Ok(report);
        }
        for episode in episodes.iter().filter(|episode| episode.route_succeeded) {
            report.candidates += 1;
            let candidate = self.entry(episode, gamma, update)?;
            if let Some(index) = self
                .entries
                .iter()
                .position(|entry| entry.identity == candidate.identity)
            {
                report.duplicates += 1;
                if physical_quality(&candidate, &self.entries[index]) == Ordering::Greater {
                    let old = &self.entries[index];
                    let mut replacement = candidate;
                    replacement.id = old.id;
                    replacement.draws = old.draws;
                    replacement.utility_ema = old.utility_ema;
                    replacement.last_draw_update = old.last_draw_update;
                    self.entries[index] = replacement;
                    report.upgraded_duplicates += 1;
                }
                continue;
            }
            self.entries.push(candidate);
            self.next_id += 1;
            report.admitted += 1;
        }
        while self.entries.len() > self.config.capacity_sequences
            || self.labels() > self.config.capacity_labels
        {
            let index = self.eviction_index(update);
            self.entries.swap_remove(index);
            report.evicted += 1;
        }
        Ok(report)
    }

    pub fn report(&self, admissions: AdmissionReport) -> Report {
        let utilities = self
            .entries
            .iter()
            .filter_map(|entry| entry.utility_ema)
            .collect::<Vec<_>>();
        let mut sequences_by_stratum = BTreeMap::new();
        for entry in &self.entries {
            let shape = if entry.stratum.crossed {
                "crossed"
            } else {
                "aligned"
            };
            *sequences_by_stratum
                .entry(format!("{shape}-depth-{}", entry.stratum.rollin_depth))
                .or_default() += 1;
        }
        Report {
            sequences: self.entries.len(),
            labels: self.labels(),
            approximate_bytes: self
                .entries
                .iter()
                .map(|entry| entry.approximate_bytes)
                .sum(),
            aligned_sequences: self
                .entries
                .iter()
                .filter(|entry| !entry.stratum.crossed)
                .count(),
            crossed_sequences: self
                .entries
                .iter()
                .filter(|entry| entry.stratum.crossed)
                .count(),
            full_session_sequences: self
                .entries
                .iter()
                .filter(|entry| entry.full_session)
                .count(),
            blocked_after_sequences: self
                .entries
                .iter()
                .filter(|entry| entry.blocked_after)
                .count(),
            mature_sequences: self
                .entries
                .iter()
                .filter(|entry| entry.draws >= self.config.minimum_utility_draws)
                .count(),
            total_draws: self.entries.iter().map(|entry| entry.draws).sum(),
            sequences_by_stratum,
            mean_utility_ema: (!utilities.is_empty())
                .then(|| utilities.iter().sum::<f64>() / utilities.len() as f64),
            admissions,
        }
    }

    fn entry(&self, episode: &Episode, gamma: f64, update: u64) -> Result<Entry> {
        ensure!(
            !episode.transitions.is_empty() && episode.route_observed_depth > episode.rollin_depth,
            "aic-learning.constructive-route-replay: incomplete successful option"
        );
        let mut returned = 0.;
        let mut labels = Vec::with_capacity(episode.transitions.len());
        for transition in episode.transitions.iter().rev() {
            returned = transition.reward + gamma * returned;
            ensure!(
                returned.is_finite(),
                "aic-learning.constructive-route-replay: nonfinite return"
            );
            labels.push(Label {
                observation: transition.observation.clone(),
                choice: transition.choice.clone(),
                returned: returned as f32,
            });
        }
        labels.reverse();
        let mut hash = Sha256::new();
        let mut approximate_bytes = 0usize;
        for transition in &episode.transitions {
            let encoded = serde_json::to_vec(&(
                transition.observation.input.as_ref(),
                transition.observation.domain.as_ref(),
                &transition.choice.factors,
            ))?;
            approximate_bytes += encoded.len() + std::mem::size_of::<Label>();
            hash.update((encoded.len() as u64).to_le_bytes());
            hash.update(encoded);
        }
        let identity: [u8; 32] = hash.finalize().into();
        Ok(Entry {
            id: self.next_id,
            identity,
            labels,
            stratum: Stratum {
                crossed: episode.route_crossed,
                rollin_depth: episode.rollin_depth,
            },
            observed_continuations: episode
                .route_observed_depth
                .saturating_sub(episode.rollin_depth + 1),
            full_session: episode.route_full_session,
            blocked_after: episode.route_blocked_after,
            terminal_return: returned,
            accepted_steps: episode
                .transitions
                .iter()
                .filter(|transition| transition.accepted)
                .count(),
            admitted_update: update,
            last_draw_update: update,
            draws: 0,
            utility_ema: None,
            approximate_bytes,
        })
    }

    fn labels(&self) -> usize {
        self.entries.iter().map(|entry| entry.labels.len()).sum()
    }

    fn eviction_index(&self, update: u64) -> usize {
        let mut counts = BTreeMap::<Stratum, (usize, usize)>::new();
        for entry in &self.entries {
            let count = counts.entry(entry.stratum).or_default();
            count.0 += 1;
            count.1 += entry.labels.len();
        }
        let overrepresented = counts
            .into_iter()
            .max_by_key(|(stratum, (sequences, labels))| (*sequences, *labels, *stratum))
            .expect("eviction requires a retained entry")
            .0;
        let candidates = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.stratum == overrepresented)
            .collect::<Vec<_>>();
        let unprotected = candidates
            .iter()
            .copied()
            .filter(|(_, entry)| {
                update.saturating_sub(entry.admitted_update) >= self.config.new_entry_grace_updates
                    || entry.draws >= self.config.minimum_utility_draws
            })
            .collect::<Vec<_>>();
        let pool = if unprotected.is_empty() {
            &candidates
        } else {
            &unprotected
        };
        pool.iter()
            .copied()
            .min_by(|(_, left), (_, right)| eviction_quality(left, right, update, &self.config))
            .expect("overrepresented stratum is nonempty")
            .0
    }

    fn random_index(&mut self, upper: usize) -> usize {
        self.random = self.random.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.random;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((value ^ (value >> 31)) as usize) % upper
    }
}

fn physical_quality(left: &Entry, right: &Entry) -> Ordering {
    (!left.blocked_after)
        .cmp(&(!right.blocked_after))
        .then_with(|| {
            left.observed_continuations
                .cmp(&right.observed_continuations)
        })
        .then_with(|| left.full_session.cmp(&right.full_session))
        .then_with(|| left.terminal_return.total_cmp(&right.terminal_return))
        .then_with(|| right.accepted_steps.cmp(&left.accepted_steps))
}

fn eviction_quality(left: &Entry, right: &Entry, update: u64, config: &Config) -> Ordering {
    physical_quality(left, right)
        .then_with(|| {
            let left_mature = left.draws >= config.minimum_utility_draws;
            let right_mature = right.draws >= config.minimum_utility_draws;
            left_mature.cmp(&right_mature).reverse()
        })
        .then_with(|| match (left.utility_ema, right.utility_ema) {
            (Some(left), Some(right)) => left.total_cmp(&right),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| {
            let left_stale =
                update.saturating_sub(left.last_draw_update) >= config.stale_after_updates;
            let right_stale =
                update.saturating_sub(right.last_draw_update) >= config.stale_after_updates;
            left_stale.cmp(&right_stale).reverse()
        })
        .then_with(|| left.admitted_update.cmp(&right.admitted_update))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downstream_quality_precedes_measured_utility() {
        let entry = |full_session, continuations, utility| Entry {
            id: 0,
            identity: [0; 32],
            labels: vec![],
            stratum: Stratum {
                crossed: false,
                rollin_depth: 0,
            },
            observed_continuations: continuations,
            full_session,
            blocked_after: false,
            terminal_return: 1.,
            accepted_steps: 1,
            admitted_update: 0,
            last_draw_update: 0,
            draws: 10,
            utility_ema: Some(utility),
            approximate_bytes: 0,
        };
        let continued = entry(false, 1, 0.);
        let standalone = entry(false, 0, 100.);
        assert_eq!(physical_quality(&continued, &standalone), Ordering::Greater);
        let standalone_complete = entry(true, 0, 0.);
        assert_eq!(
            physical_quality(&continued, &standalone_complete),
            Ordering::Greater
        );
        let continued_complete = entry(true, 1, 0.);
        assert_eq!(
            physical_quality(&continued_complete, &continued),
            Ordering::Greater
        );
    }
}
