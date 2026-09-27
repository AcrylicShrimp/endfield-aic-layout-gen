//! Necessary directed reachability before crossing-count A*.
//!
//! Reverse states retain arrival direction and straight-through crossings, but
//! relax the crossing-count limit and permit repeated cells. Every legal path
//! is a walk in this graph. Only complete queue exhaustion proves the fixed
//! query unreachable; reaching the start or hitting the cap never rejects it.

use std::collections::VecDeque;

use indexmap::IndexSet;

use super::{
    PathOptions,
    prepared::{Reservation, checked_offset},
};
use aic_layout::geometry::{Bounds, Point};

pub(super) struct Probe {
    pub status: &'static str,
    pub expanded: usize,
    pub scans: usize,
    pub frontier_peak: usize,
    pub frontier: Vec<Point>,
}

#[cfg(test)]
pub(super) fn probe(
    start: Point,
    end: Point,
    blocked: &IndexSet<Point>,
    bounds: Bounds,
    options: &PathOptions<'_>,
    cap: usize,
) -> Probe {
    let mut cursor = ProbeCursor::new(start, end, cap);
    let page_deadline = options.deadline.unwrap_or_else(|| {
        std::time::Instant::now() + std::time::Duration::from_secs(365 * 24 * 60 * 60)
    });
    loop {
        if cursor.advance(
            blocked,
            bounds,
            options,
            usize::MAX,
            usize::MAX,
            page_deadline,
            true,
            &mut |_| Reservation::Granted,
        ) {
            return cursor.into_result();
        }
    }
}

/// Retains the same reverse walk when a temporary search page ends midway.
pub(super) struct ProbeCursor {
    start: Point,
    end: Point,
    cap: usize,
    phase: ProbePhase,
    departure_direction: usize,
    seed_direction: usize,
    seed_complete: bool,
    queue: VecDeque<(Point, usize)>,
    seen: IndexSet<(Point, usize)>,
    frontier: IndexSet<Point>,
    active: Option<ReverseVisit>,
    pending_insert: Option<PendingInsert>,
    expanded: usize,
    scans: usize,
    frontier_peak: usize,
    reserved_bytes: usize,
    status: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum ProbePhase {
    Departure,
    Reverse,
}

struct ReverseVisit {
    previous_point: Point,
    direction: usize,
    previous_direction: usize,
    has_entry: bool,
}

#[derive(Clone, Copy)]
enum PendingInsert {
    Seen(Point, usize),
    Frontier(Point),
}

impl ProbeCursor {
    pub(super) fn new(start: Point, end: Point, cap: usize) -> Self {
        Self {
            start,
            end,
            cap,
            phase: ProbePhase::Departure,
            departure_direction: 0,
            seed_direction: 0,
            seed_complete: false,
            queue: VecDeque::new(),
            seen: IndexSet::new(),
            frontier: IndexSet::new(),
            active: None,
            pending_insert: None,
            expanded: 0,
            scans: 0,
            frontier_peak: 0,
            reserved_bytes: 0,
            status: None,
        }
    }

    pub(super) fn work_counts(&self) -> (usize, usize, usize) {
        (self.expanded, self.scans, self.frontier_peak)
    }

    pub(super) fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    pub(super) fn status(&self) -> Option<&'static str> {
        self.status
    }

    pub(super) fn frontier_len(&self) -> usize {
        self.frontier.len()
    }

    pub(super) fn into_result(self) -> Probe {
        let mut frontier = Vec::new();
        if frontier.try_reserve(self.frontier.len()).is_ok() {
            frontier.extend(self.frontier);
        }
        Probe {
            status: self.status.expect("completed reverse probe"),
            expanded: self.expanded,
            scans: self.scans,
            frontier_peak: self.frontier_peak,
            frontier,
        }
    }

    fn reserve(&mut self, bytes: usize, reserve: &mut dyn FnMut(usize) -> Reservation) -> bool {
        match reserve(bytes) {
            Reservation::Granted => {}
            Reservation::Retry => return false,
            Reservation::Denied => {
                self.status = Some("resource-limit");
                return false;
            }
        }
        self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
        true
    }

