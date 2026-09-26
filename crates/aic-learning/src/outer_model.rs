//! Separate policy/value model for stable-state hierarchical macro actions.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

use aic_pilot_native::constructive::learning::Input;
use aic_torch::tch::{self, CModule, Device, Kind, Tensor};
use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    constructive::{InputBatch, Model as Encoder},
    input::{Tensors, tensor},
    schema::OuterModelConfig,
};

pub const ACTION_FEATURES: usize = 30;
pub const VALUE_HEADS: usize = 8;
pub const CHECKPOINT_SCHEMA: &str = "aic-hierarchical-outer-checkpoint-v4";
pub const COMPLETION_HEAD: i64 = 0;
pub const AREA_HEAD: i64 = 4;
pub const TRANSPORT_HEAD: i64 = 5;
pub const TURNS_HEAD: i64 = 6;
pub const UNCERTAINTY_HEAD: i64 = 7;

/// Stable observable targets for the outer critic. Layout quality is conditional
/// on completion; uncertainty is trained from the detached completion error.
#[derive(Clone, Copy, Debug)]
pub struct ValueTarget {
    pub completion: f32,
    pub additional_fraction: f32,
    pub next_progress: f32,
    pub blocked_fraction: f32,
    pub area: Option<f32>,
    pub transport: Option<f32>,
    pub turns: Option<f32>,
}

pub fn normalized_layout_quality(
    footprint_area: u64,
    transport_cells: u64,
    turns: u64,
    canvas_area: i64,
    connections: usize,
) -> [f32; 3] {
    let canvas = canvas_area.max(1) as f64;
    let routed = canvas * connections.max(1) as f64;
    [
        (footprint_area as f64 / (canvas + 1.0)).min(1.0) as f32,
        (transport_cells as f64 / (routed + 1.0)).min(1.0) as f32,
        (turns as f64 / (routed + 1.0)).min(1.0) as f32,
    ]
}

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
    action_entity: Tensor,
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

/// A frozen TorchScript trace of the outer inference graph. Training continues
/// to use `Model` directly; this wrapper is created only around immutable
/// generation checkpoints.
pub struct CompiledModel<'a> {
    eager: &'a Model,
    frozen: Model,
    module: Mutex<Option<CModule>>,
}

impl Batch {
    pub fn from_rows(
        inputs: &[&Input],
        actions: &[&[[f32; ACTION_FEATURES]]],
        entity_references: &[&[i64]],
        device: Device,
    ) -> Result<Self> {
        ensure!(
            !inputs.is_empty()
                && inputs.len() == actions.len()
                && inputs.len() == entity_references.len()
                && inputs.len() <= 262_144
                && actions
                    .iter()
                    .zip(entity_references)
                    .all(|(rows, references)| !rows.is_empty() && rows.len() == references.len()),
            "aic-learning.outer-batch: invalid state or action count"
        );
        let graph = InputBatch::from_inputs(inputs, device)?;
        let mut flat = Vec::new();
        let mut action_graph = Vec::new();
        let mut action_entity = Vec::new();
        let mut offsets = Vec::with_capacity(actions.len() + 1);
        let mut entity_offset = 0i64;
        offsets.push(0);
        for (state, ((input, rows), references)) in inputs
            .iter()
            .zip(actions)
            .zip(entity_references)
            .enumerate()
        {
            ensure!(
                references.iter().all(|reference| *reference >= 0
                    && (*reference as usize) < input.entity_features.len()),
                "aic-learning.outer-batch: action entity reference is outside its state"
            );
            flat.extend(rows.iter().flatten().copied());
            action_graph.extend(std::iter::repeat_n(state as i64, rows.len()));
            action_entity.extend(references.iter().map(|reference| entity_offset + reference));
            offsets.push(action_graph.len() as i64);
            entity_offset += input.entity_features.len() as i64;
        }
        let action_features = Tensor::from_slice(&flat)
            .view([-1, ACTION_FEATURES as i64])
            .to_device(device);
        let action_graph = Tensor::from_slice(&action_graph).to_device(device);
        let action_entity = Tensor::from_slice(&action_entity).to_device(device);
        Ok(Self {
            graph,
            action_features,
            action_graph,
            action_entity,
            offsets,
        })
    }

