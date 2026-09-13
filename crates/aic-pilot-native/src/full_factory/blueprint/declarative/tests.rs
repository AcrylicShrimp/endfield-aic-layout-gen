use super::super::super::tests::{fixture, fixture_request};
use super::*;

fn session() -> Session {
    let (w, catalogs, _) = fixture();
    Session::new(
        fixture_request(&w),
        w,
        Arc::new(catalogs),
        Limits {
            schema: SCHEMA.into(),
            max_actions: 256,
            elapsed_ms: 60_000,
            route_expansions: 100_000,
            max_total_route_expansions: 10_000_000,
            search_padding: 4,
        },
    )
    .unwrap()
}

fn place(s: &Session, instance: &str, x: i64, y: i64, rotation: i64) -> Value {
    let ports: Vec<_> = s
        .blueprint
        .nodes
        .iter()
        .filter(|n| {
            matches!(n.terminal.as_ref().map(|t| &t.endpoint),
        Some(TransportNetworkEndpoint::Facility{instance:owner,..}) if owner==instance)
        })
        .map(|n| json!({"node": n.id, "port": s.state.ports[n.id]}))
        .collect();
    json!({"kind":"place","instance":instance,"x":x,"y":y,"rotation":rotation,"ports":ports})
}
fn step(s: &mut Session, action: Value) -> Value {
    s.step_json(
        &json!({"blueprint_sha256":s.blueprint_sha256,"revision":s.revision,"action":action})
            .to_string(),
    )
}
fn old_step(s: &mut super::super::Session, action: Value) -> Value {
    let o = s.observation().unwrap();
    s.step_json(
        &json!({"blueprint_sha256":o["blueprint_sha256"],"revision":o["revision"],"action":action})
            .to_string(),
    )
}

#[test]
fn automatic_common_matches_explicit_orchestration_and_saves_known_improvement() {
    let mut s = session();
    let mut old = super::super::Session::new(
        s.request.clone(),
        s.original.clone(),
        s.catalogs.clone(),
        Config {
            schema: CONTRACT.into(),
            max_actions: 768,
            elapsed_ms: 60_000,
            route_expansions: 100_000,
            search_padding: 4,
        },
    )
    .unwrap();
    for (instance, x, y) in [("C1", 27, 13), ("A0", 2, 7), ("A1", 2, 11), ("A0", 3, 2)] {
        let action = place(&s, instance, x, y, 90);
        let new = step(&mut s, action);
        old_step(
            &mut old,
            json!({"kind":"pose","instance":instance,"x":x,"y":y,"rotation":90}),
        );
        old_step(&mut old, json!({"kind":"repair_all"}));
        old_step(&mut old, json!({"kind":"commit"}));
        let a = s.observation().unwrap();
        let b = old.observation().unwrap();
        for key in ["facilities", "edges", "best_metrics", "conflicts"] {
            assert_eq!(a[key], b[key], "{key}: {new}");
        }
    }
    assert!(s.best_metrics.score < s.initial_metrics.score);
    assert_eq!(s.attempts, 4);
    assert_eq!(old.observation().unwrap()["attempts"], 12);
}

#[test]
fn incomplete_draft_survives_restore_and_later_clearance_without_commit() {
    let mut s = session();
    let initial = s.observation().unwrap();
    let action = place(&s, "A0", 2, 7, 90);
    assert_eq!(step(&mut s, action)["status"], "draft_accepted");
    let saved = s.checkpoint();
    let mut restored = Session::from_checkpoint(&saved.to_string(), s.catalogs.clone()).unwrap();
    assert_eq!(restored.state, s.state);
    assert_eq!(restored.attempts, 1);
    assert!(restored.elapsed().as_nanos() >= saved["elapsed_ns"].as_u64().unwrap() as u128);
    assert!(!restored.validated);
    for (instance, x, y) in [("A1", 2, 11), ("A0", 3, 2)] {
        let action = place(&restored, instance, x, y, 90);
        let outcome = step(&mut restored, action);
        assert_ne!(outcome["status"], "rejected", "{outcome}");
    }
    assert!(restored.validated);
    assert_eq!(
        restored.observation().unwrap()["blueprint"],
        initial["blueprint"]
    );
    let result = s.finish();
    assert_eq!(result["unfinished_draft"], true);
    assert_eq!(result["best_metrics"], initial["best_metrics"]);
}

