//! Bounded cold evidence export; changed native states move into chunk ownership.

use super::*;

struct Entry {
    revision: u64,
    state: Option<State>,
    best_metrics: FactoryLayoutMetrics,
    full: Option<Value>,
    witness: Option<Witness>,
}

pub(crate) struct Chunk {
    entries: Vec<Entry>,
    limit: usize,
    full: bool,
}

impl Chunk {
    pub(crate) fn new(limit: usize, full: bool) -> Result<Self> {
        if limit == 0 {
            return Err(error(
                "learning-chunk",
                "Positive chunk bound required",
                "chunk",
            ));
        }
        Ok(Self {
            entries: vec![],
            limit,
            full,
        })
    }

    pub(crate) fn step(&mut self, session: &mut Session, decision: Decision) -> Result<Value> {
        if self.entries.len() >= self.limit {
            return Err(error(
                "learning-chunk",
                "Chunk must be drained before another selection",
                "chunk",
            ));
        }
        let revision = session.revision;
        let best_metrics = session.best_metrics.clone();
        // Full observations are an explicit diagnostic option. Normal replay
        // needs only changed physical states, exported once after the chunk.
        let full = if self.full {
            Some(session.observation()?)
        } else {
            None
        };
        let mut previous = None;
        let outcome = session.step(|| Ok(decision), Some(&mut previous));
        let witness = (outcome["reward"].as_f64().unwrap() > 0.).then(|| session.best.clone());
        self.entries.push(Entry {
            revision,
            state: previous,
            best_metrics,
            full,
            witness,
        });
        Ok(outcome)
    }

    pub(crate) fn export(self, session: &Session) -> Result<Value> {
        self.entries
            .into_iter()
            .map(|entry| {
                let observation = if let Some(full) = entry.full {
                    full
                } else if let Some(state) = entry.state {
                    physical(session, &state, entry.revision, &entry.best_metrics)?
                } else {
                    json!({"blueprint_sha256":session.blueprint_sha256,"revision":entry.revision})
                };
                Ok(json!({"observation":observation,"validated_witness":entry.witness}))
            })
            .collect::<Result<Vec<_>>>()
            .map(|rows| json!(rows))
    }
}

fn physical(
    session: &Session,
    state: &State,
    revision: u64,
    best: &FactoryLayoutMetrics,
) -> Result<Value> {
    let bp = session.blueprint_at(state);
    let nodes = bp.nodes.iter().map(|node| Ok(json!({"id":node.id,
        "position":topology::position(node,&state.geometry,&session.catalogs)?,"components":node.components})))
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"blueprint_sha256":session.blueprint_sha256,"revision":revision,
        "facilities":state.geometry.placements,"nodes":nodes,"components":bp.components,
        "port_assignments":state.ports,"edges":bp.edges.iter().map(|edge|json!({"id":edge.id,
            "path":state.geometry.paths[edge.id],"status":if state.geometry.paths[edge.id].is_some(){"routed"}else{"pending"}})).collect::<Vec<_>>(),
        "best_metrics":best}),
    )
}
