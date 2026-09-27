//! Bounded repair ownership and retries; every attempt starts from the original.

use aic_data::layouts::{FactoryEditCatalogs, TransportNetworkEndpoint};
use aic_data::logistics::TransportKind;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Edit, Execution, Job, RouteReport, attempt, incomplete_diagnostic};
use crate::full_factory::generation::changes::PortAssignment;
use crate::full_factory::{catalog, chains::Chain, types::*};

pub const CONTRACT: &str = "factory-route-region-v3";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub attempts: usize,
    pub attempt_work: usize,
    pub chains: usize,
    pub extra_chains: usize,
    pub port_trials: usize,
    pub crossings_per_route: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            attempts: 8,
            attempt_work: 25000,
            // The witness itself bounds the number of changed chains. A smaller
            // explicit cap is available for callers that need one.
            chains: usize::MAX,
            extra_chains: 8,
            port_trials: 4,
            crossings_per_route: 8,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if self.attempt_work == 0
            || !(1..=32).contains(&self.attempts)
            || self.chains == 0
            || self.extra_chains > 32
            || self.port_trials > 16
            || self.crossings_per_route > 8
        {
            return Err(diagnostic(
                "region-invalid-limits",
                "Invalid route-region attempt, chain, port or crossing limits",
                json!(self),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct Contact {
    pub chain: usize,
    pub transport: TransportKind,
    pub position: Point,
    pub end: bool,
    pub direction: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct Blocker {
    pub chain: usize,
    pub frontier_cells: usize,
    pub nearest_goal_distance: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct Attempt {
    pub index: usize,
    pub work_budget: usize,
    pub chains: Vec<usize>,
    pub carried_chains: Vec<usize>,
    pub extra_chains: Vec<usize>,
    pub fixed_contacts: Vec<Contact>,
    pub route_order: Vec<usize>,
    pub prioritized_chain: Option<usize>,
    pub port_assignments: Vec<PortAssignment>,
    pub blockers: Vec<Blocker>,
    pub route_expansions: usize,
    pub routes: Vec<RouteReport>,
    pub diagnostics: Vec<Diagnostic>,
    pub status: &'static str,
    #[serde(skip)]
    pub port_options: Vec<PortAssignment>,
    #[serde(skip)]
    pub locked_ports: IndexSet<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub contract: &'static str,
    pub limits: Limits,
    pub work_budget: usize,
    pub route_expansions: usize,
    pub initial_chains: Vec<usize>,
    pub attempts: Vec<Attempt>,
    pub termination: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<Incomplete>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Incomplete {
    pub attempt: Option<usize>,
    pub code: String,
    pub entity: serde_json::Value,
    pub route_expansions: usize,
}

fn diagnostic(code: &str, message: &str, entity: serde_json::Value) -> Diagnostic {
    let mut d = Diagnostic::new(code, message, entity);
    d.severity = "info";
    d.stage = "local-region-repair";
    d
}

fn remember_incomplete(
    report: &mut Report,
    attempt: Option<usize>,
    code: impl Into<String>,
    entity: serde_json::Value,
) {
    if report.incomplete.is_none() {
        report.incomplete = Some(Incomplete {
            attempt,
            code: code.into(),
            entity,
            route_expansions: report.route_expansions,
        });
    }
}

fn retry_key(
    extras: &IndexSet<usize>,
    ports: &[PortAssignment],
    priority: Option<usize>,
    work_budget: usize,
) -> (Vec<usize>, Vec<(String, String)>, Option<usize>, usize) {
    (
        extras.iter().copied().collect(),
        ports
            .iter()
            .map(|port| (port.terminal.clone(), port.port.clone()))
            .collect(),
        priority,
        work_budget,
    )
}

pub(super) fn contacts(jobs: &[Job], edit: &Edit) -> Vec<Contact> {
    let selected: IndexSet<_> = edit.moves.iter().flat_map(|m| m.instances.iter()).collect();
    let mut result = Vec::new();
    for j in jobs {
        for (end, original, position, terminals, direction) in [
            (
                false,
                j.chain.start(),
                j.start,
                &j.chain.start_terminals,
                j.chain.first,
            ),
            (
                true,
                j.chain.end(),
                j.end,
                &j.chain.end_terminals,
                j.chain.last,
            ),
        ] {
            let owned = terminals.iter().any(|t| {
                matches!(&t.endpoint,
                TransportNetworkEndpoint::Facility { instance, .. } if selected.contains(instance))
            });
            if !owned && original == position {
                result.push(Contact {
                    chain: j.chain.id,
                    transport: j.chain.transport,
                    position,
                    end,
                    direction,
                });
            }
        }
    }
    result
}

pub(super) fn blockers(
    chains: &[Chain],
    changed: &IndexSet<usize>,
    carried: &IndexMap<usize, Vec<Point>>,
    job: &Job,
    frontier: &[Point],
) -> Vec<Blocker> {
    let cells: IndexSet<_> = frontier.iter().copied().collect();
    let mut result = Vec::new();
    for c in chains.iter().filter(|c| {
        c.transport == job.chain.transport
            && !changed.contains(&c.id)
            && !carried.contains_key(&c.id)
    }) {
        let hits: Vec<_> = c
            .cells
            .iter()
            .skip(1)
            .take(c.cells.len().saturating_sub(2))
            .filter(|p| cells.contains(*p))
            .collect();
        if !hits.is_empty() {
            result.push(Blocker {
                chain: c.id,
                frontier_cells: hits.len(),
                nearest_goal_distance: hits.iter().map(|p| p.distance(job.end)).min().unwrap(),
            });
        }
    }
    result.sort_by_key(|b| {
        (
            b.nearest_goal_distance,
            std::cmp::Reverse(b.frontier_cells),
            b.chain,
        )
    });
    result
}

pub(super) fn ports(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    edit: &Edit,
    eligible: &IndexSet<String>,
    job: &Job,
    locked: &IndexSet<String>,
) -> Result<Vec<PortAssignment>> {
    let selected: IndexSet<_> = edit.moves.iter().flat_map(|m| m.instances.iter()).collect();
    if selected.is_empty() {
        return Ok(vec![]);
    }
    let all_ports = catalog::project(w, catalogs)?;
    let body: IndexSet<_> = w.placements.iter().flat_map(body_cells).collect();
    let bounds = extent(w)?;
    let mut alternatives = Vec::new();
    for (ts, other) in [
        (&job.chain.start_terminals, job.end),
        (&job.chain.end_terminals, job.start),
    ] {
        for old in ts {
            if !eligible.contains(&old.id) || locked.contains(&old.id) {
                continue;
            }
            let TransportNetworkEndpoint::Facility { instance, port } = &old.endpoint else {
                continue;
            };
            if !selected.contains(instance) {
                continue;
            }
            let current_port = w
                .transport_networks
                .iter()
                .flat_map(|n| &n.terminals)
                .find(|t| t.id == old.id)
                .and_then(|t| match &t.endpoint {
                    TransportNetworkEndpoint::Facility { port, .. } => Some(port),
                    _ => None,
                })
                .unwrap_or(port);
            for p in &all_ports[instance] {
                if p.id == *current_port
                    || p.direction != old.direction
                    || p.transport != job.chain.transport
                    || !bounds.contains(p.connection)
                    || body.contains(&p.connection)
                    || w.transport_networks
                        .iter()
                        .filter(|n| n.transport == p.transport)
                        .flat_map(|n| &n.terminals)
                        .any(|t| t.id != old.id && Point::from(&t.position) == p.connection)
                    || w.logistics_components.iter().any(|c| {
                        c.transport == p.transport && Point::from(&c.position) == p.connection
                    })
                {
                    continue;
                }
                alternatives.push((p.connection.distance(other), old.id.clone(), p.id.clone()));
            }
        }
    }
    alternatives.sort();
    Ok(alternatives
        .into_iter()
        .map(|(_, terminal, port)| PortAssignment { terminal, port })
        .collect())
}

pub fn reembed(
    witness: &Witness,
    catalogs: &FactoryEditCatalogs,
    edit: &Edit,
    work_budget: usize,
    limits: &Limits,
) -> Result<Execution> {
    limits.validate()?;
    let mut report = Report {
        schema_version: 3,
        contract: CONTRACT,
        limits: limits.clone(),
        work_budget,
        route_expansions: 0,
        initial_chains: vec![],
        attempts: vec![],
        termination: "attempts-exhausted",
        incomplete: None,
    };
    let mut trial = edit.clone();
    let mut extras = IndexSet::new();
    let mut priority = None;
    let mut port_trials = 0;
    let mut tried_ports = IndexSet::new();
    let mut tried_states = IndexSet::new();
    let mut last = Execution {
        status: "rejected",
        witness: None,
        diagnostics: vec![],
        routes: vec![],
        route_expansions: 0,
        region: None,
    };
    for index in 0..limits.attempts {
        let remaining = work_budget.saturating_sub(report.route_expansions);
        if remaining == 0 {
            report.termination = "work-exhausted";
            let last_attempt = report.attempts.last().map(|a| a.index);
            remember_incomplete(
                &mut report,
                last_attempt,
                "region-work-exhausted",
                json!({"work_budget": work_budget}),
            );
            break;
        }
        let mut row = Attempt {
            index,
            work_budget: remaining.min(limits.attempt_work),
            extra_chains: extras.iter().copied().collect(),
            prioritized_chain: priority,
            port_assignments: trial.port_assignments.clone(),
            locked_ports: edit
                .port_assignments
                .iter()
                .map(|p| p.terminal.clone())
                .collect(),
            ..Default::default()
        };
        tried_states.insert(retry_key(
            &extras,
            &trial.port_assignments,
            priority,
            row.work_budget,
        ));
        let execution = attempt(
            witness,
            catalogs,
            &trial,
            remaining.min(limits.attempt_work),
            limits,
            priority,
            &mut row,
        );
        last = execution.unwrap_or_else(|d| Execution {
            status: if incomplete_diagnostic(&d) {
                "search-incomplete"
            } else {
                "rejected"
            },
            witness: None,
            diagnostics: vec![d],
            routes: vec![],
            route_expansions: row.route_expansions,
            region: None,
        });
        report.route_expansions = report
            .route_expansions
            .checked_add(row.route_expansions)
            .filter(|&n| n <= work_budget)
            .ok_or_else(|| {
                diagnostic(
                    "region-work-accounting",
                    "Route expansion accounting exceeded the admitted work budget",
                    json!({"used": report.route_expansions, "attempt": row.route_expansions,
                        "work_budget": work_budget}),
                )
            })?;
        if last.status == "search-incomplete" {
            let (code, entity) = last.diagnostics.first().map_or_else(
                || {
                    (
                        "region-search-incomplete".to_owned(),
                        serde_json::Value::Null,
                    )
                },
                |d| (d.code.clone(), d.entity.clone()),
            );
            remember_incomplete(&mut report, Some(index), code, entity);
        }
        if index == 0 {
            report.initial_chains = row.chains.clone();
        }
        row.status = last.status;
        row.diagnostics = last.diagnostics.clone();
        if last.witness.is_some() || last.status == "unchanged" {
            report.termination = if last.witness.is_some() {
                "candidate"
            } else {
                "unchanged"
            };
            report.attempts.push(row);
            break;
        }
        let failed = row
            .routes
            .last()
            .filter(|r| r.status != "found")
            .map(|r| r.id);
        let port = if port_trials < limits.port_trials {
            row.port_options
                .iter()
                .find(|p| !tried_ports.contains(&(p.terminal.clone(), p.port.clone())))
                .cloned()
        } else {
            None
        };
        let additions: Vec<_> = row
            .blockers
            .iter()
            .filter(|b| !extras.contains(&b.chain))
            .take(2.min(limits.extra_chains.saturating_sub(extras.len())))
            .take(limits.chains.saturating_sub(row.chains.len()))
            .map(|b| b.chain)
            .collect();
        let expand = !additions.is_empty() && (index % 2 == 1 || port.is_none());
        let next_work_budget = work_budget
            .saturating_sub(report.route_expansions)
            .min(limits.attempt_work);
        let can_reorder = failed.is_some_and(|id| {
            priority != Some(id)
                && row
                    .route_order
                    .iter()
                    .position(|&candidate| candidate == id)
                    != Some(0)
                && !tried_states.contains(&retry_key(
                    &extras,
                    &trial.port_assignments,
                    Some(id),
                    next_work_budget,
                ))
        });
        let has_blockers = !row.blockers.is_empty();
        let untried_ports = row
            .port_options
            .iter()
            .filter(|p| !tried_ports.contains(&(p.terminal.clone(), p.port.clone())))
            .count();
        let remaining_blockers = row
            .blockers
            .iter()
            .filter(|b| !extras.contains(&b.chain))
            .count();
        if port_trials >= limits.port_trials && untried_ports > 0 {
            remember_incomplete(
                &mut report,
                Some(index),
                "region-port-trial-limit",
                json!({"tried": port_trials, "limit": limits.port_trials,
                    "untried_options": untried_ports}),
            );
        }
        if additions.is_empty() && remaining_blockers > 0 {
            let (code, limit) = if extras.len() >= limits.extra_chains {
                ("region-extra-chain-limit", limits.extra_chains)
            } else {
                ("region-chain-limit", limits.chains)
            };
            remember_incomplete(
                &mut report,
                Some(index),
                code,
                json!({"remaining_blockers": remaining_blockers, "limit": limit,
                    "selected_chains": row.chains.len(), "extra_chains": extras.len()}),
            );
        }
        report.attempts.push(row);
        if !expand && port.is_none() && !can_reorder {
            report.termination = if has_blockers {
                "region-exhausted"
            } else {
                "fixed-boundary"
            };
            break;
        }
        if report.route_expansions == work_budget {
            report.termination = "work-exhausted";
            remember_incomplete(
                &mut report,
                Some(index),
                "region-work-exhausted",
                json!({"work_budget": work_budget}),
            );
            break;
        }
        if index + 1 == limits.attempts {
            remember_incomplete(
                &mut report,
                Some(index),
                "region-attempt-limit",
                json!({"attempts": limits.attempts}),
            );
            break;
        }
        if expand {
            for id in additions {
                extras.insert(id);
            }
            trial.chain_ids = edit
                .chain_ids
                .iter()
                .copied()
                .chain(extras.iter().copied())
                .collect();
        } else if let Some(port) = port {
            port_trials += 1;
            tried_ports.insert((port.terminal.clone(), port.port.clone()));
            trial.port_assignments = edit.port_assignments.clone();
            trial.port_assignments.push(port);
        }
        priority = failed;
    }
    if last.witness.is_none() && last.status != "unchanged" {
        if let Some(reason) = &report.incomplete {
            last.status = "search-incomplete";
            report.termination = "search-incomplete";
            last.diagnostics.push(diagnostic(
                "region-search-incomplete",
                "Bounded transaction repair ended before search completeness was established; the original factory is preserved",
                json!({"source_attempt": reason.attempt, "source_code": reason.code,
                    "source_entity": reason.entity, "source_route_expansions": reason.route_expansions,
                    "attempts": report.attempts.len(), "route_expansions": report.route_expansions,
                    "work_budget": work_budget}),
            ));
        } else {
            last.diagnostics.push(diagnostic(&format!("region-{}", report.termination),
                "Bounded transaction repair ended without a complete candidate; the original factory is preserved",
                json!({"attempts": report.attempts.len(), "extra_chains": extras.iter().collect::<Vec<_>>(),
                    "route_expansions": report.route_expansions, "work_budget": work_budget})));
        }
    }
    last.routes = report
        .attempts
        .last()
        .map(|a| a.routes.clone())
        .unwrap_or_default();
    last.route_expansions = report.route_expansions;
    last.region = Some(report);
    Ok(last)
}
