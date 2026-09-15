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
    pub assignments: Vec<batch::Assignment>,
}
impl Frozen {
    pub fn record(o: &Observation, choice: &Choice) -> Result<Self> {
        let mut branches = Vec::with_capacity(choice.ports.len());
        let mut selected = Vec::with_capacity(choice.ports.len());
        let meta = o
            .input
            .root_meta
            .get(choice.root)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.frozen-root: invalid root"))?;
        ensure!(
            !o.input.root_invalid[choice.root],
            "aic-learning.frozen-root: masked root"
        );
        let count = if meta[0] == 0 {
            o.domain.groups[meta[1] as usize].len()
        } else {
            0
        };
        ensure!(
            choice.ports.len() == count,
            "aic-learning.frozen-port: incomplete assignment"
        );
        let mut prefix = batch::Prefix::default();
        for port in &choice.ports {
            let branch = batch::branch(o, 0, choice.root, &prefix)?;
            selected.push(branch.keep.iter().position(|p| p == port).ok_or_else(|| {
                anyhow::anyhow!("aic-learning.frozen-port: excluded recorded selection")
            })?);
            prefix.append(o, choice.root, &branch, *selected.last().unwrap())?;
            branches.push(branch);
        }
        Ok(Self {
            input: o.input.clone(),
            root: choice.root,
            branches,
            selected,
            assignments: prefix.assignments,
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
            assignments: self.assignments.clone(),
        }
    }
    pub fn bytes(&self) -> u64 {
        use aic_pilot_native::learning::schema::row_bytes;
        self.input.bytes(false)
            + std::mem::size_of::<Self>() as u64
            + (row_bytes(&self.branches, false)
                + row_bytes(&self.selected, false)
                + row_bytes(&self.assignments, false)
                + self
                    .branches
                    .iter()
                    .map(|b| row_bytes(&b.features, false) + row_bytes(&b.keep, false))
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
            row.root < row.input.action_features.len()
                && row.selected.len() == row.branches.len()
                && row.assignments.len() == row.branches.len(),
            "aic-learning.frozen-index: invalid root or branch coverage"
        );
        offsets.push(nodes);
        nodes += row.input.node_static.len() as i64;
        selected_roots.push(roots + row.root as i64);
        roots += row.input.action_features.len() as i64;
        for (depth, (branch, &selected)) in row.branches.iter().zip(&row.selected).enumerate() {
            ensure!(
                selected < branch.features.len()
                    && branch.keep.len() == branch.features.len()
                    && branch
                        .references
                        .iter()
                        .all(|&n| n >= 0 && n < row.input.node_static.len() as i64)
                    && branch.depth == depth
                    && row.assignments[depth].terminal == branch.references[1]
                    && row.assignments[depth].port == branch.features[selected],
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
        let mut tensors = batch::branches(&branches, &offsets, Device::Cpu);
        batch::assignments(
            &mut tensors,
            &branches,
            &rows
                .iter()
                .map(|r| r.assignments.as_slice())
                .collect::<Vec<_>>(),
            &offsets,
            Device::Cpu,
        );
        packed.branches = Some(tensors);
    }
    Ok(packed)
}
