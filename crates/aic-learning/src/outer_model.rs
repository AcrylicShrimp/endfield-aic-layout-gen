//! Separate policy/value model for stable-state hierarchical macro actions.

use std::collections::{BTreeMap, BTreeSet};

use aic_pilot_native::constructive::learning::Input;
use aic_torch::tch::{Device, Kind, Tensor};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    constructive::{InputBatch, Model as Encoder},
    input::{Tensors, tensor},
    schema::OuterModelConfig,
};

pub const ACTION_FEATURES: usize = 24;
pub const VALUE_HEADS: usize = 6;
pub const CHECKPOINT_SCHEMA: &str = "aic-hierarchical-outer-checkpoint-v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ParameterOrder {
    pub schema: String,
    pub encoder: Vec<String>,
    pub heads: Vec<String>,
}

pub struct Batch {
    graph: InputBatch,
    action_features: Tensor,
    action_graph: Tensor,
    offsets: Vec<i64>,
}

pub struct Prediction {
    pub logits: Tensor,
    pub logp: Tensor,
    pub values: Tensor,
}

pub struct Model {
    pub config: OuterModelConfig,
    pub encoder: Encoder,
    pub parameters: Vec<(String, Tensor)>,
    weights: Tensors,
}

impl Batch {
    pub fn from_rows(
        inputs: &[&Input],
        actions: &[&[[f32; ACTION_FEATURES]]],
        device: Device,
    ) -> Result<Self> {
        ensure!(
            !inputs.is_empty()
                && inputs.len() == actions.len()
                && inputs.len() <= 262_144
                && actions.iter().all(|rows| !rows.is_empty()),
            "aic-learning.outer-batch: invalid state or action count"
        );
        let graph = InputBatch::from_inputs(inputs, device)?;
        let mut flat = Vec::new();
        let mut action_graph = Vec::new();
        let mut offsets = Vec::with_capacity(actions.len() + 1);
        offsets.push(0);
        for (state, rows) in actions.iter().enumerate() {
            flat.extend(rows.iter().flatten().copied());
            action_graph.extend(std::iter::repeat_n(state as i64, rows.len()));
            offsets.push(action_graph.len() as i64);
        }
        let action_features = Tensor::from_slice(&flat)
            .view([-1, ACTION_FEATURES as i64])
            .to_device(device);
        let action_graph = Tensor::from_slice(&action_graph).to_device(device);
        Ok(Self {
            graph,
            action_features,
            action_graph,
            offsets,
        })
    }

    pub fn action_range(&self, state: usize) -> std::ops::Range<i64> {
        self.offsets[state]..self.offsets[state + 1]
    }

    pub fn states(&self) -> usize {
        self.graph.graphs() as usize
    }
}

impl Model {
    pub fn initial_state(
        config: &OuterModelConfig,
        seed: u64,
    ) -> Result<(ParameterOrder, Tensors)> {
        config.validate()?;
        let (encoder, mut values) = Encoder::initial_state(&config.encoder, seed)?;
        aic_torch::tch::manual_seed(i64::try_from(seed).map_err(|_| {
            anyhow::anyhow!("aic-learning.outer-model-seed: seed exceeds LibTorch range")
        })?);
        let mut heads = Vec::new();
        for (name, input, output) in layers(config) {
            let bound = 1.0 / (input as f64).sqrt();
            for (suffix, shape) in [("weight", vec![output, input]), ("bias", vec![output])] {
                let parameter = format!("{name}.{suffix}");
                values.insert(
                    format!("outer.model.{parameter}"),
                    Tensor::empty(&shape, (Kind::Float, Device::Cpu)).uniform_(-bound, bound),
                );
                heads.push(parameter);
            }
        }
        Ok((
            ParameterOrder {
                schema: CHECKPOINT_SCHEMA.into(),
                encoder,
                heads,
            },
            values,
        ))
    }

