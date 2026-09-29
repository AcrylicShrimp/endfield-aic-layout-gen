//! Retained present/history repair search for one immutable macro task.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Point, logistics::TransportKind};
use aic_pilot_native::constructive::{
    Session,
    routing_candidates::{
        AllowanceStop, Candidate, Guidance, PreparedRepair, RepairCandidate, RepairDraftState,
        RepairPreparationSizes, Request, RequestAllowance, RequestFailure, RouteCandidatePager,
        RoutePageStatus, Termination, WorkKind,
    },
};
use anyhow::{Result, anyhow};

use super::{
    Counters, Factory, NetworkWitnessKey, inference_reservation_bytes,
    negotiated::{
        diagnostic_is_stop, historical_price, next_untried, publish_unique_repair, update_history,
    },
};

enum GuideOutcome {
    Ready(Option<Guidance>, usize),
    Unknown(AllowanceStop),
}

pub(super) fn route_limit_stop(
    termination: Termination,
    pager: &RouteCandidatePager,
    factory: &Factory<'_>,
    _allowance: &RequestAllowance,
) -> AllowanceStop {
    let (reason, category, limit, used) = match termination {
        Termination::SearchLimit => (
            "route-search-limit",
            None,
            factory.candidate.max_searches,
            pager.work_counts().0,
        ),
        Termination::WorkBudget => (
            "route-work-budget",
            Some(WorkKind::Expansions),
            factory.candidate.max_expanded,
            pager.work_counts().1,
        ),
        Termination::Deadline => (
            "route-deadline",
            None,
            factory.candidate.request_allowance.deadline_ms as usize,
            factory.candidate.request_allowance.deadline_ms as usize,
        ),
        Termination::WorkLimit => ("route-work-limit", None, 0, 0),
        Termination::ResourceLimit => ("route-resource-limit", None, 0, 0),
        _ => ("route-unknown", None, 0, 0),
    };
    AllowanceStop {
        reason,
        category,
        limit,
        used,
        deadline_overshoot_ms: 0,
        attempted_bytes: None,
        aggregate_peak_bytes: None,
    }
}

pub(super) enum RepairPageStatus {
    Pending,
    Complete,
    Unknown(AllowanceStop),
}

pub(super) struct RepairPage {
    pub(super) candidate_ids: Vec<usize>,
    pub(super) status: RepairPageStatus,
    pub(super) expanded: usize,
    pub(super) scans: usize,
}

struct Attempt {
    removed: Vec<String>,
    variant: usize,
    order: Vec<String>,
    depth: usize,
    draft: Option<RepairDraftState>,
    route_pager: Option<RouteCandidatePager>,
}

impl Attempt {
    fn release(mut self, allowance: &mut RequestAllowance) {
        if let Some(pager) = self.route_pager.take() {
            pager.release(allowance);
        }
        if let Some(draft) = self.draft.take() {
            draft.release(allowance);
        }
    }
}

fn start_owned_draft(
    scope: &PreparedRepair,
    removed: &[String],
    allowance: &mut RequestAllowance,
) -> std::result::Result<RepairDraftState, aic_pilot_native::constructive::Diagnostic> {
    Ok(scope.start_draft(removed, allowance)?.suspend())
}

pub(super) struct RepairPager {
    scope: PreparedRepair,
    selected: Vec<String>,
    present: Vec<(String, u64)>,
    conflict_cells: BTreeMap<String, Vec<(TransportKind, Point)>>,
    order_bytes: usize,
    history: BTreeMap<(TransportKind, Point), u32>,
    history_bytes: usize,
    tried: BTreeMap<Vec<String>, usize>,
    tried_bytes: usize,
    semantic: BTreeSet<(Vec<String>, NetworkWitnessKey)>,
    semantic_bytes: usize,
    results: Vec<RepairCandidate>,
    output_bytes: usize,
    size: usize,
    attempt: Option<Attempt>,
    done: bool,
}

impl RepairPager {
    /// Scope authorization and conflict ordering happen before any direct
    /// route work. None means the authorized removal set is empty.
    pub(super) fn new(
        source: Arc<Session>,
        selected: Vec<String>,
        factory: &Factory<'_>,
        allowance: &mut RequestAllowance,
    ) -> Result<Option<Self>> {
        let scope = match PreparedRepair::new(
            source,
            &selected,
            factory.repair_max_removed,
            factory.candidate,
            allowance,
        ) {
            Ok(scope) => scope,
            Err(failure) if failure.code == "constructive.repair-empty-scope" => return Ok(None),
            Err(failure) => return Err(failure.into()),
        };
        let (present, conflict_cells, order_bytes) = match scope.conflict_order(allowance) {
            Ok(value) => value,
            Err(failure) => {
                scope.release(allowance);
                return Err(failure.into());
            }
        };
        Ok(Some(Self::from_prepared(
            scope,
            selected,
            present,
            conflict_cells,
            order_bytes,
        )))
    }

