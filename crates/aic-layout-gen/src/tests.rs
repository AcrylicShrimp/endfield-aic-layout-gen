use crate::{
    Config as PublicConfig, Generator, IntentGenerator, MechanismConfig as Config, PackingProfile,
    Request, TopologyProfile, intent, io, packing,
};
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
fn mechanism_packing_admits_sparse_canvas_above_legacy_limit() {
    let loaded = catalogs();
    let (_, mut config) = intent::resolve(
        &intent::Intent::default(),
        Request {
            facilities: 256,
            topology: TopologyProfile::Tree,
            packing: PackingProfile::Spacious,
            long_connection_preference: 0.25,
        },
        false,
    )
    .unwrap();
    config.gap = [20, 20];
    config.validate(&loaded.catalogs).unwrap();
    let mut rng = Rng::new(17);
    let packed = packing::sample(&loaded.catalogs, &config, 256, &mut rng).unwrap();
    let area = packed.width * packed.height;
    assert!(
        area > 250_000 && area <= config.max_cells as i64,
        "area {area}"
    );
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
                terminals: vec![],
                connections: vec![],
            },
            solution: Solution {
                poses: p.poses,
                terminal_poses: vec![],
                routes: vec![],
                bridges: vec![],
                junctions: vec![],
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
fn cancellation_inside_connect_is_not_retried() {
    let loaded = catalogs();
    let generator = Generator::new(
        &loaded.catalogs,
        Config {
            max_attempts: 2,
            ..Config::default()
        },
    )
    .unwrap();
    let mut checks = 0;
    let failure = generator
        .generate_until(1000, || {
            checks += 1;
            checks == 3
        })
        .unwrap_err();
    assert_eq!(
        failure.diagnostic.code,
        "aic-layout-gen.generation-interrupted"
    );
    assert_eq!(failure.attempts.len(), 1);
    assert_eq!(failure.attempts[0].status, "interrupted");
    assert_eq!(failure.attempts[0].route_calls, 0);
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
    bad.solution.routes[0].source_port = Some("missing-port".into());
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
    expect(bad, "topology");
    let mut bad = good.clone();
    bad.solution.bridges[0].channels[1].connections =
        bad.solution.bridges[0].channels[0].connections.clone();
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
fn bridge_axis_accepts_multiple_same_commodity_route_owners() {
    let loaded = catalogs();
    let mut layout = Generator::new(&loaded.catalogs, Config::default())
        .unwrap()
        .generate(1000)
        .unwrap()
        .layout;
    let owner = layout.solution.bridges[0].channels[0].connections[0].clone();
    let connection = layout
        .problem
        .connections
        .iter_mut()
        .find(|connection| connection.id == owner)
        .unwrap();
    connection.rate.denominator *= 2;
    let mut shared = connection.clone();
    shared.id = "shared-bridge-flow".into();
    let mut shared_route = layout
        .solution
        .routes
        .iter()
        .find(|route| route.connection == owner)
        .unwrap()
        .clone();
    shared_route.connection = shared.id.clone();
    for bridge in &mut layout.solution.bridges {
        for channel in &mut bridge.channels {
            if channel.connections.contains(&owner) {
                channel.connections.push(shared.id.clone());
            }
        }
    }
    layout.problem.connections.push(shared);
    layout.solution.routes.push(shared_route);
    validate(&layout, &loaded.catalogs).unwrap();
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
            terminals: vec![],
            connections: vec![],
        },
        solution: Solution {
            poses: vec![aic_layout::physical::Pose {
                block: "block-0".into(),
                position: aic_layout::geometry::Point { x: 100, y: 100 },
                rotation: 0,
            }],
            terminal_poses: vec![],
            routes: vec![],
            bridges: vec![],
            junctions: vec![],
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
        "aic-layout-gen.generation-search-incomplete"
    );
    assert_eq!(failed.attempts.len(), 1);
    assert!(failed.attempts[0].route_expansions <= 1);
    assert!(failed.attempts[0].connected < failed.attempts[0].target_connections);
}

#[test]
fn pair_and_route_stops_keep_their_work_and_unknown_outcome() {
    let loaded = catalogs();
    let pair_limited = Generator::new(
        &loaded.catalogs,
        Config {
            max_attempts: 1,
            max_pair_trials: 1,
            ..Config::default()
        },
    )
    .unwrap()
    .generate(1000)
    .unwrap_err();
    assert_eq!(
        pair_limited.diagnostic.code,
        "aic-layout-gen.generation-search-incomplete"
    );
    let pair = &pair_limited.attempts[0];
    assert_eq!(pair.status, "pair-trials-exhausted");
    assert_eq!(pair.pair_trials, 1);
    assert_eq!(
        pair.route_calls,
        pair.routing_statuses.values().sum::<usize>()
    );
    assert!(pair.routing_statuses.keys().all(|status| matches!(
        status.as_str(),
        "found" | "no-path" | "blocked-endpoint" | "same-endpoint"
    )));

    let route_limited = Generator::new(
        &loaded.catalogs,
        Config {
            max_attempts: 1,
            max_pair_trials: 64,
            route_work_per_call: 1,
            route_work_per_attempt: 128,
            ..Config::default()
        },
    )
    .unwrap()
    .generate(1000)
    .unwrap_err();
    assert_eq!(
        route_limited.diagnostic.code,
        "aic-layout-gen.generation-search-incomplete"
    );
    let route = &route_limited.attempts[0];
    assert!(route.pair_trials > 1);
    assert!(route.route_calls > 1);
    assert!(
        route
            .routing_statuses
            .get("work-budget")
            .copied()
            .unwrap_or(0)
            > 1
    );
    assert_eq!(
        route.route_calls,
        route.routing_statuses.values().sum::<usize>()
    );
    assert!(route.route_expansions <= 128);
}

#[test]
fn total_work_stop_after_a_complete_route_remains_unknown() {
    let loaded = catalogs();
    // This seed's first complete route uses exactly 1,362 expansions. The
    // attempt has no remaining work for the next pair, without a censored path.
    let failure = Generator::new(
        &loaded.catalogs,
        Config {
            max_attempts: 1,
            max_pair_trials: 64,
            route_work_per_call: 1_362,
            route_work_per_attempt: 1_362,
            ..Config::default()
        },
    )
    .unwrap()
    .generate(1000)
    .unwrap_err();
    assert_eq!(
        failure.diagnostic.code,
        "aic-layout-gen.generation-search-incomplete"
    );
    let attempt = &failure.attempts[0];
    assert_eq!(attempt.status, "route-work-budget");
    assert_eq!(attempt.pair_trials, 1);
    assert_eq!(attempt.route_calls, 1);
    assert_eq!(attempt.route_expansions, 1_362);
    assert_eq!(attempt.routing_statuses.get("found"), Some(&1));
    assert_eq!(attempt.routing_statuses.len(), 1);
}

#[test]
fn canvas_resource_stop_is_not_fully_evaluated_construction() {
    let loaded = catalogs();
    let failure = Generator::new(
        &loaded.catalogs,
        Config {
            max_attempts: 1,
            max_cells: 16,
            ..Config::default()
        },
    )
    .unwrap()
    .generate(1000)
    .unwrap_err();
    assert_eq!(failure.attempts[0].status, "aic-layout-gen.canvas-guard");
    assert_eq!(
        failure.diagnostic.code,
        "aic-layout-gen.generation-search-incomplete"
    );
}

#[test]
fn generator_accepts_zero_length_route_without_changing_identity() {
    let loaded = catalogs();
    let generator = Generator::new(
        &loaded.catalogs,
        Config {
            blocks: [2, 2],
            edges_per_block: [0.5, 0.5],
            gap: [1, 1],
            max_attempts: 1,
            max_pair_trials: 64,
            min_branch_blocks: 0,
            min_merge_blocks: 0,
            ..Config::default()
        },
    )
    .unwrap();
    let generated = generator.generate(9).unwrap();
    assert_eq!(generated.layout.solution.routes.len(), 1);
    assert_eq!(generated.layout.solution.routes[0].cells.len(), 1);
    assert_eq!(
        generated.attempts[0].routing_statuses.get("found"),
        Some(&1)
    );
    assert_eq!(
        validate(&generated.layout, &loaded.catalogs).unwrap(),
        generated.metrics
    );
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
    let config: PublicConfig =
        io::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/mixed-dag.json")).unwrap();
    let result = IntentGenerator::new(&loaded.catalogs, config)
        .unwrap()
        .generate(8000)
        .unwrap()
        .generated;
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