    pub fn load(
        config: &OuterModelConfig,
        order: &ParameterOrder,
        values: &Tensors,
        device: Device,
    ) -> Result<Self> {
        config.validate()?;
        ensure!(
            order.schema == CHECKPOINT_SCHEMA,
            "aic-learning.outer-model-order: invalid schema"
        );
        let encoder = Encoder::load(&config.encoder, &order.encoder, values, device)?;
        let expected = layers(config)
            .into_iter()
            .flat_map(|(name, _, _)| [format!("{name}.weight"), format!("{name}.bias")])
            .collect::<BTreeSet<_>>();
        ensure!(
            order.heads.len() == expected.len()
                && order.heads.iter().cloned().collect::<BTreeSet<_>>() == expected,
            "aic-learning.outer-model-order: invalid head parameters"
        );
        let mut parameters = encoder
            .parameters
            .iter()
            .map(|(name, value)| (format!("encoder.{name}"), value.shallow_clone()))
            .collect::<Vec<_>>();
        let mut weights = BTreeMap::new();
        for name in &order.heads {
            let value = tensor(values, &format!("outer.model.{name}"))?
                .to_device(device)
                .copy()
                .set_requires_grad(true);
            ensure!(
                value.kind() == Kind::Float && bool::try_from(value.isfinite().all())?,
                "aic-learning.outer-model-parameter: {name}"
            );
            weights.insert(name.clone(), value.shallow_clone());
            parameters.push((format!("head.{name}"), value));
        }
        for (name, input, output) in layers(config) {
            ensure!(
                weights[&format!("{name}.weight")].size() == [output, input]
                    && weights[&format!("{name}.bias")].size() == [output],
                "aic-learning.outer-model-parameter-shape: {name}"
            );
        }
        Ok(Self {
            config: config.clone(),
            encoder,
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

    pub fn forward(&self, batch: &Batch) -> Result<Prediction> {
        let encoded = self.encoder.encode(&batch.graph)?;
        let action_context = encoded.context().index_select(0, &batch.action_graph);
        let hidden = self
            .linear(
                "policy.0",
                &Tensor::cat(&[action_context, batch.action_features.shallow_clone()], -1),
            )
            .tanh();
        let logits = self.linear("policy.2", &hidden).squeeze_dim(-1);
        let logp = segmented_log_softmax(&logits, &batch.action_graph, batch.graph.graphs());
        let values = self
            .linear("value.2", &self.linear("value.0", encoded.context()).tanh())
            .sigmoid();
        ensure!(
            bool::try_from(
                logits
                    .isfinite()
                    .all()
                    .logical_and(&logp.isfinite().all())
                    .logical_and(&values.isfinite().all())
            )?,
            "aic-learning.outer-model-forward: nonfinite prediction"
        );
        Ok(Prediction {
            logits,
            logp,
            values,
        })
    }

    pub fn snapshot(&self) -> Tensors {
        let mut values = self
            .encoder
            .parameters
            .iter()
            .map(|(name, value)| {
                (
                    format!("constructive.model.{name}"),
                    value.detach().to_device(Device::Cpu).copy(),
                )
            })
            .collect::<Tensors>();
        values.extend(self.weights.iter().map(|(name, value)| {
            (
                format!("outer.model.{name}"),
                value.detach().to_device(Device::Cpu).copy(),
            )
        }));
        values
    }
}

fn layers(config: &OuterModelConfig) -> Vec<(String, i64, i64)> {
    vec![
        (
            "policy.0".into(),
            config.encoder.width + ACTION_FEATURES as i64,
            config.width,
        ),
        ("policy.2".into(), config.width, 1),
        ("value.0".into(), config.encoder.width, config.width),
        ("value.2".into(), config.width, VALUE_HEADS as i64),
    ]
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

#[cfg(test)]
mod tests {
    use super::*;
    use aic_pilot_native::constructive::learning::{
        ENTITY_FEATURES, FOCUS_FEATURES, GLOBAL_FEATURES, RELATION_FEATURES, SPATIAL_FEATURES,
    };

    fn input() -> Input {
        Input {
            schema: aic_pilot_native::constructive::learning::SCHEMA,
            entity_features: vec![[0.; ENTITY_FEATURES], [1.; ENTITY_FEATURES]],
            relation_index: vec![[0, 1]],
            relation_features: vec![[0.; RELATION_FEATURES]],
            spatial_features: vec![[0.; SPATIAL_FEATURES]],
            spatial_owners: vec![0],
            global_features: [0.; GLOBAL_FEATURES],
            focus_features: [0.; FOCUS_FEATURES],
            focus_entity: -1,
        }
    }

    #[test]
    fn variable_macro_domains_are_normalized_per_state() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            width: 16,
        };
        let (order, state) = Model::initial_state(&config, 51)?;
        let model = Model::load(&config, &order, &state, Device::Cpu)?;
        let first = input();
        let second = input();
        let a = vec![[0.; ACTION_FEATURES], [1.; ACTION_FEATURES]];
        let b = vec![[0.5; ACTION_FEATURES]; 3];
        let batch = Batch::from_rows(&[&first, &second], &[&a, &b], Device::Cpu)?;
        let prediction = model.forward(&batch)?;
        for state in 0..batch.states() {
            let range = batch.action_range(state);
            let mass = prediction
                .logp
                .narrow(0, range.start, range.end - range.start)
                .exp()
                .sum(Kind::Float);
            assert!((f64::try_from(mass)? - 1.0).abs() < 1e-5);
        }
        assert_eq!(prediction.values.size(), [2, VALUE_HEADS as i64]);
        Ok(())
    }
}