    /// The caller holds the measured simultaneous scope/hash/conflict peak.
    /// On success only the completed scope and conflict order remain reserved.
    pub(super) fn new_preheld(
        source: Arc<Session>,
        selected: Vec<String>,
        factory: &Factory<'_>,
        allowance: &mut RequestAllowance,
        sizes: &RepairPreparationSizes,
    ) -> Result<Option<Self>> {
        let scope = match PreparedRepair::new_preheld(
            source,
            &selected,
            factory.repair_max_removed,
            factory.candidate,
            allowance,
            sizes,
        ) {
            Ok(scope) => scope,
            Err(failure) => {
                allowance.release_peak(sizes.peak_bytes());
                if failure.code == "constructive.repair-empty-scope" {
                    return Ok(None);
                }
                return Err(failure.into());
            }
        };
        let (present, conflict_cells, order_bytes) = match scope.conflict_order_preheld(allowance) {
            Ok(value) => value,
            Err(failure) => {
                drop(scope);
                allowance.release_peak(sizes.peak_bytes());
                return Err(failure.into());
            }
        };
        debug_assert_eq!(order_bytes, sizes.conflict_retained_bytes());
        allowance.release_peak(sizes.peak_bytes() - sizes.retained_bytes());
        Ok(Some(Self::from_prepared(
            scope,
            selected,
            present,
            conflict_cells,
            order_bytes,
        )))
    }

    fn from_prepared(
        scope: PreparedRepair,
        selected: Vec<String>,
        present: Vec<(String, u64)>,
        conflict_cells: BTreeMap<String, Vec<(TransportKind, Point)>>,
        order_bytes: usize,
    ) -> Self {
        Self {
            scope,
            selected,
            present,
            conflict_cells,
            order_bytes,
            history: BTreeMap::new(),
            history_bytes: 0,
            tried: BTreeMap::new(),
            tried_bytes: 0,
            semantic: BTreeSet::new(),
            semantic_bytes: 0,
            results: Vec::new(),
            output_bytes: 0,
            size: 1,
            attempt: None,
            done: false,
        }
    }

    pub(super) fn request(
        &self,
    ) -> &aic_pilot_native::constructive::routing_candidates::RepairRequest {
        self.scope.request()
    }

    pub(super) fn candidates(&self) -> &[RepairCandidate] {
        &self.results
    }

    fn start_attempt(
        &mut self,
        factory: &Factory<'_>,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
    ) -> Result<Option<AllowanceStop>> {
        loop {
            if self.size > self.scope.request().max_removed {
                self.done = true;
                return Ok(None);
            }
            let order = match historical_price(
                self.scope.source(),
                &self.present,
                &self.history,
                allowance,
            ) {
                Ok(order) => order,
                Err(stop) => return Ok(Some(stop)),
            };
            let (removed, variant) = match next_untried(
                &order,
                self.size,
                factory.candidate.max_candidates,
                &self.tried,
                allowance,
            ) {
                Ok(Some(value)) => value,
                Ok(None) => {
                    self.size += 1;
                    continue;
                }
                Err(stop) => return Ok(Some(stop)),
            };
            if !self.tried.contains_key(&removed) {
                let bytes = removed.iter().fold(128usize, |total, id| {
                    total.saturating_add(id.len().saturating_add(128))
                });
                if let Err(stop) = allowance.reserve_peak(bytes) {
                    return Ok(Some(stop));
                }
                self.tried_bytes += bytes;
            }
            self.tried.insert(removed.clone(), variant + 1);
            counters.repair_attempts += 1;
            let draft = match start_owned_draft(&self.scope, &removed, allowance) {
                Ok(draft) => draft,
                Err(failure) => {
                    if diagnostic_is_stop(&failure) {
                        let stop = allowance.last_stop().ok_or_else(|| {
                            anyhow!("repair draft stopped without a typed allowance cause")
                        })?;
                        return Ok(Some(stop));
                    }
                    return Err(failure.into());
                }
            };
            let mut member_order = self.selected.clone();
            member_order.extend(removed.iter().cloned());
            self.attempt = Some(Attempt {
                removed,
                variant,
                order: member_order,
                depth: 0,
                draft: Some(draft),
                route_pager: None,
            });
            return Ok(None);
        }
    }

