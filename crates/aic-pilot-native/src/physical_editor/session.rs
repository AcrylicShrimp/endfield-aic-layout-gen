//! Bounded procedural sessions over the common physical editor.

use std::time::{Duration, Instant};

use aic_layout::{physical::Metrics, validation};
use serde::{Deserialize, Serialize};

use super::{
    EditWork, PhysicalActionDomain, PhysicalCandidateAuthority, PhysicalCandidateOptions,
    PhysicalEdit, PhysicalEditLimits, ProceduralAdmission, Result, error,
};

pub const SNAPSHOT_SCHEMA: &str = "physical-editor-checkpoint-v4";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PhysicalSessionLimits {
    pub max_actions: u64,
    pub elapsed_ms: u64,
    pub route_expansions: usize,
    pub max_total_route_expansions: usize,
    pub search_padding: i64,
}

impl PhysicalSessionLimits {
    fn validate(&self) -> Result<()> {
        if !(1..=10_000_000).contains(&self.max_actions)
            || !(1..=86_400_000).contains(&self.elapsed_ms)
            || !(1..=1_000_000).contains(&self.route_expansions)
            || !(1..=500_000_000).contains(&self.max_total_route_expansions)
            || !(0..=1000).contains(&self.search_padding)
        {
            return Err(error(
                "session-limits",
                "limits",
                "invalid action, time, routing, or padding limit",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionWork {
    pub route_expansions: usize,
    pub preflight_expansions: usize,
    pub route_calls: usize,
    pub validations: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PhysicalSessionOutcome {
    pub schema: &'static str,
    pub status: &'static str,
    pub charged: bool,
    pub attempts: u64,
    pub revision: u64,
    pub done: bool,
    pub work: EditWork,
    pub best_metrics: Metrics,
    pub diagnostics: Vec<super::PhysicalEditorDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalSessionSnapshot {
    pub schema: String,
    pub source_sha256: String,
    pub catalog_sha256: String,
    pub common_problem_sha256: String,
    pub problem: aic_layout::editor::CompiledProblemV1,
    pub current: aic_layout::editor::PhysicalDraftV1,
    pub best: aic_layout::editor::PhysicalDraftV1,
    pub best_metrics: Metrics,
    pub initial_metrics: Metrics,
    pub limits: PhysicalSessionLimits,
    pub attempts: u64,
    pub revision: u64,
    pub work: SessionWork,
    pub elapsed_ns: u64,
}

pub struct ProceduralSession {
    admission: ProceduralAdmission,
    current: aic_layout::editor::PhysicalDraftV1,
    best: aic_layout::editor::PhysicalDraftV1,
    best_metrics: Metrics,
    initial_metrics: Metrics,
    limits: PhysicalSessionLimits,
    attempts: u64,
    revision: u64,
    work: SessionWork,
    started: Instant,
    elapsed_before: Duration,
    candidates: PhysicalCandidateAuthority,
}

impl ProceduralSession {
    pub fn new(
        source: &[u8],
        catalogs: &validation::Catalogs,
        limits: PhysicalSessionLimits,
    ) -> Result<Self> {
        limits.validate()?;
        let admission = ProceduralAdmission::compile(source, catalogs)?;
        let metrics =
            validation::validate(&admission.owner.original, catalogs).map_err(|failure| {
                error(
                    "source-validation",
                    failure.entity.to_string(),
                    failure.message,
                )
            })?;
        Ok(Self {
            current: admission.draft.clone(),
            best: admission.draft.clone(),
            admission,
            best_metrics: metrics.clone(),
            initial_metrics: metrics,
            limits,
            attempts: 0,
            revision: 0,
            work: SessionWork::default(),
            started: Instant::now(),
            elapsed_before: Duration::ZERO,
            candidates: PhysicalCandidateAuthority::default(),
        })
    }

    fn elapsed(&self) -> Duration {
        self.elapsed_before.saturating_add(self.started.elapsed())
    }

    pub fn done(&self) -> bool {
        self.attempts >= self.limits.max_actions
            || self.elapsed() >= Duration::from_millis(self.limits.elapsed_ms)
            || self.work.route_expansions >= self.limits.max_total_route_expansions
    }

    pub fn source_sha256(&self) -> &str {
        self.admission.source_sha256()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn current(&self) -> &aic_layout::editor::PhysicalDraftV1 {
        &self.current
    }

    pub fn problem(&self) -> &aic_layout::editor::CompiledProblemV1 {
        &self.admission.problem
    }

    pub fn best(&self) -> &aic_layout::editor::PhysicalDraftV1 {
        &self.best
    }

    pub fn observe_actions(
        &mut self,
        options: &PhysicalCandidateOptions,
    ) -> Result<PhysicalActionDomain> {
        self.candidates.enumerate(
            &self.admission.problem,
            &self.current,
            options,
            self.revision,
            self.attempts,
        )
    }

    pub fn step_candidate(
        &mut self,
        source_sha256: &str,
        revision: u64,
        handle: u64,
        root: usize,
        ports: &[usize],
        catalogs: &validation::Catalogs,
    ) -> PhysicalSessionOutcome {
        match self
            .candidates
            .select(handle, revision, self.attempts, root, ports)
        {
            Ok(action) => self.step(source_sha256, revision, action, catalogs),
            Err(failure) => {
                let charged = !self.done();
                if charged {
                    self.attempts += 1;
                }
                PhysicalSessionOutcome {
                    schema: "physical-editor-outcome-v1",
                    status: "rejected",
                    charged,
                    attempts: self.attempts,
                    revision: self.revision,
                    done: self.done(),
                    work: EditWork::default(),
                    best_metrics: self.best_metrics.clone(),
                    diagnostics: vec![failure],
                }
            }
        }
    }

    pub fn step(
        &mut self,
        source_sha256: &str,
        revision: u64,
        action: PhysicalEdit,
        catalogs: &validation::Catalogs,
    ) -> PhysicalSessionOutcome {
        let charged = !self.done();
        let mut status = "rejected";
        let mut diagnostics = Vec::new();
        let mut step_work = EditWork::default();
        if !charged {
            diagnostics.push(error(
                "budget-exhausted",
                "session",
                "session action, time, or routing allowance is exhausted",
            ));
        } else {
            self.attempts += 1;
            if source_sha256 != self.source_sha256() || revision != self.revision {
                diagnostics.push(error(
                    "stale-envelope",
                    "action",
                    "action must bind the source and current revision",
                ));
            } else {
                let mut next = self.current.clone();
                match super::apply_edit_in_place(&self.admission.problem, &mut next, action) {
                    Err(failure) => diagnostics.push(failure),
                    Ok(()) if next == self.current => status = "unchanged",
                    Ok(()) => {
                        let available = self.limits.route_expansions.min(
                            self.limits
                                .max_total_route_expansions
                                .saturating_sub(self.work.route_expansions),
                        );
                        let remaining_elapsed = Duration::from_millis(self.limits.elapsed_ms)
                            .saturating_sub(self.elapsed());
                        let deadline = Instant::now()
                            .checked_add(remaining_elapsed)
                            .unwrap_or_else(Instant::now);
                        match super::repair_pending_validated_until(
                            &self.admission.problem,
                            &mut next,
                            PhysicalEditLimits {
                                route_expansions: available,
                                search_padding: self.limits.search_padding,
                            },
                            || Instant::now() >= deadline,
                        ) {
                            Err(failure) => {
                                let elapsed = failure.code == "physical-editor.elapsed";
                                diagnostics.push(failure);
                                if !elapsed {
                                    status = "draft_accepted";
                                    self.current = next;
                                    self.revision += 1;
                                }
                            }
                            Ok(result) => {
                                step_work = result;
                                self.work.route_expansions += step_work.expanded;
                                self.work.preflight_expansions += step_work.preflight_expanded;
                                self.work.route_calls += step_work.route_calls;
                                if self.elapsed() >= Duration::from_millis(self.limits.elapsed_ms) {
                                    diagnostics.push(error(
                                        "elapsed",
                                        "session",
                                        "edit finished after the deadline; preceding draft retained",
                                    ));
                                } else {
                                    status = "draft_accepted";
                                    if step_work.committed {
                                        let mut completed = self.admission.clone();
                                        completed.draft = next.clone();
                                        match completed.export(catalogs) {
                                            Ok(layout) => {
                                                self.work.validations += 1;
                                                let metrics = validation::validate(
                                                    &layout, catalogs,
                                                )
                                                .expect("export performs the same validation");
                                                status = "validated";
                                                if metric_key(&metrics)
                                                    < metric_key(&self.best_metrics)
                                                {
                                                    self.best = next.clone();
                                                    self.best_metrics = metrics;
                                                    status = "improved";
                                                }
                                            }
                                            Err(failure) => diagnostics.push(failure),
                                        }
                                    }
                                    self.current = next;
                                    self.revision += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        PhysicalSessionOutcome {
            schema: "physical-editor-outcome-v1",
            status,
            charged,
            attempts: self.attempts,
            revision: self.revision,
            done: self.done(),
            work: step_work,
            best_metrics: self.best_metrics.clone(),
            diagnostics,
        }
    }

    pub fn export_best(
        &self,
        catalogs: &validation::Catalogs,
    ) -> Result<aic_layout::physical::Layout> {
        let mut admission = self.admission.clone();
        admission.draft = self.best.clone();
        admission.export(catalogs)
    }

    pub fn checkpoint(&self) -> PhysicalSessionSnapshot {
        PhysicalSessionSnapshot {
            schema: SNAPSHOT_SCHEMA.into(),
            source_sha256: self.source_sha256().into(),
            catalog_sha256: self.admission.problem.catalog_sha256.clone(),
            common_problem_sha256: digest(&self.admission.problem),
            problem: self.admission.problem.clone(),
            current: self.current.clone(),
            best: self.best.clone(),
            best_metrics: self.best_metrics.clone(),
            initial_metrics: self.initial_metrics.clone(),
            limits: self.limits.clone(),
            attempts: self.attempts,
            revision: self.revision,
            work: self.work.clone(),
            elapsed_ns: u64::try_from(self.elapsed().as_nanos()).unwrap_or(u64::MAX),
        }
    }

    pub fn from_checkpoint(
        snapshot: PhysicalSessionSnapshot,
        source: &[u8],
        catalogs: &validation::Catalogs,
    ) -> Result<Self> {
        if snapshot.schema != SNAPSHOT_SCHEMA {
            return Err(error(
                "checkpoint-schema",
                "checkpoint",
                "expected physical editor v4",
            ));
        }
        snapshot.limits.validate()?;
        if snapshot.attempts > snapshot.limits.max_actions
            || snapshot.revision > snapshot.attempts
            || snapshot.work.route_expansions > snapshot.limits.max_total_route_expansions
            || snapshot.work.preflight_expansions > snapshot.work.route_expansions
        {
            return Err(error(
                "checkpoint-counters",
                "checkpoint",
                "invalid resource counters",
            ));
        }
        let admission = ProceduralAdmission::compile(source, catalogs)?;
        if snapshot.source_sha256 != admission.source_sha256()
            || snapshot.catalog_sha256 != catalogs.identity
            || snapshot.problem != admission.problem
            || snapshot.common_problem_sha256 != digest(&snapshot.problem)
        {
            return Err(error(
                "checkpoint-binding",
                "checkpoint",
                "source, catalog, or common problem differs from the snapshot",
            ));
        }
        super::validate_draft(&snapshot.problem, &snapshot.current)?;
        super::validate_draft(&snapshot.problem, &snapshot.best)?;
        let mut best_admission = admission.clone();
        best_admission.draft = snapshot.best.clone();
        let best_layout = best_admission.export(catalogs)?;
        let best_metrics = validation::validate(&best_layout, catalogs).map_err(|failure| {
            error(
                "checkpoint-best",
                failure.entity.to_string(),
                failure.message,
            )
        })?;
        if best_metrics != snapshot.best_metrics
            || metric_key(&best_metrics) > metric_key(&snapshot.initial_metrics)
        {
            return Err(error(
                "checkpoint-best",
                "checkpoint",
                "best metrics are inconsistent or worse than the initial layout",
            ));
        }
        Ok(Self {
            admission,
            current: snapshot.current,
            best: snapshot.best,
            best_metrics,
            initial_metrics: snapshot.initial_metrics,
            limits: snapshot.limits,
            attempts: snapshot.attempts,
            revision: snapshot.revision,
            work: snapshot.work,
            started: Instant::now(),
            elapsed_before: Duration::from_nanos(snapshot.elapsed_ns),
            candidates: PhysicalCandidateAuthority::default(),
        })
    }
}

fn metric_key(metrics: &Metrics) -> (u64, i64, i64, u64) {
    (metrics.area, metrics.width, metrics.height, metrics.turns)
}

fn digest(value: &impl Serialize) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(serde_json::to_vec(value).expect("serializable common problem"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
