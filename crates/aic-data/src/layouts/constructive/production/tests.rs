use super::*;
use crate::logistics::{ItemCatalog, ItemDefinition};
use crate::recipes::{FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringProjection};

fn catalogs() -> (
    ValidatedFacilityCatalog,
    ValidatedItemCatalog,
    ValidatedTransportCatalog,
    ValidatedLogisticsComponentCatalog,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(
            root.join("data/examples/constructive-network.facilities.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let i = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
        schema_version: 1,
        items: vec![ItemDefinition {
            id: "ore".into(),
            transport: TransportKind::Belt,
        }],
    })
    .unwrap();
    let t = ValidatedTransportCatalog::try_from_catalog(
        crate::logistics::load_transport_catalog(root.join("data/game/normalized/transports.json"))
            .unwrap(),
    )
    .unwrap();
    (f, i, t, super::super::test_components())
}
fn wiring(edges: &[(&str, &str)]) -> FacilityInstanceWiringReport {
    let ids = edges
        .iter()
        .flat_map(|(a, b)| [*a, *b])
        .collect::<BTreeSet<_>>();
    let rate = Rate {
        numerator: 1,
        denominator: 5,
    };
    FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: ids
            .into_iter()
            .map(|id| FacilityInstanceWiringNode::Facility {
                id: id.into(),
                recipe: format!("recipe-{id}"),
                facility: "network-fixture-machine".into(),
                index: 0,
                runs_per_second: rate,
                work_seconds_per_second: rate,
                unused_capacity: Rate::zero(),
            })
            .collect(),
        edges: edges
            .iter()
            .enumerate()
            .map(|(i, (a, b))| FacilityInstanceWiringEdge {
                id: format!("edge-{i}"),
                source: (*a).into(),
                target: (*b).into(),
                kind: "recipe-flow".into(),
                item: "ore".into(),
                rate,
                projection: FacilityInstanceWiringProjection::Original,
            })
            .collect(),
        diagnostics: Vec::new(),
    }
}
fn request(seed: &str) -> ConstructiveProductionRequest {
    ConstructiveProductionRequest {
        schema_version: 1,
        target_instance: seed.into(),
        max_steps: 64,
        max_placement_attempts: 128,
        max_route_searches: 512,
        analysis_workers: 1,
        nearby_candidates: 0,
        downstream_closed_growth: false,
        flow_direction: None,
        connection_first_placement: false,
    }
}

#[test]
fn direct_placement_uses_all_neighbors_but_never_unrelated_anchors() {
    let (f, i, _, _) = catalogs();
    let w = wiring(&[("a", "b"), ("a", "c"), ("unrelated", "elsewhere")]);
    let a = construct_facility_node(&w, &f, &i, "a").unwrap();
    let b = translate_node(&construct_facility_node(&w, &f, &i, "b").unwrap(), 30, 20);
    let c = translate_node(&construct_facility_node(&w, &f, &i, "c").unwrap(), 40, 28);
    let base = merge_nodes(&b, &c);
    let (d, poses) = direct::proposals(&base, &a, &w, FacilityPortEdge::East, 0);
    assert_eq!(d.consumers, ["b", "c"]);
    assert!(d.suppliers.is_empty());
    assert_eq!(d.requirements.len(), 2);
    assert_eq!(poses.len(), 32);
    assert!(poses.windows(2).all(|p| p[0].1 <= p[1].1));
    assert!(poses.iter().all(|(p, _)| p[0] + a.bounds.width < 30));
    let extra = translate_node(
        &construct_facility_node(&w, &f, &i, "unrelated").unwrap(),
        500,
        500,
    );
    let (d2, poses2) = direct::proposals(
        &merge_nodes(&base, &extra),
        &a,
        &w,
        FacilityPortEdge::East,
        0,
    );
    assert_eq!(d, d2);
    assert_eq!(poses, poses2);

    // A supplier-only frontier grows on the opposite side, even though the graph is identical.
    let (d, poses) = direct::proposals(&a, &b, &w, FacilityPortEdge::East, 0);
    assert_eq!(d.suppliers, ["a"]);
    assert_eq!(d.placement_side, "downstream");
    assert!(
        poses
            .iter()
            .all(|(p, _)| b.placements[0].x + p[0] > a.bounds.width)
    );
}