    pub fn action_range(&self, state: usize) -> std::ops::Range<i64> {
        self.offsets[state]..self.offsets[state + 1]
    }

    pub fn states(&self) -> usize {
        self.graph.graphs() as usize
    }

    fn trace_tensors(&self) -> Vec<Tensor> {
        let mut values = self.graph.trace_tensors();
        values.push(self.action_features.shallow_clone());
        values.push(self.action_graph.shallow_clone());
        values.push(self.action_entity.shallow_clone());
        values
    }

    fn from_trace_tensors(values: &[Tensor], graphs: i64) -> Result<Self> {
        ensure!(
            values.len() == crate::constructive::INPUT_TENSOR_COUNT + 3,
            "aic-learning.outer-trace-input: invalid tensor count"
        );
        Ok(Self {
            graph: InputBatch::from_trace_tensors(
                &values[..crate::constructive::INPUT_TENSOR_COUNT],
                graphs,
            )?,
            action_features: values[crate::constructive::INPUT_TENSOR_COUNT].shallow_clone(),
            action_graph: values[crate::constructive::INPUT_TENSOR_COUNT + 1].shallow_clone(),
            action_entity: values[crate::constructive::INPUT_TENSOR_COUNT + 2].shallow_clone(),
            offsets: Vec::new(),
        })
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

    fn forward_unchecked(&self, batch: &Batch) -> Prediction {
        let encoded = self.encoder.encode_unchecked(&batch.graph);
        let action_context = encoded.context().index_select(0, &batch.action_graph);
        let action_entity = encoded.entities().index_select(0, &batch.action_entity);
        let hidden = self
            .linear(
                "policy.0",
                &Tensor::cat(
                    &[
                        action_context,
                        action_entity,
                        batch.action_features.shallow_clone(),
                    ],
                    -1,
                ),
            )
            .tanh();
        let logits = self.linear("policy.2", &hidden).squeeze_dim(-1);
        let logp = segmented_log_softmax(&logits, &batch.action_graph, batch.graph.graphs());
        let values = self
            .linear("value.2", &self.linear("value.0", encoded.context()).tanh())
            .sigmoid();
        Prediction {
            logits,
            logp,
            values,
        }
    }

    pub fn forward(&self, batch: &Batch) -> Result<Prediction> {
        let prediction = self.forward_unchecked(batch);
        ensure!(
            bool::try_from(
                prediction
                    .logits
                    .isfinite()
                    .all()
                    .logical_and(&prediction.logp.isfinite().all())
                    .logical_and(&prediction.values.isfinite().all())
            )?,
            "aic-learning.outer-model-forward: nonfinite prediction"
        );
        Ok(prediction)
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

pub fn value_loss(prediction: &Tensor, targets: &[ValueTarget], device: Device) -> Result<Tensor> {
    ensure!(
        !targets.is_empty()
            && prediction.kind() == Kind::Float
            && prediction.size() == [targets.len() as i64, VALUE_HEADS as i64],
        "aic-learning.outer-value-loss: incompatible prediction or targets"
    );
    let values = targets
        .iter()
        .flat_map(|target| {
            [
                target.completion,
                target.additional_fraction,
                target.next_progress,
                target.blocked_fraction,
                target.area.unwrap_or(0.0),
                target.transport.unwrap_or(0.0),
                target.turns.unwrap_or(0.0),
            ]
        })
        .collect::<Vec<_>>();
    ensure!(
        values
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value)),
        "aic-learning.outer-value-loss: target is outside [0, 1]"
    );
    let target = Tensor::from_slice(&values)
        .view([targets.len() as i64, 7])
        .to_device(device);
    let mask = Tensor::from_slice(
        &targets
            .iter()
            .flat_map(|target| {
                [
                    1.0f32,
                    1.0,
                    1.0,
                    1.0,
                    target.area.is_some() as u8 as f32,
                    target.transport.is_some() as u8 as f32,
                    target.turns.is_some() as u8 as f32,
                ]
            })
            .collect::<Vec<_>>(),
    )
    .view([targets.len() as i64, 7])
    .to_device(device);
    let predicted = prediction.narrow(1, 0, 7);
    let observed = ((&predicted - &target).square() * &mask).sum(Kind::Float)
        / mask.sum(Kind::Float).clamp_min(1.0);
    let uncertainty_target =
        (prediction.select(1, COMPLETION_HEAD).detach() - target.select(1, 0)).abs();
    let uncertainty = (prediction.select(1, UNCERTAINTY_HEAD) - uncertainty_target)
        .square()
        .mean(Kind::Float);
    Ok(observed + uncertainty * 0.1)
}

impl<'a> CompiledModel<'a> {
    pub fn new(eager: &'a Model) -> Self {
        // The profiling executor in the retained LibTorch build specializes an
        // empty spatial-token input to the wrong dtype on its second call.
        // The non-profiling TorchScript executor preserves the traced schema
        // and still executes the compiled graph.
        tch::jit::set_profiling_mode(false);
        let weights = eager
            .weights
            .iter()
            .map(|(name, value)| (name.clone(), value.detach()))
            .collect();
        Self {
            eager,
            frozen: Model {
                config: eager.config.clone(),
                encoder: eager.encoder.inference_copy(),
                parameters: eager
                    .parameters
                    .iter()
                    .map(|(name, value)| (name.clone(), value.detach()))
                    .collect(),
                weights,
            },
            module: Mutex::new(None),
        }
    }

