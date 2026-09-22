//! Dense opportunity-cost guidance for exact route candidate generation.
//!
//! The graph encoder is a separate parameter instance of the established
//! constructive encoder. A compact two-resolution raster decoder predicts
//! nonnegative directed edge costs; the native router remains responsible for
//! every legality and replay decision.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

use aic_pilot_native::constructive::routing_candidates::{
    DirectedCost, GUIDANCE_GRID_CHANNELS, GUIDANCE_SCHEMA, Guidance, GuidanceHead, GuidanceInput,
};
use aic_torch::tch::{self, CModule, Device, Kind, Tensor};
use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    constructive::{InputBatch, Model as Encoder},
    input::{Tensors, tensor},
    schema::RouteGuidanceModelConfig,
};

pub const CHECKPOINT_SCHEMA: &str = "aic-route-guidance-checkpoint-v2";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ParameterOrder {
    pub schema: String,
    pub encoder: Vec<String>,
    pub raster: Vec<String>,
}

pub struct Batch {
    graph: InputBatch,
    raster: Tensor,
    width: i64,
    height: i64,
}

pub struct Prediction {
    /// `[batch, head, direction, y, x]`, in native cost units.
    pub directed_costs: Tensor,
    pub success: Tensor,
    pub uncertainty: Tensor,
}

pub struct Model {
    pub config: RouteGuidanceModelConfig,
    pub encoder: Encoder,
    pub parameters: Vec<(String, Tensor)>,
    weights: Tensors,
}

/// TorchScript traces keyed by the graph count and raster dimensions used by
/// one frozen lower-model inference batch.
pub struct CompiledModel<'a> {
    eager: &'a Model,
    frozen: Model,
    modules: Mutex<BTreeMap<(i64, i64, i64), CModule>>,
}

impl Batch {
    pub fn from_inputs(inputs: &[&GuidanceInput], device: Device) -> Result<Self> {
        ensure!(
            !inputs.is_empty() && inputs.len() <= 262_144,
            "aic-learning.route-guidance-batch: invalid batch size"
        );
        let first = inputs[0];
        let width = first.bounds.width();
        let height = first.bounds.height();
        let area = usize::try_from(width.checked_mul(height).unwrap_or(0)).unwrap_or(0);
        ensure!(
            first.schema == GUIDANCE_SCHEMA
                && width > 0
                && height > 0
                && area <= 250_000
                && inputs.iter().all(|input| {
                    input.schema == GUIDANCE_SCHEMA
                        && input.bounds.width() == width
                        && input.bounds.height() == height
                        && input.channels == GUIDANCE_GRID_CHANNELS
                        && input.grid.len() == GUIDANCE_GRID_CHANNELS * area
                        && input.grid.iter().all(|value| value.is_finite())
                }),
            "aic-learning.route-guidance-batch: incompatible or malformed guidance inputs"
        );
        let graph_rows = inputs
            .iter()
            .map(|input| input.graph.as_ref())
            .collect::<Vec<_>>();
        let graph = InputBatch::from_inputs(&graph_rows, device)?;
        let raster = Tensor::from_slice(
            &inputs
                .iter()
                .flat_map(|input| input.grid.iter().copied())
                .collect::<Vec<_>>(),
        )
        .view([
            inputs.len() as i64,
            GUIDANCE_GRID_CHANNELS as i64,
            height,
            width,
        ])
        .to_device(device);
        Ok(Self {
            graph,
            raster,
            width,
            height,
        })
    }

    pub fn len(&self) -> usize {
        self.graph.graphs() as usize
    }

    pub fn dimensions(&self) -> (i64, i64) {
        (self.width, self.height)
    }

    fn trace_tensors(&self) -> Vec<Tensor> {
        let mut values = self.graph.trace_tensors();
        values.push(self.raster.shallow_clone());
        values
    }

