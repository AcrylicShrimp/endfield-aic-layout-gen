//! Pack native arrays once per logical batch, retaining original graph ordering.
use crate::input::{Packed, Tensors};
use aic_pilot_native::learning::{Observation, schema::*};
use aic_torch::tch::{Device, Kind, Tensor};
use anyhow::{Result, ensure};

pub fn graph(rows: &[&Observation], device: Device) -> Result<Packed> {
    graph_inputs(
        &rows.iter().map(|r| r.input.as_ref()).collect::<Vec<_>>(),
        device,
    )
    .map_err(|e| {
        anyhow::anyhow!(
            "{e}; cases {:?}",
            rows.iter()
                .map(|r| r.blueprint_sha256.as_str())
                .collect::<Vec<_>>()
        )
    })
}

pub fn graph_inputs(rows: &[&aic_pilot_native::learning::Input], device: Device) -> Result<Packed> {
    ensure!(
        !rows.is_empty(),
        "aic-learning.collection-batch: empty input"
    );
    let mut g = Tensors::new();
    macro_rules! matrix {
        ($name:ident, $n:expr) => {{
            let mut data =
                Vec::with_capacity(rows.iter().map(|o| o.$name.len() * $n as usize).sum());
            for o in rows {
                data.extend_from_slice(o.$name.as_flattened());
            }
            let length = data.len() as i64 / $n;
            g.insert(
                stringify!($name).into(),
                Tensor::from_slice(&data)
                    .view([length, $n])
                    .to_device(device),
            );
        }};
    }
    matrix!(node_static, NODE_STATIC as i64);
    matrix!(node_state, NODE_STATE as i64);
    matrix!(edge_static, EDGE_STATIC as i64);
    matrix!(edge_state, EDGE_STATE as i64);
    matrix!(path_cells, PATH_FEATURES as i64);
    matrix!(action_features, ACTION_FEATURES as i64);
    let mut edge_index = [Vec::new(), Vec::new()];
    let (mut path_edges, mut refs, mut nodes, mut actions, mut node_counts, mut path_counts) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    let (mut globals, mut logs, mut masks) = (vec![], vec![], vec![]);
    let (mut no, mut eo) = (0i64, 0i64);
    for (id, row) in rows.iter().enumerate() {
        let v = row;
        for [s, t] in &v.edge_index {
            edge_index[0].push(s + no);
            edge_index[1].push(t + no);
        }
        path_edges.extend(v.path_edges.iter().map(|e| e + eo));
        refs.extend(
            v.action_references
                .iter()
                .flat_map(|r| r.map(|n| if n < 0 { n } else { n + no })),
        );
        nodes.extend(std::iter::repeat_n(id as i64, v.node_static.len()));
        actions.extend(std::iter::repeat_n(id as i64, v.action_features.len()));
        node_counts.push(v.node_static.len().max(1) as i64);
        let mut counts = vec![0i64; v.edge_index.len()];
        for &e in &v.path_edges {
            counts[e as usize] += 1;
        }
        path_counts.extend(counts.into_iter().map(|n| n.max(1)));
        globals.extend_from_slice(&v.global_features);
        logs.extend_from_slice(&v.kind_log_counts);
        let mut available = 0;
        for &invalid in &v.root_invalid {
            masks.push(invalid);
            available += usize::from(!invalid);
        }
        ensure!(
            available > 0,
            "aic-learning.empty-actions: graph {id} has no available root"
        );
        no += v.node_static.len() as i64;
        eo += v.edge_index.len() as i64;
    }
    let count = rows.len() as i64;
    g.insert(
        "global_features".into(),
        Tensor::from_slice(&globals)
            .view([count, -1])
            .to_device(device),
    );
    g.insert(
        "kind_log_counts".into(),
        Tensor::from_slice(&logs).to_device(device),
    );
    g.insert(
        "root_invalid".into(),
        Tensor::from_slice(&masks).to_device(device),
    );
    let edges: Vec<i64> = edge_index.into_iter().flatten().collect();
    g.insert(
        "edge_index".into(),
        Tensor::from_slice(&edges).view([2, eo]).to_device(device),
    );
    g.insert(
        "action_references".into(),
        Tensor::from_slice(&refs).view([-1, 2]).to_device(device),
    );
    for (n, v) in [
        ("node_graph", nodes),
        ("action_graph", actions),
        ("path_edges", path_edges),
        ("node_counts", node_counts),
        ("path_counts", path_counts),
    ] {
        g.insert(n.into(), Tensor::from_slice(&v).to_device(device));
    }
    Ok(Packed {
        graph: g,
        branches: None,
        roots: Tensor::zeros([count], (Kind::Int64, device)),
        selected: Tensor::zeros([0], (Kind::Int64, device)),
        graphs: count,
        factors: 0,
    })
}

