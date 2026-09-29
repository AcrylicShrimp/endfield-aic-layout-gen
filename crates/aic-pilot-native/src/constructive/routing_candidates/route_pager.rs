//! Retained route-candidate work for one immutable stable-state request.

use std::{collections::BTreeMap, sync::Arc, time::Instant};

use aic_layout::geometry::Point;
use aic_routing::{
    Crossings, PathResult,
    prepared::{Advance, Frontier, PreflightStatus, PreparedGraph, Reservation},
};
use indexmap::IndexMap;

use super::{
    AllowanceStop, BlockedCellsCursor, Candidate, Config, EndpointPair, EndpointPairCursor,
    Guidance, GuidanceValidationCursor, Provenance, Request, RequestAllowance, RequestFailure,
    Session, Termination, WorkKind, retained_endpoint_pair_bytes, route_preparation,
};
use route_preparation::RouteCrossingCursor;

pub enum RoutePageStatus {
    Pending,
    Complete(Termination),
    Unknown(AllowanceStop),
}

pub struct RouteCandidatePage {
    pub candidate_ids: Vec<usize>,
    /// Already published witnesses whose provenance gained a new search mode.
    pub updated_candidate_ids: Vec<usize>,
    pub status: RoutePageStatus,
    pub searches: usize,
    pub expanded: usize,
    pub scans: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

/// A completed search and its still-live path reservation. Publication owns
/// this value until it accepts, rejects, or cancels the path.
pub struct FoundPathLease {
    pair_index: usize,
    result: Option<PathResult>,
    guidance_index: Option<usize>,
    reserved_bytes: usize,
}

impl FoundPathLease {
    pub fn path(&self) -> &PathResult {
        self.result.as_ref().expect("path has not been taken")
    }

    /// Publication may move path vectors into a candidate while retaining
    /// this lease as the scheduler and memory ownership token.
    pub fn take_path(&mut self) -> PathResult {
        self.result.take().expect("path has not been taken")
    }

    pub fn release(self, allowance: &mut RequestAllowance) {
        allowance.release_peak(self.reserved_bytes);
    }
}

enum PublishOutcome {
    New(usize),
    Updated(usize),
    None,
}

type SemanticKey = (
    Option<String>,
    Option<String>,
    Vec<Point>,
    Vec<(Point, usize, String, i64)>,
);

fn reserve_kernel_memory(allowance: &mut RequestAllowance, bytes: usize) -> Reservation {
    match allowance.reserve_peak_for_bounded_kernel(bytes) {
        Ok(()) => Reservation::Granted,
        Err(stop)
            if stop.reason == "aggregate-memory-limit"
                && allowance.reserved_bytes.saturating_add(bytes)
                    <= allowance
                        .tree_memory
                        .as_ref()
                        .expect("aggregate stop has tree budget")
                        .limit_bytes() =>
        {
            Reservation::Retry
        }
        Err(_) => Reservation::Denied,
    }
}

/// A kernel has already done all three kinds of work before it returns. Keep
/// every counter even when one category or the deadline reports a stop.
fn record_search_work(
    allowance: &mut RequestAllowance,
    expanded: usize,
    scans: usize,
    frontier_peak: Option<usize>,
) -> Result<(), AllowanceStop> {
    let mut stop = None;
    for result in [
        allowance.record_completed(WorkKind::Expansions, expanded),
        allowance.record_completed(WorkKind::RoutingScans, scans),
        frontier_peak.map_or(Ok(()), |peak| {
            allowance.record_peak(WorkKind::FrontierEntries, peak)
        }),
    ] {
        if let Err(next) = result {
            if stop.as_ref().is_none_or(|previous: &AllowanceStop| {
                previous.reason == "page-quantum" && next.reason != "page-quantum"
            }) {
                stop = Some(next);
            }
        }
    }
    stop.map_or(Ok(()), Err)
}

fn choose_kernel_stop(
    kernel: Option<AllowanceStop>,
    metered: Result<(), AllowanceStop>,
) -> Option<AllowanceStop> {
    let metered = metered.err();
    let soft =
        |stop: &AllowanceStop| matches!(stop.reason, "page-quantum" | "aggregate-memory-limit");
    match (kernel, metered) {
        (Some(kernel), Some(metered)) if soft(&kernel) && !soft(&metered) => Some(metered),
        (Some(kernel), _) => Some(kernel),
        (None, metered) => metered,
    }
}

/// Owns endpoint work and its memory reservation before a route pager exists.
/// A temporary page stop leaves the cursor in this object for the next turn.
pub struct RouteCandidatePreparation {
    request: Request,
    config: Config,
    guidance: Option<Guidance>,
    exact_state_and_capacity_identity: Vec<u8>,
    replay_candidates: bool,
    publish_candidates: bool,
    connection: usize,
    source_address: usize,
    source_instance_id: u64,
    source_turns: u64,
    pair_cursor: Option<EndpointPairCursor>,
    pairs: Option<Vec<EndpointPair>>,
    pair_phase_bytes: usize,
    owner_bytes: usize,
    guidance_cursor: Option<GuidanceValidationCursor>,
    guidance_maps: Option<Vec<Arc<IndexMap<(Point, usize), u32>>>>,
    guidance_map_bytes: usize,
    guidance_reserved: bool,
    blocked_cursor: Option<BlockedCellsCursor>,
    blocked: Option<indexmap::IndexSet<Point>>,
    blocked_sites: usize,
    blocked_bytes: usize,
    blocked_reserved: bool,
    carrier_index: usize,
    carrier_cells: usize,
    carrier_inventory_ready: bool,
    crossing_cursor: Option<RouteCrossingCursor>,
    crossings: Option<Crossings>,
    crossing_bytes: usize,
    crossing_reserved: bool,
}

impl RouteCandidatePreparation {
    fn owner_bytes(request: &Request) -> usize {
        std::mem::size_of::<Self>()
            .max(std::mem::size_of::<RouteCandidatePager>())
            .saturating_add(request.connection.capacity())
            .saturating_add(256)
    }

    pub fn new(
        source: &Session,
        request: Request,
        config: Config,
        guidance: Option<Guidance>,
        exact_state_and_capacity_identity: Vec<u8>,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        let mut guidance = guidance;
        let mut exact_state_and_capacity_identity = exact_state_and_capacity_identity;
        Self::new_inner(
            source,
            request,
            config,
            &mut guidance,
            &mut exact_state_and_capacity_identity,
            allowance,
            true,
            true,
            0,
        )
    }

    /// Leaves the model output and identity with the caller if admitting the
    /// preparation owner reaches a temporary page boundary.
    pub fn new_retained(
        source: &Session,
        request: Request,
        config: Config,
        guidance: &mut Option<Guidance>,
        exact_state_and_capacity_identity: &mut Vec<u8>,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        Self::new_inner(
            source,
            request,
            config,
            guidance,
            exact_state_and_capacity_identity,
            allowance,
            true,
            true,
            0,
        )
    }

    /// Reserve the retained Request owner before copying its connection ID.
    /// The caller keeps its own Request for task diagnostics and repair.
    pub fn new_retained_borrowed(
        source: &Session,
        request: &Request,
        config: &Config,
        guidance: &mut Option<Guidance>,
        exact_state_and_capacity_identity: &mut Vec<u8>,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        let mut owner_bytes = Self::owner_bytes(request);
        allowance.reserve_peak(owner_bytes)?;
        let request = request.clone();
        let actual_owner_bytes = Self::owner_bytes(&request);
        if actual_owner_bytes > owner_bytes {
            if let Err(stop) = allowance.reserve_peak(actual_owner_bytes - owner_bytes) {
                allowance.release_peak(owner_bytes);
                return Err(RequestFailure::Stopped(stop));
            }
        } else {
            allowance.release_peak(owner_bytes - actual_owner_bytes);
        }
        owner_bytes = actual_owner_bytes;
        let result = Self::new_inner(
            source,
            request,
            config.clone(),
            guidance,
            exact_state_and_capacity_identity,
            allowance,
            true,
            true,
            owner_bytes,
        );
        if result.is_err() {
            allowance.release_peak(owner_bytes);
        }
        result
    }

    pub fn new_unpublished(
        source: &Session,
        request: Request,
        config: Config,
        guidance: Option<Guidance>,
        exact_state_and_capacity_identity: Vec<u8>,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        let mut guidance = guidance;
        let mut exact_state_and_capacity_identity = exact_state_and_capacity_identity;
        Self::new_inner(
            source,
            request,
            config,
            &mut guidance,
            &mut exact_state_and_capacity_identity,
            allowance,
            false,
            false,
            0,
        )
    }

