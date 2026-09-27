//! Retained exact-graph work shared by anchor and guided local searches.
//!
//! A graph belongs to one immutable native state and one exact residual
//! capacity snapshot. Search frontiers borrow it only while advancing, so a
//! candidate pager can own the graph and several independent cost frontiers.

use std::{borrow::Cow, cmp::Ordering, collections::HashMap, ops::Deref, sync::Arc, time::Instant};

use ahash::RandomState;
use aic_layout::geometry::{Bounds, DIRECTIONS, Point};
use indexmap::{IndexMap, IndexSet};

use crate::{Crossing, Crossings, PathOptions, PathResult, reachability};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct TransitionKey {
    point: Point,
    direction: usize,
    crossings_used: usize,
    entered_bridge: bool,
    start: Point,
    end: Point,
    first: Option<usize>,
    last: Option<usize>,
    crossing_limit: usize,
}

#[derive(Clone)]
struct Transition {
    point: Point,
    direction: usize,
    via: Option<Crossing>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct PairKey {
    start: Point,
    end: Point,
    first: Option<usize>,
    last: Option<usize>,
    crossing_limit: usize,
    probe_cap: usize,
}

#[derive(Clone)]
struct Preflight {
    status: &'static str,
    frontier_peak: usize,
    blocked_frontier: Vec<Point>,
}

struct SuccessorBuild {
    key: TransitionKey,
    direction: usize,
    choice: Option<usize>,
    transitions: Vec<Transition>,
    blocked: Vec<Point>,
    pending_transition: Option<PendingTransition>,
}

struct PendingTransition {
    point: Point,
    direction: usize,
    crossing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reservation {
    Granted,
    Retry,
    Denied,
}

enum SuccessorStep {
    Pending(usize),
    Ready(usize),
    ResourceLimit(usize),
}

struct ActiveExpansion {
    index: usize,
    successors_ready: bool,
    blocked_transfer_index: usize,
    next_transition: usize,
    found: Option<FoundBuild>,
}

struct FoundBuild {
    current: Option<usize>,
    cells: Vec<Point>,
    crossings: Vec<Crossing>,
    seen: IndexSet<Point>,
    pending_check: Option<Point>,
}

pub struct PreparedGraph<'a> {
    bounds: Bounds,
    blocked: Cow<'a, IndexSet<Point>>,
    crossings: Cow<'a, Crossings>,
    /// Exact source/revision/capacity identity supplied by the native pager.
    /// No cache entry is allowed to survive a change to this identity.
    identity: Vec<u8>,
    successors: HashMap<TransitionKey, Vec<Transition>, RandomState>,
    transient_successors: Vec<Transition>,
    transient_blocked: Vec<Point>,
    transient_key: Option<TransitionKey>,
    one_shot: bool,
    check_deadlines: bool,
    successor_build: Option<SuccessorBuild>,
    preflight: HashMap<PairKey, Preflight, RandomState>,
    preflight_build: Option<(PairKey, reachability::ProbeCursor)>,
    cache_hits: usize,
    cache_misses: usize,
    reserved_bytes: usize,
}

pub struct Frontier<'a> {
    start: Point,
    end: Point,
    first: Option<usize>,
    last: Option<usize>,
    crossing_limit: usize,
    prefer_crossings: bool,
    cell_penalties: Option<PenaltyMap<'a, Point>>,
    edge_penalties: Option<PenaltyMap<'a, (Point, usize)>>,
    states: Vec<State>,
    heap: Vec<usize>,
    best: HashMap<StateKey, usize, RandomState>,
    expanded: usize,
    scans: usize,
    frontier_peak: usize,
    blocked_frontier: IndexSet<Point>,
    preflight_status: &'static str,
    preflight_expanded: usize,
    preflight_scans: usize,
    completed: bool,
    reserved_bytes: usize,
    active_expansion: Option<ActiveExpansion>,
    found_path_bytes: usize,
}

enum PenaltyMap<'a, K> {
    Borrowed(&'a IndexMap<K, u32>),
    Shared(Arc<IndexMap<K, u32>>),
}

impl<K> Deref for PenaltyMap<'_, K> {
    type Target = IndexMap<K, u32>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(map) => map,
            Self::Shared(map) => map,
        }
    }
}

#[derive(Clone)]
struct State {
    id: StateKey,
    g: Score,
    priority_cost: u128,
    via: Option<Crossing>,
    parent: Option<usize>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct StateKey {
    y: i64,
    x: i64,
    crossings_used: usize,
    direction: u8,
    entered_bridge: bool,
}

impl Ord for StateKey {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.y,
            self.x,
            self.direction,
            self.crossings_used,
            self.entered_bridge,
        )
            .cmp(&(
                other.y,
                other.x,
                other.direction,
                other.crossings_used,
                other.entered_bridge,
            ))
    }
}

impl PartialOrd for StateKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl StateKey {
    fn new(point: Point, direction: usize, crossings_used: usize, entered_bridge: bool) -> Self {
        debug_assert!(direction <= 4);
        Self {
            y: point.y,
            x: point.x,
            crossings_used,
            direction: direction as u8,
            entered_bridge,
        }
    }

