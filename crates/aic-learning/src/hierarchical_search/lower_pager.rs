//! Per-node retained lower-candidate tasks and their shared direct/repair budgets.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use aic_layout::logistics::TransportKind;
use aic_pilot_native::constructive::{
    Session,
    routing_candidates::{
        AllowanceStop, CUMULATIVE_WORK_KINDS, Candidate, Guidance, GuidanceInput, NetworkCandidate,
        PreparedRepair, Provenance, RepairCandidate, RepairPreparationSizes, Request,
        RequestAllowance, RequestFailure, RouteCandidatePager, RouteCandidatePreparation,
        RouteGuidancePreparation, RoutePageStatus, RouteReadiness, SearchMemoryBudget, WorkKind,
    },
};
use anyhow::{Result, anyhow};

use super::{
    Counters, Factory, MacroAction, capture_tree_memory_stop, inference_reservation_bytes,
    network_pager::{NetworkPageStatus, NetworkPager},
    record_request_stop,
    repair_pager::{RepairPageStatus, RepairPager},
    state_sha256,
};

enum TaskKind {
    Route(Request),
    Network(Vec<String>),
}

enum DirectPager {
    Route(RouteCandidatePager),
    Network(NetworkPager),
}

struct RouteOpen {
    guidance: Option<Guidance>,
    identity: Vec<u8>,
}

fn guidance_output_bytes(guidance: &Guidance) -> usize {
    std::mem::size_of::<Guidance>()
        .saturating_add(guidance.schema.capacity())
        .saturating_add(guidance.connection.capacity())
        .saturating_add(guidance.model_version.capacity())
        .saturating_add(
            guidance
                .heads
                .capacity()
                .saturating_mul(std::mem::size_of::<
                    aic_pilot_native::constructive::routing_candidates::GuidanceHead,
                >()),
        )
        .saturating_add(guidance.heads.iter().fold(0usize, |total, head| {
            total.saturating_add(head.id.capacity()).saturating_add(
                head.directed_costs
                    .capacity()
                    .saturating_mul(std::mem::size_of::<
                        aic_pilot_native::constructive::routing_candidates::DirectedCost,
                    >()),
            )
        }))
        .saturating_add(256)
}

#[derive(Clone, Copy)]
enum PendingNative {
    Route { id: usize, update: bool },
    Network { id: usize },
    Repair { id: usize },
}

/// Only provenance changes when an existing physical route is rediscovered.
/// The original edge witness stays in place; this owns its small pending
/// metadata copy until that edge is updated.
pub(super) struct RouteUpdate {
    pub(super) connection: String,
    pub(super) id: usize,
    pub(super) additional_provenance: Vec<Provenance>,
    pub(super) _memory: TreeReservation,
}

/// A live tree allocation. Request allowances charge native frontier and
/// pager memory to the same budget; this guard covers owned node/action copies.
pub(super) struct TreeReservation {
    budget: SearchMemoryBudget,
    bytes: usize,
}

impl TreeReservation {
    pub(super) fn new(budget: &SearchMemoryBudget, bytes: usize) -> Result<Self, AllowanceStop> {
        budget.reserve(bytes)?;
        Ok(Self {
            budget: budget.clone(),
            bytes,
        })
    }

    pub(super) fn resize(&mut self, bytes: usize) -> Result<(), AllowanceStop> {
        if bytes > self.bytes {
            self.budget.reserve(bytes - self.bytes)?;
        } else {
            self.budget.release(self.bytes - bytes);
        }
        self.bytes = bytes;
        Ok(())
    }

    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for TreeReservation {
    fn drop(&mut self) {
        self.budget.release(self.bytes);
    }
}

fn route_bytes(candidate: &Candidate) -> usize {
    4096usize
        .saturating_add(candidate.cells.len().saturating_mul(128))
        .saturating_add(candidate.commands.len().saturating_mul(256))
        .saturating_add(candidate.crossings.len().saturating_mul(512))
        .saturating_add(
            candidate
                .physical_cost
                .component_uses
                .len()
                .saturating_mul(256),
        )
        .saturating_add(provenance_bytes(&candidate.additional_provenance))
}

pub(super) fn provenance_bytes(provenances: &[Provenance]) -> usize {
    provenances.iter().fold(0usize, |bytes, provenance| {
        bytes.saturating_add(256).saturating_add(match provenance {
            Provenance::Anchor => 0,
            Provenance::Guided {
                model_version,
                head,
            } => model_version.len().saturating_add(head.len()),
            Provenance::Diversified {
                model_version,
                head,
            } => model_version
                .as_ref()
                .map_or(0, String::len)
                .saturating_add(head.as_ref().map_or(0, String::len)),
        })
    })
}

fn network_bytes(candidate: &NetworkCandidate) -> usize {
    candidate.routes.iter().fold(4096, |bytes, route| {
        bytes.saturating_add(route_bytes(route))
    })
}

fn repair_bytes(candidate: &RepairCandidate) -> usize {
    candidate.replacements.iter().fold(8192, |bytes, route| {
        bytes.saturating_add(route_bytes(route))
    })
}

pub(super) fn action_bytes(action: &MacroAction) -> usize {
    match action {
        MacroAction::Command(_) => 512,
        MacroAction::Route(candidate) => route_bytes(candidate),
        MacroAction::Network(candidate) => network_bytes(candidate),
        MacroAction::Repair { candidate, .. } => repair_bytes(candidate),
    }
}

struct Task {
    kind: TaskKind,
    allowance: RequestAllowance,
    memory: SearchMemoryBudget,
    published: Vec<TreeReservation>,
    route_guard_index: BTreeMap<usize, usize>,
    updated_routes: Vec<RouteUpdate>,
    pending_native: VecDeque<PendingNative>,
    pending_retry_live: Option<usize>,
    guidance_preparation: Option<RouteGuidancePreparation>,
    prepared_guidance_input: Option<GuidanceInput>,
    route_open: Option<RouteOpen>,
    route_preparation: Option<RouteCandidatePreparation>,
    route_output_bytes: usize,
    guidance_pending: bool,
    direct: Option<DirectPager>,
    repair: Option<RepairPager>,
    scope_prepared: bool,
    scope_sizes: Option<RepairPreparationSizes>,
    scope_retry_live: Option<usize>,
    direct_done: bool,
    repair_done: bool,
    first_direct: bool,
    first_repair: bool,
    first_quotas: [usize; CUMULATIVE_WORK_KINDS.len()],
    first_duration: Duration,
    pages: usize,
    published_actions: usize,
    repair_published: bool,
}

impl Task {
    fn guidance_input(
        &mut self,
        source: &Session,
        candidate: &aic_pilot_native::constructive::routing_candidates::Config,
        learning: &aic_pilot_native::constructive::learning::Config,
    ) -> Result<aic_pilot_native::constructive::routing_candidates::GuidanceInput, RequestFailure>
    {
        let TaskKind::Route(request) = &self.kind else {
            unreachable!("only singleton tasks prepare route guidance")
        };
        let preparation = match self.guidance_preparation.take() {
            Some(preparation) => preparation,
            None => RouteGuidancePreparation::new(
                source,
                request,
                candidate,
                learning,
                &mut self.allowance,
            )?,
        };
        match preparation.into_input(source, candidate, learning, &mut self.allowance) {
            Ok(input) => Ok(input),
            Err((preparation, RequestFailure::Stopped(stop))) if stop.reason == "page-quantum" => {
                self.guidance_preparation = Some(preparation);
                Err(RequestFailure::Stopped(stop))
            }
            Err((preparation, failure)) => {
                preparation.release(&mut self.allowance);
                Err(failure)
            }
        }
    }

    fn retain_prediction(
        &mut self,
        input: GuidanceInput,
        guidance: Option<Guidance>,
        inference_bytes: usize,
    ) {
        let output_bytes = guidance.as_ref().map_or(0, guidance_output_bytes);
        let scratch_bytes = inference_bytes
            .checked_sub(output_bytes)
            .expect("inference reservation includes its native guidance output");
        let input_bytes = input.reserved_bytes;
        drop(input);
        self.allowance
            .release_peak(scratch_bytes.saturating_add(input_bytes));
        self.route_output_bytes = output_bytes;
        self.route_open = Some(RouteOpen {
            guidance,
            identity: Vec::new(),
        });
    }

    fn advance_route_preparation(
        &mut self,
        source: &Session,
        candidate: &aic_pilot_native::constructive::routing_candidates::Config,
        source_identity: &[u8],
        task_index: usize,
    ) -> Result<RouteCandidatePager, RequestFailure> {
        let TaskKind::Route(request) = &self.kind else {
            unreachable!("only singleton tasks prepare route candidates")
        };
        let mut preparation = if let Some(preparation) = self.route_preparation.take() {
            preparation
        } else {
            let open = self
                .route_open
                .as_mut()
                .expect("route guidance output is owned before native preparation");
            if open.identity.is_empty() {
                let identity_bytes = source_identity.len().saturating_add(8);
                self.allowance.reserve_peak(identity_bytes)?;
                self.route_output_bytes = self.route_output_bytes.saturating_add(identity_bytes);
                open.identity = Vec::with_capacity(identity_bytes);
                open.identity.extend_from_slice(source_identity);
                open.identity
                    .extend_from_slice(&(task_index as u64).to_le_bytes());
            }
            let preparation = RouteCandidatePreparation::new_retained_borrowed(
                source,
                request,
                candidate,
                &mut open.guidance,
                &mut open.identity,
                &mut self.allowance,
            )?;
            self.route_open = None;
            preparation
        };
        let advanced = (|| {
            preparation.advance_pairs(source, &mut self.allowance)?;
            preparation.advance_guidance(&mut self.allowance)?;
            preparation.advance_carrier_inventory(source, &mut self.allowance)?;
            preparation.advance_blocked(source, &mut self.allowance)?;
            preparation.advance_crossings(source, &mut self.allowance)?;
            Ok(())
        })();
        if let Err(failure) = advanced {
            if matches!(&failure, RequestFailure::Stopped(stop) if stop.reason == "page-quantum") {
                self.route_preparation = Some(preparation);
            } else {
                preparation.release(&mut self.allowance);
            }
            return Err(failure);
        }
        match preparation.finish_retained(source, &mut self.allowance) {
            Ok(pager) => Ok(pager),
            Err((preparation, failure)) => {
                if matches!(&failure, RequestFailure::Stopped(stop) if stop.reason == "page-quantum")
                {
                    self.route_preparation = Some(preparation);
                } else {
                    preparation.release(&mut self.allowance);
                }
                Err(failure)
            }
        }
    }

    fn selected(&self) -> &[String] {
        match &self.kind {
            TaskKind::Route(request) => std::slice::from_ref(&request.connection),
            TaskKind::Network(connections) => connections,
        }
    }