    fn new_inner(
        source: &Session,
        request: Request,
        config: Config,
        guidance: &mut Option<Guidance>,
        exact_state_and_capacity_identity: &mut Vec<u8>,
        allowance: &mut RequestAllowance,
        replay_candidates: bool,
        publish_candidates: bool,
        pre_reserved_owner_bytes: usize,
    ) -> Result<Self, RequestFailure> {
        config.validate().map_err(RequestFailure::Invalid)?;
        if request.state_revision != source.revision() {
            return Err(RequestFailure::Invalid(super::error(
                "stale-route-candidate-request",
                &request.connection,
                "routing pager belongs to an earlier constructive revision",
            )));
        }
        if source.active.is_some() {
            return Err(RequestFailure::Invalid(super::error(
                "active-route",
                &request.connection,
                "route candidates require a stable state with no active partial route",
            )));
        }
        let Some(&connection) = source.connections.get(&request.connection) else {
            return Err(RequestFailure::Invalid(super::error(
                "unknown-connection",
                &request.connection,
                "routing pager references an unknown logical connection",
            )));
        };
        if source.routes[connection].is_some() {
            return Err(RequestFailure::Invalid(super::error(
                "completed-connection",
                &request.connection,
                "routing pager references a completed logical connection",
            )));
        }
        let owner_bytes = Self::owner_bytes(&request);
        let pair_phase_bytes = source.endpoint_pair_reservation_bytes(connection);
        if pre_reserved_owner_bytes == 0 {
            allowance.reserve_peak(pair_phase_bytes.saturating_add(owner_bytes))?;
        } else {
            debug_assert_eq!(pre_reserved_owner_bytes, owner_bytes);
            allowance.reserve_peak(pair_phase_bytes)?;
        }
        let guidance_map_bytes = guidance.as_ref().map_or(0, |guidance| {
            guidance.heads.iter().fold(0usize, |total, head| {
                total.saturating_add(head.directed_costs.len().saturating_mul(96))
            })
        });
        let blocked_sites = source
            .bodies
            .len()
            .saturating_add(source.terminal_poses.len())
            .saturating_add(
                source.occupied[super::layer(source.problem.connections[connection].transport)]
                    .len(),
            );
        let has_guidance = guidance.is_some();
        let has_crossings = config.max_crossings_per_route > 0;
        Ok(Self {
            request,
            config,
            guidance: guidance.take(),
            exact_state_and_capacity_identity: std::mem::take(exact_state_and_capacity_identity),
            replay_candidates,
            publish_candidates,
            connection,
            source_address: source as *const Session as usize,
            source_instance_id: source.instance_id.0,
            source_turns: source.turns,
            pair_cursor: Some(EndpointPairCursor::new(source, connection)),
            pairs: None,
            pair_phase_bytes: pair_phase_bytes.saturating_add(owner_bytes),
            owner_bytes,
            guidance_cursor: has_guidance.then(GuidanceValidationCursor::new),
            guidance_maps: (!has_guidance).then(Vec::new),
            guidance_map_bytes,
            guidance_reserved: false,
            blocked_cursor: None,
            blocked: None,
            blocked_sites,
            blocked_bytes: blocked_sites.saturating_mul(192),
            blocked_reserved: false,
            carrier_index: 0,
            carrier_cells: 0,
            carrier_inventory_ready: false,
            crossing_cursor: None,
            crossings: (!has_crossings).then(Crossings::new),
            crossing_bytes: 0,
            crossing_reserved: false,
        })
    }

    fn matches_source(&self, source: &Session) -> bool {
        source as *const Session as usize == self.source_address
            && source.instance_id.0 == self.source_instance_id
            && source.revision() == self.request.state_revision
            && source.turns == self.source_turns
    }

    pub fn advance_pairs(
        &mut self,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        if !self.matches_source(source) {
            return Err(RequestFailure::Invalid(super::error(
                "stale-route-candidate-request",
                &self.request.connection,
                "routing preparation source changed",
            )));
        }
        if let Some(cursor) = self.pair_cursor.as_mut() {
            self.pairs =
                Some(cursor.advance(source, self.connection, self.config.bounds, allowance)?);
            self.pair_cursor = None;
        }
        Ok(())
    }

    pub fn advance_guidance(
        &mut self,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        if self.pairs.is_none() {
            return Err(RequestFailure::Invalid(super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "endpoint preparation must finish before guidance validation",
            )));
        }
        let (Some(guidance), Some(cursor)) =
            (self.guidance.as_ref(), self.guidance_cursor.as_mut())
        else {
            return Ok(());
        };
        if !self.guidance_reserved {
            allowance.reserve_peak(self.guidance_map_bytes)?;
            self.guidance_reserved = true;
        }
        cursor.advance(guidance, &self.request, self.config.bounds, allowance)?;
        self.guidance_maps = Some(
            self.guidance_cursor
                .take()
                .expect("completed guidance cursor")
                .take_maps()
                .into_iter()
                .map(Arc::new)
                .collect(),
        );
        Ok(())
    }

    pub fn advance_carrier_inventory(
        &mut self,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        if self.guidance_maps.is_none() {
            return Err(RequestFailure::Invalid(super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "guidance preparation must finish before carrier inventory",
            )));
        }
        if !self.matches_source(source) {
            return Err(RequestFailure::Invalid(super::error(
                "stale-route-candidate-request",
                &self.request.connection,
                "routing preparation source changed",
            )));
        }
        if self.carrier_inventory_ready {
            return Ok(());
        }
        while self.carrier_index < source.routes.len() {
            if let Some(route) = source.routes[self.carrier_index].as_ref() {
                allowance.charge(WorkKind::ValidationRecords, 1)?;
                self.carrier_cells = self.carrier_cells.saturating_add(route.cells.len());
            }
            self.carrier_index += 1;
        }
        if self.config.max_crossings_per_route > 0 {
            self.crossing_bytes = self
                .carrier_cells
                .saturating_mul(640)
                .saturating_add(self.blocked_sites.saturating_mul(64))
                .saturating_add(source.routes.len().saturating_mul(128))
                .saturating_add(source.bridges.len().saturating_mul(512))
                .saturating_add(
                    source
                        .catalogs
                        .components
                        .catalog()
                        .components
                        .len()
                        .saturating_mul(256),
                );
        }
        self.carrier_inventory_ready = true;
        Ok(())
    }

    pub fn advance_blocked(
        &mut self,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        if !self.carrier_inventory_ready {
            return Err(RequestFailure::Invalid(super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "carrier inventory must finish before blocked cells",
            )));
        }
        if !self.matches_source(source) {
            return Err(RequestFailure::Invalid(super::error(
                "stale-route-candidate-request",
                &self.request.connection,
                "routing preparation source changed",
            )));
        }
        if self.blocked.is_some() {
            return Ok(());
        }
        if !self.blocked_reserved {
            allowance.reserve_peak(self.blocked_bytes)?;
            self.blocked_reserved = true;
        }
        let cursor = self
            .blocked_cursor
            .get_or_insert_with(|| BlockedCellsCursor::new(source, self.connection));
        self.blocked =
            Some(cursor.advance(source, self.connection, self.config.bounds, allowance)?);
        self.blocked_cursor = None;
        Ok(())
    }

    pub fn advance_crossings(
        &mut self,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        if self.blocked.is_none() {
            return Err(RequestFailure::Invalid(super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "blocked-cell preparation must finish before crossings",
            )));
        }
        if !self.matches_source(source) {
            return Err(RequestFailure::Invalid(super::error(
                "stale-route-candidate-request",
                &self.request.connection,
                "routing preparation source changed",
            )));
        }
        if self.crossings.is_some() {
            return Ok(());
        }
        if !self.crossing_reserved {
            allowance.reserve_peak(self.crossing_bytes)?;
            self.crossing_reserved = true;
        }
        let cursor = self
            .crossing_cursor
            .get_or_insert_with(|| RouteCrossingCursor::new(source, &self.config));
        if cursor.advance(source, self.connection, &self.config, allowance)? {
            self.crossings = Some(
                self.crossing_cursor
                    .take()
                    .expect("completed crossing cursor")
                    .finish(),
            );
        }
        Ok(())
    }

    pub fn finish(
        self,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<RouteCandidatePager, RequestFailure> {
        match self.finish_retained(source, allowance) {
            Ok(pager) => Ok(pager),
            Err((preparation, failure)) => {
                preparation.release(allowance);
                Err(failure)
            }
        }
    }

    /// Keep every prepared stage and its reservation if the final transfer
    /// reaches a temporary page boundary before ownership moves to the pager.
    pub fn finish_retained(
        mut self,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<RouteCandidatePager, (Self, RequestFailure)> {
        if !self.matches_source(source) {
            let diagnostic = super::error(
                "stale-route-candidate-request",
                &self.request.connection,
                "routing preparation source changed",
            );
            return Err((self, RequestFailure::Invalid(diagnostic)));
        }
        if self.pairs.is_none() {
            let diagnostic = super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "endpoint preparation must finish before constructing the route pager",
            );
            return Err((self, RequestFailure::Invalid(diagnostic)));
        }
        if self.guidance_maps.is_none() {
            let diagnostic = super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "guidance preparation must finish before constructing the route pager",
            );
            return Err((self, RequestFailure::Invalid(diagnostic)));
        }
        if self.blocked.is_none() {
            let diagnostic = super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "blocked-cell preparation must finish before constructing the route pager",
            );
            return Err((self, RequestFailure::Invalid(diagnostic)));
        }
        if self.crossings.is_none() {
            let diagnostic = super::error(
                "incomplete-route-candidate-preparation",
                &self.request.connection,
                "crossing preparation must finish before constructing the route pager",
            );
            return Err((self, RequestFailure::Invalid(diagnostic)));
        }
        if let Err(stop) = allowance.check_deadline() {
            return Err((self, RequestFailure::Stopped(stop)));
        }
        let persistent_bytes = self
            .owner_bytes
            .saturating_add(self.guidance_map_bytes)
            .saturating_add(retained_endpoint_pair_bytes(
                self.pairs.as_ref().expect("checked prepared pairs"),
            ))
            .saturating_add(self.blocked_bytes)
            .saturating_add(self.crossing_bytes);
        let initial_reserved_bytes = self
            .pair_phase_bytes
            .saturating_add(if self.guidance_reserved {
                self.guidance_map_bytes
            } else {
                0
            })
            .saturating_add(if self.blocked_reserved {
                self.blocked_bytes
            } else {
                0
            })
            .saturating_add(if self.crossing_reserved {
                self.crossing_bytes
            } else {
                0
            });
        if persistent_bytes > initial_reserved_bytes {
            if let Err(stop) = allowance.reserve_peak(persistent_bytes - initial_reserved_bytes) {
                return Err((self, RequestFailure::Stopped(stop)));
            }
        }
        let pairs = self.pairs.take().expect("checked prepared pairs");
        let guidance_maps = self.guidance_maps.take().expect("checked guidance maps");
        let blocked = self.blocked.take().expect("checked blocked cells");
        let crossings = self.crossings.take().expect("checked crossings");
        let pager = RouteCandidatePager::from_prepared_pairs(
            self.request,
            self.config,
            self.guidance,
            self.exact_state_and_capacity_identity,
            self.replay_candidates,
            self.publish_candidates,
            pairs,
            guidance_maps,
            blocked,
            crossings,
            persistent_bytes,
        );
        if initial_reserved_bytes > persistent_bytes {
            allowance.release_peak(initial_reserved_bytes - persistent_bytes);
        }
        Ok(pager)
    }

    pub fn release(self, allowance: &mut RequestAllowance) {
        let reserved_bytes = self
            .pair_phase_bytes
            .saturating_add(if self.guidance_reserved {
                self.guidance_map_bytes
            } else {
                0
            })
            .saturating_add(if self.blocked_reserved {
                self.blocked_bytes
            } else {
                0
            })
            .saturating_add(if self.crossing_reserved {
                self.crossing_bytes
            } else {
                0
            });
        drop(self);
        allowance.release_peak(reserved_bytes);
    }
}