#[test]
fn direct_placement_all_axes_and_both_chain_orders_are_first_valid() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    for (direction, dx, dy) in [
        (FacilityPortEdge::East, 1, 0),
        (FacilityPortEdge::South, 0, 1),
        (FacilityPortEdge::West, -1, 0),
        (FacilityPortEdge::North, 0, -1),
    ] {
        for seed in ["a", "c"] {
            let mut req = request(seed);
            req.flow_direction = Some(direction);
            req.connection_first_placement = true;
            let r = construct_production(&w, &f, &i, &t, &c, &req);
            assert!(r.complete, "{direction:?}/{seed}: {:?}", r.diagnostics);
            let node = r.final_node.as_ref().unwrap();
            for edge in &w.edges {
                let a = node
                    .placements
                    .iter()
                    .find(|p| p.instance == edge.source)
                    .unwrap();
                let b = node
                    .placements
                    .iter()
                    .find(|p| p.instance == edge.target)
                    .unwrap();
                assert!(dx * (b.x - a.x) + dy * (b.y - a.y) > 0);
            }
            assert!(r.placement_decisions.is_empty());
            for d in &r.direct_placement_decisions {
                assert_eq!(d.termination, "first-valid");
                assert!(d.trials.last().unwrap().accepted);
                assert!(d.trials[..d.trials.len() - 1].iter().all(|t| !t.accepted));
            }
            for step in &r.steps {
                network::validate(&step.node, &f, &t, &c).unwrap();
                audit::validate_coverage(&step.node, &w, &i).unwrap();
            }
            let again = construct_production(&w, &f, &i, &t, &c, &req);
            assert_eq!(r.final_node, again.final_node);
            assert_eq!(
                r.direct_placement_decisions,
                again.direct_placement_decisions
            );
        }
    }
}

#[test]
fn direct_placement_cycles_and_shared_consumers_keep_transaction_invariants() {
    let (f, i, t, c) = catalogs();
    for edges in [
        vec![("a", "b"), ("b", "c"), ("c", "a")],
        vec![("a", "b"), ("a", "c")],
    ] {
        let w = wiring(&edges);
        let mut req = request("c");
        req.flow_direction = Some(FacilityPortEdge::East);
        req.connection_first_placement = true;
        req.downstream_closed_growth = true;
        let r = construct_production(&w, &f, &i, &t, &c, &req);
        assert!(r.complete, "{edges:?}: {:?}", r.diagnostics);
        for step in &r.steps {
            network::validate(&step.node, &f, &t, &c).unwrap();
            audit::validate_coverage(&step.node, &w, &i).unwrap();
            for e in &w.edges {
                assert!(
                    !step.node.member_instances.contains(&e.source)
                        || step.node.internal_requirements.contains(&e.id)
                );
            }
        }
        if edges.len() == 3 {
            assert!(
                r.direct_placement_decisions
                    .iter()
                    .any(|d| d.mixed_neighbor_roles)
            );
        }
    }
}

#[test]
fn direct_placement_options_and_exhaustion_do_not_fall_back() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("a", "c")]);
    let mut req = request("c");
    let mut json = serde_json::to_value(&req).unwrap();
    json.as_object_mut()
        .unwrap()
        .remove("connection_first_placement");
    assert!(
        !serde_json::from_value::<ConstructiveProductionRequest>(json)
            .unwrap()
            .connection_first_placement
    );
    req.connection_first_placement = true;
    for (direction, nearby) in [(None, 0), (Some(FacilityPortEdge::East), 4)] {
        req.flow_direction = direction;
        req.nearby_candidates = nearby;
        let r = construct_production(&w, &f, &i, &t, &c, &req);
        assert_eq!(
            r.diagnostics[0].code,
            "production-direct-placement-options-invalid"
        );
        assert!(r.final_node.is_none());
    }
    req.nearby_candidates = 0;
    req.downstream_closed_growth = true;
    req.max_route_searches = 1;
    req.max_placement_attempts = 1;
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert!(!r.complete);
    assert!(r.placement_decisions.is_empty());
    assert!(r.failed_placement_attempts <= 1);
    assert!(r.failed_statistics.astar_searches <= 1);
    assert_eq!(r.final_node.as_ref().unwrap().member_instances, ["c"]);
    audit::validate_coverage(r.final_node.as_ref().unwrap(), &w, &i).unwrap();
}

#[test]
fn downstream_groups_include_shared_consumers_and_cycles_but_stop_at_closed_nodes() {
    let w = wiring(&[("a", "b"), ("a", "c"), ("c", "d"), ("d", "c")]);
    let edges = w.edges.iter().collect::<Vec<_>>();
    assert_eq!(
        order::downstream_group("a", &["b".into()], &edges),
        BTreeSet::from(["a".into(), "c".into(), "d".into()])
    );
    assert_eq!(
        order::downstream_group("c", &[], &edges),
        BTreeSet::from(["c".into(), "d".into()])
    );
    assert_eq!(
        order::downstream_group("a", &["b".into(), "c".into(), "d".into()], &edges),
        BTreeSet::from(["a".into()])
    );
}