    fn member_identity(&self, attempt: &Attempt) -> Vec<u8> {
        let mut bytes = self.scope.request().state_sha256.as_bytes().to_vec();
        for id in &attempt.removed {
            bytes.extend_from_slice(id.as_bytes());
            bytes.push(0);
        }
        bytes.extend_from_slice(&(attempt.variant as u64).to_le_bytes());
        bytes.extend_from_slice(&(attempt.depth as u64).to_le_bytes());
        bytes.extend_from_slice(
            &attempt
                .draft
                .as_ref()
                .expect("live draft")
                .session()
                .revision()
                .to_le_bytes(),
        );
        bytes
    }

    fn guidance_for_member(
        &self,
        factory: &Factory<'_>,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
    ) -> Result<GuideOutcome> {
        let attempt = self.attempt.as_ref().expect("active repair attempt");
        let session = attempt.draft.as_ref().expect("live draft").session();
        let id = &attempt.order[attempt.depth];
        let request = Request {
            state_revision: session.revision(),
            connection: id.clone(),
        };
        let input = match session.route_guidance_input_with_allowance(
            &request,
            factory.candidate,
            &factory.domain,
            allowance,
        ) {
            Ok(input) => input,
            Err(RequestFailure::Invalid(failure))
                if failure.code.starts_with("constructive.route-guidance") =>
            {
                counters.repair_guidance_fallbacks += 1;
                return Ok(GuideOutcome::Ready(None, 0));
            }
            Err(RequestFailure::Stopped(stop)) => return Ok(GuideOutcome::Unknown(stop)),
            Err(failure) => return Err(anyhow!("repair guidance input: {failure:?}")),
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
        let inference_bytes = inference_reservation_bytes(&input, factory.guidance_heads);
        let charged = allowance
            .charge(WorkKind::InferenceElements, elements)
            .and_then(|_| allowance.reserve_peak(inference_bytes));
        if let Err(stop) = charged {
            allowance.release_peak(input.reserved_bytes);
            return Ok(GuideOutcome::Unknown(stop));
        }
        let predicted =
            factory
                .guidance
                .predict_native(&[&input], factory.guidance_version, factory.device);
        allowance.release_peak(input.reserved_bytes);
        let mut guide = match predicted {
            Ok(mut guides) => {
                counters.guidance_batches += 1;
                counters.guidance_inputs += 1;
                guides.pop().expect("single input has one guide")
            }
            Err(_) => {
                allowance.release_peak(inference_bytes);
                counters.repair_guidance_fallbacks += 1;
                return Ok(GuideOutcome::Ready(None, 0));
            }
        };
        let (transport, present, present_bytes) =
            match session.repair_live_cell_prices(id, allowance) {
                Ok(prices) => prices,
                Err(failure) => {
                    allowance.release_peak(inference_bytes);
                    if diagnostic_is_stop(&failure) {
                        let stop = allowance.last_stop().ok_or_else(|| {
                            anyhow!("repair price lookup stopped without a typed allowance cause")
                        })?;
                        return Ok(GuideOutcome::Unknown(stop));
                    }
                    return Err(failure.into());
                }
            };
        let mut priced_stop = None;
        'price: for head in &mut guide.heads {
            for directed in &mut head.directed_costs {
                if let Err(stop) = allowance.charge(WorkKind::RoutingScans, 1) {
                    priced_stop = Some(stop);
                    break 'price;
                }
                let present_price = present.get(&directed.point).copied().unwrap_or(0);
                let history_price = self
                    .history
                    .get(&(transport, directed.point))
                    .copied()
                    .unwrap_or(0);
                counters.repair_priced_edges += usize::from(present_price > 0 || history_price > 0);
                directed.cost = directed
                    .cost
                    .saturating_add(present_price)
                    .saturating_add(history_price)
                    .min(1_000_000);
            }
        }
        allowance.release_peak(present_bytes);
        if let Some(stop) = priced_stop {
            allowance.release_peak(inference_bytes);
            return Ok(GuideOutcome::Unknown(stop));
        }
        Ok(GuideOutcome::Ready(Some(guide), inference_bytes))
    }

