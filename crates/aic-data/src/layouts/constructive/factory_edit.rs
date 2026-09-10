//! Complete-factory acceptance and bounded transactions over production physical DTOs.

mod metrics;
#[cfg(test)]
mod tests;
mod validation;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::WitnessProbeInput;
use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{Rate, ValidatedRecipeBook};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryEditRequest {
    pub schema_version: u32,
    pub instances: Vec<FactoryEditInstance>,
    pub terminals: Vec<FactoryEditTerminal>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryEditInstance {
    pub instance: String,
    pub recipe: String,
    pub utilization: Rate,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryEditEndpoint {
    Facility { instance: String },
    External { node: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryEditTerminal {
    pub endpoint: FactoryEditEndpoint,
    pub direction: FacilityPortDirection,
    pub item: String,
    pub rate: Rate,
}

/// Owned, immutable for a session. All catalog values are loaded by the caller.
pub struct FactoryEditCatalogs {
    pub facilities: ValidatedFacilityCatalog,
    pub recipes: ValidatedRecipeBook,
    pub items: ValidatedItemCatalog,
    pub transports: ValidatedTransportCatalog,
    pub components: ValidatedLogisticsComponentCatalog,
}

#[derive(Debug, Clone, Serialize)]
pub struct FactoryEditDiagnostic {
    pub code: &'static str,
    pub severity: &'static str,
    pub stage: &'static str,
    pub entity: Option<String>,
    pub message: String,
}

type Result<T> = std::result::Result<T, FactoryEditDiagnostic>;

fn error(
    code: &'static str,
    entity: impl Into<String>,
    message: impl Into<String>,
) -> FactoryEditDiagnostic {
    FactoryEditDiagnostic {
        code,
        severity: "error",
        stage: "factory-edit",
        entity: Some(entity.into()),
        message: message.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FactoryLayoutMetrics {
    pub definition: &'static str,
    pub score: [u64; 3],
    pub width: u64,
    pub height: u64,
    pub facility_xy_cells: u64,
    pub transport_xy_cells: u64,
    pub occupied_xy_cells: u64,
    pub empty_xy_cells: u64,
}

#[derive(Debug, Serialize)]
pub struct FactoryLayoutAcceptance {
    pub schema_version: u32,
    pub factory_validated: bool,
    pub metrics: Option<FactoryLayoutMetrics>,
    pub diagnostics: Vec<FactoryEditDiagnostic>,
    pub elapsed_ms: f64,
}

pub fn validate_factory_layout(
    request: &FactoryEditRequest,
    input: &WitnessProbeInput,
    catalogs: &FactoryEditCatalogs,
) -> FactoryLayoutAcceptance {
    let start = Instant::now();
    let result = validation::request(request, catalogs)
        .and_then(|()| validation::accept(request, input, catalogs));
    let (metrics, diagnostics) = match result {
        Ok((_, metrics)) => (Some(metrics), vec![]),
        Err(d) => (None, vec![d]),
    };
    FactoryLayoutAcceptance {
        schema_version: 1,
        factory_validated: diagnostics.is_empty(),
        metrics,
        diagnostics,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryEditBudget {
    pub proposals: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryEditStatus {
    Improved,
    AcceptedExploration,
    AcceptedEqual,
    Rejected,
    Late,
    BudgetExhausted,
}

#[derive(Debug, Serialize)]
pub struct FactoryEditOutcome {
    pub schema_version: u32,
    pub proposal: u64,
    pub charged: bool,
    pub status: FactoryEditStatus,
    pub candidate_validated: bool,
    pub candidate: Option<FactoryLayoutMetrics>,
    pub current: FactoryLayoutMetrics,
    pub best: FactoryLayoutMetrics,
    pub diagnostics: Vec<FactoryEditDiagnostic>,
    pub validation_ms: f64,
    pub elapsed_ms: f64,
}

#[derive(Serialize)]
pub struct FactoryEditSnapshot<'a> {
    pub schema_version: u32,
    pub factory_validated: bool,
    pub proposals: u64,
    pub budget: FactoryEditBudget,
    pub done: bool,
    pub initialization_ms: f64,
    pub elapsed_ms: f64,
    pub request: &'a FactoryEditRequest,
    pub initial_metrics: &'a FactoryLayoutMetrics,
    pub current_metrics: &'a FactoryLayoutMetrics,
    pub best_metrics: &'a FactoryLayoutMetrics,
    pub initial: &'a WitnessProbeInput,
    pub current: &'a WitnessProbeInput,
    pub best: &'a WitnessProbeInput,
}

/// Only fully accepted private proposals enter current/best. No routing is implicit.
pub struct FactoryEditSession {
    catalogs: Arc<FactoryEditCatalogs>,
    request: FactoryEditRequest,
    initial: WitnessProbeInput,
    current: WitnessProbeInput,
    best: WitnessProbeInput,
    initial_metrics: FactoryLayoutMetrics,
    current_metrics: FactoryLayoutMetrics,
    best_metrics: FactoryLayoutMetrics,
    budget: FactoryEditBudget,
    proposals: u64,
    initialization_ms: f64,
    started: Instant,
}

impl FactoryEditSession {
    pub fn new(
        request: FactoryEditRequest,
        initial: WitnessProbeInput,
        catalogs: Arc<FactoryEditCatalogs>,
        budget: FactoryEditBudget,
    ) -> Result<Self> {
        let began = Instant::now();
        if budget.proposals == 0 || budget.elapsed_ms == 0 {
            return Err(error(
                "factory.budget",
                "budget",
                "Proposal and elapsed budgets must be positive",
            ));
        }
        validation::request(&request, &catalogs)?;
        let (initial, metrics) = validation::accept(&request, &initial, &catalogs)?;
        Ok(Self {
            catalogs,
            request,
            initial: initial.clone(),
            current: initial.clone(),
            best: initial,
            initial_metrics: metrics.clone(),
            current_metrics: metrics.clone(),
            best_metrics: metrics,
            budget,
            proposals: 0,
            initialization_ms: began.elapsed().as_secs_f64() * 1000.0,
            started: Instant::now(),
        })
    }

    fn expired(&self) -> bool {
        self.started.elapsed() >= Duration::from_millis(self.budget.elapsed_ms)
    }

    pub fn done(&self) -> bool {
        self.proposals >= self.budget.proposals || self.expired()
    }

    pub fn snapshot(&self) -> FactoryEditSnapshot<'_> {
        FactoryEditSnapshot {
            schema_version: 1,
            factory_validated: true,
            proposals: self.proposals,
            budget: self.budget,
            done: self.done(),
            initialization_ms: self.initialization_ms,
            elapsed_ms: self.started.elapsed().as_secs_f64() * 1000.0,
            request: &self.request,
            initial_metrics: &self.initial_metrics,
            current_metrics: &self.current_metrics,
            best_metrics: &self.best_metrics,
            initial: &self.initial,
            current: &self.current,
            best: &self.best,
        }
    }

    /// JSON decoding is part of the charged proposal and deadline, not free preflight.
    pub fn propose_json(&mut self, candidate: &str) -> FactoryEditOutcome {
        if let Some(outcome) = self.reserve_proposal() {
            return outcome;
        }
        let began = Instant::now();
        let result = if candidate.len() > 16 * 1024 * 1024 {
            Err(error(
                "factory.proposal-schema",
                "proposal",
                "Proposal JSON exceeds the 16 MiB input work guard",
            ))
        } else {
            serde_json::from_str::<WitnessProbeInput>(candidate)
                .map_err(|e| error("factory.proposal-schema", "proposal", e.to_string()))
                .and_then(|input| validation::accept(&self.request, &input, &self.catalogs))
        };
        self.finish(result, began.elapsed().as_secs_f64() * 1000.0, None)
    }

    fn reserve_proposal(&mut self) -> Option<FactoryEditOutcome> {
        if self.done() {
            return Some(self.outcome(
                FactoryEditStatus::BudgetExhausted,
                false,
                None,
                vec![error(
                    "factory.budget-exhausted",
                    "budget",
                    "No proposal accepted after the proposal or elapsed budget",
                )],
                0.0,
            ));
        }
        self.proposals += 1;
        None
    }

    pub fn propose(&mut self, candidate: &WitnessProbeInput) -> FactoryEditOutcome {
        if let Some(outcome) = self.reserve_proposal() {
            return outcome;
        }
        let began = Instant::now();
        let result = validation::accept(&self.request, candidate, &self.catalogs);
        let elapsed_ms = began.elapsed().as_secs_f64() * 1000.0;
        self.finish(result, elapsed_ms, None)
    }

    /// An owning episode can have an earlier deadline than this acceptance session.
    /// Check both deadlines before committing the validated physical state.
    pub fn propose_with_deadline(
        &mut self,
        candidate: &WitnessProbeInput,
        deadline: Instant,
    ) -> FactoryEditOutcome {
        if let Some(outcome) = self.reserve_proposal() {
            return outcome;
        }
        let began = Instant::now();
        let result = validation::accept(&self.request, candidate, &self.catalogs);
        self.finish(
            result,
            began.elapsed().as_secs_f64() * 1000.0,
            Some(deadline),
        )
    }

    fn finish(
        &mut self,
        result: Result<(WitnessProbeInput, FactoryLayoutMetrics)>,
        validation_ms: f64,
        deadline: Option<Instant>,
    ) -> FactoryEditOutcome {
        let late = self.expired() || deadline.is_some_and(|d| Instant::now() >= d);
        let (candidate, metrics) = match result {
            Ok(value) => value,
            Err(d) => {
                return self.outcome(
                    if late {
                        FactoryEditStatus::Late
                    } else {
                        FactoryEditStatus::Rejected
                    },
                    true,
                    None,
                    vec![d],
                    validation_ms,
                );
            }
        };
        if late {
            return self.outcome(
                FactoryEditStatus::Late,
                true,
                Some(metrics),
                vec![error(
                    "factory.late-result",
                    "budget",
                    "Validation completed after the elapsed budget; retained layouts are unchanged",
                )],
                validation_ms,
            );
        }
        let improved = metrics.score < self.best_metrics.score;
        let status = if improved {
            FactoryEditStatus::Improved
        } else if metrics.score == self.current_metrics.score {
            FactoryEditStatus::AcceptedEqual
        } else {
            FactoryEditStatus::AcceptedExploration
        };
        if improved {
            self.best = candidate.clone();
            self.best_metrics = metrics.clone();
        }
        self.current = candidate;
        self.current_metrics = metrics.clone();
        self.outcome(status, true, Some(metrics), vec![], validation_ms)
    }

    fn outcome(
        &self,
        status: FactoryEditStatus,
        charged: bool,
        candidate: Option<FactoryLayoutMetrics>,
        diagnostics: Vec<FactoryEditDiagnostic>,
        validation_ms: f64,
    ) -> FactoryEditOutcome {
        FactoryEditOutcome {
            schema_version: 1,
            proposal: self.proposals,
            charged,
            status,
            candidate_validated: candidate.is_some(),
            candidate,
            current: self.current_metrics.clone(),
            best: self.best_metrics.clone(),
            diagnostics,
            validation_ms,
            elapsed_ms: self.started.elapsed().as_secs_f64() * 1000.0,
        }
    }
}
