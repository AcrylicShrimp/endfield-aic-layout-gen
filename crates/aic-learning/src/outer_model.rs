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
    constructive::{Encoded as GraphEncoded, InputBatch, Model as Encoder},
    input::{Tensors, tensor},
    schema::OuterModelConfig,
};

pub const ACTION_FEATURES: usize = 49;
pub const VALUE_HEADS: usize = 8;
pub const CHECKPOINT_SCHEMA: &str = "aic-hierarchical-outer-checkpoint-v5";
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

/// Reused graph encoding and value head for one or more bounded action pages.
pub(crate) struct EncodedOuterState {
    graph: GraphEncoded,
    pub(crate) values: Tensor,
    graphs: i64,
}

/// One graph encoding and detached policy leaves for a streamed root update.
/// Page backward calls fill head gradients and the detached leaves; the caller
/// combines `encoder_surrogate` with its batch value loss exactly once.
pub(crate) struct StreamedPolicyRoot<'m> {
    model: &'m Model,
    encoded: EncodedOuterState,
    context_leaf: Tensor,
    entity_leaf: Tensor,
    device: Device,
}

/// O(1) first-pass state for the exact current-model policy denominator.
pub(crate) struct StreamedPolicyStatistics {
    maximum: Option<Tensor>,
    denominator: Tensor,
    target_mass: Tensor,
    target_shifted_sum: Tensor,
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
    page_module: Mutex<Option<CModule>>,
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

    pub(crate) fn encode_unchecked(&self, graph: &InputBatch) -> EncodedOuterState {
        let encoded = self.encoder.encode_unchecked(graph);
        let values = self
            .linear("value.2", &self.linear("value.0", encoded.context()).tanh())
            .sigmoid();
        EncodedOuterState {
            graph: encoded,
            values,
            graphs: graph.graphs(),
        }
    }

    fn score_from_encoded_tensors(
        &self,
        context: &Tensor,
        entities: &Tensor,
        action_features: &Tensor,
        action_entity: &Tensor,
        action_graph: &Tensor,
    ) -> Tensor {
        let action_context = context.index_select(0, action_graph);
        let action_entity = entities.index_select(0, action_entity);
        let hidden = self
            .linear(
                "policy.0",
                &Tensor::cat(
                    &[
                        action_context,
                        action_entity,
                        action_features.shallow_clone(),
                    ],
                    -1,
                ),
            )
            .tanh();
        self.linear("policy.2", &hidden).squeeze_dim(-1)
    }

    fn score_tensor_unchecked(
        &self,
        encoded: &EncodedOuterState,
        action_features: &Tensor,
        action_entity: &Tensor,
        action_graph: &Tensor,
    ) -> Tensor {
        self.score_from_encoded_tensors(
            encoded.graph.context(),
            encoded.graph.entities(),
            action_features,
            action_entity,
            action_graph,
        )
    }

    fn one_root_page_tensors(
        encoded: &EncodedOuterState,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
        device: Device,
    ) -> Result<(Tensor, Tensor, Tensor)> {
        let entity_count = encoded.graph.entities().size()[0];
        ensure!(
            encoded.graphs == 1
                && !rows.is_empty()
                && rows.len() == entities.len()
                && entities
                    .iter()
                    .all(|entity| *entity >= 0 && *entity < entity_count),
            "aic-learning.outer-score-page: invalid one-root action references"
        );
        let features = Tensor::from_slice(&rows.iter().flatten().copied().collect::<Vec<_>>())
            .view([-1, ACTION_FEATURES as i64])
            .to_device(device);
        let entity = Tensor::from_slice(entities).to_device(device);
        let graph = Tensor::zeros([rows.len() as i64], (Kind::Int64, device));
        Ok((features, entity, graph))
    }