    fn request(&self, revision: u64) -> Request {
        Request {
            state_revision: revision,
            connection: self.selected()[0].clone(),
        }
    }

    fn complete(&self) -> bool {
        self.direct_done && self.repair_done && self.pending_native.is_empty()
    }

    fn first_quota(&self, kind: WorkKind) -> usize {
        let index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&candidate| candidate == kind)
            .expect("first-turn work category must be cumulative");
        self.first_quotas[index]
    }

    fn prepare_scope(
        &mut self,
        source: &Arc<Session>,
        factory: &Factory<'_>,
        counters: &mut Counters,
        pinned_external_bytes: usize,
    ) -> Result<()> {
        if self.scope_prepared {
            return Ok(());
        }
        if self
            .scope_retry_live
            .is_some_and(|at_stop| self.memory.live_bytes() >= at_stop)
        {
            return Ok(());
        }
        self.allowance.resume();
        if factory.repair_max_removed > 0 {
            let sizes = if let Some(sizes) = self.scope_sizes.take() {
                sizes
            } else {
                let selected = match &self.kind {
                    TaskKind::Route(request) => std::slice::from_ref(&request.connection),
                    TaskKind::Network(connections) => connections.as_slice(),
                };
                match PreparedRepair::measure_preparation(
                    source,
                    selected,
                    factory.candidate,
                    &mut self.allowance,
                ) {
                    Ok(sizes) => sizes,
                    Err(error) => {
                        if let Some(stop) = self.allowance.last_stop() {
                            self.stop(source.revision(), counters, stop);
                            return Ok(());
                        }
                        if error.code == "constructive.repair-resource-size" {
                            self.stop(
                                source.revision(),
                                counters,
                                AllowanceStop {
                                    reason: "repair-resource-size",
                                    category: None,
                                    limit: usize::MAX,
                                    used: 0,
                                    deadline_overshoot_ms: 0,
                                    attempted_bytes: None,
                                    aggregate_peak_bytes: None,
                                },
                            );
                            return Ok(());
                        }
                        return Err(error.into());
                    }
                }
            };
            if let Err(stop) = self.allowance.reserve_peak(sizes.peak_bytes()) {
                if stop.reason == "aggregate-memory-limit"
                    && self
                        .allowance
                        .aggregate_reservation_can_fit_if_others_release(
                            sizes.peak_bytes(),
                            pinned_external_bytes,
                        )
                {
                    self.scope_sizes = Some(sizes);
                    self.scope_retry_live = Some(self.memory.live_bytes());
                    if let Err(deadline_stop) = self.allowance.pause() {
                        self.stop(source.revision(), counters, deadline_stop);
                    }
                    return Ok(());
                }
                self.stop(source.revision(), counters, stop);
                return Ok(());
            }
            self.scope_retry_live = None;
            self.allowance.clear_last_stop();
            let selected = self.selected().to_vec();
            match RepairPager::new_preheld(
                Arc::clone(source),
                selected,
                factory,
                &mut self.allowance,
                &sizes,
            ) {
                Ok(Some(repair)) => {
                    self.repair = Some(repair);
                    self.repair_done = false;
                    let candidate_slots = factory
                        .candidate
                        .max_candidates
                        .saturating_sub(self.published_actions)
                        .min(self.allowance.remaining(WorkKind::Candidates));
                    let variant_slots = self.allowance.remaining(WorkKind::Variants);
                    let required_variants = if matches!(self.kind, TaskKind::Network(_)) {
                        2
                    } else {
                        1
                    };
                    if candidate_slots < 2 || variant_slots < required_variants {
                        let category = if candidate_slots < 2 {
                            WorkKind::Candidates
                        } else {
                            WorkKind::Variants
                        };
                        self.stop(
                            source.revision(),
                            counters,
                            AllowanceStop {
                                reason: "budget-too-small-for-fair-first-pages",
                                category: Some(category),
                                limit: if category == WorkKind::Candidates {
                                    candidate_slots
                                } else {
                                    variant_slots
                                },
                                used: 0,
                                deadline_overshoot_ms: 0,
                                attempted_bytes: None,
                                aggregate_peak_bytes: None,
                            },
                        );
                    }
                }
                Ok(None) => self.repair_done = true,
                Err(error) => {
                    if let Some(stop) = self.allowance.last_stop() {
                        self.stop(source.revision(), counters, stop);
                    } else {
                        return Err(error);
                    }
                }
            }
        } else {
            self.repair_done = true;
        }
        self.scope_prepared = true;
        if self.complete() {
            return Ok(());
        }
        self.first_quotas = CUMULATIVE_WORK_KINDS.map(|kind| self.allowance.remaining(kind) / 2);
        self.first_duration = self
            .allowance
            .deadline()
            .saturating_duration_since(Instant::now())
            / 2;
        if !self.complete() {
            if let Err(stop) = self.allowance.pause() {
                self.stop(source.revision(), counters, stop);
            }
        }
        Ok(())
    }

    fn stop(&mut self, revision: u64, counters: &mut Counters, stop: AllowanceStop) {
        record_request_stop(counters, &self.request(revision), &self.allowance, stop);
        self.scope_sizes = None;
        self.scope_retry_live = None;
        self.guidance_pending = false;
        self.direct_done = true;
        self.repair_done = true;
    }

    fn release(&mut self, source: &Session) {
        self.scope_sizes = None;
        self.scope_retry_live = None;
        if let Some(preparation) = self.guidance_preparation.take() {
            preparation.release(&mut self.allowance);
        }
        if let Some(input) = self.prepared_guidance_input.take() {
            let held = input.reserved_bytes;
            drop(input);
            self.allowance.release_peak(held);
        }
        self.route_open = None;
        if let Some(preparation) = self.route_preparation.take() {
            preparation.release(&mut self.allowance);
        }
        match self.direct.take() {
            Some(DirectPager::Route(pager)) => pager.release(&mut self.allowance),
            Some(DirectPager::Network(pager)) => pager.release(source, &mut self.allowance),
            None => {}
        }
        self.allowance
            .release_peak(std::mem::take(&mut self.route_output_bytes));
        if let Some(pager) = self.repair.take() {
            pager.release(&mut self.allowance);
        }
    }

    fn release_done_pagers(&mut self, source: &Session) {
        if !self.pending_native.is_empty() {
            return;
        }
        if self.direct_done {
            if let Some(preparation) = self.guidance_preparation.take() {
                preparation.release(&mut self.allowance);
            }
            if let Some(input) = self.prepared_guidance_input.take() {
                let held = input.reserved_bytes;
                drop(input);
                self.allowance.release_peak(held);
            }
            self.route_open = None;
            if let Some(preparation) = self.route_preparation.take() {
                preparation.release(&mut self.allowance);
            }
            match self.direct.take() {
                Some(DirectPager::Route(pager)) => pager.release(&mut self.allowance),
                Some(DirectPager::Network(pager)) => pager.release(source, &mut self.allowance),
                None => {}
            }
            self.allowance
                .release_peak(std::mem::take(&mut self.route_output_bytes));
        }
        if self.repair_done {
            if let Some(pager) = self.repair.take() {
                pager.release(&mut self.allowance);
            }
        }
    }

    /// Native pagers retain the original candidate until this second, tree
    /// owned copy is admitted. A failed reservation leaves the same ID first.
    fn flush_pending(&mut self) -> (Vec<MacroAction>, Option<AllowanceStop>) {
        let mut actions = Vec::new();
        while let Some(pending) = self.pending_native.front().copied() {
            let outcome = match pending {
                PendingNative::Route { id, update } => {
                    let Some(DirectPager::Route(pager)) = self.direct.as_ref() else {
                        unreachable!("pending route retains its native pager")
                    };
                    let candidate = &pager.candidates()[id];
                    if update {
                        let guard_index = self.route_guard_index[&id];
                        let metadata_bytes = provenance_bytes(&candidate.additional_provenance)
                            .saturating_add(candidate.connection.len())
                            .saturating_add(128);
                        TreeReservation::new(&self.memory, metadata_bytes).and_then(|reservation| {
                            self.published[guard_index].resize(route_bytes(candidate))?;
                            self.updated_routes.push(RouteUpdate {
                                connection: candidate.connection.clone(),
                                id,
                                additional_provenance: candidate.additional_provenance.clone(),
                                _memory: reservation,
                            });
                            Ok(None)
                        })
                    } else {
                        TreeReservation::new(&self.memory, route_bytes(candidate)).map(
                            |reservation| {
                                let action = MacroAction::Route(candidate.clone());
                                self.route_guard_index.insert(id, self.published.len());
                                self.published.push(reservation);
                                self.published_actions += 1;
                                Some(action)
                            },
                        )
                    }
                }
                PendingNative::Network { id } => {
                    let Some(DirectPager::Network(pager)) = self.direct.as_ref() else {
                        unreachable!("pending network retains its native pager")
                    };
                    let candidate = &pager.candidates()[id];
                    TreeReservation::new(&self.memory, network_bytes(candidate)).map(
                        |reservation| {
                            self.published.push(reservation);
                            self.published_actions += 1;
                            Some(MacroAction::Network(candidate.clone()))
                        },
                    )
                }
                PendingNative::Repair { id } => {
                    let pager = self
                        .repair
                        .as_ref()
                        .expect("pending repair retains its pager");
                    let candidate = &pager.candidates()[id];
                    TreeReservation::new(&self.memory, repair_bytes(candidate)).map(|reservation| {
                        self.published.push(reservation);
                        self.published_actions += 1;
                        self.repair_published = true;
                        Some(MacroAction::Repair {
                            request: pager.request().clone(),
                            candidate: candidate.clone(),
                        })
                    })
                }
            };
            match outcome {
                Ok(action) => {
                    self.pending_native.pop_front();
                    if let Some(action) = action {
                        actions.push(action);
                    }
                }
                Err(stop) => {
                    self.pending_retry_live = Some(self.memory.live_bytes());
                    return (actions, Some(stop));
                }
            }
        }
        self.pending_retry_live = None;
        (actions, None)
    }

    fn record_actions(actions: &[MacroAction], counters: &mut Counters) {
        counters.generated_macro_samples += actions.len();
        counters.route_samples += actions
            .iter()
            .filter(|action| matches!(action, MacroAction::Route(_)))
            .count();
        counters.network_samples += actions
            .iter()
            .filter(|action| matches!(action, MacroAction::Network(_)))
            .count();
        counters.shared_network_samples += actions
            .iter()
            .filter(|action| matches!(action, MacroAction::Network(candidate) if candidate.shared_cells > 0))
            .count();
    }

