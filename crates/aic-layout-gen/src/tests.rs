use crate::{Config, Generator, io, packing};
use aic_layout::{
    physical::{Layout, Problem, SCHEMA, Solution},
    rate::Rate,
    rng::Rng,
    validation::{validate, within_capacity},
};
use std::path::Path;

fn catalogs() -> io::LoadedCatalogs {
    io::load_catalogs(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized"))
        .unwrap()
}

#[test]
fn sequence_pairs_are_nonoverlapping_for_varied_rectangles_and_rotations() {
    let loaded = catalogs();
    let config = Config::default();
    for seed in 0..80 {
        let mut rng = Rng::new(seed);
        let p =
            packing::sample(&loaded.catalogs, &config, 2 + seed as usize % 30, &mut rng).unwrap();
        let layout = Layout {
            schema: SCHEMA.into(),
            catalog_sha256: loaded.catalogs.identity.clone(),
            problem: Problem {
                blocks: p.blocks,
                connections: vec![],
            },
            solution: Solution {
                poses: p.poses,
                routes: vec![],
                bridges: vec![],
            },
        };
        let m = validate(&layout, &loaded.catalogs).unwrap();
        assert_eq!(m.weak_components, layout.problem.blocks.len());
        assert!(m.body_cells <= m.area);
        assert!(p.width * p.height <= config.max_cells as i64);
    }
}

#[test]
fn complete_generation_is_deterministic_and_satisfies_requested_graph_quality() {
    let loaded = catalogs();
    let generator = Generator::new(&loaded.catalogs, Config::default()).unwrap();
    let a = generator.generate(1000).unwrap();
    let b = generator.generate(1000).unwrap();
    assert_eq!(a.layout, b.layout);
    assert_eq!(a.sequence_pair, b.sequence_pair);
    assert_eq!(a.metrics, b.metrics);
    assert_eq!(a.attempts.len(), 1);
    let boundary = Config {
        max_attempts: 1,
        max_pair_trials: a.attempts[0].pair_trials,
        ..Config::default()
    };
    let last_trial = Generator::new(&loaded.catalogs, boundary)
        .unwrap()
        .generate(1000)
        .unwrap();
    assert_eq!(
        last_trial.layout, a.layout,
        "completion on the last permitted trial must be admitted"
    );
    assert_eq!(
        a.attempts.last().unwrap().route_expansions,
        b.attempts.last().unwrap().route_expansions
    );
    assert_eq!(
        a.layout.problem.connections.len(),
        a.attempts.last().unwrap().target_connections
    );
    assert_eq!(a.metrics.weak_components, 1);
    assert!(a.metrics.branch_blocks > 0 && a.metrics.merge_blocks > 0);
    assert!(a.metrics.belt_connections > 0 && a.metrics.pipe_connections > 0);
    assert!(a.metrics.bridges > 0);
    assert_eq!(validate(&a.layout, &loaded.catalogs).unwrap(), a.metrics);
    let encoded = serde_json::to_vec(&a.layout).unwrap();
    assert_eq!(
        a.layout,
        serde_json::from_slice::<Layout>(&encoded).unwrap()
    );
}

#[test]
fn cancellation_retains_an_explicit_generation_failure() {
    let loaded = catalogs();
    let generator = Generator::new(&loaded.catalogs, Config::default()).unwrap();
    let failure = generator.generate_until(1000, || true).unwrap_err();
    assert_eq!(
        failure.diagnostic.code,
        "aic-layout-gen.generation-interrupted"
    );
    assert!(failure.attempts.is_empty());
}

#[test]
fn independent_validation_rejects_corrupted_obligations_ports_capacity_and_bridges() {
    let loaded = catalogs();
    let good = Generator::new(&loaded.catalogs, Config::default())
        .unwrap()
        .generate(1000)
        .unwrap()
        .layout;
    let expect = |layout: Layout, code: &str| {
        assert_eq!(
            validate(&layout, &loaded.catalogs).unwrap_err().code,
            format!("aic-layout.{code}")
        );
    };
    let mut bad = good.clone();
    bad.solution.routes.pop();
    expect(bad, "counts");
    let mut bad = good.clone();
    bad.problem.connections[1].id = bad.problem.connections[0].id.clone();
    expect(bad, "connection");
    let mut bad = good.clone();
    bad.solution.routes[0].source_port = "missing-port".into();
    expect(bad, "port");
    let mut bad = good.clone();
    bad.solution.poses[1].position = bad.solution.poses[0].position;
    expect(bad, "overlap");
    let mut bad = good.clone();
    bad.problem.connections[0].rate = Rate {
        numerator: i64::MAX,
        denominator: 1,
    };
    expect(bad, "capacity");
    let mut bad = good.clone();
    bad.problem.connections[0].rate.denominator = 0;
    expect(bad, "capacity");
    let mut bad = good.clone();
    bad.solution.bridges.pop();
    expect(bad, "intersection");
    let mut bad = good.clone();
    bad.solution.bridges[0].connections[1] = bad.solution.bridges[0].connections[0].clone();
    expect(bad, "bridge");
    let mut bad = good.clone();
    bad.catalog_sha256 = "0".repeat(64);
    expect(bad, "identity");
    let mut bad = good.clone();
    let repeated = bad.solution.routes[0].cells[0];
    bad.solution.routes[0].cells.insert(1, repeated);
    expect(bad, "path");
    let mut bad = good.clone();
    bad.solution.routes[0].cells[1].x = i64::MAX;
    expect(bad, "path");
    let mut bad = good.clone();
    bad.solution.poses[0].rotation = 45;
    expect(bad, "pose");
    let first = &good.problem.connections[0];
    if let Some(second) = good
        .problem
        .connections
        .iter()
        .position(|e| e.source == first.source && e.id != first.id)
    {
        let mut bad = good.clone();
        bad.solution.routes[second].source_port = good.solution.routes[0].source_port.clone();
        bad.solution.routes[second].cells[0] = good.solution.routes[0].cells[0];
        expect(bad, "port");
    }
    let mut reordered = good.clone();
    reordered.problem.blocks.reverse();
    reordered.problem.connections.reverse();
    reordered.solution.poses.reverse();
    reordered.solution.routes.reverse();
    assert_eq!(
        validate(&reordered, &loaded.catalogs).unwrap().area,
        validate(&good, &loaded.catalogs).unwrap().area
    );
    assert!(crate::render::svg(&reordered, &loaded.catalogs).contains("<svg"));
}

#[test]
fn exact_capacity_comparison_handles_extreme_valid_inputs_without_overflow() {
    assert!(within_capacity(
        Rate {
            numerator: 1,
            denominator: 2
        },
        1,
        2000
    ));
    assert!(!within_capacity(
        Rate {
            numerator: 501,
            denominator: 1000
        },
        1,
        2000
    ));
    assert!(within_capacity(
        Rate {
            numerator: 1,
            denominator: 3
        },
        1,
        2000
    ));
    assert!(within_capacity(
        Rate {
            numerator: i64::MAX,
            denominator: i64::MAX
        },
        i64::MAX,
        i64::MAX
    ));
    assert!(!within_capacity(
        Rate {
            numerator: i64::MAX,
            denominator: 1
        },
        1,
        i64::MAX
    ));
    assert!(!within_capacity(
        Rate {
            numerator: 1,
            denominator: 0
        },
        1,
        2000
    ));
}

#[test]
fn oversized_catalog_geometry_is_rejected_before_port_projection() {
    let mut loaded = catalogs();
    let mut definitions = loaded.catalogs.facilities.catalog().clone();
    let definition = &mut definitions.facilities[0];
    let id = definition.id.clone();
    definition.footprint.width = i64::MAX;
    definition
        .ports
        .retain(|p| p.edge == aic_layout::facilities::FacilityPortEdge::West);
    loaded.catalogs.facilities =
        aic_layout::facilities::ValidatedFacilityCatalog::try_from_catalog(definitions).unwrap();
    let layout = Layout {
        schema: SCHEMA.into(),
        catalog_sha256: loaded.catalogs.identity.clone(),
        problem: Problem {
            blocks: vec![aic_layout::physical::Block {
                id: "block-0".into(),
                facility: id,
            }],
            connections: vec![],
        },
        solution: Solution {
            poses: vec![aic_layout::physical::Pose {
                block: "block-0".into(),
                position: aic_layout::geometry::Point { x: 100, y: 100 },
                rotation: 0,
            }],
            routes: vec![],
            bridges: vec![],
        },
    };
    assert_eq!(
        validate(&layout, &loaded.catalogs).unwrap_err().code,
        "aic-layout.body-guard"
    );
}

#[test]
fn work_exhaustion_never_publishes_a_partial_layout_or_claims_infeasibility() {
    let loaded = catalogs();
    let config = Config {
        max_attempts: 1,
        max_pair_trials: 8,
        route_work_per_call: 1,
        route_work_per_attempt: 1,
        ..Config::default()
    };
    let failed = Generator::new(&loaded.catalogs, config)
        .unwrap()
        .generate(1000)
        .unwrap_err();
    assert_eq!(
        failed.diagnostic.code,
        "aic-layout-gen.generation-exhausted"
    );
    assert_eq!(failed.attempts.len(), 1);
    assert!(failed.attempts[0].route_expansions <= 1);
    assert!(failed.attempts[0].connected < failed.attempts[0].target_connections);
}

#[test]
fn invalid_and_impossible_configuration_is_rejected_before_sampling() {
    let loaded = catalogs();
    let cases = [
        Config {
            blocks: [0, 10],
            ..Config::default()
        },
        Config {
            gap: [0, 5],
            ..Config::default()
        },
        Config {
            max_pair_trials: 0,
            ..Config::default()
        },
        Config {
            long_link_fraction: f64::NAN,
            ..Config::default()
        },
        Config {
            facility_ids: vec!["missing".into()],
            ..Config::default()
        },
        Config {
            blocks: [2, 2],
            acyclic: true,
            edges_per_block: [1.0, 1.0],
            ..Config::default()
        },
    ];
    for c in cases {
        assert!(Generator::new(&loaded.catalogs, c).is_err());
    }
}

#[test]
fn transport_quotas_and_acyclic_order_survive_routing() {
    let loaded = catalogs();
    let config: Config =
        io::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/mixed-dag.json")).unwrap();
    let result = Generator::new(&loaded.catalogs, config)
        .unwrap()
        .generate(8000)
        .unwrap();
    assert!(result.metrics.belt_connections >= 4 && result.metrics.pipe_connections >= 3);
    let problem = &result.layout.problem;
    let mut remaining: std::collections::BTreeSet<_> =
        problem.blocks.iter().map(|b| b.id.as_str()).collect();
    while !remaining.is_empty() {
        let source = remaining
            .iter()
            .copied()
            .find(|id| {
                !problem
                    .connections
                    .iter()
                    .any(|e| e.target == *id && remaining.contains(e.source.as_str()))
            })
            .expect("directed cycle");
        remaining.remove(source);
    }
}
