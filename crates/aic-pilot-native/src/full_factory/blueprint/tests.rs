use std::sync::Arc;

use serde_json::{Value, json};

use super::super::tests::{fixture, fixture_request};
use super::*;

mod checkpoint;
mod guards;

fn session() -> Session {
    let (w, catalogs, _) = fixture();
    Session::new(
        fixture_request(&w),
        w,
        Arc::new(catalogs),
        Config {
            schema: CONTRACT.into(),
            max_actions: 256,
            elapsed_ms: 60_000,
            route_expansions: 100_000,
            search_padding: 4,
        },
    )
    .unwrap()
}

fn step(session: &mut Session, action: Value) -> Value {
    let observation = session.observation().unwrap();
    session.step_json(&json!({"blueprint_sha256": observation["blueprint_sha256"], "revision": observation["revision"], "action": action}).to_string())
}

#[test]
fn overlap_persists_and_exact_original_connections_recover() {
    let mut s = session();
    let initial = s.observation().unwrap();
    assert_eq!(
        step(
            &mut s,
            json!({"kind": "pose", "instance": "A0", "x": 2, "y": 7, "rotation": 90})
        )["status"],
        "draft_accepted"
    );
    let overlap = s.observation().unwrap();
    assert!(!overlap["conflicts"].as_array().unwrap().is_empty());
    assert!(
        overlap["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["status"] == "pending")
    );
    assert_eq!(
        step(&mut s, json!({"kind": "commit"}))["diagnostics"][0]["code"],
        "blueprint.unfinished-conflicts"
    );
    assert_eq!(s.observation().unwrap()["revision"], overlap["revision"]);
    assert_eq!(
        step(
            &mut s,
            json!({"kind": "pose", "instance": "A1", "x": 2, "y": 11, "rotation": 90})
        )["status"],
        "draft_accepted"
    );
    assert_eq!(
        step(
            &mut s,
            json!({"kind": "pose", "instance": "A0", "x": 3, "y": 2, "rotation": 90})
        )["status"],
        "draft_accepted"
    );
    let repaired = step(&mut s, json!({"kind": "repair_all"}));
    assert_eq!(repaired["status"], "draft_accepted", "{repaired}");
    let committed = step(&mut s, json!({"kind": "commit"}));
    assert!(
        matches!(committed["status"].as_str(), Some("validated" | "improved")),
        "{committed}"
    );
    let after = s.observation().unwrap();
    assert_eq!(initial["blueprint"], after["blueprint"]);
    assert_eq!(initial["blueprint_sha256"], after["blueprint_sha256"]);
    assert!(
        after["edges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["status"] == "routed")
    );
    assert!(after["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(
        committed["details"]["native_acceptance"]["factory_validated"],
        true
    );
}

#[test]
fn topology_edit_and_wrong_endpoint_are_rejected_without_mutation() {
    let mut s = session();
    let before = s.observation().unwrap();
    let paths = &before["edges"];
    let edge = 0;
    let mut cells = paths[edge]["path"].as_array().unwrap().clone();
    cells.last_mut().unwrap()["x"] = json!(999);
    let wrong = step(
        &mut s,
        json!({"kind": "route", "edge": edge, "cells": cells}),
    );
    assert_eq!(wrong["diagnostics"][0]["code"], "blueprint.route-endpoint");
    for action in [
        json!({"kind": "rewire", "edge": 0, "target": 2}),
        json!({"kind": "route", "edge": 0, "cells": paths[0]["path"], "target": 2}),
        json!({"kind": "pose", "instance": "A0", "x": 2, "y": 2, "rotation": 90, "recipe": "changed"}),
    ] {
        assert_eq!(
            step(&mut s, action)["diagnostics"][0]["code"],
            "blueprint.action-schema"
        );
    }
    let after = s.observation().unwrap();
    for key in [
        "revision",
        "blueprint",
        "facilities",
        "edges",
        "best_metrics",
    ] {
        assert_eq!(before[key], after[key]);
    }
    assert_eq!(after["attempts"], 4);
}

#[test]
fn unfinished_stop_and_failed_repairs_retain_valid_best() {
    let mut s = session();
    let before = s.observation().unwrap();
    step(
        &mut s,
        json!({"kind": "pose", "instance": "A0", "x": 2, "y": 7, "rotation": 90}),
    );
    let draft = s.observation().unwrap();
    let outcome = step(&mut s, json!({"kind": "repair_all"}));
    assert_eq!(outcome["status"], "rejected");
    assert_eq!(draft["edges"], s.observation().unwrap()["edges"]);
    let result = s.finish();
    assert_eq!(result["factory_validated"], true);
    assert_eq!(result["unfinished_draft"], true);
    assert_eq!(result["best_metrics"], before["best_metrics"]);
    assert_eq!(
        step(&mut s, json!({"kind": "restore_best"}))["charged"],
        false
    );
    let (w, catalogs, _) = fixture();
    let best = serde_json::from_value(result["best"].clone()).unwrap();
    assert!(
        aic_data::layouts::validate_factory_layout(&fixture_request(&w), &best, &catalogs)
            .factory_validated
    );
}

#[test]
fn no_hidden_32_action_terminal_or_overlap_reward() {
    let mut s = session();
    let before = s.observation().unwrap();
    for _ in 0..40 {
        let outcome = step(
            &mut s,
            json!({"kind": "pose", "instance": "A0", "x": 2, "y": 7, "rotation": 90}),
        );
        assert_eq!(outcome["status"], "draft_accepted");
        assert_eq!(outcome["reward"], 0.);
        assert_eq!(outcome["done"], false);
    }
    let after = s.observation().unwrap();
    assert_eq!(after["attempts"], 40);
    assert!(!after["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(after["blueprint"], before["blueprint"]);
    assert_eq!(step(&mut s, json!({"kind": "restore_best"}))["reward"], 0.);
    assert!(
        s.observation().unwrap()["conflicts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn smaller_valid_reembedding_is_rewarded_only_at_commit() {
    let mut s = session();
    let before = s.observation().unwrap();
    let moved = step(
        &mut s,
        json!({"kind": "pose", "instance": "C1", "x": 27, "y": 13, "rotation": 90}),
    );
    assert_eq!(moved["reward"], 0.);
    let repaired = step(&mut s, json!({"kind": "repair_all"}));
    assert_eq!(repaired["status"], "draft_accepted", "{repaired}");
    assert_eq!(repaired["reward"], 0.);
    let committed = step(&mut s, json!({"kind": "commit"}));
    assert_eq!(committed["status"], "improved", "{committed}");
    assert!(committed["reward"].as_f64().unwrap() > 0.);
    assert!(
        committed["best_metrics"]["score"][0].as_u64().unwrap()
            < before["best_metrics"]["score"][0].as_u64().unwrap()
    );
    assert_eq!(step(&mut s, json!({"kind": "commit"}))["reward"], 0.);
}
