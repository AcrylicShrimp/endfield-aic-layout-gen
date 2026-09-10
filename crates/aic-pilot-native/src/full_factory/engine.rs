//! Retained complete-factory episodes and their transactional learning boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use aic_data::layouts::{
    FactoryEditBudget, FactoryEditCatalogs, FactoryEditRequest, FactoryEditSession,
    FactoryEditStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::candidates::{self, Action, Domain};
use super::catalog;
use super::identity::{self, Identity};
use super::observation::{self as obs, Contract, History};
use super::poses::VocabularyOptions;
use super::selection::Memory;
use super::types::*;

static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub input_contract: String,
    pub horizon: u64,
    pub elapsed_ms: u64,
    pub route_work: usize,
    pub memory: bool,
    #[serde(default)]
    pub vocabulary: VocabularyOptions,
}
impl Config {
    fn validate(&self) -> Result<Contract> {
        if self.horizon == 0 || self.elapsed_ms == 0 || self.route_work == 0 {
            return Err(fail(
                "factory.episode-budget",
                "Horizon, elapsed milliseconds and route work must be positive",
            ));
        }
        self.vocabulary.validate()?;
        Contract::named(&self.input_contract)
    }
}
fn fail(code: &str, message: &str) -> Diagnostic {
    let mut d = Diagnostic::new(code, message, Value::Null);
    d.stage = "full-factory-episode";
    d
}
fn core_error(d: aic_data::layouts::FactoryEditDiagnostic) -> Diagnostic {
    Diagnostic {
        code: d.code.into(),
        severity: d.severity,
        stage: d.stage,
        message: d.message,
        entity: d.entity.map(Value::String).unwrap_or(Value::Null),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    pub engine: u64,
    pub revision: u64,
}

#[derive(Default, Serialize)]
pub struct Performance {
    pub seconds: BTreeMap<String, f64>,
    pub counters: BTreeMap<String, u64>,
}
impl Performance {
    pub fn time(&mut self, key: &str, began: Instant) {
        *self.seconds.entry(key.into()).or_default() += began.elapsed().as_secs_f64();
    }
    pub fn count(&mut self, key: &str, count: usize) {
        *self.counters.entry(key.into()).or_default() += count as u64;
    }
}
struct Prepared {
    domain: Domain,
    keys: Vec<String>,
    world: Option<obs::WorldInput>,
    tensors: Option<obs::Candidates>,
}
pub struct View<'a> {
    pub current: &'a obs::World,
    pub best: Option<&'a obs::World>,
    pub best_is_current: bool,
    pub candidates: &'a obs::Candidates,
    pub actor_contract: Contract,
    pub actor_context: [f32; 9],
    pub critic_context: [f32; 9],
    pub history: Vec<[f32; 53]>,
}
pub struct Compact {
    pub handle: Handle,
    pub allowed: Vec<bool>,
    pub kinds: Vec<&'static str>,
    pub exhausted: bool,
}

pub struct Episode {
    pub config: Config,
    pub session: FactoryEditSession,
    catalogs: Arc<FactoryEditCatalogs>,
    contract: Contract,
    id: u64,
    revision: u64,
    started: Instant,
    pub selected: u64,
    pub trace: Vec<Value>,
    current_id: Identity,
    best_id: Identity,
    memory: Memory,
    ordered_seen: BTreeSet<String>,
    history: Vec<History>,
    prepared: Option<Prepared>,
    best_world: Option<(String, obs::World)>,
    pub performance: Performance,
}

