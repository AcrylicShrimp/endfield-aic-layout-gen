//! Isolated declarative-action experiment; the original incidence never mutates.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aic_data::layouts::{
    FactoryEditRequest, FactoryLayoutMetrics, TransportNetworkEndpoint, WorldGridPosition,
    validate_factory_layout,
};
use serde_json::json;

use super::super::{catalog, identity};
use super::*;

mod checkpoint;
mod edit;
pub(in crate::full_factory) mod frame;
#[cfg(test)]
mod frame_tests;
pub(crate) mod learning;
#[cfg(test)]
mod tests;

pub const SCHEMA: &str = "declarative-edit-v2";
const GUARD: i64 = 1_000_000;

fn error(code: &str, message: impl Into<String>, entity: impl Into<Value>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(format!("declarative.{code}"), message, entity);
    diagnostic.stage = "declarative-edit";
    diagnostic
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub schema: String,
    pub max_actions: u64,
    pub elapsed_ms: u64,
    pub route_expansions: usize,
    pub max_total_route_expansions: usize,
    pub search_padding: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct State {
    geometry: Draft,
    ports: Vec<Option<String>>,
    anchors: Vec<Point>,
    components: Vec<WorldGridPosition>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
enum DecisionSchema {
    #[serde(rename = "declarative-edit-v2")]
    Local,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    schema: DecisionSchema,
    blueprint_sha256: String,
    revision: u64,
    action: Edit,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Place {
        instance: String,
        x: i64,
        y: i64,
        rotation: i64,
        ports: Vec<PortChoice>,
    },
    InsertBand {
        axis: Axis,
        at: i64,
        count: i64,
    },
    DeleteBand {
        axis: Axis,
        at: i64,
        count: i64,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Axis {
    X,
    Y,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PortChoice {
    node: usize,
    port: String,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Work {
    route_expansions: usize,
    preflight_expansions: usize,
    route_calls: usize,
    validations: usize,
}

pub struct Session {
    catalogs: Arc<crate::input::Catalogs>,
    request: FactoryEditRequest,
    original: Witness,
    blueprint: Blueprint,
    blueprint_sha256: String,
    catalog_sha256: String,
    state: State,
    best_state: State,
    best: Witness,
    best_metrics: FactoryLayoutMetrics,
    initial_metrics: FactoryLayoutMetrics,
    config: Limits,
    attempts: u64,
    revision: u64,
    work: Work,
    validated: bool,
    started: Instant,
    elapsed_before: Duration,
}

impl Session {
    pub fn new(
        request: FactoryEditRequest,
        original: Witness,
        catalogs: Arc<crate::input::Catalogs>,
        config: Limits,
    ) -> Result<Self> {
        if config.schema != SCHEMA
            || !(1..=10_000_000).contains(&config.max_actions)
            || !(1..=86_400_000).contains(&config.elapsed_ms)
            || !(1..=1_000_000).contains(&config.route_expansions)
            || !(1..=500_000_000).contains(&config.max_total_route_expansions)
            || !(0..=1000).contains(&config.search_padding)
        {
            return Err(error(
                "config",
                "Unsupported schema or invalid action/time/routing guards",
                "config",
            ));
        }
        let accepted = validate_factory_layout(&request, &original, &catalogs.data);
        if !accepted.factory_validated {
            return Err(error(
                "initial-invalid",
                "Initial input must be a complete validated factory",
                json!(accepted.diagnostics),
            ));
        }
        let metrics = accepted.metrics.unwrap();
        let working = frame::working_witness(&original, &catalogs.data)?;
        let (blueprint, geometry) = topology::compile(&working)?;
        let state = State {
            geometry,
            ports: blueprint
                .nodes
                .iter()
                .map(|n| match n.terminal.as_ref().map(|t| &t.endpoint) {
                    Some(TransportNetworkEndpoint::Facility { port, .. }) => Some(port.clone()),
                    _ => None,
                })
                .collect(),
            anchors: blueprint.nodes.iter().map(|n| n.initial_position).collect(),
            components: blueprint
                .components
                .iter()
                .map(|c| c.position.clone())
                .collect(),
        };
        let blueprint_sha256 = identity::digest(&blueprint);
        let catalog_sha256 = catalogs.identity.clone();
        let session = Self {
            catalogs,
            request,
            original: original.clone(),
            blueprint,
            blueprint_sha256,
            catalog_sha256,
            best_state: state.clone(),
            state,
            best: working,
            best_metrics: metrics.clone(),
            initial_metrics: metrics,
            config,
            attempts: 0,
            revision: 0,
            work: Work::default(),
            validated: true,
            started: Instant::now(),
            elapsed_before: Duration::ZERO,
        };
        session.check_state(&session.state)?;
        Ok(session)
    }

    fn elapsed(&self) -> Duration {
        self.elapsed_before.saturating_add(self.started.elapsed())
    }
    fn expired(&self) -> bool {
        self.elapsed() >= Duration::from_millis(self.config.elapsed_ms)
    }
    fn done(&self) -> bool {
        self.expired()
            || self.attempts >= self.config.max_actions
            || self.work.route_expansions >= self.config.max_total_route_expansions
    }

    /// A derived geometry view, never an alternative authority for original incidence.
    fn blueprint_at(&self, state: &State) -> Blueprint {
        let mut bp = self.blueprint.clone();
        for node in &mut bp.nodes {
            node.initial_position = state.anchors[node.id];
            if let Some(terminal) = &mut node.terminal
                && let TransportNetworkEndpoint::Facility { port, .. } = &mut terminal.endpoint
            {
                *port = state.ports[node.id].as_ref().unwrap().clone();
            }
        }
        for (c, position) in bp.components.iter_mut().zip(&state.components) {
            c.position = position.clone();
        }
        bp
    }

    fn routing_blueprint(&self, state: &State) -> Result<Blueprint> {
        let mut bp = self.blueprint_at(state);
        // Unchanged ports preserve all established original port-adjacent turns.
        // A reassigned port does not inherit a different port's exit heading:
        // allow the router to choose a legal turn outside its new connection cell.
        for node in &mut bp.nodes {
            let original_port = self.blueprint.nodes[node.id]
                .terminal
                .as_ref()
                .and_then(|t| match &t.endpoint {
                    TransportNetworkEndpoint::Facility { port, .. } => Some(port),
                    _ => None,
                });
            if original_port != state.ports[node.id].as_ref() {
                node.initial_position =
                    topology::position(node, &state.geometry, &self.catalogs.data)?;
                node.terminal = None;
                for edge in &mut bp.edges {
                    if edge.source == node.id {
                        edge.first = None;
                    }
                    if edge.target == node.id {
                        edge.last = None;
                    }
                }
            }
        }
        Ok(bp)
    }

    fn witness(&self, state: &State) -> Result<Witness> {
        let bp = self.blueprint_at(state);
        let mut witness = topology::rebuild(
            &self.original,
            &bp,
            &state.geometry,
            Some(&self.catalogs.data),
        )?;
        witness.logistics_components = bp.components;
        for node in &bp.nodes {
            if let Some(t) = &node.terminal {
                let terminal = witness.transport_networks[node.network]
                    .terminals
                    .iter_mut()
                    .find(|v| v.id == t.id)
                    .unwrap();
                terminal.endpoint = t.endpoint.clone();
            }
        }
        Ok(witness)
    }

    fn automatic(
        &mut self,
        state: &mut State,
        details: &mut Value,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<Option<(Witness, FactoryLayoutMetrics)>> {
        let bp = self.routing_blueprint(state)?;
        let mut candidate = state.geometry.clone();
        let mut pending: Vec<_> = bp
            .edges
            .iter()
            .filter(|e| candidate.paths[e.id].is_none())
            .collect();
        pending.sort_by_key(|e| (bp.networks[e.network]["transport"] != "pipe", e.id));
        let mut remaining = self.config.route_expansions.min(
            self.config
                .max_total_route_expansions
                .saturating_sub(self.work.route_expansions),
        );
        let mut repaired = true;
        for edge in pending {
            if self.expired() {
                return Err(error(
                    "elapsed",
                    "Deadline reached during automatic routing",
                    edge.id,
                ));
            }
            let result = geometry::repair(
                &bp,
                &candidate,
                &self.catalogs.data,
                edge,
                remaining,
                self.config.search_padding,
            );
            let result = match result {
                Ok(result) => result,
                Err(d) => {
                    diagnostics.push(d);
                    repaired = false;
                    break;
                }
            };
            self.work.route_calls += 1;
            self.work.route_expansions += result.expanded;
            self.work.preflight_expansions += result.preflight_expanded;
            remaining = remaining.saturating_sub(result.expanded);
            details["routes"].as_array_mut().unwrap().push(
                json!({"edge": edge.id, "status": result.status,
                "expanded": result.expanded, "preflight_expanded": result.preflight_expanded,
                "preflight_status": result.preflight_status}),
            );
            let Some(cells) = result.cells else {
                diagnostics.push(error(
                    "routing-pending",
                    format!("Automatic routing stopped: {}", result.status),
                    edge.id,
                ));
                repaired = false;
                break;
            };
            if let Err(d) = geometry::check_path(&bp, &candidate, &self.catalogs.data, edge, &cells)
            {
                diagnostics.push(d);
                repaired = false;
                break;
            }
            candidate.paths[edge.id] = Some(cells);
        }
        if repaired {
            state.geometry = candidate;
        }
        frame::canonicalize(&self.blueprint, state, &self.catalogs.data)?;
        let bp = self.blueprint_at(state);
        details["repair_batch_applied"] = json!(repaired);
        if !geometry::conflicts(&bp, &state.geometry).is_empty()
            || state.geometry.paths.iter().any(Option::is_none)
        {
            return Ok(None);
        }
        let witness = self.witness(state)?;
        self.work.validations += 1;
        let accepted = validate_factory_layout(&self.request, &witness, &self.catalogs.data);
        if !accepted.factory_validated {
            diagnostics.push(error(
                "native-validation",
                "Complete draft failed full physical validation",
                json!(accepted.diagnostics),
            ));
            return Ok(None);
        }
        let metrics = accepted.metrics.unwrap();
        details["native_acceptance"] = json!({"factory_validated": true, "metrics": metrics});
        Ok(Some((witness, metrics)))
    }

    fn step(
        &mut self,
        decision: impl FnOnce() -> Result<Decision>,
        previous: Option<&mut Option<State>>,
    ) -> Value {
        let began = Instant::now();
        let before_work = self.work.clone();
        let mut details = json!({"routes": []});
        let mut diagnostics = Vec::new();
        let mut status = "rejected";
        let mut reward = 0.;
        let charged = !self.done();
        let result = if !charged {
            Err(error(
                "budget-exhausted",
                "Session is closed or action/time/routing allowance is exhausted",
                "session",
            ))
        } else {
            self.attempts += 1;
            decision().and_then(|decision| {
                if decision.blueprint_sha256 != self.blueprint_sha256
                    || decision.revision != self.revision
                {
                    return Err(error(
                        "stale-envelope",
                        "Action must bind original blueprint and current revision",
                        "action",
                    ));
                }
                let mut next = self.state.clone();
                self.edit(&mut next, decision.action)?;
                if next.geometry == self.state.geometry
                    && next.ports == self.state.ports
                    && next.components == self.state.components
                    && self.blueprint.nodes.iter().all(|n| {
                        matches!(
                            n.terminal.as_ref().map(|t| &t.endpoint),
                            Some(TransportNetworkEndpoint::Facility { .. })
                        ) || next.anchors[n.id] == self.state.anchors[n.id]
                    })
                {
                    status = "unchanged";
                    return Ok(());
                }
                self.detach(&mut next)?;
                let accepted = self.automatic(&mut next, &mut details, &mut diagnostics)?;
                if self.expired() {
                    return Err(error(
                        "elapsed",
                        "Edit finished after deadline; preceding draft and best retained",
                        "session",
                    ));
                }
                status = "draft_accepted";
                self.validated = accepted.is_some();
                if let Some((witness, metrics)) = accepted {
                    status = "validated";
                    if metrics.score < self.best_metrics.score {
                        reward = (self.best_metrics.score[0] - metrics.score[0]) as f64
                            / self.initial_metrics.score[0] as f64;
                        self.best = witness;
                        self.best_metrics = metrics;
                        self.best_state = next.clone();
                        status = "improved";
                    }
                }
                if let Some(previous) = previous {
                    *previous = Some(std::mem::replace(&mut self.state, next));
                } else {
                    self.state = next;
                }
                self.revision += 1;
                Ok(())
            })
        };
        if let Err(d) = result {
            diagnostics.push(d);
        }
        details["route_expansions"] =
            json!(self.work.route_expansions - before_work.route_expansions);
        details["preflight_expansions"] =
            json!(self.work.preflight_expansions - before_work.preflight_expansions);
        details["route_calls"] = json!(self.work.route_calls - before_work.route_calls);
        details["validations"] = json!(self.work.validations - before_work.validations);
        json!({"schema": "declarative-edit-outcome-v1", "status": status, "reward": reward,
            "attempts": self.attempts, "revision": self.revision, "charged": charged,
            "best_metrics": self.best_metrics, "diagnostics": diagnostics, "details": details,
            "done": self.done(), "seconds": began.elapsed().as_secs_f64()})
    }

    fn retained_result(&self) -> Value {
        json!({"schema": "declarative-edit-result-v1", "blueprint_sha256": self.blueprint_sha256,
            "factory_validated": true, "best": self.best, "best_metrics": self.best_metrics,
            "initial_metrics": self.initial_metrics, "attempts": self.attempts, "revision": self.revision,
            "unfinished_draft": !self.validated, "work": self.work, "elapsed_ms": self.elapsed().as_secs_f64() * 1000.})
    }
}