#[cfg(test)]
thread_local! { static INSERTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(crate) fn take_insertions() -> usize {
    INSERTIONS.with(|n| n.replace(0))
}

/// Owned physical binding. Embeddings are recomputed with current parameters.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Assignment {
    pub terminal: i64,
    pub port: [f32; PORT_FEATURES],
}

#[derive(Default)]
pub struct Prefix<'a> {
    pub assignments: Vec<Assignment>,
    used: std::collections::HashSet<&'a str>,
}
impl<'a> Prefix<'a> {
    pub fn append(
        &mut self,
        row: &'a Observation,
        root: usize,
        branch: &Branch,
        selected: usize,
    ) -> Result<()> {
        let meta = row.input.root_meta[root];
        let depth = self.assignments.len();
        ensure!(
            branch.depth == depth,
            "aic-learning.port-prefix: stale depth"
        );
        let group = &row.domain.groups[meta[1] as usize][depth];
        let choice = *branch
            .keep
            .get(selected)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.port-prefix: invalid selection"))?;
        #[cfg(test)]
        INSERTIONS.with(|n| n.set(n.get() + 1));
        ensure!(
            self.used.insert(group.ports[choice].as_str()),
            "aic-learning.port-prefix: duplicate port"
        );
        self.assignments.push(Assignment {
            terminal: group.graph_node as i64,
            port: branch.features[selected],
        });
        Ok(())
    }
}

