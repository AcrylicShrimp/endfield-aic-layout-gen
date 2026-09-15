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
pub(super) struct State {
    pub(super) geometry: Draft,
    pub(super) ports: Vec<Option<String>>,
    pub(super) anchors: Vec<Point>,
    pub(super) components: Vec<WorldGridPosition>,
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
    common_problem: Arc<aic_layout::editor::CompiledProblemV1>,
    common_problem_sha256: String,
    /// Authoritative editor state; `state` is its cached factory export projection.
    common_state: aic_layout::editor::PhysicalDraftV1,
    common_best_state: aic_layout::editor::PhysicalDraftV1,
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
    fn common_facility(&self, id: &str) -> &aic_layout::facilities::FacilityDefinition {
        self.common_problem
            .used_catalog
            .facilities
            .iter()
            .find(|definition| definition.id == id)
            .expect("validated common facility")
    }

    fn common_component(&self, id: &str) -> &aic_layout::logistics::LogisticsComponentDefinition {
        self.common_problem
            .used_catalog
            .components
            .iter()
            .find(|definition| definition.id == id)
            .expect("validated common component")
    }

    fn common_transport(
        &self,
        kind: aic_layout::logistics::TransportKind,
    ) -> &aic_layout::logistics::TransportDefinition {
        self.common_problem
            .used_catalog
            .transports
            .iter()
            .find(|definition| definition.kind == kind)
            .expect("validated common transport")
    }

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
        let common_problem = Arc::new(common::problem(
            &blueprint,
            &catalogs.data,
            &catalog_sha256,
        )?);
        let common_state = common::draft(&state);
        aic_layout::editor::validate_compiled(&common_problem, &common_state).map_err(
            |failure| {
                error(
                    "common-state",
                    "Factory admission failed the common physical boundary",
                    failure.to_string(),
                )
            },
        )?;
        let common_problem_sha256 = identity::digest(common_problem.as_ref());
        let session = Self {
            catalogs,
            request,
            original: original.clone(),
            blueprint,
            common_problem,
            common_problem_sha256,
            common_best_state: common_state.clone(),
            common_state,
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
        draft: &mut aic_layout::editor::PhysicalDraftV1,
        details: &mut Value,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<(State, Option<(Witness, FactoryLayoutMetrics)>)> {
        let available = self.config.route_expansions.min(
            self.config
                .max_total_route_expansions
                .saturating_sub(self.work.route_expansions),
        );
        let remaining_elapsed =
            Duration::from_millis(self.config.elapsed_ms).saturating_sub(self.elapsed());
        let deadline = Instant::now()
            .checked_add(remaining_elapsed)
            .unwrap_or_else(Instant::now);
        let routed = crate::physical_editor::repair_pending_validated_until(
            &self.common_problem,
            draft,
            crate::physical_editor::PhysicalEditLimits {
                route_expansions: available,
                search_padding: self.config.search_padding,
            },
            || Instant::now() >= deadline,
        );
        let routed = match routed {
            Ok(routed) => routed,
            Err(failure) => {
                diagnostics.push(error("common-routing", failure.message, failure.entity));
                details["repair_batch_applied"] = json!(false);
                return Ok((
                    common::project(&self.common_problem, draft, &self.blueprint),
                    None,
                ));
            }
        };
        self.work.route_calls += routed.route_calls;
        self.work.route_expansions += routed.expanded;
        self.work.preflight_expansions += routed.preflight_expanded;
        for route in &routed.routes {
            details["routes"].as_array_mut().unwrap().push(json!({
                "edge": route.edge,
                "status": route.status,
                "expanded": route.expanded,
                "preflight_expanded": route.preflight_expanded,
                "preflight_status": route.preflight_status,
            }));
        }
        details["repair_batch_applied"] = json!(routed.committed);
        if !routed.committed {
            if let Some(route) = routed.routes.last() {
                diagnostics.push(error(
                    "routing-pending",
                    format!("Automatic routing stopped: {}", route.status),
                    route.edge,
                ));
            }
            return Ok((
                common::project(&self.common_problem, draft, &self.blueprint),
                None,
            ));
        }
        let state = common::project(&self.common_problem, draft, &self.blueprint);
        if crate::physical_editor::conflicts_validated(&self.common_problem, draft) != 0 {
            return Ok((state, None));
        }
        let witness = self.witness(&state)?;
        self.work.validations += 1;
        let accepted = validate_factory_layout(&self.request, &witness, &self.catalogs.data);
        if !accepted.factory_validated {
            diagnostics.push(error(
                "native-validation",
                "Complete draft failed full physical validation",
                json!(accepted.diagnostics),
            ));
            return Ok((state, None));
        }
        let metrics = accepted.metrics.unwrap();
        details["native_acceptance"] = json!({"factory_validated": true, "metrics": metrics});
        Ok((state, Some((witness, metrics))))
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
                let mut common_next = self.common_state.clone();
                self.edit(&mut common_next, decision.action)?;
                if common_next == self.common_state {
                    status = "unchanged";
                    return Ok(());
                }
                let (next, accepted) =
                    self.automatic(&mut common_next, &mut details, &mut diagnostics)?;
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
                        self.common_best_state = common_next.clone();
                        status = "improved";
                    }
                }
                if let Some(previous) = previous {
                    *previous = Some(std::mem::replace(&mut self.state, next));
                } else {
                    self.state = next;
                }
                self.common_state = common_next;
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
