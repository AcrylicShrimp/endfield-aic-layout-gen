//! Causal complete-episode replay with hydrated sparse inputs and utility retention.
use super::frozen::{self, Frozen};
use crate::{collection::rollout::Rollout, input::ReplayBatch};
use aic_torch::{Generator, tch::Tensor};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub type Reference = [u64; 2];

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    pub ema_alpha: f64,
    pub minimum_draws: u64,
    pub minimum_updates: u64,
    pub stale_updates: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub sampler: String,
    pub max_sequences: usize,
    pub max_labels: usize,
    pub max_bytes: u64,
    pub retention: Retention,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.sampler.as_str(), "uniform" | "balanced_available")
                && self.max_sequences > 0
                && self.max_labels > 0
                && self.max_bytes > 0
                && self.max_sequences <= 1_000_000
                && self.max_labels <= 16_777_216
                && (0. ..=1.).contains(&self.retention.ema_alpha)
                && self.retention.ema_alpha > 0.
                && self.retention.minimum_draws > 0
                && self.retention.minimum_updates > 0
                && self.retention.stale_updates > 0,
            "aic-learning.replay-config: invalid replay bounds/policy"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Label {
    pub reference: Reference,
    pub stage: String,
    pub returned: f64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Utility {
    pub ema: f64,
    pub draws: u64,
    pub updates: u64,
    pub last_update: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub admitted_update: u64,
    pub stream: usize,
    pub episode: u64,
    pub stratum: String,
    pub product: String,
    pub labels: Vec<Label>,
    pub uses: u64,
    pub occurrences: u64,
    pub utility: Option<Utility>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PendingLabel {
    reference: Reference,
    action_index: usize,
    stage: String,
    state: Value,
    action: Value,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Active {
    pub episode: u64,
    rewards: Vec<f64>,
    segment: Vec<PendingLabel>,
    pending: Vec<Vec<PendingLabel>>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub entries: BTreeMap<String, Entry>,
    pub active: BTreeMap<usize, Active>,
    pub positive_improvements: u64,
    pub admissions: u64,
    pub duplicates: u64,
    pub evictions: u64,
    pub selected_uses: u64,
}
#[derive(Default)]
pub struct Buffer {
    pub state: State,
    pub inputs: BTreeMap<Reference, Arc<Frozen>>,
    pub labels: usize,
    pub bytes: u64,
}
pub struct Draw {
    pub id: String,
    pub rows: Vec<(Arc<Frozen>, f64, String)>,
}
pub struct Prepared {
    pub batch: ReplayBatch,
    pub groups: Vec<(String, f64, bool)>,
}
pub struct Sampling {
    ids: Vec<String>,
    strata: BTreeMap<String, Vec<String>>,
}
impl Sampling {
    pub fn new(buffer: &Buffer, ordinal: u64) -> Result<Self> {
        let mut strata: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (id, entry) in &buffer.state.entries {
            ensure!(
                entry.admitted_update < ordinal,
                "aic-learning.replay-causality: current update entered replay"
            );
            strata
                .entry(entry.stratum.clone())
                .or_default()
                .push(id.clone());
        }
        ensure!(
            strata.len() <= 2,
            "aic-learning.replay-stratum: expected at most two strata"
        );
        Ok(Self {
            ids: buffer.state.entries.keys().cloned().collect(),
            strata,
        })
    }
}
pub fn prepare(draws: &[Draw]) -> Result<Option<Prepared>> {
    if draws.is_empty() {
        return Ok(None);
    }
    let (mut rows, mut returns, mut weights, mut groups) = (vec![], vec![], vec![], vec![]);
    for draw in draws {
        let mut stages = BTreeMap::new();
        for (_, _, stage) in &draw.rows {
            *stages.entry(stage).or_insert(0usize) += 1;
        }
        ensure!(
            !stages.is_empty(),
            "aic-learning.replay-empty: empty sequence {}",
            draw.id
        );
        for (i, (input, returned, stage)) in draw.rows.iter().enumerate() {
            let weight = 1. / (stages.len() * stages[stage]) as f64;
            rows.push(input.as_ref());
            returns.push(*returned as f32);
            weights.push((weight / draws.len() as f64) as f32);
            groups.push((draw.id.clone(), weight, i == 0));
        }
    }
    Ok(Some(Prepared {
        batch: ReplayBatch {
            packed: frozen::pack(&rows)?,
            returns: Tensor::from_slice(&returns),
            weights: Tensor::from_slice(&weights),
        },
        groups,
    }))
}

impl Buffer {
    pub fn draw(
        &mut self,
        sampling: &Sampling,
        config: &Config,
        rng: &mut Generator,
        order: &mut Generator,
    ) -> Result<Vec<Draw>> {
        if sampling.ids.is_empty() {
            return Ok(vec![]);
        }
        let mut ids = vec![];
        if config.sampler == "balanced_available" && sampling.strata.len() == 2 {
            for values in sampling.strata.values() {
                for i in Vec::<i64>::try_from(rng.integers(values.len() as i64, 4)?)? {
                    ids.push(values[i as usize].to_owned());
                }
            }
            ids = Vec::<i64>::try_from(order.permutation(8)?)?
                .into_iter()
                .map(|i| ids[i as usize].clone())
                .collect();
        } else {
            let values = &sampling.ids;
            for i in Vec::<i64>::try_from(rng.integers(values.len() as i64, 8)?)? {
                ids.push(values[i as usize].clone());
            }
        }
        ids.into_iter()
            .map(|id| {
                let entry = self.state.entries.get_mut(&id).unwrap();
                entry.uses += 1;
                self.state.selected_uses += 1;
                let rows = entry
                    .labels
                    .iter()
                    .map(|label| {
                        let input = self.inputs.get(&label.reference).ok_or_else(|| {
                            anyhow::anyhow!(
                                "aic-learning.replay-input: missing {:?}",
                                label.reference
                            )
                        })?;
                        Ok((input.clone(), label.returned, label.stage.clone()))
                    })
                    .collect::<Result<_>>()?;
                Ok(Draw { id, rows })
            })
            .collect()
    }
    pub fn observe(
        &mut self,
        rows: BTreeMap<String, (u64, f64)>,
        ordinal: u64,
        config: &Config,
    ) -> Result<()> {
        for (id, (draws, contribution)) in rows {
            let mean = contribution / draws as f64;
            ensure!(
                draws > 0 && mean.is_finite() && mean >= 0.,
                "aic-learning.replay-utility: invalid contribution for {id}"
            );
            let entry = self
                .state
                .entries
                .get_mut(&id)
                .ok_or_else(|| anyhow::anyhow!("aic-learning.replay-utility: unknown {id}"))?;
            let old = entry.utility.as_ref();
            ensure!(
                old.is_none_or(|u| u.last_update < ordinal),
                "aic-learning.replay-utility: repeated update for {id}"
            );
            entry.utility = Some(Utility {
                ema: old.map_or(mean, |u| {
                    config.retention.ema_alpha * mean + (1. - config.retention.ema_alpha) * u.ema
                }),
                draws: draws + old.map_or(0, |u| u.draws),
                updates: 1 + old.map_or(0, |u| u.updates),
                last_update: ordinal,
            });
        }
        Ok(())
    }
    pub fn ingest(
        &mut self,
        rollout: &Rollout,
        frozen: &[Frozen],
        ordinal: u64,
        episode: u64,
        products: &[String],
        strata: &BTreeMap<String, Vec<String>>,
        gamma: f64,
        config: &Config,
    ) -> Result<Vec<Value>> {
        let streams = products.len();
        ensure!(
            rollout.samples.len() == frozen.len() && rollout.evidence.len() == streams,
            "aic-learning.replay-coverage: incomplete rollout/evidence"
        );
        let mut events = vec![];
        let mut replacement = None;
        for (index, sample) in rollout.samples.iter().enumerate() {
            let stream = sample.stream;
            let evidence = &rollout.evidence[stream]["rows"][index / streams];
            let active = self.state.active.entry(stream).or_insert_with(|| Active {
                episode,
                ..Active::default()
            });
            ensure!(
                active.episode == episode
                    && sample.observation.attempts as usize == active.rewards.len(),
                "aic-learning.replay-continuity: stream {stream} episode {episode} action {}",
                sample.observation.attempts
            );
            active.rewards.push(sample.reward);
            if evidence["observation"]["facilities"].is_array() {
                let action = sample.outcome["action"].clone();
                let stage = action["kind"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("aic-learning.replay-action: missing kind"))?
                    .to_owned();
                let reference = [ordinal, index as u64];
                let input = Arc::new(frozen[index].compact());
                self.bytes += input.bytes();
                self.inputs.insert(reference, input);
                let mut state = evidence["observation"].clone();
                state.as_object_mut().unwrap().remove("revision");
                active.segment.push(PendingLabel {
                    reference,
                    action_index: active.rewards.len() - 1,
                    stage,
                    state,
                    action,
                });
            }
            if sample.reward > 0. {
                ensure!(
                    !evidence["validated_witness"].is_null()
                        && !active.segment.is_empty()
                        && sample.outcome["outcome"]["status"] == "improved",
                    "aic-learning.replay-witness: improvement lacks validated edits"
                );
                active.pending.push(std::mem::take(&mut active.segment));
                self.state.positive_improvements += 1;
            }
            if !sample.terminal {
                continue;
            }
            let active = self.state.active.remove(&stream).unwrap();
            let mut returns = vec![0.; active.rewards.len()];
            let mut value = 0.;
            for (i, reward) in active.rewards.iter().enumerate().rev() {
                value = reward + gamma * value;
                returns[i] = value;
            }
            let product = &products[stream];
            let names: Vec<_> = strata
                .iter()
                .filter(|(_, ps)| ps.contains(product))
                .map(|(n, _)| n)
                .collect();
            ensure!(
                names.len() == 1,
                "aic-learning.replay-stratum: product {product} must have one stratum"
            );
            for segment in active.pending {
                let semantic: Vec<_> = segment
                    .iter()
                    .map(|r| {
                        json!({"state":r.state,"action":r.action,
                    "return_to_terminal":(returns[r.action_index]*1e12).round()/1e12})
                    })
                    .collect();
                let id: String = Sha256::digest(serde_json::to_vec(&semantic)?)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                if let Some(entry) = self.state.entries.get_mut(&id) {
                    entry.occurrences += 1;
                    self.state.duplicates += 1;
                    continue;
                }
                let labels: Vec<_> = segment
                    .into_iter()
                    .map(|r| Label {
                        reference: r.reference,
                        stage: r.stage,
                        returned: returns[r.action_index],
                    })
                    .collect();
                ensure!(
                    labels.len() <= config.max_labels,
                    "aic-learning.replay-capacity: one sequence exceeds label bound"
                );
                while self.state.entries.len() >= config.max_sequences
                    || self.labels + labels.len() > config.max_labels
                {
                    let index = replacement.get_or_insert_with(|| {
                        super::retention::Index::new(
                            &self.state.entries,
                            ordinal,
                            &config.retention,
                        )
                    });
                    let victim = index.choose(names[0])?;
                    let removed = self.state.entries.remove(&victim).unwrap();
                    self.labels -= removed.labels.len();
                    self.state.evictions += 1;
                    events.push(json!({"kind":"evicted","id":victim}));
                }
                self.labels += labels.len();
                self.state.admissions += 1;
                events.push(json!({"kind":"admitted","id":id,"labels":labels.len()}));
                self.state.entries.insert(
                    id.clone(),
                    Entry {
                        id,
                        admitted_update: ordinal,
                        stream,
                        episode,
                        stratum: names[0].clone(),
                        product: product.clone(),
                        labels,
                        uses: 0,
                        occurrences: 1,
                        utility: None,
                    },
                );
                if let Some(index) = &mut replacement {
                    let id = events.last().unwrap()["id"].as_str().unwrap();
                    index.add(&self.state.entries[id]);
                }
            }
        }
        self.prune_inputs();
        ensure!(
            self.bytes <= config.max_bytes,
            "aic-learning.replay-memory: retained inputs exceed byte bound"
        );
        Ok(events)
    }
    pub fn references(&self) -> BTreeSet<Reference> {
        self.state
            .entries
            .values()
            .flat_map(|e| e.labels.iter().map(|l| l.reference))
            .chain(self.state.active.values().flat_map(|a| {
                a.segment
                    .iter()
                    .chain(a.pending.iter().flatten())
                    .map(|l| l.reference)
            }))
            .collect()
    }
    pub fn prune_inputs(&mut self) {
        let keep = self.references();
        self.inputs.retain(|r, _| keep.contains(r));
        self.bytes = self.inputs.values().map(|i| i.bytes()).sum();
    }
    pub fn summary(&self) -> Value {
        json!({"sequences":self.state.entries.len(),"labels":self.labels,"bytes":self.bytes,
            "active_streams":self.state.active.len(),"admissions":self.state.admissions,"duplicates":self.state.duplicates,
            "evictions":self.state.evictions,"selected_uses":self.state.selected_uses,"positive_improvements":self.state.positive_improvements})
    }
}
