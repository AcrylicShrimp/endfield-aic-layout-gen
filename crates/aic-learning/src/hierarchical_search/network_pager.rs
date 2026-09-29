//! Retained, bounded progress for one same-commodity network macro task.

use std::{collections::BTreeSet, time::Instant};

use aic_layout::logistics::TransportKind;
use aic_pilot_native::constructive::{
    Session,
    routing_candidates::{
        Candidate, Guidance, NetworkCandidate, Request, RequestAllowance, RequestFailure,
        RouteCandidatePager, RoutePageStatus, WorkKind, network_shared_cells_with_allowance,
    },
};
use anyhow::{Result, anyhow};

use super::{
    Counters, Factory, NetworkWitnessKey, inference_reservation_bytes,
    json_encoded_len_with_allowance, network_witness_key_bounded,
    network_witness_key_reservation_bytes, record_request_stop, repair_pager::route_limit_stop,
    session_network_identity,
};

pub(super) enum NetworkPageStatus {
    Pending,
    Complete,
    Unknown(aic_pilot_native::constructive::routing_candidates::AllowanceStop),
}

pub(super) struct NetworkPage {
    pub(super) candidate_ids: Vec<usize>,
    pub(super) status: NetworkPageStatus,
    pub(super) expanded: usize,
    pub(super) scans: usize,
}

struct Partial {
    variant: usize,
    order: Vec<String>,
    session: Session,
    routes: Vec<Candidate>,
    depth: usize,
    route_pager: Option<RouteCandidatePager>,
    retained_bytes: usize,
    replay_copy_bytes: usize,
}

impl Partial {
    fn release(mut self, allowance: &mut RequestAllowance, clone_bytes: usize) {
        if let Some(pager) = self.route_pager.take() {
            pager.release(allowance);
        }
        allowance.release_peak(clone_bytes.saturating_add(self.retained_bytes));
    }
}

/// The caller owns the immutable source Session and the one shared task
/// allowance. Only one bounded variant batch lives here at a time; a scheduled
/// yield retains its sessions, member index and native A* frontiers.
pub(super) struct NetworkPager {
    connections: Vec<String>,
    source_revision: u64,
    source_identity: String,
    commodity: String,
    transport: TransportKind,
    next_variant: usize,
    partials: Vec<Partial>,
    semantic: BTreeSet<NetworkWitnessKey>,
    semantic_bytes: usize,
    results: Vec<NetworkCandidate>,
    result_bytes: usize,
    done: bool,
}

impl NetworkPager {
    pub(super) fn new(
        source: &Session,
        source_identity: &str,
        connections: Vec<String>,
        allowance: &mut RequestAllowance,
    ) -> Result<Self> {
        let (commodity, transport) = session_network_identity(source, &connections, allowance)
            .map_err(|failure| anyhow!("network identity: {failure:?}"))?;
        Ok(Self {
            connections,
            source_revision: source.revision(),
            source_identity: source_identity.to_owned(),
            commodity,
            transport,
            next_variant: 0,
            partials: Vec::new(),
            semantic: BTreeSet::new(),
            semantic_bytes: 0,
            results: Vec::new(),
            result_bytes: 0,
            done: false,
        })
    }

    pub(super) fn candidates(&self) -> &[NetworkCandidate] {
        &self.results
    }

    pub(super) fn blocked_by_held_variant(
        &self,
        max_variants: usize,
        remaining_allowance_variants: usize,
        held_variants: usize,
    ) -> bool {
        self.partials.is_empty()
            && held_variants > 0
            && (self.next_variant >= max_variants.saturating_sub(held_variants)
                || remaining_allowance_variants <= held_variants)
    }

    fn request(&self, source: &Session) -> Request {
        Request {
            state_revision: source.revision(),
            connection: self.connections[0].clone(),
        }
    }