    fn from_trace_tensors(values: &[Tensor], graphs: i64, width: i64, height: i64) -> Result<Self> {
        ensure!(
            values.len() == crate::constructive::INPUT_TENSOR_COUNT + 1,
            "aic-learning.route-guidance-trace-input: invalid tensor count"
        );
        Ok(Self {
            graph: InputBatch::from_trace_tensors(
                &values[..crate::constructive::INPUT_TENSOR_COUNT],
                graphs,
            )?,
            raster: values[crate::constructive::INPUT_TENSOR_COUNT].shallow_clone(),
            width,
            height,
        })
    }
}

impl Model {
    pub fn initial_state(
        config: &RouteGuidanceModelConfig,
        seed: u64,
    ) -> Result<(ParameterOrder, Tensors)> {
        config.validate()?;
        let (encoder, mut values) = Encoder::initial_state(&config.encoder, seed)?;
        tch::manual_seed(
            i64::try_from(seed)
                .map_err(|_| anyhow::anyhow!("aic-learning.route-guidance-seed: out of range"))?,
        );
        let mut raster = Vec::new();
        for (name, shape, fan_in) in raster_layers(config) {
            let bound = 1.0 / (fan_in as f64).sqrt();
            let weight_name = format!("{name}.weight");
            values.insert(
                format!("route-guidance.model.{weight_name}"),
                Tensor::empty(&shape, (Kind::Float, Device::Cpu)).uniform_(-bound, bound),
            );
            raster.push(weight_name);
            let bias_name = format!("{name}.bias");
            values.insert(
                format!("route-guidance.model.{bias_name}"),
                Tensor::empty([shape[0]], (Kind::Float, Device::Cpu)).uniform_(-bound, bound),
            );
            raster.push(bias_name);
        }
        Ok((
            ParameterOrder {
                schema: CHECKPOINT_SCHEMA.into(),
                encoder,
                raster,
            },
            values,
        ))
    }

    pub fn load(
        config: &RouteGuidanceModelConfig,
        order: &ParameterOrder,
        values: &Tensors,
        device: Device,
    ) -> Result<Self> {
        config.validate()?;
        ensure!(
            order.schema == CHECKPOINT_SCHEMA,
            "aic-learning.route-guidance-order: invalid schema"
        );
        let encoder = Encoder::load(&config.encoder, &order.encoder, values, device)?;
        let expected = raster_layers(config)
            .into_iter()
            .flat_map(|(name, _, _)| [format!("{name}.weight"), format!("{name}.bias")])
            .collect::<BTreeSet<_>>();
        ensure!(
            order.raster.len() == expected.len()
                && order.raster.iter().cloned().collect::<BTreeSet<_>>() == expected,
            "aic-learning.route-guidance-order: missing, duplicate or unknown parameter"
        );
        let mut parameters = encoder
            .parameters
            .iter()
            .map(|(name, value)| (format!("encoder.{name}"), value.shallow_clone()))
            .collect::<Vec<_>>();
        let mut weights = BTreeMap::new();
        for name in &order.raster {
            let value = tensor(values, &format!("route-guidance.model.{name}"))?
                .to_device(device)
                .copy()
                .set_requires_grad(true);
            ensure!(
                value.kind() == Kind::Float && bool::try_from(value.isfinite().all())?,
                "aic-learning.route-guidance-parameter: {name}"
            );
            weights.insert(name.clone(), value.shallow_clone());
            parameters.push((format!("raster.{name}"), value));
        }
        for (name, shape, _) in raster_layers(config) {
            ensure!(
                weights[&format!("{name}.weight")].size() == shape
                    && weights[&format!("{name}.bias")].size() == [shape[0]],
                "aic-learning.route-guidance-parameter-shape: {name}"
            );
        }
        Ok(Self {
            config: config.clone(),
            encoder,
            parameters,
            weights,
        })
    }

    fn conv(&self, name: &str, value: &Tensor, stride: i64, padding: i64) -> Tensor {
        value.conv2d(
            &self.weights[&format!("{name}.weight")],
            Some(&self.weights[&format!("{name}.bias")]),
            stride,
            padding,
            1,
            1,
        )
    }

    fn linear(&self, name: &str, value: &Tensor) -> Tensor {
        value.linear(
            &self.weights[&format!("{name}.weight")],
            Some(&self.weights[&format!("{name}.bias")]),
        )
    }

