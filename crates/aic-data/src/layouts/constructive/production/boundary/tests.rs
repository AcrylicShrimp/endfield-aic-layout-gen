use super::*;
use std::collections::HashSet;

#[derive(Deserialize)]
struct Fixtures {
    schema_version: u32,
    cases: Vec<Case>,
    scaling_sides: Vec<i64>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    max: [i64; 2],
    blocked: Vec<[i64; 2]>,
    crossings: Vec<(i64, i64, bool)>,
}
fn fixtures() -> Fixtures {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/examples/constructive-boundary-screening.json");
    let f: Fixtures = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(f.schema_version, 1);
    f
}
fn reference_grid(case: &Case) -> HashSet<(i64, i64)> {
    let blocked = case
        .blocked
        .iter()
        .map(|p| (p[0], p[1]))
        .collect::<HashSet<_>>();
    let crossing = case
        .crossings
        .iter()
        .map(|&(x, y, h)| ((x, y), h))
        .collect::<BTreeMap<_, _>>();
    let mut seen = HashSet::from([(-1, -1)]);
    let mut queue = VecDeque::from([(-1, -1)]);
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let mut p = (x + dx, y + dy);
            while crossing.get(&p).is_some_and(|h| *h == (dy == 0)) {
                p = (p.0 + dx, p.1 + dy);
            }
            if p.0 >= -1
                && p.1 >= -1
                && p.0 <= case.max[0]
                && p.1 <= case.max[1]
                && !blocked.contains(&p)
                && seen.insert(p)
            {
                queue.push_back(p);
            }
        }
    }
    seen
}
fn compare(case: &Case) {
    let mut grid = Workspace::new(case.max[0], case.max[1]).unwrap();
    for p in &case.blocked {
        grid.mark(p[0], p[1], BELT);
    }
    // Different commodity/layer screens must not retain visited or crossing flags.
    for crossings_enabled in [true, false, true] {
        grid.reset();
        if crossings_enabled {
            for &(x, y, h) in &case.crossings {
                if let Some(i) = grid.index(x, y) {
                    grid.cells[i] |= if h { HORIZONTAL } else { VERTICAL };
                }
            }
        }
        let work = grid.flood(BELT);
        let without_crossings = Case {
            name: case.name.clone(),
            max: case.max,
            blocked: case.blocked.clone(),
            crossings: vec![],
        };
        let expected = reference_grid(if crossings_enabled {
            case
        } else {
            &without_crossings
        });
        assert_eq!(work.visited as usize, expected.len(), "{}", case.name);
        assert_eq!(work.probes, work.visited * 4);
        for y in -3..=case.max[1].max(0) + 2 {
            for x in -3..=case.max[0].max(0) + 2 {
                assert_eq!(
                    grid.seen(x, y),
                    expected.contains(&(x, y)),
                    "{}: ({x},{y})",
                    case.name
                );
            }
        }
        assert!(grid.bytes() <= WORKSPACE_BYTES_LIMIT);
    }
    grid.reset();
    let pipe_work = grid.flood(PIPE);
    assert_eq!(pipe_work.visited as usize, grid.cells.len());
}

#[test]
fn grid_matches_reference_for_crossings_enclosures_and_repeated_screens() {
    for case in &fixtures().cases {
        compare(case);
    }
}

#[test]
fn scaling_preserves_reachable_cells_with_bounded_linear_storage() {
    for side in fixtures().scaling_sides {
        compare(&Case {
            name: format!("scaling-{side}"),
            max: [side, side],
            blocked: vec![],
            crossings: vec![],
        });
        let grid = Workspace::new(side, side).unwrap();
        assert_eq!(
            grid.bytes(),
            (side as usize + 2).pow(2) * (2 + size_of::<usize>())
        );
    }
}

#[test]
fn checked_dimensions_reject_resources_and_coordinates_without_false_blocking() {
    assert!(matches!(
        Workspace::new(i64::MAX, i64::MAX),
        Err(ScreenError::Resource)
    ));
    assert!(matches!(
        Workspace::new(10_000, 10_000),
        Err(ScreenError::Resource)
    ));
    let grid = Workspace::new(2, 2).unwrap();
    assert_eq!(grid.index(i64::MAX, 0), None);
    assert_eq!(grid.index(i64::MIN, 0), None);
    assert_eq!(grid.index(0, i64::MIN), None);
    assert_eq!(grid.index(-1, -1), Some(0));
    assert_eq!(grid.index(3, 0), None);
    assert_ne!(
        ScreenError::Coordinates.code(),
        ScreenError::Resource.code()
    );
    assert_ne!(ScreenError::Allocation.code(), ScreenError::Resource.code());
}
