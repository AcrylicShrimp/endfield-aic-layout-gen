use super::*;
use crate::layouts::collect_constructive_profile;
use crate::recipes::Rate;
use serde::Deserialize;
use std::{
    collections::{BTreeSet, VecDeque},
    time::Instant,
};

#[derive(Deserialize)]
struct Fixtures {
    schema_version: u32,
    rate: FixtureRate,
    absent_positions: Vec<[i64; 2]>,
    cases: Vec<Case>,
    scaling: Scaling,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    edges: Vec<[[i64; 2]; 2]>,
}

#[derive(Deserialize)]
struct FixtureRate {
    numerator: i64,
    denominator: i64,
}

impl Fixtures {
    fn rate(&self) -> Rate {
        Rate {
            numerator: self.rate.numerator,
            denominator: self.rate.denominator,
        }
    }
}

#[derive(Deserialize)]
struct Scaling {
    lengths: Vec<usize>,
    query_repetitions: usize,
    benchmark_rounds: usize,
}

fn fixtures() -> Fixtures {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/examples/constructive-path-queries.json");
    let result: Fixtures = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(result.schema_version, 1);
    result
}

fn position(p: [i64; 2]) -> WorldGridPosition {
    WorldGridPosition { x: p[0], y: p[1] }
}

fn segments(case: &Case, rate: Rate) -> Vec<TransportNetworkSegment> {
    case.edges
        .iter()
        .map(|[from, to]| TransportNetworkSegment {
            from: position(*from),
            to: position(*to),
            rate,
        })
        .collect()
}

/// The former production BFS, with counting only; never used by the constructor.
fn reference(
    segments: &[TransportNetworkSegment],
    start: &WorldGridPosition,
    end: &WorldGridPosition,
) -> (Option<Vec<usize>>, u64, u64) {
    let mut visited = 0;
    let mut examined = 0;
    let mut queue = VecDeque::from([xy(start)]);
    let mut seen = BTreeMap::from([(xy(start), None)]);
    while let Some(p) = queue.pop_front() {
        visited += 1;
        if p == xy(end) {
            break;
        }
        for (i, segment) in segments.iter().enumerate() {
            examined += 1;
            if xy(&segment.from) != p {
                continue;
            }
            if let std::collections::btree_map::Entry::Vacant(e) = seen.entry(xy(&segment.to)) {
                e.insert(Some((p, i)));
                queue.push_back(xy(&segment.to));
            }
        }
    }
    let result = (|| {
        let mut cursor = xy(end);
        let mut path = Vec::new();
        while let Some((previous, i)) = *seen.get(&cursor)? {
            path.push(i);
            cursor = previous;
        }
        path.reverse();
        Some(path)
    })();
    (result, visited, examined)
}

fn compare_all(segments: &[TransportNetworkSegment], absent: &[[i64; 2]], name: &str) {
    let points = segments
        .iter()
        .flat_map(|s| [xy(&s.from), xy(&s.to)])
        .chain(absent.iter().map(|p| (p[0], p[1])))
        .collect::<BTreeSet<_>>();
    let mut paths = DirectedPaths::new(segments);
    // Repeat in reverse order to exercise scratch reuse after both success and failure.
    for reverse in [false, true] {
        let mut points = points.iter().copied().collect::<Vec<_>>();
        if reverse {
            points.reverse();
        }
        for &(x, y) in &points {
            for &(ex, ey) in &points {
                let start = WorldGridPosition { x, y };
                let end = WorldGridPosition { x: ex, y: ey };
                assert_eq!(
                    paths.path(&start, &end),
                    reference(segments, &start, &end).0,
                    "{name}: {start:?} -> {end:?}"
                );
            }
        }
    }
}

fn permutations(
    segments: &mut [TransportNetworkSegment],
    index: usize,
    absent: &[[i64; 2]],
    name: &str,
) {
    if index == segments.len() {
        compare_all(segments, absent, name);
        return;
    }
    for next in index..segments.len() {
        segments.swap(index, next);
        permutations(segments, index + 1, absent, name);
        segments.swap(index, next);
    }
}

#[test]
fn matches_reference_for_all_fixture_pairs_and_edge_permutations() {
    let fixtures = fixtures();
    for case in &fixtures.cases {
        let mut segments = segments(case, fixtures.rate());
        permutations(&mut segments, 0, &fixtures.absent_positions, &case.name);
    }
}

#[test]
fn changed_graph_gets_a_new_index_without_stale_results() {
    let fixtures = fixtures();
    for case in &fixtures.cases {
        let mut segments = segments(case, fixtures.rate());
        compare_all(&segments, &fixtures.absent_positions, &case.name);
        while segments.pop().is_some() {
            compare_all(&segments, &fixtures.absent_positions, &case.name);
        }
    }
}

fn chain(length: usize, rate: Rate) -> Vec<TransportNetworkSegment> {
    (0..length)
        .map(|i| TransportNetworkSegment {
            from: position([i as i64, 0]),
            to: position([i as i64 + 1, 0]),
            rate,
        })
        .collect()
}

#[test]
fn indexed_work_scales_with_reachable_edges_not_vertices_times_all_edges() {
    let fixtures = fixtures();
    for &length in &fixtures.scaling.lengths {
        let segments = chain(length, fixtures.rate());
        let start = position([0, 0]);
        let end = position([length as i64, 0]);
        let (expected, visited, examined) = reference(&segments, &start, &end);
        let (actual, profile) =
            collect_constructive_profile(|| DirectedPaths::new(&segments).path(&start, &end))
                .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(examined, (length * length) as u64);
        let query = &profile.operations[&ProfileOp::SharedPath];
        assert_eq!(query.calls, 1);
        assert_eq!(query.work["vertices-visited"], visited);
        assert_eq!(query.work["edges-examined"], length as u64);
        let build = &profile.operations[&ProfileOp::PathIndexBuild];
        assert_eq!(build.calls, 1);
        assert_eq!(build.work["vertices-indexed"], length as u64 + 1);
        assert_eq!(build.work["segments-indexed"], length as u64);
    }
}

#[test]
#[ignore = "explicit release-mode operation benchmark; no timing assertions in unit tests"]
fn profile_path_scaling() {
    assert!(!cfg!(debug_assertions), "run this benchmark with --release");
    let fixtures = fixtures();
    let count = fixtures.scaling.query_repetitions;
    for &length in &fixtures.scaling.lengths {
        let segments = chain(length, fixtures.rate());
        let start = position([0, 0]);
        let end = position([length as i64, 0]);
        for round in 0..fixtures.scaling.benchmark_rounds {
            for indexed in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let started = Instant::now();
                if indexed {
                    let mut paths = DirectedPaths::new(&segments);
                    for _ in 0..count {
                        std::hint::black_box(paths.path(&start, &end));
                    }
                } else {
                    for _ in 0..count {
                        std::hint::black_box(reference(&segments, &start, &end));
                    }
                }
                println!(
                    "{}",
                    serde_json::json!({"length":length,"queries":count,"round":round,
                    "indexed":indexed,"elapsed_ns":started.elapsed().as_nanos()})
                );
            }
        }
    }
}
