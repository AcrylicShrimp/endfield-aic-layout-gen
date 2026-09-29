//! Bounded diverse route candidates for stable constructive states.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use aic_layout::{
    facilities::{FacilityPortDirection, FacilityPortEdge},
    geometry::{Bounds, Point, layer},
    logistics::{LogisticsComponentKind, TransportKind},
    physical::{Bridge, BridgeAxis},
    projection::{edge_index, project_port},
    validation::within_capacity,
};
use aic_routing::{Crossing, PathOptions, shortest_path};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::{
    Command, Diagnostic, Direction, ReplayMeter, ReplayWork, Session, Step, error,
    native_replay::{NativeReplayProgress, ReplayAdvance, SourceOwner},
};

mod guidance_preparation;
mod physical_cost;
mod repair;
mod route_pager;
mod route_preparation;
pub use guidance_preparation::RouteGuidancePreparation;
pub use physical_cost::{ComponentUse, PhysicalCost};
pub use repair::{
    PreparedRepair, REPAIR_CANDIDATE_SCHEMA, RepairCandidate, RepairDraft, RepairDraftState,
    RepairPreparationSizes, RepairRequest, RepairScope, publish_repair_candidate,
};
pub use route_pager::{
    FoundPathLease, RouteCandidatePage, RouteCandidatePager, RouteCandidatePreparation,
    RoutePageStatus,
};

pub const SCHEMA: &str = "aic-constructive-route-candidates-v5";
pub const GUIDANCE_SCHEMA: &str = "aic-constructive-route-guidance-v2";
pub const GUIDANCE_GRID_CHANNELS: usize = 16;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bounds: Bounds,
    pub max_candidates: usize,
    pub max_searches: usize,
    pub work_per_search: usize,
    pub max_expanded: usize,
    pub overlap_penalty: u32,
    /// Missing in older saved panels means the prior crossing-disabled behavior.
    #[serde(default)]
    pub max_crossings_per_route: usize,
    #[serde(default)]
    pub request_allowance: AllowanceCaps,
}

/// Separate count ceilings for one logical-network request. Each ceiling is
/// finite; units are deliberately not combined into a synthetic work score.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AllowanceCaps {
    pub endpoint_visits: usize,
    pub raster_cells: usize,
    pub graph_rows: usize,
    pub route_cells: usize,
    pub blocked_visits: usize,
    pub copied_bytes: usize,
    pub inference_elements: usize,
    pub expansions: usize,
    pub frontier_entries: usize,
    pub routing_scans: usize,
    pub replay_route_cells: usize,
    pub replay_users: usize,
    pub catalog_visits: usize,
    pub validation_records: usize,
    pub published_bytes: usize,
    pub peak_live_bytes: usize,
    pub max_candidates: usize,
    pub max_variants: usize,
    pub max_inference_batch: usize,
    pub deadline_ms: u64,
}