pub struct RouteCandidatePager {
    request: Request,
    config: Config,
    replay_candidates: bool,
    publish_candidates: bool,
    guidance: Option<Guidance>,
    guidance_maps: Vec<Arc<IndexMap<(aic_layout::geometry::Point, usize), u32>>>,
    pairs: Vec<EndpointPair>,
    graph: PreparedGraph<'static>,
    current: Option<Frontier<'static>>,
    pending_found: Option<FoundPathLease>,
    publication_outstanding: bool,
    current_work_limit: usize,
    preflight_charged_expansions: usize,
    preflight_charged_scans: usize,
    phase: usize,
    pair_index: usize,
    phase_searches: usize,
    searches: usize,
    expanded: usize,
    scans: usize,
    anchor_no_path: usize,
    anchor_complete: bool,
    penalties: Arc<IndexMap<Point, u32>>,
    /// None marks a natively rejected witness; Some points to a published one.
    semantic: BTreeMap<SemanticKey, Option<usize>>,
    candidates: Vec<Candidate>,
    persistent_bytes: usize,
    done: bool,
}

impl RouteCandidatePager {
    pub fn new(
        source: &Session,
        request: Request,
        config: Config,
        guidance: Option<Guidance>,
        exact_state_and_capacity_identity: Vec<u8>,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        Self::new_inner(
            source,
            request,
            config,
            guidance,
            exact_state_and_capacity_identity,
            allowance,
            true,
            true,
        )
    }

    pub fn new_unpublished(
        source: &Session,
        request: Request,
        config: Config,
        guidance: Option<Guidance>,
        exact_state_and_capacity_identity: Vec<u8>,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        Self::new_inner(
            source,
            request,
            config,
            guidance,
            exact_state_and_capacity_identity,
            allowance,
            false,
            false,
        )
    }

    fn new_inner(
        source: &Session,
        request: Request,
        config: Config,
        guidance: Option<Guidance>,
        exact_state_and_capacity_identity: Vec<u8>,
        allowance: &mut RequestAllowance,
        replay_candidates: bool,
        publish_candidates: bool,
    ) -> Result<Self, RequestFailure> {
        let mut guidance = guidance;
        let mut exact_state_and_capacity_identity = exact_state_and_capacity_identity;
        let mut preparation = RouteCandidatePreparation::new_inner(
            source,
            request,
            config,
            &mut guidance,
            &mut exact_state_and_capacity_identity,
            allowance,
            replay_candidates,
            publish_candidates,
            0,
        )?;
        if let Err(failure) = preparation.advance_pairs(source, allowance) {
            preparation.release(allowance);
            return Err(failure);
        }
        if let Err(failure) = preparation.advance_guidance(allowance) {
            preparation.release(allowance);
            return Err(failure);
        }
        if let Err(failure) = preparation.advance_carrier_inventory(source, allowance) {
            preparation.release(allowance);
            return Err(failure);
        }
        if let Err(failure) = preparation.advance_blocked(source, allowance) {
            preparation.release(allowance);
            return Err(failure);
        }
        if let Err(failure) = preparation.advance_crossings(source, allowance) {
            preparation.release(allowance);
            return Err(failure);
        }
        preparation.finish(source, allowance)
    }

