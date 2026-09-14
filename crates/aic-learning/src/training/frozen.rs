//! Immutable selected likelihood inputs, shared in memory and sparse on disk.
use crate::{
    collection::{batch, policy::Choice},
    input::Packed,
};
use aic_pilot_native::learning::{Input, Observation};
use aic_torch::tch::{Device, Tensor};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
pub struct Frozen {
    pub input: Arc<Input>,
    pub root: usize,
    pub branches: Vec<batch::Branch>,
    pub selected: Vec<usize>,
}
impl Frozen {
    pub fn record(o: &Observation, choice: &Choice) -> Result<Self> {
        let mut branches = Vec::with_capacity(choice.ports.len());
        let mut selected = Vec::with_capacity(choice.ports.len());
        for (depth, port) in choice.ports.iter().enumerate() {
            let branch = batch::branch(o, 0, choice.root, &choice.ports, depth)?;
            selected.push(branch.keep.iter().position(|p| p == port).ok_or_else(|| {
                anyhow::anyhow!("aic-learning.frozen-port: excluded recorded selection")
            })?);
            branches.push(branch);
        }
        Ok(Self {
            input: o.input.clone(),
            root: choice.root,
            branches,
            selected,
        })
    }
    /// Only admitted/active replay needs an independent compact numerical bank.
    /// Branch alternatives were frozen once; unused roots' port tables are gone.
    pub fn compact(&self) -> Self {
        let v = &self.input;
        let input = Input {
            node_static: v.node_static.clone(),
            node_state: v.node_state.clone(),
            edge_index: v.edge_index.clone(),
            edge_static: v.edge_static.clone(),
            edge_state: v.edge_state.clone(),
            path_cells: v.path_cells.clone(),
            path_edges: v.path_edges.clone(),
            global_features: v.global_features.clone(),
            action_features: v.action_features.clone(),
            action_references: v.action_references.clone(),
            kind_log_counts: v.kind_log_counts.clone(),
            root_invalid: v.root_invalid.clone(),
            ..Input::default()
        };
        Self {
            input: Arc::new(input),
            root: self.root,
            branches: self.branches.clone(),
            selected: self.selected.clone(),
        }
    }
    pub fn bytes(&self) -> u64 {
        let v = &self.input;
        (v.node_static.len() * 56
            + v.node_state.len() * 40
            + v.edge_index.len() * 16
            + v.edge_static.len() * 40
            + v.edge_state.len() * 16
            + v.path_cells.len() * 8
            + v.path_edges.len() * 8
            + v.global_features.len() * 4
            + v.action_features.len() * 48
            + v.action_references.len() * 16
            + v.kind_log_counts.len() * 4
            + v.root_invalid.len()
            + v.port_features.len() * 48
            + v.root_meta.len() * 24
            + self
                .branches
                .iter()
                .map(|b| b.features.len() * 48 + b.keep.len() * 8 + 128)
                .sum::<usize>()) as u64
    }
}

pub fn pack(rows: &[&Frozen]) -> Result<Packed> {
    ensure!(!rows.is_empty(), "aic-learning.frozen-batch: empty batch");
    let mut packed = batch::graph_inputs(
        &rows.iter().map(|r| r.input.as_ref()).collect::<Vec<_>>(),
        Device::Cpu,
    )?;
    let (mut nodes, mut roots, mut alternatives) = (0, 0, 0);
    let (mut offsets, mut selected_roots, mut selected_ports, mut branches) =
        (vec![], vec![], vec![], vec![]);
    for (owner, row) in rows.iter().enumerate() {
        ensure!(
            row.root < row.input.action_features.len() && row.selected.len() == row.branches.len(),
            "aic-learning.frozen-index: invalid root or branch coverage"
        );
        offsets.push(nodes);
        nodes += row.input.node_static.len() as i64;
        selected_roots.push(roots + row.root as i64);
        roots += row.input.action_features.len() as i64;
        for (branch, &selected) in row.branches.iter().zip(&row.selected) {
            ensure!(
                selected < branch.features.len(),
                "aic-learning.frozen-index: invalid port"
            );
            selected_ports.push(alternatives + selected as i64);
            alternatives += branch.features.len() as i64;
            let mut branch = branch.clone();
            branch.owner = owner;
            branches.push(branch);
        }
    }
    packed.roots = Tensor::from_slice(&selected_roots);
    packed.selected = Tensor::from_slice(&selected_ports);
    packed.factors = branches.len() as i64;
    if !branches.is_empty() {
        packed.branches = Some(batch::branches(&branches, &offsets, Device::Cpu));
    }
    Ok(packed)
}
