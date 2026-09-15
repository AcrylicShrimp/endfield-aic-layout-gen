//! Admission identities of a typed, directed physical multigraph.
//! Opaque catalog/source names must be resolved to vertices before this boundary.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

pub const SEMANTICS: &str = "aic-physical-identity-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Combined structural and exact search visits, including their roots.
    pub max_individualization_nodes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalIdentity {
    pub semantics: String,
    pub structural: String,
    pub exact_problem: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Kind {
    Facility,
    Port,
    Network,
    Commodity,
    Cell,
    Segment,
    Terminal,
    External,
    Component,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Relation {
    OwnsPort,
    Commodity,
    NetworkCell,
    NetworkSegment,
    NetworkTerminal,
    NetworkComponent,
    SegmentFrom,
    SegmentTo,
    TerminalCell,
    TerminalEndpoint,
}

/// Integer physical attributes in a fixed schema per `kind`, never neural features.
/// Exact-only attributes contain reduced rational pairs and balance constraints.
#[derive(Clone, Debug)]
pub struct Vertex {
    pub kind: Kind,
    pub structural: Vec<i64>,
    pub exact: Vec<i64>,
}
#[derive(Default, Clone, Debug)]
pub struct Graph {
    pub vertices: Vec<Vertex>,
    pub relations: Vec<(usize, Relation, usize)>,
}
impl Graph {
    pub fn vertex(&mut self, kind: Kind, structural: Vec<i64>, exact: Vec<i64>) -> usize {
        let index = self.vertices.len();
        self.vertices.push(Vertex {
            kind,
            structural,
            exact,
        });
        index
    }
    pub fn relation(&mut self, from: usize, kind: Relation, to: usize) {
        self.relations.push((from, kind, to));
    }
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: &'static str,
    pub stage: &'static str,
    pub entity: String,
    pub message: &'static str,
    pub work_used: u64,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}
impl std::error::Error for Diagnostic {}
struct Work<'a> {
    limit: u64,
    used: u64,
    entity: &'a str,
    cancelled: &'a mut dyn FnMut() -> bool,
}
impl Work<'_> {
    fn error(&self, code: &'static str, message: &'static str) -> Diagnostic {
        Diagnostic {
            code,
            severity: "error",
            stage: "physical-identity",
            entity: self.entity.into(),
            message,
            work_used: self.used,
        }
    }
    fn visit(&mut self) -> Result<(), Diagnostic> {
        if (self.cancelled)() {
            return Err(self.error(
                "aic-learning.physical-identity-interrupted",
                "Physical identity canonicalization was interrupted before separation was certified",
            ));
        }
        if self.used >= self.limit {
            return Err(self.error("aic-learning.physical-identity-budget",
                "Combined structural/exact canonicalization budget exhausted; retain input and retry with a larger identity limit"));
        }
        self.used += 1;
        Ok(())
    }
}

/// Returns keys only if BOTH independent canonical searches complete.
pub fn identities(
    graph: &Graph,
    limits: &Limits,
    entity: &str,
) -> Result<(PhysicalIdentity, u64), Diagnostic> {
    identities_until(graph, limits, entity, || false)
}

