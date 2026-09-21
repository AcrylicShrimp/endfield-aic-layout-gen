//! Attention actor/critic for the direct constructive command domain.
//!
//! The graph encoder retains the established small message-passing shape. Exact
//! occupied-cell tokens stay separate, and a query made from the persistent
//! constructive focus cross-attends to graph and cell memory before each
//! conditional action factor is scored.

use std::collections::{BTreeMap, BTreeSet};

use aic_pilot_native::constructive::learning::{
    CHOICE_FEATURES, Choice, ENTITY_FEATURES, FOCUS_FEATURES, GLOBAL_FEATURES, Input,
    RELATION_FEATURES, SPATIAL_FEATURES,
};
use aic_torch::tch::{Device, Kind, Tensor};
use anyhow::{Result, ensure};

use crate::{
    input::{Tensors, tensor},
    schema::ConstructiveModelConfig,
};

pub mod policy;
pub mod ppo;
pub mod rollout;
pub mod route;
pub mod run;
pub mod search;
#[cfg(test)]
mod test_support;

pub const CHECKPOINT_SCHEMA: &str = "aic-constructive-attention-checkpoint-v2";

pub struct FactorDecision<'a> {
    pub observation: usize,
    pub choices: &'a [Choice],
}

pub struct InputBatch {
    values: Tensors,
    graphs: i64,
    entity_offsets: Vec<i64>,
}

pub struct ChoiceBatch {
    values: Tensors,
    factors: i64,
    choice_offsets: Vec<i64>,
}

impl InputBatch {
    pub fn from_inputs(rows: &[&Input], device: Device) -> Result<Self> {
        ensure!(
            !rows.is_empty() && rows.len() <= 262_144,
            "aic-learning.constructive-batch: invalid graph count"
        );
        let mut values = Tensors::new();
        let mut entities = Vec::new();
        let mut relations = Vec::new();
        let mut relation_index = [Vec::new(), Vec::new()];
        let mut entity_graph = Vec::new();
        let mut entity_counts = Vec::new();
        let mut spatial = Vec::new();
        let mut spatial_owners = Vec::new();
        let mut spatial_graph = Vec::new();
        let mut spatial_counts = Vec::new();
        let mut globals = Vec::new();
        let mut focuses = Vec::new();
        let mut focus_entities = Vec::new();
        let mut entity_offset = 0i64;
        let mut entity_offsets = Vec::with_capacity(rows.len() + 1);
        entity_offsets.push(0);
        for (graph, input) in rows.iter().enumerate() {
            ensure!(
                input.schema == aic_pilot_native::constructive::learning::SCHEMA
                    && !input.entity_features.is_empty()
                    && input.entity_features.len() <= 69_632
                    && input.relation_index.len() == input.relation_features.len()
                    && input.spatial_features.len() == input.spatial_owners.len(),
                "aic-learning.constructive-batch: malformed graph {graph}"
            );
            entities.extend(input.entity_features.iter().flatten().copied());
            relations.extend(input.relation_features.iter().flatten().copied());
            for &[source, target] in &input.relation_index {
                ensure!(
                    source >= 0
                        && target >= 0
                        && source < input.entity_features.len() as i64
                        && target < input.entity_features.len() as i64,
                    "aic-learning.constructive-batch: invalid relation in graph {graph}"
                );
                relation_index[0].push(source + entity_offset);
                relation_index[1].push(target + entity_offset);
            }
            entity_graph.extend(std::iter::repeat_n(
                graph as i64,
                input.entity_features.len(),
            ));
            entity_counts.push(input.entity_features.len() as i64);
            spatial.extend(input.spatial_features.iter().flatten().copied());
            for &owner in &input.spatial_owners {
                ensure!(
                    owner >= 0 && owner < input.entity_features.len() as i64,
                    "aic-learning.constructive-batch: invalid spatial owner in graph {graph}"
                );
                spatial_owners.push(owner + entity_offset);
            }
            spatial_graph.extend(std::iter::repeat_n(
                graph as i64,
                input.spatial_features.len(),
            ));
            spatial_counts.push(input.spatial_features.len() as i64);
            globals.extend_from_slice(&input.global_features);
            focuses.extend_from_slice(&input.focus_features);
            ensure!(
                input.focus_entity >= -1 && input.focus_entity < input.entity_features.len() as i64,
                "aic-learning.constructive-batch: invalid focus in graph {graph}"
            );
            focus_entities.push(if input.focus_entity < 0 {
                -1
            } else {
                input.focus_entity + entity_offset
            });
            entity_offset += input.entity_features.len() as i64;
            entity_offsets.push(entity_offset);
        }
        let graphs = rows.len() as i64;
        insert_matrix(
            &mut values,
            "entity_features",
            &entities,
            ENTITY_FEATURES as i64,
            device,
        );
        insert_matrix(
            &mut values,
            "relation_features",
            &relations,
            RELATION_FEATURES as i64,
            device,
        );
        insert_matrix(
            &mut values,
            "spatial_features",
            &spatial,
            SPATIAL_FEATURES as i64,
            device,
        );
        insert_matrix(
            &mut values,
            "global_features",
            &globals,
            GLOBAL_FEATURES as i64,
            device,
        );
        insert_matrix(
            &mut values,
            "focus_features",
            &focuses,
            FOCUS_FEATURES as i64,
            device,
        );
        values.insert(
            "relation_index".into(),
            Tensor::from_slice(&relation_index.into_iter().flatten().collect::<Vec<_>>())
                .view([2, -1])
                .to_device(device),
        );
        for (name, data) in [
            ("entity_graph", entity_graph),
            ("entity_counts", entity_counts),
            ("spatial_owners", spatial_owners),
            ("spatial_graph", spatial_graph),
            ("spatial_counts", spatial_counts),
            ("focus_entity", focus_entities),
        ] {
            values.insert(name.into(), Tensor::from_slice(&data).to_device(device));
        }
        Ok(Self {
            values,
            graphs,
            entity_offsets,
        })
    }

