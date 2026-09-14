//! Validate packed ownership and shapes once on the CPU before device transfer.
use super::{Tensors, tensor};
use crate::schema::ModelConfig;
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
    for (a, z, rows, width) in [
        ("node_static", "node_state", nodes, c.node_features),
        ("edge_static", "edge_state", edges, c.edge_features),
    ] {
        let x = get(a)?.size()[1];
        field(v, &b(a), Kind::Float, &[rows, x])?;
        field(v, &b(z), Kind::Float, &[rows, width - x])?;
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
    field(v, &b("path_cells"), Kind::Float, &[paths, 2])?;
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
        ("history", c.port_features),
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
    indices(selected, 0, candidates, &format!("{prefix}.selected"))?;
    ensure!(
        bg.index_select(0, selected)
            .equal(&Tensor::arange(factors, (Kind::Int64, ng.device()))),
        "aic-learning.input-owner: selected factors in {prefix}"
    );
    Ok(())
}