#[test]
fn fixed_flow_rotation_uses_required_port_normals_and_reports_side_groups() {
    let (f, i, _, _) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "a")]);
    let mut node = construct_facility_node(&w, &f, &i, "a").unwrap();
    for (direction, expected) in [
        (FacilityPortEdge::East, 0),
        (FacilityPortEdge::South, 90),
        (FacilityPortEdge::West, 180),
        (FacilityPortEdge::North, 270),
    ] {
        let d = orientation::choose(&node, &f, direction).unwrap();
        assert_eq!(d.rotation, expected);
        assert_eq!((d.aligned_input_groups, d.aligned_output_groups), (1, 1));
    }
    let mut side = node
        .boundary_requirements
        .iter()
        .find(|b| b.direction == FacilityPortDirection::Output)
        .unwrap()
        .clone();
    side.item = "side-output".into();
    side.transport = TransportKind::Pipe;
    for a in &mut side.attachments {
        a.port_mut().edge = FacilityPortEdge::North;
    }
    node.boundary_requirements.push(side);
    let d = orientation::choose(&node, &f, FacilityPortEdge::East).unwrap();
    assert_eq!(d.rotation, 0); // Output tie is broken by upstream-facing inputs.
    assert_eq!((d.aligned_output_groups, d.output_groups), (1, 2));
}

#[test]
fn fixed_flow_growth_never_retries_a_different_orientation() {
    let (f, i, t, c) = catalogs();
    for edges in [
        vec![("a", "b"), ("b", "c")],
        vec![("a", "b"), ("a", "c")],
        vec![("a", "b"), ("b", "c"), ("c", "a")],
    ] {
        let w = wiring(&edges);
        for nearby in [0, 4] {
            let mut req = request("c");
            req.downstream_closed_growth = true;
            req.flow_direction = Some(FacilityPortEdge::South);
            req.nearby_candidates = nearby;
            let r = construct_production(&w, &f, &i, &t, &c, &req);
            assert!(r.complete, "{edges:?}/{nearby}: {:?}", r.diagnostics);
            assert_eq!(r.orientation_decisions.len(), w.nodes.len());
            for step in &r.steps {
                assert!(step.node.placements.iter().all(|p| p.rotation == 90));
                network::validate(&step.node, &f, &t, &c).unwrap();
                audit::validate_coverage(&step.node, &w, &i).unwrap();
                assert!(step.placement_attempts <= req.max_placement_attempts);
                assert!(step.statistics.astar_searches <= req.max_route_searches);
            }
            assert!(
                r.placement_decisions
                    .iter()
                    .filter_map(|d| d.selected_pose)
                    .all(|p| p[0] == 0)
            );
            let repeated = construct_production(&w, &f, &i, &t, &c, &req);
            assert_eq!(r.final_node, repeated.final_node);
            assert_eq!(r.orientation_decisions, repeated.orientation_decisions);
        }
    }
}

#[test]
fn fixed_flow_handles_nonzero_catalog_default_and_nonsquare_footprint() {
    let (f, i, t, c) = catalogs();
    let mut raw = f.catalog().clone();
    raw.facilities[0].footprint.width = 3;
    raw.facilities[0].footprint.height = 2;
    raw.facilities[0].allowed_rotations = vec![180, 90, 270];
    raw.facilities[0].ports[1].position.x = 2;
    let f = ValidatedFacilityCatalog::try_from_catalog(raw).unwrap();
    let w = wiring(&[("a", "b")]);
    let mut req = request("b");
    req.flow_direction = Some(FacilityPortEdge::North);
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert!(r.complete, "{:?}", r.diagnostics);
    assert!(
        r.final_node
            .unwrap()
            .placements
            .iter()
            .all(|p| p.rotation == 270 && p.width == 2 && p.height == 3)
    );
}

#[test]
fn fixed_flow_is_optional_and_budget_failure_retains_fixed_seed() {
    let mut value = serde_json::to_value(request("b")).unwrap();
    value.as_object_mut().unwrap().remove("flow_direction");
    assert!(
        serde_json::from_value::<ConstructiveProductionRequest>(value)
            .unwrap()
            .flow_direction
            .is_none()
    );
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("a", "c")]);
    let mut req = request("c");
    req.flow_direction = Some(FacilityPortEdge::West);
    req.downstream_closed_growth = true;
    req.max_route_searches = 1;
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert!(!r.complete);
    assert!(r.steps.is_empty());
    assert_eq!(r.final_node.unwrap().placements[0].rotation, 180);
    assert!(r.failed_statistics.astar_searches <= 1);
}