    fn page(
        &mut self,
        source: &Session,
        source_identity: &str,
        factory: &Factory<'_>,
        counters: &mut Counters,
    ) -> Result<Vec<MacroAction>> {
        if self.complete() {
            return Ok(Vec::new());
        }
        // Give repair its reserved first search turn even when the first
        // direct candidate is waiting for tree-copy memory. Its native page
        // does not advance the direct pager or discard that candidate ID.
        let repair_first_due = self.first_direct && !self.first_repair && !self.repair_done;
        if !self.pending_native.is_empty() && !repair_first_due {
            if self
                .pending_retry_live
                .is_some_and(|at_stop| self.memory.live_bytes() >= at_stop)
            {
                return Ok(Vec::new());
            }
            let (actions, stop) = self.flush_pending();
            if let Some(stop) = stop {
                record_request_stop(
                    counters,
                    &self.request(source.revision()),
                    &self.allowance,
                    stop,
                );
            }
            Self::record_actions(&actions, counters);
            self.release_done_pagers(source);
            return Ok(actions);
        }
        self.allowance.resume();
        let mut direct_stage = !self.direct_done
            && (!self.first_direct
                || self.repair_done
                || self.first_repair && self.pages.is_multiple_of(2));
        let held_repair_slot =
            usize::from(self.repair.is_some() && !self.repair_done && !self.repair_published);
        let direct_slots = factory
            .candidate
            .max_candidates
            .saturating_sub(self.published_actions.saturating_add(held_repair_slot))
            .min(
                self.allowance
                    .remaining(WorkKind::Candidates)
                    .saturating_sub(held_repair_slot),
            );
        if direct_stage && direct_slots == 0 && !self.repair_done {
            direct_stage = false;
        }
        if direct_stage
            && !self.repair_done
            && matches!(self.direct.as_ref(), Some(DirectPager::Network(pager)) if pager.blocked_by_held_variant(
                factory.candidate.max_candidates,
                self.allowance.remaining(WorkKind::Variants),
                held_repair_slot,
            ))
        {
            direct_stage = false;
        }
        if direct_stage && direct_slots == 0 {
            self.direct_done = true;
            self.release_done_pagers(source);
            return Ok(Vec::new());
        }
        let first = if direct_stage {
            !self.first_direct
        } else {
            !self.first_repair
        };
        let expansions = if first && self.repair.is_some() {
            self.first_quota(WorkKind::Expansions)
                .min(self.allowance.remaining(WorkKind::Expansions))
        } else {
            self.allowance.remaining(WorkKind::Expansions)
        };
        let scans = if first && self.repair.is_some() {
            self.first_quota(WorkKind::RoutingScans)
                .min(self.allowance.remaining(WorkKind::RoutingScans))
        } else {
            self.allowance.remaining(WorkKind::RoutingScans)
        };
        let now = Instant::now();
        let page_deadline = if first && self.repair.is_some() {
            (now + self.first_duration).min(self.allowance.deadline())
        } else {
            self.allowance.deadline()
        };
        if expansions == 0 || scans == 0 || page_deadline <= now {
            if first && self.repair.is_some() {
                // A zero temporary half is a yielded opportunity. The same
                // task can still use the shared hard remainder after repair
                // has received its first turn.
                if direct_stage {
                    self.first_direct = true;
                } else {
                    self.first_repair = true;
                }
                self.pages += 1;
                if let Err(stop) = self.allowance.pause() {
                    self.stop(source.revision(), counters, stop);
                    self.release_done_pagers(source);
                }
                return Ok(Vec::new());
            }
            let category = if expansions == 0 {
                Some(WorkKind::Expansions)
            } else if scans == 0 {
                Some(WorkKind::RoutingScans)
            } else {
                None
            };
            self.stop(
                source.revision(),
                counters,
                AllowanceStop {
                    reason: "lower-page-budget-exhausted",
                    category,
                    limit: category.map_or(0, |kind| {
                        self.allowance.remaining(kind).saturating_add(
                            self.allowance
                                .usage()
                                .categories
                                .get(&kind)
                                .copied()
                                .unwrap_or(0),
                        )
                    }),
                    used: category.map_or(0, |kind| {
                        self.allowance
                            .usage()
                            .categories
                            .get(&kind)
                            .copied()
                            .unwrap_or(0)
                    }),
                    deadline_overshoot_ms: 0,
                    attempted_bytes: None,
                    aggregate_peak_bytes: None,
                },
            );
            self.release_done_pagers(source);
            return Ok(Vec::new());
        }
        let usage_before = self.allowance.usage();
        let candidate_page = (if first {
            1
        } else {
            factory.candidate.max_candidates
        })
        .min(if direct_stage {
            direct_slots
        } else {
            factory
                .candidate
                .max_candidates
                .saturating_sub(self.published_actions)
                .min(self.allowance.remaining(WorkKind::Candidates))
        });
        let mut actions = Vec::new();
        if direct_stage {
            self.first_direct = true;
            if self.direct.is_none() {
                if let TaskKind::Network(connections) = &self.kind {
                    match NetworkPager::new(
                        source,
                        source_identity,
                        connections.clone(),
                        &mut self.allowance,
                    ) {
                        Ok(pager) => {
                            counters.network_tasks += 1;
                            self.direct = Some(DirectPager::Network(pager));
                        }
                        Err(error) => {
                            if let Some(stop) = self.allowance.last_stop() {
                                record_request_stop(
                                    counters,
                                    &self.request(source.revision()),
                                    &self.allowance,
                                    stop,
                                );
                                counters.network_unknown += 1;
                                self.direct_done = true;
                            } else {
                                return Err(error);
                            }
                        }
                    }
                }
            }
            match self.direct.as_mut() {
                Some(DirectPager::Route(pager)) => {
                    let identity = pager.prepared_identity().to_vec();
                    let page = pager
                        .next_page(
                            source,
                            &identity,
                            &mut self.allowance,
                            candidate_page,
                            expansions,
                            scans,
                            page_deadline,
                        )
                        .map_err(anyhow::Error::new)?;
                    counters.route_searches += page.searches;
                    counters.route_expanded += page.expanded;
                    let new_ids = page.candidate_ids.iter().copied().collect::<BTreeSet<_>>();
                    for id in page.candidate_ids {
                        self.pending_native
                            .push_back(PendingNative::Route { id, update: false });
                    }
                    for id in page.updated_candidate_ids {
                        if new_ids.contains(&id) {
                            continue;
                        }
                        self.pending_native
                            .push_back(PendingNative::Route { id, update: true });
                    }
                    match page.status {
                        RoutePageStatus::Pending => {}
                        RoutePageStatus::Complete(_) => self.direct_done = true,
                        RoutePageStatus::Unknown(stop) => {
                            record_request_stop(
                                counters,
                                &self.request(source.revision()),
                                &self.allowance,
                                stop,
                            );
                            self.direct_done = true;
                        }
                    }
                }
                Some(DirectPager::Network(pager)) => {
                    let page = pager.next_page(
                        source,
                        factory,
                        counters,
                        &mut self.allowance,
                        candidate_page,
                        held_repair_slot,
                        expansions,
                        scans,
                        page_deadline,
                    )?;
                    for id in page.candidate_ids {
                        self.pending_native.push_back(PendingNative::Network { id });
                    }
                    match page.status {
                        NetworkPageStatus::Pending => {}
                        NetworkPageStatus::Complete => self.direct_done = true,
                        NetworkPageStatus::Unknown(_) => {
                            counters.network_unknown += 1;
                            self.direct_done = true;
                        }
                    }
                }
                None => {}
            }
        } else if !self.repair_done {
            self.first_repair = true;
            let pager = self.repair.as_mut().expect("prepared repair scope");
            let page = pager.next_page(
                factory,
                counters,
                &mut self.allowance,
                candidate_page,
                expansions,
                scans,
                page_deadline,
            )?;
            // Repair's first turn must be publishable before a direct copy
            // that already failed tree admission; retain the direct ID next.
            for id in page.candidate_ids.into_iter().rev() {
                self.pending_native.push_front(PendingNative::Repair { id });
            }
            match page.status {
                RepairPageStatus::Pending => {}
                RepairPageStatus::Complete => self.repair_done = true,
                RepairPageStatus::Unknown(stop) => {
                    record_request_stop(
                        counters,
                        &self.request(source.revision()),
                        &self.allowance,
                        stop,
                    );
                    self.repair_done = true;
                }
            }
        }
        let (published, stop) = self.flush_pending();
        actions.extend(published);
        self.release_done_pagers(source);
        if let Some(stop) = stop {
            record_request_stop(
                counters,
                &self.request(source.revision()),
                &self.allowance,
                stop,
            );
        }
        self.pages += 1;
        Self::record_actions(&actions, counters);
        if !self.complete() {
            if !first
                && actions.is_empty()
                && self.pending_native.is_empty()
                && self.allowance.usage().categories == usage_before.categories
            {
                record_request_stop(
                    counters,
                    &self.request(source.revision()),
                    &self.allowance,
                    AllowanceStop {
                        reason: "lower-page-no-progress",
                        category: None,
                        limit: 0,
                        used: 0,
                        deadline_overshoot_ms: 0,
                        attempted_bytes: None,
                        aggregate_peak_bytes: None,
                    },
                );
                if direct_stage {
                    self.direct_done = true;
                    match self.direct.take() {
                        Some(DirectPager::Route(pager)) => pager.release(&mut self.allowance),
                        Some(DirectPager::Network(pager)) => {
                            pager.release(source, &mut self.allowance)
                        }
                        None => {}
                    }
                } else {
                    self.repair_done = true;
                    if let Some(pager) = self.repair.take() {
                        pager.release(&mut self.allowance);
                    }
                }
            }
            if !self.complete() {
                if let Err(stop) = self.allowance.pause() {
                    self.stop(source.revision(), counters, stop);
                    self.release_done_pagers(source);
                }
            }
        }
        Ok(actions)
    }
}

/// The node owns one immutable source Session; repair and the lower scheduler
/// share its Arc. Each logical task owns one allowance across direct and repair.
pub(super) struct LowerPager {
    source: Arc<Session>,
    source_identity: Option<String>,
    factory_identity: Arc<()>,
    memory: SearchMemoryBudget,
    pinned_external_bytes: usize,
    hash_retry_live: Option<usize>,
    tasks: Vec<Task>,
    cursor: usize,
}

impl LowerPager {
    fn verify_factory(&self, factory: &Factory<'_>) -> Result<()> {
        if !Arc::ptr_eq(&self.factory_identity, &factory.identity) {
            return Err(anyhow!("lower pager factory changed"));
        }
        Ok(())
    }

