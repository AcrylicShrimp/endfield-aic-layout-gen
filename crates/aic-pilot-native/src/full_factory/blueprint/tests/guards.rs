use super::*;

#[test]
fn original_bridge_channel_and_unintended_intersections_are_checked_before_mutation() {
    let (w, catalogs, _) = fixture();
    let (bp, draft) = topology::compile(&w).unwrap();
    let edge = bp
        .edges
        .iter()
        .find(|e| {
            !bp.nodes[e.source].components.is_empty()
                && draft.paths[e.id].as_ref().unwrap().len() > 3
        })
        .unwrap();
    let mut path = draft.paths[edge.id].clone().unwrap();
    let first = path[0];
    path[1] = first.offset((edge.first.unwrap() + 1) % 4);
    assert_eq!(
        geometry::check_path(&bp, &draft, &catalogs, edge, &path)
            .unwrap_err()
            .code,
        "blueprint.route-direction"
    );
    // A retained route's interior and unrelated anchors remain obstacles even
    // when their commodity happens to match the repaired edge.
    let blocked = geometry::blocked(&bp, &draft, &catalogs, edge).unwrap();
    for other in &bp.edges {
        if other.id != edge.id
            && bp.networks[other.network]["transport"] == bp.networks[edge.network]["transport"]
        {
            let p = draft.paths[other.id].as_ref().unwrap();
            for cell in &p[1..p.len() - 1] {
                assert!(blocked.contains(cell));
            }
        }
    }
}

#[test]
fn action_and_elapsed_guards_preserve_the_last_valid_witness() {
    let (w, catalogs, _) = fixture();
    let mut s = Session::new(
        fixture_request(&w),
        w.clone(),
        Arc::new(catalogs),
        Config {
            schema: CONTRACT.into(),
            max_actions: 1,
            elapsed_ms: 60_000,
            route_expansions: 1,
            search_padding: 0,
        },
    )
    .unwrap();
    let result = step(
        &mut s,
        json!({"kind": "pose", "instance": "A0", "x": 2, "y": 7, "rotation": 90}),
    );
    assert_eq!(result["done"], true);
    assert_eq!(result["status"], "draft_accepted");
    let blocked = step(&mut s, json!({"kind": "restore_best"}));
    assert_eq!(blocked["charged"], false);
    assert_eq!(
        blocked["diagnostics"][0]["code"],
        "blueprint.budget-exhausted"
    );
    assert_eq!(s.finish()["best"], json!(w));

    let (w, catalogs, _) = fixture();
    let mut s = Session::new(
        fixture_request(&w),
        w.clone(),
        Arc::new(catalogs),
        Config {
            schema: CONTRACT.into(),
            max_actions: 100,
            elapsed_ms: 1,
            route_expansions: 1,
            search_padding: 0,
        },
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    assert_eq!(
        step(&mut s, json!({"kind": "commit"}))["diagnostics"][0]["code"],
        "blueprint.budget-exhausted"
    );
    assert_eq!(s.finish()["best"], json!(w));
}

#[test]
fn original_topology_hash_and_revision_are_required() {
    let mut s = session();
    let before = s.observation().unwrap();
    let outcome = s.step_json(&json!({"blueprint_sha256": "foreign-blueprint", "revision": 0, "action": {"kind": "restore_best"}}).to_string());
    assert_eq!(
        outcome["diagnostics"][0]["code"],
        "blueprint.stale-envelope"
    );
    let outcome = s.step_json(&json!({"blueprint_sha256": before["blueprint_sha256"], "revision": 1, "action": {"kind": "restore_best"}}).to_string());
    assert_eq!(
        outcome["diagnostics"][0]["code"],
        "blueprint.stale-envelope"
    );
    assert_eq!(s.observation().unwrap()["revision"], 0);
}

#[test]
fn routing_work_is_shared_and_unvalidated_exports_fail() {
    let (w, catalogs, _) = fixture();
    let mut s = Session::new(
        fixture_request(&w),
        w,
        Arc::new(catalogs),
        Config {
            schema: CONTRACT.into(),
            max_actions: 10,
            elapsed_ms: 60_000,
            route_expansions: 1,
            search_padding: 4,
        },
    )
    .unwrap();
    step(
        &mut s,
        json!({"kind": "pose", "instance": "C1", "x": 27, "y": 13, "rotation": 90}),
    );
    let before = s.observation().unwrap();
    assert_eq!(
        s.validated_draft().unwrap_err().code,
        "blueprint.draft-unvalidated"
    );
    let outcome = step(&mut s, json!({"kind": "repair_all"}));
    assert_eq!(outcome["status"], "rejected");
    assert!(outcome["details"]["route_expansions"].as_u64().unwrap() <= 1);
    let after = s.observation().unwrap();
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(before["edges"], after["edges"]);
}
