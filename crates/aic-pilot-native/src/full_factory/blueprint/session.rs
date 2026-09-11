//! A draft may be invalid; the retained best never is.

use std::sync::Arc;
use std::time::{Duration, Instant};

use aic_data::layouts::{
    FactoryEditCatalogs, FactoryEditRequest, FactoryLayoutMetrics, validate_factory_layout,
};
use serde_json::json;

use super::super::identity;
use super::*;

pub struct Session {
    catalogs: Arc<FactoryEditCatalogs>,
    request: FactoryEditRequest,
    original: Witness,
    blueprint: Blueprint,
    blueprint_sha256: String,
    draft: Draft,
    best_draft: Draft,
    best: Witness,
    best_metrics: FactoryLayoutMetrics,
    initial_metrics: FactoryLayoutMetrics,
    config: Config,
    started: Instant,
    attempts: u64,
    revision: u64,
    closed: bool,
    draft_validated: bool,
}

impl Session {
    pub fn new(
        request: FactoryEditRequest,
        original: Witness,
        catalogs: Arc<FactoryEditCatalogs>,
        config: Config,
    ) -> Result<Self> {
        if config.schema != CONTRACT
            || config.max_actions == 0
            || config.max_actions > 10_000_000
            || config.elapsed_ms == 0
            || config.elapsed_ms > 86_400_000
            || config.route_expansions == 0
            || config.route_expansions > 1_000_000
            || !(0..=1000).contains(&config.search_padding)
        {
            return Err(fail(
                "config",
                "Unsupported schema or invalid action, elapsed, routing-work or padding guard",
                "config",
            ));
        }
        let accepted = validate_factory_layout(&request, &original, &catalogs);
        if !accepted.factory_validated {
            return Err(fail(
                "initial-invalid",
                "Original blueprint must be a complete validated factory",
                json!(accepted.diagnostics),
            ));
        }
        let metrics = accepted.metrics.unwrap();
        let (blueprint, draft) = topology::compile(&original)?;
        for edge in &blueprint.edges {
            geometry::check_path(
                &blueprint,
                &draft,
                &catalogs,
                edge,
                draft.paths[edge.id].as_ref().unwrap(),
            )?;
        }
        let blueprint_sha256 = identity::digest(&blueprint);
        Ok(Self {
            catalogs,
            request,
            best: original.clone(),
            original,
            blueprint,
            blueprint_sha256,
            best_draft: draft.clone(),
            draft,
            initial_metrics: metrics.clone(),
            best_metrics: metrics,
            config,
            started: Instant::now(),
            attempts: 0,
            revision: 0,
            closed: false,
            draft_validated: true,
        })
    }

    fn expired(&self) -> bool {
        self.started.elapsed() >= Duration::from_millis(self.config.elapsed_ms)
    }
    fn done(&self) -> bool {
        self.closed || self.attempts >= self.config.max_actions || self.expired()
    }

    pub fn observation(&self) -> Result<Value> {
        let nodes: Vec<_> = self
            .blueprint
            .nodes
            .iter()
            .map(|n| {
                Ok(json!({"id": n.id,
            "position": topology::position(n, &self.draft, &self.catalogs)?}))
            })
            .collect::<Result<_>>()?;
        let edges: Vec<_> = self
            .blueprint
            .edges
            .iter()
            .map(|e| {
                json!({"id": e.id,
            "status": if self.draft.paths[e.id].is_some() { "routed" } else { "pending" },
            "path": self.draft.paths[e.id]})
            })
            .collect();
        Ok(
            json!({"schema": OBSERVATION, "blueprint": self.blueprint, "blueprint_sha256": self.blueprint_sha256,
            "revision": self.revision, "draft_factory_validated": self.draft_validated, "facilities": self.draft.placements,
            "nodes": nodes, "edges": edges, "conflicts": geometry::conflicts(&self.blueprint, &self.draft),
            "best_metrics": self.best_metrics, "initial_metrics": self.initial_metrics,
            "attempts": self.attempts, "config": self.config, "elapsed_ms": self.started.elapsed().as_secs_f64() * 1000.,
            "done": self.done()}),
        )
    }