impl Episode {
    pub fn new(
        request: FactoryEditRequest,
        witness: Witness,
        catalogs: Arc<FactoryEditCatalogs>,
        config: Config,
    ) -> Result<Self> {
        let started = Instant::now();
        let contract = config.validate()?;
        let session = FactoryEditSession::new(
            request,
            witness,
            catalogs.clone(),
            FactoryEditBudget {
                proposals: config.horizon,
                elapsed_ms: config.elapsed_ms,
            },
        )
        .map_err(core_error)?;
        let mut performance = Performance::default();
        performance.time("admission", started);
        let began = Instant::now();
        let current_id = Identity::new(session.snapshot().current);
        performance.time("identities", began);
        performance.count("state_identities", 1);
        Ok(Self {
            config,
            session,
            catalogs,
            contract,
            id: NEXT_ENGINE.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            started,
            selected: 0,
            trace: vec![],
            memory: Memory::new(&current_id.physical),
            ordered_seen: BTreeSet::from([current_id.ordered.clone()]),
            best_id: current_id.clone(),
            current_id,
            history: vec![],
            prepared: None,
            best_world: None,
            performance,
        })
    }
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
    pub fn done(&self) -> bool {
        self.selected >= self.config.horizon
            || self.elapsed() >= Duration::from_millis(self.config.elapsed_ms)
    }
    fn handle(&self) -> Handle {
        Handle {
            engine: self.id,
            revision: self.revision,
        }
    }
    pub fn prepare(&mut self, arrays: bool, critic: bool) -> Result<Compact> {
        if self.done() {
            return Err(fail(
                "factory.episode-exhausted",
                "Episode selected-action or elapsed allowance is exhausted",
            ));
        }
        if self.prepared.is_none() {
            let began = Instant::now();
            let w = self.session.snapshot().current;
            let domain = candidates::describe(w, &self.catalogs, &self.config.vocabulary)?;
            self.performance.time("candidate_indexes", began);
            self.performance.count("candidate_revisions", 1);
            self.performance.count("candidates", domain.actions.len());
            let began = Instant::now();
            let keys = domain
                .actions
                .iter()
                .map(|a| identity::action_key(a, &domain, w, &self.best_id.physical))
                .collect();
            self.performance.time("action_keys", began);
            self.prepared = Some(Prepared {
                domain,
                keys,
                world: None,
                tensors: None,
            });
        }
        let p = self.prepared.as_mut().unwrap();
        if arrays || self.contract.history > 0 {
            if p.world.is_none() {
                let began = Instant::now();
                p.world = Some(obs::world(
                    self.session.snapshot().current,
                    &p.domain.ports,
                )?);
                self.performance.time("current_world", began);
                self.performance.count("current_world_revisions", 1);
            }
            if p.tensors.is_none() {
                let began = Instant::now();
                p.tensors = Some(obs::candidates(
                    self.session.snapshot().current,
                    &p.domain,
                    p.world.as_ref().unwrap(),
                ));
                self.performance.time("action_arrays", began);
            }
        }
        if arrays
            && (critic || self.contract.best == obs::Best::Layout)
            && self.current_id.ordered != self.best_id.ordered
            && self
                .best_world
                .as_ref()
                .is_none_or(|(id, _)| id != &self.best_id.ordered)
        {
            let began = Instant::now();
            let w = self.session.snapshot().best;
            let ports = catalog::project(w, &self.catalogs)?;
            self.best_world = Some((self.best_id.ordered.clone(), obs::world(w, &ports)?.world));
            self.performance.time("best_world", began);
            self.performance.count("best_world_revisions", 1);
        }
        let began = Instant::now();
        let (allowed, exhausted) = self.memory.mask(
            &self.current_id.physical,
            &p.keys,
            &p.domain.actions,
            self.config.memory,
            self.contract.restore,
        )?;
        let kinds = p.domain.actions.iter().map(Action::kind).collect();
        self.performance.time("mask", began);
        self.performance.count("observe_calls", 1);
        Ok(Compact {
            handle: self.handle(),
            allowed,
            exhausted,
            kinds,
        })
    }
    pub fn view(&self) -> View<'_> {
        let p = self.prepared.as_ref().expect("prepare arrays before view");
        let current = &p.world.as_ref().unwrap().world;
        let state = self.session.snapshot();
        let make_context = |contract| {
            obs::context(
                current.width,
                current.height,
                state.current.placements.len(),
                1. - self.selected as f64 / self.config.horizon as f64,
                state.initial_metrics.score.map(|v| v as i64),
                state.current_metrics.score.map(|v| v as i64),
                state.best_metrics.score.map(|v| v as i64),
                contract,
            )
        };
        View {
            current,
            best: self
                .best_world
                .as_ref()
                .filter(|(id, _)| id == &self.best_id.ordered)
                .map(|(_, w)| w),
            best_is_current: self.current_id.ordered == self.best_id.ordered,
            candidates: p.tensors.as_ref().unwrap(),
            actor_contract: self.contract,
            actor_context: make_context(self.contract),
            critic_context: make_context(Contract::named("current-best-layout").unwrap()),
            history: obs::history(&self.history, self.contract),
        }
    }
    pub fn step(&mut self, handle: Handle, index: usize) -> Result<Value> {
        if handle != self.handle()
            || self
                .prepared
                .as_ref()
                .is_none_or(|p| index >= p.domain.actions.len())
        {
            return Err(fail(
                "factory.candidate-handle",
                "Foreign, stale or out-of-range candidate selection",
            ));
        }
        if self.done() {
            return Err(fail(
                "factory.episode-exhausted",
                "Episode selected-action or elapsed allowance is exhausted",
            ));
        }
        let prepared = self.prepared.as_ref().unwrap();
        let (allowed, exhausted) = self.memory.mask(
            &self.current_id.physical,
            &prepared.keys,
            &prepared.domain.actions,
            self.config.memory,
            self.contract.restore,
        )?;
        if !allowed[index] {
            return Err(fail(
                "factory.masked-selection",
                "Selected index is masked in the current candidate domain",
            ));
        }
        let began = Instant::now();
        let action = prepared.domain.actions[index].clone();
        let key = prepared.keys[index].clone();
        let token = if self.contract.history > 0 {
            Some(prepared.tensors.as_ref().unwrap().token(index))
        } else {
            None
        };
        let before_id = self.current_id.clone();
        let before = self.session.snapshot();
        let before_score = before.current_metrics.score;
        let best_area_before = before.best_metrics.score[0];
        let initial_score = before.initial_metrics.score;
        self.selected += 1;
        let execute_started = Instant::now();
        let execution = if matches!(action, Action::RestoreBest { .. }) {
            Ok(super::routing::Execution {
                status: "candidate",
                witness: Some(before.best.clone()),
                diagnostics: vec![],
                routes: vec![],
                route_expansions: 0,
            })
        } else {
            candidates::execute(
                before.current,
                &self.catalogs,
                &action,
                self.config.route_work,
            )
        };
        let executor_seconds = execute_started.elapsed().as_secs_f64();
        self.performance.time("execution", execute_started);
        let mut execution = execution.unwrap_or_else(|d| super::routing::Execution {
            status: "executor_rejected",
            witness: None,
            diagnostics: vec![d],
            routes: vec![],
            route_expansions: 0,
        });
        self.performance
            .count("route_expansions", execution.route_expansions);
        // The reference executor submits unchanged geometry for native validation.
        if execution.status == "unchanged" {
            execution.witness = Some(self.session.snapshot().current.clone());
        }
        let mut row = json!({"selected": self.selected, "action": action, "executor_status": execution.status,
            "route_expansions": execution.route_expansions, "executor_seconds": executor_seconds, "diagnostics": execution.diagnostics});
        let status;
        if self.elapsed() >= Duration::from_millis(self.config.elapsed_ms) {
            status = "late";
        } else if let Some(w) = &execution.witness {
            let remaining =
                Duration::from_millis(self.config.elapsed_ms).saturating_sub(self.elapsed());
            let deadline = Instant::now().checked_add(remaining).ok_or_else(|| {
                fail(
                    "factory.episode-deadline",
                    "Elapsed allowance exceeds clock range",
                )
            })?;
            let acceptance_started = Instant::now();
            let acceptance = self.session.propose_with_deadline(w, deadline);
            self.performance.time("acceptance", acceptance_started);
            self.performance
                .count("native_proposals", usize::from(acceptance.charged));
            status = match acceptance.status {
                FactoryEditStatus::Improved => "improved",
                FactoryEditStatus::AcceptedExploration => "accepted_exploration",
                FactoryEditStatus::AcceptedEqual => "accepted_equal",
                FactoryEditStatus::Rejected => "rejected",
                FactoryEditStatus::Late | FactoryEditStatus::BudgetExhausted => "late",
            };
            if matches!(
                acceptance.status,
                FactoryEditStatus::Improved
                    | FactoryEditStatus::AcceptedExploration
                    | FactoryEditStatus::AcceptedEqual
            ) {
                let identity_started = Instant::now();
                self.current_id = if matches!(action, Action::RestoreBest { .. }) {
                    self.best_id.clone()
                } else if execution.status == "unchanged" {
                    before_id.clone()
                } else {
                    self.performance.count("state_identities", 1);
                    Identity::new(self.session.snapshot().current)
                };
                if acceptance.status == FactoryEditStatus::Improved {
                    self.best_id = self.current_id.clone();
                }
                self.performance.time("identities", identity_started);
            }
            row["acceptance"] = serde_json::to_value(acceptance).unwrap();
        } else {
            status = "executor_rejected";
        }
        let unchanged = before_id.ordered == self.current_id.ordered;
        let revisited = !unchanged && self.ordered_seen.contains(&self.current_id.ordered);
        self.ordered_seen.insert(self.current_id.ordered.clone());
        let (physical_unchanged, physical_revisited) =
            self.memory
                .record(&before_id.physical, &self.current_id.physical, &key);
        let after = self.session.snapshot();
        let reward_numerator = best_area_before - after.best_metrics.score[0];
        let reward = reward_numerator as f64 / initial_score[0] as f64;
        let additions = json!({"status": status, "unchanged": unchanged, "revisited": revisited,
            "current_score": after.current_metrics.score, "best_score": after.best_metrics.score, "seconds": began.elapsed().as_secs_f64(), "elapsed_seconds": self.elapsed().as_secs_f64(),
            "best_area_before": best_area_before, "reward_numerator": reward_numerator, "reward_denominator": initial_score[0], "reward": reward,
            "vocabulary_index": index, "allowed_sha256": identity::digest(&allowed), "allowed_count": allowed.iter().filter(|b| **b).count(), "memory_exhausted": exhausted,
            "current_before_sha256": before_id.ordered, "current_sha256": self.current_id.ordered, "best_sha256": self.best_id.ordered,
            "physical_before": before_id.physical, "physical_after": self.current_id.physical, "physical_unchanged": physical_unchanged, "physical_revisited": physical_revisited, "action_key": key});
        row.as_object_mut()
            .unwrap()
            .extend(additions.as_object().unwrap().clone());
        if let Some(token) = token {
            let mut outcome = [0.; 10];
            let i = [
                "improved",
                "accepted_exploration",
                "accepted_equal",
                "rejected",
                "executor_rejected",
                "late",
            ]
            .iter()
            .position(|s| *s == status)
            .unwrap();
            outcome[i] = 1.;
            outcome[6] = if unchanged { 1. } else { 0. };
            for j in 0..3 {
                outcome[7 + j] = ((after.current_metrics.score[j] as f64 - before_score[j] as f64)
                    / initial_score[j].max(1) as f64) as f32;
            }
            self.history.push(History {
                action: token,
                outcome,
            });
            if self.history.len() > self.contract.history {
                self.history.remove(0);
            }
        }
        if !unchanged {
            self.prepared = None;
        }
        self.revision += 1;
        row["done"] = Value::Bool(self.done());
        self.trace.push(row.clone());
        Ok(row)
    }
    pub fn summary(&self) -> Value {
        let s = self.session.snapshot();
        json!({"instances": s.best.placements.len(), "initial_metrics": s.initial_metrics, "current_metrics": s.current_metrics, "best_metrics": s.best_metrics,
            "selected": self.selected, "native_proposals": s.proposals, "elapsed_seconds": self.elapsed().as_secs_f64(), "trace": self.trace, "best_sha256": self.best_id.ordered, "done": self.done()})
    }
    pub fn diagnostic_domain(&mut self) -> Result<Value> {
        self.prepare(false, false)?;
        let p = &self.prepared.as_ref().unwrap().domain;
        let ports: serde_json::Map<_, _> = p
            .ports
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
            .collect();
        Ok(
            json!({"schema_version": 3, "ports": ports, "chains": p.decomposition.chains, "actions": p.actions, "poses": p.report}),
        )
    }
}