    pub fn choices(&self, rows: &[FactorDecision<'_>], device: Device) -> Result<ChoiceBatch> {
        ensure!(
            !rows.is_empty() && rows.len() <= 2_097_152,
            "aic-learning.constructive-batch: invalid factor count"
        );
        let mut values = Tensors::new();
        let mut choices = Vec::new();
        let mut choice_references = Vec::new();
        let mut factor_observation = Vec::with_capacity(rows.len());
        let mut choice_factor = Vec::new();
        let mut choice_offset = 0i64;
        let mut choice_offsets = Vec::with_capacity(rows.len() + 1);
        choice_offsets.push(0);
        for (factor, row) in rows.iter().enumerate() {
            ensure!(
                row.observation < self.graphs as usize
                    && !row.choices.is_empty()
                    && row.choices.len() <= 1_000_000,
                "aic-learning.constructive-batch: malformed factor {factor}"
            );
            let entity_start = self.entity_offsets[row.observation];
            let entity_count = self.entity_offsets[row.observation + 1] - entity_start;
            for choice in row.choices {
                ensure!(
                    choice.references.iter().all(|&reference| {
                        reference == -1 || (reference >= 0 && reference < entity_count)
                    }),
                    "aic-learning.constructive-batch: invalid choice reference in factor {factor}"
                );
                choices.extend_from_slice(&choice.features);
                choice_references.extend(choice.references.map(|reference| {
                    if reference < 0 {
                        -1
                    } else {
                        reference + entity_start
                    }
                }));
                choice_factor.push(factor as i64);
            }
            factor_observation.push(row.observation as i64);
            choice_offset += row.choices.len() as i64;
            choice_offsets.push(choice_offset);
        }
        insert_matrix(
            &mut values,
            "choice_features",
            &choices,
            CHOICE_FEATURES as i64,
            device,
        );
        values.insert(
            "choice_references".into(),
            Tensor::from_slice(&choice_references)
                .view([-1, 2])
                .to_device(device),
        );
        values.insert(
            "factor_observation".into(),
            Tensor::from_slice(&factor_observation).to_device(device),
        );
        values.insert(
            "choice_factor".into(),
            Tensor::from_slice(&choice_factor).to_device(device),
        );
        Ok(ChoiceBatch {
            values,
            factors: rows.len() as i64,
            choice_offsets,
        })
    }

    pub fn graphs(&self) -> i64 {
        self.graphs
    }
}

impl ChoiceBatch {
    pub fn factors(&self) -> i64 {
        self.factors
    }

    pub fn choice_range(&self, factor: usize) -> std::ops::Range<i64> {
        self.choice_offsets[factor]..self.choice_offsets[factor + 1]
    }

