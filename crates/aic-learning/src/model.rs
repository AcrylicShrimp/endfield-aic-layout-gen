//! The existing graph encoder and conditional port head, expressed as LibTorch
//! tensor operations. Parameter names match the Python model for explicit import.
use crate::{
    input::{Packed, Tensors, tensor},
    schema::ModelConfig,
};
use aic_torch::tch::{Device, Kind, Tensor};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

pub struct Model {
    pub config: ModelConfig,
    pub parameters: Vec<(String, Tensor)>,
    weights: Tensors,
}
impl Model {
    /// Cold initialization follows the existing Linear layer order and uniform
    /// fan-in bounds. It runs once, before workers; training uses owned RNGs.
    pub fn initial_state(config: &ModelConfig, seed: u64) -> Result<(Vec<String>, Tensors)> {
        ensure!(
            seed <= i64::MAX as u64
                && config.width > 0
                && config.width <= 4096
                && config.message_rounds <= 32,
            "aic-learning.model-initialization: invalid seed or dimensions"
        );
        aic_torch::tch::manual_seed(seed as i64);
        let w = config.width;
        let mut layers = vec![
            ("nodes.0".to_owned(), config.node_features, w),
            ("edges.0".into(), config.edge_features, w),
            ("paths.0".into(), 2, w),
        ];
        for i in 0..config.message_rounds {
            layers.push((format!("messages.{i}"), w * 3, w));
        }
        for i in 0..config.message_rounds {
            layers.push((format!("updates.{i}"), w * 2, w));
        }
        layers.extend([
            ("context.0".into(), w + config.global_features, w),
            ("actor.0".into(), w * 3 + config.action_features, w),
            ("actor.2".into(), w, 1),
            ("critic.0".into(), w, w),
            ("critic.2".into(), w, 1),
            (
                "port_actor.0".into(),
                w * 3 + config.action_features + config.port_features * 2,
                w,
            ),
            ("port_actor.2".into(), w, 1),
        ]);
        let mut order = vec![];
        let mut tensors = Tensors::new();
        for (layer, input, output) in layers {
            ensure!(
                input > 0 && output > 0,
                "aic-learning.model-initialization: invalid fan-in for {layer}"
            );
            let bound = 1. / (input as f64).sqrt();
            for (suffix, shape) in [("weight", vec![output, input]), ("bias", vec![output])] {
                let name = format!("{layer}.{suffix}");
                let parameter =
                    Tensor::empty(&shape, (Kind::Float, Device::Cpu)).uniform_(-bound, bound);
                tensors.insert(format!("model.{name}"), parameter);
                let group = if name.starts_with("critic.") {
                    "critic"
                } else {
                    "actor"
                };
                tensors.insert(
                    format!("adam.{group}.{name}.step"),
                    Tensor::from_slice(&[0i64]),
                );
                order.push(name);
            }
        }
        for (name, base) in [
            ("sampler", 1_000_000),
            ("shuffle", 2_000_000),
            ("replay", 5_000_000),
            ("replay_order", 6_000_000),
        ] {
            let seed = seed.checked_add(base).ok_or_else(|| {
                anyhow::anyhow!("aic-learning.model-initialization: seed overflow")
            })?;
            tensors.insert(
                format!("rng.{name}"),
                aic_torch::Generator::new(seed)?.state()?,
            );
        }
        Ok((order, tensors))
    }
    pub fn load(
        config: &ModelConfig,
        order: &[String],
        values: &Tensors,
        device: Device,
    ) -> Result<Self> {
        ensure!(
            config.width > 0 && config.width <= 4096 && config.message_rounds <= 32,
            "aic-learning.model-config: invalid width or message rounds"
        );
        let w = config.width;
        let mut layers = vec![
            ("nodes.0".to_owned(), config.node_features, w),
            ("edges.0".to_owned(), config.edge_features, w),
            ("paths.0".to_owned(), 2, w),
            ("context.0".to_owned(), w + config.global_features, w),
            ("actor.0".to_owned(), w * 3 + config.action_features, w),
            ("actor.2".to_owned(), w, 1),
            ("critic.0".to_owned(), w, w),
            ("critic.2".to_owned(), w, 1),
            (
                "port_actor.0".to_owned(),
                w * 3 + config.action_features + config.port_features * 2,
                w,
            ),
            ("port_actor.2".to_owned(), w, 1),
        ];
        for i in 0..config.message_rounds {
            layers.push((format!("messages.{i}"), w * 3, w));
            layers.push((format!("updates.{i}"), w * 2, w));
        }
        let mut expected = BTreeSet::new();
        for (name, input, output) in layers {
            for (suffix, shape) in [("weight", vec![output, input]), ("bias", vec![output])] {
                let name = format!("{name}.{suffix}");
                let t = tensor(values, &format!("model.{name}"))?;
                ensure!(
                    input > 0
                        && output > 0
                        && t.kind() == Kind::Float
                        && t.size() == shape
                        && bool::try_from(t.isfinite().all())?,
                    "aic-learning.model-parameter: {name}"
                );
                expected.insert(name);
            }
        }
        ensure!(
            order.len() == expected.len()
                && order.iter().cloned().collect::<BTreeSet<_>>() == expected,
            "aic-learning.model-order: missing, duplicate or unknown parameter"
        );
        let parameters = order
            .iter()
            .map(|name| {
                Ok((
                    name.clone(),
                    tensor(values, &format!("model.{name}"))?
                        .to_device(device)
                        .copy()
                        .set_requires_grad(true),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let weights = parameters
            .iter()
            .map(|(name, t)| (name.clone(), t.shallow_clone()))
            .collect();
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
    fn encode(&self, b: &Tensors, graphs: i64) -> (Tensor, Tensor) {
        let mut nodes = self
            .linear(
                "nodes.0",
                &Tensor::cat(&[&b["node_static"], &b["node_state"]], -1),
            )
            .tanh();
        let edges = self
            .linear(
                "edges.0",
                &Tensor::cat(&[&b["edge_static"], &b["edge_state"]], -1),
            )
            .tanh();
        let route = Tensor::zeros_like(&edges).index_add_(
            0,
            &b["path_edges"],
            &self.linear("paths.0", &b["path_cells"]).tanh(),
        );
        let edges = edges + route / b["path_counts"].unsqueeze(-1);
        let source = b["edge_index"].get(0);
        let target = b["edge_index"].get(1);
        for i in 0..self.config.message_rounds {
            let forward = self
                .linear(
                    &format!("messages.{i}"),
                    &Tensor::cat(
                        &[
                            nodes.index(&[Some(&source)]),
                            nodes.index(&[Some(&target)]),
                            edges.shallow_clone(),
                        ],
                        -1,
                    ),
                )
                .tanh();
            let reverse = self
                .linear(
                    &format!("messages.{i}"),
                    &Tensor::cat(
                        &[
                            nodes.index(&[Some(&target)]),
                            nodes.index(&[Some(&source)]),
                            -&edges,
                        ],
                        -1,
                    ),
                )
                .tanh();
            let incoming = Tensor::zeros_like(&nodes)
                .index_add_(0, &target, &forward)
                .index_add_(0, &source, &reverse);
            nodes = self
                .linear(
                    &format!("updates.{i}"),
                    &Tensor::cat(&[nodes, incoming], -1),
                )
                .tanh();
        }
        let pooled = Tensor::zeros([graphs, self.config.width], (Kind::Float, nodes.device()))
            .index_add_(0, &b["node_graph"], &nodes)
            / b["node_counts"].unsqueeze(-1);
        let pooled = self
            .linear(
                "context.0",
                &Tensor::cat(&[&pooled, &b["global_features"]], -1),
            )
            .tanh();
        (nodes, pooled)
    }
    pub fn roots(&self, prepared: &Packed) -> RootPrediction {
        let b = &prepared.graph;
        let (nodes, pooled) = self.encode(b, prepared.graphs);
        let refs = &b["action_references"];
        let context = pooled.index(&[Some(&b["action_graph"])]);
        let selected = masked_references(&nodes, refs, &context);
        let features = Tensor::cat(
            &[
                selected.flatten(1, -1),
                context,
                b["action_features"].shallow_clone(),
            ],
            -1,
        );
        let mut logits = self
            .linear("actor.2", &self.linear("actor.0", &features).tanh())
            .squeeze_dim(-1);
        if let Some(mask) = b.get("root_invalid") {
            logits = logits.masked_fill(&mask.to_kind(Kind::Bool), -1e9);
        }
        let values = self
            .linear("critic.2", &self.linear("critic.0", &pooled).tanh())
            .squeeze_dim(-1);
        let (root_logp, entropy) = distribution(&logits, b, prepared.graphs);
        RootPrediction {
            nodes,
            pooled,
            root_logp,
            entropy,
            values,
        }
    }
    pub fn ports(&self, nodes: &Tensor, pooled: &Tensor, branches: &Tensors) -> Tensor {
        let count = branches["features"].size()[0];
        let refs = nodes
            .index(&[Some(&branches["references"])])
            .view([count, -1]);
        let port_features = Tensor::cat(
            &[
                refs,
                pooled.index(&[Some(&branches["owner"])]),
                branches["root_features"].shallow_clone(),
                branches["features"].shallow_clone(),
                branches["history"].shallow_clone(),
            ],
            -1,
        );
        let port_logits = self
            .linear(
                "port_actor.2",
                &self.linear("port_actor.0", &port_features).tanh(),
            )
            .squeeze_dim(-1);
        port_logits
    }
    pub fn predict(&self, prepared: &Packed) -> Result<Prediction> {
        let RootPrediction {
            nodes,
            pooled,
            root_logp,
            mut entropy,
            values,
        } = self.roots(prepared);
        let mut logp = root_logp.index(&[Some(&prepared.roots)]);
        if let Some(branches) = &prepared.branches {
            let port_logits = self.ports(&nodes, &pooled, branches);
            let (conditional, port_entropy) =
                distribution(&port_logits, branches, prepared.factors);
            logp = logp.index_add(
                0,
                &branches["factor_owners"],
                &conditional.index(&[Some(&prepared.selected)]),
            );
            entropy = entropy.index_add(0, &branches["factor_owners"], &port_entropy);
        }
        ensure!(
            bool::try_from(
                logp.isfinite()
                    .all()
                    .logical_and(&values.isfinite().all())
                    .logical_and(&entropy.isfinite().all())
            )?,
            "aic-learning.forward: nonfinite prediction"
        );
        Ok(Prediction {
            logp,
            entropy,
            values,
        })
    }
}

/// Missing references use graph context, so their node gradient is zero.
/// Spread these masked destinations across the node bank: clamping them all to
/// zero makes deterministic indexed backward serially accumulate huge runs of
/// zeros at node 0. Valid references and their accumulation order stay intact.
fn masked_references(nodes: &Tensor, refs: &Tensor, context: &Tensor) -> Tensor {
    let valid = refs.ge(0);
    let placeholders = Tensor::arange(refs.numel() as i64, (refs.kind(), refs.device()))
        .remainder(nodes.size()[0])
        .view(refs.size().as_slice());
    let indices = refs.where_self(&valid, &placeholders);
    nodes
        .index(&[Some(&indices)])
        .where_self(&valid.unsqueeze(-1), &context.unsqueeze(1))
}

pub struct RootPrediction {
    pub nodes: Tensor,
    pub pooled: Tensor,
    pub root_logp: Tensor,
    pub entropy: Tensor,
    pub values: Tensor,
}

pub struct Prediction {
    pub logp: Tensor,
    pub entropy: Tensor,
    pub values: Tensor,
}

pub(crate) fn distribution(logits: &Tensor, b: &Tensors, count: i64) -> (Tensor, Tensor) {
    let adjusted = logits - &b["kind_log_counts"];
    let graph = &b["action_graph"];
    let maximum = Tensor::full([count], f64::NEG_INFINITY, (logits.kind(), logits.device()))
        .internal_scatter_reduce(0, graph, &adjusted, "amax", true);
    let shifted = &adjusted - maximum.index(&[Some(graph)]);
    let denominator = Tensor::zeros_like(&maximum).index_add_(0, graph, &shifted.exp());
    let logp = shifted - denominator.log().index(&[Some(graph)]);
    let entropy = Tensor::zeros_like(&maximum).index_add_(0, graph, &(-logp.exp() * &logp));
    (logp, entropy)
}