    fn forward_unchecked(&self, batch: &Batch) -> Prediction {
        let encoded = self.encoder.encode_unchecked(&batch.graph);
        let first = self.conv("raster.enc", &batch.raster, 1, 1).silu();
        let down = self.conv("raster.down", &first, 2, 1).silu();
        let size = down.size();
        let context = encoded
            .context()
            .unsqueeze(-1)
            .unsqueeze(-1)
            .expand([-1, -1, size[2], size[3]], false);
        let fused = self
            .conv("raster.fuse", &Tensor::cat(&[down, context], 1), 1, 1)
            .silu();
        let up = fused.upsample_nearest2d([batch.height, batch.width], None, None);
        let decoded = self
            .conv("raster.decode", &Tensor::cat(&[first, up], 1), 1, 1)
            .silu();
        let directed_costs = self
            .conv("raster.cost", &decoded, 1, 0)
            .softplus()
            .clamp_max(self.config.maximum_edge_cost as f64)
            .view([
                batch.graph.graphs(),
                self.config.heads as i64,
                4,
                batch.height,
                batch.width,
            ]);
        let outcomes = self
            .linear("outcome", encoded.context())
            .view([batch.graph.graphs(), self.config.heads as i64, 2])
            .sigmoid();
        let success = outcomes.select(-1, 0);
        let uncertainty = outcomes.select(-1, 1);
        Prediction {
            directed_costs,
            success,
            uncertainty,
        }
    }

    pub fn forward(&self, batch: &Batch) -> Result<Prediction> {
        let prediction = self.forward_unchecked(batch);
        ensure!(
            bool::try_from(
                prediction
                    .directed_costs
                    .isfinite()
                    .all()
                    .logical_and(&prediction.success.isfinite().all())
                    .logical_and(&prediction.uncertainty.isfinite().all())
            )?,
            "aic-learning.route-guidance-forward: nonfinite prediction"
        );
        Ok(prediction)
    }

    pub fn predict_native(
        &self,
        inputs: &[&GuidanceInput],
        model_version: &str,
        device: Device,
    ) -> Result<Vec<Guidance>> {
        ensure!(
            !model_version.is_empty() && model_version.len() <= 256,
            "aic-learning.route-guidance-version: invalid model version"
        );
        let batch = Batch::from_inputs(inputs, device)?;
        let prediction = tch::no_grad(|| self.forward(&batch))?;
        prediction_to_native(
            inputs,
            model_version,
            &batch,
            prediction,
            self.config.heads,
            self.config.maximum_edge_cost,
        )
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
                format!("route-guidance.model.{name}"),
                value.detach().to_device(Device::Cpu).copy(),
            )
        }));
        values
    }
}

