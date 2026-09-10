//! Necessary directed reachability before crossing-count A*.
//!
//! Reverse states retain arrival direction and straight-through crossings, but
//! relax the crossing-count limit and permit repeated cells. Every legal path
//! is a walk in this graph. Only complete queue exhaustion proves the fixed
//! query unreachable; reaching the start or hitting the cap never rejects it.

use std::collections::VecDeque;

use indexmap::IndexSet;

use super::PathOptions;
use crate::full_factory::types::{Bounds, Point};

pub(super) struct Probe {
    pub status: &'static str,
    pub expanded: usize,
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
    let result = |status, expanded, frontier| Probe {
        status,
        expanded,
        frontier,
    };
    if cap == 0 {
        return result("skipped", 0, vec![]);
    }
    // Avoid reverse preprocessing for an immediately sealed departure. This
    // examines the same four outgoing transitions as the initial A* state.
    let mut frontier = IndexSet::new();
    let departure = (0..4).any(|d| {
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
    if !departure {
        return result("unreachable", 1, frontier.into_iter().collect());
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
    let mut expanded = 1;
    while let Some((q, direction)) = queue.pop_front() {
        if expanded == cap {
            return result("unknown", expanded, vec![]);
        }
        expanded += 1;
        if q == start {
            return result("reachable", expanded, vec![]);
        }
        let p = q.offset((direction + 2) % 4);
        if !bounds.contains(p) {
            continue;
        }
        if p == start {
            if options.first.is_none_or(|first| first == direction) && seen.insert((p, 4)) {
                queue.push_back((p, 4));
            }
            continue;
        }
        let mut has_entry = false;
        for previous in 0..4 {
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
            }
        }
        if !has_entry && blocked.contains(&p) && frontier.len() < 2048 {
            frontier.insert(p);
        }
    }
    result("unreachable", expanded, frontier.into_iter().collect())
}
