//! Bounded present/history conflict ordering for native-legal repair actions.

use std::collections::{BTreeMap, BTreeSet};

use aic_layout::{geometry::Point, logistics::TransportKind};
use aic_pilot_native::constructive::{
    Diagnostic, Session,
    routing_candidates::{
        AllowanceStop, AllowanceUsage, Candidate, RepairCandidate, RepairDraft, RepairRequest,
        RepairScope, Request, RequestAllowance, RequestFailure, Termination, WorkKind,
        publish_repair_candidate,
    },
};
use anyhow::Result;
use serde::Serialize;

use super::{
    Counters, Factory, NetworkWitnessKey, ROUTE_ATTEMPT_SAMPLE_LIMIT, RouteReport,
    inference_reservation_bytes, network_witness_key_bounded,
    network_witness_key_reservation_bytes, record_route_report,
};

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum NegotiationTermination {
    Disabled,
    CandidateLimit,
    Exhausted,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct NegotiationSummary {
    pub(super) selected: Vec<String>,
    pub(super) termination: NegotiationTermination,
    pub(super) candidates: usize,
    pub(super) attempts: usize,
    pub(super) retries: usize,
    pub(super) legalization_failures: usize,
    pub(super) duplicate_candidates: usize,
    pub(super) allowance: AllowanceUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stop: Option<AllowanceStop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_failure: Option<String>,
}

pub(super) struct NegotiationReport {
    pub(super) request: Option<RepairRequest>,
    pub(super) actions: Vec<RepairCandidate>,
    pub(super) summary: NegotiationSummary,
}

enum AttemptResult {
    Candidate(RepairCandidate),
    NoRoute,
    Rejected(Diagnostic),
}

pub(super) fn record_negotiation_report(counters: &mut Counters, report: &NegotiationReport) {
    if report.summary.termination == NegotiationTermination::Disabled {
        return;
    }
    counters.repair_tasks += 1;
    counters.repair_attempts += report.summary.attempts;
    counters.repair_samples += report.actions.len();
    counters.repair_legalization_failures += report.summary.legalization_failures;
    if report.summary.termination == NegotiationTermination::Unknown {
        counters.repair_unknown += 1;
    }
    if counters.repair_attempt_samples.len() < ROUTE_ATTEMPT_SAMPLE_LIMIT {
        counters.repair_attempt_samples.push(report.summary.clone());
    }
}

pub(super) fn diagnostic_is_stop(diagnostic: &Diagnostic) -> bool {
    matches!(
        diagnostic.code.as_str(),
        "constructive.repair-work-limit"
            | "constructive.repair-deadline"
            | "constructive.repair-resource-limit"
    )
}

fn stopped_route(report: &RouteReport) -> bool {
    matches!(
        report.termination,
        Termination::WorkLimit | Termination::Deadline | Termination::ResourceLimit
    )
}

fn next_combination(indices: &mut [usize], domain: usize) -> bool {
    for position in (0..indices.len()).rev() {
        if indices[position] < domain - (indices.len() - position) {
            indices[position] += 1;
            for following in position + 1..indices.len() {
                indices[following] = indices[following - 1] + 1;
            }
            return true;
        }
    }
    false
}

pub(super) fn next_untried(
    order: &[String],
    size: usize,
    max_variant: usize,
    tried: &BTreeMap<Vec<String>, usize>,
    allowance: &mut RequestAllowance,
) -> std::result::Result<Option<(Vec<String>, usize)>, AllowanceStop> {
    let mut indices = (0..size).collect::<Vec<_>>();
    loop {
        let mut removed = indices
            .iter()
            .map(|&index| order[index].clone())
            .collect::<Vec<_>>();
        removed.sort();
        allowance.charge(WorkKind::RoutingScans, 1)?;
        let variant = tried.get(&removed).copied().unwrap_or(0);
        if variant < max_variant {
            allowance.charge(WorkKind::Variants, 1)?;
            return Ok(Some((removed, variant)));
        }
        if !next_combination(&mut indices, order.len()) {
            return Ok(None);
        }
    }
}

pub(super) fn historical_price(
    session: &Session,
    ids: &[(String, u64)],
    history: &BTreeMap<(TransportKind, Point), u32>,
    allowance: &mut RequestAllowance,
) -> std::result::Result<Vec<String>, AllowanceStop> {
    let mut priced = Vec::with_capacity(ids.len());
    for (id, present) in ids {
        let mut score = *present;
        if let Some((transport, cells)) = session.committed_repair_cells(id) {
            for &cell in cells {
                allowance.charge(WorkKind::RouteCells, 1)?;
                score = score.saturating_add(u64::from(
                    history.get(&(transport, cell)).copied().unwrap_or(0),
                ));
            }
        }
        priced.push((std::cmp::Reverse(score), id.clone()));
    }
    priced.sort();
    Ok(priced.into_iter().map(|(_, id)| id).collect())
}

pub(super) fn update_history(
    removed: &[String],
    conflict_cells: &BTreeMap<String, Vec<(TransportKind, Point)>>,
    history: &mut BTreeMap<(TransportKind, Point), u32>,
    allowance: &mut RequestAllowance,
    reserved_bytes: &mut usize,
) -> std::result::Result<(), AllowanceStop> {
    for id in removed {
        allowance.charge(WorkKind::ValidationRecords, 1)?;
        let Some(cells) = conflict_cells.get(id) else {
            continue;
        };
        for &(transport, cell) in cells {
            allowance.charge(WorkKind::RouteCells, 1)?;
            let key = (transport, cell);
            if !history.contains_key(&key) {
                allowance.reserve_peak(96)?;
                *reserved_bytes = reserved_bytes.saturating_add(96);
            }
            let price = history.entry(key).or_default();
            *price = price.saturating_add(2);
        }
    }
    Ok(())
}

pub(super) fn publish_unique_repair(
    request: &RepairRequest,
    candidate: &RepairCandidate,
    semantic: &mut BTreeSet<(Vec<String>, NetworkWitnessKey)>,
    allowance: &mut RequestAllowance,
) -> std::result::Result<Option<(usize, usize)>, Diagnostic> {
    let key_bytes = network_witness_key_reservation_bytes(&candidate.replacements)
        .saturating_add(
            candidate
                .removed
                .iter()
                .map(|id| id.len().saturating_add(64))
                .sum::<usize>(),
        )
        .saturating_add(256);
    allowance
        .reserve_peak(key_bytes)
        .map_err(|stop| repair_stop("repair", &stop))?;
    let key = match network_witness_key_bounded(&candidate.replacements, allowance) {
        Ok(routes) => (candidate.removed.clone(), routes),
        Err(stop) => {
            allowance.release_peak(key_bytes);
            return Err(repair_stop("repair", &stop));
        }
    };
    if semantic.contains(&key) {
        allowance.release_peak(key_bytes);
        return Ok(None);
    }
    let serialized = match publish_repair_candidate(request, candidate, allowance) {
        Ok(size) => size,
        Err(failure) => {
            allowance.release_peak(key_bytes);
            return Err(failure);
        }
    };
    let retained = serialized
        .saturating_add(
            candidate
                .replacements
                .iter()
                .map(|route| {
                    route
                        .cells
                        .len()
                        .saturating_mul(32)
                        .saturating_add(route.commands.len().saturating_mul(128))
                })
                .sum::<usize>(),
        )
        .saturating_add(4096);
    if let Err(stop) = allowance.reserve_peak(retained) {
        allowance.release_peak(key_bytes);
        return Err(repair_stop("repair", &stop));
    }
    semantic.insert(key);
    Ok(Some((retained, key_bytes)))
}

impl Factory<'_> {
    pub(super) fn negotiated_repair_actions(
        &self,
        session: &Session,
        selected: &[String],
        candidate_slots: usize,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
    ) -> Result<NegotiationReport> {
        let mut summary = NegotiationSummary {
            selected: selected.to_vec(),
            termination: NegotiationTermination::Disabled,
            candidates: 0,
            attempts: 0,
            retries: 0,
            legalization_failures: 0,
            duplicate_candidates: 0,
            allowance: allowance.usage(),
            stop: None,
            last_failure: None,
        };
        if self.repair_max_removed == 0 || selected.is_empty() || candidate_slots == 0 {
            return Ok(NegotiationReport {
                request: None,
                actions: Vec::new(),
                summary,
            });
        }
        let (scope, scope_bytes) = match session.repair_scope_for_policy(
            selected,
            self.repair_max_removed,
            self.candidate,
            allowance,
        ) {
            Ok(scope) => scope,
            Err(failure) if failure.code == "constructive.repair-empty-scope" => {
                return Ok(NegotiationReport {
                    request: None,
                    actions: Vec::new(),
                    summary,
                });
            }
            Err(failure) if diagnostic_is_stop(&failure) => {
                summary.termination = NegotiationTermination::Unknown;
                summary.stop = allowance.last_stop();
                summary.last_failure = Some(failure.to_string());
                summary.allowance = allowance.usage();
                return Ok(NegotiationReport {
                    request: None,
                    actions: Vec::new(),
                    summary,
                });
            }
            Err(failure) => return Err(failure.into()),
        };
        let request = scope.request();
        summary.termination = NegotiationTermination::Exhausted;
        let (present, conflict_cells, order_bytes) = match scope.conflict_order(allowance) {
            Ok(value) => value,
            Err(failure) if diagnostic_is_stop(&failure) => {
                summary.termination = NegotiationTermination::Unknown;
                summary.stop = allowance.last_stop();
                summary.last_failure = Some(failure.to_string());
                summary.allowance = allowance.usage();
                allowance.release_peak(scope_bytes);
                return Ok(NegotiationReport {
                    request: Some(request.clone()),
                    actions: Vec::new(),
                    summary,
                });
            }
            Err(failure) => {
                allowance.release_peak(scope_bytes);
                return Err(failure.into());
            }
        };
        let mut history = BTreeMap::new();
        let mut history_bytes = 0usize;
        let mut actions = Vec::new();
        let mut output_bytes = 0usize;
        let mut semantic = BTreeSet::<(Vec<String>, NetworkWitnessKey)>::new();
        let mut semantic_bytes = 0usize;
        let mut stopped = false;
        let mut tried = BTreeMap::new();
        let mut tried_bytes = 0usize;
        let mut fatal = None;
        'sizes: for size in 1..=request.max_removed {
            loop {
                if let Err(stop) = allowance.check_deadline() {
                    summary.stop = Some(stop);
                    stopped = true;
                    break 'sizes;
                }
                let order = match historical_price(session, &present, &history, allowance) {
                    Ok(order) => order,
                    Err(stop) => {
                        summary.stop = Some(stop);
                        stopped = true;
                        break 'sizes;
                    }
                };
                let (removed, variant) = match next_untried(
                    &order,
                    size,
                    self.candidate.max_candidates,
                    &tried,
                    allowance,
                ) {
                    Ok(Some(value)) => value,
                    Ok(None) => break,
                    Err(stop) => {
                        summary.stop = Some(stop);
                        stopped = true;
                        break 'sizes;
                    }
                };
                let memory = removed
                    .iter()
                    .map(|id| id.len().saturating_add(128))
                    .sum::<usize>()
                    .saturating_add(128);
                if !tried.contains_key(&removed) {
                    if let Err(stop) = allowance.reserve_peak(memory) {
                        summary.stop = Some(stop);
                        stopped = true;
                        break 'sizes;
                    }
                    tried_bytes = tried_bytes.saturating_add(memory);
                }
                tried.insert(removed.clone(), variant + 1);
                summary.retries += usize::from(summary.attempts > 0);
                summary.attempts += 1;
                let attempt = self.try_repair_neighborhood(
                    &scope, selected, &removed, variant, &history, counters, allowance,
                );
                let failed = match attempt {
                    Ok(AttemptResult::Candidate(candidate)) => {
                        match publish_unique_repair(request, &candidate, &mut semantic, allowance) {
                            Ok(Some((retained, key_bytes))) => {
                                output_bytes = output_bytes.saturating_add(retained);
                                semantic_bytes = semantic_bytes.saturating_add(key_bytes);
                                actions.push(candidate);
                                if actions.len() >= candidate_slots {
                                    summary.termination = NegotiationTermination::CandidateLimit;
                                    break 'sizes;
                                }
                            }
                            Ok(None) => summary.duplicate_candidates += 1,
                            Err(failure) if diagnostic_is_stop(&failure) => {
                                summary.stop = allowance.last_stop();
                                summary.last_failure = Some(failure.to_string());
                                stopped = true;
                                break 'sizes;
                            }
                            Err(failure) => {
                                fatal = Some(failure);
                                break 'sizes;
                            }
                        }
                        false
                    }
                    Ok(AttemptResult::NoRoute) => true,
                    Ok(AttemptResult::Rejected(failure)) => {
                        summary.legalization_failures += 1;
                        summary.last_failure = Some(failure.to_string());
                        true
                    }
                    Err(failure) if diagnostic_is_stop(&failure) => {
                        summary.stop = if failure.code == "constructive.repair-resource-limit" {
                            None
                        } else {
                            allowance
                                .last_stop()
                                .or_else(|| allowance.check_deadline().err())
                        };
                        summary.last_failure = Some(failure.to_string());
                        stopped = true;
                        break 'sizes;
                    }
                    Err(failure) => {
                        fatal = Some(failure);
                        break 'sizes;
                    }
                };
                if failed {
                    if let Err(stop) = update_history(
                        &removed,
                        &conflict_cells,
                        &mut history,
                        allowance,
                        &mut history_bytes,
                    ) {
                        summary.stop = Some(stop);
                        stopped = true;
                        break 'sizes;
                    }
                }
            }
        }
        allowance.release_peak(
            order_bytes
                .saturating_add(history_bytes)
                .saturating_add(tried_bytes)
                .saturating_add(scope_bytes)
                .saturating_add(output_bytes)
                .saturating_add(semantic_bytes),
        );
        if let Some(failure) = fatal {
            return Err(failure.into());
        }
        if stopped {
            summary.termination = NegotiationTermination::Unknown;
        }
        summary.candidates = actions.len();
        summary.allowance = allowance.usage();
        Ok(NegotiationReport {
            request: Some(request.clone()),
            actions,
            summary,
        })
    }

    fn try_repair_neighborhood(
        &self,
        scope: &RepairScope<'_>,
        selected: &[String],
        removed: &[String],
        variant: usize,
        history: &BTreeMap<(TransportKind, Point), u32>,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
    ) -> std::result::Result<AttemptResult, Diagnostic> {
        let mut draft = scope.start_draft(removed, allowance)?;
        for (depth, id) in selected.iter().enumerate() {
            let chosen = if depth == 0 { variant } else { 0 };
            let Some(member) =
                self.search_repair_member(&mut draft, id, chosen, history, counters)?
            else {
                return Ok(AttemptResult::NoRoute);
            };
            if let Err(failure) = draft.apply_member(member) {
                return if diagnostic_is_stop(&failure) {
                    Err(failure)
                } else {
                    Ok(AttemptResult::Rejected(failure))
                };
            }
        }
        for id in removed {
            let Some(member) = self.search_repair_member(&mut draft, id, 0, history, counters)?
            else {
                return Ok(AttemptResult::NoRoute);
            };
            if let Err(failure) = draft.apply_member(member) {
                return if diagnostic_is_stop(&failure) {
                    Err(failure)
                } else {
                    Ok(AttemptResult::Rejected(failure))
                };
            }
        }
        let (_, candidate, _) = match draft.finish() {
            Ok(complete) => complete,
            Err(failure) if diagnostic_is_stop(&failure) => return Err(failure),
            Err(failure) => return Ok(AttemptResult::Rejected(failure)),
        };
        Ok(AttemptResult::Candidate(candidate))
    }

    fn search_repair_member(
        &self,
        draft: &mut RepairDraft<'_>,
        id: &str,
        variant: usize,
        history: &BTreeMap<(TransportKind, Point), u32>,
        counters: &mut Counters,
    ) -> std::result::Result<Option<Candidate>, Diagnostic> {
        let request = Request {
            state_revision: draft.session().revision(),
            connection: id.into(),
        };
        let input = {
            let (session, allowance) = draft.session_and_allowance();
            session.route_guidance_input_with_allowance(
                &request,
                self.candidate,
                &self.domain,
                allowance,
            )
        };
        let input = match input {
            Ok(input) => input,
            Err(RequestFailure::Invalid(failure))
                if failure.code.starts_with("constructive.route-guidance") =>
            {
                counters.repair_guidance_fallbacks += 1;
                return self.anchor_repair_member(draft, id, variant, counters);
            }
            Err(RequestFailure::Invalid(failure)) => return Err(failure),
            Err(RequestFailure::Stopped(stop)) if stop.reason != "deadline" => {
                counters.repair_guidance_fallbacks += 1;
                return self.anchor_repair_member(draft, id, variant, counters);
            }
            Err(RequestFailure::Stopped(stop)) => return Err(repair_stop(id, &stop)),
            Err(RequestFailure::PendingPlacement { .. }) => return Ok(None),
            Err(RequestFailure::NoCompatibleEndpoints { .. }) => return Ok(None),
        };
        let elements = input
            .grid
            .len()
            .saturating_add(
                input.graph.entity_features.len()
                    * aic_pilot_native::constructive::learning::ENTITY_FEATURES,
            )
            .saturating_add(
                input.graph.relation_features.len()
                    * aic_pilot_native::constructive::learning::RELATION_FEATURES,
            )
            .saturating_add(
                input.graph.spatial_features.len()
                    * aic_pilot_native::constructive::learning::SPATIAL_FEATURES,
            );
        let inference_bytes = inference_reservation_bytes(&input, self.guidance_heads);
        let mut inference_reserved = false;
        let inference = {
            let (_, allowance) = draft.session_and_allowance();
            allowance
                .preflight(WorkKind::InferenceElements, elements)
                .and_then(|_| {
                    allowance.reserve_peak(inference_bytes)?;
                    inference_reserved = true;
                    allowance.charge(WorkKind::InferenceElements, elements)
                })
        };
        if let Err(stop) = inference {
            let reserved = input.reserved_bytes;
            drop(input);
            draft
                .session_and_allowance()
                .1
                .release_peak(reserved.saturating_add(if inference_reserved {
                    inference_bytes
                } else {
                    0
                }));
            if stop.reason != "deadline" {
                counters.repair_guidance_fallbacks += 1;
                return self.anchor_repair_member(draft, id, variant, counters);
            }
            return Err(repair_stop(id, &stop));
        }
        let predicted = self
            .guidance
            .predict_native(&[&input], self.guidance_version, self.device);
        let reserved = input.reserved_bytes;
        drop(input);
        draft
            .session_and_allowance()
            .1
            .release_peak(reserved.saturating_add(inference_bytes));
        let mut guides = match predicted {
            Ok(guides) => guides,
            Err(_) => {
                counters.repair_guidance_fallbacks += 1;
                return self.anchor_repair_member(draft, id, variant, counters);
            }
        };
        counters.guidance_batches += 1;
        counters.guidance_inputs += 1;
        let mut guide = guides.pop().expect("one guidance input returns one output");
        let guidance_bytes = guide.heads.iter().fold(0usize, |total, head| {
            total.saturating_add(head.directed_costs.len().saturating_mul(32))
        });
        if let Err(stop) = draft.session_and_allowance().1.reserve_peak(guidance_bytes) {
            if stop.reason != "deadline" {
                counters.repair_guidance_fallbacks += 1;
                return self.anchor_repair_member(draft, id, variant, counters);
            }
            return Err(repair_stop(id, &stop));
        }
        let prices = {
            let (session, allowance) = draft.session_and_allowance();
            session.repair_live_cell_prices(id, allowance)
        };
        let (transport, present, present_bytes) = match prices {
            Ok(prices) => prices,
            Err(failure) => {
                draft.session_and_allowance().1.release_peak(guidance_bytes);
                return Err(failure);
            }
        };
        let priced = (|| {
            for head in &mut guide.heads {
                for directed in &mut head.directed_costs {
                    draft
                        .session_and_allowance()
                        .1
                        .charge(WorkKind::RoutingScans, 1)
                        .map_err(|stop| repair_stop(id, &stop))?;
                    let present_price = present.get(&directed.point).copied().unwrap_or(0);
                    let history_price = history
                        .get(&(transport, directed.point))
                        .copied()
                        .unwrap_or(0);
                    counters.repair_priced_edges +=
                        usize::from(present_price > 0 || history_price > 0);
                    directed.cost = directed
                        .cost
                        .saturating_add(present_price)
                        .saturating_add(history_price)
                        .min(1_000_000);
                }
            }
            Ok(())
        })();
        drop(present);
        draft.session_and_allowance().1.release_peak(present_bytes);
        if let Err(failure) = priced {
            draft.session_and_allowance().1.release_peak(guidance_bytes);
            return Err(failure);
        }
        let report = draft.route_candidates(id, self.candidate, Some(&guide));
        drop(guide);
        draft.session_and_allowance().1.release_peak(guidance_bytes);
        match report {
            Ok(report) => choose_repair_candidate(report, id, variant, counters),
            Err(failure) if failure.code.starts_with("constructive.route-guidance") => {
                counters.repair_guidance_fallbacks += 1;
                self.anchor_repair_member(draft, id, variant, counters)
            }
            Err(failure) => Err(failure),
        }
    }

    fn anchor_repair_member(
        &self,
        draft: &mut RepairDraft<'_>,
        id: &str,
        variant: usize,
        counters: &mut Counters,
    ) -> std::result::Result<Option<Candidate>, Diagnostic> {
        choose_repair_candidate(
            draft.route_candidates(id, self.candidate, None)?,
            id,
            variant,
            counters,
        )
    }
}

fn choose_repair_candidate(
    report: RouteReport,
    id: &str,
    variant: usize,
    counters: &mut Counters,
) -> std::result::Result<Option<Candidate>, Diagnostic> {
    record_route_report(counters, &report);
    if variant >= report.candidates.len() && stopped_route(&report) {
        return Err(Diagnostic {
            code: if report.termination == Termination::ResourceLimit {
                "constructive.repair-resource-limit"
            } else if report.termination == Termination::Deadline {
                "constructive.repair-deadline"
            } else {
                "constructive.repair-work-limit"
            }
            .into(),
            severity: "error",
            stage: "hierarchical-search",
            message: format!("route search stopped at {:?}", report.termination),
            entity: id.into(),
        });
    }
    Ok(report.candidates.into_iter().nth(variant))
}

fn repair_stop(id: &str, stop: &AllowanceStop) -> Diagnostic {
    Diagnostic {
        code: if stop.reason == "deadline" {
            "constructive.repair-deadline"
        } else {
            "constructive.repair-work-limit"
        }
        .into(),
        severity: "error",
        stage: "hierarchical-search",
        message: format!(
            "repair stopped at {:?}: used {}, limit {}",
            stop.category, stop.used, stop.limit
        ),
        entity: id.into(),
    }
}