    fn execute(
        &self,
        draft: &mut Draft,
        action: Action,
        details: &mut Value,
    ) -> Result<Option<(Witness, FactoryLayoutMetrics)>> {
        match action {
            Action::Pose {
                instance,
                x,
                y,
                rotation,
            } => {
                details["detached_edges"] = json!(geometry::pose(
                    &self.blueprint,
                    draft,
                    &self.catalogs,
                    &instance,
                    x,
                    y,
                    rotation
                )?);
            }
            Action::Route { edge, cells } => {
                let edge = self
                    .blueprint
                    .edges
                    .get(edge)
                    .ok_or_else(|| fail("edge", "Unknown original edge", edge))?;
                geometry::check_path(&self.blueprint, draft, &self.catalogs, edge, &cells)?;
                draft.paths[edge.id] = Some(cells);
            }
            Action::Repair { .. } | Action::RepairAll => {
                let mut edges: Vec<_> = match action {
                    Action::Repair { edge } => vec![
                        self.blueprint
                            .edges
                            .get(edge)
                            .ok_or_else(|| fail("edge", "Unknown original edge", edge))?,
                    ],
                    _ => self
                        .blueprint
                        .edges
                        .iter()
                        .filter(|e| draft.paths[e.id].is_none())
                        .collect(),
                };
                edges.sort_by_key(|e| {
                    (
                        self.blueprint.networks[e.network]["transport"] != "pipe",
                        e.id,
                    )
                });
                let mut expanded = 0;
                details["routes"] = json!([]);
                for edge in edges {
                    if self.expired() {
                        return Err(fail(
                            "elapsed",
                            "Deadline reached before routing the next edge",
                            edge.id,
                        ));
                    }
                    let result = geometry::repair(
                        &self.blueprint,
                        draft,
                        &self.catalogs,
                        edge,
                        self.config.route_expansions.saturating_sub(expanded),
                        self.config.search_padding,
                    )?;
                    expanded += result.expanded;
                    details["route_expansions"] = json!(expanded);
                    details["routes"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"edge": edge.id, "result": result}));
                    let Some(cells) = result.cells else {
                        return Err(fail(
                            "routing-failed",
                            format!("Original edge repair stopped: {}", result.status),
                            edge.id,
                        ));
                    };
                    geometry::check_path(&self.blueprint, draft, &self.catalogs, edge, &cells)?;
                    draft.paths[edge.id] = Some(cells);
                }
            }
            Action::Commit => {
                let conflicts = geometry::conflicts(&self.blueprint, draft);
                if !conflicts.is_empty() {
                    return Err(fail(
                        "unfinished-conflicts",
                        "Overlapping draft cannot be committed",
                        json!(conflicts),
                    ));
                }
                for edge in &self.blueprint.edges {
                    let path = draft.paths[edge.id].as_ref().ok_or_else(|| {
                        fail(
                            "pending-edge",
                            "Original connection remains pending",
                            edge.id,
                        )
                    })?;
                    geometry::check_path(&self.blueprint, draft, &self.catalogs, edge, path)?;
                }
                let witness = topology::rebuild(
                    &self.original,
                    &self.blueprint,
                    draft,
                    Some(&self.catalogs),
                )?;
                let accepted = validate_factory_layout(&self.request, &witness, &self.catalogs);
                details["native_acceptance"] = json!(accepted);
                if !accepted.factory_validated {
                    return Err(fail(
                        "native-validation",
                        "Rebuilt original topology failed full physical factory validation",
                        json!(accepted.diagnostics),
                    ));
                }
                return Ok(Some((witness, accepted.metrics.unwrap())));
            }
            Action::RestoreBest => {
                *draft = self.best_draft.clone();
            }
        }
        Ok(None)
    }

    pub fn step_json(&mut self, input: &str) -> Value {
        let began = Instant::now();
        let mut details = json!({"route_expansions": 0});
        let mut status = "rejected";
        let mut reward = 0.;
        let mut diagnostics = Vec::new();
        let mut charged = false;
        let result = if self.done() {
            Err(fail(
                "budget-exhausted",
                "Session is closed or its action/elapsed guard is exhausted",
                "budget",
            ))
        } else {
            self.attempts += 1;
            charged = true;
            crate::python_factory::decode::<Envelope>(input)
                .map_err(|e| fail("action-schema", e, "action"))
                .and_then(|envelope| {
                    if envelope.blueprint_sha256 != self.blueprint_sha256
                        || envelope.revision != self.revision
                    {
                        return Err(fail(
                            "stale-envelope",
                            "Action must bind this blueprint and current draft revision",
                            "action",
                        ));
                    }
                    let mut candidate = self.draft.clone();
                    let validated = matches!(envelope.action, Action::Commit | Action::RestoreBest);
                    let commit = self.execute(&mut candidate, envelope.action, &mut details)?;
                    if self.expired() {
                        return Err(fail(
                            "elapsed",
                            "Action completed after the elapsed guard; draft and best retained",
                            "budget",
                        ));
                    }
                    status = "draft_accepted";
                    if let Some((witness, metrics)) = commit {
                        status = "validated";
                        if metrics.score < self.best_metrics.score {
                            reward = (self.best_metrics.score[0] - metrics.score[0]) as f64
                                / self.initial_metrics.score[0] as f64;
                            self.best = witness;
                            self.best_metrics = metrics;
                            self.best_draft = candidate.clone();
                            status = "improved";
                        }
                    }
                    self.draft = candidate;
                    self.draft_validated = validated;
                    self.revision += 1;
                    Ok(())
                })
        };
        if let Err(d) = result {
            diagnostics.push(d);
        }
        json!({"schema": "blueprint-edit-outcome-v1", "blueprint_sha256": self.blueprint_sha256,
            "revision": self.revision, "attempts": self.attempts, "charged": charged, "status": status,
            "reward": reward, "best_metrics": self.best_metrics, "diagnostics": diagnostics, "details": details,
            "done": self.done(), "seconds": began.elapsed().as_secs_f64()})
    }

    pub fn finish(&mut self) -> Value {
        self.closed = true;
        json!({"schema": "blueprint-edit-result-v1", "blueprint_sha256": self.blueprint_sha256,
            "factory_validated": true, "best": self.best, "best_metrics": self.best_metrics,
            "attempts": self.attempts, "revision": self.revision,
            "unfinished_draft": !self.draft_validated,
            "elapsed_ms": self.started.elapsed().as_secs_f64() * 1000.})
    }

    pub fn validated_draft(&self) -> Result<Value> {
        if !self.draft_validated {
            return Err(fail(
                "draft-unvalidated",
                "Only an independently validated draft can be exported as a witness",
                "draft",
            ));
        }
        Ok(json!(topology::rebuild(
            &self.original,
            &self.blueprint,
            &self.draft,
            Some(&self.catalogs)
        )?))
    }
}