    fn member_identity(&self, partial: &Partial) -> Vec<u8> {
        let mut bytes = self.source_identity.as_bytes().to_vec();
        bytes.extend_from_slice(&(partial.variant as u64).to_le_bytes());
        bytes.extend_from_slice(&(partial.depth as u64).to_le_bytes());
        bytes.extend_from_slice(&partial.session.revision().to_le_bytes());
        bytes
    }

    fn start_batch(
        &mut self,
        source: &Session,
        factory: &Factory<'_>,
        allowance: &mut RequestAllowance,
        held_variants: usize,
    ) -> std::result::Result<(), aic_pilot_native::constructive::routing_candidates::AllowanceStop>
    {
        let remaining = factory
            .candidate
            .max_candidates
            .saturating_sub(held_variants)
            .saturating_sub(self.next_variant);
        let count = remaining
            .min(factory.candidate.request_allowance.max_inference_batch)
            .min(
                allowance
                    .remaining(WorkKind::Variants)
                    .saturating_sub(held_variants),
            )
            .min(if self.next_variant == 0 {
                1
            } else {
                usize::MAX
            });
        if count == 0 {
            return Err(allowance.exhausted_stop(WorkKind::Variants));
        }
        let clone_bytes = source.clone_reservation_bytes();
        let bytes = clone_bytes.saturating_mul(count);
        allowance.reserve_peak(bytes)?;
        if let Err(stop) = allowance
            .charge(WorkKind::Variants, count)
            .and_then(|_| allowance.charge(WorkKind::CopiedBytes, bytes))
        {
            allowance.release_peak(bytes);
            return Err(stop);
        }
        for variant in self.next_variant..self.next_variant + count {
            let mut order = self.connections.clone();
            let rotation = variant % order.len();
            order.rotate_left(rotation);
            if (variant / order.len()) % 2 == 1 {
                order.reverse();
            }
            self.partials.push(Partial {
                variant,
                order,
                session: source.clone(),
                routes: Vec::with_capacity(self.connections.len()),
                depth: 0,
                route_pager: None,
                retained_bytes: 0,
                replay_copy_bytes: 0,
            });
        }
        self.next_variant += count;
        Ok(())
    }