#[test]
fn grid_translation_preserves_branches_bridge_and_ports_and_can_be_reversed() {
    let mut s = session();
    let original = s.state.clone();
    for axis in ["x", "y"] {
        let outcome = step(
            &mut s,
            json!({"kind":"insert_band","axis":axis,"at":0,"count":3}),
        );
        assert_eq!(outcome["status"], "validated", "{outcome}");
        assert_eq!(outcome["details"]["route_calls"], 0);
        let moved = s.witness(&s.state).unwrap();
        assert!(validate_factory_layout(&s.request, &moved, &s.catalogs).factory_validated);
        let outcome = step(
            &mut s,
            json!({"kind":"delete_band","axis":axis,"at":0,"count":3}),
        );
        assert_eq!(outcome["status"], "validated", "{outcome}");
        assert_eq!(s.state, original);
    }
    let outcome = step(
        &mut s,
        json!({"kind":"delete_band","axis":"x","at":24,"count":1}),
    );
    assert_eq!(outcome["status"], "improved", "{outcome}");
    assert!(s.best_metrics.score[0] < s.initial_metrics.score[0]);
}

#[test]
fn row_deletion_cannot_remove_obligations_and_joint_port_edits_are_atomic() {
    let mut s = session();
    let before = s.state.clone();
    for action in [
        json!({"kind":"delete_band","axis":"x","at":3,"count":1}),
        json!({"kind":"insert_band","axis":"x","at":3,"count":1}),
        json!({"kind":"delete_band","axis":"x","at":10,"count":1}),
        json!({"kind":"delete_band","axis":"x","at":0,"count":1}),
        json!({"kind":"repair_all"}),
        json!({"kind":"commit"}),
    ] {
        assert_eq!(step(&mut s, action)["status"], "rejected");
        assert_eq!(s.state, before);
    }
    let mut action = place(&s, "C1", 27, 13, 90);
    let assignments = action["ports"].as_array_mut().unwrap();
    assignments[1]["port"] = assignments[0]["port"].clone();
    assert_eq!(step(&mut s, action)["status"], "rejected");
    assert_eq!(s.state, before);
    let mut saved = s.checkpoint();
    saved["original"]["placements"][0]["x"] = json!(999);
    assert!(Session::from_checkpoint(&saved.to_string(), s.catalogs.clone()).is_err());
    let mut saved = s.checkpoint();
    saved["state"]["ports"][0] = json!("missing-port");
    assert!(Session::from_checkpoint(&saved.to_string(), s.catalogs.clone()).is_err());
}

#[test]
fn unchanged_decisions_skip_routing_and_validation_and_horizon_is_explicit() {
    let mut s = session();
    for _ in 0..40 {
        let action = place(&s, "A0", 2, 2, 90);
        let outcome = step(&mut s, action);
        assert_eq!(outcome["status"], "unchanged");
        assert_eq!(outcome["details"]["route_calls"], 0);
        assert_eq!(outcome["details"]["validations"], 0);
        assert_eq!(outcome["done"], false);
    }
    assert_eq!(s.attempts, 40);
    s.finish();
    let before = s.state.clone();
    let action = place(&s, "A0", 2, 7, 90);
    assert_eq!(step(&mut s, action)["charged"], false);
    assert_eq!(s.state, before);
}

#[test]
fn exhausted_routing_budget_retains_edited_draft_and_valid_best() {
    let mut s = session();
    s.config.route_expansions = 1;
    s.config.max_total_route_expansions = 1;
    let action = place(&s, "C1", 27, 13, 90);
    let result = step(&mut s, action);
    assert_eq!(result["status"], "draft_accepted", "{result}");
    assert_eq!(result["done"], true);
    assert!(
        s.state
            .geometry
            .placements
            .iter()
            .any(|p| p.instance == "C1" && p.y == 13)
    );
    assert!(s.state.geometry.paths.iter().any(Option::is_none));
    assert_eq!(s.best_metrics.score, s.initial_metrics.score);
    assert!(s.work.route_expansions <= 1);
    let result = s.finish();
    let witness = serde_json::from_value(result["best"].clone()).unwrap();
    assert!(validate_factory_layout(&s.request, &witness, &s.catalogs).factory_validated);
}