    fn add_seen(
        &mut self,
        point: Point,
        direction: usize,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> bool {
        if self.seen.contains(&(point, direction)) {
            return true;
        }
        if !self.reserve(128, reserve) {
            if self.status.is_none() {
                self.pending_insert = Some(PendingInsert::Seen(point, direction));
            }
            return false;
        }
        if self.seen.len() == self.seen.capacity() && self.seen.try_reserve(1).is_err()
            || self.queue.len() == self.queue.capacity() && self.queue.try_reserve(1).is_err()
        {
            self.status = Some("resource-limit");
            return false;
        }
        self.seen.insert((point, direction));
        self.queue.push_back((point, direction));
        self.frontier_peak = self.frontier_peak.max(self.queue.len());
        true
    }

    fn add_frontier(
        &mut self,
        point: Point,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> bool {
        if self.frontier.contains(&point) || self.frontier.len() == 2048 {
            return true;
        }
        if !self.reserve(64, reserve) {
            if self.status.is_none() {
                self.pending_insert = Some(PendingInsert::Frontier(point));
            }
            return false;
        }
        if self.frontier.len() == self.frontier.capacity() && self.frontier.try_reserve(1).is_err()
        {
            self.status = Some("resource-limit");
            return false;
        }
        self.frontier.insert(point);
        true
    }

    /// Returns true only when a terminal result is ready. Page stops retain
    /// the exact departure, queue and predecessor-direction positions.
    pub(super) fn advance(
        &mut self,
        blocked: &IndexSet<Point>,
        bounds: Bounds,
        options: &PathOptions<'_>,
        max_new_expansions: usize,
        max_new_scans: usize,
        page_deadline: std::time::Instant,
        check_page_deadline: bool,
        reserve: &mut dyn FnMut(usize) -> Reservation,
    ) -> bool {
        if self.status.is_some() {
            return true;
        }
        if self.cap == 0 {
            self.status = Some("skipped");
            return true;
        }
        let expansion_end = self.expanded.saturating_add(max_new_expansions);
        let scan_end = self.scans.saturating_add(max_new_scans);
        if options
            .deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            self.status = Some("deadline");
            return true;
        }
        if check_page_deadline && std::time::Instant::now() >= page_deadline {
            return false;
        }
        if self.reserved_bytes == 0 && !self.reserve(512, reserve) {
            return self.status.is_some();
        }
        loop {
            if options
                .deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
            {
                self.status = Some("deadline");
                return true;
            }
            if check_page_deadline && std::time::Instant::now() >= page_deadline {
                return false;
            }
            if let Some(pending) = self.pending_insert.take() {
                let inserted = match pending {
                    PendingInsert::Seen(point, direction) => {
                        self.add_seen(point, direction, reserve)
                    }
                    PendingInsert::Frontier(point) => self.add_frontier(point, reserve),
                };
                if !inserted {
                    return self.status.is_some();
                }
                if self.frontier_peak > options.frontier_budget {
                    self.status = Some("frontier-budget");
                    return true;
                }
            }
            match self.phase {
                ProbePhase::Departure => {
                    if self.departure_direction == 4 {
                        if self.expanded >= expansion_end {
                            return false;
                        }
                        self.expanded += 1;
                        self.status = Some("unreachable");
                        return true;
                    }
                    if self.scans >= scan_end {
                        return false;
                    }
                    let d = self.departure_direction;
                    self.departure_direction += 1;
                    self.scans += 1;
                    if self.scans > options.scan_budget {
                        self.status = Some("scan-budget");
                        return true;
                    }
                    if options.first.is_some_and(|first| first != d) {
                        continue;
                    }
                    let Some(q) = checked_offset(self.start, d) else {
                        continue;
                    };
                    if blocked.contains(&q)
                        && (options.crossing_limit == 0 || !options.crossings.contains_key(&(q, d)))
                    {
                        if !self.add_frontier(q, reserve) {
                            return self.status.is_some();
                        }
                        continue;
                    }
                    if bounds.contains(q)
                        && (q != self.end || options.last.is_none_or(|last| last == d))
                    {
                        self.phase = ProbePhase::Reverse;
                    }
                }
                ProbePhase::Reverse => {
                    if !self.seed_complete {
                        if self.expanded == 0 {
                            if self.expanded >= expansion_end {
                                return false;
                            }
                            self.expanded = 1;
                            self.frontier.clear();
                        }
                        while self.seed_direction < 4 {
                            let direction = self.seed_direction;
                            self.seed_direction += 1;
                            if options.last.is_none_or(|last| last == direction)
                                && !self.add_seen(self.end, direction, reserve)
                            {
                                return self.status.is_some();
                            }
                        }
                        self.seed_complete = true;
                        if self.frontier_peak > options.frontier_budget {
                            self.status = Some("frontier-budget");
                            return true;
                        }
                    }
                    if let Some(active) = self.active.as_mut() {
                        if active.previous_direction == 4 {
                            let point = active.previous_point;
                            let add = !active.has_entry && blocked.contains(&point);
                            self.active = None;
                            if add && !self.add_frontier(point, reserve) {
                                return self.status.is_some();
                            }
                            continue;
                        }
                        if self.scans >= scan_end {
                            return false;
                        }
                        let previous = active.previous_direction;
                        active.previous_direction += 1;
                        self.scans += 1;
                        if self.scans > options.scan_budget {
                            self.status = Some("scan-budget");
                            return true;
                        }
                        let p = active.previous_point;
                        let direction = active.direction;
                        if (direction + 2) % 4 == previous
                            || p == self.end && options.last.is_some_and(|last| last != previous)
                            || blocked.contains(&p)
                                && (previous != direction
                                    || !options.crossings.contains_key(&(p, previous)))
                        {
                            continue;
                        }
                        active.has_entry = true;
                        if !self.add_seen(p, previous, reserve) {
                            return self.status.is_some();
                        }
                        if self.frontier_peak > options.frontier_budget {
                            self.status = Some("frontier-budget");
                            return true;
                        }
                        continue;
                    }
                    if self.queue.is_empty() {
                        self.status = Some("unreachable");
                        return true;
                    }
                    if self.scans >= scan_end || self.expanded >= expansion_end {
                        return false;
                    }
                    let (q, direction) = self.queue.pop_front().unwrap();
                    self.scans += 1;
                    if self.scans > options.scan_budget {
                        self.status = Some("scan-budget");
                        return true;
                    }
                    if self.expanded == self.cap {
                        self.status = Some("unknown");
                        return true;
                    }
                    self.expanded += 1;
                    if q == self.start {
                        self.status = Some("reachable");
                        return true;
                    }
                    let Some(p) = checked_offset(q, (direction + 2) % 4) else {
                        continue;
                    };
                    if !bounds.contains(p) {
                        continue;
                    }
                    if p == self.start {
                        if options.first.is_none_or(|first| first == direction)
                            && !self.add_seen(p, 4, reserve)
                        {
                            return self.status.is_some();
                        }
                        if self.frontier_peak > options.frontier_budget {
                            self.status = Some("frontier-budget");
                            return true;
                        }
                        continue;
                    }
                    self.active = Some(ReverseVisit {
                        previous_point: p,
                        direction,
                        previous_direction: 0,
                        has_entry: false,
                    });
                }
            }
        }
    }
}