#[test]
fn downstream_growth_closes_every_internal_output_before_commit() {
    let (f, i, t, c) = catalogs();
    for (edges, seed) in [
        (vec![("a", "b"), ("b", "c")], "c"),
        (vec![("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")], "d"),
        (vec![("a", "b"), ("a", "c")], "c"),
        (vec![("a", "b"), ("b", "c"), ("c", "a")], "c"),
    ] {
        let w = wiring(&edges);
        let mut req = request(seed);
        req.downstream_closed_growth = true;
        req.max_route_searches = 4096;
        for nearby in [0, 4] {
            req.nearby_candidates = nearby;
            let r = construct_production(&w, &f, &i, &t, &c, &req);
            assert!(
                r.complete,
                "{edges:?}, nearby={nearby}: {:?} {:?}",
                r.diagnostics, r.rejected_reasons
            );
            for step in &r.steps {
                network::validate(&step.node, &f, &t, &c).unwrap();
                audit::validate_coverage(&step.node, &w, &i).unwrap();
                // Assert directly against the original ledger, not the group-construction helper.
                for e in &w.edges {
                    if step.node.member_instances.contains(&e.source) {
                        assert!(step.node.member_instances.contains(&e.target));
                        assert!(step.node.internal_requirements.contains(&e.id));
                    }
                }
                assert!(step.placement_attempts <= req.max_placement_attempts);
                assert!(step.statistics.astar_searches <= req.max_route_searches);
            }
            let second = construct_production(&w, &f, &i, &t, &c, &req);
            assert_eq!(r.final_node, second.final_node);
            assert_eq!(r.growth_decisions, second.growth_decisions);
            if edges.len() == 2 && edges[0].0 == edges[1].0 {
                assert_eq!(r.steps[0].added_instances.len(), 2);
                assert_eq!(r.steps[0].node.member_instances.len(), 3);
            }
        }
    }
}

#[test]
fn downstream_group_failure_never_commits_half_a_branch_or_private_seed_cycle() {
    let (f, i, t, c) = catalogs();
    for (edges, seed_is_closed) in [
        (vec![("a", "b"), ("a", "c")], true),
        (vec![("a", "b"), ("b", "c"), ("c", "a")], false),
    ] {
        let w = wiring(&edges);
        let mut req = request("c");
        req.downstream_closed_growth = true;
        req.max_route_searches = 1;
        let r = construct_production(&w, &f, &i, &t, &c, &req);
        assert!(!r.complete);
        assert!(r.steps.is_empty());
        assert_eq!(r.final_node.is_some(), seed_is_closed);
        if let Some(node) = r.final_node {
            assert_eq!(node.member_instances, vec!["c"]);
        }
        assert!(r.failed_statistics.astar_searches <= 1);
        assert!(!r.attempted_groups_at_failure.is_empty());
    }
}

#[test]
fn downstream_closure_audit_detects_unrouted_edges_between_placed_nodes() {
    let (f, i, _, _) = catalogs();
    let w = wiring(&[("a", "b")]);
    let mut node = construct_facility_node(&w, &f, &i, "a").unwrap();
    node.member_instances.push("b".into());
    let edges = w.edges.iter().collect::<Vec<_>>();
    assert_eq!(order::unclosed_outputs(&node, &edges), vec!["edge-0"]);
    node.internal_requirements.push("edge-0".into());
    assert!(order::unclosed_outputs(&node, &edges).is_empty());
}

#[test]
fn downstream_order_is_opt_in_for_existing_request_files() {
    let mut value = serde_json::to_value(request("c")).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("downstream_closed_growth");
    let req: ConstructiveProductionRequest = serde_json::from_value(value).unwrap();
    assert!(!req.downstream_closed_growth);
}

#[test]
fn nearby_selection_validates_growth_and_is_deterministic() {
    let (f, i, t, c) = catalogs();
    for edges in [
        vec![("a", "b"), ("b", "c")],
        vec![("a", "b"), ("a", "c")],
        vec![("a", "b"), ("b", "c"), ("c", "a")],
    ] {
        let w = wiring(&edges);
        let mut req = request("c");
        req.nearby_candidates = 4;
        req.max_route_searches = 4096;
        let first = construct_production(&w, &f, &i, &t, &c, &req);
        let second = construct_production(&w, &f, &i, &t, &c, &req);
        assert!(
            first.complete,
            "{:?} {:?}",
            first.diagnostics, first.rejected_reasons
        );
        assert_eq!(first.final_node, second.final_node);
        assert_eq!(first.placement_decisions, second.placement_decisions);
        for step in &first.steps {
            network::validate(&step.node, &f, &t, &c).unwrap();
            audit::validate_coverage(&step.node, &w, &i).unwrap();
            assert!(placement::future_corridors_clear(&step.node, &w));
            assert!(step.placement_attempts <= req.max_placement_attempts);
            assert!(step.statistics.astar_searches <= req.max_route_searches);
        }
        assert!(!first.placement_decisions.is_empty());
        for d in &first.placement_decisions {
            assert!(d.valid <= 4 && d.attempted <= 32 && d.attempted <= d.shortlisted);
            assert!(d.selected_score <= d.first_score);
        }
    }
}

#[test]
fn nearby_limits_are_explicit_and_preserve_incumbents() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c"), ("c", "a")]);
    let mut req = request("c");
    req.nearby_candidates = 9;
    let invalid = construct_production(&w, &f, &i, &t, &c, &req);
    assert_eq!(invalid.status, "invalid-input");
    assert_eq!(invalid.diagnostics[0].code, "production-input-invalid");
    req.nearby_candidates = 4;
    req.max_route_searches = 1;
    let limited = construct_production(&w, &f, &i, &t, &c, &req);
    assert!(!limited.complete);
    assert!(limited.steps.is_empty());
    assert_eq!(
        limited.final_node.as_ref().unwrap().member_instances,
        vec!["c"]
    );
    assert!(limited.failed_statistics.astar_searches <= 1);
    network::validate(limited.final_node.as_ref().unwrap(), &f, &t, &c).unwrap();
}

