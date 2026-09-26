//! Necessary directed reachability before crossing-count A*.
//!
//! Reverse states retain arrival direction and straight-through crossings, but
//! relax the crossing-count limit and permit repeated cells. Every legal path
//! is a walk in this graph. Only complete queue exhaustion proves the fixed
//! query unreachable; reaching the start or hitting the cap never rejects it.

use std::collections::VecDeque;

use indexmap::IndexSet;

use super::PathOptions;
use aic_layout::geometry::{Bounds, Point};

pub(super) struct Probe {
    pub status: &'static str,
    pub expanded: usize,
    pub scans: usize,
    pub frontier_peak: usize,
    pub frontier: Vec<Point>,
}

pub(super) fn probe(
    start: Point,
    end: Point,
    blocked: &IndexSet<Point>,
    bounds: Bounds,
    options: &PathOptions<'_>,
    cap: usize,
) -> Probe {
    let result = |status, expanded, scans, frontier_peak, frontier| Probe {
        status,
        expanded,
        scans,
        frontier_peak,
        frontier,
    };
    if cap == 0 {
        return result("skipped", 0, 0, 0, vec![]);
    }
    // Avoid reverse preprocessing for an immediately sealed departure. This
    // examines the same four outgoing transitions as the initial A* state.
    let mut frontier = IndexSet::new();
    let mut scans = 0usize;
    let departure = (0..4).any(|d| {
        scans += 1;
        if options.first.is_some_and(|first| d != first) {
            return false;
        }
        let q = start.offset(d);
        if blocked.contains(&q)
            && (options.crossing_limit == 0 || !options.crossings.contains_key(&(q, d)))
        {
            frontier.insert(q);
            return false;
        }
        bounds.contains(q) && (q != end || options.last.is_none_or(|last| last == d))
    });
    if scans > options.scan_budget {
        return result("scan-budget", 0, scans, 0, vec![]);
    }
    if !departure {
        return result("unreachable", 1, scans, 0, frontier.into_iter().collect());
    }
    frontier.clear();
    let mut seen = IndexSet::new();
    let mut queue = VecDeque::new();
    for direction in 0..4 {
        if options.last.is_none_or(|last| last == direction) {
            seen.insert((end, direction));
            queue.push_back((end, direction));
        }
    }
    let mut frontier_peak = queue.len();
    if frontier_peak > options.frontier_budget {
        return result("frontier-budget", 0, scans, frontier_peak, vec![]);
    }
    let mut expanded = 1;
    while let Some((q, direction)) = queue.pop_front() {
        scans += 1;
        if scans > options.scan_budget {
            return result("scan-budget", expanded, scans, frontier_peak, vec![]);
        }
        if expanded % 64 == 0
            && options
                .deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return result("deadline", expanded, scans, frontier_peak, vec![]);
        }
        if expanded == cap {
            return result("unknown", expanded, scans, frontier_peak, vec![]);
        }
        expanded += 1;
        if q == start {
            return result("reachable", expanded, scans, frontier_peak, vec![]);
        }
        let p = q.offset((direction + 2) % 4);
        if !bounds.contains(p) {
            continue;
        }
        if p == start {
            if options.first.is_none_or(|first| first == direction) && seen.insert((p, 4)) {
                queue.push_back((p, 4));
                frontier_peak = frontier_peak.max(queue.len());
                if frontier_peak > options.frontier_budget {
                    return result("frontier-budget", expanded, scans, frontier_peak, vec![]);
                }
            }
            continue;
        }
        let mut has_entry = false;
        for previous in 0..4 {
            scans += 1;
            if scans > options.scan_budget {
                return result("scan-budget", expanded, scans, frontier_peak, vec![]);
            }
            if (direction + 2) % 4 == previous {
                continue;
            }
            if p == end && options.last.is_some_and(|last| previous != last) {
                continue;
            }
            if blocked.contains(&p)
                && (previous != direction || !options.crossings.contains_key(&(p, previous)))
            {
                continue;
            }
            has_entry = true;
            if seen.insert((p, previous)) {
                queue.push_back((p, previous));
                frontier_peak = frontier_peak.max(queue.len());
                if frontier_peak > options.frontier_budget {
                    return result("frontier-budget", expanded, scans, frontier_peak, vec![]);
                }
            }
        }
        if !has_entry && blocked.contains(&p) && frontier.len() < 2048 {
            frontier.insert(p);
        }
    }
    result(
        "unreachable",
        expanded,
        scans,
        frontier_peak,
        frontier.into_iter().collect(),
    )
}
