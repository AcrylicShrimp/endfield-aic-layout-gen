//! Validate packed ownership and shapes once on the CPU before device transfer.
use super::{Tensors, tensor};
use crate::schema::ModelConfig;
use aic_pilot_native::learning::schema::*;
use aic_torch::tch::{Kind, Tensor};
use anyhow::{Result, ensure};

fn field<'a>(v: &'a Tensors, n: &str, kind: Kind, shape: &[i64]) -> Result<&'a Tensor> {
    let t = tensor(v, n)?;
    ensure!(
        t.kind() == kind && t.size() == shape,
        "aic-learning.input-shape: {n}, expected {kind:?} {shape:?}, got {:?} {:?}",
        t.kind(),
        t.size()
    );
    if kind == Kind::Float {
        ensure!(
            bool::try_from(t.isfinite().all())?,
            "aic-learning.input-finite: {n}"
        );
    }
    Ok(t)
}
fn indices(t: &Tensor, low: i64, high: i64, name: &str) -> Result<()> {
    ensure!(
        bool::try_from(t.ge(low).logical_and(&t.lt(high)).all())?,
        "aic-learning.input-index: {name} outside [{low}, {high})"
    );
    Ok(())
}
fn groups(t: &Tensor, count: i64, name: &str) -> Result<Tensor> {
    indices(t, 0, count, name)?;
    let counts = t.bincount::<&Tensor>(None, count);
    ensure!(
        bool::try_from(counts.gt(0).all())?,
        "aic-learning.input-group: empty {name}"
    );
    let list = Vec::<i64>::try_from(t)?;
    ensure!(
        list.windows(2).all(|w| w[0] <= w[1]),
        "aic-learning.input-group: unordered {name}"
    );
    Ok(counts)
}
pub(super) fn packed(
    v: &Tensors,
    prefix: &str,
    graphs: i64,
    factors: i64,
    c: &ModelConfig,
) -> Result<()> {
    let b = |n: &str| format!("{prefix}.batch.{n}");
    let get = |n: &str| tensor(v, &b(n));
    for n in [
        "node_static",
        "node_state",
        "edge_static",
        "edge_state",
        "action_features",
        "path_cells",
    ] {
        ensure!(get(n)?.dim() == 2, "aic-learning.input-rank: {}", b(n));
    }
    let nodes = get("node_static")?.size()[0];
    let edges = get("edge_static")?.size()[0];
    let actions = get("action_features")?.size()[0];
    let paths = get("path_cells")?.size()[0];
    ensure!(
        nodes > 0 && actions > 0,
        "aic-learning.input-empty: {prefix}"
    );
    for (name, rows, width) in [
        ("node_static", nodes, NODE_STATIC),
        ("node_state", nodes, NODE_STATE),
        ("edge_static", edges, EDGE_STATIC),
        ("edge_state", edges, EDGE_STATE),
    ] {
        field(v, &b(name), Kind::Float, &[rows, width as i64])?;
    }
    field(
        v,
        &b("action_features"),
        Kind::Float,
        &[actions, c.action_features],
    )?;
    field(
        v,
        &b("global_features"),
        Kind::Float,
        &[graphs, c.global_features],
    )?;
    field(
        v,
        &b("path_cells"),
        Kind::Float,
        &[paths, PATH_FEATURES as i64],
    )?;
    let ng = field(v, &b("node_graph"), Kind::Int64, &[nodes])?;
    let nc = groups(ng, graphs, &b("node_graph"))?;
    ensure!(
        field(v, &b("node_counts"), Kind::Int64, &[graphs])?.equal(&nc),
        "aic-learning.input-counts: {}",
        b("node_counts")
    );
    let ei = field(v, &b("edge_index"), Kind::Int64, &[2, edges])?;
    indices(ei, 0, nodes, &b("edge_index"))?;
    ensure!(
        ng.index_select(0, &ei.get(0))
            .equal(&ng.index_select(0, &ei.get(1))),
        "aic-learning.input-owner: cross-graph edge in {prefix}"
    );
    let pe = field(v, &b("path_edges"), Kind::Int64, &[paths])?;
    indices(pe, 0, edges, &b("path_edges"))?;
    let pc = pe.bincount::<&Tensor>(None, edges).clamp_min(1);
    ensure!(
        field(v, &b("path_counts"), Kind::Int64, &[edges])?.equal(&pc),
        "aic-learning.input-counts: {}",
        b("path_counts")
    );
    let ag = field(v, &b("action_graph"), Kind::Int64, &[actions])?;
    let _ = groups(ag, graphs, &b("action_graph"))?;
    field(v, &b("kind_log_counts"), Kind::Float, &[actions])?;
    let refs = field(v, &b("action_references"), Kind::Int64, &[actions, 2])?;
    indices(refs, -1, nodes, &b("action_references"))?;
    let owners = ng
        .index_select(0, &refs.clamp_min(0).view([-1]))
        .view([actions, 2]);
    ensure!(
        bool::try_from(
            owners
                .eq_tensor(&ag.unsqueeze(-1))
                .logical_or(&refs.eq(-1))
                .all()
        )?,
        "aic-learning.input-owner: action reference in {prefix}"
    );
    let roots = field(v, &format!("{prefix}.roots"), Kind::Int64, &[graphs])?;
    indices(roots, 0, actions, &format!("{prefix}.roots"))?;
    ensure!(
        ag.index_select(0, roots)
            .equal(&Tensor::arange(graphs, (Kind::Int64, ng.device()))),
        "aic-learning.input-owner: selected roots in {prefix}"
    );
    if v.contains_key(&b("root_invalid")) {
        let mask = field(v, &b("root_invalid"), Kind::Bool, &[actions])?;
        ensure!(
            !bool::try_from(mask.index_select(0, roots).any())?,
            "aic-learning.input-mask: selected invalid root in {prefix}"
        );
    }
    let selected = field(v, &format!("{prefix}.selected"), Kind::Int64, &[factors])?;
    if factors == 0 {
        return Ok(());
    }
    let q = |n: &str| format!("{prefix}.branches.{n}");
    let features = tensor(v, &q("features"))?;
    ensure!(
        features.dim() == 2,
        "aic-learning.input-rank: {}",
        q("features")
    );
    let candidates = features.size()[0];
    for (n, width) in [
        ("features", c.port_features),
        ("root_features", c.action_features),
    ] {
        field(v, &q(n), Kind::Float, &[candidates, width])?;
    }
    let bg = field(v, &q("action_graph"), Kind::Int64, &[candidates])?;
    let _ = groups(bg, factors, &q("action_graph"))?;
    field(v, &q("kind_log_counts"), Kind::Float, &[candidates])?;
    let owners = field(v, &q("owner"), Kind::Int64, &[candidates])?;
    indices(owners, 0, graphs, &q("owner"))?;
    let fo = field(v, &q("factor_owners"), Kind::Int64, &[factors])?;
    indices(fo, 0, graphs, &q("factor_owners"))?;
    ensure!(
        owners.equal(&fo.index_select(0, bg)),
        "aic-learning.input-owner: factors in {prefix}"
    );
    let refs = field(v, &q("references"), Kind::Int64, &[candidates, 2])?;
    indices(refs, 0, nodes, &q("references"))?;
    ensure!(
        bool::try_from(
            ng.index_select(0, &refs.view([-1]))
                .view([candidates, 2])
                .eq_tensor(&owners.unsqueeze(-1))
                .all()
        )?,
        "aic-learning.input-owner: ports in {prefix}"
    );
    let assignment_ports = field(
        v,
        &q("assignment_ports"),
        Kind::Float,
        &[factors, c.port_features],
    )?;
    let _ = assignment_ports;
    let terminals = field(v, &q("assignment_terminals"), Kind::Int64, &[factors])?;
    indices(terminals, 0, nodes, &q("assignment_terminals"))?;
    let assignment_owners = field(v, &q("assignment_owners"), Kind::Int64, &[factors])?;
    indices(assignment_owners, 0, graphs, &q("assignment_owners"))?;
    ensure!(
        ng.index_select(0, terminals).equal(assignment_owners),
        "aic-learning.input-owner: assignments in {prefix}"
    );
    let start = field(v, &q("prefix_start"), Kind::Int64, &[factors])?;
    let end = field(v, &q("prefix_end"), Kind::Int64, &[factors])?;
    indices(start, 0, factors + 1, &q("prefix_start"))?;
    indices(end, 0, factors + 1, &q("prefix_end"))?;
    let assignments = Vec::<i64>::try_from(assignment_owners)?;
    let starts = Vec::<i64>::try_from(start)?;
    let ends = Vec::<i64>::try_from(end)?;
    let factor_owners = Vec::<i64>::try_from(fo)?;
    let mut offset = 0;
    for owner in 0..graphs {
        let count = assignments.iter().filter(|&&id| id == owner).count();
        ensure!(
            assignments[offset..offset + count]
                .iter()
                .all(|&id| id == owner),
            "aic-learning.input-owner: unordered assignment bank in {prefix}"
        );
        let factors: Vec<_> = factor_owners
            .iter()
            .enumerate()
            .filter(|(_, id)| **id == owner)
            .map(|(i, _)| i)
            .collect();
        ensure!(
            count == factors.len(),
            "aic-learning.input-counts: assignment bank in {prefix}"
        );
        for (depth, factor) in factors.into_iter().enumerate() {
            ensure!(
                starts[factor] == offset as i64 && ends[factor] == (offset + depth) as i64,
                "aic-learning.input-index: prefix range in {prefix}"
            );
        }
        offset += count;
    }
    indices(selected, 0, candidates, &format!("{prefix}.selected"))?;
    ensure!(
        bg.index_select(0, selected)
            .equal(&Tensor::arange(factors, (Kind::Int64, ng.device()))),
        "aic-learning.input-owner: selected factors in {prefix}"
    );
    Ok(())
}