#[test]
fn pending_port_corridors_block_footprints_but_not_induced_connections() {
    let (f, i, _, _) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    let mut node = construct_facility_node(&w, &f, &i, "c").unwrap();
    node = translate_node(&node, 10, 10);
    let p = node.boundary_requirements[0].attachments[0].port().clone();
    let mut blocker = construct_facility_node(&w, &f, &i, "a")
        .unwrap()
        .placements
        .remove(0);
    blocker.x = p.connection.x + p.connection.x - p.position.x;
    blocker.y = p.connection.y + p.connection.y - p.position.y;
    // Keep the actual connection cell free while obstructing its reserved outward continuation.
    blocker.width = 1;
    blocker.height = 1;
    node.placements.push(blocker);
    node.member_instances.push("a".into());
    assert!(!placement::future_corridors_clear(&node, &w));
    // The corridor belongs to the edge that will close now, not a future-edge reservation.
    node.member_instances.push("b".into());
    assert!(placement::future_corridors_clear(&node, &w));
}

#[test]
fn profiling_preserves_nodes_choices_and_counts_actual_routes() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    let request = request("c");
    let baseline = construct_production(&w, &f, &i, &t, &c, &request);
    let (captured, profile) =
        collect_constructive_profile(|| construct_production(&w, &f, &i, &t, &c, &request))
            .unwrap();
    assert!(captured.complete);
    assert_eq!(baseline.final_node, captured.final_node);
    assert_eq!(baseline.steps.len(), captured.steps.len());
    for (before, after) in baseline.steps.iter().zip(&captured.steps) {
        assert_eq!(before.node, after.node);
        assert_eq!(before.statistics, after.statistics);
        assert_eq!(before.placement_attempts, after.placement_attempts);
    }
    assert_eq!(profile.operations[&ProfileOp::Construction].calls, 1);
    let screen = &profile.operations[&ProfileOp::BoundaryEscape];
    assert!(screen.work["peak-workspace-bytes"] > 0);
    assert!(screen.work["peak-workspace-bytes"] <= screen.work["workspace-bytes"]);
    assert_eq!(
        screen.work["neighbor-probes"],
        4 * screen.work["cells-visited"]
    );
    assert_eq!(
        screen.work["commodity-screens"],
        profile.operations[&ProfileOp::BoundaryFlood].calls
    );
    assert_eq!(
        profile.operations[&ProfileOp::Astar].calls,
        captured
            .steps
            .iter()
            .map(|s| s.statistics.astar_searches)
            .sum::<u64>()
    );
    assert_eq!(
        profile
            .operations
            .values()
            .map(|r| r.exclusive_ns)
            .sum::<u128>(),
        profile.operations[&ProfileOp::Construction].inclusive_ns
    );
}

#[test]
fn bounded_parallel_analysis_preserves_witnesses_budgets_and_worker_work() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    let serial = construct_production(&w, &f, &i, &t, &c, &request("c"));
    for workers in [1, 2, 4, 8] {
        let mut req = request("c");
        req.analysis_workers = workers;
        let parallel = construct_production(&w, &f, &i, &t, &c, &req);
        assert_eq!(parallel.final_node, serial.final_node);
        assert_eq!(parallel.diagnostics, serial.diagnostics);
        for (a, b) in parallel.steps.iter().zip(&serial.steps) {
            assert_eq!(a.statistics, b.statistics);
            assert_eq!(a.node, b.node);
        }
    }
    // Distinct commodities force multiple independent jobs without changing the query geometry.
    let mut node = serial.steps[0].node.clone();
    let first = node.boundary_requirements[0].clone();
    for suffix in ["-second", "-third", "-fourth"] {
        let mut b = first.clone();
        b.item.push_str(suffix);
        b.requirement.push_str(suffix);
        node.boundary_requirements.push(b);
    }
    let (expected, one) =
        collect_constructive_profile(|| blocked_boundaries(&node, &t, &c, 1)).unwrap();
    for workers in [2, 4, 8] {
        for _ in 0..3 {
            let (result, profile) =
                collect_constructive_profile(|| blocked_boundaries(&node, &t, &c, workers))
                    .unwrap();
            assert_eq!(result, expected);
            let total = &profile.operations[&ProfileOp::BoundaryEscape];
            for key in [
                "commodity-screens",
                "cells-visited",
                "neighbor-probes",
                "crossing-hops",
            ] {
                assert_eq!(
                    total.work[key],
                    one.operations[&ProfileOp::BoundaryEscape].work[key]
                );
            }
            if std::thread::available_parallelism().unwrap().get() > 1 {
                let worker = profile.workers.unwrap();
                assert_eq!(worker.jobs, total.work["commodity-screens"]);
                assert_eq!(
                    worker.operations[&ProfileOp::BoundaryFlood].calls,
                    worker.jobs
                );
                assert!(worker.max_parallelism <= workers);
            }
        }
    }
}