    pub fn choice_offsets(&self) -> &[i64] {
        &self.choice_offsets
    }
}

fn insert_matrix(values: &mut Tensors, name: &str, data: &[f32], width: i64, device: Device) {
    let rows = data.len() as i64 / width;
    values.insert(
        name.into(),
        Tensor::from_slice(data)
            .view([rows, width])
            .to_device(device),
    );
}

pub struct Model {
    pub config: ConstructiveModelConfig,
    pub parameters: Vec<(String, Tensor)>,
    weights: Tensors,
}

impl Model {
    pub fn initial_state(
        config: &ConstructiveModelConfig,
        seed: u64,
    ) -> Result<(Vec<String>, Tensors)> {
        config.validate()?;
        ensure!(
            seed <= i64::MAX as u64,
            "aic-learning.constructive-initialization: seed exceeds LibTorch range"
        );
        aic_torch::tch::manual_seed(seed as i64);
        let mut order = Vec::new();
        let mut values = Tensors::new();
        for (name, input, output) in layers(config) {
            let bound = 1. / (input as f64).sqrt();
            for (suffix, shape) in [("weight", vec![output, input]), ("bias", vec![output])] {
                let parameter = format!("{name}.{suffix}");
                values.insert(
                    format!("constructive.model.{parameter}"),
                    Tensor::empty(&shape, (Kind::Float, Device::Cpu)).uniform_(-bound, bound),
                );
                order.push(parameter);
            }
        }
        Ok((order, values))
    }