    pub(super) fn new(
        source: Arc<Session>,
        factory: &Factory<'_>,
        counters: &mut Counters,
        memory: &SearchMemoryBudget,
        pinned_external_bytes: usize,
    ) -> Result<Self> {
        let mut groups = BTreeMap::<(String, TransportKind), Vec<Request>>::new();
        for (index, connection) in source.problem().connections.iter().enumerate() {
            if !source.route_is_complete(index) {
                groups
                    .entry((connection.commodity.clone(), connection.transport))
                    .or_default()
                    .push(Request {
                        state_revision: source.revision(),
                        connection: connection.id.clone(),
                    });
            }
        }
        let mut tasks = Vec::new();
        for requests in groups.into_values() {
            let mut pending = 0;
            for request in &requests {
                match source
                    .route_request_readiness(request)
                    .map_err(anyhow::Error::new)?
                {
                    RouteReadiness::Ready => {}
                    RouteReadiness::PendingPlacement { .. } => pending += 1,
                }
            }
            if pending > 0 {
                counters.pending_route_networks += 1;
                counters.pending_route_connections += pending;
                continue;
            }
            let kind = if requests.len() == 1 {
                TaskKind::Route(requests[0].clone())
            } else {
                TaskKind::Network(
                    requests
                        .into_iter()
                        .map(|request| request.connection)
                        .collect(),
                )
            };
            let mut allowance = RequestAllowance::with_memory_budget(
                factory.candidate.request_allowance.clone(),
                Some(memory.clone()),
            );
            allowance
                .pause()
                .map_err(|stop| anyhow!("task allowance: {stop:?}"))?;
            tasks.push(Task {
                kind,
                allowance,
                memory: memory.clone(),
                published: Vec::new(),
                route_guard_index: BTreeMap::new(),
                updated_routes: Vec::new(),
                pending_native: VecDeque::new(),
                pending_retry_live: None,
                guidance_preparation: None,
                prepared_guidance_input: None,
                route_open: None,
                route_preparation: None,
                route_output_bytes: 0,
                guidance_pending: false,
                direct: None,
                repair: None,
                scope_prepared: false,
                scope_sizes: None,
                scope_retry_live: None,
                direct_done: false,
                repair_done: factory.repair_max_removed == 0,
                first_direct: false,
                first_repair: false,
                first_quotas: [0; CUMULATIVE_WORK_KINDS.len()],
                first_duration: Duration::ZERO,
                pages: 0,
                published_actions: 0,
                repair_published: false,
            });
        }
        Ok(Self {
            source,
            source_identity: None,
            factory_identity: Arc::clone(&factory.identity),
            memory: memory.clone(),
            pinned_external_bytes,
            hash_retry_live: None,
            tasks,
            cursor: 0,
        })
    }

    pub(super) fn pending(&self) -> usize {
        self.tasks.iter().filter(|task| !task.complete()).count()
    }

    pub(super) fn hash_memory_pending(&self) -> bool {
        self.hash_retry_live.is_some()
    }

    pub(super) fn hash_memory_retry_ready(&self) -> bool {
        self.hash_retry_live
            .is_some_and(|at_stop| self.memory.live_bytes() < at_stop)
    }

    pub(super) fn publication_memory_retry_ready(&self) -> bool {
        self.tasks.iter().any(|task| {
            !task.pending_native.is_empty()
                && task
                    .pending_retry_live
                    .is_none_or(|at_stop| self.memory.live_bytes() < at_stop)
        })
    }

    pub(super) fn scope_memory_retry_ready(&self) -> bool {
        self.tasks.iter().any(|task| {
            task.scope_retry_live
                .is_some_and(|at_stop| self.memory.live_bytes() < at_stop)
        })
    }

    pub(super) fn scope_memory_pending(&self) -> bool {
        self.tasks
            .iter()
            .any(|task| task.scope_retry_live.is_some())
    }

    pub(super) fn pending_memory_stop(&self) -> Option<AllowanceStop> {
        self.tasks
            .iter()
            .find(|task| task.scope_retry_live.is_some() || task.pending_retry_live.is_some())
            .and_then(|task| task.allowance.last_stop())
    }

    pub(super) fn all_pending_blocked_on_memory(&self) -> bool {
        if self.hash_memory_pending() && !self.hash_memory_retry_ready() {
            return true;
        }
        let mut pending = self.tasks.iter().filter(|task| !task.complete()).peekable();
        pending.peek().is_some()
            && pending.all(|task| {
                task.scope_retry_live
                    .is_some_and(|at_stop| self.memory.live_bytes() >= at_stop)
                    || (!task.pending_native.is_empty()
                        && task
                            .pending_retry_live
                            .is_some_and(|at_stop| self.memory.live_bytes() >= at_stop))
            })
    }

    fn ensure_source_identity(&mut self, counters: &mut Counters) -> Result<bool> {
        if self.source_identity.is_some() {
            return Ok(true);
        }
        if self
            .hash_retry_live
            .is_some_and(|at_stop| self.memory.live_bytes() >= at_stop)
        {
            return Ok(false);
        }
        let reservation = match TreeReservation::new(
            &self.memory,
            self.source.hierarchical_state_hash_workspace_bytes(),
        ) {
            Ok(reservation) => reservation,
            Err(stop) => {
                capture_tree_memory_stop(counters, self.source.revision(), &self.memory, stop);
                self.hash_retry_live = Some(self.memory.live_bytes());
                return Ok(false);
            }
        };
        self.source_identity = Some(state_sha256(&self.source)?);
        drop(reservation);
        self.hash_retry_live = None;
        Ok(true)
    }

    pub(super) fn take_updated_routes(&mut self) -> Vec<RouteUpdate> {
        self.tasks
            .iter_mut()
            .flat_map(|task| std::mem::take(&mut task.updated_routes))
            .collect()
    }

