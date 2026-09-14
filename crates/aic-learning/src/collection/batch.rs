//! Pack native arrays once per logical batch, retaining original graph ordering.
use crate::input::{Packed, Tensors};
use aic_pilot_native::learning::Observation;
use aic_torch::tch::{Device, Kind, Tensor};
use anyhow::{Result, ensure};

pub fn graph(rows: &[&Observation], device: Device) -> Result<Packed> {
    graph_inputs(
        &rows.iter().map(|r| r.input.as_ref()).collect::<Vec<_>>(),
        device,
    )
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
    matrix!(node_static, 14);
    matrix!(node_state, 10);
    matrix!(edge_static, 10);
    matrix!(edge_state, 4);
    matrix!(path_cells, 2);
    matrix!(action_features, 12);
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
        masks.extend_from_slice(&v.root_invalid);
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

/// A selected prefix's compatible alternatives, in original port order.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Branch {
    pub features: Vec<[f32; 12]>,
    pub history: [f32; 12],
    pub root_features: [f32; 12],
    pub references: [i64; 2],
    pub owner: usize,
    pub keep: Vec<usize>,
}
pub fn branch(
    row: &Observation,
    owner: usize,
    root: usize,
    choices: &[usize],
    depth: usize,
) -> Result<Branch> {
    let meta = row.input.root_meta[root];
    ensure!(meta[0] == 0, "aic-learning.port-root: expected placement");
    let facility = meta[1] as usize;
    let groups = &row.domain.groups[facility];
    ensure!(
        depth < groups.len() && choices.len() >= depth,
        "aic-learning.port-prefix: invalid depth"
    );
    let mut used = std::collections::BTreeSet::new();
    let mut history = [0f32; 12];
    for (group, &choice) in groups[..depth].iter().zip(choices) {
        ensure!(
            choice < group.ports.len() && used.insert(group.ports[choice].as_str()),
            "aic-learning.port-prefix: duplicate or invalid port"
        );
        let start = group.features[&meta[2]].0;
        for (h, f) in history
            .iter_mut()
            .zip(row.input.port_features[start + choice])
        {
            *h += f;
        }
    }
    // The established head receives the mean previous-port feature, not its sum.
    for value in &mut history {
        *value /= depth.max(1) as f32;
    }
    let group = &groups[depth];
    let keep: Vec<_> = group
        .ports
        .iter()
        .enumerate()
        .filter(|(_, p)| !used.contains(p.as_str()))
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
        history,
        root_features: row.input.action_features[root],
        references: [facility as i64, group.graph_node as i64],
        owner,
        keep,
    })
}
pub fn branches(rows: &[Branch], offsets: &[i64], device: Device) -> Tensors {
    let mut g = Tensors::new();
    let (mut features, mut history, mut roots, mut references, mut owners, mut groups) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    for (i, row) in rows.iter().enumerate() {
        for f in &row.features {
            features.extend_from_slice(f);
            history.extend_from_slice(&row.history);
            roots.extend_from_slice(&row.root_features);
            references.extend(row.references.map(|n| n + offsets[row.owner]));
            owners.push(row.owner as i64);
            groups.push(i as i64);
        }
    }
    let n = owners.len() as i64;
    for (name, data, width) in [
        ("features", features, 12),
        ("history", history, 12),
        ("root_features", roots, 12),
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
    fn multiport_prefix_uses_mean_and_excludes_used_names() {
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
            port_features: vec![[2.; 12], [4.; 12], [8.; 12]],
        };
        let row = Observation {
            input: Arc::new(input),
            domain: Arc::new(Domain {
                groups: vec![
                    (0..3)
                        .map(|i| PortGroup {
                            graph_node: i + 1,
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
        let prefix = branch(&row, 0, 0, &[0, 1], 2).unwrap();
        assert_eq!(prefix.history, [3.; 12]);
        assert_eq!(prefix.keep, vec![2]);
        assert_eq!(prefix.features, vec![[8.; 12]]);
        assert!(branch(&row, 0, 0, &[0, 0], 2).is_err());
        assert_eq!(branch(&row, 0, 0, &[], 0).unwrap().history, [0.; 12]);
    }
}
