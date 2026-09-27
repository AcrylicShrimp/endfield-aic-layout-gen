use std::collections::{HashSet, VecDeque};

use super::*;

#[test]
fn one_shot_router_admits_sparse_large_bounds_and_work_request() {
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 999,
        max_y: 999,
    };
    let crossings = Crossings::new();
    let result = shortest_path(
        Point { x: 0, y: 0 },
        Point { x: 2, y: 0 },
        &IndexSet::new(),
        bounds,
        PathOptions {
            first: None,
            last: None,
            work_budget: 1_000_001,
            scan_budget: 10_000,
            frontier_budget: 10_000,
            deadline: None,
            crossings: &crossings,
            crossing_limit: 0,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "found");
    assert_eq!(result.steps, Some(2));
}

#[test]
fn one_shot_router_checks_neighbor_at_coordinate_limit() {
    let bounds = Bounds {
        min_x: i64::MAX - 2,
        min_y: 0,
        max_x: i64::MAX,
        max_y: 0,
    };
    let crossings = Crossings::new();
    let result = shortest_path(
        Point { x: i64::MAX, y: 0 },
        Point {
            x: i64::MAX - 2,
            y: 0,
        },
        &IndexSet::new(),
        bounds,
        PathOptions {
            first: None,
            last: None,
            work_budget: 100,
            scan_budget: 1_000,
            frontier_budget: 100,
            deadline: None,
            crossings: &crossings,
            crossing_limit: 0,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "found");
    assert_eq!(result.steps, Some(2));
}

#[test]
fn reverse_probe_frontier_stop_does_not_claim_no_path() {
    let start = Point { x: 0, y: 0 };
    let end = Point { x: 8, y: 8 };
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 9,
        max_y: 9,
    };
    let blocked = (0..4).map(|direction| end.offset(direction)).collect();
    let crossings = Crossings::new();
    let result = shortest_path(
        start,
        end,
        &blocked,
        bounds,
        PathOptions {
            first: None,
            last: None,
            work_budget: 1_000,
            scan_budget: 10_000,
            frontier_budget: 1,
            deadline: None,
            crossings: &crossings,
            crossing_limit: 0,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "frontier-budget");
    assert!(result.frontier_entries_peak > 1);
}

#[test]
fn expired_deadline_cannot_be_reported_as_local_infeasibility() {
    let result = shortest_path(
        Point { x: 0, y: 0 },
        Point { x: 2, y: 2 },
        &IndexSet::new(),
        Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 2,
            max_y: 2,
        },
        PathOptions {
            first: None,
            last: None,
            work_budget: 100,
            scan_budget: 100,
            frontier_budget: 100,
            deadline: Some(Instant::now() - std::time::Duration::from_millis(1)),
            crossings: &Crossings::new(),
            crossing_limit: 0,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "deadline");
    assert_eq!(result.expanded, 0);
}

#[test]
fn shared_straight_trunk_offers_only_capacity_valid_bridge_sites() {
    let raw: aic_layout::logistics::LogisticsComponentCatalog = serde_json::from_slice(
        include_bytes!("../../../data/game/normalized/logistics-components.json"),
    )
    .unwrap();
    let catalogs = ValidatedLogisticsComponentCatalog::try_from_catalog(raw).unwrap();
    let left = (0..5).map(|x| Point { x, y: 2 }).collect::<Vec<_>>();
    let right = (1..6).map(|x| Point { x, y: 2 }).collect::<Vec<_>>();
    let rate = Rate {
        numerator: 1,
        denominator: 4,
    };
    let mut carriers = [
        Carrier {
            id: 0,
            network: 0,
            transport: TransportKind::Belt,
            rate,
            cells: &left,
        },
        Carrier {
            id: 1,
            network: 0,
            transport: TransportKind::Belt,
            rate,
            cells: &right,
        },
    ];
    let job = RouteJob {
        id: 2,
        transport: TransportKind::Belt,
        rate: Rate {
            numerator: 1,
            denominator: 2,
        },
    };
    let site = Point { x: 3, y: 2 };
    assert!(
        crossing_options(&carriers, &job, &catalogs, &IndexSet::new()).contains_key(&(site, 3))
    );
    let mut uninterrupted_visits = Vec::new();
    let uninterrupted =
        crossing_options_with_visit(&carriers, &job, &catalogs, &IndexSet::new(), |kind| {
            uninterrupted_visits.push(kind);
            Ok::<(), ()>(())
        })
        .unwrap();
    let mut cursor = CrossingOptionsCursor::new();
    let mut resumed_visits = Vec::new();
    let mut completed = false;
    for _ in 0..=uninterrupted_visits.len() {
        let mut remaining = 1;
        match cursor.advance(&carriers, &job, &catalogs, &IndexSet::new(), |kind| {
            if remaining == 0 {
                return Err(());
            }
            remaining -= 1;
            resumed_visits.push(kind);
            Ok(())
        }) {
            Ok(true) => {
                completed = true;
                break;
            }
            Err(()) => {}
            Ok(false) => unreachable!("an uninterrupted cursor completes"),
        }
    }
    assert!(completed);
    assert_eq!(cursor.finish(), uninterrupted);
    assert_eq!(resumed_visits, uninterrupted_visits);
    carriers[1].network = 1;
    assert!(
        !crossing_options(&carriers, &job, &catalogs, &IndexSet::new()).contains_key(&(site, 3))
    );
    carriers[1].network = 0;
    carriers[1].rate = Rate {
        numerator: 1,
        denominator: 2,
    };
    assert!(
        !crossing_options(&carriers, &job, &catalogs, &IndexSet::new()).contains_key(&(site, 3))
    );
}

#[test]
fn same_cell_share_and_bridge_are_separate_search_choices() {
    let site = Point { x: 1, y: 1 };
    let mut crossings = Crossings::new();
    crossings.insert(
        (site, 0),
        Crossing {
            position: site,
            direction: 0,
            component: "test-bridge".into(),
            rotation: 0,
            transport: TransportKind::Belt,
            kind: LogisticsComponentKind::Bridge,
            carrier_chain: 0,
            carrier_network: 0,
        },
    );
    let bounds = Bounds {
        min_x: 0,
        min_y: 1,
        max_x: 2,
        max_y: 1,
    };
    for (prefer_crossings, expected) in [(false, 0), (true, 1)] {
        let result = shortest_path(
            Point { x: 0, y: 1 },
            Point { x: 2, y: 1 },
            &IndexSet::new(),
            bounds,
            PathOptions {
                first: Some(0),
                last: Some(0),
                work_budget: 100,
                scan_budget: usize::MAX,
                frontier_budget: usize::MAX,
                deadline: None,
                crossings: &crossings,
                crossing_limit: 1,
                prefer_crossings,
                allow_zero_length: false,
                cell_penalties: None,
                edge_penalties: None,
            },
        )
        .unwrap();
        assert_eq!(result.status, "found");
        assert_eq!(result.crossings.unwrap().len(), expected);
    }
}

/// Explicit extraction/performance check; never part of normal routing or tests.
#[test]
#[ignore]
fn route_kernel_timing() {
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 63,
        max_y: 63,
    };
    let blocked: IndexSet<_> = (8..56)
        .step_by(8)
        .flat_map(|x| {
            (0..64)
                .filter(move |y| *y != (x * 3) % 59 && *y != 62)
                .map(move |y| Point { x, y })
        })
        .collect();
    let crossings = Crossings::new();
    let start = std::time::Instant::now();
    let mut expanded = 0;
    let mut steps = 0;
    for i in 0..200 {
        let result = shortest_path(
            Point { x: 2, y: i % 61 },
            Point {
                x: 61,
                y: (i * 7) % 61,
            },
            &blocked,
            bounds,
            PathOptions {
                first: None,
                last: None,
                work_budget: 100_000,
                scan_budget: usize::MAX,
                frontier_budget: usize::MAX,
                deadline: None,
                crossings: &crossings,
                crossing_limit: 0,
                prefer_crossings: false,
                allow_zero_length: false,
                cell_penalties: None,
                edge_penalties: None,
            },
        )
        .unwrap();
        assert_eq!(result.status, "found");
        expanded += result.expanded;
        steps += result.steps.unwrap();
    }
    eprintln!(
        "route_kernel_timing queries=200 expanded={expanded} steps={steps} elapsed_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
}

// Independent forward exploration of the finite crossing-count walk graph.
// This is intentionally stronger than a simple-path witness: a negative
// preflight must not discard even a reachable walk in the actual bounded graph.
fn forward_reachable(
    start: Point,
    end: Point,
    blocked: &IndexSet<Point>,
    bounds: Bounds,
    options: &PathOptions<'_>,
) -> bool {
    let mut queue = VecDeque::from([(start, 4, 0)]);
    let mut seen = HashSet::from([(start, 4, 0)]);
    while let Some((p, previous, used)) = queue.pop_front() {
        if p == end {
            return true;
        }
        for d in 0..4 {
            let q = p.offset(d);
            if !bounds.contains(q) || q == start {
                continue;
            }
            if previous == 4 && options.first.is_some_and(|first| d != first) {
                continue;
            }
            if previous != 4 && ((d + 2) % 4 == previous || blocked.contains(&p) && d != previous) {
                continue;
            }
            if q == end && options.last.is_some_and(|last| d != last) {
                continue;
            }
            let next_used = used + usize::from(blocked.contains(&q));
            if next_used > options.crossing_limit
                || blocked.contains(&q) && !options.crossings.contains_key(&(q, d))
            {
                continue;
            }
            let next = (q, d, next_used);
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    false
}

#[test]
fn reverse_negative_preserves_every_forward_reachable_crossing_walk() {
    let start = Point { x: -1, y: -1 };
    let end = Point { x: 2, y: 1 };
    let bounds = Bounds {
        min_x: -1,
        min_y: -1,
        max_x: 2,
        max_y: 1,
    };
    let mut negative = 0;
    let mut positive = 0;
    for pattern in 0_u64..128 {
        let mut random = pattern + 1;
        let mut blocked = IndexSet::new();
        let mut crossings = Crossings::new();
        for y in bounds.min_y..=bounds.max_y {
            for x in bounds.min_x..=bounds.max_x {
                let p = Point { x, y };
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                if p != start && p != end && (random >> 32) % 3 == 0 {
                    blocked.insert(p);
                    for d in 0..4 {
                        if (random >> (40 + d)) & 1 != 0 {
                            crossings.insert(
                                (p, d),
                                Crossing {
                                    position: p,
                                    direction: d,
                                    component: "test-crossing".into(),
                                    rotation: 0,
                                    transport: TransportKind::Belt,
                                    kind: LogisticsComponentKind::Bridge,
                                    carrier_chain: 0,
                                    carrier_network: 0,
                                },
                            );
                        }
                    }
                }
            }
        }
        for first in [None, Some(0), Some(1), Some(2), Some(3)] {
            for last in [None, Some(0), Some(1), Some(2), Some(3)] {
                for crossing_limit in [0, 1, 3] {
                    let options = PathOptions {
                        first,
                        last,
                        work_budget: 10000,
                        scan_budget: usize::MAX,
                        frontier_budget: usize::MAX,
                        deadline: None,
                        crossings: &crossings,
                        crossing_limit,
                        prefer_crossings: false,
                        allow_zero_length: false,
                        cell_penalties: None,
                        edge_penalties: None,
                    };
                    let reachable = forward_reachable(start, end, &blocked, bounds, &options);
                    for cap in [0, 1, 2, 5, 256] {
                        let probe =
                            reachability::probe(start, end, &blocked, bounds, &options, cap);
                        assert!(probe.expanded <= cap);
                        if probe.status == "unreachable" {
                            negative += 1;
                            assert!(
                                !reachable,
                                "unsound rejection: pattern={pattern}, first={first:?}, last={last:?}, limit={crossing_limit}, cap={cap}"
                            );
                        }
                        positive += usize::from(reachable);
                    }
                }
            }
        }
    }
    assert!(negative > 1000 && positive > 1000);
}

#[test]
fn sealed_goal_fails_locally_and_charges_reverse_work() {
    let start = Point { x: 0, y: 0 };
    let end = Point { x: 8, y: 8 };
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 9,
        max_y: 9,
    };
    let blocked = (0..4).map(|d| end.offset(d)).collect();
    let crossings = Crossings::new();
    let result = shortest_path(
        start,
        end,
        &blocked,
        bounds,
        PathOptions {
            first: None,
            last: None,
            work_budget: 1000,
            scan_budget: usize::MAX,
            frontier_budget: usize::MAX,
            deadline: None,
            crossings: &crossings,
            crossing_limit: 8,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "no-path");
    assert_eq!(result.preflight_status, "unreachable");
    assert_eq!(result.expanded, result.preflight_expanded);
    assert!(result.expanded < 10);
    assert_eq!(result.blocked_frontier.len(), 4);
}

#[test]
fn reverse_cutoff_still_runs_astar_and_preserves_directed_route_quality() {
    let start = Point { x: 0, y: 0 };
    let end = Point { x: 8, y: 8 };
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 9,
        max_y: 9,
    };
    let crossings = Crossings::new();
    let result = shortest_path(
        start,
        end,
        &IndexSet::new(),
        bounds,
        PathOptions {
            first: Some(0),
            last: Some(1),
            work_budget: 1200,
            scan_budget: usize::MAX,
            frontier_budget: usize::MAX,
            deadline: None,
            crossings: &crossings,
            crossing_limit: 0,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "found");
    assert_eq!((result.steps, result.turns), (Some(16), Some(1)));
    assert!(result.preflight_expanded > 0 && result.preflight_expanded < result.expanded);
    assert!(result.expanded <= 1200);
}

#[test]
fn reverse_probe_obeys_the_public_frontier_budget() {
    let crossings = Crossings::new();
    let result = shortest_path(
        Point { x: 0, y: 0 },
        Point { x: 9, y: 9 },
        &IndexSet::new(),
        Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 9,
            max_y: 9,
        },
        PathOptions {
            first: None,
            last: None,
            work_budget: 1_000,
            scan_budget: 10_000,
            frontier_budget: 1,
            deadline: None,
            crossings: &crossings,
            crossing_limit: 0,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "frontier-budget");
    assert_eq!(result.preflight_status, "frontier-budget");
    assert!(result.preflight_expanded < 1_000);
}

#[test]
fn forward_no_path_retains_blocked_cells_after_inconclusive_probe() {
    let start = Point { x: 0, y: 0 };
    let end = Point { x: 49, y: 49 };
    let blocked = (0..50).map(|y| Point { x: 2, y }).collect::<IndexSet<_>>();
    let crossings = Crossings::new();
    let result = shortest_path(
        start,
        end,
        &blocked,
        Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 49,
            max_y: 49,
        },
        PathOptions {
            first: None,
            last: None,
            work_budget: 2_000,
            scan_budget: 10_000,
            frontier_budget: 2_000,
            deadline: None,
            crossings: &crossings,
            crossing_limit: 0,
            prefer_crossings: false,
            allow_zero_length: false,
            cell_penalties: None,
            edge_penalties: None,
        },
    )
    .unwrap();
    assert_eq!(result.preflight_status, "unknown");
    assert_eq!(result.status, "no-path");
    assert!(result.blocked_frontier.contains(&Point { x: 2, y: 0 }));
    assert!(result.blocked_frontier.contains(&Point { x: 2, y: 1 }));
}

#[test]
fn cell_penalties_select_a_distinct_equal_length_corridor() {
    let start = Point { x: 0, y: 1 };
    let end = Point { x: 4, y: 1 };
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 4,
        max_y: 2,
    };
    let blocked = IndexSet::from([Point { x: 2, y: 1 }]);
    let crossings = Crossings::new();
    let route = |penalties| {
        shortest_path(
            start,
            end,
            &blocked,
            bounds,
            PathOptions {
                first: None,
                last: None,
                work_budget: 1_000,
                scan_budget: usize::MAX,
                frontier_budget: usize::MAX,
                deadline: None,
                crossings: &crossings,
                crossing_limit: 0,
                prefer_crossings: false,
                allow_zero_length: false,
                cell_penalties: penalties,
                edge_penalties: None,
            },
        )
        .unwrap()
        .cells
        .unwrap()
    };
    let anchor = route(None);
    let penalties = anchor
        .iter()
        .copied()
        .skip(1)
        .take(anchor.len() - 2)
        .map(|point| (point, 10))
        .collect::<IndexMap<_, _>>();
    let diverse = route(Some(&penalties));

    assert_eq!(anchor.len(), diverse.len());
    assert_ne!(anchor, diverse);
    assert_eq!(diverse.first(), Some(&start));
    assert_eq!(diverse.last(), Some(&end));
}

#[test]
fn directed_edge_penalties_change_ordering_without_changing_legality() {
    let start = Point { x: 0, y: 1 };
    let end = Point { x: 4, y: 1 };
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 4,
        max_y: 2,
    };
    let blocked = IndexSet::from([Point { x: 2, y: 1 }]);
    let crossings = Crossings::new();
    let route = |penalties| {
        shortest_path(
            start,
            end,
            &blocked,
            bounds,
            PathOptions {
                first: None,
                last: None,
                work_budget: 1_000,
                scan_budget: usize::MAX,
                frontier_budget: usize::MAX,
                deadline: None,
                crossings: &crossings,
                crossing_limit: 0,
                prefer_crossings: false,
                allow_zero_length: false,
                cell_penalties: None,
                edge_penalties: penalties,
            },
        )
        .unwrap()
        .cells
        .unwrap()
    };
    let anchor = route(None);
    let penalties = anchor
        .windows(2)
        .map(|edge| {
            let direction = match (edge[1].x - edge[0].x, edge[1].y - edge[0].y) {
                (1, 0) => 0,
                (0, 1) => 1,
                (-1, 0) => 2,
                (0, -1) => 3,
                _ => unreachable!(),
            };
            ((edge[1], direction), 10)
        })
        .collect::<IndexMap<_, _>>();
    let guided = route(Some(&penalties));

    assert_eq!(anchor.len(), guided.len());
    assert_ne!(anchor, guided);
    assert!(guided.iter().all(|point| !blocked.contains(point)));
}