    /// Retain the established bounded batch inference path for newly opened
    /// singleton tasks. Scope authorization has already run for every member.
    fn prepare_guided_routes(
        &mut self,
        indices: &[usize],
        factory: &Factory<'_>,
        counters: &mut Counters,
    ) -> Result<()> {
        let mut prepared = Vec::new();
        let mut fallback = Vec::new();
        for &index in indices {
            let task = &mut self.tasks[index];
            if task.complete() || task.direct_done || task.direct.is_some() {
                continue;
            }
            if !matches!(task.kind, TaskKind::Route(_)) {
                continue;
            }
            task.allowance.resume();
            task.guidance_pending = false;
            if task.route_open.is_some() || task.route_preparation.is_some() {
                continue;
            }
            let input = match task.prepared_guidance_input.take() {
                Some(input) => Ok(input),
                None => task.guidance_input(&self.source, factory.candidate, &factory.domain),
            };
            match input {
                Ok(input) => {
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
                    let bytes = inference_reservation_bytes(&input, factory.guidance_heads);
                    match task.allowance.reserve_peak(bytes) {
                        Err(stop) if stop.reason == "page-quantum" => {
                            task.prepared_guidance_input = Some(input);
                            task.guidance_pending = true;
                            task.first_direct = true;
                            task.pages += 1;
                            task.allowance.end_page_window();
                        }
                        Err(stop) => {
                            let held = input.reserved_bytes;
                            drop(input);
                            task.allowance.release_peak(held);
                            task.stop(self.source.revision(), counters, stop);
                        }
                        Ok(()) => {
                            match task.allowance.charge(WorkKind::InferenceElements, elements) {
                                Ok(()) => prepared.push((index, input, bytes)),
                                Err(stop) if stop.reason == "page-quantum" => {
                                    task.allowance.release_peak(bytes);
                                    task.prepared_guidance_input = Some(input);
                                    task.guidance_pending = true;
                                    task.first_direct = true;
                                    task.pages += 1;
                                    task.allowance.end_page_window();
                                }
                                Err(stop) => {
                                    let held = input.reserved_bytes;
                                    drop(input);
                                    task.allowance.release_peak(bytes.saturating_add(held));
                                    task.stop(self.source.revision(), counters, stop);
                                }
                            }
                        }
                    }
                }
                Err(RequestFailure::Invalid(diagnostic))
                    if diagnostic.code.starts_with("constructive.route-guidance") =>
                {
                    counters.repair_guidance_fallbacks += 1;
                    fallback.push(index);
                }
                Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum" => {
                    task.guidance_pending = true;
                    task.first_direct = true;
                    task.pages += 1;
                    if task.allowance.page_window_remaining().is_some() {
                        task.allowance.end_page_window();
                    }
                }
                Err(RequestFailure::Stopped(stop)) => {
                    task.stop(self.source.revision(), counters, stop);
                }
                Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                    fallback.push(index);
                }
                Err(RequestFailure::PendingPlacement { .. }) => {
                    return Err(anyhow!(
                        "ready task became pending without a state revision"
                    ));
                }
                Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic.into()),
            }
        }
        let predictions = if prepared.is_empty() {
            Vec::new()
        } else {
            let inputs = prepared
                .iter()
                .map(|(_, input, _)| input)
                .collect::<Vec<_>>();
            match factory
                .guidance
                .predict_native(&inputs, factory.guidance_version, factory.device)
            {
                Ok(guidance) => {
                    counters.guidance_batches += 1;
                    counters.guidance_inputs += inputs.len();
                    guidance.into_iter().map(Some).collect::<Vec<_>>()
                }
                Err(_) => {
                    counters.repair_guidance_fallbacks += inputs.len();
                    vec![None; inputs.len()]
                }
            }
        };
        for ((index, input, bytes), guide) in prepared.into_iter().zip(predictions) {
            let task = &mut self.tasks[index];
            task.retain_prediction(input, guide, bytes);
        }
        for index in fallback {
            self.tasks[index].route_open = Some(RouteOpen {
                guidance: None,
                identity: Vec::new(),
            });
        }
        for &index in indices {
            let task = &mut self.tasks[index];
            if task.complete()
                || task.direct_done
                || task.direct.is_some()
                || (task.route_open.is_none() && task.route_preparation.is_none())
            {
                continue;
            }
            match task.advance_route_preparation(
                &self.source,
                factory.candidate,
                self.source_identity
                    .as_deref()
                    .expect("source identity ready")
                    .as_bytes(),
                index,
            ) {
                Ok(pager) => task.direct = Some(DirectPager::Route(pager)),
                Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum" => {
                    task.guidance_pending = true;
                    task.first_direct = true;
                    task.pages += 1;
                    if task.allowance.page_window_remaining().is_some() {
                        task.allowance.end_page_window();
                    }
                }
                Err(RequestFailure::Stopped(stop)) => {
                    task.stop(self.source.revision(), counters, stop);
                }
                Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                    task.direct_done = true;
                }
                Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic.into()),
                Err(RequestFailure::PendingPlacement { .. }) => {
                    return Err(anyhow!(
                        "ready task became pending without a state revision"
                    ));
                }
            }
        }
        for &index in indices {
            let task = &mut self.tasks[index];
            if !task.complete() {
                if let Err(stop) = task.allowance.pause() {
                    task.stop(self.source.revision(), counters, stop);
                }
            }
            task.release_done_pagers(&self.source);
        }
        Ok(())
    }

    /// Spread the first lower page across the ordered commodity groups, then
    /// let every selected repair alternative take its reserved first turn.
    pub(super) fn first_actions(
        &mut self,
        factory: &Factory<'_>,
        counters: &mut Counters,
    ) -> Result<Vec<MacroAction>> {
        self.verify_factory(factory)?;
        if self.tasks.is_empty() {
            return Ok(Vec::new());
        }
        if !self.ensure_source_identity(counters)? {
            return Ok(Vec::new());
        }
        let count = self
            .tasks
            .len()
            .min(factory.candidate.request_allowance.max_inference_batch);
        let indices = (0..count)
            .map(|slot| slot * self.tasks.len() / count)
            .collect::<Vec<_>>();
        for &index in &indices {
            self.tasks[index].prepare_scope(
                &self.source,
                factory,
                counters,
                self.pinned_external_bytes,
            )?;
        }
        let ready = indices
            .into_iter()
            .filter(|&index| self.tasks[index].scope_prepared)
            .collect::<Vec<_>>();
        self.prepare_guided_routes(&ready, factory, counters)?;
        let mut actions = Vec::new();
        for index in ready {
            let task = &mut self.tasks[index];
            if task.complete() {
                task.release(&self.source);
                continue;
            }
            if task.guidance_pending && task.repair_done {
                continue;
            }
            actions.extend(
                task.page(
                    &self.source,
                    self.source_identity
                        .as_deref()
                        .expect("source identity ready"),
                    factory,
                    counters,
                )?,
            );
            if !task.repair_done && !task.first_repair {
                actions.extend(
                    task.page(
                        &self.source,
                        self.source_identity
                            .as_deref()
                            .expect("source identity ready"),
                        factory,
                        counters,
                    )?,
                );
            }
            if task.complete() {
                task.release(&self.source);
            }
        }
        Ok(actions)
    }

    /// Called only when a node reaches its explicit visit-based widening
    /// schedule, or when a zero-action first page needs a legal action.
    pub(super) fn next_actions(
        &mut self,
        factory: &Factory<'_>,
        counters: &mut Counters,
    ) -> Result<Vec<MacroAction>> {
        self.verify_factory(factory)?;
        if self.pending() == 0 {
            return Ok(Vec::new());
        }
        if !self.ensure_source_identity(counters)? {
            return Ok(Vec::new());
        }
        let total = self.tasks.len();
        if let Some(index) = (0..total).find(|&index| {
            let task = &self.tasks[index];
            !task.pending_native.is_empty()
                && task
                    .pending_retry_live
                    .is_none_or(|at_stop| self.memory.live_bytes() < at_stop)
        }) {
            return self.tasks[index].page(
                &self.source,
                self.source_identity
                    .as_deref()
                    .expect("source identity ready"),
                factory,
                counters,
            );
        }
        let unseen = (0..total).any(|index| {
            let task = &self.tasks[index];
            !task.complete()
                && task.pending_native.is_empty()
                && task
                    .scope_retry_live
                    .is_none_or(|at_stop| self.memory.live_bytes() < at_stop)
                && !task.first_direct
        });
        let index = (0..total)
            .map(|offset| (self.cursor + offset) % total)
            .find(|&index| {
                let task = &self.tasks[index];
                !task.complete()
                    && task.pending_native.is_empty()
                    && task
                        .scope_retry_live
                        .is_none_or(|at_stop| self.memory.live_bytes() < at_stop)
                    && (!unseen || !task.first_direct)
            });
        let Some(index) = index else {
            return Ok(Vec::new());
        };
        self.cursor = (index + 1) % total;
        if self.tasks[index].guidance_pending
            && !self.tasks[index].repair_done
            && !self.tasks[index].first_repair
        {
            return self.tasks[index].page(
                &self.source,
                self.source_identity
                    .as_deref()
                    .expect("source identity ready"),
                factory,
                counters,
            );
        }
        if !self.tasks[index].scope_prepared || self.tasks[index].guidance_pending {
            let mut batch = vec![index];
            for offset in 1..total {
                if self.tasks[index].guidance_pending {
                    break;
                }
                if batch.len() >= factory.candidate.request_allowance.max_inference_batch {
                    break;
                }
                let other = (index + offset) % total;
                if !self.tasks[other].scope_prepared
                    && self.tasks[other]
                        .scope_retry_live
                        .is_none_or(|at_stop| self.memory.live_bytes() < at_stop)
                    && matches!(self.tasks[other].kind, TaskKind::Route(_))
                {
                    batch.push(other);
                }
            }
            for &member in &batch {
                self.tasks[member].prepare_scope(
                    &self.source,
                    factory,
                    counters,
                    self.pinned_external_bytes,
                )?;
            }
            let ready = batch
                .into_iter()
                .filter(|&member| self.tasks[member].scope_prepared)
                .collect::<Vec<_>>();
            self.prepare_guided_routes(&ready, factory, counters)?;
        }
        if !self.tasks[index].scope_prepared {
            return Ok(Vec::new());
        }
        if self.tasks[index].guidance_pending {
            return Ok(Vec::new());
        }
        let task = &mut self.tasks[index];
        let mut actions = task.page(
            &self.source,
            self.source_identity
                .as_deref()
                .expect("source identity ready"),
            factory,
            counters,
        )?;
        if !task.repair_done && !task.first_repair {
            actions.extend(
                task.page(
                    &self.source,
                    self.source_identity
                        .as_deref()
                        .expect("source identity ready"),
                    factory,
                    counters,
                )?,
            );
        }
        if task.complete() {
            task.release(&self.source);
        }
        Ok(actions)
    }

    pub(super) fn release(mut self) {
        for task in &mut self.tasks {
            task.release(&self.source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_pilot_native::constructive::{Command, Limits};

    #[test]
    fn repair_scope_wait_retains_sizing_and_retries_after_shared_memory_release() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-repair/fixture.json"
        )))?;
        let mut source = Session::new(
            serde_json::from_value(fixture["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value::<Limits>(fixture["limits"].clone())?,
        )
        .map_err(anyhow::Error::new)?;
        for command in fixture["prefix"].as_array().expect("prefix commands") {
            let step = source.step(serde_json::from_value::<Command>(command.clone())?);
            assert!(step.accepted, "{step:?}");
        }
        let candidate: aic_pilot_native::constructive::routing_candidates::Config =
            serde_json::from_value(fixture["candidate"].clone())?;
        let selected = vec!["second".to_owned()];
        let mut sizing_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let sizes = PreparedRepair::measure_preparation(
            &source,
            &selected,
            &candidate,
            &mut sizing_allowance,
        )
        .map_err(anyhow::Error::new)?;
        let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let first_pressure_bytes = budget.limit_bytes() - sizes.peak_bytes() + 2;
        let pressure = TreeReservation::new(&budget, first_pressure_bytes)
            .map_err(|stop| anyhow!("memory pressure fixture: {stop:?}"))?;
        let model_config = crate::schema::RouteGuidanceModelConfig {
            schema: crate::schema::RouteGuidanceModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            raster_width: 8,
            heads: 1,
            maximum_edge_cost: 32,
        };
        let (order, state) =
            crate::hierarchical_search::GuidanceModel::initial_state(&model_config, 17)?;
        let model = crate::hierarchical_search::GuidanceModel::load(
            &model_config,
            &order,
            &state,
            crate::hierarchical_search::Device::Cpu,
        )?;
        let factory = Factory::new(
            &model,
            "test",
            crate::hierarchical_search::Device::Cpu,
            crate::hierarchical_search::DomainConfig {
                placement: candidate.bounds,
                extension_lengths: vec![1],
            },
            &candidate,
            1,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let source = Arc::new(source);
        let mut pager = LowerPager::new(
            Arc::clone(&source),
            &factory,
            &mut Counters::default(),
            &budget,
            0,
        )?;
        assert_eq!(pager.tasks.len(), 1);
        assert!(matches!(pager.tasks[0].kind, TaskKind::Route(_)));
        pager.source_identity = Some(state_sha256(&source)?);
        assert!(
            pager
                .first_actions(&factory, &mut Counters::default())?
                .is_empty()
        );
        assert!(!pager.tasks[0].scope_prepared);
        assert!(pager.tasks[0].scope_sizes.is_some());
        assert!(pager.tasks[0].guidance_preparation.is_none());
        assert!(pager.tasks[0].direct.is_none());
        assert!(pager.tasks[0].repair.is_none());
        assert_eq!(pager.tasks[0].pages, 0);
        assert_eq!(budget.live_bytes(), first_pressure_bytes);
        assert!(pager.all_pending_blocked_on_memory());
        let charged_sizing = pager.tasks[0].allowance.usage().categories;
        assert_eq!(charged_sizing, sizing_allowance.usage().categories);
        assert!(
            pager
                .next_actions(&factory, &mut Counters::default())?
                .is_empty()
        );
        assert_eq!(pager.tasks[0].allowance.usage().categories, charged_sizing);
        assert!(!pager.scope_memory_retry_ready());

        drop(pressure);
        assert!(pager.scope_memory_retry_ready());
        let second_pressure = TreeReservation::new(&budget, first_pressure_bytes - 1)
            .map_err(|stop| anyhow!("second memory pressure fixture: {stop:?}"))?;
        assert!(
            pager
                .next_actions(&factory, &mut Counters::default())?
                .is_empty()
        );
        assert!(!pager.tasks[0].scope_prepared);
        assert_eq!(pager.tasks[0].allowance.usage().categories, charged_sizing);
        assert_eq!(
            pager.tasks[0].scope_retry_live,
            Some(first_pressure_bytes - 1)
        );
        assert!(!pager.scope_memory_retry_ready());
        drop(second_pressure);
        assert!(pager.scope_memory_retry_ready());
        let node_memory = TreeReservation::new(&budget, 0)
            .map_err(|stop| anyhow!("cached node fixture: {stop:?}"))?;
        let mut nodes = vec![crate::hierarchical_search::Node {
            session: Arc::clone(&source),
            memory: node_memory,
            visits: 1,
            value: None,
            actions: Some(Vec::new()),
            placement_pages: None,
            lower: Some(pager),
            lower_loaded_pages: 1,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        crate::hierarchical_search::maybe_widen_lower(
            &mut nodes,
            0,
            &factory,
            &mut Counters::default(),
            &budget,
        )?;
        let resumed = nodes[0].lower.as_ref().expect("retained lower pager");
        assert!(resumed.tasks[0].scope_prepared);
        assert!(resumed.tasks[0].scope_sizes.is_none());
        assert!(resumed.tasks[0].scope_retry_live.is_none());
        assert!(resumed.tasks[0].first_quota(WorkKind::Candidates) > 0);
        assert!(!resumed.all_pending_blocked_on_memory());
        assert_eq!(nodes[0].lower_loaded_pages, 1);
        nodes[0]
            .lower
            .take()
            .expect("retained lower pager")
            .release();
        drop(nodes);
        assert_eq!(budget.live_bytes(), 0);

        let mut two_route_source = Session::new(
            serde_json::from_value(fixture["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value::<Limits>(fixture["limits"].clone())?,
        )
        .map_err(anyhow::Error::new)?;
        for command in fixture["prefix"]
            .as_array()
            .expect("prefix commands")
            .iter()
            .take_while(|command| command["kind"] == "place")
        {
            let step = two_route_source.step(serde_json::from_value::<Command>(command.clone())?);
            assert!(step.accepted, "{step:?}");
        }
        let two_route_source = Arc::new(two_route_source);
        let two_owner_budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut first_owner = LowerPager::new(
            Arc::clone(&source),
            &factory,
            &mut Counters::default(),
            &two_owner_budget,
            0,
        )?;
        let mut second_owner = LowerPager::new(
            Arc::clone(&two_route_source),
            &factory,
            &mut Counters::default(),
            &two_owner_budget,
            0,
        )?;
        assert_eq!(first_owner.tasks.len(), 1);
        assert_eq!(second_owner.tasks.len(), 2);
        let scope_bytes = two_route_source.problem().connections.len() * 256 + 4096;
        let available_for_scope_only = scope_bytes + 1;
        let shared_pressure_bytes = two_owner_budget.limit_bytes() - available_for_scope_only;
        let shared_pressure = TreeReservation::new(&two_owner_budget, shared_pressure_bytes)
            .map_err(|stop| anyhow!("two-owner pressure fixture: {stop:?}"))?;
        for (task, owner_source) in [
            (&mut first_owner.tasks[0], &source),
            (&mut second_owner.tasks[0], &two_route_source),
        ] {
            task.prepare_scope(owner_source, &factory, &mut Counters::default(), 0)?;
            assert!(!task.scope_prepared);
            assert!(task.scope_sizes.is_some());
            assert!(task.scope_retry_live.is_some());
        }
        let first_peak = first_owner.tasks[0]
            .scope_sizes
            .as_ref()
            .expect("first sizing")
            .peak_bytes();
        let second_peak = second_owner.tasks[0]
            .scope_sizes
            .as_ref()
            .expect("second sizing")
            .peak_bytes();
        assert_ne!(
            first_peak, second_peak,
            "the shared cap must be tested with different preparation sizes"
        );
        assert!(scope_bytes < available_for_scope_only);
        assert!(available_for_scope_only < first_peak.min(second_peak));
        assert_eq!(
            two_owner_budget.live_bytes(),
            shared_pressure_bytes,
            "neither pending owner retains a partial scope reservation"
        );
        drop(shared_pressure);
        for (task, owner_source) in [
            (&mut first_owner.tasks[0], &source),
            (&mut second_owner.tasks[0], &two_route_source),
        ] {
            task.prepare_scope(owner_source, &factory, &mut Counters::default(), 0)?;
            assert!(task.scope_prepared);
            assert!(task.scope_sizes.is_none());
        }
        first_owner.release();
        second_owner.release();
        assert_eq!(two_owner_budget.live_bytes(), 0);

        let terminal_budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let root_bytes = source
            .clone_reservation_bytes()
            .saturating_add(std::mem::size_of::<crate::hierarchical_search::Node>());
        let root_memory = TreeReservation::new(&terminal_budget, root_bytes)
            .map_err(|stop| anyhow!("terminal node fixture: {stop:?}"))?;
        assert!(sizes.peak_bytes() > source.hierarchical_state_hash_workspace_bytes());
        let terminal_pressure_bytes = terminal_budget
            .limit_bytes()
            .checked_sub(root_bytes)
            .and_then(|remaining| remaining.checked_sub(sizes.peak_bytes()))
            .expect("root and repair scope fit separately")
            + 1;
        let terminal_pressure = TreeReservation::new(&terminal_budget, terminal_pressure_bytes)
            .map_err(|stop| anyhow!("terminal pressure fixture: {stop:?}"))?;
        let mut terminal_nodes = vec![crate::hierarchical_search::Node {
            session: Arc::clone(&source),
            memory: root_memory,
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        crate::hierarchical_search::ensure_node_expanded(
            &mut terminal_nodes,
            0,
            &factory,
            &mut Counters::default(),
            false,
            &terminal_budget,
        )?;
        assert!(
            terminal_nodes[0]
                .actions
                .as_ref()
                .is_some_and(Vec::is_empty)
        );
        assert!(terminal_nodes[0].lower.is_none());
        assert_eq!(
            terminal_nodes[0]
                .no_macro_diagnostic
                .as_ref()
                .expect("typed terminal resource stop")
                .code,
            "hierarchical.aggregate-memory-limit"
        );
        drop(terminal_nodes);
        drop(terminal_pressure);
        assert_eq!(terminal_budget.live_bytes(), 0);

        let mixed_budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut mixed = LowerPager::new(
            Arc::clone(&source),
            &factory,
            &mut Counters::default(),
            &mixed_budget,
            0,
        )?;
        let mut duplicate = LowerPager::new(
            Arc::clone(&source),
            &factory,
            &mut Counters::default(),
            &mixed_budget,
            0,
        )?;
        mixed.tasks.append(&mut duplicate.tasks);
        assert_eq!(mixed.tasks.len(), 2);
        mixed.source_identity = Some(state_sha256(&source)?);
        // The previous section exercises a real aggregate refusal. Here the
        // retained wait mark isolates scheduler filtering while another task
        // has enough memory to use both of its first turns.
        let wait_marker = TreeReservation::new(&mixed_budget, 256 * 1024 * 1024)
            .map_err(|stop| anyhow!("mixed-batch wait marker: {stop:?}"))?;
        let wait_live = mixed_budget.live_bytes();
        let waiting_selected = mixed.tasks[1].selected().to_vec();
        mixed.tasks[1].allowance.resume();
        mixed.tasks[1].scope_sizes = Some(
            PreparedRepair::measure_preparation(
                &source,
                &waiting_selected,
                &candidate,
                &mut mixed.tasks[1].allowance,
            )
            .map_err(anyhow::Error::new)?,
        );
        mixed.tasks[1].scope_retry_live = Some(wait_live);
        mixed.tasks[1]
            .allowance
            .pause()
            .map_err(|stop| anyhow!("mixed-batch wait allowance: {stop:?}"))?;
        mixed.tasks[0].prepare_scope(&source, &factory, &mut Counters::default(), 0)?;
        assert!(mixed.tasks[0].scope_prepared);
        assert!(mixed.tasks[0].repair.is_some());
        let waiting_work = mixed.tasks[1].allowance.usage().categories;
        let mut mixed_counters = Counters::default();
        let _ = mixed.first_actions(&factory, &mut mixed_counters)?;
        assert!(mixed_counters.guidance_inputs > 0);
        assert_eq!(mixed.tasks[1].allowance.usage().categories, waiting_work);
        assert!(!mixed.tasks[1].scope_prepared);
        assert!(mixed.tasks[1].scope_sizes.is_some());
        assert!(mixed.tasks[1].guidance_preparation.is_none());
        assert!(mixed.tasks[1].prepared_guidance_input.is_none());
        assert!(mixed.tasks[1].route_open.is_none());
        assert!(mixed.tasks[1].route_preparation.is_none());
        assert!(!mixed.tasks[1].guidance_pending);
        assert!(mixed.tasks[1].direct.is_none());
        assert!(mixed.tasks[1].repair.is_none());
        assert!(!mixed.tasks[1].direct_done);
        assert_eq!(mixed.tasks[1].pages, 0);
        assert!(mixed.tasks[0].first_direct);
        assert!(mixed.tasks[0].first_repair);
        assert!(mixed.tasks[0].pages >= 2);
        assert!(!mixed.tasks[0].complete(), "ready task retains later work");
        let ready_pages = mixed.tasks[0].pages;
        let pending_work = mixed.tasks[1].allowance.usage().categories;
        let _ = mixed.next_actions(&factory, &mut Counters::default())?;
        assert!(mixed.tasks[0].pages > ready_pages || mixed.tasks[0].complete());
        assert_eq!(mixed.tasks[1].allowance.usage().categories, pending_work);
        assert_eq!(mixed.tasks[1].pages, 0);
        drop(wait_marker);
        assert!(mixed.scope_memory_retry_ready());
        for _ in 0..4 {
            if mixed.tasks[1].scope_prepared {
                break;
            }
            let _ = mixed.next_actions(&factory, &mut Counters::default())?;
        }
        assert!(mixed.tasks[1].scope_prepared);
        mixed.release();
        assert_eq!(mixed_budget.live_bytes(), 0);

        let later_budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut later = LowerPager::new(
            Arc::clone(&source),
            &factory,
            &mut Counters::default(),
            &later_budget,
            0,
        )?;
        let mut later_duplicate = LowerPager::new(
            Arc::clone(&source),
            &factory,
            &mut Counters::default(),
            &later_budget,
            0,
        )?;
        later.tasks.append(&mut later_duplicate.tasks);
        assert!(matches!(later.tasks[0].kind, TaskKind::Route(_)));
        assert!(matches!(later.tasks[1].kind, TaskKind::Route(_)));
        later.source_identity = Some(state_sha256(&source)?);
        let later_wait_marker = TreeReservation::new(&later_budget, 256 * 1024 * 1024)
            .map_err(|stop| anyhow!("later-batch wait marker: {stop:?}"))?;
        let waiting_selected = later.tasks[1].selected().to_vec();
        later.tasks[1].allowance.resume();
        later.tasks[1].scope_sizes = Some(
            PreparedRepair::measure_preparation(
                &source,
                &waiting_selected,
                &candidate,
                &mut later.tasks[1].allowance,
            )
            .map_err(anyhow::Error::new)?,
        );
        later.tasks[1].scope_retry_live = Some(later_budget.live_bytes());
        later.tasks[1]
            .allowance
            .pause()
            .map_err(|stop| anyhow!("later-batch wait allowance: {stop:?}"))?;
        let pending_work = later.tasks[1].allowance.usage().categories;
        let mut later_counters = Counters::default();
        let _ = later.next_actions(&factory, &mut later_counters)?;
        assert!(later.tasks[0].scope_prepared);
        assert!(later.tasks[0].first_quota(WorkKind::Candidates) > 0);
        assert!(later_counters.guidance_inputs > 0);
        assert!(later.tasks[0].first_direct);
        assert!(later.tasks[0].first_repair);
        assert!(later.tasks[0].pages >= 2);
        assert!(!later.tasks[1].scope_prepared);
        assert!(later.tasks[1].scope_sizes.is_some());
        assert!(later.tasks[1].scope_retry_live.is_some());
        assert_eq!(later.tasks[1].allowance.usage().categories, pending_work);
        assert!(later.tasks[1].guidance_preparation.is_none());
        assert!(later.tasks[1].prepared_guidance_input.is_none());
        assert!(later.tasks[1].route_open.is_none());
        assert!(later.tasks[1].route_preparation.is_none());
        assert_eq!(later.tasks[1].pages, 0);
        drop(later_wait_marker);
        later.release();
        assert_eq!(later_budget.live_bytes(), 0);
        Ok(())
    }

    #[test]
    fn task_retains_guidance_input_work_across_first_page_stop() -> Result<()> {
        let panel: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-search/panel.json"
        )))?;
        let case = &panel["cases"][0];
        let mut source = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value::<Limits>(panel["limits"].clone())?,
        )
        .map_err(anyhow::Error::new)?;
        for command in case["prefix"].as_array().expect("prefix commands") {
            let step = source.step(serde_json::from_value::<Command>(command.clone())?);
            assert!(step.accepted, "{step:?}");
        }
        let search: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-search/config.json"
        )))?;
        let candidate: aic_pilot_native::constructive::routing_candidates::Config =
            serde_json::from_value(search["candidate"].clone())?;
        let learning = aic_pilot_native::constructive::learning::Config {
            placement: candidate.bounds,
            extension_lengths: vec![1],
        };
        let request = Request {
            state_revision: source.revision(),
            connection: "root".into(),
        };
        let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut task = Task {
            kind: TaskKind::Route(request.clone()),
            allowance: RequestAllowance::with_memory_budget(
                candidate.request_allowance.clone(),
                Some(budget.clone()),
            ),
            memory: budget.clone(),
            published: Vec::new(),
            route_guard_index: BTreeMap::new(),
            updated_routes: Vec::new(),
            pending_native: VecDeque::new(),
            pending_retry_live: None,
            guidance_preparation: None,
            prepared_guidance_input: None,
            route_open: None,
            route_preparation: None,
            route_output_bytes: 0,
            guidance_pending: false,
            direct: None,
            repair: None,
            scope_prepared: true,
            scope_sizes: None,
            scope_retry_live: None,
            direct_done: false,
            repair_done: true,
            first_direct: false,
            first_repair: true,
            first_quotas: [0; CUMULATIVE_WORK_KINDS.len()],
            first_duration: Duration::ZERO,
            pages: 0,
            published_actions: 0,
            repair_published: false,
        };
        let mut direct_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let direct = source
            .route_guidance_input_with_allowance(
                &request,
                &candidate,
                &learning,
                &mut direct_allowance,
            )
            .map_err(|failure| anyhow!("direct guidance failed: {failure:?}"))?;
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::EndpointVisits)
            .unwrap();
        quotas[slot] = 1;
        task.allowance
            .begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            task.guidance_input(&source, &candidate, &learning),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        assert!(task.guidance_preparation.is_some());
        assert!(budget.live_bytes() > 0);
        task.allowance.end_page_window();
        let resumed = task
            .guidance_input(&source, &candidate, &learning)
            .map_err(|failure| anyhow!("resumed guidance failed: {failure:?}"))?;
        assert!(task.guidance_preparation.is_none());
        assert_eq!(
            serde_json::to_value(&resumed)?,
            serde_json::to_value(&direct)?
        );
        assert_eq!(
            task.allowance.usage().categories,
            direct_allowance.usage().categories
        );
        assert_eq!(
            task.allowance.usage().peak_live_bytes,
            direct_allowance.usage().peak_live_bytes
        );
        let mut bound_input = resumed.clone();
        bound_input.connection = "long-port-identity".repeat(65_536);
        bound_input.bounds = aic_layout::geometry::Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 0,
            max_y: 0,
        };
        bound_input.grid =
            vec![0.0; aic_pilot_native::constructive::routing_candidates::GUIDANCE_GRID_CHANNELS];
        bound_input.graph = Arc::new(aic_pilot_native::constructive::learning::Input {
            schema: aic_pilot_native::constructive::learning::SCHEMA,
            entity_features: vec![[0.0; aic_pilot_native::constructive::learning::ENTITY_FEATURES]],
            relation_index: Vec::new(),
            relation_features: Vec::new(),
            spatial_features: Vec::new(),
            spatial_owners: Vec::new(),
            global_features: [0.0; aic_pilot_native::constructive::learning::GLOBAL_FEATURES],
            focus_features: [0.0; aic_pilot_native::constructive::learning::FOCUS_FEATURES],
            focus_entity: 0,
            bridge_resources: Vec::new(),
        });
        let area = usize::try_from(bound_input.bounds.width() * bound_input.bounds.height())?;
        let heads = (0..8)
            .map(
                |head| aic_pilot_native::constructive::routing_candidates::GuidanceHead {
                    id: format!("head-{head}"),
                    directed_costs: Vec::with_capacity(4 * area),
                    predicted_success: 0.0,
                    uncertainty: 0.0,
                },
            )
            .collect();
        let widest_output = Guidance {
            schema: aic_pilot_native::constructive::routing_candidates::GUIDANCE_SCHEMA.into(),
            state_revision: request.state_revision,
            connection: bound_input.connection.clone(),
            model_version: "v".repeat(256),
            heads,
        };
        assert!(
            guidance_output_bytes(&widest_output)
                <= inference_reservation_bytes(&bound_input, widest_output.heads.len()),
            "native guidance output must fit its pre-inference reservation"
        );
        let inferred = Guidance {
            schema: aic_pilot_native::constructive::routing_candidates::GUIDANCE_SCHEMA.into(),
            state_revision: request.state_revision,
            connection: request.connection,
            model_version: "test-model".into(),
            heads: vec![
                aic_pilot_native::constructive::routing_candidates::GuidanceHead {
                    id: "head-0".into(),
                    directed_costs: Vec::new(),
                    predicted_success: 0.5,
                    uncertainty: 0.25,
                },
            ],
        };
        let output_bytes = guidance_output_bytes(&inferred);
        let inference_bytes = inference_reservation_bytes(&resumed, inferred.heads.len());
        task.allowance
            .reserve_peak(inference_bytes)
            .map_err(|stop| anyhow!("pre-inference reservation: {stop:?}"))?;
        task.retain_prediction(resumed, Some(inferred), inference_bytes);
        assert!(task.route_open.is_some());
        assert_eq!(task.route_output_bytes, output_bytes);
        assert_eq!(budget.live_bytes(), output_bytes);
        direct_allowance.release_peak(direct.reserved_bytes);
        task.release(&source);
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }

    #[test]
    fn task_retains_native_route_preparation_across_page_stop() -> Result<()> {
        let panel: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-search/panel.json"
        )))?;
        let case = &panel["cases"][0];
        let mut source = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value::<Limits>(panel["limits"].clone())?,
        )
        .map_err(anyhow::Error::new)?;
        for command in case["prefix"].as_array().expect("prefix commands") {
            let step = source.step(serde_json::from_value::<Command>(command.clone())?);
            assert!(step.accepted, "{step:?}");
        }
        let search: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-search/config.json"
        )))?;
        let candidate: aic_pilot_native::constructive::routing_candidates::Config =
            serde_json::from_value(search["candidate"].clone())?;
        let request = Request {
            state_revision: source.revision(),
            connection: "root".into(),
        };
        let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut task = Task {
            kind: TaskKind::Route(request.clone()),
            allowance: RequestAllowance::with_memory_budget(
                candidate.request_allowance.clone(),
                Some(budget.clone()),
            ),
            memory: budget.clone(),
            published: Vec::new(),
            route_guard_index: BTreeMap::new(),
            updated_routes: Vec::new(),
            pending_native: VecDeque::new(),
            pending_retry_live: None,
            guidance_preparation: None,
            prepared_guidance_input: None,
            route_open: Some(RouteOpen {
                guidance: None,
                identity: Vec::new(),
            }),
            route_preparation: None,
            route_output_bytes: 0,
            guidance_pending: false,
            direct: None,
            repair: None,
            scope_prepared: true,
            scope_sizes: None,
            scope_retry_live: None,
            direct_done: false,
            repair_done: true,
            first_direct: false,
            first_repair: true,
            first_quotas: [0; CUMULATIVE_WORK_KINDS.len()],
            first_duration: Duration::ZERO,
            pages: 0,
            published_actions: 0,
            repair_published: false,
        };
        let identity = b"fixed-source-identity";
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let endpoint_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::EndpointVisits)
            .expect("endpoint visits are cumulative");
        quotas[endpoint_slot] = 1;
        task.allowance
            .begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            task.advance_route_preparation(&source, &candidate, identity, 0),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        assert!(task.route_open.is_none());
        assert!(task.route_preparation.is_some());
        assert!(budget.live_bytes() > 0);
        task.allowance.end_page_window();
        let resumed = task
            .advance_route_preparation(&source, &candidate, identity, 0)
            .map_err(|failure| anyhow!("resumed native route preparation: {failure:?}"))?;
        let mut direct_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut exact_identity = identity.to_vec();
        exact_identity.extend_from_slice(&0u64.to_le_bytes());
        let direct = RouteCandidatePager::new(
            &source,
            request.clone(),
            candidate.clone(),
            None,
            exact_identity,
            &mut direct_allowance,
        )
        .map_err(|failure| anyhow!("uninterrupted native route preparation: {failure:?}"))?;
        assert_eq!(resumed.prepared_identity(), direct.prepared_identity());
        assert_eq!(
            task.allowance.usage().categories,
            direct_allowance.usage().categories
        );
        task.direct = Some(DirectPager::Route(resumed));
        task.release(&source);
        direct.release(&mut direct_allowance);
        assert_eq!(budget.live_bytes(), 0);

        let guided = Guidance {
            schema: aic_pilot_native::constructive::routing_candidates::GUIDANCE_SCHEMA.into(),
            state_revision: request.state_revision,
            connection: request.connection.clone(),
            model_version: "test-model".into(),
            heads: vec![
                aic_pilot_native::constructive::routing_candidates::GuidanceHead {
                    id: "head-0".into(),
                    directed_costs: vec![
                        aic_pilot_native::constructive::routing_candidates::DirectedCost {
                            point: aic_layout::geometry::Point {
                                x: candidate.bounds.min_x,
                                y: candidate.bounds.min_y,
                            },
                            direction: 0,
                            cost: 1,
                        },
                        aic_pilot_native::constructive::routing_candidates::DirectedCost {
                            point: aic_layout::geometry::Point {
                                x: candidate.bounds.min_x,
                                y: candidate.bounds.min_y,
                            },
                            direction: 1,
                            cost: 2,
                        },
                    ],
                    predicted_success: 0.5,
                    uncertainty: 0.25,
                },
            ],
        };
        let guided_bytes = guidance_output_bytes(&guided);
        task.allowance = RequestAllowance::with_memory_budget(
            candidate.request_allowance.clone(),
            Some(budget.clone()),
        );
        task.allowance
            .reserve_peak(guided_bytes)
            .map_err(|stop| anyhow!("guided output reservation: {stop:?}"))?;
        task.route_output_bytes = guided_bytes;
        task.route_open = Some(RouteOpen {
            guidance: Some(guided.clone()),
            identity: Vec::new(),
        });
        let mut guide_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let scan_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::RoutingScans)
            .expect("routing scans are cumulative");
        guide_quotas[scan_slot] = 1;
        task.allowance
            .begin_page_window(guide_quotas, Duration::from_secs(1));
        assert!(matches!(
            task.advance_route_preparation(&source, &candidate, identity, 0),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        assert!(task.route_preparation.is_some());
        assert!(budget.live_bytes() >= guided_bytes);
        task.allowance.end_page_window();
        let resumed_guided = task
            .advance_route_preparation(&source, &candidate, identity, 0)
            .map_err(|failure| anyhow!("resumed guided preparation: {failure:?}"))?;
        let mut direct_guided_allowance =
            RequestAllowance::new(candidate.request_allowance.clone());
        let mut exact_identity = identity.to_vec();
        exact_identity.extend_from_slice(&0u64.to_le_bytes());
        let direct_guided = RouteCandidatePager::new(
            &source,
            request,
            candidate,
            Some(guided),
            exact_identity,
            &mut direct_guided_allowance,
        )
        .map_err(|failure| anyhow!("uninterrupted guided preparation: {failure:?}"))?;
        assert_eq!(
            resumed_guided.prepared_identity(),
            direct_guided.prepared_identity()
        );
        assert_eq!(
            task.allowance.usage().categories,
            direct_guided_allowance.usage().categories
        );
        task.direct = Some(DirectPager::Route(resumed_guided));
        task.release(&source);
        direct_guided.release(&mut direct_guided_allowance);
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }

    #[test]
    fn tree_admission_retries_the_same_native_route_id_after_memory_release() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
        let panel: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-search/panel.json"
        )))?;
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value::<Limits>(panel["limits"].clone())?,
        )
        .map_err(anyhow::Error::new)?;
        for command in case["prefix"].as_array().expect("prefix commands") {
            let step = session.step(serde_json::from_value::<Command>(command.clone())?);
            assert!(step.accepted, "{step:?}");
        }
        let search: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/benchmarks/hierarchical-search/config.json"
        )))?;
        let config: aic_pilot_native::constructive::routing_candidates::Config =
            serde_json::from_value(search["candidate"].clone())?;
        let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut allowance = RequestAllowance::with_memory_budget(
            config.request_allowance.clone(),
            Some(budget.clone()),
        );
        let request = Request {
            state_revision: session.revision(),
            connection: "root".into(),
        };
        let guidance = Guidance {
            schema: aic_pilot_native::constructive::routing_candidates::GUIDANCE_SCHEMA.into(),
            state_revision: request.state_revision,
            connection: request.connection.clone(),
            model_version: "test-zero-cost".into(),
            heads: vec![
                aic_pilot_native::constructive::routing_candidates::GuidanceHead {
                    id: "same-route".into(),
                    directed_costs: Vec::new(),
                    predicted_success: 0.5,
                    uncertainty: 0.5,
                },
            ],
        };
        let identity = vec![1, 2, 3];
        let mut pager = RouteCandidatePager::new(
            &session,
            request.clone(),
            config.clone(),
            Some(guidance),
            identity.clone(),
            &mut allowance,
        )
        .map_err(|error| anyhow!("{error:?}"))?;
        let mut candidate_id = None;
        for _ in 0..8 {
            let page = pager
                .next_page(
                    &session,
                    &identity,
                    &mut allowance,
                    1,
                    100_000,
                    1_000_000,
                    Instant::now() + Duration::from_secs(10),
                )
                .map_err(|error| anyhow!("{error:?}"))?;
            candidate_id = page.candidate_ids.into_iter().next();
            if candidate_id.is_some() {
                break;
            }
            assert!(matches!(page.status, RoutePageStatus::Pending));
        }
        let candidate_id = candidate_id.expect("native route candidate");
        let candidate_bytes = route_bytes(&pager.candidates()[candidate_id]);
        let mut task = Task {
            kind: TaskKind::Route(request),
            allowance,
            memory: budget.clone(),
            published: Vec::new(),
            route_guard_index: BTreeMap::new(),
            updated_routes: Vec::new(),
            pending_native: VecDeque::from([PendingNative::Route {
                id: candidate_id,
                update: false,
            }]),
            pending_retry_live: None,
            guidance_preparation: None,
            prepared_guidance_input: None,
            route_open: None,
            route_preparation: None,
            route_output_bytes: 0,
            guidance_pending: false,
            direct: Some(DirectPager::Route(pager)),
            repair: None,
            scope_prepared: true,
            scope_sizes: None,
            scope_retry_live: None,
            direct_done: false,
            repair_done: true,
            first_direct: true,
            first_repair: true,
            first_quotas: [0; CUMULATIVE_WORK_KINDS.len()],
            first_duration: Duration::ZERO,
            pages: 1,
            published_actions: 0,
            repair_published: false,
        };
        let pressure_bytes = 1024 * 1024 * 1024 - budget.live_bytes() - candidate_bytes + 1;
        let pressure = TreeReservation::new(&budget, pressure_bytes)
            .expect("pressure fits before the extra tree copy");
        let (first, stop) = task.flush_pending();
        assert!(first.is_empty());
        let stop = stop.expect("tree admission reports aggregate memory pressure");
        assert_eq!(task.pending_native.len(), 1);
        assert!(!task.complete());
        let prior_done = (task.direct_done, task.repair_done);
        task.stop(session.revision(), &mut Counters::default(), stop);
        task.release_done_pagers(&session);
        assert!(matches!(task.direct, Some(DirectPager::Route(_))));
        drop(pressure);
        let (second, stop) = task.flush_pending();
        assert!(stop.is_none());
        assert!(matches!(second.as_slice(), [MacroAction::Route(_)]));
        // Resume the existing fixture's later guided-provenance check. A real
        // hard-stopped task would release its pager after this admission.
        (task.direct_done, task.repair_done) = prior_done;
        assert!(task.pending_native.is_empty());
        assert!(!task.complete());
        assert!(task.flush_pending().0.is_empty());
        let original_guard_bytes = task.published[task.route_guard_index[&candidate_id]].bytes();
        let mut saw_update = false;
        for _ in 0..16 {
            let Some(DirectPager::Route(pager)) = task.direct.as_mut() else {
                unreachable!("retained native pager")
            };
            let page = pager
                .next_page(
                    &session,
                    &identity,
                    &mut task.allowance,
                    1,
                    100_000,
                    1_000_000,
                    Instant::now() + Duration::from_secs(10),
                )
                .map_err(|error| anyhow!("{error:?}"))?;
            if page.updated_candidate_ids.contains(&candidate_id) {
                task.pending_native.push_back(PendingNative::Route {
                    id: candidate_id,
                    update: true,
                });
                saw_update = true;
                break;
            }
            if matches!(page.status, RoutePageStatus::Complete(_)) {
                break;
            }
        }
        assert!(
            saw_update,
            "zero-cost guided phase should rediscover the anchor route"
        );
        let (new_actions, stop) = task.flush_pending();
        assert!(new_actions.is_empty());
        assert!(stop.is_none());
        assert_eq!(task.updated_routes.len(), 1);
        assert_eq!(task.updated_routes[0].id, candidate_id);
        assert!(!task.updated_routes[0].additional_provenance.is_empty());
        let Some(DirectPager::Route(pager)) = task.direct.as_ref() else {
            unreachable!("retained native pager")
        };
        assert_eq!(
            task.published[task.route_guard_index[&candidate_id]].bytes(),
            route_bytes(&pager.candidates()[candidate_id]),
        );
        assert!(
            task.published[task.route_guard_index[&candidate_id]].bytes()
                < original_guard_bytes.saturating_mul(2),
        );

        let model_config = crate::schema::RouteGuidanceModelConfig {
            schema: crate::schema::RouteGuidanceModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            raster_width: 8,
            heads: 1,
            maximum_edge_cost: 32,
        };
        let (order, state) =
            crate::hierarchical_search::GuidanceModel::initial_state(&model_config, 17)?;
        let model = crate::hierarchical_search::GuidanceModel::load(
            &model_config,
            &order,
            &state,
            crate::hierarchical_search::Device::Cpu,
        )?;
        let factory = super::Factory::new(
            &model,
            "test",
            crate::hierarchical_search::Device::Cpu,
            crate::hierarchical_search::DomainConfig {
                placement: config.bounds,
                extension_lengths: vec![1, 2, 4],
            },
            &config,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let action = second.into_iter().next().expect("admitted anchor route");
        let identity = crate::hierarchical_search::physical_action_identity(&action)?;
        let original_identity = identity.clone();
        let edge_memory = TreeReservation::new(
            &budget,
            session
                .clone_reservation_bytes()
                .saturating_add(std::mem::size_of::<crate::hierarchical_search::Node>())
                .saturating_add(std::mem::size_of::<crate::hierarchical_search::Edge>()),
        )
        .expect("expanded node fits");
        let mut nodes = vec![crate::hierarchical_search::Node {
            session: Arc::new(session.clone()),
            memory: edge_memory,
            visits: 7,
            value: None,
            actions: Some(vec![crate::hierarchical_search::Edge {
                action,
                identity,
                features: [0.0; crate::hierarchical_search::ACTION_FEATURES],
                entity_index: 0,
                anchor_score: 0.0,
                prior: 1.0,
                anchor_prior: 1.0,
                visits: 3,
                value_sum: 1.25,
                child: None,
                blocked_value: None,
            }]),
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        let update = task.updated_routes.remove(0);
        crate::hierarchical_search::append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut super::Counters::default(),
            &budget,
            Vec::new(),
            vec![update],
        )?;
        let edges = nodes[0].actions.as_ref().expect("expanded actions");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].identity, original_identity);
        assert_eq!(edges[0].visits, 3);
        assert_eq!(edges[0].value_sum, 1.25);
        assert_eq!(edges[0].features[13], 1.0);
        assert_eq!(edges[0].features[14], 1.0);
        let MacroAction::Route(updated) = &edges[0].action else {
            unreachable!("route edge")
        };
        assert!(!updated.additional_provenance.is_empty());
        let record_action = edges[0].action.clone();
        let MacroAction::Route(record_candidate) = record_action else {
            unreachable!("record action")
        };
        assert_eq!(
            record_candidate.additional_provenance,
            updated.additional_provenance
        );
        drop(nodes);
        task.release_done_pagers(&session);
        task.release(&session);
        drop(task);
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }
}