    fn from_prepared_pairs(
        request: Request,
        config: Config,
        guidance: Option<Guidance>,
        exact_state_and_capacity_identity: Vec<u8>,
        replay_candidates: bool,
        publish_candidates: bool,
        pairs: Vec<EndpointPair>,
        guidance_maps: Vec<Arc<IndexMap<(Point, usize), u32>>>,
        blocked: indexmap::IndexSet<Point>,
        crossings: Crossings,
        persistent_bytes: usize,
    ) -> Self {
        Self {
            request,
            config: config.clone(),
            replay_candidates,
            publish_candidates,
            guidance,
            guidance_maps,
            pairs,
            graph: PreparedGraph::new(
                config.bounds,
                blocked,
                crossings,
                exact_state_and_capacity_identity,
            ),
            current: None,
            pending_found: None,
            publication_outstanding: false,
            current_work_limit: 0,
            preflight_charged_expansions: 0,
            preflight_charged_scans: 0,
            phase: 0,
            pair_index: 0,
            phase_searches: 0,
            searches: 0,
            expanded: 0,
            scans: 0,
            anchor_no_path: 0,
            anchor_complete: true,
            penalties: Arc::new(IndexMap::new()),
            semantic: BTreeMap::new(),
            candidates: Vec::new(),
            persistent_bytes,
            done: false,
        }
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn prepared_identity(&self) -> &[u8] {
        self.graph.identity()
    }

    pub fn work_counts(&self) -> (usize, usize) {
        (self.searches, self.expanded)
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn next_page(
        &mut self,
        source: &Session,
        exact_state_and_capacity_identity: &[u8],
        allowance: &mut RequestAllowance,
        max_new_candidates: usize,
        max_new_expansions: usize,
        max_new_scans: usize,
        page_deadline: Instant,
    ) -> Result<RouteCandidatePage, super::Diagnostic> {
        self.next_page_inner(
            source,
            exact_state_and_capacity_identity,
            allowance,
            max_new_candidates,
            max_new_expansions,
            max_new_scans,
            page_deadline,
            false,
        )
    }

    /// Search-only entry point for a retained publication owner. The caller
    /// takes a found lease and settles it before asking for another search.
    pub fn next_search_page(
        &mut self,
        source: &Session,
        exact_state_and_capacity_identity: &[u8],
        allowance: &mut RequestAllowance,
        max_new_expansions: usize,
        max_new_scans: usize,
        page_deadline: Instant,
    ) -> Result<RouteCandidatePage, super::Diagnostic> {
        self.next_page_inner(
            source,
            exact_state_and_capacity_identity,
            allowance,
            1,
            max_new_expansions,
            max_new_scans,
            page_deadline,
            true,
        )
    }

    pub fn take_found_lease(&mut self) -> Option<FoundPathLease> {
        let lease = self.pending_found.take()?;
        self.publication_outstanding = true;
        Some(lease)
    }

    /// Call after the publication owner has released or transferred the
    /// lease's reservation and committed its accept/reject decision.
    pub fn settle_found_lease(&mut self, lease: FoundPathLease, allowance: &mut RequestAllowance) {
        assert!(self.publication_outstanding);
        assert_eq!(lease.pair_index, self.pair_index);
        lease.release(allowance);
        self.publication_outstanding = false;
        self.pair_index += 1;
    }

    fn next_page_inner(
        &mut self,
        source: &Session,
        exact_state_and_capacity_identity: &[u8],
        allowance: &mut RequestAllowance,
        max_new_candidates: usize,
        max_new_expansions: usize,
        max_new_scans: usize,
        page_deadline: Instant,
        search_only: bool,
    ) -> Result<RouteCandidatePage, super::Diagnostic> {
        if source.revision() != self.request.state_revision
            || exact_state_and_capacity_identity != self.graph.identity()
        {
            return Err(super::error(
                "stale-route-candidate-pager",
                &self.request.connection,
                "routing pager source or exact capacity identity changed",
            ));
        }
        let searches_before = self.searches;
        let expanded_before = self.expanded;
        let scans_before = self.scans;
        let cache_before = self.graph.cache_counts();
        let mut new_candidate_ids = Vec::new();
        let mut updated_candidate_ids = Vec::new();
        let page_expanded_end = self.expanded.saturating_add(max_new_expansions);
        let page_scans_end = self.scans.saturating_add(max_new_scans);
        let status = loop {
            if self.publication_outstanding || search_only && self.pending_found.is_some() {
                break RoutePageStatus::Pending;
            }
            if self.pending_found.is_some() {
                if new_candidate_ids.len() >= max_new_candidates || Instant::now() >= page_deadline
                {
                    break RoutePageStatus::Pending;
                }
                if let Err(stop) = allowance.check_deadline() {
                    if stop.reason == "page-quantum" {
                        break RoutePageStatus::Pending;
                    }
                    self.pending_found
                        .take()
                        .expect("found lease exists")
                        .release(allowance);
                    self.done = true;
                    break RoutePageStatus::Unknown(stop);
                }
            }
            if let Some(mut lease) = self.pending_found.take() {
                let outcome = self.publish_found(
                    source,
                    self.pairs[lease.pair_index].clone(),
                    lease.take_path(),
                    lease.guidance_index,
                    allowance,
                );
                lease.release(allowance);
                match outcome {
                    Ok(PublishOutcome::New(candidate_id)) => new_candidate_ids.push(candidate_id),
                    Ok(PublishOutcome::Updated(candidate_id)) => {
                        if !updated_candidate_ids.contains(&candidate_id) {
                            updated_candidate_ids.push(candidate_id);
                        }
                    }
                    Ok(PublishOutcome::None) => {}
                    Err(RequestFailure::Stopped(stop)) => {
                        self.done = true;
                        break RoutePageStatus::Unknown(stop);
                    }
                    Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic),
                    Err(_) => unreachable!("stable route replay does not prepare endpoints"),
                }
                self.pair_index += 1;
                continue;
            }
            if self.done {
                break RoutePageStatus::Complete(Termination::CandidateLimit);
            }
            if self.pairs.is_empty() {
                self.done = true;
                break RoutePageStatus::Complete(Termination::NoCompatibleEndpoints);
            }
            if self.candidates.len() >= self.config.max_candidates {
                self.done = true;
                break RoutePageStatus::Complete(Termination::CandidateLimit);
            }
            if let Err(stop) = allowance.check_deadline() {
                if stop.reason == "page-quantum" {
                    break RoutePageStatus::Pending;
                }
                self.done = true;
                break RoutePageStatus::Unknown(stop);
            }
            if new_candidate_ids.len() >= max_new_candidates
                || self.expanded >= page_expanded_end
                || self.scans >= page_scans_end
                || Instant::now() >= page_deadline
            {
                break RoutePageStatus::Pending;
            }
            if self.searches >= self.config.max_searches {
                self.done = true;
                break RoutePageStatus::Complete(Termination::SearchLimit);
            }
            if self.expanded >= self.config.max_expanded && self.current.is_none() {
                self.done = true;
                break RoutePageStatus::Complete(Termination::WorkBudget);
            }
            let (anchor, guided, guidance_index, phase_target) = self.phase_info();
            if self.pair_index == self.pairs.len()
                || self.candidates.len() >= phase_target && self.current.is_none()
            {
                if anchor && self.anchor_complete && self.anchor_no_path == self.pairs.len() {
                    self.done = true;
                    let crossing_domain_complete = if self.config.max_crossings_per_route == 0 {
                        source.routes.iter().all(Option::is_none)
                    } else {
                        self.graph.crossing_options_len() <= self.config.max_crossings_per_route
                    };
                    break RoutePageStatus::Complete(if crossing_domain_complete {
                        Termination::FrontierExhausted
                    } else {
                        Termination::ResourceLimit
                    });
                }
                self.phase += 1;
                self.pair_index = 0;
                self.phase_searches = 0;
                continue;
            }
            if self.current.is_none() {
                if allowance.remaining(WorkKind::RoutingScans) == 0 {
                    self.done = true;
                    break RoutePageStatus::Unknown(
                        allowance.exhausted_stop(WorkKind::RoutingScans),
                    );
                }
                if allowance.remaining(WorkKind::Expansions) == 0 {
                    self.done = true;
                    break RoutePageStatus::Unknown(allowance.exhausted_stop(WorkKind::Expansions));
                }
                if allowance.page_remaining(WorkKind::Expansions) == 0
                    || allowance.page_remaining(WorkKind::RoutingScans) == 0
                {
                    break RoutePageStatus::Pending;
                }
                if self.current_work_limit == 0 {
                    let remaining = self.config.max_expanded.saturating_sub(self.expanded);
                    self.current_work_limit = remaining
                        .min(self.config.work_per_search)
                        .min(allowance.remaining(WorkKind::Expansions));
                }
                let work_budget = self.current_work_limit;
                if work_budget == 0 {
                    self.done = true;
                    let stop = allowance.exhausted_stop(WorkKind::Expansions);
                    break RoutePageStatus::Unknown(stop);
                }
                let pair = &self.pairs[self.pair_index];
                allowance.clear_last_stop();
                let preflight = self.graph.advance_preflight(
                    pair.source.point,
                    pair.target.point,
                    pair.source.direction,
                    pair.target.direction,
                    self.config.max_crossings_per_route,
                    work_budget,
                    page_expanded_end
                        .saturating_sub(self.expanded)
                        .min(allowance.page_remaining(WorkKind::Expansions)),
                    page_scans_end
                        .saturating_sub(self.scans)
                        .min(allowance.page_remaining(WorkKind::RoutingScans)),
                    allowance.caps().frontier_entries,
                    page_deadline,
                    allowance.deadline(),
                    &mut |bytes| reserve_kernel_memory(allowance, bytes),
                );
                allowance.release_peak(preflight.released_bytes);
                self.expanded = self.expanded.saturating_add(preflight.expanded);
                self.scans = self.scans.saturating_add(preflight.scans);
                self.preflight_charged_expansions = self
                    .preflight_charged_expansions
                    .saturating_add(preflight.expanded);
                self.preflight_charged_scans =
                    self.preflight_charged_scans.saturating_add(preflight.scans);
                let kernel_stop = allowance.last_stop();
                let metered = record_search_work(
                    allowance,
                    preflight.expanded,
                    preflight.scans,
                    Some(preflight.frontier_peak),
                );
                if let Some(stop) = choose_kernel_stop(kernel_stop, metered) {
                    if stop.reason == "page-quantum"
                        || stop.reason == "aggregate-memory-limit"
                            && matches!(preflight.status, PreflightStatus::Pending)
                    {
                        break RoutePageStatus::Pending;
                    }
                    allowance.confirm_hard_stop(&stop);
                    self.done = true;
                    break RoutePageStatus::Unknown(stop);
                }
                match preflight.status {
                    PreflightStatus::Pending => break RoutePageStatus::Pending,
                    PreflightStatus::Ready => {}
                    PreflightStatus::Stopped(result) => {
                        self.searches += 1;
                        self.phase_searches += 1;
                        self.done = true;
                        break self.stop_status(allowance, result.status);
                    }
                }
                if self.expanded >= page_expanded_end
                    || self.scans >= page_scans_end
                    || allowance.page_remaining(WorkKind::Expansions) == 0
                    || allowance.page_remaining(WorkKind::RoutingScans) == 0
                {
                    break RoutePageStatus::Pending;
                }
                let edge_penalties = guidance_index
                    .and_then(|index| self.guidance_maps.get(index))
                    .cloned();
                let cell_penalties = (!anchor && !guided).then(|| self.penalties.clone());
                allowance.clear_last_stop();
                let result = Frontier::new(
                    &mut self.graph,
                    pair.source.point,
                    pair.target.point,
                    pair.source.direction,
                    pair.target.direction,
                    self.config.max_crossings_per_route,
                    !anchor,
                    work_budget,
                    self.preflight_charged_expansions,
                    self.preflight_charged_scans,
                    cell_penalties,
                    edge_penalties,
                    &mut |bytes| reserve_kernel_memory(allowance, bytes),
                );
                let kernel_stop = allowance.last_stop();
                match result {
                    Ok(frontier) => {
                        // Keep the original probe key stable across an initial
                        // frontier-memory retry. Only a live frontier consumes
                        // the preflight's part of this search's work budget.
                        self.current_work_limit =
                            work_budget.saturating_sub(self.preflight_charged_expansions);
                        self.preflight_charged_expansions = 0;
                        self.preflight_charged_scans = 0;
                        let (expanded, scans, peak) = frontier.work_counts();
                        self.current = Some(frontier);
                        self.expanded = self.expanded.saturating_add(expanded);
                        self.scans = self.scans.saturating_add(scans);
                        let metered = record_search_work(allowance, expanded, scans, Some(peak));
                        if let Some(stop) = choose_kernel_stop(kernel_stop, metered) {
                            if stop.reason == "page-quantum" {
                                break RoutePageStatus::Pending;
                            }
                            allowance.confirm_hard_stop(&stop);
                            self.done = true;
                            break RoutePageStatus::Unknown(stop);
                        }
                    }
                    Err(result) => {
                        if result.status == "memory-pending" {
                            break RoutePageStatus::Pending;
                        }
                        self.current_work_limit = 0;
                        self.preflight_charged_expansions = 0;
                        self.preflight_charged_scans = 0;
                        self.searches += 1;
                        self.phase_searches += 1;
                        self.pair_index += 1;
                        self.anchor_no_path += usize::from(anchor && result.status == "no-path");
                        let metered =
                            record_search_work(allowance, 0, 0, Some(result.frontier_entries_peak));
                        if let Some(stop) = choose_kernel_stop(kernel_stop, metered) {
                            if stop.reason == "page-quantum" {
                                break RoutePageStatus::Pending;
                            }
                            allowance.confirm_hard_stop(&stop);
                            self.done = true;
                            break RoutePageStatus::Unknown(stop);
                        }
                        if result.status == "resource-limit" || result.status == "deadline" {
                            self.done = true;
                            break self.stop_status(allowance, result.status);
                        }
                        continue;
                    }
                }
            }
            let frontier = self.current.as_mut().expect("frontier was prepared");
            let before = frontier.work_counts();
            let page_expansions = page_expanded_end
                .saturating_sub(self.expanded)
                .min(allowance.page_remaining(WorkKind::Expansions));
            let page_scans = page_scans_end
                .saturating_sub(self.scans)
                .min(allowance.page_remaining(WorkKind::RoutingScans));
            let hard_scans = before
                .1
                .saturating_add(allowance.remaining(WorkKind::RoutingScans));
            let hard_expansions = self.current_work_limit.min(
                before
                    .0
                    .saturating_add(allowance.remaining(WorkKind::Expansions)),
            );
            allowance.clear_last_stop();
            let result = frontier.advance(
                &mut self.graph,
                page_expansions,
                page_scans,
                page_deadline,
                allowance.deadline(),
                hard_expansions,
                hard_scans,
                allowance.caps().frontier_entries,
                &mut |bytes| reserve_kernel_memory(allowance, bytes),
            );
            let kernel_stop = allowance.last_stop();
            let after = frontier.work_counts();
            let expansion_delta = after.0.saturating_sub(before.0);
            let scan_delta = after.1.saturating_sub(before.1);
            self.expanded = self.expanded.saturating_add(expansion_delta);
            self.scans = self.scans.saturating_add(scan_delta);
            let completed = matches!(&result, Advance::Complete(_));
            if completed {
                self.searches += 1;
                self.phase_searches += 1;
            }
            let completed_result = if let Advance::Complete(result) = result {
                self.current_work_limit = 0;
                let mut frontier = self.current.take().expect("completed frontier exists");
                if result.status == "found" {
                    let reserved_bytes = frontier.take_found_path_bytes();
                    self.pending_found = Some(FoundPathLease {
                        pair_index: self.pair_index,
                        result: Some(result),
                        guidance_index,
                        reserved_bytes,
                    });
                    allowance.release_peak(frontier.reserved_bytes());
                    None
                } else {
                    allowance.release_peak(frontier.reserved_bytes());
                    self.pair_index += 1;
                    if anchor
                        && matches!(
                            result.status,
                            "no-path" | "blocked-endpoint" | "same-endpoint"
                        )
                    {
                        self.anchor_no_path += 1;
                    }
                    if anchor && result.status == "work-budget" {
                        self.anchor_complete = false;
                    }
                    Some(result)
                }
            } else {
                None
            };
            let metered = record_search_work(allowance, expansion_delta, scan_delta, Some(after.2));
            if let Some(stop) = choose_kernel_stop(kernel_stop, metered) {
                break self.after_kernel_stop(stop, completed, allowance);
            }
            if !completed {
                break RoutePageStatus::Pending;
            }
            let Some(result) = completed_result else {
                continue;
            };
            match result.status {
                "no-path" | "blocked-endpoint" | "same-endpoint" | "work-budget" => {}
                "deadline" | "scan-budget" | "frontier-budget" | "resource-limit" => {
                    self.done = true;
                    break self.stop_status(allowance, result.status);
                }
                "repeated-cell" => {}
                _ => {}
            }
        };
        let cache_after = self.graph.cache_counts();
        Ok(RouteCandidatePage {
            candidate_ids: new_candidate_ids,
            updated_candidate_ids,
            status,
            searches: self.searches - searches_before,
            expanded: self.expanded - expanded_before,
            scans: self.scans - scans_before,
            cache_hits: cache_after.0 - cache_before.0,
            cache_misses: cache_after.1 - cache_before.1,
        })
    }

    fn phase_info(&self) -> (bool, bool, Option<usize>, usize) {
        let heads = self.guidance.as_ref().map_or(0, |guide| guide.heads.len());
        let anchor = self.phase == 0;
        let guided = self.phase > 0 && self.phase <= heads;
        let guidance_index = if guided {
            Some(self.phase - 1)
        } else if !anchor && heads > 0 {
            Some((self.phase - heads - 1) % heads)
        } else {
            None
        };
        let target = if anchor && heads > 0 {
            1
        } else if guided {
            1 + ((self.config.max_candidates - 1) * self.phase).div_ceil(heads)
        } else {
            self.config.max_candidates
        };
        (
            anchor,
            guided,
            guidance_index,
            target.min(self.config.max_candidates),
        )
    }

    fn candidate_provenance(&self, guidance_index: Option<usize>) -> Provenance {
        let head = guidance_index.and_then(|index| {
            self.guidance
                .as_ref()
                .and_then(|guide| guide.heads.get(index))
        });
        if self.phase == 0 {
            Provenance::Anchor
        } else if self.phase <= self.guidance.as_ref().map_or(0, |guide| guide.heads.len()) {
            Provenance::Guided {
                model_version: self
                    .guidance
                    .as_ref()
                    .expect("guided phase")
                    .model_version
                    .clone(),
                head: head.expect("guided head").id.clone(),
            }
        } else {
            Provenance::Diversified {
                model_version: self
                    .guidance
                    .as_ref()
                    .map(|guide| guide.model_version.clone()),
                head: head.map(|head| head.id.clone()),
            }
        }
    }

    fn stop_status(&self, allowance: &RequestAllowance, kernel_status: &str) -> RoutePageStatus {
        if let Some(stop) = allowance.last_stop() {
            return RoutePageStatus::Unknown(stop);
        }
        let category = match kernel_status {
            "scan-budget" => Some(WorkKind::RoutingScans),
            "frontier-budget" => Some(WorkKind::FrontierEntries),
            _ => None,
        };
        let usage = allowance.usage();
        let stop = AllowanceStop {
            reason: match kernel_status {
                "deadline" => "deadline",
                "scan-budget" | "frontier-budget" => "work-limit",
                _ => "resource-limit",
            },
            category,
            limit: category.map_or(0, |kind| allowance.limit(kind)),
            used: category.map_or(0, |kind| usage.categories.get(&kind).copied().unwrap_or(0)),
            deadline_overshoot_ms: usage.deadline_overshoot_ms,
            attempted_bytes: None,
            aggregate_peak_bytes: None,
        };
        allowance.confirm_hard_stop(&stop);
        RoutePageStatus::Unknown(stop)
    }

    fn after_kernel_stop(
        &mut self,
        stop: AllowanceStop,
        completed: bool,
        allowance: &mut RequestAllowance,
    ) -> RoutePageStatus {
        if stop.reason == "page-quantum" || stop.reason == "aggregate-memory-limit" && !completed {
            return RoutePageStatus::Pending;
        }
        if let Some(lease) = self.pending_found.take() {
            lease.release(allowance);
        }
        allowance.confirm_hard_stop(&stop);
        self.done = true;
        RoutePageStatus::Unknown(stop)
    }

    fn publish_found(
        &mut self,
        source: &Session,
        pair: EndpointPair,
        mut result: PathResult,
        guidance_index: Option<usize>,
        allowance: &mut RequestAllowance,
    ) -> Result<PublishOutcome, RequestFailure> {
        let cells = result.cells.take().expect("found path has cells");
        let crossings = result.crossings.take().unwrap_or_default();
        let postprocess_bytes = cells
            .len()
            .saturating_mul(2048)
            .saturating_add(crossings.len().saturating_mul(1024))
            .saturating_add(4096);
        allowance.reserve_peak(postprocess_bytes)?;
        let mut extra_reserved = 0usize;
        let mut updated_candidate = None;
        let outcome = (|| {
            allowance.charge(
                WorkKind::RoutingScans,
                cells.len().saturating_add(crossings.len()),
            )?;
            allowance.charge(
                WorkKind::CopiedBytes,
                cells.len().saturating_mul(std::mem::size_of::<Point>()),
            )?;
            let key = (
                pair.source.port.clone(),
                pair.target.port.clone(),
                cells.clone(),
                crossings
                    .iter()
                    .map(|crossing| {
                        (
                            crossing.position,
                            crossing.direction,
                            crossing.component.clone(),
                            crossing.rotation,
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            let overlap = super::maximum_overlap(&cells, &self.candidates, allowance)?;
            let learned_cost = if let Some(costs) =
                guidance_index.and_then(|index| self.guidance_maps.get(index))
            {
                super::directed_cost(&cells, costs, allowance)?
            } else {
                0
            };
            if let Some(published) = self.semantic.get(&key).copied() {
                // The key contains every input to Session::route_commands;
                // with the fixed source revision, replay has the same exact
                // physical effect and cost. Equal costs are already the
                // lexicographic minimum, so only provenance can change.
                if let Some(index) = published {
                    let provenance = self.candidate_provenance(guidance_index);
                    let candidate = &mut self.candidates[index];
                    if candidate.provenance != provenance
                        && !candidate.additional_provenance.contains(&provenance)
                    {
                        let bytes = 256usize.saturating_add(match &provenance {
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
                        });
                        allowance.reserve_peak(bytes)?;
                        self.persistent_bytes = self.persistent_bytes.saturating_add(bytes);
                        candidate.additional_provenance.push(provenance);
                        updated_candidate = Some(index);
                    }
                }
                super::add_overlap_penalty(
                    Arc::make_mut(&mut self.penalties),
                    &cells,
                    self.config.overlap_penalty,
                    allowance,
                )?;
                return Ok(None);
            }
            let commands = source.route_commands(
                &self.request.connection,
                pair.source.port.clone(),
                pair.target.port.clone(),
                &cells,
                &crossings,
                allowance,
            )?;
            let physical_cost = if self.replay_candidates {
                match source.replay_route_witness_with_allowance(
                    self.request.state_revision,
                    &commands,
                    allowance,
                ) {
                    Ok((after, reserved_bytes)) => {
                        let cost = super::physical_cost::physical_cost_between(
                            source,
                            &after,
                            std::slice::from_ref(&self.request.connection),
                        );
                        drop(after);
                        allowance.release_peak(reserved_bytes);
                        cost.map_err(RequestFailure::Invalid)?
                    }
                    Err(RequestFailure::Invalid(_)) => {
                        self.semantic.insert(key, None);
                        super::add_overlap_penalty(
                            Arc::make_mut(&mut self.penalties),
                            &cells,
                            self.config.overlap_penalty,
                            allowance,
                        )?;
                        return Ok(None);
                    }
                    Err(failure) => return Err(failure),
                }
            } else {
                super::PhysicalCost::default()
            };
            let provenance = self.candidate_provenance(guidance_index);
            let head = guidance_index.and_then(|index| {
                self.guidance
                    .as_ref()
                    .and_then(|guide| guide.heads.get(index))
            });
            let candidate = Candidate {
                id: self.candidates.len(),
                state_revision: self.request.state_revision,
                connection: self.request.connection.clone(),
                source_port: pair.source.port,
                target_port: pair.target.port,
                cells,
                commands,
                crossings,
                steps: result.steps.expect("found path has steps"),
                turns: result.turns.expect("found path has turns"),
                expanded: result.expanded,
                overlap_with_previous: overlap,
                learned_cost,
                predicted_success: head.map(|head| head.predicted_success),
                guidance_uncertainty: head.map(|head| head.uncertainty),
                provenance,
                additional_provenance: Vec::new(),
                physical_cost,
                charged_work: allowance.usage(),
            };
            let penalty_before = self.penalties.len();
            super::add_overlap_penalty(
                Arc::make_mut(&mut self.penalties),
                &candidate.cells,
                self.config.overlap_penalty,
                allowance,
            )?;
            let penalty_bytes = self
                .penalties
                .len()
                .saturating_sub(penalty_before)
                .saturating_mul(128);
            let retained = candidate
                .cells
                .len()
                .saturating_mul(128)
                .saturating_add(candidate.commands.len().saturating_mul(256))
                .saturating_add(candidate.crossings.len().saturating_mul(512))
                .saturating_add(penalty_bytes)
                .saturating_add(2048);
            if retained > postprocess_bytes {
                let extra = retained - postprocess_bytes;
                allowance.reserve_peak(extra)?;
                extra_reserved = extra;
            }
            if self.publish_candidates {
                super::json_size_with_allowance(&candidate, allowance)?;
                allowance.charge(WorkKind::Candidates, 1)?;
            } else {
                allowance.charge(WorkKind::CopiedBytes, retained)?;
            }
            let candidate_id = candidate.id;
            self.semantic.insert(key, Some(candidate_id));
            self.candidates.push(candidate);
            Ok(Some((candidate_id, retained)))
        })();
        match outcome {
            Ok(Some((candidate_id, retained))) => {
                if retained < postprocess_bytes {
                    allowance.release_peak(postprocess_bytes - retained);
                }
                self.persistent_bytes = self.persistent_bytes.saturating_add(retained);
                Ok(PublishOutcome::New(candidate_id))
            }
            Ok(None) => {
                allowance.release_peak(postprocess_bytes.saturating_add(extra_reserved));
                Ok(updated_candidate.map_or(PublishOutcome::None, PublishOutcome::Updated))
            }
            Err(failure) => {
                allowance.release_peak(postprocess_bytes.saturating_add(extra_reserved));
                Err(failure)
            }
        }
    }

    pub fn release(self, allowance: &mut RequestAllowance) {
        let reserved_bytes = self
            .persistent_bytes
            .saturating_add(self.graph.reserved_bytes())
            .saturating_add(self.current.as_ref().map_or(0, Frontier::reserved_bytes))
            .saturating_add(
                self.pending_found
                    .as_ref()
                    .map_or(0, |found| found.reserved_bytes),
            );
        drop(self);
        allowance.release_peak(reserved_bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::super::CUMULATIVE_WORK_KINDS;
    use super::super::{DirectedCost, GUIDANCE_SCHEMA, GuidanceHead};
    use super::*;
    use std::time::Duration;

    #[test]
    fn long_connection_id_stays_reserved_through_preparation_and_pager() {
        let mut problem = super::super::super::tests::session().problem().clone();
        let connection_id = "connection".repeat(4096);
        problem.connections[0].id = connection_id.clone();
        let mut source = Session::new(
            problem,
            super::super::super::tests::catalogs(),
            crate::constructive::Limits {
                max_turns: 32,
                max_extension: 16,
            },
        )
        .unwrap();
        super::super::super::tests::place_pair(&mut source);
        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: connection_id,
        };
        let owner_bytes = RouteCandidatePreparation::owner_bytes(&request);
        let memory = super::super::SearchMemoryBudget::new(128 * 1024 * 1024);
        let mut allowance = RequestAllowance::with_memory_budget(
            config.request_allowance.clone(),
            Some(memory.clone()),
        );
        let mut guidance = None;
        let mut identity = vec![1, 2, 3];
        let mut preparation = RouteCandidatePreparation::new_retained_borrowed(
            &source,
            &request,
            &config,
            &mut guidance,
            &mut identity,
            &mut allowance,
        )
        .unwrap();
        assert!(memory.live_bytes() >= owner_bytes);
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let endpoint_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::EndpointVisits)
            .unwrap();
        quotas[endpoint_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            preparation.advance_pairs(&source, &mut allowance),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        assert!(memory.live_bytes() >= owner_bytes);
        allowance.end_page_window();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        preparation.advance_guidance(&mut allowance).unwrap();
        preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_crossings(&source, &mut allowance)
            .unwrap();
        let pager = preparation
            .finish_retained(&source, &mut allowance)
            .unwrap_or_else(|(_, failure)| panic!("retained finish failed: {failure:?}"));
        assert_eq!(pager.request().connection, request.connection);
        assert!(memory.live_bytes() >= owner_bytes);
        pager.release(&mut allowance);
        assert_eq!(memory.live_bytes(), 0);

        let mut aborted = RouteCandidatePreparation::new_retained_borrowed(
            &source,
            &request,
            &config,
            &mut guidance,
            &mut identity,
            &mut allowance,
        )
        .unwrap();
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert!(matches!(
            aborted.advance_pairs(&source, &mut allowance),
            Err(RequestFailure::Stopped(stop)) if stop.reason == "page-quantum"
        ));
        allowance.end_page_window();
        aborted.release(&mut allowance);
        assert_eq!(memory.live_bytes(), 0);
    }

    #[test]
    fn final_preparation_transfer_yields_without_losing_owned_stages() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let identity = vec![4, 5, 6];
        let mut direct_allowance = RequestAllowance::new(config.request_allowance.clone());
        let direct = RouteCandidatePager::new(
            &source,
            request.clone(),
            config.clone(),
            None,
            identity.clone(),
            &mut direct_allowance,
        )
        .unwrap();

        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut preparation = RouteCandidatePreparation::new(
            &source,
            request,
            config,
            None,
            identity,
            &mut allowance,
        )
        .unwrap();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        preparation.advance_guidance(&mut allowance).unwrap();
        preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_crossings(&source, &mut allowance)
            .unwrap();
        allowance.begin_page_window([usize::MAX; CUMULATIVE_WORK_KINDS.len()], Duration::ZERO);
        let (preparation, failure) = match preparation.finish_retained(&source, &mut allowance) {
            Ok(_) => panic!("zero-time first page should yield before ownership transfer"),
            Err(retained) => retained,
        };
        assert!(matches!(
            failure,
            RequestFailure::Stopped(stop) if stop.reason == "page-quantum"
        ));
        assert_eq!(
            preparation.pairs.as_ref().unwrap().len(),
            direct.pairs.len()
        );
        allowance.end_page_window();
        let resumed = preparation
            .finish_retained(&source, &mut allowance)
            .unwrap_or_else(|(_, failure)| panic!("retained finish failed: {failure:?}"));
        let signature = |pairs: &[EndpointPair]| {
            pairs
                .iter()
                .map(|pair| {
                    (
                        pair.source.port.clone(),
                        pair.source.point,
                        pair.source.direction,
                        pair.target.port.clone(),
                        pair.target.point,
                        pair.target.direction,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(signature(&resumed.pairs), signature(&direct.pairs));
        assert_eq!(
            allowance.usage().categories,
            direct_allowance.usage().categories
        );
        assert_eq!(allowance.reserved_bytes, direct_allowance.reserved_bytes);
        resumed.release(&mut allowance);
        direct.release(&mut direct_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(direct_allowance.reserved_bytes, 0);
    }

    #[test]
    fn preparation_admission_yield_keeps_model_output_and_identity() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut guidance = Some(Guidance {
            schema: GUIDANCE_SCHEMA.into(),
            state_revision: source.revision(),
            connection: "connection-0".into(),
            model_version: "test".into(),
            heads: vec![GuidanceHead {
                id: "first".into(),
                directed_costs: vec![],
                predicted_success: 0.5,
                uncertainty: 0.5,
            }],
        });
        let mut identity = vec![7, 8, 9];
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        allowance.begin_page_window([usize::MAX; CUMULATIVE_WORK_KINDS.len()], Duration::ZERO);
        let failure = match RouteCandidatePreparation::new_retained(
            &source,
            request.clone(),
            config.clone(),
            &mut guidance,
            &mut identity,
            &mut allowance,
        ) {
            Ok(_) => panic!("zero-time page should yield before admission"),
            Err(failure) => failure,
        };
        assert!(matches!(
            failure,
            RequestFailure::Stopped(stop) if stop.reason == "page-quantum"
        ));
        assert_eq!(identity, vec![7, 8, 9]);
        assert_eq!(guidance.as_ref().unwrap().heads[0].id, "first");
        assert_eq!(allowance.reserved_bytes, 0);
        allowance.end_page_window();
        let preparation = RouteCandidatePreparation::new_retained(
            &source,
            request,
            config,
            &mut guidance,
            &mut identity,
            &mut allowance,
        )
        .unwrap();
        assert!(guidance.is_none());
        assert!(identity.is_empty());
        assert_eq!(preparation.exact_state_and_capacity_identity, vec![7, 8, 9]);
        preparation.release(&mut allowance);
        assert_eq!(allowance.reserved_bytes, 0);
    }

    #[test]
    fn preparation_owns_endpoint_page_yield_and_transfers_its_reservation() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut baseline_allowance = RequestAllowance::new(config.request_allowance.clone());
        let baseline = RouteCandidatePager::new(
            &source,
            request.clone(),
            config.clone(),
            None,
            vec![7],
            &mut baseline_allowance,
        )
        .unwrap();

        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut preparation =
            RouteCandidatePreparation::new(&source, request, config, None, vec![7], &mut allowance)
                .unwrap();
        let pair_bytes = preparation.pair_phase_bytes;
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let visit_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::EndpointVisits)
            .unwrap();
        quotas[visit_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let failure = preparation
            .advance_pairs(&source, &mut allowance)
            .unwrap_err();
        assert!(
            matches!(failure, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum")
        );
        assert_eq!(allowance.reserved_bytes, pair_bytes);
        assert!(preparation.pair_cursor.is_some());
        allowance.end_page_window();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        assert!(preparation.pair_cursor.is_none());
        preparation.advance_guidance(&mut allowance).unwrap();
        preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap();
        let pager = preparation.finish(&source, &mut allowance).unwrap();
        let signature = |pager: &RouteCandidatePager| {
            pager
                .pairs
                .iter()
                .map(|pair| {
                    (
                        pair.source.port.clone(),
                        pair.source.point,
                        pair.target.port.clone(),
                        pair.target.point,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(signature(&pager), signature(&baseline));
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
        assert_eq!(allowance.reserved_bytes, pager.persistent_bytes);
        pager.release(&mut allowance);
        baseline.release(&mut baseline_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(baseline_allowance.reserved_bytes, 0);
    }

    #[test]
    fn preparation_rejects_early_finish_and_a_different_source() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let early = RouteCandidatePreparation::new(
            &source,
            request.clone(),
            config.clone(),
            None,
            vec![7],
            &mut allowance,
        )
        .unwrap();
        assert!(matches!(
            early.finish(&source, &mut allowance),
            Err(RequestFailure::Invalid(diagnostic))
                if diagnostic.code.ends_with("incomplete-route-candidate-preparation")
        ));
        assert_eq!(allowance.reserved_bytes, 0);

        let mut preparation =
            RouteCandidatePreparation::new(&source, request, config, None, vec![7], &mut allowance)
                .unwrap();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        let other = source.clone();
        assert!(matches!(
            preparation.finish(&other, &mut allowance),
            Err(RequestFailure::Invalid(diagnostic))
                if diagnostic.code.ends_with("stale-route-candidate-request")
        ));
        assert_eq!(allowance.reserved_bytes, 0);

        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut preparation =
            RouteCandidatePreparation::new(&source, request, config, None, vec![7], &mut allowance)
                .unwrap();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        let mut fork = source.clone();
        std::mem::swap(&mut source, &mut fork);
        assert_eq!(source.revision(), fork.revision());
        assert_eq!(source.turns, fork.turns);
        assert!(matches!(
            preparation.finish(&source, &mut allowance),
            Err(RequestFailure::Invalid(diagnostic))
                if diagnostic.code.ends_with("stale-route-candidate-request")
        ));
        assert_eq!(allowance.reserved_bytes, 0);
    }

    #[test]
    fn preparation_owns_guidance_scan_across_page_yield() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let guidance = Guidance {
            schema: GUIDANCE_SCHEMA.into(),
            state_revision: request.state_revision,
            connection: request.connection.clone(),
            model_version: "test".into(),
            heads: vec![GuidanceHead {
                id: "head".into(),
                predicted_success: 0.5,
                uncertainty: 0.25,
                directed_costs: (0..3)
                    .map(|x| DirectedCost {
                        point: Point { x, y: 1 },
                        direction: 0,
                        cost: 100 + x as u32,
                    })
                    .collect(),
            }],
        };
        let mut baseline_allowance = RequestAllowance::new(config.request_allowance.clone());
        let baseline = RouteCandidatePager::new(
            &source,
            request.clone(),
            config.clone(),
            Some(guidance.clone()),
            vec![9],
            &mut baseline_allowance,
        )
        .unwrap();

        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut preparation = RouteCandidatePreparation::new(
            &source,
            request,
            config,
            Some(guidance),
            vec![9],
            &mut allowance,
        )
        .unwrap();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let scan_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::RoutingScans)
            .unwrap();
        quotas[scan_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let failure = preparation.advance_guidance(&mut allowance).unwrap_err();
        assert!(
            matches!(failure, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum")
        );
        assert_eq!(preparation.guidance_cursor.as_ref().unwrap().cost_index, 1);
        assert_eq!(
            allowance.reserved_bytes,
            preparation.pair_phase_bytes + preparation.guidance_map_bytes
        );
        allowance.end_page_window();
        preparation.advance_guidance(&mut allowance).unwrap();
        preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap();
        let pager = preparation.finish(&source, &mut allowance).unwrap();
        assert_eq!(pager.guidance_maps, baseline.guidance_maps);
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
        assert_eq!(allowance.reserved_bytes, pager.persistent_bytes);
        pager.release(&mut allowance);
        baseline.release(&mut baseline_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(baseline_allowance.reserved_bytes, 0);
    }

    #[test]
    fn preparation_owns_blocked_scan_across_page_yield() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let config = super::super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut baseline_allowance = RequestAllowance::new(config.request_allowance.clone());
        let expected_blocked = source
            .route_blocked_cells_bounded(
                0,
                config.bounds,
                &mut RequestAllowance::new(config.request_allowance.clone()),
            )
            .unwrap();
        let baseline = RouteCandidatePager::new(
            &source,
            request.clone(),
            config.clone(),
            None,
            vec![11],
            &mut baseline_allowance,
        )
        .unwrap();

        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut preparation = RouteCandidatePreparation::new(
            &source,
            request,
            config,
            None,
            vec![11],
            &mut allowance,
        )
        .unwrap();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        preparation.advance_guidance(&mut allowance).unwrap();
        preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let visit_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::BlockedVisits)
            .unwrap();
        quotas[visit_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let failure = preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap_err();
        assert!(
            matches!(failure, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum")
        );
        assert!(preparation.blocked_cursor.is_some());
        assert_eq!(
            allowance.reserved_bytes,
            preparation.pair_phase_bytes + preparation.blocked_bytes
        );
        allowance.end_page_window();
        preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap();
        assert_eq!(preparation.blocked.as_ref().unwrap(), &expected_blocked);
        let pager = preparation.finish(&source, &mut allowance).unwrap();
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
        assert_eq!(allowance.reserved_bytes, pager.persistent_bytes);
        pager.release(&mut allowance);
        baseline.release(&mut baseline_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(baseline_allowance.reserved_bytes, 0);
    }

    #[test]
    fn preparation_owns_crossing_scans_across_page_yields() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let mut config = super::super::super::tests::route_candidate_config();
        config.max_crossings_per_route = 1;
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut baseline_allowance = RequestAllowance::new(config.request_allowance.clone());
        let baseline = RouteCandidatePager::new(
            &source,
            request.clone(),
            config.clone(),
            None,
            vec![13],
            &mut baseline_allowance,
        )
        .unwrap();

        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut preparation = RouteCandidatePreparation::new(
            &source,
            request,
            config,
            None,
            vec![13],
            &mut allowance,
        )
        .unwrap();
        preparation.advance_pairs(&source, &mut allowance).unwrap();
        preparation.advance_guidance(&mut allowance).unwrap();
        preparation
            .advance_carrier_inventory(&source, &mut allowance)
            .unwrap();
        preparation
            .advance_blocked(&source, &mut allowance)
            .unwrap();
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let blocked_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::BlockedVisits)
            .unwrap();
        quotas[blocked_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let stop = preparation
            .advance_crossings(&source, &mut allowance)
            .unwrap_err();
        assert!(matches!(stop, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum"));
        assert!(preparation.crossing_cursor.is_some());
        assert_eq!(
            allowance.reserved_bytes,
            preparation.pair_phase_bytes + preparation.blocked_bytes + preparation.crossing_bytes
        );
        allowance.end_page_window();

        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let catalog_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::CatalogVisits)
            .unwrap();
        quotas[catalog_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let stop = preparation
            .advance_crossings(&source, &mut allowance)
            .unwrap_err();
        assert!(matches!(stop, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum"));
        allowance.end_page_window();
        preparation
            .advance_crossings(&source, &mut allowance)
            .unwrap();
        let pager = preparation.finish(&source, &mut allowance).unwrap();
        assert_eq!(
            pager.graph.crossing_options_len(),
            baseline.graph.crossing_options_len()
        );
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
        assert_eq!(allowance.reserved_bytes, pager.persistent_bytes);
        pager.release(&mut allowance);
        baseline.release(&mut baseline_allowance);
        assert_eq!(allowance.reserved_bytes, 0);
        assert_eq!(baseline_allowance.reserved_bytes, 0);
    }

    #[test]
    fn completed_kernel_records_every_category_even_after_first_stop() {
        let mut allowance = RequestAllowance::new(super::super::AllowanceCaps {
            expansions: 1,
            routing_scans: 1,
            frontier_entries: 1,
            ..Default::default()
        });
        let stop = record_search_work(&mut allowance, 2, 3, Some(4)).unwrap_err();
        assert_eq!(stop.reason, "work-limit");
        let usage = allowance.usage();
        assert_eq!(usage.categories[&WorkKind::Expansions], 2);
        assert_eq!(usage.categories[&WorkKind::RoutingScans], 3);
        assert_eq!(usage.categories[&WorkKind::FrontierEntries], 4);
    }

    #[test]
    fn frontier_memory_retry_reuses_the_charged_preflight_key() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let mut config = super::super::super::tests::route_candidate_config();
        config.work_per_search = 1024;
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let tree = super::super::SearchMemoryBudget::new(100_000_000);
        let mut allowance = RequestAllowance::with_memory_budget(
            config.request_allowance.clone(),
            Some(tree.clone()),
        );
        let mut pager = RouteCandidatePager::new_unpublished(
            &source,
            request,
            config,
            None,
            vec![23],
            &mut allowance,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..1000 {
            let page = pager
                .next_page(&source, &[23], &mut allowance, 1, 1, 1, deadline)
                .unwrap();
            assert!(matches!(page.status, RoutePageStatus::Pending));
            if pager.graph.retained_entries() > 0 {
                break;
            }
        }
        assert_eq!(pager.graph.retained_entries(), 1);
        assert!(pager.current.is_none());
        let preflight_expansions = pager.preflight_charged_expansions;
        assert!(preflight_expansions > 0);
        assert_eq!(pager.current_work_limit, 1024);
        assert!(
            256.min((1024 - preflight_expansions) / 4) < 256,
            "this fixture must expose the old changing-key bug"
        );
        let work_before_retry = allowance.usage().categories;
        let peer_bytes = tree.limit_bytes() - tree.live_bytes() - 256;
        tree.reserve(peer_bytes).unwrap();
        let pending = pager
            .next_page(&source, &[23], &mut allowance, 1, 1, 1, deadline)
            .unwrap();
        assert!(matches!(pending.status, RoutePageStatus::Pending));
        assert!(pager.current.is_none());
        assert_eq!(pager.current_work_limit, 1024);
        assert_eq!(pager.preflight_charged_expansions, preflight_expansions);
        assert_eq!(pager.graph.retained_entries(), 1);
        assert_eq!(allowance.usage().categories, work_before_retry);
        tree.release(peer_bytes);
        let resumed = pager
            .next_page(&source, &[23], &mut allowance, 1, 1, 1, deadline)
            .unwrap();
        assert!(matches!(resumed.status, RoutePageStatus::Pending));
        assert!(pager.current.is_some());
        assert_eq!(pager.graph.retained_entries(), 1);
        assert_eq!(pager.preflight_charged_expansions, 0);
        pager.release(&mut allowance);
        assert_eq!(allowance.reserved_bytes(), 0);
        assert_eq!(tree.live_bytes(), 0);
    }

    #[test]
    fn hard_completed_work_stop_overrides_soft_kernel_yield() {
        let soft = AllowanceStop {
            reason: "page-quantum",
            category: Some(WorkKind::RoutingScans),
            limit: 1,
            used: 2,
            deadline_overshoot_ms: 0,
            attempted_bytes: None,
            aggregate_peak_bytes: None,
        };
        let hard = AllowanceStop {
            reason: "work-limit",
            category: Some(WorkKind::Expansions),
            limit: 1,
            used: 2,
            deadline_overshoot_ms: 0,
            attempted_bytes: None,
            aggregate_peak_bytes: None,
        };
        assert_eq!(
            choose_kernel_stop(Some(soft), Err(hard.clone())),
            Some(hard)
        );
    }

    #[test]
    fn search_only_found_lease_pauses_and_releases_once() {
        let mut source = super::super::super::tests::session();
        super::super::super::tests::place_pair(&mut source);
        let mut config = super::super::super::tests::route_candidate_config();
        config.work_per_search = 2_000_000;
        config.max_expanded = 2_000_000;
        let mut legacy_allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut legacy = RouteCandidatePager::new_unpublished(
            &source,
            Request {
                state_revision: source.revision(),
                connection: "connection-0".into(),
            },
            config.clone(),
            None,
            vec![31],
            &mut legacy_allowance,
        )
        .unwrap();
        let request = Request {
            state_revision: source.revision(),
            connection: "connection-0".into(),
        };
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        let mut pager = RouteCandidatePager::new_unpublished(
            &source,
            request,
            config,
            None,
            vec![31],
            &mut allowance,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..1000 {
            let page = pager
                .next_search_page(&source, &[31], &mut allowance, 64, 256, deadline)
                .unwrap();
            assert!(matches!(page.status, RoutePageStatus::Pending));
            if pager.pending_found.is_some() {
                break;
            }
        }
        assert!(pager.pending_found.is_some(), "fixture must find a route");
        let searches = pager.searches;
        let work = allowance.usage().categories;
        let post_kernel_page_stop = AllowanceStop {
            reason: "page-quantum",
            category: Some(WorkKind::RoutingScans),
            limit: pager.scans,
            used: pager.scans,
            deadline_overshoot_ms: 0,
            attempted_bytes: None,
            aggregate_peak_bytes: None,
        };
        assert!(matches!(
            pager.after_kernel_stop(post_kernel_page_stop, true, &mut allowance),
            RoutePageStatus::Pending
        ));
        assert!(pager.pending_found.is_some());
        assert_eq!(pager.searches, searches);
        let lease = pager.take_found_lease().unwrap();
        assert_eq!(lease.path().status, "found");
        assert!(lease.reserved_bytes > 0);
        assert!(lease.path().preflight_expanded > 0);
        assert_eq!(lease.path().expanded, pager.expanded);
        assert_eq!(lease.path().routing_scans, pager.scans);
        for _ in 0..1000 {
            let page = legacy
                .next_page(&source, &[31], &mut legacy_allowance, 1, 64, 256, deadline)
                .unwrap();
            if !page.candidate_ids.is_empty() {
                break;
            }
        }
        assert_eq!(legacy.candidates().len(), 1);
        assert_eq!(
            lease.path().cells.as_ref().unwrap(),
            &legacy.candidates()[0].cells
        );
        assert_eq!(pager.searches, legacy.searches);
        assert_eq!(pager.expanded, legacy.expanded);
        assert_eq!(pager.scans, legacy.scans);
        assert!(matches!(
            pager
                .next_search_page(&source, &[31], &mut allowance, 64, 256, deadline)
                .unwrap()
                .status,
            RoutePageStatus::Pending
        ));
        assert_eq!(pager.searches, searches);
        assert_eq!(allowance.usage().categories, work);
        pager.settle_found_lease(lease, &mut allowance);
        assert_eq!(pager.pair_index, 1);
        pager.release(&mut allowance);
        legacy.release(&mut legacy_allowance);
        assert_eq!(allowance.reserved_bytes(), 0);
        assert_eq!(legacy_allowance.reserved_bytes(), 0);
    }

    #[test]
    fn found_lease_accept_and_cancel_release_the_search_reservation() {
        for accept in [true, false] {
            let mut source = super::super::super::tests::session();
            super::super::super::tests::place_pair(&mut source);
            let config = super::super::super::tests::route_candidate_config();
            let mut allowance = RequestAllowance::new(config.request_allowance.clone());
            let mut pager = RouteCandidatePager::new_unpublished(
                &source,
                Request {
                    state_revision: source.revision(),
                    connection: "connection-0".into(),
                },
                config,
                None,
                vec![41],
                &mut allowance,
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            for _ in 0..1000 {
                pager
                    .next_search_page(&source, &[41], &mut allowance, 64, 256, deadline)
                    .unwrap();
                if pager.pending_found.is_some() {
                    break;
                }
            }
            let mut lease = pager.take_found_lease().expect("fixture finds a path");
            if accept {
                let path = lease.take_path();
                assert!(path.cells.as_ref().is_some_and(|cells| !cells.is_empty()));
                drop(path);
                pager.settle_found_lease(lease, &mut allowance);
                assert_eq!(pager.pair_index, 1);
            } else {
                lease.release(&mut allowance);
                assert!(pager.publication_outstanding);
                assert_eq!(pager.pair_index, 0);
            }
            pager.release(&mut allowance);
            assert_eq!(allowance.reserved_bytes(), 0);
        }
    }
}