    fn point(self) -> Point {
        Point {
            x: self.x,
            y: self.y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Score {
    path_cost: u128,
    turns: u128,
}

impl Score {
    const ZERO: Self = Self {
        path_cost: 0,
        turns: 0,
    };

    fn with_step(self, penalty: u32, turned: bool) -> Option<Self> {
        Some(Self {
            path_cost: self.path_cost.checked_add(u128::from(penalty) + 1)?,
            turns: self.turns.checked_add(u128::from(turned))?,
        })
    }

    fn with_heuristic(self, point: Point, end: Point) -> Option<Self> {
        let dx = (i128::from(point.x) - i128::from(end.x)).unsigned_abs();
        let dy = (i128::from(point.y) - i128::from(end.y)).unsigned_abs();
        Some(Self {
            path_cost: self.path_cost.checked_add(dx.checked_add(dy)?)?,
            turns: self.turns,
        })
    }
}

pub(super) fn checked_offset(point: Point, direction: usize) -> Option<Point> {
    let (dx, dy) = *DIRECTIONS.get(direction)?;
    Some(Point {
        x: point.x.checked_add(dx)?,
        y: point.y.checked_add(dy)?,
    })
}

fn clipped_crossing_limit(bounds: Bounds, requested: usize) -> usize {
    let width = (i128::from(bounds.max_x) - i128::from(bounds.min_x) + 1).max(0) as u128;
    let height = (i128::from(bounds.max_y) - i128::from(bounds.min_y) + 1).max(0) as u128;
    let area = width.checked_mul(height).unwrap_or(u128::MAX);
    requested.min(usize::try_from(area).unwrap_or(usize::MAX))
}

fn try_clone_crossing(crossing: &Crossing) -> Option<Crossing> {
    let mut component = String::new();
    component.try_reserve(crossing.component.len()).ok()?;
    component.push_str(&crossing.component);
    Some(Crossing {
        position: crossing.position,
        direction: crossing.direction,
        component,
        rotation: crossing.rotation,
        transport: crossing.transport,
        kind: crossing.kind,
        carrier_chain: crossing.carrier_chain,
        carrier_network: crossing.carrier_network,
    })
}

pub enum Advance {
    /// The frontier is retained unchanged for a later page.
    Pending,
    Complete(PathResult),
}

pub enum PreflightStatus {
    Pending,
    Ready,
    Stopped(PathResult),
}

pub struct PreflightPage {
    pub status: PreflightStatus,
    pub expanded: usize,
    pub scans: usize,
    pub frontier_peak: usize,
    /// The caller releases this much after the owned reverse queue is gone.
    pub released_bytes: usize,
}

impl<'a> PreparedGraph<'a> {
    pub fn new(
        bounds: Bounds,
        blocked: IndexSet<Point>,
        crossings: Crossings,
        exact_state_and_capacity_identity: Vec<u8>,
    ) -> Self {
        Self {
            bounds,
            blocked: Cow::Owned(blocked),
            crossings: Cow::Owned(crossings),
            identity: exact_state_and_capacity_identity,
            successors: HashMap::with_hasher(RandomState::new()),
            transient_successors: Vec::new(),
            transient_blocked: Vec::new(),
            transient_key: None,
            one_shot: false,
            check_deadlines: true,
            successor_build: None,
            preflight: HashMap::with_hasher(RandomState::new()),
            preflight_build: None,
            cache_hits: 0,
            cache_misses: 0,
            reserved_bytes: 0,
        }
    }

    /// A one-shot caller can reuse stable input collections without cloning
    /// the entire blocked/crossing maps into a short-lived prepared graph.
    pub fn borrowed(
        bounds: Bounds,
        blocked: &'a IndexSet<Point>,
        crossings: &'a Crossings,
        check_deadlines: bool,
    ) -> Self {
        Self {
            bounds,
            blocked: Cow::Borrowed(blocked),
            crossings: Cow::Borrowed(crossings),
            identity: Vec::new(),
            successors: HashMap::with_hasher(RandomState::new()),
            transient_successors: Vec::new(),
            transient_blocked: Vec::new(),
            transient_key: None,
            one_shot: true,
            check_deadlines,
            successor_build: None,
            preflight: HashMap::with_hasher(RandomState::new()),
            preflight_build: None,
            cache_hits: 0,
            cache_misses: 0,
            reserved_bytes: 0,
        }
    }

    pub fn identity(&self) -> &[u8] {
        &self.identity
    }

    pub fn cache_counts(&self) -> (usize, usize) {
        (self.cache_hits, self.cache_misses)
    }

    pub fn retained_entries(&self) -> usize {
        self.successors.len().saturating_add(self.preflight.len())
    }

    pub fn crossing_options_len(&self) -> usize {
        self.crossings.len()
    }

    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes.saturating_add(
            self.preflight_build
                .as_ref()
                .map_or(0, |(_, cursor)| cursor.reserved_bytes()),
        )
    }

    pub fn advance_preflight(
        &mut self,
        start: Point,
        end: Point,
        first: Option<usize>,
        last: Option<usize>,
        crossing_limit: usize,
        work_budget: usize,
        max_new_expansions: usize,
        max_new_scans: usize,
        hard_frontier: usize,
        page_deadline: Instant,
        hard_deadline: Instant,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> PreflightPage {
        let pair = PairKey {
            start,
            end,
            first,
            last,
            crossing_limit: clipped_crossing_limit(self.bounds, crossing_limit),
            probe_cap: 256.min(work_budget / 4),
        };
        if pair.probe_cap == 0 {
            return PreflightPage {
                status: PreflightStatus::Ready,
                expanded: 0,
                scans: 0,
                frontier_peak: 0,
                released_bytes: 0,
            };
        }
        if self.preflight.contains_key(&pair) {
            return PreflightPage {
                status: PreflightStatus::Ready,
                expanded: 0,
                scans: 0,
                frontier_peak: 0,
                released_bytes: 0,
            };
        }
        if self.preflight_build.is_none() {
            self.preflight_build = Some((
                pair,
                reachability::ProbeCursor::new(start, end, pair.probe_cap),
            ));
        }
        let (active_pair, cursor) = self.preflight_build.as_mut().unwrap();
        assert_eq!(*active_pair, pair, "reverse probe belongs to this pair");
        let before = cursor.work_counts();
        let options = PathOptions {
            first,
            last,
            work_budget: pair.probe_cap.max(1).saturating_mul(4),
            scan_budget: usize::MAX,
            frontier_budget: hard_frontier,
            deadline: self.check_deadlines.then_some(hard_deadline),
            crossings: &self.crossings,
            crossing_limit: pair.crossing_limit,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        };
        let complete = cursor.advance(
            &self.blocked,
            self.bounds,
            &options,
            max_new_expansions,
            max_new_scans,
            page_deadline,
            self.check_deadlines,
            reserve,
        );
        let after = cursor.work_counts();
        let mut page = PreflightPage {
            status: PreflightStatus::Pending,
            expanded: after.0.saturating_sub(before.0),
            scans: after.1.saturating_sub(before.1),
            frontier_peak: after.2,
            released_bytes: 0,
        };
        if !complete {
            return page;
        }
        let cursor = &self.preflight_build.as_ref().unwrap().1;
        let cacheable = matches!(
            cursor.status(),
            Some("reachable" | "unreachable" | "unknown" | "skipped")
        );
        let (_, cursor) = self.preflight_build.take().unwrap();
        let cache_bytes = 512usize.saturating_add(cursor.frontier_len().saturating_mul(32));
        let cursor_bytes = cursor.reserved_bytes();
        let result = cursor.into_result();
        if !cacheable {
            page.released_bytes = cursor_bytes;
            let mut stopped = PathResult::stopped(result.status, result.expanded);
            stopped.preflight_expanded = result.expanded;
            stopped.preflight_status = result.status;
            stopped.routing_scans = result.scans;
            stopped.frontier_entries_peak = result.frontier_peak;
            stopped.blocked_frontier = result.frontier;
            page.status = PreflightStatus::Stopped(stopped);
            return page;
        }
        // The completed cursor already holds at least this cache lease.
        // Transfer it instead of momentarily requiring two reservations.
        if cursor_bytes < cache_bytes || self.preflight.try_reserve(1).is_err() {
            page.released_bytes = cursor_bytes;
            let mut stopped = PathResult::stopped("resource-limit", result.expanded);
            stopped.preflight_expanded = result.expanded;
            stopped.preflight_status = "resource-limit";
            stopped.routing_scans = result.scans;
            stopped.frontier_entries_peak = result.frontier_peak;
            page.status = PreflightStatus::Stopped(stopped);
            return page;
        }
        page.released_bytes = cursor_bytes - cache_bytes;
        let bytes = cache_bytes;
        self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
        self.preflight.insert(
            pair,
            Preflight {
                status: result.status,
                frontier_peak: result.frontier_peak,
                blocked_frontier: result.frontier,
            },
        );
        page.status = PreflightStatus::Ready;
        page
    }

    fn successors_step(
        &mut self,
        key: TransitionKey,
        scan_room: usize,
        deadline: Instant,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> SuccessorStep {
        if self.successor_build.is_none()
            && (self.one_shot && self.transient_key == Some(key)
                || !self.one_shot && self.successors.contains_key(&key))
        {
            if scan_room == 0 || self.check_deadlines && Instant::now() >= deadline {
                return SuccessorStep::Pending(0);
            }
            self.cache_hits += 1;
            return SuccessorStep::Ready(1);
        }
        if self.successor_build.is_none() {
            if scan_room == 0 || self.check_deadlines && Instant::now() >= deadline {
                return SuccessorStep::Pending(0);
            }
            match reserve(256) {
                Reservation::Granted => {}
                Reservation::Retry => return SuccessorStep::Pending(0),
                Reservation::Denied => return SuccessorStep::ResourceLimit(0),
            }
            self.reserved_bytes = self.reserved_bytes.saturating_add(256);
            self.cache_misses += 1;
            let mut transitions = if self.one_shot {
                self.transient_key = None;
                std::mem::take(&mut self.transient_successors)
            } else {
                Vec::new()
            };
            transitions.clear();
            let mut blocked = if self.one_shot {
                std::mem::take(&mut self.transient_blocked)
            } else {
                Vec::new()
            };
            blocked.clear();
            self.successor_build = Some(SuccessorBuild {
                key,
                direction: 0,
                choice: None,
                transitions,
                blocked,
                pending_transition: None,
            });
        }
        assert_eq!(self.successor_build.as_ref().unwrap().key, key);
        let mut scans = 0usize;
        loop {
            let build = self.successor_build.as_mut().unwrap();
            if let Some(pending) = build.pending_transition.take() {
                let via = pending
                    .crossing
                    .then(|| self.crossings.get(&(pending.point, pending.direction)))
                    .flatten();
                let bytes =
                    256usize.saturating_add(via.map_or(0, |crossing| crossing.component.len()));
                match reserve(bytes) {
                    Reservation::Granted => {
                        self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
                        if build.transitions.try_reserve(1).is_err() {
                            return SuccessorStep::ResourceLimit(scans);
                        }
                        let via = if let Some(crossing) = via {
                            let Some(cloned) = try_clone_crossing(crossing) else {
                                return SuccessorStep::ResourceLimit(scans);
                            };
                            Some(cloned)
                        } else {
                            None
                        };
                        build.transitions.push(Transition {
                            point: pending.point,
                            direction: pending.direction,
                            via,
                        });
                    }
                    Reservation::Retry => {
                        build.pending_transition = Some(pending);
                        return SuccessorStep::Pending(scans);
                    }
                    Reservation::Denied => return SuccessorStep::ResourceLimit(scans),
                }
            }
            if build.direction == 4 {
                if !self.one_shot && self.successors.try_reserve(1).is_err() {
                    return SuccessorStep::ResourceLimit(scans);
                }
                let build = self.successor_build.take().unwrap();
                if self.one_shot {
                    self.transient_successors = build.transitions;
                    self.transient_blocked = build.blocked;
                    self.transient_key = Some(key);
                } else {
                    self.successors.insert(key, build.transitions);
                }
                return SuccessorStep::Ready(scans);
            }
            if scans == scan_room || self.check_deadlines && Instant::now() >= deadline {
                return SuccessorStep::Pending(scans);
            }
            let direction = build.direction;
            scans += 1;
            if let Some(choice) = build.choice {
                let Some(next) = checked_offset(key.point, direction) else {
                    build.direction += 1;
                    build.choice = None;
                    continue;
                };
                let site = if key.crossing_limit == 0 {
                    None
                } else {
                    self.crossings.get(&(next, direction))
                };
                let via = if choice == 0 { None } else { site };
                if self.one_shot
                    && choice == 0
                    && self.blocked.contains(&next)
                    && (site.is_none() || key.crossings_used >= key.crossing_limit)
                {
                    match reserve(64) {
                        Reservation::Granted => {}
                        Reservation::Retry => return SuccessorStep::Pending(scans),
                        Reservation::Denied => return SuccessorStep::ResourceLimit(scans),
                    }
                    self.reserved_bytes = self.reserved_bytes.saturating_add(64);
                    if build.blocked.try_reserve(1).is_err() {
                        return SuccessorStep::ResourceLimit(scans);
                    }
                    build.blocked.push(next);
                }
                if !(via.is_none() && self.blocked.contains(&next)
                    || via.is_some() && key.crossings_used >= key.crossing_limit)
                {
                    let bytes =
                        256usize.saturating_add(via.map_or(0, |crossing| crossing.component.len()));
                    match reserve(bytes) {
                        Reservation::Granted => {
                            self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
                            if build.transitions.try_reserve(1).is_err() {
                                return SuccessorStep::ResourceLimit(scans);
                            }
                            let via = if let Some(crossing) = via {
                                let Some(cloned) = try_clone_crossing(crossing) else {
                                    return SuccessorStep::ResourceLimit(scans);
                                };
                                Some(cloned)
                            } else {
                                None
                            };
                            build.transitions.push(Transition {
                                point: next,
                                direction,
                                via,
                            });
                        }
                        Reservation::Retry => {
                            build.pending_transition = Some(PendingTransition {
                                point: next,
                                direction,
                                crossing: via.is_some(),
                            });
                        }
                        Reservation::Denied => return SuccessorStep::ResourceLimit(scans),
                    }
                }
                if choice < usize::from(site.is_some()) {
                    build.choice = Some(choice + 1);
                } else {
                    build.direction += 1;
                    build.choice = None;
                }
                if build.pending_transition.is_some() {
                    return SuccessorStep::Pending(scans);
                }
            } else if key.entered_bridge && direction != key.direction
                || key.direction == 4 && key.first.is_some_and(|first| direction != first)
                || key.direction != 4 && (direction + 2) % 4 == key.direction
            {
                build.direction += 1;
            } else {
                let Some(next) = checked_offset(key.point, direction) else {
                    build.direction += 1;
                    continue;
                };
                if !self.bounds.contains(next)
                    || next == key.start
                    || next == key.end && key.last.is_some_and(|last| direction != last)
                {
                    build.direction += 1;
                } else {
                    build.choice = Some(0);
                }
            }
        }
    }

    fn transitions(&self, key: &TransitionKey) -> &[Transition] {
        if self.one_shot {
            debug_assert_eq!(self.transient_key, Some(*key));
            &self.transient_successors
        } else {
            self.successors.get(key).expect("successors are ready")
        }
    }
}

impl<'a> Frontier<'a> {
    pub fn new(
        graph: &mut PreparedGraph<'_>,
        start: Point,
        end: Point,
        first: Option<usize>,
        last: Option<usize>,
        crossing_limit: usize,
        prefer_crossings: bool,
        work_budget: usize,
        charged_preflight_expanded: usize,
        charged_preflight_scans: usize,
        cell_penalties: Option<Arc<IndexMap<Point, u32>>>,
        edge_penalties: Option<Arc<IndexMap<(Point, usize), u32>>>,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> Result<Self, PathResult> {
        Self::new_with_maps(
            graph,
            start,
            end,
            first,
            last,
            crossing_limit,
            prefer_crossings,
            work_budget,
            charged_preflight_expanded,
            charged_preflight_scans,
            cell_penalties.map(PenaltyMap::Shared),
            edge_penalties.map(PenaltyMap::Shared),
            reserve,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_borrowed(
        graph: &mut PreparedGraph<'_>,
        start: Point,
        end: Point,
        first: Option<usize>,
        last: Option<usize>,
        crossing_limit: usize,
        prefer_crossings: bool,
        work_budget: usize,
        charged_preflight_expanded: usize,
        charged_preflight_scans: usize,
        cell_penalties: Option<&'a IndexMap<Point, u32>>,
        edge_penalties: Option<&'a IndexMap<(Point, usize), u32>>,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> Result<Self, PathResult> {
        Self::new_with_maps(
            graph,
            start,
            end,
            first,
            last,
            crossing_limit,
            prefer_crossings,
            work_budget,
            charged_preflight_expanded,
            charged_preflight_scans,
            cell_penalties.map(PenaltyMap::Borrowed),
            edge_penalties.map(PenaltyMap::Borrowed),
            reserve,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_maps(
        graph: &mut PreparedGraph<'_>,
        start: Point,
        end: Point,
        first: Option<usize>,
        last: Option<usize>,
        crossing_limit: usize,
        prefer_crossings: bool,
        work_budget: usize,
        charged_preflight_expanded: usize,
        charged_preflight_scans: usize,
        cell_penalties: Option<PenaltyMap<'a, Point>>,
        edge_penalties: Option<PenaltyMap<'a, (Point, usize)>>,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> Result<Self, PathResult> {
        let early_stop = |status| {
            let mut result = PathResult::stopped(status, charged_preflight_expanded);
            result.preflight_expanded = charged_preflight_expanded;
            result.routing_scans = charged_preflight_scans;
            result
        };
        if !graph.bounds.contains(start)
            || !graph.bounds.contains(end)
            || graph.blocked.contains(&start)
            || graph.blocked.contains(&end)
        {
            return Err(early_stop("blocked-endpoint"));
        }
        if start == end {
            return Err(early_stop("same-endpoint"));
        }
        let crossing_limit = clipped_crossing_limit(graph.bounds, crossing_limit);
        let pair = PairKey {
            start,
            end,
            first,
            last,
            crossing_limit,
            probe_cap: 256.min(work_budget / 4),
        };
        let skipped = Preflight {
            status: "skipped",
            frontier_peak: 0,
            blocked_frontier: Vec::new(),
        };
        let preflight = if pair.probe_cap == 0 {
            &skipped
        } else {
            graph
                .preflight
                .get(&pair)
                .expect("frontier requires a completed matching reverse probe")
        };
        // `advance_preflight` already charged the probe. Its cache lease uses
        // 32 bytes per blocked point, covering the cached Vec and one stopped
        // result copy while both are live.
        if preflight.status == "unreachable" {
            let mut result = PathResult::stopped("no-path", charged_preflight_expanded);
            result.preflight_expanded = charged_preflight_expanded;
            result.preflight_status = preflight.status;
            result.routing_scans = charged_preflight_scans;
            result.frontier_entries_peak = preflight.frontier_peak;
            if result
                .blocked_frontier
                .try_reserve(preflight.blocked_frontier.len())
                .is_ok()
            {
                result
                    .blocked_frontier
                    .extend_from_slice(&preflight.blocked_frontier);
            }
            return Err(result);
        }
        if preflight.status == "deadline" {
            let mut result = PathResult::stopped("deadline", charged_preflight_expanded);
            result.preflight_expanded = charged_preflight_expanded;
            result.preflight_status = preflight.status;
            result.routing_scans = charged_preflight_scans;
            result.frontier_entries_peak = preflight.frontier_peak;
            return Err(result);
        }
        let initial = StateKey::new(start, 4, 0, false);
        let Some(initial_priority) = Score::ZERO.with_heuristic(start, end) else {
            return Err(early_stop("resource-limit"));
        };
        let mut states = Vec::new();
        let mut heap = Vec::new();
        let mut best = HashMap::with_hasher(RandomState::new());
        if states.try_reserve(1).is_err()
            || heap.try_reserve(1).is_err()
            || best.try_reserve(1).is_err()
        {
            return Err(early_stop("resource-limit"));
        }
        let initial_reservation = reserve(512);
        if initial_reservation != Reservation::Granted {
            let mut result = PathResult::stopped(
                if initial_reservation == Reservation::Retry {
                    "memory-pending"
                } else {
                    "resource-limit"
                },
                charged_preflight_expanded,
            );
            result.preflight_expanded = charged_preflight_expanded;
            result.preflight_status = preflight.status;
            result.routing_scans = charged_preflight_scans;
            result.frontier_entries_peak = preflight.frontier_peak;
            return Err(result);
        }
        states.push(State {
            id: initial,
            g: Score::ZERO,
            priority_cost: initial_priority.path_cost,
            via: None,
            parent: None,
        });
        heap.push(0);
        best.insert(initial, 0);
        Ok(Self {
            start,
            end,
            first,
            last,
            crossing_limit,
            prefer_crossings,
            cell_penalties,
            edge_penalties,
            states,
            heap,
            best,
            expanded: 0,
            scans: 0,
            frontier_peak: preflight.frontier_peak.max(1),
            blocked_frontier: IndexSet::new(),
            preflight_status: preflight.status,
            preflight_expanded: charged_preflight_expanded,
            preflight_scans: charged_preflight_scans,
            completed: false,
            reserved_bytes: 512,
            active_expansion: None,
            found_path_bytes: 0,
        })
    }

    fn heap_less(&self, a: usize, b: usize) -> bool {
        let a_state = &self.states[a];
        let b_state = &self.states[b];
        let preference = |used: usize| {
            if self.prefer_crossings {
                -(used as i128)
            } else {
                used as i128
            }
        };
        (
            (a_state.priority_cost, a_state.g.turns),
            a_state.g,
            preference(a_state.id.crossings_used),
            a_state.id,
            a,
        ) < (
            (b_state.priority_cost, b_state.g.turns),
            b_state.g,
            preference(b_state.id.crossings_used),
            b_state.id,
            b,
        )
    }

    fn heap_push(&mut self, index: usize) {
        self.heap.push(index);
        let mut child = self.heap.len() - 1;
        while child > 0 {
            let parent = (child - 1) / 2;
            if !self.heap_less(self.heap[child], self.heap[parent]) {
                break;
            }
            self.heap.swap(child, parent);
            child = parent;
        }
    }

    fn heap_pop(&mut self) -> Option<usize> {
        let root = *self.heap.first()?;
        let last = self.heap.pop().unwrap();
        if !self.heap.is_empty() {
            self.heap[0] = last;
            let mut parent = 0;
            loop {
                let left = parent * 2 + 1;
                if left >= self.heap.len() {
                    break;
                }
                let right = left + 1;
                let child = if right < self.heap.len()
                    && self.heap_less(self.heap[right], self.heap[left])
                {
                    right
                } else {
                    left
                };
                if !self.heap_less(self.heap[child], self.heap[parent]) {
                    break;
                }
                self.heap.swap(child, parent);
                parent = child;
            }
        }
        Some(root)
    }

    pub fn work_counts(&self) -> (usize, usize, usize) {
        (self.expanded, self.scans, self.frontier_peak)
    }

    pub fn preflight_status(&self) -> &'static str {
        self.preflight_status
    }

    pub fn retained_states(&self) -> usize {
        self.states
            .len()
            .saturating_add(self.heap.len())
            .saturating_add(self.best.len())
    }

    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    /// Move the completed path reservation to its next owner while the
    /// search's expendable frontier reservation stays with this frontier.
    pub fn take_found_path_bytes(&mut self) -> usize {
        assert!(
            self.completed,
            "only a completed search may transfer its path"
        );
        let bytes = std::mem::take(&mut self.found_path_bytes);
        self.reserved_bytes -= bytes;
        bytes
    }

    pub fn advance(
        &mut self,
        graph: &mut PreparedGraph<'_>,
        max_new_expansions: usize,
        max_new_scans: usize,
        page_deadline: Instant,
        hard_deadline: Instant,
        hard_expansions: usize,
        hard_scans: usize,
        hard_frontier: usize,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> Advance {
        assert!(!self.completed, "completed frontier must not be resumed");
        let page_expansion_end = self.expanded.saturating_add(max_new_expansions);
        let page_scan_end = self.scans.saturating_add(max_new_scans);
        loop {
            if graph.check_deadlines && Instant::now() >= hard_deadline {
                return self.finish_stopped("deadline");
            }
            if graph.check_deadlines && Instant::now() >= page_deadline {
                return Advance::Pending;
            }
            if self.active_expansion.is_none() {
                if self.heap.is_empty() {
                    return self.finish_stopped("no-path");
                }
                if self.scans >= hard_scans {
                    return self.finish_stopped("scan-budget");
                }
                if self.expanded >= hard_expansions {
                    return self.finish_stopped("work-budget");
                }
                if self.scans >= page_scan_end || self.expanded >= page_expansion_end {
                    return Advance::Pending;
                }
                let Some(index) = self.heap_pop() else {
                    return self.finish_stopped("no-path");
                };
                self.scans += 1;
                let state = &self.states[index];
                if self.best.get(&state.id) != Some(&index) {
                    continue;
                }
                if self.expanded >= hard_expansions {
                    return self.finish_stopped("work-budget");
                }
                self.expanded += 1;
                self.active_expansion = Some(ActiveExpansion {
                    index,
                    successors_ready: false,
                    blocked_transfer_index: 0,
                    next_transition: 0,
                    found: None,
                });
            }
            let active = self.active_expansion.as_ref().unwrap();
            let index = active.index;
            let state = &self.states[index];
            let state_point = state.id.point();
            let state_direction = usize::from(state.id.direction);
            let state_crossings_used = state.id.crossings_used;
            let state_g = state.g;
            let entered_bridge = state.via.is_some();
            if state_point == self.end && self.last.is_none_or(|last| last == state_direction) {
                if self.active_expansion.as_ref().unwrap().found.is_none() {
                    self.active_expansion.as_mut().unwrap().found = Some(FoundBuild {
                        current: Some(index),
                        cells: Vec::new(),
                        crossings: Vec::new(),
                        seen: IndexSet::new(),
                        pending_check: None,
                    });
                }
                loop {
                    let found = self
                        .active_expansion
                        .as_mut()
                        .unwrap()
                        .found
                        .as_mut()
                        .unwrap();
                    if found.current.is_none() && found.pending_check.is_none() {
                        break;
                    }
                    if self.scans >= hard_scans {
                        return self.finish_stopped("scan-budget");
                    }
                    if self.scans >= page_scan_end
                        || graph.check_deadlines && Instant::now() >= page_deadline
                    {
                        return Advance::Pending;
                    }
                    if let Some(point) = found.pending_check.take() {
                        self.scans += 1;
                        if found.seen.contains(&point) {
                            return self.finish_stopped("repeated-cell");
                        }
                        if found.seen.len() == found.seen.capacity()
                            && found.seen.try_reserve(1).is_err()
                        {
                            return self.finish_stopped("resource-limit");
                        }
                        found.seen.insert(point);
                    } else if let Some(i) = found.current {
                        let path_state = &self.states[i];
                        let bytes = 128usize.saturating_add(path_state.via.as_ref().map_or(
                            0,
                            |crossing| {
                                std::mem::size_of::<Crossing>()
                                    .saturating_mul(2)
                                    .saturating_add(crossing.component.len())
                            },
                        ));
                        match reserve(bytes) {
                            Reservation::Granted => {}
                            Reservation::Retry => return Advance::Pending,
                            Reservation::Denied => return self.finish_stopped("resource-limit"),
                        }
                        self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
                        self.found_path_bytes = self.found_path_bytes.saturating_add(bytes);
                        if found.cells.len() == found.cells.capacity()
                            && found.cells.try_reserve(1).is_err()
                        {
                            return self.finish_stopped("resource-limit");
                        }
                        found.cells.push(path_state.id.point());
                        if let Some(crossing) = &path_state.via {
                            if found.crossings.len() == found.crossings.capacity()
                                && found.crossings.try_reserve(1).is_err()
                            {
                                return self.finish_stopped("resource-limit");
                            }
                            let Some(cloned) = try_clone_crossing(crossing) else {
                                return self.finish_stopped("resource-limit");
                            };
                            found.crossings.push(cloned);
                        }
                        found.pending_check = Some(path_state.id.point());
                        found.current = path_state.parent;
                        self.scans += 1;
                    }
                }
                return self.finish_found(index);
            }
            let key = TransitionKey {
                point: state_point,
                direction: state_direction,
                crossings_used: state_crossings_used,
                entered_bridge,
                start: self.start,
                end: self.end,
                first: self.first,
                last: self.last,
                crossing_limit: self.crossing_limit,
            };
            if !active.successors_ready {
                if self.scans >= hard_scans {
                    return self.finish_stopped("scan-budget");
                }
                let scan_room = hard_scans
                    .saturating_sub(self.scans)
                    .min(page_scan_end.saturating_sub(self.scans));
                let step = graph.successors_step(key, scan_room, page_deadline, reserve);
                match step {
                    SuccessorStep::Pending(scans) => {
                        self.scans = self.scans.saturating_add(scans);
                        return Advance::Pending;
                    }
                    SuccessorStep::ResourceLimit(scans) => {
                        self.scans = self.scans.saturating_add(scans);
                        return self.finish_stopped("resource-limit");
                    }
                    SuccessorStep::Ready(scans) => {
                        self.scans = self.scans.saturating_add(scans);
                        self.active_expansion.as_mut().unwrap().successors_ready = true;
                    }
                }
            }
            if graph.one_shot {
                while self
                    .active_expansion
                    .as_ref()
                    .unwrap()
                    .blocked_transfer_index
                    < graph.transient_blocked.len()
                {
                    let point = graph.transient_blocked[self
                        .active_expansion
                        .as_ref()
                        .unwrap()
                        .blocked_transfer_index];
                    if self.blocked_frontier.len() < 2048 && !self.blocked_frontier.contains(&point)
                    {
                        match reserve(64) {
                            Reservation::Granted => {}
                            Reservation::Retry => return Advance::Pending,
                            Reservation::Denied => return self.finish_stopped("resource-limit"),
                        }
                        self.reserved_bytes = self.reserved_bytes.saturating_add(64);
                        if self.blocked_frontier.try_reserve(1).is_err() {
                            return self.finish_stopped("resource-limit");
                        }
                        self.blocked_frontier.insert(point);
                    }
                    self.active_expansion
                        .as_mut()
                        .unwrap()
                        .blocked_transfer_index += 1;
                }
            }
            let transitions = graph.transitions(&key);
            while self.active_expansion.as_ref().unwrap().next_transition < transitions.len() {
                if graph.check_deadlines && Instant::now() >= page_deadline {
                    return Advance::Pending;
                }
                let next_transition = self.active_expansion.as_ref().unwrap().next_transition;
                let transition = &transitions[next_transition];
                let Some(used) =
                    state_crossings_used.checked_add(usize::from(transition.via.is_some()))
                else {
                    return self.finish_stopped("resource-limit");
                };
                let penalty = self
                    .cell_penalties
                    .as_ref()
                    .and_then(|map| map.get(&transition.point))
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(
                        self.edge_penalties
                            .as_ref()
                            .and_then(|map| map.get(&(transition.point, transition.direction)))
                            .copied()
                            .unwrap_or(0),
                    )
                    .min(1_000_000);
                let Some(g) = state_g.with_step(
                    penalty,
                    state_direction != 4 && state_direction != transition.direction,
                ) else {
                    return self.finish_stopped("resource-limit");
                };
                let id = StateKey::new(
                    transition.point,
                    transition.direction,
                    used,
                    transition.via.is_some(),
                );
                if self
                    .best
                    .get(&id)
                    .is_some_and(|&old| self.states[old].g <= g)
                {
                    self.active_expansion.as_mut().unwrap().next_transition += 1;
                    continue;
                }
                let Some(priority) = g.with_heuristic(transition.point, self.end) else {
                    return self.finish_stopped("resource-limit");
                };
                let bytes = 256usize.saturating_add(
                    transition
                        .via
                        .as_ref()
                        .map_or(0, |crossing| crossing.component.len()),
                );
                match reserve(bytes) {
                    Reservation::Granted => {}
                    Reservation::Retry => return Advance::Pending,
                    Reservation::Denied => return self.finish_stopped("resource-limit"),
                }
                self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
                if self.best.len() == self.best.capacity() && self.best.try_reserve(1).is_err()
                    || self.states.len() == self.states.capacity()
                        && self.states.try_reserve(1).is_err()
                    || self.heap.len() == self.heap.capacity() && self.heap.try_reserve(1).is_err()
                {
                    return self.finish_stopped("resource-limit");
                }
                let via = if let Some(crossing) = &transition.via {
                    let Some(cloned) = try_clone_crossing(crossing) else {
                        return self.finish_stopped("resource-limit");
                    };
                    Some(cloned)
                } else {
                    None
                };
                let next_index = self.states.len();
                self.best.insert(id, next_index);
                self.states.push(State {
                    id,
                    g,
                    priority_cost: priority.path_cost,
                    via,
                    parent: Some(index),
                });
                self.heap_push(next_index);
                self.frontier_peak = self.frontier_peak.max(self.heap.len());
                if self.frontier_peak > hard_frontier {
                    return self.finish_stopped("frontier-budget");
                }
                self.active_expansion.as_mut().unwrap().next_transition += 1;
            }
            self.active_expansion = None;
        }
    }

    fn finish_found(&mut self, index: usize) -> Advance {
        let mut found = self
            .active_expansion
            .as_mut()
            .unwrap()
            .found
            .take()
            .expect("found path was built");
        let cells = &mut found.cells;
        let crossings = &mut found.crossings;
        cells.reverse();
        crossings.reverse();
        let mut result = PathResult::stopped(
            "found",
            self.expanded.saturating_add(self.preflight_expanded),
        );
        let Some(steps) = cells
            .len()
            .checked_sub(1)
            .and_then(|n| i64::try_from(n).ok())
        else {
            return self.finish_stopped("resource-limit");
        };
        let Ok(turns) = i64::try_from(self.states[index].g.turns) else {
            return self.finish_stopped("resource-limit");
        };
        result.steps = Some(steps);
        result.turns = Some(turns);
        result.cells = Some(std::mem::take(cells));
        result.crossings = Some(std::mem::take(crossings));
        self.decorate(&mut result);
        self.completed = true;
        Advance::Complete(result)
    }

    fn finish_stopped(&mut self, status: &'static str) -> Advance {
        let mut result = PathResult::stopped(
            status,
            self.expanded.saturating_add(self.preflight_expanded),
        );
        if result
            .blocked_frontier
            .try_reserve(self.blocked_frontier.len())
            .is_ok()
        {
            result
                .blocked_frontier
                .extend(self.blocked_frontier.iter().copied());
        } else {
            result.status = "resource-limit";
        }
        self.decorate(&mut result);
        self.completed = true;
        Advance::Complete(result)
    }

    fn decorate(&self, result: &mut PathResult) {
        result.preflight_status = self.preflight_status;
        result.preflight_expanded = self.preflight_expanded;
        result.routing_scans = self.scans.saturating_add(self.preflight_scans);
        result.frontier_entries_peak = self.frontier_peak;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use aic_layout::logistics::{LogisticsComponentKind, TransportKind};

    use super::*;
    use crate::shortest_path;

    fn prepare_pair(
        graph: &mut PreparedGraph<'_>,
        start: Point,
        end: Point,
        work_budget: usize,
        deadline: Instant,
    ) -> PreflightPage {
        loop {
            let page = graph.advance_preflight(
                start,
                end,
                None,
                None,
                0,
                work_budget,
                usize::MAX,
                usize::MAX,
                usize::MAX,
                deadline,
                deadline,
                &mut |_| Reservation::Granted,
            );
            match page.status {
                PreflightStatus::Pending => continue,
                PreflightStatus::Ready => return page,
                PreflightStatus::Stopped(_) => panic!("test pair must finish preflight"),
            }
        }
    }

    #[test]
    fn crossing_successors_resume_after_each_of_twelve_scans() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let center = Point { x: 2, y: 2 };
        let mut crossings = Crossings::new();
        for direction in 0..4 {
            let point = center.offset(direction);
            crossings.insert(
                (point, direction),
                Crossing {
                    position: point,
                    direction,
                    component: "bridge".repeat(1024),
                    rotation: 0,
                    transport: TransportKind::Belt,
                    kind: LogisticsComponentKind::Bridge,
                    carrier_chain: 0,
                    carrier_network: 0,
                },
            );
        }
        let mut graph = PreparedGraph::new(bounds, IndexSet::new(), crossings, vec![3]);
        let key = TransitionKey {
            point: center,
            direction: 4,
            crossings_used: 0,
            entered_bridge: false,
            start: Point { x: 0, y: 0 },
            end: Point { x: 4, y: 4 },
            first: None,
            last: None,
            crossing_limit: 1,
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut scans = 0;
        let mut reserved = 0;
        let mut retried = false;
        loop {
            match graph.successors_step(key, 1, deadline, &mut |bytes| {
                if bytes > 256 && !retried {
                    retried = true;
                    return Reservation::Retry;
                }
                reserved += bytes;
                Reservation::Granted
            }) {
                SuccessorStep::Pending(delta) => {
                    assert!(delta <= 1);
                    scans += delta;
                }
                SuccessorStep::Ready(delta) => {
                    scans += delta;
                    break;
                }
                SuccessorStep::ResourceLimit(_) => panic!("unlimited reservation refused"),
            }
        }
        assert_eq!(scans, 12);
        assert!(retried);
        assert_eq!(graph.successors[&key].len(), 8);
        assert_eq!(reserved, graph.reserved_bytes());
        assert!(reserved >= 256 + 8 * 256 + 4 * 6 * 1024);
        assert_eq!(graph.cache_counts(), (0, 1));
        assert!(matches!(
            graph.successors_step(key, 1, deadline, &mut |_| Reservation::Granted),
            SuccessorStep::Ready(1)
        ));
        assert_eq!(graph.cache_counts(), (1, 1));
    }

    #[test]
    fn reverse_probe_completes_through_single_work_unit_pages() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let start = Point { x: 1, y: 1 };
        let end = Point { x: 2, y: 1 };
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut graph = PreparedGraph::new(bounds, IndexSet::new(), Crossings::new(), vec![4]);
        let mut total_expanded = 0;
        let mut total_scans = 0;
        let mut live = 0;
        let mut retry_seen = false;
        for _ in 0..1000 {
            let page = graph.advance_preflight(
                start,
                end,
                None,
                None,
                0,
                100,
                1,
                1,
                usize::MAX,
                deadline,
                deadline,
                &mut |bytes| {
                    if bytes == 128 && !retry_seen {
                        retry_seen = true;
                        return Reservation::Retry;
                    }
                    live += bytes;
                    Reservation::Granted
                },
            );
            live -= page.released_bytes;
            total_expanded += page.expanded;
            total_scans += page.scans;
            match page.status {
                PreflightStatus::Pending => continue,
                PreflightStatus::Ready => break,
                PreflightStatus::Stopped(result) => {
                    panic!("unexpected probe stop: {}", result.status)
                }
            }
        }
        let preflight = graph.preflight.values().next().expect("probe completed");
        assert!(total_expanded > 0);
        assert!(total_scans > 0);
        assert_eq!(preflight.status, "reachable");
        assert!(retry_seen);
        assert_eq!(live, graph.reserved_bytes());
        let cached = graph.advance_preflight(
            start,
            end,
            None,
            None,
            0,
            100,
            1,
            1,
            usize::MAX,
            deadline,
            deadline,
            &mut |_| panic!("cached preflight must not reserve"),
        );
        assert!(matches!(cached.status, PreflightStatus::Ready));
        assert_eq!((cached.expanded, cached.scans), (0, 0));
    }

    #[test]
    fn small_search_budget_skips_reverse_probe_without_cache_reservation() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        for work_budget in 1..4 {
            let mut graph = PreparedGraph::new(bounds, IndexSet::new(), Crossings::new(), vec![5]);
            let page = graph.advance_preflight(
                Point { x: 1, y: 1 },
                Point { x: 2, y: 1 },
                None,
                None,
                0,
                work_budget,
                1,
                1,
                usize::MAX,
                deadline,
                deadline,
                &mut |_| panic!("skipped probe must not reserve"),
            );
            assert!(matches!(page.status, PreflightStatus::Ready));
            assert_eq!(
                (page.expanded, page.scans, graph.reserved_bytes()),
                (0, 0, 0)
            );
            let frontier = Frontier::new(
                &mut graph,
                Point { x: 1, y: 1 },
                Point { x: 2, y: 1 },
                None,
                None,
                0,
                false,
                work_budget,
                0,
                0,
                None,
                None,
                &mut |_| Reservation::Granted,
            )
            .unwrap();
            assert_eq!(frontier.preflight_status, "skipped");
            assert!(graph.preflight.is_empty());
        }
    }

    #[test]
    fn found_path_reservation_retry_does_not_repeat_search_work() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        let run = |retry_path: bool| {
            let mut graph = PreparedGraph::new(bounds, IndexSet::new(), Crossings::new(), vec![6]);
            let probe = prepare_pair(
                &mut graph,
                Point { x: 0, y: 0 },
                Point { x: 4, y: 4 },
                1000,
                deadline,
            );
            assert!(probe.expanded > 0);
            let mut frontier = Frontier::new(
                &mut graph,
                Point { x: 0, y: 0 },
                Point { x: 4, y: 4 },
                None,
                None,
                0,
                false,
                1000,
                probe.expanded,
                probe.scans,
                None,
                None,
                &mut |_| Reservation::Granted,
            )
            .unwrap();
            assert_eq!(frontier.work_counts().0, 0);
            assert_eq!(frontier.work_counts().1, 0);
            let mut retried = false;
            let result = loop {
                match frontier.advance(
                    &mut graph,
                    1000,
                    10000,
                    deadline,
                    deadline,
                    1000 - probe.expanded,
                    10000,
                    1000,
                    &mut |bytes| {
                        if retry_path && bytes == 128 && !retried {
                            retried = true;
                            Reservation::Retry
                        } else {
                            Reservation::Granted
                        }
                    },
                ) {
                    Advance::Pending => continue,
                    Advance::Complete(result) => {
                        assert_eq!(result.expanded, probe.expanded + frontier.work_counts().0);
                        assert_eq!(result.routing_scans, probe.scans + frontier.work_counts().1);
                        break result;
                    }
                }
            };
            (result, frontier.work_counts(), retried)
        };
        let (baseline, baseline_work, _) = run(false);
        let (resumed, resumed_work, retried) = run(true);
        assert!(retried);
        assert_eq!(resumed.status, "found");
        assert_eq!(resumed.cells, baseline.cells);
        assert_eq!(resumed_work, baseline_work);
    }

    #[test]
    fn failed_frontier_reservation_does_not_repeat_completed_probe_work() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let mut graph = PreparedGraph::new(bounds, IndexSet::new(), Crossings::new(), vec![1]);
        let deadline = Instant::now() + Duration::from_secs(1);
        let page = prepare_pair(
            &mut graph,
            Point { x: 0, y: 0 },
            Point { x: 4, y: 4 },
            100,
            deadline,
        );
        assert!(page.expanded > 0);
        let before = graph.cache_counts();
        let result = Frontier::new(
            &mut graph,
            Point { x: 0, y: 0 },
            Point { x: 4, y: 4 },
            None,
            None,
            0,
            false,
            100,
            0,
            0,
            None,
            None,
            &mut |_| Reservation::Denied,
        )
        .err()
        .expect("reservation was denied after cached reverse preflight");
        assert_eq!(result.status, "resource-limit");
        assert_eq!(result.preflight_expanded, 0);
        assert_eq!(result.routing_scans, 0);
        assert!(result.frontier_entries_peak > 0);
        assert_eq!(graph.cache_counts(), before);
    }

    #[test]
    fn failed_successor_reservation_keeps_completed_scan_count() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let mut graph = PreparedGraph::new(bounds, IndexSet::new(), Crossings::new(), vec![2]);
        let deadline = Instant::now() + Duration::from_secs(1);
        prepare_pair(
            &mut graph,
            Point { x: 0, y: 0 },
            Point { x: 4, y: 4 },
            100,
            deadline,
        );
        let mut frontier = Frontier::new(
            &mut graph,
            Point { x: 0, y: 0 },
            Point { x: 4, y: 4 },
            None,
            None,
            0,
            false,
            100,
            0,
            0,
            None,
            None,
            &mut |_| Reservation::Granted,
        )
        .unwrap();
        let scans_before = frontier.work_counts().1;
        let mut reservations = 0;
        let result = frontier.advance(
            &mut graph,
            100,
            1000,
            deadline,
            deadline,
            1000,
            1000,
            1000,
            &mut |_| {
                reservations += 1;
                if reservations == 1 {
                    Reservation::Granted
                } else {
                    Reservation::Denied
                }
            },
        );
        assert!(matches!(result, Advance::Complete(ref path) if path.status == "resource-limit"));
        assert!(frontier.work_counts().1 > scans_before);
    }

    #[test]
    fn resumed_anchor_and_guided_frontiers_reuse_exact_successors() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let start = Point { x: 0, y: 2 };
        let end = Point { x: 4, y: 2 };
        let blocked = IndexSet::from_iter([Point { x: 2, y: 2 }]);
        let mut guidance = IndexMap::new();
        for x in 0..=4 {
            guidance.insert((Point { x, y: 1 }, 0), 100);
        }
        let mut graph = PreparedGraph::new(bounds, blocked.clone(), Crossings::new(), vec![1]);
        for penalties in [None, Some(guidance)] {
            let expected = shortest_path(
                start,
                end,
                &blocked,
                bounds,
                PathOptions {
                    first: None,
                    last: None,
                    work_budget: 10_000,
                    scan_budget: 1_000_000,
                    frontier_budget: 10_000,
                    deadline: None,
                    crossings: &Crossings::new(),
                    crossing_limit: 0,
                    prefer_crossings: false,
                    allow_zero_length: false,
                    cell_penalties: None,
                    edge_penalties: penalties.as_ref(),
                },
            )
            .unwrap();
            let hard_deadline = Instant::now() + Duration::from_secs(10);
            prepare_pair(&mut graph, start, end, 10_000, hard_deadline);
            let mut frontier = Frontier::new(
                &mut graph,
                start,
                end,
                None,
                None,
                0,
                false,
                10_000,
                0,
                0,
                None,
                penalties.map(Arc::new),
                &mut |_| Reservation::Granted,
            )
            .unwrap();
            let actual = loop {
                match frontier.advance(
                    &mut graph,
                    1,
                    1,
                    hard_deadline,
                    hard_deadline,
                    10_000,
                    1_000_000,
                    10_000,
                    &mut |_| Reservation::Granted,
                ) {
                    Advance::Pending => continue,
                    Advance::Complete(result) => break result,
                }
            };
            assert_eq!(actual.status, expected.status);
            assert_eq!(actual.cells, expected.cells);
            assert_eq!(actual.steps, expected.steps);
            assert_eq!(actual.turns, expected.turns);
        }
        assert!(graph.cache_counts().0 > 0);
        assert_eq!(graph.preflight.len(), 1);
    }
}