/// Returns keys and charged visits only when both searches complete before cancellation.
pub fn identities_until(
    graph: &Graph,
    limits: &Limits,
    entity: &str,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(PhysicalIdentity, u64), Diagnostic> {
    let mut work = Work {
        limit: limits.max_individualization_nodes,
        used: 0,
        entity,
        cancelled: &mut cancelled,
    };
    if graph.vertices.is_empty()
        || graph
            .relations
            .iter()
            .any(|&(a, _, b)| a >= graph.vertices.len() || b >= graph.vertices.len())
    {
        return Err(work.error(
            "aic-learning.physical-identity-graph",
            "Empty graph or invalid relation endpoint",
        ));
    }
    let mut adjacency = vec![vec![]; graph.vertices.len()];
    for &(a, relation, b) in &graph.relations {
        adjacency[a].push((relation, false, b));
        adjacency[b].push((relation, true, a));
    }
    let structural = canonical(graph, &adjacency, false, &mut work)?;
    let exact = canonical(graph, &adjacency, true, &mut work)?;
    let digest = |domain: &str, encoding: Vec<u8>| {
        let mut hash = Sha256::new();
        hash.update(SEMANTICS);
        hash.update(domain);
        hash.update(encoding);
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    };
    Ok((
        PhysicalIdentity {
            semantics: SEMANTICS.into(),
            structural: digest("structural", structural),
            exact_problem: digest("exact", exact),
        },
        work.used,
    ))
}

fn ranks<T: Ord>(keys: &[T]) -> Vec<usize> {
    let mut unique: Vec<_> = keys.iter().collect();
    unique.sort();
    unique.dedup();
    keys.iter()
        .map(|k| unique.binary_search(&k).unwrap())
        .collect()
}
type Adjacency = [Vec<(Relation, bool, usize)>];
fn refine(mut colors: Vec<usize>, adjacency: &Adjacency) -> Vec<usize> {
    loop {
        let keys: Vec<_> = adjacency
            .iter()
            .enumerate()
            .map(|(v, incident)| {
                let mut neighbors: Vec<_> = incident
                    .iter()
                    .map(|&(r, reverse, other)| (r, reverse, colors[other]))
                    .collect();
                neighbors.sort_unstable();
                (colors[v], neighbors)
            })
            .collect();
        let next = ranks(&keys);
        if next == colors {
            return colors;
        }
        colors = next;
    }
}
fn encoding(graph: &Graph, colors: &[usize], exact: bool) -> Vec<u8> {
    let mut order: Vec<_> = (0..colors.len()).collect();
    order.sort_unstable_by_key(|&v| colors[v]);
    let vertices: Vec<_> = order
        .iter()
        .map(|&v| {
            let p = &graph.vertices[v];
            (
                p.kind,
                &p.structural,
                if exact { p.exact.as_slice() } else { &[] },
            )
        })
        .collect();
    let mut relations: Vec<_> = graph
        .relations
        .iter()
        .map(|&(a, r, b)| (colors[a], r, colors[b]))
        .collect();
    relations.sort_unstable();
    serde_json::to_vec(&(vertices, relations)).expect("integer graph serialization")
}

fn canonical(
    graph: &Graph,
    adjacency: &Adjacency,
    exact: bool,
    work: &mut Work<'_>,
) -> Result<Vec<u8>, Diagnostic> {
    let keys: Vec<_> = graph
        .vertices
        .iter()
        .map(|p| {
            (
                p.kind,
                &p.structural,
                if exact { p.exact.as_slice() } else { &[] },
            )
        })
        .collect();
    let mut next = Some(ranks(&keys));
    // Explicit DFS avoids recursion-stack exhaustion. Only the current branch is expanded.
    let mut stack: Vec<(Vec<usize>, Vec<usize>, usize)> = vec![];
    let mut best: Option<Vec<u8>> = None;
    loop {
        if let Some(colors) = next.take() {
            work.visit()?;
            let colors = refine(colors, adjacency);
            let mut classes = BTreeMap::<usize, Vec<usize>>::new();
            for (v, &color) in colors.iter().enumerate() {
                classes.entry(color).or_default().push(v);
            }
            if let Some(class) = classes.into_values().find(|c| c.len() > 1) {
                stack.push((colors, class, 0));
            } else {
                let encoded = encoding(graph, &colors, exact);
                if best.as_ref().is_none_or(|old| &encoded < old) {
                    best = Some(encoded);
                }
            }
        }
        while let Some((colors, candidates, index)) = stack.last_mut() {
            if *index == candidates.len() {
                stack.pop();
                continue;
            }
            let selected = candidates[*index];
            *index += 1;
            let individualized: Vec<_> = colors
                .iter()
                .enumerate()
                .map(|(v, &color)| (color, v == selected))
                .collect();
            next = Some(ranks(&individualized));
            break;
        }
        if next.is_none() {
            return Ok(best.expect("at least one complete labeling"));
        }
    }
}