    /// Batch the missing guidance inputs across live variants before advancing
    /// any one member. A failed model call uses the retained anchor search.
    fn prepare_members(
        &mut self,
        factory: &Factory<'_>,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
    ) -> Result<Option<aic_pilot_native::constructive::routing_candidates::AllowanceStop>> {
        // Establish and search the unguided baseline before optional guidance
        // preparation can spend the same task allowance.
        if let Some(anchor) = self
            .partials
            .iter_mut()
            .find(|partial| partial.variant == 0)
        {
            if anchor.depth < self.connections.len() && anchor.route_pager.is_none() {
                let request = Request {
                    state_revision: anchor.session.revision(),
                    connection: anchor.order[anchor.depth].clone(),
                };
                let mut identity = self.source_identity.as_bytes().to_vec();
                identity.extend_from_slice(&0_u64.to_le_bytes());
                identity.extend_from_slice(&(anchor.depth as u64).to_le_bytes());
                identity.extend_from_slice(&anchor.session.revision().to_le_bytes());
                match RouteCandidatePager::new_unpublished(
                    &anchor.session,
                    request,
                    factory.candidate.clone(),
                    None,
                    identity,
                    allowance,
                ) {
                    Ok(pager) => anchor.route_pager = Some(pager),
                    Err(RequestFailure::Stopped(stop)) => return Ok(Some(stop)),
                    Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                        anchor.depth = self.connections.len() + 1;
                    }
                    Err(failure) => return Err(anyhow!("network anchor preparation: {failure:?}")),
                }
            }
            if anchor.depth <= self.connections.len() {
                return Ok(None);
            }
        }
        let mut prepared = Vec::new();
        for (index, partial) in self.partials.iter().enumerate() {
            if partial.depth >= self.connections.len() || partial.route_pager.is_some() {
                continue;
            }
            if partial.variant == 0 {
                continue;
            }
            let request = Request {
                state_revision: partial.session.revision(),
                connection: partial.order[partial.depth].clone(),
            };
            match partial.session.route_guidance_input_with_allowance(
                &request,
                factory.candidate,
                &factory.domain,
                allowance,
            ) {
                Ok(input) => prepared.push((index, input)),
                Err(RequestFailure::Invalid(diagnostic))
                    if diagnostic.code.starts_with("constructive.route-guidance") =>
                {
                    counters.repair_guidance_fallbacks += 1;
                }
                Err(RequestFailure::Stopped(stop)) => {
                    for (_, input) in prepared {
                        allowance.release_peak(input.reserved_bytes);
                    }
                    return Ok(Some(stop));
                }
                Err(failure) => return Err(anyhow!("network guidance input: {failure:?}")),
            }
        }
        let mut guidance_by_index = vec![None; self.partials.len()];
        let mut inference_held = 0usize;
        if !prepared.is_empty() {
            let inference_bytes = prepared.iter().fold(0usize, |total, (_, input)| {
                total.saturating_add(inference_reservation_bytes(input, factory.guidance_heads))
            });
            let elements = prepared.iter().fold(0usize, |total, (_, input)| {
                total
                    .saturating_add(input.grid.len())
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
                    )
            });
            let charged = allowance
                .charge(WorkKind::InferenceElements, elements)
                .and_then(|_| allowance.reserve_peak(inference_bytes));
            match charged {
                Ok(()) => {
                    inference_held = inference_bytes;
                    let inputs = prepared.iter().map(|(_, input)| input).collect::<Vec<_>>();
                    let predicted = factory.guidance.predict_native(
                        &inputs,
                        factory.guidance_version,
                        factory.device,
                    );
                    match predicted {
                        Ok(guides) => {
                            counters.guidance_batches += 1;
                            counters.guidance_inputs += inputs.len();
                            for ((index, _), guide) in prepared.iter().zip(guides) {
                                guidance_by_index[*index] = Some(guide);
                            }
                        }
                        Err(_) => {
                            counters.repair_guidance_fallbacks += inputs.len();
                            allowance.release_peak(inference_held);
                            inference_held = 0;
                        }
                    }
                }
                Err(stop) => {
                    for (_, input) in prepared {
                        allowance.release_peak(input.reserved_bytes);
                    }
                    return Ok(Some(stop));
                }
            }
        }
        for (_, input) in prepared {
            allowance.release_peak(input.reserved_bytes);
        }
        for index in 0..self.partials.len() {
            let partial = &self.partials[index];
            if partial.depth >= self.connections.len() || partial.route_pager.is_some() {
                continue;
            }
            let request = Request {
                state_revision: partial.session.revision(),
                connection: partial.order[partial.depth].clone(),
            };
            let identity = self.member_identity(partial);
            let guide: Option<Guidance> = guidance_by_index[index].take();
            match RouteCandidatePager::new_unpublished(
                &partial.session,
                request,
                factory.candidate.clone(),
                guide,
                identity,
                allowance,
            ) {
                Ok(pager) => self.partials[index].route_pager = Some(pager),
                Err(RequestFailure::Stopped(stop)) => {
                    allowance.release_peak(inference_held);
                    return Ok(Some(stop));
                }
                Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                    self.partials[index].depth = self.connections.len() + 1;
                }
                Err(failure) => {
                    allowance.release_peak(inference_held);
                    return Err(anyhow!("network member preparation: {failure:?}"));
                }
            }
        }
        allowance.release_peak(inference_held);
        Ok(None)
    }

    fn finish_member(
        &mut self,
        index: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<Option<aic_pilot_native::constructive::routing_candidates::AllowanceStop>> {
        let clone_bytes = self.partials[index].session.clone_reservation_bytes();
        let partial = &mut self.partials[index];
        let pager = partial.route_pager.take().expect("member pager exists");
        if pager.candidates().is_empty() {
            pager.release(allowance);
            partial.depth = self.connections.len() + 1;
            return Ok(None);
        }
        let rank = if partial.variant == 0 {
            0
        } else {
            1 + (partial.variant + partial.depth - 1)
                % pager.candidates().len().saturating_sub(1).max(1)
        }
        .min(pager.candidates().len() - 1);
        let selected = &pager.candidates()[rank];
        let growth_bytes = selected
            .cells
            .len()
            .saturating_mul(4096)
            .saturating_add(selected.crossings.len().saturating_mul(4096))
            .saturating_add(selected.commands.len().saturating_mul(256))
            .saturating_add(4096);
        if let Err(stop) = allowance.reserve_peak(growth_bytes) {
            pager.release(allowance);
            return Ok(Some(stop));
        }
        let mut candidate = selected.clone();
        pager.release(allowance);
        let replay = partial
            .session
            .apply_unpublished_route_candidate_with_cost_allowance(&candidate, allowance);
        match replay {
            Ok((_, physical_cost)) => candidate.physical_cost = physical_cost,
            Err(RequestFailure::Stopped(stop)) => {
                allowance.release_peak(growth_bytes);
                return Ok(Some(stop));
            }
            Err(failure) => {
                allowance.release_peak(growth_bytes);
                return Err(anyhow!("network member replay: {failure:?}"));
            }
        }
        let retained = partial
            .session
            .clone_reservation_bytes()
            .saturating_sub(clone_bytes)
            .saturating_add(candidate.cells.len().saturating_mul(128))
            .saturating_add(candidate.commands.len().saturating_mul(256))
            .saturating_add(candidate.crossings.len().saturating_mul(512))
            .saturating_add(2048);
        if retained > growth_bytes {
            if let Err(stop) = allowance.reserve_peak(retained - growth_bytes) {
                allowance.release_peak(growth_bytes);
                return Ok(Some(stop));
            }
        } else {
            allowance.release_peak(growth_bytes - retained);
        }
        partial.retained_bytes = partial.retained_bytes.saturating_add(retained);
        partial.replay_copy_bytes = partial.replay_copy_bytes.saturating_add(clone_bytes);
        partial.routes.push(candidate);
        partial.depth += 1;
        Ok(None)
    }

    fn finish_variant(
        &mut self,
        index: usize,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<Option<aic_pilot_native::constructive::routing_candidates::AllowanceStop>> {
        let partial = &self.partials[index];
        let key_bytes = network_witness_key_reservation_bytes(&partial.routes);
        if let Err(stop) = allowance.reserve_peak(key_bytes) {
            return Ok(Some(stop));
        }
        let key = match network_witness_key_bounded(&partial.routes, allowance) {
            Ok(key) => key,
            Err(stop) => {
                allowance.release_peak(key_bytes);
                return Ok(Some(stop));
            }
        };
        if self.semantic.contains(&key) {
            allowance.release_peak(key_bytes);
            return Ok(None);
        }
        let shared_cells = match network_shared_cells_with_allowance(&partial.routes, allowance) {
            Ok(count) => count,
            Err(stop) => {
                allowance.release_peak(key_bytes);
                return Ok(Some(stop));
            }
        };
        let candidate_bytes = partial.routes.iter().fold(8192usize, |total, route| {
            total
                .saturating_add(route.cells.len().saturating_mul(384))
                .saturating_add(route.commands.len().saturating_mul(256))
                .saturating_add(route.crossings.len().saturating_mul(512))
        });
        if let Err(stop) = allowance.reserve_peak(candidate_bytes) {
            allowance.release_peak(key_bytes);
            return Ok(Some(stop));
        }
        let physical_cost = match source.physical_cost_between(&partial.session, &self.connections)
        {
            Ok(cost) => cost,
            Err(failure) => {
                allowance.release_peak(key_bytes + candidate_bytes);
                return Err(anyhow::Error::new(failure));
            }
        };
        let candidate = NetworkCandidate {
            id: self.results.len(),
            state_revision: source.revision(),
            commodity: self.commodity.clone(),
            transport: self.transport,
            steps: partial.routes.iter().map(|route| route.steps).sum(),
            turns: partial.routes.iter().map(|route| route.turns).sum(),
            expanded: partial.routes.iter().map(|route| route.expanded).sum(),
            shared_cells,
            routes: partial.routes.clone(),
            physical_cost,
            charged_work: allowance.usage(),
        };
        let clone_bytes = source.clone_reservation_bytes();
        if let Err(stop) = allowance.reserve_peak(clone_bytes) {
            allowance.release_peak(key_bytes + candidate_bytes);
            return Ok(Some(stop));
        }
        let mut replay = source.clone();
        let final_clone_bytes = partial.session.clone_reservation_bytes();
        let final_layout_copy = partial.session.metrics().is_some();
        let replay_copied_bytes = clone_bytes
            .saturating_mul(2)
            .saturating_add(partial.replay_copy_bytes)
            .saturating_add(if final_layout_copy {
                final_clone_bytes
            } else {
                0
            });
        let replay_result = replay.apply_network_candidate_with_allowance(
            &candidate,
            final_clone_bytes,
            replay_copied_bytes,
            final_layout_copy,
            allowance,
        );
        allowance.release_peak(clone_bytes);
        match replay_result {
            Ok(_) => {}
            Err(RequestFailure::Stopped(stop)) => {
                allowance.release_peak(key_bytes + candidate_bytes);
                return Ok(Some(stop));
            }
            Err(failure) => {
                allowance.release_peak(key_bytes + candidate_bytes);
                return Err(anyhow!("network publication replay: {failure:?}"));
            }
        }
        let encoded_bytes = match json_encoded_len_with_allowance(&candidate, allowance) {
            Ok(bytes) => bytes,
            Err(RequestFailure::Stopped(stop)) => {
                allowance.release_peak(key_bytes + candidate_bytes);
                return Ok(Some(stop));
            }
            Err(failure) => {
                allowance.release_peak(key_bytes + candidate_bytes);
                return Err(anyhow!("network publication serialization: {failure:?}"));
            }
        };
        let held_bytes = encoded_bytes.max(candidate_bytes);
        if held_bytes > candidate_bytes {
            if let Err(stop) = allowance.reserve_peak(held_bytes - candidate_bytes) {
                allowance.release_peak(key_bytes + candidate_bytes);
                return Ok(Some(stop));
            }
        }
        if let Err(stop) = allowance.charge(WorkKind::Candidates, 1) {
            allowance.release_peak(held_bytes + key_bytes);
            return Ok(Some(stop));
        }
        self.semantic.insert(key);
        self.semantic_bytes += key_bytes;
        self.result_bytes += held_bytes;
        self.results.push(candidate);
        Ok(None)
    }

    pub(super) fn next_page(
        &mut self,
        source: &Session,
        factory: &Factory<'_>,
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
        max_new_candidates: usize,
        held_variants: usize,
        max_new_expansions: usize,
        max_new_scans: usize,
        page_deadline: Instant,
    ) -> Result<NetworkPage> {
        if source.revision() != self.source_revision {
            return Err(anyhow!("network pager source revision changed"));
        }
        let mut ids = Vec::new();
        let mut expanded = 0usize;
        let mut scans = 0usize;
        let clone_bytes = source.clone_reservation_bytes();
        let status = 'pages: loop {
            if self.done {
                break NetworkPageStatus::Complete;
            }
            if ids.len() >= max_new_candidates
                || expanded >= max_new_expansions
                || scans >= max_new_scans
                || Instant::now() >= page_deadline
            {
                break NetworkPageStatus::Pending;
            }
            if self.partials.is_empty() {
                if self.next_variant >= factory.candidate.max_candidates {
                    self.done = true;
                    break NetworkPageStatus::Complete;
                }
                if held_variants > 0
                    && (self.next_variant
                        >= factory
                            .candidate
                            .max_candidates
                            .saturating_sub(held_variants)
                        || allowance.remaining(WorkKind::Variants) <= held_variants)
                {
                    break NetworkPageStatus::Pending;
                }
                if let Err(stop) = self.start_batch(source, factory, allowance, held_variants) {
                    break NetworkPageStatus::Unknown(stop);
                }
            }
            if let Some(stop) = self.prepare_members(factory, counters, allowance)? {
                break NetworkPageStatus::Unknown(stop);
            }
            let mut progress = false;
            let mut remove = Vec::new();
            let mut page_limit_reached = false;
            for index in 0..self.partials.len() {
                if ids.len() >= max_new_candidates
                    || expanded >= max_new_expansions
                    || scans >= max_new_scans
                    || Instant::now() >= page_deadline
                {
                    page_limit_reached = true;
                    break;
                }
                if self.partials[index].depth > self.connections.len() {
                    remove.push(index);
                    continue;
                }
                if self.partials[index].depth == self.connections.len() {
                    let before_results = self.results.len();
                    if let Some(stop) = self.finish_variant(index, source, allowance)? {
                        break 'pages NetworkPageStatus::Unknown(stop);
                    }
                    if self.results.len() > before_results {
                        ids.push(self.results.len() - 1);
                    }
                    remove.push(index);
                    progress = true;
                    continue;
                }
                if self.partials[index].route_pager.is_none() {
                    continue;
                }
                let identity = self.member_identity(&self.partials[index]);
                let partial = &mut self.partials[index];
                let pager = partial.route_pager.as_mut().expect("member pager prepared");
                let page = pager
                    .next_page(
                        &partial.session,
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
                    RoutePageStatus::Pending => {}
                    RoutePageStatus::Complete(termination) => {
                        if pager.candidates().is_empty()
                            && matches!(
                                termination,
                                aic_pilot_native::constructive::routing_candidates::Termination::WorkLimit
                                    | aic_pilot_native::constructive::routing_candidates::Termination::Deadline
                                    | aic_pilot_native::constructive::routing_candidates::Termination::ResourceLimit
                                    | aic_pilot_native::constructive::routing_candidates::Termination::SearchLimit
                                    | aic_pilot_native::constructive::routing_candidates::Termination::WorkBudget
                            )
                        {
                            break 'pages NetworkPageStatus::Unknown(route_limit_stop(
                                termination,
                                pager,
                                factory,
                                allowance,
                            ));
                        }
                        if let Some(stop) = self.finish_member(index, allowance)? {
                            break 'pages NetworkPageStatus::Unknown(stop);
                        }
                        progress = true;
                    }
                    RoutePageStatus::Unknown(stop) => {
                        break 'pages NetworkPageStatus::Unknown(stop);
                    }
                }
            }
            for index in remove.into_iter().rev() {
                self.partials.remove(index).release(allowance, clone_bytes);
            }
            if page_limit_reached {
                break NetworkPageStatus::Pending;
            }
            if !progress {
                break NetworkPageStatus::Pending;
            }
        };
        if let NetworkPageStatus::Unknown(stop) = &status {
            record_request_stop(counters, &self.request(source), allowance, stop.clone());
            for partial in self.partials.drain(..) {
                partial.release(allowance, clone_bytes);
            }
            self.done = true;
        }
        Ok(NetworkPage {
            candidate_ids: ids,
            status,
            expanded,
            scans,
        })
    }

    pub(super) fn release(mut self, source: &Session, allowance: &mut RequestAllowance) {
        let clone_bytes = source.clone_reservation_bytes();
        for partial in self.partials.drain(..) {
            partial.release(allowance, clone_bytes);
        }
        allowance.release_peak(self.semantic_bytes + self.result_bytes);
    }
}
