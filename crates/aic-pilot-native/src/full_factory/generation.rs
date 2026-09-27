//! Randomized valid starts. Reuses the original seven setup edit algorithms;
//! no Python editor, learned full-factory policy or legacy episode harness.
pub(crate) mod candidates;
mod changes;
mod groups;
mod movement;
mod poses;
pub use aic_layout::rng;
pub mod physical_identity;
mod routing;
mod selection;
pub mod source;
mod spatial;
mod strips;
#[cfg(test)]
mod tests;
use super::{catalog, chains, identity, path, types};
use crate::input::Catalogs;
use aic_data::layouts::{
    FactoryEditBudget, FactoryEditRequest, FactoryEditSession, FactoryEditStatus, WitnessProbeInput,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub identity_limits: physical_identity::Limits,
    pub steps: usize,
    pub elapsed_ms: u64,
    pub route_work: usize,
    pub kinds: Vec<String>,
}
#[derive(Serialize)]
pub struct Output {
    pub original: WitnessProbeInput,
    pub physical: physical_identity::PhysicalIdentity,
    pub constructor_physical: physical_identity::PhysicalIdentity,
    pub records: Vec<Value>,
    pub elapsed_seconds: f64,
}
#[derive(Serialize)]
pub struct Failure {
    pub message: String,
    pub current: WitnessProbeInput,
    pub best: WitnessProbeInput,
    pub records: Vec<Value>,
}
pub fn randomize(
    request: FactoryEditRequest,
    original: WitnessProbeInput,
    catalogs: Arc<Catalogs>,
    config: &Setup,
    seed: u64,
    guard: &dyn Fn() -> std::result::Result<(), String>,
) -> std::result::Result<Output, Box<Failure>> {
    let started = Instant::now();
    let failed = |message: String,
                  current: WitnessProbeInput,
                  best: WitnessProbeInput,
                  records: Vec<Value>| {
        Box::new(Failure {
            message,
            current,
            best,
            records,
        })
    };
    if config.steps == 0
        || config.steps > 32
        || config.elapsed_ms == 0
        || config.route_work == 0
        || config.kinds.is_empty()
        || config.kinds.iter().any(|k| {
            !matches!(
                k.as_str(),
                "pose"
                    | "contract"
                    | "insert"
                    | "port"
                    | "reroute"
                    | "component"
                    | "release_bridge"
            )
        })
    {
        return Err(failed(
            "aic-generation.setup-config: invalid setup bounds/kinds".into(),
            original.clone(),
            original,
            vec![],
        ));
    }
    let mut session = FactoryEditSession::new(
        request,
        original.clone(),
        catalogs.data.clone(),
        FactoryEditBudget {
            proposals: config.steps as u64,
            elapsed_ms: config.elapsed_ms,
        },
    )
    .map_err(|e| {
        failed(
            serde_json::to_string(&e).unwrap(),
            original.clone(),
            original,
            vec![],
        )
    })?;
    let mut records = vec![];
    let identify = |w: &WitnessProbeInput| {
        physical_identity::identify(
            w,
            &catalogs.data,
            &config.identity_limits,
            &format!("setup/{seed}"),
        )
    };
    let constructor_physical = identify(session.snapshot().current)
        .map_err(|message| {
            failed(
                message,
                session.snapshot().current.clone(),
                session.snapshot().best.clone(),
                vec![],
            )
        })?
        .0;
    let mut current = identity::Identity::new(session.snapshot().current);
    let mut best = current.clone();
    let mut memory = selection::Memory::new(&current.physical);
    let vocabulary: poses::VocabularyOptions =
        serde_json::from_value(json!({"pose_domain":"basic","mode":"local"})).unwrap();
    let repair = routing::Limits::default();
    let mut rng = rng::Rng::new(seed);
    let mut cached: Option<(candidates::Domain, Vec<String>)> = None;
    let result = (|| -> types::Result<()> {
        for _ in 0..config.steps {
            guard().map_err(|e| types::Diagnostic::new("aic-generation.resource", e, seed))?;
            if started.elapsed() >= Duration::from_millis(config.elapsed_ms) {
                return Err(types::Diagnostic::new(
                    "aic-generation.setup-timeout",
                    "Randomization exceeded its elapsed budget",
                    seed,
                ));
            }
            if cached.is_none() {
                let domain =
                    candidates::describe(session.snapshot().current, &catalogs.data, &vocabulary)?;
                let keys = domain
                    .actions
                    .iter()
                    .map(|a| {
                        identity::action_key(a, &domain, session.snapshot().current, &best.physical)
                    })
                    .collect();
                cached = Some((domain, keys));
            }
            let (domain, keys) = cached.as_ref().unwrap();
            let (allowed, _) = memory.mask(&current.physical, keys, &domain.actions, true, true)?;
            let mut kinds: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
            for (i, (action, allowed)) in domain.actions.iter().zip(allowed).enumerate() {
                if allowed && config.kinds.iter().any(|k| k == action.kind()) {
                    kinds.entry(action.kind()).or_default().push(i);
                }
            }
            if kinds.is_empty() {
                return Err(types::Diagnostic::new(
                    "aic-generation.setup-exhausted",
                    "No eligible randomized setup action",
                    seed,
                ));
            }
            let choices = kinds.values().nth(rng.index(kinds.len())).unwrap();
            let index = choices[rng.index(choices.len())];
            let action = &domain.actions[index];
            let key = &keys[index];
            let before = current.clone();
            let mut execution = candidates::execute(
                session.snapshot().current,
                &catalogs.data,
                action,
                config.route_work,
                &repair,
            )
            .unwrap_or_else(|d| routing::Execution {
                status: if routing::incomplete_diagnostic(&d) {
                    "search-incomplete"
                } else {
                    "executor_rejected"
                },
                witness: None,
                diagnostics: vec![d],
                routes: vec![],
                route_expansions: 0,
                region: None,
            });
            if execution.status == "unchanged" {
                execution.witness = Some(session.snapshot().current.clone());
            }
            let mut record = json!({"action":action,"executor_status":execution.status,"route_expansions":execution.route_expansions,"diagnostics":execution.diagnostics});
            if execution.status == "search-incomplete" {
                record["routes"] = json!(
                    execution
                        .routes
                        .iter()
                        .map(|route| json!({
                            "id": route.id, "status": route.status, "expanded": route.expanded,
                            "routing_scans": route.routing_scans,
                            "frontier_entries_peak": route.frontier_entries_peak,
                            "preflight_status": route.preflight_status,
                            "preflight_expanded": route.preflight_expanded,
                        }))
                        .collect::<Vec<_>>()
                );
                if let Some(region) = &execution.region {
                    record["region"] = json!({
                        "contract": region.contract, "schema_version": region.schema_version,
                        "termination": region.termination, "attempts": region.attempts.len(),
                        "route_expansions": region.route_expansions, "work_budget": region.work_budget,
                        "attempt_work": region.limits.attempt_work, "incomplete": region.incomplete,
                    });
                }
            }
            if let Some(witness) = &execution.witness {
                let remaining =
                    Duration::from_millis(config.elapsed_ms).saturating_sub(started.elapsed());
                let outcome = session.propose_with_deadline(witness, Instant::now() + remaining);
                if matches!(
                    outcome.status,
                    FactoryEditStatus::Late | FactoryEditStatus::BudgetExhausted
                ) {
                    records.push(record);
                    return Err(types::Diagnostic::new(
                        "aic-generation.setup-timeout",
                        "Randomized proposal exceeded its budget",
                        seed,
                    ));
                }
                if matches!(
                    outcome.status,
                    FactoryEditStatus::Improved
                        | FactoryEditStatus::AcceptedEqual
                        | FactoryEditStatus::AcceptedExploration
                ) {
                    current = identity::Identity::new(session.snapshot().current);
                    if outcome.status == FactoryEditStatus::Improved {
                        best = current.clone();
                    }
                }
                record["acceptance"] = serde_json::to_value(outcome).unwrap();
            }
            memory.record(&before.physical, &current.physical, key);
            let changed = before.ordered != current.ordered;
            record["changed"] = json!(changed);
            records.push(record);
            if changed {
                cached = None;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        return Err(failed(
            serde_json::to_string(&error).unwrap(),
            session.snapshot().current.clone(),
            session.snapshot().best.clone(),
            records,
        ));
    }
    let original = session.snapshot().current.clone();
    Ok(Output {
        physical: identify(&original)
            .map_err(|message| {
                failed(
                    message,
                    original.clone(),
                    session.snapshot().best.clone(),
                    records.clone(),
                )
            })?
            .0,
        original,
        constructor_physical,
        records,
        elapsed_seconds: started.elapsed().as_secs_f64(),
    })
}