    fn prepare_member(
        &mut self,
        factory: &Factory<'_>,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
    ) -> Result<Option<AllowanceStop>> {
        let (guide, guide_reservation) =
            match self.guidance_for_member(factory, counters, allowance)? {
                GuideOutcome::Ready(guide, reservation) => (guide, reservation),
                GuideOutcome::Unknown(stop) => return Ok(Some(stop)),
            };
        let attempt = self.attempt.as_ref().expect("active attempt");
        let session = attempt.draft.as_ref().expect("live draft").session();
        let request = Request {
            state_revision: session.revision(),
            connection: attempt.order[attempt.depth].clone(),
        };
        let identity = self.member_identity(attempt);
        let prepared = RouteCandidatePager::new_unpublished(
            session,
            request,
            factory.candidate.clone(),
            guide,
            identity,
            allowance,
        );
        allowance.release_peak(guide_reservation);
        match prepared {
            Ok(pager) => self.attempt.as_mut().expect("active attempt").route_pager = Some(pager),
            Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                return self.fail_attempt(allowance);
            }
            Err(RequestFailure::Stopped(stop)) => return Ok(Some(stop)),
            Err(failure) => return Err(anyhow!("repair route preparation: {failure:?}")),
        }
        Ok(None)
    }

    fn fail_attempt(&mut self, allowance: &mut RequestAllowance) -> Result<Option<AllowanceStop>> {
        let attempt = self.attempt.take().expect("active repair attempt");
        let removed = attempt.removed.clone();
        attempt.release(allowance);
        match update_history(
            &removed,
            &self.conflict_cells,
            &mut self.history,
            allowance,
            &mut self.history_bytes,
        ) {
            Ok(()) => Ok(None),
            Err(stop) => Ok(Some(stop)),
        }
    }

    fn finish_member(&mut self, allowance: &mut RequestAllowance) -> Result<Option<AllowanceStop>> {
        let attempt = self.attempt.as_mut().expect("active attempt");
        let pager = attempt.route_pager.take().expect("member pager exists");
        let chosen = if attempt.depth == 0 {
            attempt.variant
        } else {
            0
        };
        if chosen >= pager.candidates().len() {
            pager.release(allowance);
            return self.fail_attempt(allowance);
        }
        let candidate_bytes = pager.candidates()[chosen]
            .cells
            .len()
            .saturating_mul(128)
            .saturating_add(
                pager.candidates()[chosen]
                    .commands
                    .len()
                    .saturating_mul(256),
            )
            .saturating_add(2048);
        if let Err(stop) = allowance.reserve_peak(candidate_bytes) {
            pager.release(allowance);
            return Ok(Some(stop));
        }
        let candidate: Candidate = pager.candidates()[chosen].clone();
        pager.release(allowance);
        let state = attempt.draft.take().expect("live draft");
        let mut draft = self
            .scope
            .resume_draft(state, allowance)
            .map_err(anyhow::Error::new)?;
        let applied = draft.apply_member(candidate);
        match applied {
            Ok(()) => {
                let state = draft.suspend();
                allowance.release_peak(candidate_bytes);
                attempt.draft = Some(state);
                attempt.depth += 1;
                Ok(None)
            }
            Err(failure) => {
                drop(draft);
                allowance.release_peak(candidate_bytes);
                if diagnostic_is_stop(&failure) {
                    let stop = allowance.last_stop().ok_or_else(|| {
                        anyhow!("repair member stopped without a typed allowance cause")
                    })?;
                    return Ok(Some(stop));
                }
                self.fail_attempt(allowance)
            }
        }
    }

    fn finish_attempt(
        &mut self,
        allowance: &mut RequestAllowance,
    ) -> Result<Option<AllowanceStop>> {
        let mut attempt = self.attempt.take().expect("complete attempt");
        let state = attempt.draft.take().expect("live draft");
        let draft = self
            .scope
            .resume_draft(state, allowance)
            .map_err(anyhow::Error::new)?;
        let result = draft.finish();
        let candidate = match result {
            Ok((_, candidate, _)) => candidate,
            Err(failure) => {
                if diagnostic_is_stop(&failure) {
                    let stop = allowance.last_stop().ok_or_else(|| {
                        anyhow!("repair completion stopped without a typed allowance cause")
                    })?;
                    return Ok(Some(stop));
                }
                let removed = attempt.removed;
                return match update_history(
                    &removed,
                    &self.conflict_cells,
                    &mut self.history,
                    allowance,
                    &mut self.history_bytes,
                ) {
                    Ok(()) => Ok(None),
                    Err(stop) => Ok(Some(stop)),
                };
            }
        };
        match publish_unique_repair(
            self.scope.request(),
            &candidate,
            &mut self.semantic,
            allowance,
        ) {
            Ok(Some((retained, key_bytes))) => {
                self.output_bytes += retained;
                self.semantic_bytes += key_bytes;
                self.results.push(candidate);
                Ok(None)
            }
            Ok(None) => Ok(None),
            Err(failure) => {
                if diagnostic_is_stop(&failure) {
                    let stop = allowance.last_stop().ok_or_else(|| {
                        anyhow!("repair publication stopped without a typed allowance cause")
                    })?;
                    Ok(Some(stop))
                } else {
                    Err(failure.into())
                }
            }
        }
    }

    pub(super) fn next_page(
        &mut self,
        factory: &Factory<'_>,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
        max_new_candidates: usize,
        max_new_expansions: usize,
        max_new_scans: usize,
        page_deadline: Instant,
    ) -> Result<RepairPage> {
        let mut ids = Vec::new();
        let mut expanded = 0usize;
        let mut scans = 0usize;
        let status = loop {
            if self.done {
                break RepairPageStatus::Complete;
            }
            if ids.len() >= max_new_candidates
                || expanded >= max_new_expansions
                || scans >= max_new_scans
                || Instant::now() >= page_deadline
            {
                break RepairPageStatus::Pending;
            }
            if self.attempt.is_none() {
                if let Some(stop) = self.start_attempt(factory, counters, allowance)? {
                    break RepairPageStatus::Unknown(stop);
                }
                if self.done {
                    break RepairPageStatus::Complete;
                }
            }
            let attempt = self.attempt.as_ref().expect("active attempt");
            if attempt.depth == attempt.order.len() {
                let before = self.results.len();
                if let Some(stop) = self.finish_attempt(allowance)? {
                    break RepairPageStatus::Unknown(stop);
                }
                if self.results.len() > before {
                    ids.push(self.results.len() - 1);
                }
                continue;
            }
            if attempt.route_pager.is_none() {
                if let Some(stop) = self.prepare_member(factory, counters, allowance)? {
                    break RepairPageStatus::Unknown(stop);
                }
                if self.attempt.is_none() {
                    continue;
                }
            }
            let identity = self.member_identity(self.attempt.as_ref().expect("active attempt"));
            let attempt = self.attempt.as_mut().expect("active attempt");
            let pager = attempt.route_pager.as_mut().expect("member pager");
            let page = pager
                .next_page(
                    attempt.draft.as_ref().expect("live draft").session(),
                    &identity,
                    allowance,
                    factory.candidate.max_candidates,
                    max_new_expansions.saturating_sub(expanded),
                    max_new_scans.saturating_sub(scans),
                    page_deadline,
                )
                .map_err(anyhow::Error::new)?;
            expanded += page.expanded;
            scans += page.scans;
            match page.status {
                RoutePageStatus::Pending => break RepairPageStatus::Pending,
                RoutePageStatus::Unknown(stop) => break RepairPageStatus::Unknown(stop),
                RoutePageStatus::Complete(termination) => {
                    let chosen = if attempt.depth == 0 {
                        attempt.variant
                    } else {
                        0
                    };
                    if chosen >= pager.candidates().len()
                        && matches!(
                            termination,
                            Termination::WorkLimit
                                | Termination::Deadline
                                | Termination::ResourceLimit
                                | Termination::SearchLimit
                                | Termination::WorkBudget
                        )
                    {
                        break RepairPageStatus::Unknown(route_limit_stop(
                            termination,
                            pager,
                            factory,
                            allowance,
                        ));
                    }
                    if let Some(stop) = self.finish_member(allowance)? {
                        break RepairPageStatus::Unknown(stop);
                    }
                }
            }
        };
        if matches!(status, RepairPageStatus::Unknown(_)) {
            counters.repair_unknown += 1;
            if let Some(attempt) = self.attempt.take() {
                attempt.release(allowance);
            }
            self.done = true;
        }
        Ok(RepairPage {
            candidate_ids: ids,
            status,
            expanded,
            scans,
        })
    }

    pub(super) fn release(mut self, allowance: &mut RequestAllowance) {
        if let Some(attempt) = self.attempt.take() {
            attempt.release(allowance);
        }
        allowance.release_peak(
            self.order_bytes
                + self.history_bytes
                + self.tried_bytes
                + self.semantic_bytes
                + self.output_bytes,
        );
        self.scope.release(allowance);
    }
}