impl Default for AllowanceCaps {
    fn default() -> Self {
        Self {
            endpoint_visits: 1_000_000,
            raster_cells: 10_000_000,
            graph_rows: 10_000_000,
            route_cells: 50_000_000,
            blocked_visits: 10_000_000,
            copied_bytes: 512 * 1024 * 1024,
            inference_elements: 100_000_000,
            expansions: 100_000_000,
            frontier_entries: 10_000_000,
            routing_scans: 100_000_000,
            replay_route_cells: 50_000_000,
            replay_users: 10_000_000,
            catalog_visits: 10_000_000,
            validation_records: 10_000_000,
            published_bytes: 64 * 1024 * 1024,
            peak_live_bytes: 1024 * 1024 * 1024,
            max_candidates: 64,
            max_variants: 64,
            max_inference_batch: 8,
            deadline_ms: 60_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum WorkKind {
    EndpointVisits,
    RasterCells,
    GraphRows,
    RouteCells,
    BlockedVisits,
    CopiedBytes,
    InferenceElements,
    Expansions,
    FrontierEntries,
    RoutingScans,
    ReplayRouteCells,
    ReplayUsers,
    CatalogVisits,
    ValidationRecords,
    PublishedBytes,
    Candidates,
    Variants,
}

pub const CUMULATIVE_WORK_KINDS: [WorkKind; 16] = [
    WorkKind::EndpointVisits,
    WorkKind::RasterCells,
    WorkKind::GraphRows,
    WorkKind::RouteCells,
    WorkKind::BlockedVisits,
    WorkKind::CopiedBytes,
    WorkKind::InferenceElements,
    WorkKind::Expansions,
    WorkKind::RoutingScans,
    WorkKind::ReplayRouteCells,
    WorkKind::ReplayUsers,
    WorkKind::CatalogVisits,
    WorkKind::ValidationRecords,
    WorkKind::PublishedBytes,
    WorkKind::Candidates,
    WorkKind::Variants,
];

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AllowanceStop {
    pub reason: &'static str,
    pub category: Option<WorkKind>,
    pub limit: usize,
    pub used: usize,
    pub deadline_overshoot_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempted_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate_peak_bytes: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AllowanceUsage {
    pub categories: BTreeMap<WorkKind, usize>,
    pub peak_live_bytes: usize,
    pub deadline_overshoot_ms: u64,
}

pub struct RequestAllowance {
    caps: AllowanceCaps,
    used: BTreeMap<WorkKind, usize>,
    deadline: Instant,
    paused_at: Option<Instant>,
    reserved_bytes: usize,
    peak_live_bytes: usize,
    tree_memory: Option<SearchMemoryBudget>,
    page_window: Option<PageWindow>,
    last_stop: std::cell::RefCell<Option<AllowanceStop>>,
}

struct PageWindow {
    limits: BTreeMap<WorkKind, usize>,
    total_duration: Duration,
    remaining_duration: Duration,
    active_since: Option<Instant>,
}

impl Drop for RequestAllowance {
    fn drop(&mut self) {
        if let Some(tree) = &self.tree_memory {
            tree.release(self.reserved_bytes);
        }
    }
}

/// A single MCTS tree's live retained and temporary byte reservations. Each
/// request still applies its own independent peak cap before reaching this
/// aggregate cap. The counters are shared by clone, not reset per node.
#[derive(Clone, Debug)]
pub struct SearchMemoryBudget {
    inner: Arc<SearchMemoryBudgetInner>,
}

#[derive(Debug)]
struct SearchMemoryBudgetInner {
    limit: usize,
    live: AtomicUsize,
    peak: AtomicUsize,
}

impl SearchMemoryBudget {
    pub fn new(limit: usize) -> Self {
        assert!(limit > 0, "aggregate search memory cap must be positive");
        Self {
            inner: Arc::new(SearchMemoryBudgetInner {
                limit,
                live: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
            }),
        }
    }

    pub fn reserve(&self, bytes: usize) -> Result<(), AllowanceStop> {
        let mut live = self.inner.live.load(Ordering::Relaxed);
        loop {
            let Some(next) = live.checked_add(bytes) else {
                return Err(self.stop(live, bytes));
            };
            if next > self.inner.limit {
                return Err(self.stop(live, bytes));
            }
            match self.inner.live.compare_exchange_weak(
                live,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.inner.peak.fetch_max(next, Ordering::AcqRel);
                    return Ok(());
                }
                Err(actual) => live = actual,
            }
        }
    }

    pub fn release(&self, bytes: usize) {
        let old = self.inner.live.fetch_sub(bytes, Ordering::AcqRel);
        assert!(
            old >= bytes,
            "aggregate memory release must match a reservation"
        );
    }

    pub fn live_bytes(&self) -> usize {
        self.inner.live.load(Ordering::Acquire)
    }

    pub fn peak_bytes(&self) -> usize {
        self.inner.peak.load(Ordering::Acquire)
    }

    pub fn limit_bytes(&self) -> usize {
        self.inner.limit
    }

    fn stop(&self, live: usize, attempted: usize) -> AllowanceStop {
        AllowanceStop {
            reason: "aggregate-memory-limit",
            category: None,
            limit: self.inner.limit,
            used: live,
            deadline_overshoot_ms: 0,
            attempted_bytes: Some(attempted),
            aggregate_peak_bytes: Some(self.peak_bytes()),
        }
    }
}

#[derive(Debug)]
pub enum RequestFailure {
    Invalid(Diagnostic),
    Stopped(AllowanceStop),
    PendingPlacement { endpoints: Vec<String> },
    NoCompatibleEndpoints { connection: String },
}

impl From<Diagnostic> for RequestFailure {
    fn from(value: Diagnostic) -> Self {
        Self::Invalid(value)
    }
}

impl From<AllowanceStop> for RequestFailure {
    fn from(value: AllowanceStop) -> Self {
        Self::Stopped(value)
    }
}

impl RequestFailure {
    fn into_diagnostic(self, connection: &str) -> Diagnostic {
        match self {
            Self::Invalid(diagnostic) => diagnostic,
            Self::Stopped(stop) => error(
                if stop.reason == "deadline" {
                    "route-request-deadline"
                } else {
                    "route-request-work-limit"
                },
                connection,
                format!(
                    "request preparation stopped: {} {:?}, used {}, limit {}",
                    stop.reason, stop.category, stop.used, stop.limit
                ),
            ),
            Self::PendingPlacement { endpoints } => error(
                "route-request-not-ready",
                connection,
                format!(
                    "route endpoint placement is pending: {}",
                    endpoints.join(", ")
                ),
            ),
            Self::NoCompatibleEndpoints { connection } => error(
                "route-no-compatible-endpoints",
                connection,
                "placed route endpoints have no compatible available port pair",
            ),
        }
    }
}

impl RequestAllowance {
    pub fn new(caps: AllowanceCaps) -> Self {
        Self::with_memory_budget(caps, None)
    }

    pub fn with_memory_budget(
        caps: AllowanceCaps,
        tree_memory: Option<SearchMemoryBudget>,
    ) -> Self {
        let deadline = Instant::now() + Duration::from_millis(caps.deadline_ms);
        Self {
            caps,
            used: BTreeMap::new(),
            deadline,
            paused_at: None,
            reserved_bytes: 0,
            peak_live_bytes: 0,
            tree_memory,
            page_window: None,
            last_stop: std::cell::RefCell::new(None),
        }
    }

    pub fn caps(&self) -> &AllowanceCaps {
        &self.caps
    }

    pub fn remaining(&self, kind: WorkKind) -> usize {
        self.limit(kind)
            .saturating_sub(self.used.get(&kind).copied().unwrap_or(0))
    }

    /// Work available to this page, including the request's hard remainder.
    pub fn page_remaining(&self, kind: WorkKind) -> usize {
        let hard = self.remaining(kind);
        self.page_window
            .as_ref()
            .and_then(|window| window.limits.get(&kind))
            .map_or(hard, |&limit| {
                hard.min(limit.saturating_sub(self.used.get(&kind).copied().unwrap_or(0)))
            })
    }

    /// Whether this request's own retained reservations leave room for one
    /// more allocation after every competing request has released its bytes.
    pub fn aggregate_reservation_can_fit_if_others_release(
        &self,
        bytes: usize,
        pinned_external_bytes: usize,
    ) -> bool {
        self.tree_memory.as_ref().is_none_or(|tree| {
            self.reserved_bytes
                .checked_add(pinned_external_bytes)
                .and_then(|floor| floor.checked_add(bytes))
                .is_some_and(|floor| floor <= tree.limit_bytes())
        })
    }

    pub(crate) fn stable_peak_cap(&self, pinned_external_bytes: usize) -> usize {
        self.caps
            .peak_live_bytes
            .min(self.tree_memory.as_ref().map_or(usize::MAX, |tree| {
                tree.limit_bytes().saturating_sub(pinned_external_bytes)
            }))
    }

    pub(crate) fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// A first-stage page gets a temporary charged-work window. Quotas follow
    /// `CUMULATIVE_WORK_KINDS` order, so no cumulative category can be omitted.
    /// This does not change the hard caps; clear it when the stage yields.
    pub fn begin_page_window(
        &mut self,
        quotas: [usize; CUMULATIVE_WORK_KINDS.len()],
        duration: Duration,
    ) {
        assert!(self.page_window.is_none(), "page window already active");
        assert!(
            self.paused_at.is_none(),
            "resume before opening a page window"
        );
        let started = Instant::now();
        let limits = CUMULATIVE_WORK_KINDS
            .into_iter()
            .zip(quotas)
            .map(|(kind, quota)| {
                (
                    kind,
                    self.used
                        .get(&kind)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(quota),
                )
            })
            .collect();
        self.page_window = Some(PageWindow {
            limits,
            total_duration: duration,
            remaining_duration: duration,
            active_since: Some(started),
        });
    }

    /// Freeze only the temporary first-turn clock. The absolute work limits
    /// remain attached to this window across other tasks' preparation.
    pub fn suspend_page_window(&mut self) {
        let window = self.page_window.as_mut().expect("page window is active");
        let started = window.active_since.take().expect("page window is running");
        window.remaining_duration = window
            .remaining_duration
            .saturating_sub(Instant::now().duration_since(started));
    }

    pub fn resume_page_window(&mut self) {
        assert!(self.paused_at.is_none(), "resume allowance first");
        let window = self.page_window.as_mut().expect("page window exists");
        assert!(
            window.active_since.is_none(),
            "page window is already running"
        );
        window.active_since = Some(Instant::now());
    }

    pub fn page_window_remaining(&self) -> Option<Duration> {
        self.page_window.as_ref().map(|window| {
            window
                .active_since
                .map_or(window.remaining_duration, |started| {
                    window
                        .remaining_duration
                        .saturating_sub(Instant::now().duration_since(started))
                })
        })
    }

    pub fn end_page_window(&mut self) {
        self.page_window = None;
    }

    fn page_work_stop(&self, kind: WorkKind, used: usize) -> Option<AllowanceStop> {
        let limit = *self.page_window.as_ref()?.limits.get(&kind)?;
        (used > limit).then_some(AllowanceStop {
            reason: "page-quantum",
            category: Some(kind),
            limit,
            used,
            deadline_overshoot_ms: 0,
            attempted_bytes: None,
            aggregate_peak_bytes: None,
        })
    }

    /// A retained candidate pager charges only active page work to its request
    /// deadline. The caller resumes before entering a native search kernel.
    pub fn pause(&mut self) -> Result<(), AllowanceStop> {
        assert!(
            self.page_window
                .as_ref()
                .is_none_or(|window| window.active_since.is_none()),
            "suspend the page window before pausing"
        );
        self.check_hard_deadline()?;
        self.paused_at.get_or_insert_with(Instant::now);
        Ok(())
    }

    pub fn resume(&mut self) {
        if let Some(paused_at) = self.paused_at.take() {
            self.deadline += Instant::now().duration_since(paused_at);
        }
    }

    pub fn last_stop(&self) -> Option<AllowanceStop> {
        self.last_stop.borrow().clone()
    }

    /// Clear a previously reported stop before a retained kernel is retried.
    /// The caller has already recorded that earlier stop in its task report.
    pub fn clear_last_stop(&self) {
        *self.last_stop.borrow_mut() = None;
    }

    pub fn confirm_hard_stop(&self, stop: &AllowanceStop) {
        if stop.reason != "page-quantum" {
            *self.last_stop.borrow_mut() = Some(stop.clone());
        }
    }

    fn note_stop(&self, stop: AllowanceStop) -> AllowanceStop {
        *self.last_stop.borrow_mut() = Some(stop.clone());
        stop
    }

    fn assert_active(&self) {
        assert!(self.paused_at.is_none(), "resume allowance before work");
        assert!(
            self.page_window
                .as_ref()
                .is_none_or(|window| window.active_since.is_some()),
            "resume page window before work"
        );
    }

    fn check_hard_deadline(&self) -> Result<(), AllowanceStop> {
        if let Some(overrun) = self
            .paused_at
            .unwrap_or_else(Instant::now)
            .checked_duration_since(self.deadline)
        {
            return Err(self.note_stop(AllowanceStop {
                reason: "deadline",
                category: None,
                limit: self.caps.deadline_ms as usize,
                used: self
                    .caps
                    .deadline_ms
                    .saturating_add(overrun.as_millis() as u64) as usize,
                deadline_overshoot_ms: overrun.as_millis() as u64,
                attempted_bytes: None,
                aggregate_peak_bytes: None,
            }));
        }
        Ok(())
    }

    pub fn check_deadline(&self) -> Result<(), AllowanceStop> {
        self.assert_active();
        self.check_hard_deadline()?;
        if let Some(window) = &self.page_window {
            if let Some(started) = window.active_since {
                let elapsed_segment = Instant::now().duration_since(started);
                if elapsed_segment >= window.remaining_duration {
                    let used = window
                        .total_duration
                        .saturating_sub(window.remaining_duration)
                        .saturating_add(elapsed_segment);
                    return Err(AllowanceStop {
                        reason: "page-quantum",
                        category: None,
                        limit: window.total_duration.as_millis() as usize,
                        used: used.as_millis() as usize,
                        deadline_overshoot_ms: 0,
                        attempted_bytes: None,
                        aggregate_peak_bytes: None,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn charge(&mut self, kind: WorkKind, count: usize) -> Result<(), AllowanceStop> {
        self.check_deadline()?;
        let limit = self.limit(kind);
        let old = self.used.get(&kind).copied().unwrap_or(0);
        let used = old.saturating_add(count);
        if used > limit {
            return Err(self.note_stop(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used: old,
                deadline_overshoot_ms: 0,
                attempted_bytes: None,
                aggregate_peak_bytes: None,
            }));
        }
        if let Some(stop) = self.page_work_stop(kind, used) {
            return Err(stop);
        }
        self.used.insert(kind, used);
        Ok(())
    }

    /// Account work a bounded kernel has already performed. Such kernels may
    /// report one final probe beyond a cap; retain that real use in diagnostics.
    pub fn record_completed(&mut self, kind: WorkKind, count: usize) -> Result<(), AllowanceStop> {
        self.assert_active();
        let limit = self.limit(kind);
        let used = self
            .used
            .get(&kind)
            .copied()
            .unwrap_or(0)
            .saturating_add(count);
        self.used.insert(kind, used);
        if used > limit {
            return Err(self.note_stop(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used,
                deadline_overshoot_ms: 0,
                attempted_bytes: None,
                aggregate_peak_bytes: None,
            }));
        }
        if let Some(stop) = self.page_work_stop(kind, used) {
            return Err(stop);
        }
        self.check_deadline()
    }

    /// Record a live high-water mark, such as A* frontier occupancy. Searches
    /// run sequentially, so their peaks do not add to one another.
    pub fn record_peak(&mut self, kind: WorkKind, count: usize) -> Result<(), AllowanceStop> {
        self.assert_active();
        let limit = self.limit(kind);
        let used = self.used.get(&kind).copied().unwrap_or(0).max(count);
        self.used.insert(kind, used);
        if used > limit {
            return Err(self.note_stop(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used,
                deadline_overshoot_ms: 0,
                attempted_bytes: None,
                aggregate_peak_bytes: None,
            }));
        }
        self.check_deadline()
    }

    pub fn exhausted_stop(&self, kind: WorkKind) -> AllowanceStop {
        self.note_stop(AllowanceStop {
            reason: "work-limit",
            category: Some(kind),
            limit: self.limit(kind),
            used: self.used.get(&kind).copied().unwrap_or(0),
            deadline_overshoot_ms: 0,
            attempted_bytes: None,
            aggregate_peak_bytes: None,
        })
    }

    pub fn preflight(&self, kind: WorkKind, upper_bound: usize) -> Result<(), AllowanceStop> {
        self.check_deadline()?;
        let limit = self.limit(kind);
        let used = self
            .used
            .get(&kind)
            .copied()
            .unwrap_or(0)
            .saturating_add(upper_bound);
        if used > limit {
            return Err(self.note_stop(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used: self.used.get(&kind).copied().unwrap_or(0),
                deadline_overshoot_ms: 0,
                attempted_bytes: None,
                aggregate_peak_bytes: None,
            }));
        }
        if let Some(stop) = self.page_work_stop(kind, used) {
            return Err(stop);
        }
        Ok(())
    }

    pub fn reserve_peak(&mut self, bytes: usize) -> Result<(), AllowanceStop> {
        self.check_deadline()?;
        self.reserve_peak_for_bounded_kernel(bytes)
    }

    /// A retained kernel checks both clocks before each bounded atomic step.
    /// Reserving that step cannot turn an in-flight page yield into a hard
    /// memory failure; the caller meters elapsed time after the step.
    pub fn reserve_peak_for_bounded_kernel(&mut self, bytes: usize) -> Result<(), AllowanceStop> {
        self.assert_active();
        let peak = self.reserved_bytes.saturating_add(bytes);
        if peak > self.caps.peak_live_bytes {
            return Err(self.note_stop(AllowanceStop {
                reason: "peak-memory-limit",
                category: None,
                limit: self.caps.peak_live_bytes,
                used: self.reserved_bytes,
                deadline_overshoot_ms: 0,
                attempted_bytes: Some(bytes),
                aggregate_peak_bytes: None,
            }));
        }
        if let Some(tree) = &self.tree_memory {
            tree.reserve(bytes).map_err(|stop| self.note_stop(stop))?;
        }
        self.reserved_bytes = peak;
        self.peak_live_bytes = self.peak_live_bytes.max(peak);
        Ok(())
    }

    pub fn release_peak(&mut self, bytes: usize) {
        self.reserved_bytes = self
            .reserved_bytes
            .checked_sub(bytes)
            .expect("request reservation release must match a prior reserve");
        if let Some(tree) = &self.tree_memory {
            tree.release(bytes);
        }
    }

    pub fn usage(&self) -> AllowanceUsage {
        AllowanceUsage {
            categories: self.used.clone(),
            peak_live_bytes: self.peak_live_bytes,
            deadline_overshoot_ms: self
                .paused_at
                .unwrap_or_else(Instant::now)
                .checked_duration_since(self.deadline)
                .map_or(0, |duration| duration.as_millis() as u64),
        }
    }

    fn limit(&self, kind: WorkKind) -> usize {
        match kind {
            WorkKind::EndpointVisits => self.caps.endpoint_visits,
            WorkKind::RasterCells => self.caps.raster_cells,
            WorkKind::GraphRows => self.caps.graph_rows,
            WorkKind::RouteCells => self.caps.route_cells,
            WorkKind::BlockedVisits => self.caps.blocked_visits,
            WorkKind::CopiedBytes => self.caps.copied_bytes,
            WorkKind::InferenceElements => self.caps.inference_elements,
            WorkKind::Expansions => self.caps.expansions,
            WorkKind::FrontierEntries => self.caps.frontier_entries,
            WorkKind::RoutingScans => self.caps.routing_scans,
            WorkKind::ReplayRouteCells => self.caps.replay_route_cells,
            WorkKind::ReplayUsers => self.caps.replay_users,
            WorkKind::CatalogVisits => self.caps.catalog_visits,
            WorkKind::ValidationRecords => self.caps.validation_records,
            WorkKind::PublishedBytes => self.caps.published_bytes,
            WorkKind::Candidates => self.caps.max_candidates,
            WorkKind::Variants => self.caps.max_variants,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub state_revision: u64,
    pub connection: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NetworkRequest {
    pub state_revision: u64,
    pub connections: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Guidance {
    pub schema: String,
    pub state_revision: u64,
    pub connection: String,
    pub model_version: String,
    pub heads: Vec<GuidanceHead>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GuidanceHead {
    pub id: String,
    pub directed_costs: Vec<DirectedCost>,
    pub predicted_success: f32,
    pub uncertainty: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectedCost {
    pub point: Point,
    pub direction: usize,
    pub cost: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct GuidanceInput {
    pub schema: &'static str,
    pub state_revision: u64,
    pub connection: String,
    pub catalog_identity: String,
    pub bounds: Bounds,
    pub channels: usize,
    pub bridge_resources: Vec<Bridge>,
    /// Channel-major `[channel][y][x]` values.
    pub grid: Vec<f32>,
    pub graph: std::sync::Arc<super::learning::Input>,
    /// Unfinished obligations waiting for a block or terminal pose. Their
    /// identities remain in the graph even though they have no raster ports.
    pub pending_connections: Vec<String>,
    #[serde(skip)]
    pub reserved_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteReadiness {
    Ready,
    PendingPlacement { endpoints: Vec<String> },
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Termination {
    CandidateLimit,
    SearchLimit,
    WorkBudget,
    ResourceLimit,
    FrontierExhausted,
    NoCompatibleEndpoints,
    WorkLimit,
    Deadline,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Provenance {
    Anchor,
    Guided {
        model_version: String,
        head: String,
    },
    Diversified {
        model_version: Option<String>,
        head: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: usize,
    pub state_revision: u64,
    pub connection: String,
    pub source_port: Option<String>,
    pub target_port: Option<String>,
    pub cells: Vec<Point>,
    pub commands: Vec<Command>,
    #[serde(default)]
    pub crossings: Vec<Crossing>,
    pub steps: i64,
    pub turns: i64,
    pub expanded: usize,
    pub overlap_with_previous: usize,
    pub learned_cost: u64,
    pub predicted_success: Option<f32>,
    pub guidance_uncertainty: Option<f32>,
    pub provenance: Provenance,
    /// Other search modes that independently found the same physical witness.
    pub additional_provenance: Vec<Provenance>,
    pub physical_cost: PhysicalCost,
    /// Cumulative charged work for this logical request through this witness.
    pub charged_work: AllowanceUsage,
}

/// One atomic same-commodity network witness. Its member routes were generated
/// and replayed in order, so later routes see exact residual shared capacity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkCandidate {
    pub id: usize,
    pub state_revision: u64,
    pub commodity: String,
    pub transport: TransportKind,
    pub routes: Vec<Candidate>,
    pub steps: i64,
    pub turns: i64,
    pub expanded: usize,
    pub shared_cells: usize,
    pub physical_cost: PhysicalCost,
    pub charged_work: AllowanceUsage,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Rejection {
    pub source_port: Option<String>,
    pub target_port: Option<String>,
    pub diagnostic: Diagnostic,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Report {
    pub schema: &'static str,
    pub state_revision: u64,
    pub connection: String,
    pub termination: Termination,
    pub fixed_request_proven_infeasible: bool,
    pub endpoint_pairs: usize,
    pub searches: usize,
    pub expanded: usize,
    pub crossing_limit: usize,
    pub preflight_expanded: usize,
    pub guides_found: usize,
    pub duplicate_guides: usize,
    pub native_rejections: Vec<Rejection>,
    pub minimum_pairwise_dissimilarity: Option<f64>,
    pub mean_pairwise_dissimilarity: Option<f64>,
    pub candidates: Vec<Candidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance: Option<AllowanceUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance_stop: Option<AllowanceStop>,
}

#[derive(Clone)]
struct Endpoint {
    port: Option<String>,
    point: Point,
    direction: Option<usize>,
}

#[derive(Clone)]
struct EndpointPair {
    source: Endpoint,
    target: Endpoint,
}

type EndpointPairKey = (i64, Option<String>, Option<String>);

fn retained_endpoint_pair_bytes(pairs: &Vec<EndpointPair>) -> usize {
    pairs
        .capacity()
        .saturating_mul(std::mem::size_of::<EndpointPair>())
        .saturating_add(
            pairs
                .iter()
                .flat_map(|pair| [&pair.source, &pair.target])
                .filter_map(|endpoint| endpoint.port.as_ref())
                .map(String::len)
                .sum::<usize>(),
        )
}

pub(super) fn bridge_copy_bytes(bridge: &Bridge) -> usize {
    std::mem::size_of::<Bridge>()
        .saturating_add(bridge.id.len())
        .saturating_add(bridge.component.len())
        .saturating_add(
            bridge
                .channels
                .iter()
                .map(|channel| {
                    channel
                        .commodity
                        .len()
                        .saturating_add(channel.connections.iter().map(String::len).sum::<usize>())
                        .saturating_add(
                            channel
                                .connections
                                .len()
                                .saturating_mul(std::mem::size_of::<String>()),
                        )
                })
                .sum::<usize>(),
        )
}

/// Owns one endpoint domain across temporary page stops.
struct EndpointDomainCursor {
    endpoint: String,
    expected: FacilityPortDirection,
    port_index: usize,
    endpoints: Vec<Endpoint>,
}

impl EndpointDomainCursor {
    fn advance(
        &mut self,
        session: &Session,
        connection: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Endpoint>, RequestFailure> {
        let obligation = &session.problem.connections[connection];
        let Some(&block) = session.blocks.get(&self.endpoint) else {
            allowance.charge(WorkKind::EndpointVisits, 1)?;
            let (point, _) =
                session.route_endpoint(&self.endpoint, None, self.expected, obligation)?;
            return Ok(vec![Endpoint {
                port: None,
                point,
                direction: None,
            }]);
        };
        let pose = session.poses[block].as_ref().ok_or_else(|| {
            error(
                "unplaced-endpoint",
                &self.endpoint,
                "route endpoint facility is not placed",
            )
        })?;
        let definition = session
            .catalogs
            .facilities
            .facility(&session.problem.blocks[block].facility)
            .expect("admitted facility definition");
        while self.port_index < definition.ports.len() {
            allowance.charge(WorkKind::EndpointVisits, 1)?;
            let port_definition = &definition.ports[self.port_index];
            let port = project_port(definition, port_definition, pose.rotation, pose.position);
            let available = {
                if port.direction != self.expected || port.transport != obligation.transport {
                    false
                } else {
                    session
                        .used_ports
                        .get(&(block, port.id.clone()))
                        .is_none_or(|used| {
                            let Ok(total) = used.rate.checked_add(obligation.rate) else {
                                return false;
                            };
                            let capacity =
                                session.catalogs.transports.capacity(obligation.transport);
                            used.commodity == obligation.commodity
                                && used.transport == obligation.transport
                                && used.direction == self.expected
                                && within_capacity(total, capacity.quantity, capacity.duration_ms)
                        })
                }
            };
            if available {
                self.endpoints.push(Endpoint {
                    port: Some(port.id),
                    point: port.connection,
                    direction: Some(match self.expected {
                        FacilityPortDirection::Output => edge_index(port.edge),
                        FacilityPortDirection::Input => opposite(port.edge),
                    }),
                });
            }
            self.port_index += 1;
        }
        self.endpoints
            .sort_by(|left, right| left.port.cmp(&right.port));
        Ok(std::mem::take(&mut self.endpoints))
    }
}

/// Owns endpoint domains and their Cartesian scan so a temporary page stop
/// cannot charge a visit twice or discard an already ordered pair.
struct EndpointPairCursor {
    source_domain: Option<EndpointDomainCursor>,
    target_domain: Option<EndpointDomainCursor>,
    sources: Vec<Endpoint>,
    targets: Vec<Endpoint>,
    ordered: BTreeMap<(i64, Option<String>, Option<String>), EndpointPair>,
    source_index: usize,
    target_index: usize,
    visit_charged: bool,
}

impl EndpointPairCursor {
    fn new(session: &Session, connection: usize) -> Self {
        let obligation = &session.problem.connections[connection];
        let capacity = |endpoint: &str| {
            session.blocks.get(endpoint).map_or(1, |&block| {
                session
                    .catalogs
                    .facilities
                    .facility(&session.problem.blocks[block].facility)
                    .expect("admitted facility")
                    .ports
                    .len()
            })
        };
        Self {
            source_domain: Some(EndpointDomainCursor {
                endpoint: obligation.source.clone(),
                expected: FacilityPortDirection::Output,
                port_index: 0,
                endpoints: Vec::with_capacity(capacity(&obligation.source)),
            }),
            target_domain: Some(EndpointDomainCursor {
                endpoint: obligation.target.clone(),
                expected: FacilityPortDirection::Input,
                port_index: 0,
                endpoints: Vec::with_capacity(capacity(&obligation.target)),
            }),
            sources: Vec::new(),
            targets: Vec::new(),
            ordered: BTreeMap::new(),
            source_index: 0,
            target_index: 0,
            visit_charged: false,
        }
    }

    fn advance(
        &mut self,
        session: &Session,
        connection: usize,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<EndpointPair>, RequestFailure> {
        if let Some(domain) = self.source_domain.as_mut() {
            self.sources = domain.advance(session, connection, allowance)?;
            self.source_domain = None;
        }
        if let Some(domain) = self.target_domain.as_mut() {
            self.targets = domain.advance(session, connection, allowance)?;
            self.target_domain = None;
        }
        self.advance_pairs(bounds, allowance)
            .map_err(RequestFailure::Stopped)
    }

    fn advance_pairs(
        &mut self,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<EndpointPair>, AllowanceStop> {
        if self.targets.is_empty() {
            return Ok(Vec::new());
        }
        while self.source_index < self.sources.len() {
            let source = &self.sources[self.source_index];
            let target = &self.targets[self.target_index];
            if !self.visit_charged {
                allowance.charge(WorkKind::EndpointVisits, 1)?;
                self.visit_charged = true;
            }
            if bounds.contains(source.point) && bounds.contains(target.point) {
                allowance.charge(WorkKind::RoutingScans, 1)?;
                self.ordered.insert(
                    (
                        source.point.distance(target.point),
                        source.port.clone(),
                        target.port.clone(),
                    ),
                    EndpointPair {
                        source: source.clone(),
                        target: target.clone(),
                    },
                );
            }
            self.visit_charged = false;
            self.target_index += 1;
            if self.target_index == self.targets.len() {
                self.target_index = 0;
                self.source_index += 1;
            }
        }
        let mut pairs = Vec::with_capacity(self.ordered.len());
        pairs.extend(std::mem::take(&mut self.ordered).into_values());
        Ok(pairs)
    }
}

#[cfg(test)]
mod endpoint_pair_cursor_tests {
    use super::*;

    #[test]
    fn long_port_ids_are_included_in_pair_peak_before_allocation() {
        use aic_layout::facilities::{FacilityCatalog, ValidatedFacilityCatalog};
        use std::{fs::File, path::Path};

        let mut session = super::super::tests::session();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized");
        let mut catalog: FacilityCatalog =
            serde_json::from_reader(File::open(root.join("facilities.json")).unwrap()).unwrap();
        let facility = catalog
            .facilities
            .iter_mut()
            .find(|facility| facility.id == "grinder-1-mode-normal")
            .unwrap();
        let port = facility.ports.first_mut().unwrap();
        let long_id = "p".repeat(16 * 1024);
        port.id = long_id.clone();
        Arc::get_mut(&mut session.catalogs).unwrap().facilities =
            ValidatedFacilityCatalog::try_from_catalog(catalog).unwrap();
        super::super::tests::place_pair(&mut session);

        let reserved = session.endpoint_pair_reservation_bytes(0);
        assert!(reserved > long_id.len() * 2);
        let mut insufficient = AllowanceCaps::default();
        insufficient.peak_live_bytes = reserved - 1;
        assert!(
            RequestAllowance::new(insufficient)
                .reserve_peak(reserved)
                .is_err()
        );

        let mut exact = AllowanceCaps::default();
        exact.peak_live_bytes = reserved;
        let mut allowance = RequestAllowance::new(exact);
        allowance.reserve_peak(reserved).unwrap();
        let pairs = session
            .endpoint_pairs(
                0,
                Bounds {
                    min_x: 0,
                    min_y: 0,
                    max_x: 15,
                    max_y: 15,
                },
                &mut allowance,
            )
            .unwrap();
        assert!(!pairs.is_empty());
        assert!(pairs.iter().any(|pair| {
            pair.source.port.as_deref() == Some(long_id.as_str())
                || pair.target.port.as_deref() == Some(long_id.as_str())
        }));
        let retained = retained_endpoint_pair_bytes(&pairs);
        drop(pairs);
        allowance.release_peak(reserved);
        assert_eq!(allowance.reserved_bytes, 0);

        let config = super::super::tests::route_candidate_config();
        let request = Request {
            state_revision: session.revision(),
            connection: "connection-0".into(),
        };
        let mut pager_allowance = RequestAllowance::new(config.request_allowance.clone());
        let pager = RouteCandidatePager::new(
            &session,
            request,
            config,
            None,
            vec![7],
            &mut pager_allowance,
        )
        .unwrap();
        assert!(pager_allowance.reserved_bytes >= retained);
        pager.release(&mut pager_allowance);
        assert_eq!(pager_allowance.reserved_bytes, 0);
    }

    #[test]
    fn temporary_domain_yield_preserves_ports_and_endpoint_work() {
        let mut session = super::super::tests::session();
        super::super::tests::place_pair(&mut session);
        let connection = 0;
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 15,
            max_y: 15,
        };
        let mut uninterrupted = RequestAllowance::new(Default::default());
        let expected = session
            .endpoint_pairs(connection, bounds, &mut uninterrupted)
            .unwrap();
        assert!(!expected.is_empty());

        let mut cursor = EndpointPairCursor::new(&session, connection);
        let mut allowance = RequestAllowance::new(Default::default());
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let visit_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::EndpointVisits)
            .unwrap();
        quotas[visit_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        assert_eq!(allowance.page_remaining(WorkKind::EndpointVisits), 1);
        let stop = cursor
            .advance(&session, connection, bounds, &mut allowance)
            .err()
            .expect("the first page must stop inside a port domain");
        assert!(matches!(stop, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum"));
        assert_eq!(cursor.source_domain.as_ref().unwrap().port_index, 1);
        assert_eq!(allowance.page_remaining(WorkKind::EndpointVisits), 0);
        allowance.end_page_window();
        assert!(allowance.page_remaining(WorkKind::EndpointVisits) > 0);
        let actual = cursor
            .advance(&session, connection, bounds, &mut allowance)
            .unwrap();
        let signature = |pairs: Vec<EndpointPair>| {
            pairs
                .into_iter()
                .map(|pair| {
                    (
                        pair.source.port,
                        pair.source.point,
                        pair.target.port,
                        pair.target.point,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(signature(actual), signature(expected));
        assert_eq!(
            allowance.usage().categories,
            uninterrupted.usage().categories
        );
    }

    #[test]
    fn temporary_scan_yield_keeps_ordered_pair_and_prior_visit_charge() {
        let mut cursor = EndpointPairCursor {
            source_domain: None,
            target_domain: None,
            sources: vec![Endpoint {
                port: Some("output".into()),
                point: Point { x: 0, y: 0 },
                direction: Some(0),
            }],
            targets: vec![Endpoint {
                port: Some("input".into()),
                point: Point { x: 1, y: 0 },
                direction: Some(2),
            }],
            ordered: BTreeMap::new(),
            source_index: 0,
            target_index: 0,
            visit_charged: false,
        };
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 1,
            max_y: 1,
        };
        let mut allowance = RequestAllowance::new(Default::default());
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let scan_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::RoutingScans)
            .unwrap();
        quotas[scan_slot] = 0;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let stop = match cursor.advance_pairs(bounds, &mut allowance) {
            Err(stop) => stop,
            Ok(_) => panic!("temporary scan window must yield"),
        };
        assert_eq!(stop.reason, "page-quantum");
        assert_eq!(stop.category, Some(WorkKind::RoutingScans));
        assert_eq!(allowance.usage().categories[&WorkKind::EndpointVisits], 1);
        allowance.end_page_window();
        let pairs = cursor.advance_pairs(bounds, &mut allowance).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source.port.as_deref(), Some("output"));
        assert_eq!(pairs[0].target.port.as_deref(), Some("input"));
        assert_eq!(allowance.usage().categories[&WorkKind::EndpointVisits], 1);
        assert_eq!(allowance.usage().categories[&WorkKind::RoutingScans], 1);
    }
}

impl Config {
    fn validate(&self) -> Result<(), Diagnostic> {
        let width = self
            .bounds
            .max_x
            .checked_sub(self.bounds.min_x)
            .and_then(|value| value.checked_add(1));
        let height = self
            .bounds
            .max_y
            .checked_sub(self.bounds.min_y)
            .and_then(|value| value.checked_add(1));
        let area = width.and_then(|width| height.and_then(|height| width.checked_mul(height)));
        if width.is_none_or(|width| width <= 0)
            || height.is_none_or(|height| height <= 0)
            || area.is_none_or(|area| area > 250_000)
            || !(1..=64).contains(&self.max_candidates)
            || !(1..=4_096).contains(&self.max_searches)
            || !(1..=10_000_000).contains(&self.work_per_search)
            || !(1..=100_000_000).contains(&self.max_expanded)
            || !(1..=1_000_000).contains(&self.overlap_penalty)
            || area.is_some_and(|area| self.max_crossings_per_route > area as usize)
            || !(1..=3_600_000).contains(&self.request_allowance.deadline_ms)
            || self.request_allowance.max_inference_batch == 0
            || self.request_allowance.max_inference_batch > 64
            || self.request_allowance.peak_live_bytes == 0
            || [
                self.request_allowance.endpoint_visits,
                self.request_allowance.raster_cells,
                self.request_allowance.graph_rows,
                self.request_allowance.route_cells,
                self.request_allowance.blocked_visits,
                self.request_allowance.copied_bytes,
                self.request_allowance.inference_elements,
                self.request_allowance.expansions,
                self.request_allowance.frontier_entries,
                self.request_allowance.routing_scans,
                self.request_allowance.replay_route_cells,
                self.request_allowance.replay_users,
                self.request_allowance.catalog_visits,
                self.request_allowance.validation_records,
                self.request_allowance.published_bytes,
                self.request_allowance.max_candidates,
                self.request_allowance.max_variants,
            ]
            .contains(&0)
        {
            return Err(error(
                "route-candidate-config",
                "route-candidates",
                "invalid bounds, candidate count, search count, work budget, overlap penalty or crossing limit",
            ));
        }
        Ok(())
    }
}

/// Retains validated guidance rows and the next directed cost across a page stop.
struct GuidanceValidationCursor {
    identity_checked: bool,
    head_index: usize,
    cost_index: usize,
    current: Option<IndexMap<(Point, usize), u32>>,
    maps: Vec<IndexMap<(Point, usize), u32>>,
}

impl GuidanceValidationCursor {
    fn new() -> Self {
        Self {
            identity_checked: false,
            head_index: 0,
            cost_index: 0,
            current: None,
            maps: Vec::new(),
        }
    }

    fn advance(
        &mut self,
        guidance: &Guidance,
        request: &Request,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        let area = usize::try_from(bounds.width())
            .ok()
            .and_then(|width| {
                usize::try_from(bounds.height())
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .unwrap_or(0);
        if !self.identity_checked {
            if guidance.schema != GUIDANCE_SCHEMA
                || guidance.state_revision != request.state_revision
                || guidance.connection != request.connection
                || guidance.model_version.is_empty()
                || guidance.model_version.len() > 256
                || !(1..=8).contains(&guidance.heads.len())
            {
                return Err(error(
                    "route-guidance-identity",
                    &request.connection,
                    "route guidance schema, state, connection, model version or head count is invalid",
                )
                .into());
            }
            self.identity_checked = true;
        }
        while self.head_index < guidance.heads.len() {
            let head = &guidance.heads[self.head_index];
            if self.current.is_none() {
                allowance.check_deadline()?;
                if head.id.is_empty()
                    || head.id.len() > 128
                    || guidance.heads[..self.head_index]
                        .iter()
                        .any(|previous| previous.id == head.id)
                    || !head.predicted_success.is_finite()
                    || !(0.0..=1.0).contains(&head.predicted_success)
                    || !head.uncertainty.is_finite()
                    || !(0.0..=1.0).contains(&head.uncertainty)
                    || head.directed_costs.len() > area.saturating_mul(4)
                {
                    return Err(error(
                        "route-guidance-head",
                        &head.id,
                        "route guidance head metadata or directed-cost count is invalid",
                    )
                    .into());
                }
                self.current = Some(IndexMap::with_capacity(head.directed_costs.len()));
            }
            let map = self.current.as_mut().expect("validated guidance head map");
            while self.cost_index < head.directed_costs.len() {
                allowance.charge(WorkKind::RoutingScans, 1)?;
                let cost = &head.directed_costs[self.cost_index];
                if !bounds.contains(cost.point)
                    || cost.direction > 3
                    || cost.cost > 1_000_000
                    || map
                        .insert((cost.point, cost.direction), cost.cost)
                        .is_some()
                {
                    return Err(error(
                        "route-guidance-cost",
                        &head.id,
                        "route guidance contains an invalid or duplicate directed edge cost",
                    )
                    .into());
                }
                self.cost_index += 1;
            }
            self.maps
                .push(self.current.take().expect("completed guidance head map"));
            self.head_index += 1;
            self.cost_index = 0;
        }
        Ok(())
    }

    fn take_maps(self) -> Vec<IndexMap<(Point, usize), u32>> {
        self.maps
    }
}

impl Guidance {
    fn validate_with_allowance(
        &self,
        request: &Request,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<IndexMap<(Point, usize), u32>>, RequestFailure> {
        let mut cursor = GuidanceValidationCursor::new();
        cursor.advance(self, request, bounds, allowance)?;
        Ok(cursor.take_maps())
    }
}

/// Owns the sorted blocked set and exact visit position across a soft stop.
struct BlockedCellsCursor {
    bodies: Vec<Point>,
    occupied: Vec<Point>,
    body_index: usize,
    terminal_index: usize,
    occupied_index: usize,
    occupied_visit_charged: bool,
    blocked: BTreeSet<(i64, i64)>,
}

impl BlockedCellsCursor {
    fn new(session: &Session, connection: usize) -> Self {
        let transport = session.problem.connections[connection].transport;
        Self {
            bodies: session.bodies.iter().copied().collect(),
            occupied: session.occupied[layer(transport)].keys().copied().collect(),
            body_index: 0,
            terminal_index: 0,
            occupied_index: 0,
            occupied_visit_charged: false,
            blocked: BTreeSet::new(),
        }
    }

    fn advance(
        &mut self,
        session: &Session,
        connection: usize,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<IndexSet<Point>, AllowanceStop> {
        let obligation = &session.problem.connections[connection];
        while self.body_index < self.bodies.len() {
            allowance.charge(WorkKind::BlockedVisits, 1)?;
            let point = self.bodies[self.body_index];
            if bounds.contains(point) {
                self.blocked.insert((point.y, point.x));
            }
            self.body_index += 1;
        }
        while self.terminal_index < session.terminal_poses.len() {
            allowance.charge(WorkKind::BlockedVisits, 1)?;
            let terminal = self.terminal_index;
            let id = &session.problem.terminals[terminal].id;
            if id != &obligation.source && id != &obligation.target {
                if let Some(pose) = &session.terminal_poses[terminal] {
                    if bounds.contains(pose.position) {
                        self.blocked.insert((pose.position.y, pose.position.x));
                    }
                }
            }
            self.terminal_index += 1;
        }
        let occupied = &session.occupied[layer(obligation.transport)];
        while self.occupied_index < self.occupied.len() {
            let point = self.occupied[self.occupied_index];
            let users = &occupied[&point];
            if !self.occupied_visit_charged {
                allowance.charge(WorkKind::BlockedVisits, 1)?;
                self.occupied_visit_charged = true;
            }
            if bounds.contains(point) {
                let blocked_here = if session.bridges.contains_key(&(obligation.transport, point)) {
                    true
                } else {
                    allowance.charge(WorkKind::BlockedVisits, users.len())?;
                    !session.shared_cell_available(point, connection)
                };
                if blocked_here {
                    self.blocked.insert((point.y, point.x));
                }
            }
            self.occupied_visit_charged = false;
            self.occupied_index += 1;
        }
        Ok(std::mem::take(&mut self.blocked)
            .into_iter()
            .map(|(y, x)| Point { x, y })
            .collect())
    }
}

#[cfg(test)]
mod guidance_validation_cursor_tests {
    use super::*;

    #[test]
    fn temporary_scan_yield_keeps_validated_costs_and_charges_once() {
        let request = Request {
            state_revision: 7,
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
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 3,
            max_y: 3,
        };
        let mut baseline_allowance = RequestAllowance::new(Default::default());
        let expected = guidance
            .validate_with_allowance(&request, bounds, &mut baseline_allowance)
            .unwrap();

        let mut cursor = GuidanceValidationCursor::new();
        let mut allowance = RequestAllowance::new(Default::default());
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let scan_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::RoutingScans)
            .unwrap();
        quotas[scan_slot] = 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let stop = cursor
            .advance(&guidance, &request, bounds, &mut allowance)
            .unwrap_err();
        assert!(matches!(stop, RequestFailure::Stopped(ref stop) if stop.reason == "page-quantum"));
        assert_eq!(cursor.cost_index, 1);
        assert_eq!(cursor.current.as_ref().unwrap().len(), 1);
        allowance.end_page_window();
        cursor
            .advance(&guidance, &request, bounds, &mut allowance)
            .unwrap();
        assert_eq!(cursor.take_maps(), expected);
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
    }
}

#[cfg(test)]
mod blocked_cells_cursor_tests {
    use super::*;

    #[test]
    fn occupied_user_yield_charges_the_cell_once() {
        let mut source = super::super::tests::session();
        super::super::tests::place_pair(&mut source);
        let bounds = super::super::tests::route_candidate_config().bounds;
        let connection = 0;
        let transport = source.problem.connections[connection].transport;
        source.occupied[layer(transport)].insert(Point { x: 5, y: 5 }, vec![connection]);
        let mut baseline_allowance = RequestAllowance::new(Default::default());
        let expected = source
            .route_blocked_cells_bounded(connection, bounds, &mut baseline_allowance)
            .unwrap();

        let mut allowance = RequestAllowance::new(Default::default());
        let mut cursor = BlockedCellsCursor::new(&source, connection);
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let blocked_slot = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|&kind| kind == WorkKind::BlockedVisits)
            .unwrap();
        quotas[blocked_slot] = source.bodies.len() + source.terminal_poses.len() + 1;
        allowance.begin_page_window(quotas, Duration::from_secs(1));
        let stop = cursor
            .advance(&source, connection, bounds, &mut allowance)
            .unwrap_err();
        assert_eq!(stop.reason, "page-quantum");
        assert_eq!(cursor.occupied_index, 0);
        assert!(cursor.occupied_visit_charged);
        allowance.end_page_window();
        let actual = cursor
            .advance(&source, connection, bounds, &mut allowance)
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            allowance.usage().categories,
            baseline_allowance.usage().categories
        );
    }
}

impl Session {
    /// Exact observable delta between this immutable source and a natively
    /// replayed successor for the named changed obligations.
    pub fn physical_cost_between(
        &self,
        after: &Session,
        changed_connections: &[String],
    ) -> Result<PhysicalCost, Diagnostic> {
        physical_cost::physical_cost_between(self, after, changed_connections)
    }

    /// Inspect only placement flags and stable identifiers before allocating
    /// endpoint domains or model inputs.
    pub fn route_request_readiness(&self, request: &Request) -> Result<RouteReadiness, Diagnostic> {
        if request.state_revision != self.revision || self.active.is_some() {
            return Err(error(
                "route-readiness-state",
                &request.connection,
                "route readiness requires the exact stable state revision",
            ));
        }
        let Some(&connection) = self.connections.get(&request.connection) else {
            return Err(error(
                "unknown-connection",
                &request.connection,
                "route readiness references an unknown logical connection",
            ));
        };
        if self.routes[connection].is_some() {
            return Err(error(
                "completed-connection",
                &request.connection,
                "route readiness references a completed logical connection",
            ));
        }
        let obligation = &self.problem.connections[connection];
        let mut pending = Vec::new();
        for endpoint in [&obligation.source, &obligation.target] {
            let placed = if let Some(&block) = self.blocks.get(endpoint) {
                self.poses[block].is_some()
            } else {
                self.terminal_poses[self.terminals[endpoint]].is_some()
            };
            if !placed && !pending.contains(endpoint) {
                pending.push(endpoint.clone());
            }
        }
        if pending.is_empty() {
            Ok(RouteReadiness::Ready)
        } else {
            Ok(RouteReadiness::PendingPlacement { endpoints: pending })
        }
    }

    pub fn route_guidance_input(
        &self,
        request: &Request,
        candidate: &Config,
        learning: &super::learning::Config,
    ) -> Result<GuidanceInput, Diagnostic> {
        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        self.route_guidance_input_with_allowance(request, candidate, learning, &mut allowance)
            .map_err(|failure| failure.into_diagnostic(&request.connection))
    }

    pub fn route_guidance_input_with_allowance(
        &self,
        request: &Request,
        candidate: &Config,
        learning: &super::learning::Config,
        allowance: &mut RequestAllowance,
    ) -> Result<GuidanceInput, RequestFailure> {
        let preparation =
            RouteGuidancePreparation::new(self, request, candidate, learning, allowance)?;
        match preparation.into_input(self, candidate, learning, allowance) {
            Ok(input) => Ok(input),
            Err((preparation, failure)) => {
                preparation.release(allowance);
                Err(failure)
            }
        }
    }

    pub fn route_candidates(
        &self,
        request: &Request,
        config: &Config,
    ) -> Result<Report, Diagnostic> {
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        self.route_candidates_inner(request, config, None, &mut allowance, true, true)
    }

    pub fn route_candidates_with_guidance(
        &self,
        request: &Request,
        config: &Config,
        guidance: &Guidance,
    ) -> Result<Report, Diagnostic> {
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        self.route_candidates_inner(request, config, Some(guidance), &mut allowance, true, true)
    }

    pub fn route_candidates_with_guidance_allowance(
        &self,
        request: &Request,
        config: &Config,
        guidance: &Guidance,
        allowance: &mut RequestAllowance,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, Some(guidance), allowance, true, true)
    }

    pub fn route_candidates_with_allowance(
        &self,
        request: &Request,
        config: &Config,
        allowance: &mut RequestAllowance,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, None, allowance, true, true)
    }

    pub fn route_candidates_for_network_with_allowance(
        &self,
        request: &Request,
        config: &Config,
        guidance: &Guidance,
        allowance: &mut RequestAllowance,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, Some(guidance), allowance, false, true)
    }

    pub fn route_candidates_for_network_anchor_with_allowance(
        &self,
        request: &Request,
        config: &Config,
        allowance: &mut RequestAllowance,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, None, allowance, false, true)
    }

    fn route_candidates_inner(
        &self,
        request: &Request,
        config: &Config,
        guidance: Option<&Guidance>,
        allowance: &mut RequestAllowance,
        publish_candidates: bool,
        replay_candidates: bool,
    ) -> Result<Report, Diagnostic> {
        config.validate()?;
        if request.state_revision != self.revision {
            return Err(error(
                "stale-route-candidate-request",
                &request.connection,
                "routing request belongs to an earlier constructive revision",
            ));
        }
        if self.active.is_some() {
            return Err(error(
                "active-route",
                &request.connection,
                "route candidates require a stable state with no active partial route",
            ));
        }
        let Some(&connection) = self.connections.get(&request.connection) else {
            return Err(error(
                "unknown-connection",
                &request.connection,
                "route candidate request references an unknown logical connection",
            ));
        };
        if self.routes[connection].is_some() {
            return Err(error(
                "completed-connection",
                &request.connection,
                "route candidate request references a completed logical connection",
            ));
        }

        let pair_phase_bytes = self.endpoint_pair_reservation_bytes(connection);
        if let Err(stop) = allowance.reserve_peak(pair_phase_bytes) {
            return Ok(stopped_report(
                request,
                config.max_crossings_per_route,
                0,
                allowance,
                stop,
            ));
        }
        let pair_result = self.endpoint_pairs(connection, config.bounds, allowance);
        let pairs = match pair_result {
            Ok(pairs) => pairs,
            Err(RequestFailure::Invalid(diagnostic)) => {
                allowance.release_peak(pair_phase_bytes);
                return Err(diagnostic);
            }
            Err(RequestFailure::Stopped(stop)) => {
                allowance.release_peak(pair_phase_bytes);
                return Ok(stopped_report(
                    request,
                    config.max_crossings_per_route,
                    0,
                    allowance,
                    stop,
                ));
            }
            Err(RequestFailure::PendingPlacement { endpoints }) => {
                allowance.release_peak(pair_phase_bytes);
                return Err(error(
                    "route-request-not-ready",
                    &request.connection,
                    format!("route endpoints are pending: {}", endpoints.join(", ")),
                ));
            }
            Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                unreachable!("endpoint enumeration returns an empty vector")
            }
        };
        let pair_retained_bytes = retained_endpoint_pair_bytes(&pairs);
        if pair_retained_bytes > pair_phase_bytes {
            if let Err(stop) = allowance.reserve_peak(pair_retained_bytes - pair_phase_bytes) {
                drop(pairs);
                allowance.release_peak(pair_phase_bytes);
                return Ok(stopped_report(
                    request,
                    config.max_crossings_per_route,
                    0,
                    allowance,
                    stop,
                ));
            }
        } else {
            allowance.release_peak(pair_phase_bytes - pair_retained_bytes);
        }
        if pairs.is_empty() {
            drop(pairs);
            allowance.release_peak(pair_retained_bytes);
            let mut report = empty_report(
                request,
                Termination::NoCompatibleEndpoints,
                0,
                false,
                config.max_crossings_per_route,
            );
            report.allowance = Some(allowance.usage());
            return Ok(report);
        }
        let guidance_map_bytes = guidance.map_or(0, |guidance| {
            guidance.heads.iter().fold(0usize, |total, head| {
                total.saturating_add(head.directed_costs.len().saturating_mul(96))
            })
        });
        // The tree used to order blocked sites and its final indexed set may
        // coexist. Keep their reservation through all searches using the set.
        let blocked_sites = self
            .bodies
            .len()
            .saturating_add(self.terminal_poses.len())
            .saturating_add(
                self.occupied[layer(self.problem.connections[connection].transport)].len(),
            );
        let mut carrier_cells = 0usize;
        for route in self.routes.iter().flatten() {
            if let Err(stop) = allowance.charge(WorkKind::ValidationRecords, 1) {
                let pair_count = pairs.len();
                drop(pairs);
                allowance.release_peak(pair_retained_bytes);
                return Ok(stopped_report(
                    request,
                    config.max_crossings_per_route,
                    pair_count,
                    allowance,
                    stop,
                ));
            }
            carrier_cells = carrier_cells.saturating_add(route.cells.len());
        }
        let crossing_bytes = if config.max_crossings_per_route == 0 {
            0
        } else {
            carrier_cells
                .saturating_mul(640)
                .saturating_add(blocked_sites.saturating_mul(64))
                .saturating_add(self.routes.len().saturating_mul(128))
                .saturating_add(self.bridges.len().saturating_mul(512))
                .saturating_add(
                    self.catalogs
                        .components
                        .catalog()
                        .components
                        .len()
                        .saturating_mul(256),
                )
        };
        let mut persistent_bytes = guidance_map_bytes
            .saturating_add(pair_retained_bytes)
            .saturating_add(blocked_sites.saturating_mul(192))
            .saturating_add(crossing_bytes);
        if let Err(stop) = allowance.reserve_peak(persistent_bytes - pair_retained_bytes) {
            let pair_count = pairs.len();
            drop(pairs);
            allowance.release_peak(pair_retained_bytes);
            return Ok(stopped_report(
                request,
                config.max_crossings_per_route,
                pair_count,
                allowance,
                stop,
            ));
        }
        let result = (|| -> Result<Report, Diagnostic> {
            let guidance_maps = match guidance
                .map(|guidance| guidance.validate_with_allowance(request, config.bounds, allowance))
                .transpose()
            {
                Ok(maps) => maps,
                Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic),
                Err(RequestFailure::Stopped(stop)) => {
                    return Ok(stopped_report(
                        request,
                        config.max_crossings_per_route,
                        pairs.len(),
                        allowance,
                        stop,
                    ));
                }
                Err(RequestFailure::PendingPlacement { .. }) => {
                    unreachable!("guidance validation does not inspect placement")
                }
                Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                    unreachable!("guidance validation does not inspect ports")
                }
            };
            let guidance_heads = guidance.map_or(&[][..], |guidance| guidance.heads.as_slice());
            let blocked =
                match self.route_blocked_cells_bounded(connection, config.bounds, allowance) {
                    Ok(blocked) => blocked,
                    Err(stop) => {
                        return Ok(stopped_report(
                            request,
                            config.max_crossings_per_route,
                            pairs.len(),
                            allowance,
                            stop,
                        ));
                    }
                };
            let crossings =
                match route_preparation::route_crossings(self, connection, config, allowance) {
                    Ok(crossings) => crossings,
                    Err(stop) => {
                        return Ok(stopped_report(
                            request,
                            config.max_crossings_per_route,
                            pairs.len(),
                            allowance,
                            stop,
                        ));
                    }
                };
            let mut penalties = IndexMap::<Point, u32>::new();
            let mut semantic = BTreeSet::new();
            let mut candidates = Vec::new();
            let mut rejections = Vec::new();
            let mut searches = 0usize;
            let mut expanded = 0usize;
            let mut preflight_expanded = 0usize;
            let mut guides_found = 0usize;
            let mut duplicate_guides = 0usize;
            let mut anchor_no_path = 0usize;
            let mut anchor_complete = true;
            let mut phase = 0usize;
            let mut allowance_stop = None;
            macro_rules! keep_or_stop {
            ($label:lifetime, $work:expr) => {
                match $work {
                    Ok(value) => value,
                    Err(stop) => {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break $label termination;
                    }
                }
            };
        }
            let termination = 'rounds: loop {
                if let Err(stop) = allowance.check_deadline() {
                    let termination = stop_termination(&stop);
                    allowance_stop = Some(stop);
                    break termination;
                }
                let anchor = phase == 0;
                let guided = phase > 0 && phase <= guidance_heads.len();
                let guidance_index = if guided {
                    Some(phase - 1)
                } else if !anchor && !guidance_heads.is_empty() {
                    Some((phase - guidance_heads.len() - 1) % guidance_heads.len())
                } else {
                    None
                };
                let edge_penalties = guidance_index
                    .and_then(|index| guidance_maps.as_ref().map(|maps| &maps[index]));
                let phase_target = if anchor && !guidance_heads.is_empty() {
                    1
                } else if guided {
                    1 + ((config.max_candidates - 1) * phase).div_ceil(guidance_heads.len())
                } else {
                    config.max_candidates
                }
                .min(config.max_candidates);
                let mut phase_searches = 0usize;
                'phase: for pair in &pairs {
                    if candidates.len() == config.max_candidates {
                        break 'rounds Termination::CandidateLimit;
                    }
                    if candidates.len() >= phase_target {
                        break 'phase;
                    }
                    if searches == config.max_searches {
                        break 'rounds Termination::SearchLimit;
                    }
                    let remaining = config.max_expanded.saturating_sub(expanded);
                    if remaining == 0 {
                        break 'rounds Termination::WorkBudget;
                    }
                    let allowance_remaining = allowance.remaining(WorkKind::Expansions);
                    if allowance_remaining == 0 {
                        let stop = allowance.charge(WorkKind::Expansions, 1).unwrap_err();
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    let work_budget = remaining
                        .min(config.work_per_search)
                        .min(allowance_remaining);
                    let scan_bound = penalties
                        .len()
                        .saturating_add(edge_penalties.map_or(0, |costs| costs.len()));
                    if let Err(stop) = allowance.preflight(WorkKind::RoutingScans, scan_bound) {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    let state_bound = work_budget
                        .saturating_mul(8)
                        .saturating_add(4)
                        .min(1_000_000);
                    let search_bytes = state_bound
                        .saturating_mul(256)
                        .saturating_add(blocked.len().saturating_mul(128))
                        .saturating_add(crossings.len().saturating_mul(256))
                        .saturating_add(pairs.len().saturating_mul(128))
                        .saturating_add(
                            candidates
                                .iter()
                                .map(|candidate: &Candidate| {
                                    candidate.cells.len().saturating_mul(128)
                                })
                                .sum::<usize>(),
                        );
                    if let Err(stop) = allowance.reserve_peak(search_bytes) {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    let result = shortest_path(
                        pair.source.point,
                        pair.target.point,
                        &blocked,
                        config.bounds,
                        PathOptions {
                            first: pair.source.direction,
                            last: pair.target.direction,
                            work_budget,
                            scan_budget: allowance.remaining(WorkKind::RoutingScans).max(1),
                            frontier_budget: allowance.caps().frontier_entries,
                            deadline: Some(allowance.deadline()),
                            crossings: &crossings,
                            crossing_limit: config.max_crossings_per_route,
                            prefer_crossings: phase > 0,
                            allow_zero_length: false,
                            cell_penalties: (!anchor && !guided).then_some(&penalties),
                            edge_penalties,
                        },
                    );
                    allowance.release_peak(search_bytes);
                    let result = result.map_err(|failure| {
                        error(
                            "route-candidate-search",
                            &request.connection,
                            failure.message,
                        )
                    })?;
                    searches += 1;
                    phase_searches += 1;
                    expanded += result.expanded;
                    preflight_expanded += result.preflight_expanded;
                    let scan_stop = allowance
                        .record_completed(WorkKind::RoutingScans, result.routing_scans)
                        .err();
                    let frontier_stop = allowance
                        .record_peak(WorkKind::FrontierEntries, result.frontier_entries_peak)
                        .err();
                    let expansion_stop = allowance
                        .record_completed(WorkKind::Expansions, result.expanded)
                        .err();
                    if let Some(stop) = scan_stop.or(frontier_stop).or(expansion_stop) {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    if result.status == "deadline" {
                        let stop = allowance.check_deadline().unwrap_err();
                        allowance_stop = Some(stop);
                        break 'rounds Termination::Deadline;
                    }
                    if result.status == "scan-budget" || result.status == "frontier-budget" {
                        let kind = if result.status == "scan-budget" {
                            WorkKind::RoutingScans
                        } else {
                            WorkKind::FrontierEntries
                        };
                        let stop = allowance.exhausted_stop(kind);
                        allowance_stop = Some(stop);
                        break 'rounds Termination::WorkLimit;
                    }
                    if let Err(stop) = allowance.check_deadline() {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    if result.status == "resource-limit" {
                        break 'rounds Termination::ResourceLimit;
                    }
                    if result.status == "work-budget" {
                        if anchor {
                            anchor_complete = false;
                        }
                        if expanded >= config.max_expanded {
                            break 'rounds Termination::WorkBudget;
                        }
                        continue;
                    }
                    let Some(cells) = result.cells else {
                        if anchor
                            && matches!(
                                result.status,
                                "no-path" | "blocked-endpoint" | "same-endpoint"
                            )
                        {
                            anchor_no_path += 1;
                        }
                        continue;
                    };
                    let found_crossings = result.crossings.unwrap_or_default();
                    // The path key, penalties, command buffer and candidate
                    // coexist during postprocessing. Keep a conservative
                    // reservation through that phase, then retain only the
                    // entries that remain live for later searches.
                    let postprocess_bytes = cells
                        .len()
                        .saturating_mul(2048)
                        .saturating_add(found_crossings.len().saturating_mul(1024))
                        .saturating_add(4096);
                    keep_or_stop!('rounds, allowance.reserve_peak(postprocess_bytes));
                    persistent_bytes = persistent_bytes.saturating_add(postprocess_bytes);
                    let penalties_before = penalties.len();
                    guides_found += 1;
                    let source_port = pair.source.port.clone();
                    let target_port = pair.target.port.clone();
                    keep_or_stop!('rounds, allowance.charge(WorkKind::RoutingScans, cells.len().saturating_add(found_crossings.len())));
                    keep_or_stop!('rounds, allowance.charge(WorkKind::CopiedBytes, cells.len().saturating_mul(std::mem::size_of::<Point>())));
                    let key = (
                        source_port.clone(),
                        target_port.clone(),
                        cells.clone(),
                        found_crossings
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
                    let overlap =
                        keep_or_stop!('rounds, maximum_overlap(&cells, &candidates, allowance));
                    let learned_cost = if let Some(costs) = edge_penalties {
                        keep_or_stop!('rounds, directed_cost(&cells, costs, allowance))
                    } else {
                        0
                    };
                    keep_or_stop!('rounds, add_overlap_penalty(&mut penalties, &cells, config.overlap_penalty, allowance));
                    let penalty_bytes = penalties
                        .len()
                        .saturating_sub(penalties_before)
                        .saturating_mul(128);
                    if !semantic.insert(key) {
                        duplicate_guides += 1;
                        allowance.release_peak(postprocess_bytes - penalty_bytes);
                        persistent_bytes -= postprocess_bytes - penalty_bytes;
                        continue;
                    }
                    let key_bytes = cells
                        .len()
                        .saturating_mul(64)
                        .saturating_add(found_crossings.len().saturating_mul(256))
                        .saturating_add(
                            source_port
                                .as_ref()
                                .map_or(0, String::len)
                                .saturating_mul(2),
                        )
                        .saturating_add(
                            target_port
                                .as_ref()
                                .map_or(0, String::len)
                                .saturating_mul(2),
                        )
                        .saturating_add(512);
                    let commands = self.route_commands(
                        &request.connection,
                        source_port.clone(),
                        target_port.clone(),
                        &cells,
                        &found_crossings,
                        allowance,
                    );
                    let commands = keep_or_stop!('rounds, commands);
                    let physical_cost = if replay_candidates {
                        match self.replay_route_witness_with_allowance(
                            request.state_revision,
                            &commands,
                            allowance,
                        ) {
                            Ok((after, reserved_bytes)) => {
                                let cost = physical_cost::physical_cost_between(
                                    self,
                                    &after,
                                    std::slice::from_ref(&request.connection),
                                );
                                drop(after);
                                allowance.release_peak(reserved_bytes);
                                cost?
                            }
                            Err(RequestFailure::Invalid(diagnostic)) => {
                                rejections.push(Rejection {
                                    source_port,
                                    target_port,
                                    diagnostic,
                                });
                                let retained =
                                    penalty_bytes.saturating_add(key_bytes).saturating_add(2048);
                                let slack = postprocess_bytes.saturating_sub(retained);
                                allowance.release_peak(slack);
                                persistent_bytes -= slack;
                                continue;
                            }
                            Err(RequestFailure::Stopped(stop)) => {
                                let termination = stop_termination(&stop);
                                allowance_stop = Some(stop);
                                break 'rounds termination;
                            }
                            Err(RequestFailure::PendingPlacement { endpoints }) => {
                                return Err(error(
                                    "route-request-not-ready",
                                    &request.connection,
                                    format!(
                                        "route endpoints are pending: {}",
                                        endpoints.join(", ")
                                    ),
                                ));
                            }
                            Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                                unreachable!("native replay does not prepare endpoints")
                            }
                        }
                    } else {
                        // Internal member witnesses receive their exact net
                        // delta when the owning network/repair draft applies
                        // them to its partial native session.
                        PhysicalCost::default()
                    };
                    candidates.push(Candidate {
                        id: candidates.len(),
                        state_revision: request.state_revision,
                        connection: request.connection.clone(),
                        source_port,
                        target_port,
                        cells,
                        commands,
                        crossings: found_crossings,
                        steps: result.steps.expect("found path has a step count"),
                        turns: result.turns.expect("found path has a turn count"),
                        expanded: result.expanded,
                        overlap_with_previous: overlap,
                        learned_cost,
                        predicted_success: guidance_index
                            .map(|index| guidance_heads[index].predicted_success),
                        guidance_uncertainty: guidance_index
                            .map(|index| guidance_heads[index].uncertainty),
                        provenance: if anchor {
                            Provenance::Anchor
                        } else if guided {
                            let index = guidance_index.expect("guided phase has guidance head");
                            Provenance::Guided {
                                model_version: guidance
                                    .expect("guided phase has guidance")
                                    .model_version
                                    .clone(),
                                head: guidance_heads[index].id.clone(),
                            }
                        } else {
                            Provenance::Diversified {
                                model_version: guidance
                                    .map(|guidance| guidance.model_version.clone()),
                                head: guidance_index.map(|index| guidance_heads[index].id.clone()),
                            }
                        },
                        additional_provenance: Vec::new(),
                        physical_cost,
                        charged_work: allowance.usage(),
                    });
                    let candidate = candidates.last().expect("candidate just pushed");
                    let candidate_bytes = candidate
                        .cells
                        .len()
                        .saturating_mul(64)
                        .saturating_add(candidate.commands.len().saturating_mul(128))
                        .saturating_add(candidate.crossings.len().saturating_mul(256))
                        .saturating_add(1024);
                    let retained = penalty_bytes
                        .saturating_add(key_bytes)
                        .saturating_add(candidate_bytes);
                    let slack = postprocess_bytes.saturating_sub(retained);
                    allowance.release_peak(slack);
                    persistent_bytes -= slack;
                    let publication = if publish_candidates {
                        match json_size_with_allowance(candidate, allowance) {
                            Ok(_) => allowance.charge(WorkKind::Candidates, 1),
                            Err(RequestFailure::Stopped(stop)) => Err(stop),
                            Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic),
                            Err(_) => {
                                unreachable!("candidate serialization does not prepare endpoints")
                            }
                        }
                    } else {
                        let bytes = candidate
                            .cells
                            .len()
                            .saturating_mul(std::mem::size_of::<Point>())
                            .saturating_add(
                                candidate
                                    .commands
                                    .len()
                                    .saturating_mul(std::mem::size_of::<Command>()),
                            )
                            .saturating_add(512);
                        allowance.charge(WorkKind::CopiedBytes, bytes)
                    };
                    if let Err(stop) = publication.and_then(|_| allowance.check_deadline()) {
                        candidates.pop();
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                }
                if anchor && phase_searches < pairs.len() {
                    anchor_complete = false;
                }
                if anchor && anchor_complete && anchor_no_path == pairs.len() {
                    break Termination::FrontierExhausted;
                }
                if searches >= config.max_searches {
                    break Termination::SearchLimit;
                }
                phase += 1;
            };
            let mut termination = termination;
            // A crossing-count cap prunes otherwise legal paths. Exhausting that
            // reduced graph cannot prove the complete fixed request impossible.
            let crossing_domain_complete = if config.max_crossings_per_route == 0 {
                self.routes.iter().all(Option::is_none)
            } else {
                crossings.len() <= config.max_crossings_per_route
            };
            if termination == Termination::FrontierExhausted && !crossing_domain_complete {
                termination = Termination::ResourceLimit;
            }
            let (minimum_pairwise_dissimilarity, mean_pairwise_dissimilarity) =
                match diversity(&candidates, allowance) {
                    Ok(metrics) => metrics,
                    Err(stop) => {
                        termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        (None, None)
                    }
                };
            Ok(Report {
                schema: SCHEMA,
                state_revision: request.state_revision,
                connection: request.connection.clone(),
                termination,
                fixed_request_proven_infeasible: termination == Termination::FrontierExhausted
                    && candidates.is_empty(),
                endpoint_pairs: pairs.len(),
                searches,
                expanded,
                crossing_limit: config.max_crossings_per_route,
                preflight_expanded,
                guides_found,
                duplicate_guides,
                native_rejections: rejections,
                minimum_pairwise_dissimilarity,
                mean_pairwise_dissimilarity,
                candidates,
                allowance: Some(allowance.usage()),
                allowance_stop,
            })
        })();
        drop(pairs);
        allowance.release_peak(persistent_bytes);
        result
    }

    /// Atomically applies a candidate only to the exact state that produced it.
    pub fn apply_route_candidate(
        &mut self,
        candidate: &Candidate,
    ) -> Result<Vec<Step>, Diagnostic> {
        self.apply_route_candidate_checked(candidate, true)
    }

    fn apply_route_candidate_checked(
        &mut self,
        candidate: &Candidate,
        verify_physical_cost: bool,
    ) -> Result<Vec<Step>, Diagnostic> {
        let connection = self.validate_route_candidate_before_replay(candidate)?;
        let (fork, steps) =
            self.replay_route_witness(candidate.state_revision, &candidate.commands)?;
        self.validate_route_candidate_after_replay(candidate, connection, &fork)?;
        if verify_physical_cost {
            let actual = physical_cost::physical_cost_between(
                self,
                &fork,
                std::slice::from_ref(&candidate.connection),
            )?;
            if actual != candidate.physical_cost {
                return Err(error(
                    "route-candidate-physical-cost",
                    &candidate.connection,
                    "candidate physical cost differs from exact native replay",
                ));
            }
        }
        *self = fork;
        Ok(steps)
    }

    fn validate_route_candidate_before_replay(
        &self,
        candidate: &Candidate,
    ) -> Result<usize, Diagnostic> {
        let Some(&connection) = self.connections.get(&candidate.connection) else {
            return Err(error(
                "route-candidate-connection",
                &candidate.connection,
                "route candidate references an unknown logical connection",
            ));
        };
        let mut valid_shape = matches!(
            candidate.commands.first(),
            Some(Command::BeginRoute {
                connection,
                source_port,
                target_port,
            }) if connection == &candidate.connection
                && source_port == &candidate.source_port
                && target_port == &candidate.target_port
        ) && candidate.commands.len() >= 2
            && candidate.commands.last() == Some(&Command::Connect);
        if valid_shape {
            for command in &candidate.commands[1..candidate.commands.len() - 1] {
                self.charge_replay(ReplayWork::Validation, 1)?;
                if !matches!(
                    command,
                    Command::Extend { .. } | Command::ReserveBridge { .. }
                ) {
                    valid_shape = false;
                    break;
                }
            }
        }
        if !valid_shape {
            return Err(error(
                "route-candidate-witness",
                &candidate.connection,
                "route candidate does not contain one complete route witness",
            ));
        }
        let mut reservations = Vec::new();
        for command in &candidate.commands {
            self.charge_replay(ReplayWork::Validation, 1)?;
            if let Some(reservation) = match command {
                Command::ReserveBridge {
                    position,
                    component,
                    rotation,
                    crossed_axis,
                } => Some((*position, component.as_str(), *rotation, *crossed_axis)),
                _ => None,
            } {
                reservations.push(reservation);
            }
        }
        let mut networks = IndexMap::<&str, usize>::new();
        for (ci, route) in self.routes.iter().enumerate() {
            self.charge_replay(ReplayWork::Validation, 1)?;
            if route.is_some() {
                let commodity = self.problem.connections[ci].commodity.as_str();
                let next = networks.len();
                networks.entry(commodity).or_insert(next);
            }
        }
        let mut crossing_mismatch = reservations.len() != candidate.crossings.len();
        if !crossing_mismatch {
            for (&(position, component, rotation, crossed_axis), crossing) in
                reservations.iter().zip(&candidate.crossings)
            {
                self.charge_replay(ReplayWork::Validation, 1)?;
                let axis = if crossing.direction % 2 == 0 {
                    BridgeAxis::Vertical
                } else {
                    BridgeAxis::Horizontal
                };
                let mut incoming_direction = None;
                for (pi, &point) in candidate.cells.iter().enumerate() {
                    self.charge_replay(ReplayWork::Validation, 1)?;
                    if point == position {
                        if pi > 0 {
                            incoming_direction =
                                aic_layout::geometry::direction(candidate.cells[pi - 1], point);
                        }
                        break;
                    }
                }
                let bridge_owner =
                    self.bridges
                        .get(&(crossing.transport, position))
                        .and_then(|bridge| {
                            bridge.channels[crossing.direction % 2]
                                .connections
                                .first()
                                .and_then(|owner| self.connections.get(owner).copied())
                        });
                let carrier_owner = if let Some(owner) = bridge_owner {
                    Some(owner)
                } else {
                    let mut first_user = None;
                    if let Some(users) = self.occupied[layer(crossing.transport)].get(&position) {
                        for &user in users {
                            self.charge_replay(ReplayWork::Users, 1)?;
                            first_user =
                                Some(first_user.map_or(user, |first: usize| first.min(user)));
                        }
                    }
                    first_user
                };
                crossing_mismatch = crossing.position != position
                    || crossing.component != component
                    || crossing.rotation != rotation
                    || crossing.kind != LogisticsComponentKind::Bridge
                    || crossing.transport != self.problem.connections[connection].transport
                    || crossed_axis != axis
                    || incoming_direction != Some(crossing.direction)
                    || carrier_owner != Some(crossing.carrier_chain)
                    || self
                        .problem
                        .connections
                        .get(crossing.carrier_chain)
                        .and_then(|carrier| networks.get(carrier.commodity.as_str()).copied())
                        != Some(crossing.carrier_network);
                if crossing_mismatch {
                    break;
                }
            }
        }
        if crossing_mismatch {
            return Err(error(
                "route-candidate-bridge-witness",
                &candidate.connection,
                "bridge metadata does not match the reserved native commands",
            ));
        }
        Ok(connection)
    }

    fn validate_route_candidate_after_replay(
        &self,
        candidate: &Candidate,
        connection: usize,
        fork: &Session,
    ) -> Result<(), Diagnostic> {
        let geometry_matches = if let Some(route) = fork.routes[connection].as_ref() {
            if route.cells.len() != candidate.cells.len() {
                false
            } else {
                let mut same = true;
                for (actual, expected) in route.cells.iter().zip(&candidate.cells) {
                    self.charge_replay(ReplayWork::Validation, 1)?;
                    if actual != expected {
                        same = false;
                        break;
                    }
                }
                same
            }
        } else {
            false
        };
        if !geometry_matches {
            return Err(error(
                "route-candidate-geometry",
                &candidate.connection,
                "replayed route does not match candidate geometry",
            ));
        }
        let mut crossings_match = true;
        for crossing in &candidate.crossings {
            self.charge_replay(ReplayWork::Validation, 1)?;
            let Some(bridge) = fork.bridges.get(&(crossing.transport, crossing.position)) else {
                crossings_match = false;
                break;
            };
            if bridge.component != crossing.component || bridge.rotation != crossing.rotation {
                crossings_match = false;
                break;
            }
            let mut connection_found = false;
            for owner in &bridge.channels[crossing.direction % 2].connections {
                self.charge_replay(ReplayWork::Validation, 1)?;
                if owner == &candidate.connection {
                    connection_found = true;
                    break;
                }
            }
            if !connection_found {
                crossings_match = false;
                break;
            }
        }
        if !crossings_match {
            return Err(error(
                "route-candidate-bridge-witness",
                &candidate.connection,
                "replayed bridge axes do not match the candidate witness",
            ));
        }
        Ok(())
    }

    /// Atomically applies a complete same-commodity network witness. Each
    /// member route is replayed against the state produced by the preceding
    /// member, which delegates cell and port capacity to the native session.
    pub fn apply_network_candidate(
        &mut self,
        candidate: &NetworkCandidate,
    ) -> Result<Vec<Step>, Diagnostic> {
        if self.revision != candidate.state_revision
            || candidate.routes.len() < 2
            || candidate.commodity.is_empty()
        {
            return Err(error(
                "network-candidate-identity",
                &candidate.commodity,
                "network candidate is stale, empty, or does not contain multiple routes",
            ));
        }
        let mut connections = BTreeSet::new();
        let mut fork = self.clone();
        let mut steps = Vec::new();
        for route in &candidate.routes {
            let Some(&connection) = fork.connections.get(&route.connection) else {
                return Err(error(
                    "network-candidate-connection",
                    &route.connection,
                    "network candidate references an unknown connection",
                ));
            };
            let obligation = &fork.problem.connections[connection];
            if obligation.commodity != candidate.commodity
                || obligation.transport != candidate.transport
                || !connections.insert(route.connection.as_str())
            {
                return Err(error(
                    "network-candidate-task",
                    &route.connection,
                    "network routes must be distinct and share commodity and transport",
                ));
            }
            steps.extend(fork.apply_route_candidate(route)?);
        }
        let steps_total = candidate
            .routes
            .iter()
            .map(|route| route.steps)
            .sum::<i64>();
        let turns_total = candidate
            .routes
            .iter()
            .map(|route| route.turns)
            .sum::<i64>();
        let expanded_total = candidate
            .routes
            .iter()
            .map(|route| route.expanded)
            .sum::<usize>();
        let shared_cells = if self.replay_meter.is_some() {
            network_shared_cells_with_replay(&candidate.routes, |count| {
                self.charge_replay(ReplayWork::Validation, count)
            })?
        } else {
            network_shared_cells(&candidate.routes)
        };
        if candidate.steps != steps_total
            || candidate.turns != turns_total
            || candidate.expanded != expanded_total
            || candidate.shared_cells != shared_cells
        {
            return Err(error(
                "network-candidate-metadata",
                &candidate.commodity,
                "network candidate aggregate work or sharing metadata differs from its routes",
            ));
        }
        let changed = candidate
            .routes
            .iter()
            .map(|route| route.connection.clone())
            .collect::<Vec<_>>();
        let actual_cost = physical_cost::physical_cost_between(self, &fork, &changed)?;
        if actual_cost != candidate.physical_cost {
            return Err(error(
                "network-candidate-physical-cost",
                &candidate.commodity,
                "network physical cost differs from exact native replay",
            ));
        }
        *self = fork;
        Ok(steps)
    }

    fn would_complete_after_route(
        &self,
        connection: &str,
        allowance: &mut RequestAllowance,
    ) -> Result<bool, AllowanceStop> {
        allowance.charge(
            WorkKind::ValidationRecords,
            self.poses
                .len()
                .saturating_add(self.terminal_poses.len())
                .saturating_add(self.routes.len()),
        )?;
        Ok(self.poses.iter().all(Option::is_some)
            && self.terminal_poses.iter().all(Option::is_some)
            && self.routes.iter().enumerate().all(|(index, route)| {
                route.is_some() || self.problem.connections[index].id == connection
            }))
    }

    fn apply_with_request_allowance<T>(
        &mut self,
        allowance: &mut RequestAllowance,
        copied_bytes: usize,
        peak_bytes: usize,
        apply: impl FnOnce(&Session, &mut Session) -> Result<T, Diagnostic>,
    ) -> Result<T, RequestFailure> {
        allowance.preflight(WorkKind::CopiedBytes, copied_bytes)?;
        allowance.reserve_peak(peak_bytes)?;
        if let Err(stop) = allowance.charge(WorkKind::CopiedBytes, copied_bytes) {
            allowance.release_peak(peak_bytes);
            return Err(RequestFailure::Stopped(stop));
        }
        let meter = std::sync::Arc::new(ReplayMeter::new([
            allowance.remaining(WorkKind::ReplayRouteCells),
            allowance.remaining(WorkKind::ReplayUsers),
            allowance.remaining(WorkKind::CatalogVisits),
            allowance.remaining(WorkKind::ValidationRecords),
        ]));
        let mut fork = self.clone();
        fork.replay_deadline = Some(allowance.deadline());
        fork.replay_meter = Some(meter.clone());
        let result = apply(self, &mut fork);
        let mut completed_stop = None;
        for (kind, used) in [
            WorkKind::ReplayRouteCells,
            WorkKind::ReplayUsers,
            WorkKind::CatalogVisits,
            WorkKind::ValidationRecords,
        ]
        .into_iter()
        .zip(meter.usage())
        {
            if let Err(stop) = allowance.record_completed(kind, used) {
                completed_stop.get_or_insert(stop);
            }
        }
        let stop = meter
            .stopped()
            .map(|kind| {
                allowance.exhausted_stop(match kind {
                    ReplayWork::RouteCells => WorkKind::ReplayRouteCells,
                    ReplayWork::Users => WorkKind::ReplayUsers,
                    ReplayWork::Catalog => WorkKind::CatalogVisits,
                    ReplayWork::Validation => WorkKind::ValidationRecords,
                })
            })
            .or(completed_stop)
            .or_else(|| allowance.check_deadline().err());
        allowance.release_peak(peak_bytes);
        if let Some(stop) = stop {
            return Err(RequestFailure::Stopped(stop));
        }
        let value = result.map_err(RequestFailure::Invalid)?;
        fork.replay_meter = None;
        fork.replay_deadline = None;
        *self = fork;
        Ok(value)
    }

    pub fn apply_route_candidate_with_allowance(
        &mut self,
        candidate: &Candidate,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Step>, RequestFailure> {
        self.apply_route_candidate_with_cost_inner(candidate, allowance, true)
            .map(|(steps, _)| steps)
    }

    /// Return the exact delta from the already-required atomic replay fork.
    /// Network members can retain physical metadata without cloning the
    /// partial factory a second time for every route.
    pub fn apply_unpublished_route_candidate_with_cost_allowance(
        &mut self,
        candidate: &Candidate,
        allowance: &mut RequestAllowance,
    ) -> Result<(Vec<Step>, PhysicalCost), RequestFailure> {
        self.apply_route_candidate_with_cost_inner(candidate, allowance, false)
    }

    fn apply_route_candidate_with_cost_inner(
        &mut self,
        candidate: &Candidate,
        allowance: &mut RequestAllowance,
        verify_physical_cost: bool,
    ) -> Result<(Vec<Step>, PhysicalCost), RequestFailure> {
        let root_bytes = self.clone_reservation_bytes();
        let final_layout_copy =
            self.would_complete_after_route(&candidate.connection, allowance)?;
        let projected_bytes = root_bytes
            .saturating_add(candidate.cells.len().saturating_mul(4096))
            .saturating_add(candidate.crossings.len().saturating_mul(4096))
            .saturating_add(candidate.commands.len().saturating_mul(128))
            .saturating_add(4096);
        let copies = root_bytes
            .saturating_mul(2)
            .saturating_add(if final_layout_copy {
                projected_bytes
            } else {
                0
            });
        let peak = root_bytes
            .saturating_add(projected_bytes.saturating_mul(1 + usize::from(final_layout_copy)));
        self.apply_with_request_allowance(allowance, copies, peak, |before, fork| {
            let steps = fork.apply_route_candidate_checked(candidate, false)?;
            let cost = physical_cost::physical_cost_between(
                before,
                fork,
                std::slice::from_ref(&candidate.connection),
            )?;
            if verify_physical_cost && cost != candidate.physical_cost {
                return Err(error(
                    "route-candidate-physical-cost",
                    &candidate.connection,
                    "candidate physical cost differs from exact native replay",
                ));
            }
            Ok((steps, cost))
        })
    }

    pub fn apply_network_candidate_with_allowance(
        &mut self,
        candidate: &NetworkCandidate,
        projected_final_clone_bytes: usize,
        projected_copied_bytes: usize,
        final_layout_copy: bool,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Step>, RequestFailure> {
        let root_bytes = self.clone_reservation_bytes();
        let final_bytes = projected_final_clone_bytes.max(root_bytes);
        let peak_bytes = root_bytes
            .saturating_add(final_bytes.saturating_mul(2 + usize::from(final_layout_copy)));
        self.apply_with_request_allowance(
            allowance,
            projected_copied_bytes,
            peak_bytes,
            |_, fork| fork.apply_network_candidate(candidate),
        )
    }

    fn endpoint_pairs(
        &self,
        connection: usize,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<EndpointPair>, RequestFailure> {
        EndpointPairCursor::new(self, connection).advance(self, connection, bounds, allowance)
    }

    fn endpoint_pair_reservation_bytes(&self, connection: usize) -> usize {
        let obligation = &self.problem.connections[connection];
        let domain = |endpoint: &str| {
            self.blocks.get(endpoint).map_or((1, 0, 0), |&block| {
                let ports = &self
                    .catalogs
                    .facilities
                    .facility(&self.problem.blocks[block].facility)
                    .expect("admitted facility")
                    .ports;
                (
                    ports.len(),
                    ports.iter().map(|port| port.id.len()).sum::<usize>(),
                    ports.iter().map(|port| port.id.len()).max().unwrap_or(0),
                )
            })
        };
        let (sources, source_ids, source_max) = domain(&obligation.source);
        let (targets, target_ids, target_max) = domain(&obligation.target);
        let pairs = sources.saturating_mul(targets);
        let paired_id_bytes = source_ids
            .saturating_mul(targets)
            .saturating_add(target_ids.saturating_mul(sources));

        // A B-tree node can hold 11 key/value slots plus 12 child pointers in
        // the supported Rust implementation. Budget one full node per possible
        // entry, also covering a transient split node. This deliberately bounds
        // the allocation while retaining incremental, ordered insertion.
        let node_bytes = 128usize
            .saturating_add(
                11usize.saturating_mul(
                    std::mem::size_of::<EndpointPairKey>()
                        .saturating_add(std::mem::size_of::<EndpointPair>()),
                ),
            )
            .saturating_add(12 * std::mem::size_of::<usize>());
        let domains = sources
            .saturating_add(targets)
            .saturating_mul(std::mem::size_of::<Endpoint>())
            .saturating_add(source_ids)
            .saturating_add(target_ids)
            .saturating_add(obligation.source.len())
            .saturating_add(obligation.target.len());
        let ordered = pairs
            .saturating_add(1)
            .saturating_mul(node_bytes)
            .saturating_add(paired_id_bytes.saturating_mul(2));
        let final_pairs = pairs
            .saturating_mul(std::mem::size_of::<EndpointPair>())
            .saturating_add(paired_id_bytes);
        let transient_port_ids = source_max.max(target_max).saturating_mul(2);
        domains
            .saturating_add(ordered)
            .saturating_add(final_pairs)
            .saturating_add(transient_port_ids)
    }

    pub(super) fn guidance_graph_bounds(&self) -> (usize, usize, usize) {
        let entities = self
            .problem
            .blocks
            .len()
            .saturating_add(self.problem.terminals.len())
            .saturating_add(self.problem.connections.len());
        let relations = self.problem.connections.len().saturating_mul(2);
        let route_cells = self
            .routes
            .iter()
            .flatten()
            .map(|route| route.cells.len())
            .sum::<usize>();
        let bridge_rows = self
            .bridges
            .values()
            .flat_map(|bridge| bridge.channels.iter())
            .map(|channel| channel.connections.len())
            .sum::<usize>();
        let spatial = self
            .bodies
            .len()
            .saturating_add(route_cells)
            .saturating_add(bridge_rows)
            .saturating_add(self.terminal_poses.iter().flatten().count());
        let rows = entities.saturating_add(relations).saturating_add(spatial);
        let bytes = entities
            .saturating_mul(super::learning::ENTITY_FEATURES * std::mem::size_of::<f32>())
            .saturating_add(relations.saturating_mul(
                2 * std::mem::size_of::<i64>()
                    + super::learning::RELATION_FEATURES * std::mem::size_of::<f32>(),
            ))
            .saturating_add(spatial.saturating_mul(
                super::learning::SPATIAL_FEATURES * std::mem::size_of::<f32>()
                    + std::mem::size_of::<i64>(),
            ))
            .saturating_mul(2);
        (rows, route_cells, bytes)
    }

    pub(super) fn bridge_copy_bytes(&self) -> usize {
        self.bridges.values().map(bridge_copy_bytes).sum()
    }

    fn route_blocked_cells_bounded(
        &self,
        connection: usize,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<IndexSet<Point>, AllowanceStop> {
        BlockedCellsCursor::new(self, connection).advance(self, connection, bounds, allowance)
    }

    fn route_commands(
        &self,
        connection: &str,
        source_port: Option<String>,
        target_port: Option<String>,
        cells: &[Point],
        crossings: &[Crossing],
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Command>, AllowanceStop> {
        let mut commands = vec![Command::BeginRoute {
            connection: connection.to_owned(),
            source_port,
            target_port,
        }];
        let at = crossings
            .iter()
            .map(|crossing| (crossing.position, crossing))
            .collect::<IndexMap<_, _>>();
        let mut start = 0usize;
        while start + 1 < cells.len() {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            let heading = direction(cells[start], cells[start + 1]);
            if let Some(crossing) = at.get(&cells[start + 1]) {
                commands.push(Command::ReserveBridge {
                    position: crossing.position,
                    component: crossing.component.clone(),
                    rotation: crossing.rotation,
                    crossed_axis: if crossing.direction % 2 == 0 {
                        BridgeAxis::Vertical
                    } else {
                        BridgeAxis::Horizontal
                    },
                });
                commands.push(Command::Extend {
                    direction: heading,
                    length: 1,
                });
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end + 1 < cells.len()
                && direction(cells[end], cells[end + 1]) == heading
                && !at.contains_key(&cells[end + 1])
                && end - start < self.limits.max_extension as usize
            {
                allowance.charge(WorkKind::RoutingScans, 1)?;
                end += 1;
            }
            commands.push(Command::Extend {
                direction: heading,
                length: (end - start) as i64,
            });
            start = end;
        }
        commands.push(Command::Connect);
        Ok(commands)
    }

    fn replay_route_witness(
        &self,
        state_revision: u64,
        commands: &[Command],
    ) -> Result<(Session, Vec<Step>), Diagnostic> {
        if self.revision != state_revision {
            return Err(error(
                "stale-route-candidate",
                "route-candidate",
                "route candidate belongs to an earlier constructive revision",
            ));
        }
        let mut fork = self.clone();
        let mut steps = Vec::with_capacity(commands.len());
        for command in commands {
            let step = fork.step(command.clone());
            if !step.accepted || step.diagnostic.is_some() {
                return Err(step.diagnostic.unwrap_or_else(|| {
                    error(
                        "route-candidate-replay",
                        "route-candidate",
                        "native replay rejected a route candidate",
                    )
                }));
            }
            steps.push(step);
        }
        Ok((fork, steps))
    }

    fn replay_route_witness_with_allowance(
        &self,
        state_revision: u64,
        commands: &[Command],
        allowance: &mut RequestAllowance,
    ) -> Result<(Session, usize), RequestFailure> {
        if self.revision != state_revision {
            return Err(error(
                "stale-route-candidate",
                "route-candidate",
                "route candidate belongs to an earlier constructive revision",
            )
            .into());
        }
        let mut progress = NativeReplayProgress::new_borrowed(self, commands);
        match progress.advance(allowance, 0) {
            ReplayAdvance::Pending(stop) => {
                progress.cancel(allowance);
                Err(RequestFailure::Stopped(stop))
            }
            ReplayAdvance::Complete(SourceOwner::Borrowed(_), Ok((child, outcomes, bytes)), _) => {
                let outcome_bytes = outcomes.capacity()
                    * std::mem::size_of::<super::native_replay::ReplayCommandOutcome>();
                drop(outcomes);
                allowance.release_peak(outcome_bytes);
                Ok((child, bytes - outcome_bytes))
            }
            ReplayAdvance::Complete(
                SourceOwner::Borrowed(_),
                Err((failure, outcomes, bytes)),
                _,
            ) => {
                drop(outcomes);
                allowance.release_peak(bytes);
                Err(failure)
            }
            _ => unreachable!("borrowed replay must return borrowed source"),
        }
    }
}

fn empty_report(
    request: &Request,
    termination: Termination,
    endpoint_pairs: usize,
    proven: bool,
    crossing_limit: usize,
) -> Report {
    Report {
        schema: SCHEMA,
        state_revision: request.state_revision,
        connection: request.connection.clone(),
        termination,
        fixed_request_proven_infeasible: proven,
        endpoint_pairs,
        searches: 0,
        expanded: 0,
        crossing_limit,
        preflight_expanded: 0,
        guides_found: 0,
        duplicate_guides: 0,
        native_rejections: vec![],
        minimum_pairwise_dissimilarity: None,
        mean_pairwise_dissimilarity: None,
        candidates: vec![],
        allowance: None,
        allowance_stop: None,
    }
}

fn stopped_report(
    request: &Request,
    crossing_limit: usize,
    endpoint_pairs: usize,
    allowance: &RequestAllowance,
    stop: AllowanceStop,
) -> Report {
    let termination = stop_termination(&stop);
    let mut report = empty_report(request, termination, endpoint_pairs, false, crossing_limit);
    report.allowance = Some(allowance.usage());
    report.allowance_stop = Some(stop);
    report
}

fn stop_termination(stop: &AllowanceStop) -> Termination {
    if stop.reason == "deadline" {
        Termination::Deadline
    } else {
        Termination::WorkLimit
    }
}

fn opposite(edge: FacilityPortEdge) -> usize {
    (edge_index(edge) + 2) % 4
}

fn json_size_with_allowance<T: Serialize>(
    value: &T,
    allowance: &mut RequestAllowance,
) -> Result<usize, RequestFailure> {
    struct Count<'a> {
        bytes: usize,
        pending: usize,
        allowance: &'a mut RequestAllowance,
        stopped: Option<AllowanceStop>,
    }
    impl Write for Count<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self.bytes.saturating_add(bytes.len());
            self.pending = self.pending.saturating_add(bytes.len());
            if self.pending >= 4096 {
                if let Err(stop) = self
                    .allowance
                    .record_completed(WorkKind::PublishedBytes, self.pending)
                {
                    self.stopped = Some(stop);
                    return Err(std::io::Error::other(
                        "candidate encoding allowance stopped",
                    ));
                }
                self.pending = 0;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count {
        bytes: 0,
        pending: 0,
        allowance,
        stopped: None,
    };
    if let Err(failure) = serde_json::to_writer(&mut count, value) {
        if let Some(stop) = count.stopped {
            return Err(RequestFailure::Stopped(stop));
        }
        return Err(RequestFailure::Invalid(error(
            "route-candidate-publication",
            "route-candidate",
            failure.to_string(),
        )));
    }
    count
        .allowance
        .record_completed(WorkKind::PublishedBytes, count.pending)?;
    Ok(count.bytes)
}

fn direction(from: Point, to: Point) -> Direction {
    match (to.x - from.x, to.y - from.y) {
        (1, 0) => Direction::East,
        (0, 1) => Direction::South,
        (-1, 0) => Direction::West,
        (0, -1) => Direction::North,
        _ => unreachable!("routing kernel returns cardinal unit steps"),
    }
}

fn interior(cells: &[Point]) -> impl Iterator<Item = Point> + '_ {
    cells
        .iter()
        .copied()
        .skip(1)
        .take(cells.len().saturating_sub(2))
}

fn add_overlap_penalty(
    penalties: &mut IndexMap<Point, u32>,
    cells: &[Point],
    amount: u32,
    allowance: &mut RequestAllowance,
) -> Result<(), AllowanceStop> {
    for point in interior(cells) {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        let entry = penalties.entry(point).or_default();
        *entry = entry.saturating_add(amount).min(1_000_000);
    }
    Ok(())
}

fn directed_cost(
    cells: &[Point],
    costs: &IndexMap<(Point, usize), u32>,
    allowance: &mut RequestAllowance,
) -> Result<u64, AllowanceStop> {
    let mut total = 0u64;
    for edge in cells.windows(2) {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        let direction = match (edge[1].x - edge[0].x, edge[1].y - edge[0].y) {
            (1, 0) => 0,
            (0, 1) => 1,
            (-1, 0) => 2,
            (0, -1) => 3,
            _ => unreachable!("routing kernel returns cardinal unit steps"),
        };
        total = total.saturating_add(u64::from(
            costs.get(&(edge[1], direction)).copied().unwrap_or(0),
        ));
    }
    Ok(total)
}

fn maximum_overlap(
    cells: &[Point],
    candidates: &[Candidate],
    allowance: &mut RequestAllowance,
) -> Result<usize, AllowanceStop> {
    let mut current = HashSet::new();
    for point in interior(cells) {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        current.insert(point);
    }
    let mut maximum = 0;
    for candidate in candidates {
        let mut overlap = 0;
        for point in interior(&candidate.cells) {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            overlap += usize::from(current.contains(&point));
        }
        maximum = maximum.max(overlap);
    }
    Ok(maximum)
}

pub fn network_shared_cells(routes: &[Candidate]) -> usize {
    let mut uses = IndexMap::<Point, usize>::new();
    for route in routes {
        for point in route.cells.iter().copied().collect::<BTreeSet<_>>() {
            *uses.entry(point).or_default() += 1;
        }
    }
    uses.values().filter(|&&count| count > 1).count()
}

fn network_shared_cells_with_replay(
    routes: &[Candidate],
    mut visit: impl FnMut(usize) -> Result<(), Diagnostic>,
) -> Result<usize, Diagnostic> {
    let mut uses = IndexMap::<Point, usize>::new();
    for route in routes {
        let mut unique = HashSet::new();
        for &point in &route.cells {
            visit(1)?;
            unique.insert(point);
        }
        for point in unique {
            visit(1)?;
            *uses.entry(point).or_default() += 1;
        }
    }
    let mut shared = 0;
    for count in uses.values() {
        visit(1)?;
        shared += usize::from(*count > 1);
    }
    Ok(shared)
}

pub fn network_shared_cells_with_allowance(
    routes: &[Candidate],
    allowance: &mut RequestAllowance,
) -> Result<usize, AllowanceStop> {
    let mut uses = IndexMap::<Point, usize>::new();
    for route in routes {
        let mut unique = HashSet::new();
        for &point in &route.cells {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            unique.insert(point);
        }
        for point in unique {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            *uses.entry(point).or_default() += 1;
        }
    }
    let mut shared = 0;
    for count in uses.values() {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        shared += usize::from(*count > 1);
    }
    Ok(shared)
}

#[cfg(test)]
mod allowance_memory_tests {
    use super::{AllowanceCaps, Command, Direction, RequestAllowance, SearchMemoryBudget};

    #[test]
    fn one_shot_native_replay_keeps_source_and_releases_exact_child_reservation() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        let original_revision = source.revision;
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let commands = [
            Command::BeginRoute {
                connection: "connection-0".into(),
                source_port: Some("output-belt-1".into()),
                target_port: Some("input-belt-1".into()),
            },
            Command::Extend {
                direction: Direction::North,
                length: 1,
            },
        ];
        let (child, bytes) = source
            .replay_route_witness_with_allowance(original_revision, &commands, &mut allowance)
            .expect("valid native route prefix");
        assert_eq!(source.revision, original_revision);
        assert!(source.active.is_none());
        assert!(child.active.is_some());
        assert_eq!(allowance.reserved_bytes(), bytes);
        drop(child);
        allowance.release_peak(bytes);
        assert_eq!(allowance.reserved_bytes(), 0);

        let invalid = [Command::Extend {
            direction: Direction::North,
            length: 1,
        }];
        assert!(matches!(
            source.replay_route_witness_with_allowance(original_revision, &invalid, &mut allowance),
            Err(super::RequestFailure::Invalid(_))
        ));
        assert_eq!(allowance.reserved_bytes(), 0);
    }

    #[test]
    fn aggregate_cap_counts_two_live_requests_and_recovers_after_release() {
        let tree = SearchMemoryBudget::new(100);
        let caps = AllowanceCaps {
            peak_live_bytes: 100,
            ..AllowanceCaps::default()
        };
        let mut first = RequestAllowance::with_memory_budget(caps.clone(), Some(tree.clone()));
        let mut second = RequestAllowance::with_memory_budget(caps, Some(tree.clone()));
        first.reserve_peak(60).unwrap();
        second.reserve_peak(35).unwrap();
        let stop = second.reserve_peak(10).unwrap_err();
        assert_eq!(stop.reason, "aggregate-memory-limit");
        assert_eq!(tree.live_bytes(), 95);
        first.release_peak(60);
        second.reserve_peak(10).unwrap();
        assert_eq!(tree.peak_bytes(), 95);
        second.release_peak(45);
        assert_eq!(tree.live_bytes(), 0);
        first.reserve_peak(20).unwrap();
        drop(first);
        assert_eq!(tree.live_bytes(), 0);
    }
}

fn diversity(
    candidates: &[Candidate],
    allowance: &mut RequestAllowance,
) -> Result<(Option<f64>, Option<f64>), AllowanceStop> {
    let mut minimum = f64::INFINITY;
    let mut total = 0.0;
    let mut pairs = 0usize;
    for left_index in 0..candidates.len() {
        let mut left = HashSet::new();
        for point in interior(&candidates[left_index].cells) {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            left.insert(point);
        }
        for right in &candidates[left_index + 1..] {
            let mut right_set = HashSet::new();
            for point in interior(&right.cells) {
                allowance.charge(WorkKind::RoutingScans, 1)?;
                right_set.insert(point);
            }
            allowance.charge(
                WorkKind::RoutingScans,
                left.len().saturating_add(right_set.len()),
            )?;
            let intersection = left.intersection(&right_set).count();
            let union = left.union(&right_set).count();
            let value = if union == 0 {
                0.0
            } else {
                1.0 - intersection as f64 / union as f64
            };
            minimum = minimum.min(value);
            total += value;
            pairs += 1;
        }
    }
    if pairs == 0 {
        return Ok((None, None));
    }
    Ok((Some(minimum), Some(total / pairs as f64)))
}