impl<'a> CompiledModel<'a> {
    pub fn new(eager: &'a Model) -> Self {
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
            modules: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn predict_native(
        &self,
        inputs: &[&GuidanceInput],
        model_version: &str,
        device: Device,
    ) -> Result<Vec<Guidance>> {
        ensure!(
            !model_version.is_empty() && model_version.len() <= 256,
            "aic-learning.route-guidance-version: invalid model version"
        );
        let batch = Batch::from_inputs(inputs, device)?;
        let prediction = tch::no_grad(|| self.forward(&batch))?;
        prediction_to_native(
            inputs,
            model_version,
            &batch,
            prediction,
            self.eager.config.heads,
            self.eager.config.maximum_edge_cost,
        )
    }

    fn forward(&self, batch: &Batch) -> Result<Prediction> {
        let inputs = batch.trace_tensors();
        let key = (batch.graph.graphs(), batch.width, batch.height);
        let mut modules = self.modules.lock().map_err(|_| {
            anyhow!("aic-learning.route-guidance-compiled-lock: poisoned trace cache")
        })?;
        if !modules.contains_key(&key) {
            let frozen = &self.frozen;
            let (graphs, width, height) = key;
            let mut trace = |values: &[Tensor]| {
                let batch = Batch::from_trace_tensors(values, graphs, width, height)
                    .expect("validated route-guidance trace tensors");
                vec![pack_prediction(&frozen.forward_unchecked(&batch))]
            };
            let module = tch::no_grad(|| {
                CModule::create_by_tracing(
                    "AicRouteGuidanceInference",
                    "forward",
                    &inputs,
                    &mut trace,
                )
            })?;
            let expected = pack_prediction(&self.eager.forward_unchecked(batch));
            let actual = module.forward_ts(&inputs).context(
                "aic-learning.route-guidance-compiled-execution: parity invocation failed",
            )?;
            let costs = graphs * self.eager.config.heads as i64 * 4 * width * height;
            let actual_costs = actual.narrow(0, 0, costs);
            let expected_costs = expected.narrow(0, 0, costs);
            let rounded_costs_match = bool::try_from(
                actual_costs
                    .round()
                    .eq_tensor(&expected_costs.round())
                    .all(),
            )?;
            let outcomes_match = actual.narrow(0, costs, actual.size()[0] - costs).allclose(
                &expected.narrow(0, costs, expected.size()[0] - costs),
                1e-4,
                1e-4,
                false,
            );
            let maximum_absolute_error =
                f64::try_from((&actual - &expected).abs().max()).unwrap_or(f64::INFINITY);
            ensure!(
                actual.size() == expected.size()
                    && rounded_costs_match
                    && outcomes_match
                    && bool::try_from(actual.isfinite().all())?,
                "aic-learning.route-guidance-compiled-parity: traced output changes native costs or outcome estimates; max_abs={maximum_absolute_error:.6e}"
            );
            modules.insert(key, module);
        }
        let packed = modules[&key].forward_ts(&inputs).context(
            "aic-learning.route-guidance-compiled-execution: inference invocation failed",
        )?;
        let costs = key.0 * self.eager.config.heads as i64 * 4 * key.1 * key.2;
        let outcomes = key.0 * self.eager.config.heads as i64;
        Ok(Prediction {
            directed_costs: packed.narrow(0, 0, costs).view([
                key.0,
                self.eager.config.heads as i64,
                4,
                key.2,
                key.1,
            ]),
            success: packed
                .narrow(0, costs, outcomes)
                .view([key.0, self.eager.config.heads as i64]),
            uncertainty: packed
                .narrow(0, costs + outcomes, outcomes)
                .view([key.0, self.eager.config.heads as i64]),
        })
    }
}

fn pack_prediction(prediction: &Prediction) -> Tensor {
    Tensor::cat(
        &[
            prediction.directed_costs.flatten(0, -1),
            prediction.success.flatten(0, -1),
            prediction.uncertainty.flatten(0, -1),
        ],
        0,
    )
}

fn prediction_to_native(
    inputs: &[&GuidanceInput],
    model_version: &str,
    batch: &Batch,
    prediction: Prediction,
    heads: usize,
    maximum_edge_cost: u32,
) -> Result<Vec<Guidance>> {
    let costs = Vec::<f32>::try_from(
        prediction
            .directed_costs
            .round()
            .to_device(Device::Cpu)
            .view([-1]),
    )?;
    let success = Vec::<f32>::try_from(prediction.success.to_device(Device::Cpu).view([-1]))?;
    let uncertainty =
        Vec::<f32>::try_from(prediction.uncertainty.to_device(Device::Cpu).view([-1]))?;
    let area = usize::try_from(batch.width * batch.height).expect("validated raster area");
    let per_input = heads * 4 * area;
    Ok(inputs
        .iter()
        .enumerate()
        .map(|(input_index, input)| Guidance {
            schema: GUIDANCE_SCHEMA.into(),
            state_revision: input.state_revision,
            connection: input.connection.clone(),
            model_version: model_version.into(),
            heads: (0..heads)
                .map(|head| {
                    let base = input_index * per_input + head * 4 * area;
                    let mut directed_costs = Vec::with_capacity(4 * area);
                    for direction in 0..4 {
                        for y in 0..batch.height {
                            for x in 0..batch.width {
                                let offset = base
                                    + direction * area
                                    + usize::try_from(y * batch.width + x)
                                        .expect("validated raster offset");
                                let cost =
                                    costs[offset].clamp(0.0, maximum_edge_cost as f32) as u32;
                                if cost > 0 {
                                    directed_costs.push(DirectedCost {
                                        point: aic_layout::geometry::Point {
                                            x: input.bounds.min_x + x,
                                            y: input.bounds.min_y + y,
                                        },
                                        direction,
                                        cost,
                                    });
                                }
                            }
                        }
                    }
                    GuidanceHead {
                        id: format!("head-{head}"),
                        directed_costs,
                        predicted_success: success[input_index * heads + head],
                        uncertainty: uncertainty[input_index * heads + head],
                    }
                })
                .collect(),
        })
        .collect())
}

fn raster_layers(config: &RouteGuidanceModelConfig) -> Vec<(String, Vec<i64>, i64)> {
    let width = config.raster_width;
    let context = config.encoder.width;
    let outputs = (config.heads * 4) as i64;
    vec![
        (
            "raster.enc".into(),
            vec![width, GUIDANCE_GRID_CHANNELS as i64, 3, 3],
            GUIDANCE_GRID_CHANNELS as i64 * 9,
        ),
        ("raster.down".into(), vec![width, width, 3, 3], width * 9),
        (
            "raster.fuse".into(),
            vec![width, width + context, 3, 3],
            (width + context) * 9,
        ),
        (
            "raster.decode".into(),
            vec![width, width * 2, 3, 3],
            width * 18,
        ),
        ("raster.cost".into(), vec![outputs, width, 1, 1], width),
        (
            "outcome".into(),
            vec![(config.heads * 2) as i64, context],
            context,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_layout::geometry::Bounds;
    use aic_pilot_native::constructive::learning::{
        ENTITY_FEATURES, FOCUS_FEATURES, GLOBAL_FEATURES, Input, RELATION_FEATURES,
        SPATIAL_FEATURES,
    };
    use std::sync::Arc;

    #[test]
    fn dense_guidance_produces_bounded_native_heads() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let config = RouteGuidanceModelConfig {
            schema: RouteGuidanceModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            raster_width: 8,
            heads: 2,
            maximum_edge_cost: 32,
        };
        let (order, state) = Model::initial_state(&config, 17)?;
        let model = Model::load(&config, &order, &state, Device::Cpu)?;
        let graph = Arc::new(Input {
            schema: aic_pilot_native::constructive::learning::SCHEMA,
            entity_features: vec![[0.; ENTITY_FEATURES], [1.; ENTITY_FEATURES]],
            relation_index: vec![[0, 1]],
            relation_features: vec![[0.; RELATION_FEATURES]],
            spatial_features: vec![[0.; SPATIAL_FEATURES]],
            spatial_owners: vec![0],
            global_features: [0.; GLOBAL_FEATURES],
            focus_features: [0.; FOCUS_FEATURES],
            focus_entity: 1,
        });
        let input = GuidanceInput {
            schema: GUIDANCE_SCHEMA,
            state_revision: 4,
            connection: "route".into(),
            catalog_identity: "0".repeat(64),
            bounds: Bounds {
                min_x: -1,
                min_y: 2,
                max_x: 6,
                max_y: 7,
            },
            channels: GUIDANCE_GRID_CHANNELS,
            grid: vec![0.; GUIDANCE_GRID_CHANNELS * 8 * 6],
            graph,
        };
        let native = model.predict_native(&[&input], "random-v1", Device::Cpu)?;
        assert_eq!(native.len(), 1);
        assert_eq!(native[0].heads.len(), 2);
        assert!(native[0].heads.iter().all(|head| {
            head.directed_costs
                .iter()
                .all(|cost| cost.cost <= 32 && input.bounds.contains(cost.point))
                && (0.0..=1.0).contains(&head.predicted_success)
                && (0.0..=1.0).contains(&head.uncertainty)
        }));
        Ok(())
    }
}