    pub fn forward(&self, batch: &Batch) -> Result<Prediction> {
        let inputs = batch.trace_tensors();
        let graphs = batch.graph.graphs();
        let actions = batch.action_features.size()[0];
        let mut guard = self
            .module
            .lock()
            .map_err(|_| anyhow!("aic-learning.outer-compiled-lock: poisoned trace cache"))?;
        if guard.is_none() {
            let frozen = &self.frozen;
            let mut trace = |values: &[Tensor]| {
                let batch = Batch::from_trace_tensors(values, graphs)
                    .expect("validated outer trace tensors");
                let prediction = frozen.forward_unchecked(&batch);
                vec![pack_prediction(&prediction)]
            };
            let module = tch::no_grad(|| {
                CModule::create_by_tracing("AicOuterInference", "forward", &inputs, &mut trace)
            })?;
            let expected = pack_prediction(&self.eager.forward_unchecked(batch));
            let actual = module
                .forward_ts(&inputs)
                .context("aic-learning.outer-compiled-execution: parity invocation failed")?;
            ensure!(
                actual.size() == expected.size()
                    && actual.allclose(&expected, 1e-5, 1e-5, false)
                    && bool::try_from(actual.isfinite().all())?,
                "aic-learning.outer-compiled-parity: traced output differs from eager output"
            );
            *guard = Some(module);
        }
        let packed = guard
            .as_ref()
            .expect("compiled outer module initialized")
            .forward_ts(&inputs)
            .map_err(|error| {
                anyhow!(
                    "aic-learning.outer-compiled-execution: inference invocation failed; inputs={:?}: {error}",
                    inputs
                        .iter()
                        .map(|value| (value.kind(), value.size()))
                        .collect::<Vec<_>>()
                )
            })?;
        let logp = packed.narrow(0, 0, actions);
        let values = packed
            .narrow(0, actions, graphs * VALUE_HEADS as i64)
            .view([graphs, VALUE_HEADS as i64]);
        // The search boundary consumes probabilities and values only. Retain
        // logits as an empty tensor so training cannot accidentally use this
        // frozen inference wrapper.
        Ok(Prediction {
            logits: Tensor::empty([0], (Kind::Float, packed.device())),
            logp,
            values,
        })
    }
}

fn pack_prediction(prediction: &Prediction) -> Tensor {
    Tensor::cat(
        &[
            prediction.logp.flatten(0, -1),
            prediction.values.flatten(0, -1),
        ],
        0,
    )
}

fn layers(config: &OuterModelConfig) -> Vec<(String, i64, i64)> {
    vec![
        (
            "policy.0".into(),
            2 * config.encoder.width + ACTION_FEATURES as i64,
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
            bridge_resources: vec![],
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
        let a_entities = [0, 1];
        let b_entities = [0, 1, 0];
        let batch = Batch::from_rows(
            &[&first, &second],
            &[&a, &b],
            &[&a_entities, &b_entities],
            Device::Cpu,
        )?;
        assert_eq!(
            Vec::<i64>::try_from(batch.action_entity.to_device(Device::Cpu))?,
            vec![0, 1, 2, 3, 2]
        );
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