#[test]
fn analysis_worker_limits_return_structured_input_errors() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b")]);
    for count in [0, 17] {
        let mut req = request("b");
        req.analysis_workers = count;
        let report = construct_production(&w, &f, &i, &t, &c, &req);
        assert!(!report.success);
        assert_eq!(report.diagnostics[0].code, "production-input-invalid");
    }
}

#[test]
fn completes_connected_chain_without_compact_module_discovery() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    let r = construct_production(&w, &f, &i, &t, &c, &request("c"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    assert_eq!(r.steps.len(), 2);
    assert_eq!(r.final_node.as_ref().unwrap().member_instances.len(), 3);
    for step in &r.steps {
        network::validate(&step.node, &f, &t, &c).unwrap();
        assert_eq!(step.statistics.valid_candidates_scored, 0);
    }
    let repeated = construct_production(&w, &f, &i, &t, &c, &request("c"));
    assert_eq!(r.final_node, repeated.final_node);
}

#[test]
fn boundary_screen_matches_reference_across_growth_commodities_and_geometry_changes() {
    let (f, i, t, c) = catalogs();
    for edges in [
        vec![("a", "b"), ("b", "c")],
        vec![("a", "b"), ("a", "c")],
        vec![("a", "b"), ("b", "c"), ("c", "a")],
    ] {
        let r = construct_production(&wiring(&edges), &f, &i, &t, &c, &request("c"));
        assert!(r.complete, "{:?}: {:?}", edges, r.diagnostics);
        for step in &r.steps {
            for offset in [0, 3, -1] {
                let mut node = translate_node(&step.node, offset, offset);
                for variant in 0..4 {
                    if variant == 1 {
                        let mut extra = node.boundary_requirements.clone();
                        for b in &mut extra {
                            b.item.push_str("-other");
                            b.requirement.push_str("-other");
                        }
                        node.boundary_requirements.extend(extra);
                    } else if variant == 2 {
                        for n in &mut node.transport_networks {
                            n.transport = TransportKind::Pipe;
                        }
                    } else if variant == 3 {
                        let facility = &node.placements[0];
                        for b in &mut node.boundary_requirements {
                            for a in &mut b.attachments {
                                let p = match a {
                                    ConstructiveBoundaryAttachment::NewPort { port } => {
                                        &mut port.connection
                                    }
                                    ConstructiveBoundaryAttachment::ExistingNetwork {
                                        branch_cell,
                                        ..
                                    } => branch_cell,
                                };
                                p.x = facility.x;
                                p.y = facility.y;
                            }
                        }
                    }
                    let expected = boundary::reference::blocked_boundaries(&node, &t, &c);
                    for workers in [1, 4] {
                        assert_eq!(
                            blocked_boundaries(&node, &t, &c, workers).unwrap(),
                            expected
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn boundary_coordinate_failure_is_not_a_blocked_requirement() {
    let (f, i, t, c) = catalogs();
    let r = construct_production(&wiring(&[("a", "b")]), &f, &i, &t, &c, &request("b"));
    let mut node = r.final_node.unwrap();
    node.placements[0].x = i64::MAX;
    assert_eq!(
        blocked_boundaries(&node, &t, &c, 1),
        Err(boundary::ScreenError::Coordinates)
    );
}

#[test]
fn reuses_existing_producer_and_constructs_shared_network() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("a", "c")]);
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    let n = r.final_node.unwrap();
    assert_eq!(n.member_instances.len(), 3);
    assert_eq!(n.logistics_components.len(), 1);
    assert_eq!(n.transport_networks.len(), 1);
    assert_eq!(n.internal_requirements.len(), 2);
}

#[test]
fn commits_cycle_atomically_and_routes_all_induced_edges() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c"), ("c", "a")]);
    let r = construct_production(&w, &f, &i, &t, &c, &request("c"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    assert_eq!(r.steps.len(), 1);
    assert_eq!(r.steps[0].added_instances.len(), 2);
    assert_eq!(r.steps[0].node.internal_requirements.len(), 3);
}

#[test]
fn operation_budget_preserves_seed_and_does_not_claim_completion() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c"), ("c", "a")]);
    let mut req = request("c");
    req.max_route_searches = 1;
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert!(!r.success);
    assert!(!r.internal_complete);
    assert!(r.steps.is_empty());
    let n = r.final_node.unwrap();
    assert_eq!(n.member_instances, vec!["c"]);
    assert!(n.transport_networks.is_empty());
    assert_eq!(r.failed_statistics.astar_searches, 1);
}

#[test]
fn step_limit_is_a_partial_result_not_a_successful_blueprint() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    let mut req = request("c");
    req.max_steps = 1;
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert_eq!(r.status, "step-limit");
    assert!(!r.success);
    assert_eq!(r.steps.len(), 1);
    assert_eq!(r.unresolved_internal_requirements.len(), 1);
}

#[test]
fn realizes_external_io_before_declaring_complete() {
    let (f, i, t, c) = catalogs();
    let mut w = wiring(&[("a", "b")]);
    w.nodes.push(FacilityInstanceWiringNode::External {
        id: "import".into(),
        item: "ore".into(),
    });
    w.edges.push(FacilityInstanceWiringEdge {
        id: "import-edge".into(),
        source: "import".into(),
        target: "a".into(),
        kind: "external-input".into(),
        item: "ore".into(),
        rate: Rate {
            numerator: 1,
            denominator: 5,
        },
        projection: FacilityInstanceWiringProjection::Original,
    });
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert!(r.internal_complete, "{:?}", r.diagnostics);
    assert!(r.complete);
    assert!(r.success);
    assert_eq!(r.status, "complete");
    assert!(r.unresolved_external_requirements.is_empty());
    let n = r.final_node.unwrap();
    assert_eq!(n.external_requirements, vec!["import-edge"]);
    network::validate(&n, &f, &t, &c).unwrap();
    audit::validate_coverage(&n, &w, &i).unwrap();
    for rotation in [0, 90, 180, 270] {
        network::validate(&canonicalize_node(&rotate_node(&n, rotation)), &f, &t, &c).unwrap();
    }
    let mut wrong = w.clone();
    wrong.edges[0].rate = Rate {
        numerator: 3,
        denominator: 10,
    };
    assert!(
        audit::validate_coverage(&n, &wrong, &i).is_err(),
        "physical totals must match the original request"
    );
    let mut bad = n.clone();
    let terminal = bad
        .transport_networks
        .iter_mut()
        .flat_map(|n| &mut n.terminals)
        .find(|t| matches!(t.endpoint, TransportNetworkEndpoint::External { .. }))
        .unwrap();
    terminal.position.x += 1;
    terminal.position.y += 1;
    assert!(network::validate(&bad, &f, &t, &c).is_err());
}

#[test]
fn rejects_duplicate_wiring_ids_and_zero_budgets() {
    let (f, i, t, c) = catalogs();
    let mut w = wiring(&[("a", "b")]);
    w.edges.push(w.edges[0].clone());
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert_eq!(r.status, "invalid-input");
    assert!(r.final_node.is_none());
    let mut req = request("b");
    req.max_route_searches = 0;
    assert!(
        construct_production(&wiring(&[("a", "b")]), &f, &i, &t, &c, &req)
            .final_node
            .is_none()
    );
}

#[test]
fn separated_moss_fan_in_preserves_four_total_output_port_domains() {
    let (_, _, t, c) = catalogs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(root.join("data/game/normalized/facilities.json"))
            .unwrap(),
    )
    .unwrap();
    let i = ValidatedItemCatalog::try_from_catalog(
        crate::logistics::load_item_catalog(root.join("data/game/normalized/items.json")).unwrap(),
    )
    .unwrap();
    let mut w = wiring(&[("planter-a", "grinder"), ("planter-b", "grinder")]);
    for node in &mut w.nodes {
        if let FacilityInstanceWiringNode::Facility { id, facility, .. } = node {
            *facility = if id == "grinder" {
                "grinder-1-mode-normal"
            } else {
                "planter-1-mode-normal"
            }
            .into();
        }
    }
    for e in &mut w.edges {
        e.item = "item-plant-moss-2".into();
        e.rate = Rate {
            numerator: 1,
            denominator: 2,
        };
    }
    for (j, id) in ["planter-a", "planter-b"].into_iter().enumerate() {
        let endpoint = format!("external-{j}");
        w.nodes.push(FacilityInstanceWiringNode::Target {
            id: endpoint.clone(),
            item: "item-plant-moss-2".into(),
        });
        w.edges.push(FacilityInstanceWiringEdge {
            id: format!("export-{j}"),
            source: id.into(),
            target: endpoint,
            kind: "target-output".into(),
            item: "item-plant-moss-2".into(),
            rate: Rate {
                numerator: 1,
                denominator: 4,
            },
            projection: FacilityInstanceWiringProjection::Original,
        });
    }
    let r = construct_production(&w, &f, &i, &t, &c, &request("grinder"));
    assert!(
        r.internal_complete,
        "{:?} {:?}",
        r.diagnostics, r.rejected_reasons
    );
    let n = &r
        .steps
        .iter()
        .rev()
        .find(|s| s.stage == "internal-growth")
        .unwrap()
        .node;
    assert_eq!(n.placements.len(), 3);
    assert_eq!(n.boundary_requirements.len(), 2);
    assert!(blocked_boundaries(&n, &t, &c, 1).unwrap().is_empty());
    let used = n
        .transport_networks
        .iter()
        .flat_map(|n| &n.terminals)
        .filter(|t| t.direction == FacilityPortDirection::Output)
        .count();
    let remaining = n
        .boundary_requirements
        .iter()
        .flat_map(|b| &b.attachments)
        .filter(|a| matches!(a, ConstructiveBoundaryAttachment::NewPort { .. }))
        .count();
    assert_eq!(used + remaining, 4);
    assert!(r.complete);
}

#[test]
fn grows_pipe_frontier_before_lexically_earlier_belt_frontier() {
    let (_, _, t, c) = catalogs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(
            root.join("data/examples/constructive-production.facilities.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let i = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
        schema_version: 1,
        items: vec![
            ItemDefinition {
                id: "ore".into(),
                transport: TransportKind::Belt,
            },
            ItemDefinition {
                id: "fluid".into(),
                transport: TransportKind::Pipe,
            },
        ],
    })
    .unwrap();
    let mut w = wiring(&[("belt", "target"), ("pipe", "target")]);
    for node in &mut w.nodes {
        if let FacilityInstanceWiringNode::Facility { facility, .. } = node {
            *facility = "mixed-machine".into();
        }
    }
    w.edges[1].item = "fluid".into();
    let r = construct_production(&w, &f, &i, &t, &c, &request("target"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    assert_eq!(r.steps[0].added_instances, vec!["pipe"]);
    assert_eq!(
        r.steps[0].node.transport_networks[0].transport,
        TransportKind::Pipe
    );
}

#[test]
fn splits_new_demand_across_partially_loaded_existing_ports() {
    let (_, i, t, c) = catalogs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(root.join("data/game/normalized/facilities.json"))
            .unwrap(),
    )
    .unwrap();
    let mut w = wiring(&[("a", "b"), ("a", "c"), ("a", "d")]);
    for node in &mut w.nodes {
        if let FacilityInstanceWiringNode::Facility { facility, .. } = node {
            *facility = "grinder-1-mode-normal".into();
        }
    }
    w.edges[0].rate = Rate {
        numerator: 3,
        denominator: 10,
    };
    w.edges[1].rate = Rate {
        numerator: 3,
        denominator: 10,
    };
    w.edges[2].rate = Rate {
        numerator: 2,
        denominator: 5,
    };
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    let n = r.final_node.unwrap();
    let lanes = n
        .attachment_events
        .iter()
        .filter(|e| e.requirement == "edge-2")
        .collect::<Vec<_>>();
    assert_eq!(lanes.len(), 2);
    assert!(lanes.iter().all(|e| e.rate
        == Rate {
            numerator: 1,
            denominator: 5
        }
        && e.source_kind == "existing-network"));
    network::validate(&n, &f, &t, &c).unwrap();
}

#[test]
fn external_lane_budget_is_atomic_and_does_not_claim_success() {
    let (_, i, t, c) = catalogs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(root.join("data/game/normalized/facilities.json"))
            .unwrap(),
    )
    .unwrap();
    let mut w = wiring(&[("a", "b")]);
    w.nodes
        .retain(|n| matches!(n,FacilityInstanceWiringNode::Facility{id,..}if id=="a"));
    if let FacilityInstanceWiringNode::Facility { facility, .. } = &mut w.nodes[0] {
        *facility = "grinder-1-mode-normal".into();
    }
    w.nodes.push(FacilityInstanceWiringNode::External {
        id: "import".into(),
        item: "ore".into(),
    });
    w.edges[0].source = "import".into();
    w.edges[0].target = "a".into();
    w.edges[0].rate = Rate {
        numerator: 1,
        denominator: 1,
    };
    let mut req = request("a");
    req.max_route_searches = 1;
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert!(r.internal_complete);
    assert!(!r.complete);
    assert!(!r.success);
    assert_eq!(r.failed_statistics.astar_searches, 1);
    let n = r.final_node.unwrap();
    assert!(n.transport_networks.is_empty());
    assert!(n.external_requirements.is_empty());
    assert_eq!(r.unresolved_external_requirements.len(), 1);
    network::validate(&n, &f, &t, &c).unwrap();
    let full = construct_production(&w, &f, &i, &t, &c, &request("a"));
    assert!(full.complete, "{:?}", full.diagnostics);
    let n = full.final_node.unwrap();
    assert_eq!(
        n.transport_networks[0]
            .terminals
            .iter()
            .filter(|t| matches!(t.endpoint, TransportNetworkEndpoint::External { .. }))
            .count(),
        2
    );
    audit::validate_coverage(&n, &w, &i).unwrap();
    if let FacilityInstanceWiringNode::External { item, .. } = &mut w.nodes[1] {
        *item = "wrong-item".into();
    }
    assert_eq!(
        construct_production(&w, &f, &i, &t, &c, &request("a")).status,
        "invalid-input"
    );
}
