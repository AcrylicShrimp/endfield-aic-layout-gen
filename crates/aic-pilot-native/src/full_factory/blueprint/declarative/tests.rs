use super::super::super::tests::{fixture, fixture_request};
use super::*;

fn session() -> Session {
    let (w, catalogs, _) = fixture();
    Session::new(
        fixture_request(&w),
        w,
        Arc::new(crate::input::Catalogs::new(catalogs)),
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
    let decision = serde_json::from_value(
        json!({"blueprint_sha256":s.blueprint_sha256,"revision":s.revision,"action":action}),
    )
    .map_err(|e| error("action-schema", e.to_string(), "action"));
    s.step(|| decision, None)
}
#[test]
fn automatic_edits_save_known_improvement() {
    let mut s = session();
    for (instance, x, y) in [("C1", 27, 13), ("A0", 2, 7), ("A1", 2, 11), ("A0", 3, 2)] {
        let action = place(&s, instance, x, y, 90);
        step(&mut s, action);
    }
    assert!(s.best_metrics.score < s.initial_metrics.score);
    assert_eq!(s.attempts, 4);
}

#[test]
fn incomplete_draft_survives_restore_and_later_clearance_without_commit() {
    let mut s = session();
    let blueprint = serde_json::to_value(&s.blueprint).unwrap();
    let best_metrics = serde_json::to_value(&s.best_metrics).unwrap();
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
        serde_json::to_value(&restored.blueprint).unwrap(),
        blueprint
    );
    let result = s.retained_result();
    assert_eq!(result["unfinished_draft"], true);
    assert_eq!(result["best_metrics"], best_metrics);
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
        assert!(validate_factory_layout(&s.request, &moved, &s.catalogs.data).factory_validated);
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
    s.config.max_actions = s.attempts;
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
    let result = s.retained_result();
    let witness = serde_json::from_value(result["best"].clone()).unwrap();
    assert!(validate_factory_layout(&s.request, &witness, &s.catalogs.data).factory_validated);
}

#[test]
fn typed_worker_pool_preserves_ownership_inputs_and_cold_restore() {
    use crate::learning::{
        Options,
        pool::{Command, Pool, Reply},
    };
    let base = session();
    let options: Options = serde_json::from_value(
        json!({"translation_steps":[-1,1],"band_counts":[1],"masking":"structural-bands-v1"}),
    )
    .unwrap();
    let mut pool = Pool::new(base.catalogs.clone(), 2, 2).unwrap();
    let opened = pool
        .exchange(
            (0..2)
                .rev()
                .map(|i| {
                    (
                        i,
                        Command::Open {
                            request: base.request.clone(),
                            original: base.original.clone(),
                            limits: base.config.clone(),
                            options: options.clone(),
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
    assert_eq!(opened.iter().map(|r| r.0).collect::<Vec<_>>(), vec![0, 1]);
    assert!(
        pool.exchange(vec![(0, Command::Snapshot), (0, Command::Snapshot)])
            .is_err()
    );
    let observations: Vec<_> = pool
        .exchange((0..2).map(|i| (i, Command::Begin { actions: 2 })).collect())
        .unwrap()
        .into_iter()
        .map(|(_, r)| {
            let Reply::Observation(o) = r else {
                panic!("observation")
            };
            o
        })
        .collect();
    let retained = serde_json::to_value(observations[0].input.as_ref()).unwrap();
    let commands = observations
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let root = o
                .input
                .root_meta
                .iter()
                .enumerate()
                .find(|(i, r)| r[0] == 1 && !o.input.root_invalid[*i])
                .unwrap()
                .0;
            (
                i,
                Command::Step {
                    handle: o.handle,
                    root,
                    ports: vec![],
                },
            )
        })
        .collect();
    let after = pool.exchange(commands).unwrap();
    assert_eq!(
        serde_json::to_value(observations[0].input.as_ref()).unwrap(),
        retained
    );
    let Reply::Step { next, result } = &after[0].1 else {
        panic!("step")
    };
    assert!(Arc::ptr_eq(&observations[0].domain, &next.domain));
    assert!(!Arc::ptr_eq(&observations[0].input, &next.input));
    assert_eq!(result["outcome"]["charged"], true);
    assert!(
        pool.exchange(vec![
            (1, Command::Snapshot),
            (
                0,
                Command::Step {
                    handle: observations[0].handle,
                    root: 0,
                    ports: vec![]
                }
            )
        ])
        .is_err()
    );
    let partial = pool.take_failed_replies();
    assert_eq!(partial.len(), 1);
    assert_eq!(partial[0].0, 1);
    assert!(pool.take_failed_replies().is_empty());
    let snapshots = pool
        .exchange((0..2).map(|i| (i, Command::Snapshot)).collect())
        .unwrap();
    let evidence = pool
        .exchange((0..2).map(|i| (i, Command::Drain)).collect())
        .unwrap();
    for (_, r) in evidence {
        let Reply::Data(rows) = r else {
            panic!("evidence")
        };
        assert_eq!(rows.as_array().unwrap().len(), 1);
    }
    drop(pool);
    let mut restored = Pool::new(base.catalogs, 1, 2).unwrap();
    restored
        .exchange(
            snapshots
                .into_iter()
                .map(|(i, r)| {
                    let Reply::Data(saved) = r else {
                        panic!("snapshot")
                    };
                    assert_eq!(saved["attempts"], 1);
                    (
                        i,
                        Command::Restore {
                            checkpoint: saved.to_string(),
                            options: options.clone(),
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
    let restored = restored
        .exchange((0..2).map(|i| (i, Command::Begin { actions: 2 })).collect())
        .unwrap();
    for ((_, new), (_, old)) in restored.iter().zip(&after) {
        let Reply::Observation(new) = new else {
            panic!("restore")
        };
        let Reply::Step { next: old, .. } = old else {
            panic!("step")
        };
        assert_eq!(new.input.node_state, old.input.node_state);
        assert_eq!(new.input.action_features, old.input.action_features);
        assert_eq!(new.input.root_meta, old.input.root_meta);
        assert_eq!(new.attempts, old.attempts);
        assert_eq!(new.revision, old.revision);
        for (i, (a, b)) in new
            .input
            .global_features
            .iter()
            .zip(&old.input.global_features)
            .enumerate()
        {
            if i != new.input.global_features.len() - 2 {
                assert_eq!(a, b);
            } // Actual elapsed-time feature advances.
        }
    }
}