/// A factor's compatible alternatives and range [0, depth) in its owner's bank.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Branch {
    pub features: Vec<[f32; PORT_FEATURES]>,
    pub depth: usize,
    pub root_features: [f32; ACTION_FEATURES],
    pub references: [i64; 2],
    pub owner: usize,
    pub keep: Vec<usize>,
}
pub fn branch(row: &Observation, owner: usize, root: usize, prefix: &Prefix<'_>) -> Result<Branch> {
    let meta = *row
        .input
        .root_meta
        .get(root)
        .ok_or_else(|| anyhow::anyhow!("aic-learning.port-root: invalid root"))?;
    ensure!(meta[0] == 0, "aic-learning.port-root: expected placement");
    let depth = prefix.assignments.len();
    let groups = &row.domain.groups[meta[1] as usize];
    ensure!(
        depth < groups.len(),
        "aic-learning.port-prefix: invalid depth"
    );
    let group = &groups[depth];
    let keep: Vec<_> = group
        .ports
        .iter()
        .enumerate()
        .filter(|(_, p)| !prefix.used.contains(p.as_str()))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        !keep.is_empty(),
        "aic-learning.port-domain: empty compatible domain"
    );
    let start = group.features[&meta[2]].0;
    Ok(Branch {
        features: keep
            .iter()
            .map(|i| row.input.port_features[start + i])
            .collect(),
        depth,
        root_features: row.input.action_features[root],
        references: [
            row.input.action_references[root][0],
            group.graph_node as i64,
        ],
        owner,
        keep,
    })
}
pub fn branches(rows: &[Branch], offsets: &[i64], device: Device) -> Tensors {
    let mut g = Tensors::new();
    let (mut features, mut roots, mut references, mut owners, mut groups) =
        (vec![], vec![], vec![], vec![], vec![]);
    for (i, row) in rows.iter().enumerate() {
        for f in &row.features {
            features.extend_from_slice(f);
            roots.extend_from_slice(&row.root_features);
            references.extend(row.references.map(|n| n + offsets[row.owner]));
            owners.push(row.owner as i64);
            groups.push(i as i64);
        }
    }
    let n = owners.len() as i64;
    for (name, data, width) in [
        ("features", features, PORT_FEATURES as i64),
        ("root_features", roots, ACTION_FEATURES as i64),
    ] {
        g.insert(
            name.into(),
            Tensor::from_slice(&data).view([n, width]).to_device(device),
        );
    }
    g.insert(
        "references".into(),
        Tensor::from_slice(&references)
            .view([n, 2])
            .to_device(device),
    );
    g.insert(
        "owner".into(),
        Tensor::from_slice(&owners).to_device(device),
    );
    g.insert(
        "action_graph".into(),
        Tensor::from_slice(&groups).to_device(device),
    );
    g.insert(
        "kind_log_counts".into(),
        Tensor::zeros([n], (Kind::Float, device)),
    );
    g.insert(
        "factor_owners".into(),
        Tensor::from_slice(&rows.iter().map(|r| r.owner as i64).collect::<Vec<_>>())
            .to_device(device),
    );
    g
}
/// Pack each assignment once, with factor prefix bounds independent of alternatives.
pub fn assignments(
    g: &mut Tensors,
    rows: &[Branch],
    banks: &[&[Assignment]],
    offsets: &[i64],
    device: Device,
) {
    let (mut terminals, mut ports, mut owners, mut starts) = (vec![], vec![], vec![], vec![]);
    for (owner, bank) in banks.iter().enumerate() {
        starts.push(terminals.len() as i64);
        for pair in *bank {
            terminals.push(pair.terminal + offsets[owner]);
            ports.extend_from_slice(&pair.port);
            owners.push(owner as i64);
        }
    }
    let n = terminals.len() as i64;
    g.insert(
        "assignment_ports".into(),
        Tensor::from_slice(&ports)
            .view([n, PORT_FEATURES as i64])
            .to_device(device),
    );
    for (name, data) in [
        ("assignment_terminals", terminals),
        ("assignment_owners", owners),
        (
            "prefix_start",
            rows.iter().map(|r| starts[r.owner]).collect(),
        ),
        (
            "prefix_end",
            rows.iter()
                .map(|r| starts[r.owner] + r.depth as i64)
                .collect(),
        ),
    ] {
        g.insert(name.into(), Tensor::from_slice(&data).to_device(device));
    }
}
pub fn offsets(rows: &[&Observation]) -> Vec<i64> {
    let mut n = 0;
    rows.iter()
        .map(|r| {
            let start = n;
            n += r.input.node_static.len() as i64;
            start
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_pilot_native::learning::{Domain, Input, PortGroup};
    use std::sync::Arc;

    #[test]
    fn multiport_prefix_retains_bindings_and_excludes_used_names() {
        let input = Input {
            pending: 0,
            conflicts: 0,
            node_static: vec![],
            node_state: vec![],
            edge_index: vec![],
            edge_static: vec![],
            edge_state: vec![],
            path_cells: vec![],
            path_edges: vec![],
            global_features: vec![],
            action_features: vec![[0.; 12]],
            action_references: vec![[0, -1]],
            root_meta: vec![[0, 0, 90]],
            kind_log_counts: vec![0.],
            root_invalid: vec![false],
            port_features: vec![
                [2.; PORT_FEATURES],
                [4.; PORT_FEATURES],
                [8.; PORT_FEATURES],
            ],
        };
        let row = Observation {
            input: Arc::new(input),
            domain: Arc::new(Domain {
                groups: vec![
                    (0..3)
                        .map(|i| PortGroup {
                            graph_node: i + 1,
                            equivalence: i,
                            port_equivalences: vec![0, 1, 2],
                            ports: vec!["a".into(), "b".into(), "c".into()],
                            features: [(90, (0, 3))].into_iter().collect(),
                        })
                        .collect(),
                ],
            }),
            handle: 1,
            blueprint_sha256: String::new(),
            revision: 0,
            attempts: 0,
            done: false,
        };
        let mut prefix = Prefix::default();
        let first = branch(&row, 0, 0, &prefix).unwrap();
        assert_eq!(first.depth, 0);
        assert!(prefix.append(&row, 0, &first, 3).is_err());
        prefix.append(&row, 0, &first, 1).unwrap();
        assert!(prefix.append(&row, 0, &first, 0).is_err());
        let second = branch(&row, 0, 0, &prefix).unwrap();
        assert_eq!(second.keep, vec![0, 2]);
        prefix.append(&row, 0, &second, 0).unwrap();
        let third = branch(&row, 0, 0, &prefix).unwrap();
        assert_eq!(third.depth, 2);
        assert_eq!(third.keep, vec![2]);
        assert_eq!(
            prefix.assignments,
            vec![
                Assignment {
                    terminal: 1,
                    port: [4.; PORT_FEATURES]
                },
                Assignment {
                    terminal: 2,
                    port: [2.; PORT_FEATURES]
                }
            ]
        );
        assert_eq!(prefix.used.len(), 2);
        prefix.append(&row, 0, &third, 0).unwrap();
        assert_eq!(prefix.assignments.len(), 3);
        assert!(branch(&row, 0, 0, &prefix).is_err());
    }
}
