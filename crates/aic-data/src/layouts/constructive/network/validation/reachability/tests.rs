use super::*;
use serde::Deserialize;
use std::collections::VecDeque;

#[derive(Deserialize)]
struct Fixtures {
    schema_version: u32,
    rate: FixtureRate,
    cases: Vec<Case>,
    scaling_lengths: Vec<usize>,
}
#[derive(Deserialize)]
struct FixtureRate {
    numerator: i64,
    denominator: i64,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    edges: Vec<[[i64; 2]; 2]>,
    cells: Vec<[i64; 2]>,
    supplies: Vec<[i64; 2]>,
    demands: Vec<[i64; 2]>,
}
fn fixtures() -> Fixtures {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/examples/constructive-validation-queries.json");
    let f: Fixtures = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(f.schema_version, 1);
    f
}
fn reference(
    cells: &BTreeSet<(i64, i64)>,
    segments: &[TransportNetworkSegment],
    roots: &[(i64, i64)],
    reverse: bool,
) -> (bool, u64, u64) {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    for &root in roots {
        seen.insert(root);
        queue.push_back(root);
    }
    let mut visited = 0;
    while let Some(p) = queue.pop_front() {
        visited += 1;
        for s in segments {
            let (a, b) = if reverse {
                (&s.to, &s.from)
            } else {
                (&s.from, &s.to)
            };
            if xy(a) == p && seen.insert(xy(b)) {
                queue.push_back(xy(b));
            }
        }
    }
    (seen == *cells, visited, visited * segments.len() as u64)
}
fn point(p: [i64; 2]) -> WorldGridPosition {
    WorldGridPosition { x: p[0], y: p[1] }
}

#[test]
fn coverage_matches_directed_reference_and_resets_after_success_failure_and_rebuild() {
    let f = fixtures();
    for case in f.cases {
        let cells = case
            .cells
            .iter()
            .copied()
            .chain(case.edges.iter().flat_map(|e| e.iter().copied()))
            .map(|p| (p[0], p[1]))
            .collect::<BTreeSet<_>>();
        let mut segments = case
            .edges
            .iter()
            .map(|e| TransportNetworkSegment {
                from: point(e[0]),
                to: point(e[1]),
                rate: Rate {
                    numerator: f.rate.numerator,
                    denominator: f.rate.denominator,
                },
            })
            .collect::<Vec<_>>();
        let mut roots = vec![
            vec![],
            case.supplies.iter().map(|p| (p[0], p[1])).collect(),
            case.demands.iter().map(|p| (p[0], p[1])).collect(),
        ];
        roots.extend(cells.iter().map(|&p| vec![p]));
        roots.push(cells.iter().copied().collect());
        loop {
            let mut index = Reachability::new(&cells, &segments, &BTreeSet::new());
            for reverse in [false, true, true, false] {
                for roots in &roots {
                    assert_eq!(
                        index.covers(roots.iter().copied(), reverse).0,
                        reference(&cells, &segments, roots, reverse).0,
                        "{}",
                        case.name
                    );
                }
            }
            // Rebuild after graph replacement, including retained isolated cells.
            if segments.pop().is_none() {
                break;
            }
            segments.reverse();
        }
    }
}

#[test]
fn independent_index_work_is_linear_in_edges_instead_of_full_segment_scans() {
    let f = fixtures();
    for length in f.scaling_lengths {
        let cells = (0..=length).map(|x| (x as i64, 0)).collect::<BTreeSet<_>>();
        let segments = (0..length)
            .map(|x| TransportNetworkSegment {
                from: point([x as i64, 0]),
                to: point([x as i64 + 1, 0]),
                rate: Rate {
                    numerator: f.rate.numerator,
                    denominator: f.rate.denominator,
                },
            })
            .collect::<Vec<_>>();
        let (_, profile) = crate::layouts::collect_constructive_profile(|| {
            let mut index = Reachability::new(&cells, &segments, &BTreeSet::new());
            for reverse in [false, true] {
                let roots = vec![(if reverse { length as i64 } else { 0 }, 0)];
                let old = reference(&cells, &segments, &roots, reverse);
                let new = index.covers(roots.into_iter(), reverse);
                assert!(new.0);
                assert_eq!(new.0, old.0);
                assert_eq!(new.1, old.1);
                assert_eq!(new.2, length as u64);
                assert_eq!(old.2, (length * (length + 1)) as u64);
            }
        })
        .unwrap();
        let build = &profile.operations[&ProfileOp::ValidationIndexBuild];
        assert_eq!(build.calls, 1);
        assert_eq!(build.work["segments-indexed"], length as u64);
        assert_eq!(build.work["vertices-indexed"], length as u64 + 1);
    }
}

#[test]
fn bridge_channels_do_not_connect_balanced_circulation_to_a_supply() {
    let f = fixtures();
    let rate = Rate {
        numerator: f.rate.numerator,
        denominator: f.rate.denominator,
    };
    // A supply/demand line crosses both sides of an otherwise disconnected loop.
    let horizontal = (0..=6).map(|x| [x, 2]).collect::<Vec<_>>();
    let circulation = [
        [2, 1],
        [2, 2],
        [2, 3],
        [3, 3],
        [4, 3],
        [4, 2],
        [4, 1],
        [3, 1],
        [2, 1],
    ];
    let segments = horizontal
        .windows(2)
        .chain(circulation.windows(2))
        .map(|p| TransportNetworkSegment {
            from: point(p[0]),
            to: point(p[1]),
            rate,
        })
        .collect::<Vec<_>>();
    let cells = segments
        .iter()
        .flat_map(|s| [xy(&s.from), xy(&s.to)])
        .collect();
    let bridges = BTreeSet::from([(2, 2), (4, 2)]);
    let mut index = Reachability::new(&cells, &segments, &bridges);
    for (root, reverse) in [((0, 2), false), ((6, 2), true)] {
        assert!(
            reference(&cells, &segments, &[root], reverse).0,
            "The former coordinate graph incorrectly joins the loop at a bridge"
        );
        assert!(!index.covers([root].into_iter(), reverse).0);
        assert!(
            index.covers([root, (2, 1)].into_iter(), reverse).0,
            "Every real channel is covered when the loop has its own root"
        );
    }
}