    pub(crate) fn score_one_root_page(
        &self,
        encoded: &EncodedOuterState,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
        device: Device,
    ) -> Result<Tensor> {
        let (features, entity, graph) =
            Self::one_root_page_tensors(encoded, rows, entities, device)?;
        let logits = self.score_tensor_unchecked(encoded, &features, &entity, &graph);
        ensure!(
            bool::try_from(logits.isfinite().all())?,
            "aic-learning.outer-score-page: nonfinite logits"
        );
        Ok(logits)
    }

    fn forward_unchecked(&self, batch: &Batch) -> Prediction {
        let encoded = self.encode_unchecked(&batch.graph);
        let logits = self.score_tensor_unchecked(
            &encoded,
            &batch.action_features,
            &batch.action_entity,
            &batch.action_graph,
        );
        let logp = segmented_log_softmax(&logits, &batch.action_graph, encoded.graphs);
        Prediction {
            logits,
            logp,
            values: encoded.values,
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

impl<'m> StreamedPolicyRoot<'m> {
    pub(crate) fn new(model: &'m Model, input: &Input, device: Device) -> Result<Self> {
        let graph = InputBatch::from_inputs(&[input], device)?;
        let encoded = model.encode_unchecked(&graph);
        ensure!(
            bool::try_from(encoded.values.isfinite().all())?,
            "aic-learning.outer-streamed-value: nonfinite root value"
        );
        let context_leaf = encoded.graph.context().detach().set_requires_grad(true);
        let entity_leaf = encoded.graph.entities().detach().set_requires_grad(true);
        Ok(Self {
            model,
            encoded,
            context_leaf,
            entity_leaf,
            device,
        })
    }

    pub(crate) fn values(&self) -> &Tensor {
        &self.encoded.values
    }

    fn page_logits(&self, rows: &[[f32; ACTION_FEATURES]], entities: &[i64]) -> Result<Tensor> {
        let (features, entity, graph) =
            Model::one_root_page_tensors(&self.encoded, rows, entities, self.device)?;
        Ok(self.model.score_from_encoded_tensors(
            &self.context_leaf,
            &self.entity_leaf,
            &features,
            &entity,
            &graph,
        ))
    }

    pub(crate) fn frozen_logits(
        &self,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
    ) -> Result<Tensor> {
        tch::no_grad(|| self.page_logits(rows, entities))
    }

    /// Accumulate policy-head and detached encoder-output gradients for a page.
    /// The normalizer and target mass come from a complete no-gradient pass.
    pub(crate) fn backward_page(
        &self,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
        targets: &[f32],
        statistics: &StreamedPolicyStatistics,
        batch_states: usize,
    ) -> Result<()> {
        ensure!(
            batch_states > 0 && rows.len() == targets.len(),
            "aic-learning.outer-streamed-page: target length or batch size differs"
        );
        let maximum = statistics.maximum.as_ref().ok_or_else(|| {
            anyhow!("aic-learning.outer-streamed-page: missing full root normalizer")
        })?;
        let logits = self.page_logits(rows, entities)?;
        let target = Tensor::from_slice(targets)
            .to_device(self.device)
            .to_kind(Kind::Double);
        let coefficient = tch::no_grad(|| {
            ((logits.detach() - maximum).exp().to_kind(Kind::Double)
                * (&statistics.target_mass / &statistics.denominator)
                - target)
                .to_kind(Kind::Float)
        });
        ((logits * coefficient).sum(Kind::Float) / batch_states as f64).backward();
        Ok(())
    }

    pub(crate) fn encoder_surrogate(&self) -> Result<Tensor> {
        let context_grad = self.context_leaf.grad();
        let entity_grad = self.entity_leaf.grad();
        ensure!(
            context_grad.defined() && entity_grad.defined(),
            "aic-learning.outer-streamed-encoder: policy page gradients are missing"
        );
        Ok(
            (self.encoded.graph.context() * context_grad.detach()).sum(Kind::Float)
                + (self.encoded.graph.entities() * entity_grad.detach()).sum(Kind::Float),
        )
    }
}

impl StreamedPolicyStatistics {
    pub(crate) fn new(device: Device) -> Self {
        Self {
            maximum: None,
            denominator: Tensor::zeros([], (Kind::Double, device)),
            target_mass: Tensor::zeros([], (Kind::Double, device)),
            target_shifted_sum: Tensor::zeros([], (Kind::Double, device)),
        }
    }

    pub(crate) fn observe(&mut self, logits: &Tensor, targets: &[f32]) -> Result<()> {
        ensure!(
            logits.size() == [targets.len() as i64],
            "aic-learning.outer-streamed-statistics: page target length differs"
        );
        let target = Tensor::from_slice(targets)
            .to_device(logits.device())
            .to_kind(Kind::Double);
        tch::no_grad(|| {
            let page_maximum = logits.max();
            let previous = self.maximum.take();
            let maximum = previous.as_ref().map_or_else(
                || page_maximum.shallow_clone(),
                |old| old.maximum(&page_maximum),
            );
            if let Some(old) = previous {
                let shift = (old - &maximum).to_kind(Kind::Double);
                self.denominator = &self.denominator * shift.exp();
                self.target_shifted_sum = &self.target_shifted_sum + &self.target_mass * shift;
            }
            let shifted = logits - &maximum;
            self.denominator =
                &self.denominator + shifted.exp().to_kind(Kind::Double).sum(Kind::Double);
            self.target_mass = &self.target_mass + target.sum(Kind::Double);
            self.target_shifted_sum = &self.target_shifted_sum
                + (&target * shifted.to_kind(Kind::Double)).sum(Kind::Double);
            self.maximum = Some(maximum);
        });
        Ok(())
    }

    pub(crate) fn true_cross_entropy(&self) -> Result<Tensor> {
        self.maximum
            .as_ref()
            .ok_or_else(|| anyhow!("aic-learning.outer-streamed-statistics: empty root policy"))?;
        let loss = &self.target_mass * self.denominator.log() - &self.target_shifted_sum;
        ensure!(
            bool::try_from(loss.isfinite().all())?,
            "aic-learning.outer-streamed-statistics: nonfinite root policy loss"
        );
        Ok(loss)
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
    pub(crate) fn config(&self) -> &OuterModelConfig {
        &self.eager.config
    }

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
            page_module: Mutex::new(None),
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

    pub(crate) fn encode_one_root(&self, graph: &InputBatch) -> Result<EncodedOuterState> {
        ensure!(
            graph.graphs() == 1,
            "aic-learning.outer-score-page: expected one root graph"
        );
        let encoded = tch::no_grad(|| self.frozen.encode_unchecked(graph));
        ensure!(
            bool::try_from(encoded.values.isfinite().all())?,
            "aic-learning.outer-score-page: nonfinite root value"
        );
        Ok(encoded)
    }

    pub(crate) fn score_one_root_page(
        &self,
        encoded: &EncodedOuterState,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
        device: Device,
    ) -> Result<Tensor> {
        let (features, entity, graph) =
            Model::one_root_page_tensors(encoded, rows, entities, device)?;
        let inputs = [
            encoded.graph.context().shallow_clone(),
            encoded.graph.entities().shallow_clone(),
            features,
            entity,
            graph,
        ];
        let mut guard = self
            .page_module
            .lock()
            .map_err(|_| anyhow!("aic-learning.outer-compiled-page-lock: poisoned trace cache"))?;
        if guard.is_none() {
            let frozen = &self.frozen;
            let mut trace = |values: &[Tensor]| {
                vec![frozen.score_from_encoded_tensors(
                    &values[0], &values[1], &values[2], &values[3], &values[4],
                )]
            };
            let module = tch::no_grad(|| {
                CModule::create_by_tracing("AicOuterPageHead", "forward", &inputs, &mut trace)
            })?;
            let expected = tch::no_grad(|| {
                frozen.score_from_encoded_tensors(
                    &inputs[0], &inputs[1], &inputs[2], &inputs[3], &inputs[4],
                )
            });
            let actual = module.forward_ts(&inputs)?;
            ensure!(
                actual.size() == expected.size() && actual.allclose(&expected, 1e-5, 1e-5, false),
                "aic-learning.outer-compiled-page-parity: traced page head differs from eager"
            );
            *guard = Some(module);
        }
        let logits = tch::no_grad(|| {
            guard
                .as_ref()
                .expect("compiled page head initialized")
                .forward_ts(&inputs)
        })?;
        ensure!(
            logits.size() == [rows.len() as i64] && bool::try_from(logits.isfinite().all())?,
            "aic-learning.outer-compiled-page: invalid logits"
        );
        Ok(logits)
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
    use std::time::Instant;

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
    #[ignore = "local root-action scale probe"]
    fn plan00_outer_action_scoring_probe() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let rows = std::env::var("AIC_ROOT_SCORE_ROWS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(4096);
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 32,
                message_rounds: 1,
            },
            width: 32,
        };
        let (order, state) = Model::initial_state(&config, 51)?;
        let model = Model::load(&config, &order, &state, Device::Cpu)?;
        let graph = if let Ok(panel_path) = std::env::var("AIC_ROOT_SCORE_PANEL") {
            use aic_pilot_native::constructive::{Command, Limits, Session};
            let panel: serde_json::Value = serde_json::from_slice(&std::fs::read(panel_path)?)?;
            let case = &panel["cases"][0];
            let catalog_root =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized");
            let loaded = aic_layout_gen::io::load_catalogs(&catalog_root)?;
            let mut source = Session::new(
                serde_json::from_value(case["problem"].clone())?,
                std::sync::Arc::new(loaded.catalogs),
                serde_json::from_value::<Limits>(panel["limits"].clone())?,
            )?;
            let prefix = std::env::var("AIC_ROOT_SCORE_PREFIX")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            for command in case["prefix"].as_array().unwrap().iter().take(prefix) {
                let step = source.step(serde_json::from_value::<Command>(command.clone())?);
                assert!(step.accepted && step.diagnostic.is_none(), "{step:?}");
            }
            let candidate: aic_pilot_native::constructive::routing_candidates::Config =
                serde_json::from_value(panel["candidate"].clone())?;
            source.macro_learning_input(&aic_pilot_native::constructive::learning::Config {
                placement: candidate.bounds,
                extension_lengths: vec![1, 2, 4],
            })?
        } else {
            std::sync::Arc::new(input())
        };
        let features = vec![[0.5; ACTION_FEATURES]; rows];
        let references = vec![0; rows];
        let started = Instant::now();
        let batch = Batch::from_rows(&[&graph], &[&features], &[&references], Device::Cpu)?;
        let batch_seconds = started.elapsed().as_secs_f64();
        let started = Instant::now();
        let prediction = tch::no_grad(|| model.forward(&batch))?;
        let forward_seconds = started.elapsed().as_secs_f64();
        assert_eq!(prediction.logp.size(), [rows as i64]);
        eprintln!(
            "plan00 score rows={rows} entities={} spatial={} batch_seconds={batch_seconds:.6} forward_seconds={forward_seconds:.6}",
            graph.entity_features.len(),
            graph.spatial_features.len(),
        );
        if std::env::var("AIC_ROOT_SCORE_COMPARE_COMPILED").as_deref() == Ok("1") {
            let compiled = CompiledModel::new(&model);
            let _warm = tch::no_grad(|| compiled.forward(&batch))?;
            let started = Instant::now();
            let dense_compiled = tch::no_grad(|| compiled.forward(&batch))?;
            let dense_seconds = started.elapsed().as_secs_f64();
            let graph_batch = InputBatch::from_inputs(&[&graph], Device::Cpu)?;
            let started = Instant::now();
            let encoded = compiled.encode_one_root(&graph_batch)?;
            let encode_seconds = started.elapsed().as_secs_f64();
            let _warm =
                compiled.score_one_root_page(&encoded, &features, &references, Device::Cpu)?;
            let started = Instant::now();
            let page_logits =
                compiled.score_one_root_page(&encoded, &features, &references, Device::Cpu)?;
            let page_seconds = started.elapsed().as_secs_f64();
            let page_logp = page_logits.log_softmax(0, Kind::Float);
            assert!(bool::try_from(page_logp.allclose(
                &dense_compiled.logp,
                1e-5,
                1e-5,
                false
            ))?);
            assert!(bool::try_from(encoded.values.allclose(
                &dense_compiled.values,
                1e-6,
                1e-6,
                false
            ))?);
            eprintln!(
                "plan02 compiled compare rows={rows} dense_seconds={dense_seconds:.6} one_encode_seconds={encode_seconds:.6} reused_page_seconds={page_seconds:.6}"
            );
        }
        Ok(())
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

    #[test]
    fn encoded_action_pages_match_dense_logits_and_value() -> Result<()> {
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
        let (order, state) = Model::initial_state(&config, 71)?;
        let model = Model::load(&config, &order, &state, Device::Cpu)?;
        let source = input();
        let rows = [
            [0.0; ACTION_FEATURES],
            [0.5; ACTION_FEATURES],
            [1.0; ACTION_FEATURES],
        ];
        let entities = [0, 1, 0];
        let dense = Batch::from_rows(&[&source], &[&rows], &[&entities], Device::Cpu)?;
        let dense = tch::no_grad(|| model.forward(&dense))?;
        let graph = InputBatch::from_inputs(&[&source], Device::Cpu)?;
        let encoded = tch::no_grad(|| model.encode_unchecked(&graph));
        let left = tch::no_grad(|| {
            model.score_one_root_page(&encoded, &rows[..1], &entities[..1], Device::Cpu)
        })?;
        let right = tch::no_grad(|| {
            model.score_one_root_page(&encoded, &rows[1..], &entities[1..], Device::Cpu)
        })?;
        let joined = Tensor::cat(&[left, right], 0);
        assert!(bool::try_from(joined.allclose(
            &dense.logits,
            1e-6,
            1e-6,
            false
        ))?);
        assert!(bool::try_from(encoded.values.allclose(
            &dense.values,
            0.0,
            0.0,
            false
        ))?);
        let compiled = CompiledModel::new(&model);
        let compiled_state = compiled.encode_one_root(&graph)?;
        let left = compiled.score_one_root_page(
            &compiled_state,
            &rows[..1],
            &entities[..1],
            Device::Cpu,
        )?;
        let right = compiled.score_one_root_page(
            &compiled_state,
            &rows[1..],
            &entities[1..],
            Device::Cpu,
        )?;
        let joined = Tensor::cat(&[left, right], 0);
        assert!(bool::try_from(joined.allclose(
            &dense.logits,
            1e-6,
            1e-6,
            false
        ))?);
        assert!(bool::try_from(compiled_state.values.allclose(
            &dense.values,
            0.0,
            0.0,
            false
        ))?);
        Ok(())
    }

    fn streamed_gradient_case(policy_bias: f64) -> Result<()> {
        let device = Device::Cpu;
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            width: 16,
        };
        let (order, state) = Model::initial_state(&config, 71)?;
        let dense_model = Model::load(&config, &order, &state, device)?;
        let streamed_model = Model::load(&config, &order, &state, device)?;
        for model in [&dense_model, &streamed_model] {
            let mut bias = model.weights["policy.2.bias"].shallow_clone();
            let _ = tch::no_grad(|| bias.fill_(policy_bias));
        }
        let source = input();
        let mut rows = [[0.0f32; ACTION_FEATURES]; 5];
        for (index, row) in rows.iter_mut().enumerate() {
            row[index] = 0.2 + index as f32 * 0.1;
            row[12] = (index as f32 - 2.0) * 0.3;
        }
        let entities = [0, 1, 0, 1, 0];
        // A zero-target late action stays in the denominator. The sum is
        // deliberately 0.7, so assuming unit target mass changes gradients.
        let targets = [0.1f32, 0.15, 0.0, 0.2, 0.25];
        let values = ValueTarget {
            completion: 0.5,
            additional_fraction: 0.25,
            next_progress: 0.75,
            blocked_fraction: 0.3,
            area: Some(0.2),
            transport: Some(0.4),
            turns: Some(0.6),
        };
        let dense_batch = Batch::from_rows(&[&source], &[&rows], &[&entities], device)?;
        let prediction = dense_model.forward(&dense_batch)?;
        let dense_policy =
            -(Tensor::from_slice(&targets).to_device(device) * prediction.logp).sum(Kind::Float);
        let dense_loss = dense_policy + value_loss(&prediction.values, &[values], device)?;
        let dense_scalar = f64::try_from(&dense_loss)?;
        dense_loss.backward();

        let streamed = StreamedPolicyRoot::new(&streamed_model, &source, device)?;
        let mut statistics = StreamedPolicyStatistics::new(device);
        for (start, end) in [(0, 2), (2, 4), (4, 5)] {
            let logits = streamed.frozen_logits(&rows[start..end], &entities[start..end])?;
            statistics.observe(&logits, &targets[start..end])?;
        }
        let streamed_value = value_loss(streamed.values(), &[values], device)?;
        let streamed_scalar = f64::try_from(statistics.true_cross_entropy()? + &streamed_value)?;
        for (start, end) in [(0, 2), (2, 4), (4, 5)] {
            streamed.backward_page(
                &rows[start..end],
                &entities[start..end],
                &targets[start..end],
                &statistics,
                1,
            )?;
        }
        (streamed.encoder_surrogate()? + streamed_value).backward();
        assert!(
            (streamed_scalar - dense_scalar).abs() <= 2e-6,
            "bias {policy_bias} dense {dense_scalar} streamed {streamed_scalar}"
        );
        for ((dense_name, dense_value), (streamed_name, streamed_value)) in dense_model
            .parameters
            .iter()
            .zip(&streamed_model.parameters)
        {
            assert_eq!(dense_name, streamed_name);
            let dense_grad = dense_value.grad();
            let streamed_grad = streamed_value.grad();
            assert_eq!(
                dense_grad.defined(),
                streamed_grad.defined(),
                "gradient presence differs for {dense_name}"
            );
            if !dense_grad.defined() {
                continue;
            }
            assert!(
                bool::try_from(dense_grad.allclose(&streamed_grad, 1e-4, 2e-5, false))?,
                "parameter gradient differs: {dense_name}"
            );
        }
        Ok(())
    }

    #[test]
    fn streamed_policy_matches_dense_loss_and_every_parameter_gradient() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        streamed_gradient_case(0.0)?;
        streamed_gradient_case(1e8)
    }

    #[test]
    fn two_streamed_roots_preserve_batch_policy_and_value_scaling() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let device = Device::Cpu;
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            width: 16,
        };
        let (order, state) = Model::initial_state(&config, 29)?;
        let dense_model = Model::load(&config, &order, &state, device)?;
        let streamed_model = Model::load(&config, &order, &state, device)?;
        let first = input();
        let mut second = input();
        second.global_features[0] = 0.6;
        second.entity_features[0][0] = 0.35;
        let first_rows = [[0.1; ACTION_FEATURES], [0.4; ACTION_FEATURES]];
        let second_rows = [
            [0.2; ACTION_FEATURES],
            [0.5; ACTION_FEATURES],
            [0.8; ACTION_FEATURES],
        ];
        let first_entities = [0, 1];
        let second_entities = [1, 0, 1];
        let first_targets = [0.3f32, 0.7];
        let second_targets = [0.1f32, 0.2, 0.4];
        let targets = [
            ValueTarget {
                completion: 0.0,
                additional_fraction: 0.25,
                next_progress: 0.0,
                blocked_fraction: 0.8,
                area: None,
                transport: None,
                turns: None,
            },
            ValueTarget {
                completion: 1.0,
                additional_fraction: 0.75,
                next_progress: 1.0,
                blocked_fraction: 0.1,
                area: Some(0.3),
                transport: Some(0.2),
                turns: Some(0.4),
            },
        ];
        let dense_batch = Batch::from_rows(
            &[&first, &second],
            &[&first_rows, &second_rows],
            &[&first_entities, &second_entities],
            device,
        )?;
        let prediction = dense_model.forward(&dense_batch)?;
        let first_policy = -(Tensor::from_slice(&first_targets)
            * prediction.logp.narrow(0, 0, first_rows.len() as i64))
        .sum(Kind::Float);
        let second_policy = -(Tensor::from_slice(&second_targets)
            * prediction
                .logp
                .narrow(0, first_rows.len() as i64, second_rows.len() as i64))
        .sum(Kind::Float);
        let dense_loss = (first_policy + second_policy) / 2.0
            + value_loss(&prediction.values, &targets, device)?;
        let dense_scalar = f64::try_from(&dense_loss)?;
        dense_loss.backward();

        let first_root = StreamedPolicyRoot::new(&streamed_model, &first, device)?;
        let second_root = StreamedPolicyRoot::new(&streamed_model, &second, device)?;
        let mut first_stats = StreamedPolicyStatistics::new(device);
        let mut second_stats = StreamedPolicyStatistics::new(device);
        for index in 0..first_rows.len() {
            let logits = first_root.frozen_logits(
                &first_rows[index..index + 1],
                &first_entities[index..index + 1],
            )?;
            first_stats.observe(&logits, &first_targets[index..index + 1])?;
        }
        for index in 0..second_rows.len() {
            let logits = second_root.frozen_logits(
                &second_rows[index..index + 1],
                &second_entities[index..index + 1],
            )?;
            second_stats.observe(&logits, &second_targets[index..index + 1])?;
        }
        let streamed_values = Tensor::cat(
            &[
                first_root.values().shallow_clone(),
                second_root.values().shallow_clone(),
            ],
            0,
        );
        let streamed_value_loss = value_loss(&streamed_values, &targets, device)?;
        let streamed_scalar = f64::try_from(
            (first_stats.true_cross_entropy()? + second_stats.true_cross_entropy()?) / 2.0
                + &streamed_value_loss,
        )?;
        for index in 0..first_rows.len() {
            first_root.backward_page(
                &first_rows[index..index + 1],
                &first_entities[index..index + 1],
                &first_targets[index..index + 1],
                &first_stats,
                2,
            )?;
        }
        for index in 0..second_rows.len() {
            second_root.backward_page(
                &second_rows[index..index + 1],
                &second_entities[index..index + 1],
                &second_targets[index..index + 1],
                &second_stats,
                2,
            )?;
        }
        (first_root.encoder_surrogate()? + second_root.encoder_surrogate()? + streamed_value_loss)
            .backward();
        assert!((streamed_scalar - dense_scalar).abs() <= 2e-6);
        for ((dense_name, dense_value), (streamed_name, streamed_value)) in dense_model
            .parameters
            .iter()
            .zip(&streamed_model.parameters)
        {
            assert_eq!(dense_name, streamed_name);
            let dense_grad = dense_value.grad();
            let streamed_grad = streamed_value.grad();
            assert_eq!(
                dense_grad.defined(),
                streamed_grad.defined(),
                "{dense_name}"
            );
            if dense_grad.defined() {
                assert!(
                    bool::try_from(dense_grad.allclose(&streamed_grad, 1e-4, 2e-5, false))?,
                    "batch gradient differs: {dense_name}"
                );
            }
        }
        Ok(())
    }
}