    pub fn load(
        config: &ConstructiveModelConfig,
        order: &[String],
        values: &Tensors,
        device: Device,
    ) -> Result<Self> {
        config.validate()?;
        let mut expected = BTreeSet::new();
        for (name, input, output) in layers(config) {
            for (suffix, shape) in [("weight", vec![output, input]), ("bias", vec![output])] {
                let parameter = format!("{name}.{suffix}");
                let value = tensor(values, &format!("constructive.model.{parameter}"))?;
                ensure!(
                    value.kind() == Kind::Float
                        && value.size() == shape
                        && bool::try_from(value.isfinite().all())?,
                    "aic-learning.constructive-parameter: {parameter}"
                );
                expected.insert(parameter);
            }
        }
        ensure!(
            order.len() == expected.len()
                && order.iter().cloned().collect::<BTreeSet<_>>() == expected,
            "aic-learning.constructive-model-order: missing, duplicate or unknown parameter"
        );
        let parameters = order
            .iter()
            .map(|name| {
                Ok((
                    name.clone(),
                    tensor(values, &format!("constructive.model.{name}"))?
                        .to_device(device)
                        .copy()
                        .set_requires_grad(true),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let weights = parameters
            .iter()
            .map(|(name, value)| (name.clone(), value.shallow_clone()))
            .collect::<BTreeMap<_, _>>();
        Ok(Self {
            config: config.clone(),
            parameters,
            weights,
        })
    }

    fn linear(&self, name: &str, value: &Tensor) -> Tensor {
        value.linear(
            &self.weights[&format!("{name}.weight")],
            Some(&self.weights[&format!("{name}.bias")]),
        )
    }

    pub fn encode(&self, batch: &InputBatch) -> Result<Encoded> {
        let b = &batch.values;
        let mut entities = self.linear("entities.0", &b["entity_features"]).tanh();
        let relations = self.linear("relations.0", &b["relation_features"]).tanh();
        let source = b["relation_index"].get(0);
        let target = b["relation_index"].get(1);
        for round in 0..self.config.message_rounds {
            let messages = self
                .linear(
                    &format!("messages.{round}"),
                    &Tensor::cat(
                        &[
                            entities.index_select(0, &source),
                            entities.index_select(0, &target),
                            relations.shallow_clone(),
                        ],
                        -1,
                    ),
                )
                .tanh();
            let incoming = Tensor::zeros_like(&entities).index_add_(0, &target, &messages);
            entities = self
                .linear(
                    &format!("updates.{round}"),
                    &Tensor::cat(&[entities, incoming], -1),
                )
                .tanh();
        }
        let entity_pooled = pool(
            &entities,
            &b["entity_graph"],
            &b["entity_counts"],
            batch.graphs,
        );
        let spatial = self
            .linear(
                "spatial.0",
                &Tensor::cat(
                    &[
                        b["spatial_features"].shallow_clone(),
                        entities.index_select(0, &b["spatial_owners"]),
                    ],
                    -1,
                ),
            )
            .tanh();
        let spatial_pooled = pool(
            &spatial,
            &b["spatial_graph"],
            &b["spatial_counts"],
            batch.graphs,
        );
        let focus_entity = masked_rows(&entities, &b["focus_entity"]);
        let query = self
            .linear(
                "focus.0",
                &Tensor::cat(
                    &[
                        b["focus_features"].shallow_clone(),
                        focus_entity,
                        b["global_features"].shallow_clone(),
                    ],
                    -1,
                ),
            )
            .tanh();
        let memory = Tensor::cat(&[entities.shallow_clone(), spatial], 0);
        let memory_graph = Tensor::cat(&[&b["entity_graph"], &b["spatial_graph"]], 0);
        let keys = self.linear("attention.key", &memory);
        let attention_logits = (&keys * query.index_select(0, &memory_graph)).sum_dim_intlist(
            [-1].as_slice(),
            false,
            Kind::Float,
        ) / (self.config.width as f64).sqrt();
        let attention = segmented_log_softmax(&attention_logits, &memory_graph, batch.graphs).exp();
        let attended = Tensor::zeros(
            [batch.graphs, self.config.width],
            (Kind::Float, memory.device()),
        )
        .index_add_(
            0,
            &memory_graph,
            &(self.linear("attention.value", &memory) * attention.unsqueeze(-1)),
        );
        let context = self
            .linear(
                "context.0",
                &Tensor::cat(
                    &[
                        entity_pooled,
                        spatial_pooled,
                        attended,
                        b["global_features"].shallow_clone(),
                    ],
                    -1,
                ),
            )
            .tanh();
        let values = self
            .linear("critic.2", &self.linear("critic.0", &context).tanh())
            .squeeze_dim(-1);
        ensure!(
            bool::try_from(values.isfinite().all())?,
            "aic-learning.constructive-forward: nonfinite encoding"
        );
        Ok(Encoded {
            entities,
            context,
            values,
            attention,
        })
    }

    pub fn score(&self, encoded: &Encoded, batch: &ChoiceBatch) -> Result<Prediction> {
        let b = &batch.values;
        let choice_observation = b["factor_observation"].index_select(0, &b["choice_factor"]);
        let references = masked_references(
            &encoded.entities,
            &b["choice_references"],
            &encoded.context.index_select(0, &choice_observation),
        );
        let actor_input = Tensor::cat(
            &[
                references.flatten(1, -1),
                encoded.context.index_select(0, &choice_observation),
                b["choice_features"].shallow_clone(),
            ],
            -1,
        );
        let logits = self
            .linear("actor.2", &self.linear("actor.0", &actor_input).tanh())
            .squeeze_dim(-1);
        let logp = segmented_log_softmax(&logits, &b["choice_factor"], batch.factors);
        let safe_logp = logp.masked_fill(&logp.isneginf(), 0.);
        let entropy = Tensor::zeros([batch.factors], (Kind::Float, logits.device())).index_add_(
            0,
            &b["choice_factor"],
            &(-logp.exp() * safe_logp),
        );
        ensure!(
            bool::try_from(logp.isfinite().all().logical_and(&entropy.isfinite().all()))?,
            "aic-learning.constructive-forward: nonfinite choice scores"
        );
        Ok(Prediction { logp, entropy })
    }
}

fn layers(config: &ConstructiveModelConfig) -> Vec<(String, i64, i64)> {
    let w = config.width;
    let mut layers = vec![
        ("entities.0".into(), ENTITY_FEATURES as i64, w),
        ("relations.0".into(), RELATION_FEATURES as i64, w),
        ("spatial.0".into(), SPATIAL_FEATURES as i64 + w, w),
        (
            "focus.0".into(),
            FOCUS_FEATURES as i64 + w + GLOBAL_FEATURES as i64,
            w,
        ),
        ("attention.key".into(), w, w),
        ("attention.value".into(), w, w),
        ("context.0".into(), w * 3 + GLOBAL_FEATURES as i64, w),
        ("actor.0".into(), w * 3 + CHOICE_FEATURES as i64, w),
        ("actor.2".into(), w, 1),
        ("critic.0".into(), w, w),
        ("critic.2".into(), w, 1),
    ];
    for round in 0..config.message_rounds {
        layers.push((format!("messages.{round}"), w * 3, w));
        layers.push((format!("updates.{round}"), w * 2, w));
    }
    layers
}

fn pool(values: &Tensor, graph: &Tensor, counts: &Tensor, graphs: i64) -> Tensor {
    Tensor::zeros([graphs, values.size()[1]], (values.kind(), values.device()))
        .index_add_(0, graph, values)
        / counts.clamp_min(1).unsqueeze(-1)
}

fn masked_rows(values: &Tensor, references: &Tensor) -> Tensor {
    let valid = references.ge(0);
    let placeholders = Tensor::zeros_like(references);
    values
        .index_select(0, &references.where_self(&valid, &placeholders))
        .where_self(&valid.unsqueeze(-1), &Tensor::zeros_like(values).get(0))
}

fn masked_references(values: &Tensor, references: &Tensor, fallback: &Tensor) -> Tensor {
    let valid = references.ge(0);
    let placeholders = Tensor::zeros_like(references);
    values
        .index(&[Some(&references.where_self(&valid, &placeholders))])
        .where_self(&valid.unsqueeze(-1), &fallback.unsqueeze(1))
}

fn segmented_log_softmax(logits: &Tensor, graph: &Tensor, count: i64) -> Tensor {
    let maximum = Tensor::full([count], f64::NEG_INFINITY, (logits.kind(), logits.device()))
        .internal_scatter_reduce(0, graph, logits, "amax", true);
    let shifted = logits - maximum.index_select(0, graph);
    let denominator = Tensor::zeros([count], (Kind::Double, logits.device())).index_add_(
        0,
        graph,
        &shifted.exp().to_kind(Kind::Double),
    );
    shifted
        - denominator
            .log()
            .to_kind(logits.kind())
            .index_select(0, graph)
}

pub struct Prediction {
    /// Conditional log probabilities in packed choice order.
    pub logp: Tensor,
    pub entropy: Tensor,
}

pub struct Encoded {
    entities: Tensor,
    context: Tensor,
    pub values: Tensor,
    /// Focus-to-memory attention weights, exposed for bounded diagnostics.
    pub attention: Tensor,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(focus_entity: i64) -> Input {
        Input {
            schema: aic_pilot_native::constructive::learning::SCHEMA,
            entity_features: vec![[0.; ENTITY_FEATURES], [1.; ENTITY_FEATURES]],
            relation_index: vec![[0, 1]],
            relation_features: vec![[0.5; RELATION_FEATURES]],
            spatial_features: vec![[0.25; SPATIAL_FEATURES]],
            spatial_owners: vec![1],
            global_features: [0.1; GLOBAL_FEATURES],
            focus_features: [0.2; FOCUS_FEATURES],
            focus_entity,
        }
    }

    #[test]
    fn encoded_observations_score_repeated_conditional_factors_and_values() {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let config = ConstructiveModelConfig {
            schema: ConstructiveModelConfig::SCHEMA.into(),
            width: 16,
            message_rounds: 2,
        };
        let (order, state) = Model::initial_state(&config, 41).unwrap();
        let model = Model::load(&config, &order, &state, Device::Cpu).unwrap();
        let choices = vec![
            Choice {
                features: [0.; CHOICE_FEATURES],
                references: [0, -1],
            },
            Choice {
                features: [1.; CHOICE_FEATURES],
                references: [1, 0],
            },
        ];
        let first = input(0);
        let second = input(1);
        let inputs = InputBatch::from_inputs(&[&first, &second], Device::Cpu).unwrap();
        let encoded = model.encode(&inputs).unwrap();
        let batch = inputs
            .choices(
                &[
                    FactorDecision {
                        observation: 0,
                        choices: &choices,
                    },
                    FactorDecision {
                        observation: 0,
                        choices: &choices,
                    },
                    FactorDecision {
                        observation: 1,
                        choices: &choices,
                    },
                ],
                Device::Cpu,
            )
            .unwrap();
        let prediction = model.score(&encoded, &batch).unwrap();
        assert_eq!(prediction.logp.size(), [6]);
        assert_eq!(prediction.entropy.size(), [3]);
        let logp = Vec::<f32>::try_from(prediction.logp).unwrap();
        assert_eq!(&logp[0..2], &logp[2..4]);
        assert_eq!(encoded.values.size(), [2]);
        assert_eq!(encoded.attention.size(), [6]);
        let attention = Vec::<f32>::try_from(encoded.attention).unwrap();
        assert!((attention[0] + attention[1] + attention[4] - 1.).abs() < 1e-5);
        assert!((attention[2] + attention[3] + attention[5] - 1.).abs() < 1e-5);
        let values = Vec::<f32>::try_from(encoded.values).unwrap();
        assert_ne!(values[0], values[1]);
    }
}
