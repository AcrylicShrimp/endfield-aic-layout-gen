use super::super::super::tests::{fixture, fixture_request};
use super::*;

pub(super) fn session() -> Session {
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

#[test]
#[ignore = "set AIC_PLAN24_WITNESS to an admitted measured witness"]
fn measure_generated_input_and_checkpoint_bytes() {
    let path = std::env::var("AIC_PLAN24_WITNESS").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let (request, witness, input_bytes) = if value.get("original").is_some() {
        let generated: crate::generation::source::Generated =
            serde_json::from_value(value).unwrap();
        (generated.request, generated.original, bytes.len())
    } else {
        let witness: Witness = serde_json::from_value(value).unwrap();
        let request = fixture_request(&witness);
        let generated = crate::generation::source::Generated {
            request: request.clone(),
            original: witness.clone(),
            metadata: json!({}),
            admission: None,
        };
        (
            request,
            witness,
            serde_json::to_vec(&generated).unwrap().len(),
        )
    };
    let (_, catalogs, _) = fixture();
    let catalogs = Arc::new(crate::input::Catalogs::new(catalogs));
    let networks = witness.transport_networks.len();
    let cells = witness
        .transport_networks
        .iter()
        .map(|network| network.cells.len())
        .sum::<usize>();
    let segments = witness
        .transport_networks
        .iter()
        .map(|network| network.segments.len())
        .sum::<usize>();
    let terminals = witness
        .transport_networks
        .iter()
        .map(|network| network.terminals.len())
        .sum::<usize>();
    println!(
        "plan24_input={}",
        json!({"path":path,"facilities":witness.placements.len(),"networks":networks,
            "cells":cells,"segments":segments,"terminals":terminals,
            "input_bytes":input_bytes})
    );
    let started = std::time::Instant::now();
    let mut session = Session::new(
        request,
        witness.clone(),
        catalogs.clone(),
        Limits {
            schema: SCHEMA.into(),
            max_actions: 256,
            elapsed_ms: 60_000,
            route_expansions: 100_000,
            max_total_route_expansions: 10_000_000,
            search_padding: 4,
        },
    )
    .unwrap();
    let new_ms = started.elapsed().as_secs_f64() * 1000.0;
    let checkpoint = session.checkpoint().to_string();
    let checkpoint_bytes = checkpoint.len();
    let started = std::time::Instant::now();
    let restored = Session::from_checkpoint(&checkpoint, catalogs).unwrap();
    let restore_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(
        restored.checkpoint()["input_sha256"],
        session.checkpoint()["input_sha256"]
    );
    assert_eq!(restored.original.placements.len(), witness.placements.len());
    let mut changed_checkpoint_bytes = None;
    if std::env::var_os("AIC_PLAN24_STEP").is_some() {
        let first = &witness.placements[0];
        let action = place(
            &session,
            &first.instance,
            i64::from(first.x) + 1,
            i64::from(first.y),
            i64::from(first.rotation),
        );
        let outcome = step(&mut session, action);
        assert_ne!(outcome["status"], "rejected", "{outcome}");
        let changed = session.checkpoint().to_string();
        let resumed = Session::from_checkpoint(&changed, session.catalogs.clone()).unwrap();
        assert_eq!(resumed.state, session.state);
        assert_eq!(resumed.best_state, session.best_state);
        assert_eq!(
            serde_json::to_value(&resumed.work).unwrap(),
            serde_json::to_value(&session.work).unwrap()
        );
        assert_eq!(resumed.attempts, session.attempts);
        assert_eq!(resumed.revision, session.revision);
        assert_eq!(
            resumed.checkpoint()["source"],
            session.checkpoint()["source"]
        );
        assert_ne!(resumed.state, resumed.best_state);
        changed_checkpoint_bytes = Some(changed.len());
    }
    println!(
        "plan24={}",
        json!({"path":path,"facilities":witness.placements.len(),"networks":networks,
            "cells":cells,"segments":segments,"terminals":terminals,
            "input_bytes":input_bytes,"checkpoint_bytes":checkpoint_bytes,
            "changed_checkpoint_bytes":changed_checkpoint_bytes,
            "session_new_ms":new_ms,"restore_ms":restore_ms})
    );
}

pub(super) fn place(s: &Session, instance: &str, x: i64, y: i64, rotation: i64) -> Value {
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
pub(super) fn step(s: &mut Session, action: Value) -> Value {
    let decision = serde_json::from_value(
        json!({"schema":SCHEMA,"blueprint_sha256":s.blueprint_sha256,"revision":s.revision,"action":action}),
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
fn v4_snapshot_binds_common_problem_and_draft_before_restore() {
    let s = session();
    let saved = s.checkpoint();
    assert_eq!(saved["schema"], "declarative-edit-checkpoint-v4");
    assert!(saved["common_problem"].is_object());
    assert!(saved["common_draft"].is_object());
    assert!(saved["common_best_draft"].is_object());
    Session::from_checkpoint(&saved.to_string(), s.catalogs.clone()).unwrap();

    let mut changed = saved.clone();
    changed["common_problem"]["catalog_sha256"] = json!("0".repeat(64));
    let failure = Session::from_checkpoint(&changed.to_string(), s.catalogs.clone())
        .err()
        .unwrap();
    assert_eq!(failure.code, "declarative.checkpoint-common-problem");

    let mut changed = saved;
    changed["common_draft"]["anchors"][0]["x"] = json!(999);
    let failure = Session::from_checkpoint(&changed.to_string(), s.catalogs.clone())
        .err()
        .unwrap();
    assert_eq!(failure.code, "declarative.checkpoint-common-draft");
}

#[test]
fn global_translation_is_unchanged_and_preserves_branches_bridge_and_ports() {
    let mut s = session();
    let original = s.state.clone();
    for axis in ["x", "y"] {
        let outcome = step(
            &mut s,
            json!({"kind":"insert_band","axis":axis,"at":0,"count":3}),
        );
        assert_eq!(outcome["status"], "unchanged", "{outcome}");
        assert_eq!(outcome["details"]["route_calls"], 0);
        assert_eq!(outcome["details"]["validations"], 0);
        let moved = s.witness(&s.state).unwrap();
        assert!(validate_factory_layout(&s.request, &moved, &s.catalogs.data).factory_validated);
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
        let p = s
            .state
            .geometry
            .placements
            .iter()
            .find(|p| p.instance == "A0")
            .unwrap();
        let action = place(&s, "A0", p.x, p.y, p.rotation);
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
    assert_eq!(result["details"]["repair_batch_applied"], false);
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
fn automatic_repair_charges_post_search_resource_failure_and_reports_found_work() {
    let mut session = session();
    let mut draft = session.common_state.clone();
    draft.paths.fill(None);
    let before = draft.clone();
    let mut details = json!({"routes": []});
    let mut diagnostics = Vec::new();
    crate::physical_editor::fail_reservation_after("path", 0);
    let (_, accepted) = session
        .automatic(&mut draft, &mut details, &mut diagnostics)
        .unwrap();
    crate::physical_editor::clear_reservation_failure();
    assert!(accepted.is_none());
    assert_eq!(details["routes"][0]["status"], "found");
    assert_eq!(details["repair_batch_applied"], false);
    assert!(session.work.route_expansions > 0);
    assert_eq!(
        session.work.route_expansions,
        details["routes"][0]["expanded"].as_u64().unwrap() as usize
    );
    assert_eq!(diagnostics[0].code, "declarative.common-routing");
    assert_eq!(draft, before);
    session.config.max_total_route_expansions = session.work.route_expansions;
    assert!(session.done());
}

#[test]
fn action_resource_failure_maps_to_factory_diagnostic_without_committing() {
    let mut session = session();
    let before = session.common_state.clone();
    let action = place(&session, "A0", 2, 7, 90);
    crate::physical_editor::fail_reservation_after("path", 0);
    let outcome = step(&mut session, action);
    crate::physical_editor::clear_reservation_failure();
    assert_eq!(outcome["status"], "rejected", "{outcome}");
    assert_eq!(
        outcome["diagnostics"][0]["code"],
        "declarative.route-resource"
    );
    assert_eq!(outcome["details"]["route_expansions"], 0);
    assert_eq!(session.common_state, before);
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
        assert_eq!(new.input.global_features, old.input.global_features);
    }
}

/// Explicit continuity probe; never part of a normal training run.
#[test]
#[ignore]
fn extraction_reference() {
    let Some(output) = std::env::var_os("AIC_EDIT_REFERENCE") else {
        return;
    };
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).unwrap();
    let started = Instant::now();
    let mut rows = vec![];
    let mut numerical_bytes = 0usize;
    for _ in 0..16 {
        let mut s = session();
        let options = serde_json::from_value(json!({"translation_steps":[-2,-1,1,2],"band_counts":[1,2],"masking":"structural-bands-v1"})).unwrap();
        let mut learning = learning::Learning::new(&s, options).unwrap();
        // Reuse the historical script's physical destinations in its source
        // coordinate system. One external anchor tracks that diagnostic frame;
        // it is never fed to the model or retained in production state.
        let original_bp = topology::compile(&s.original).unwrap().0;
        let anchor = original_bp
            .nodes
            .iter()
            .find(|n| {
                matches!(
                    n.terminal.as_ref().map(|t| &t.endpoint),
                    Some(TransportNetworkEndpoint::External { .. })
                )
            })
            .unwrap()
            .id;
        let mut world_anchor = original_bp.nodes[anchor].initial_position;
        for round in 0..4 {
            let actions = [
                place(&s, "C1", 27, 13, 90),
                place(&s, "A0", 2, 7, 90),
                place(&s, "A1", 2, 11, 90),
                place(&s, "A0", 3, 2, 90),
                json!({"kind":"insert_band","axis":"x","at":0,"count":3}),
                json!({"kind":"delete_band","axis":"x","at":0,"count":3}),
                json!({"kind":"delete_band","axis":"x","at":3,"count":1}),
                json!({"kind":"delete_band","axis":"x","at":24,"count":1}),
            ];
            for (i, action) in actions.into_iter().enumerate() {
                let input = learning.observe(&s).unwrap();
                macro_rules! size { ($($field:ident),*) => { $(numerical_bytes += std::mem::size_of_val(input.$field.as_slice());)* }; }
                size!(
                    node_static,
                    node_state,
                    edge_index,
                    edge_static,
                    edge_state,
                    path_cells,
                    path_edges,
                    global_features,
                    action_features,
                    action_references,
                    root_meta,
                    kind_log_counts,
                    root_invalid,
                    port_features
                );
                let origin = [
                    world_anchor.x - s.state.anchors[anchor].x,
                    world_anchor.y - s.state.anchors[anchor].y,
                ];
                let mut local = action.clone();
                if action["kind"] == "place" {
                    local["x"] = json!(action["x"].as_i64().unwrap() - origin[0]);
                    local["y"] = json!(action["y"].as_i64().unwrap() - origin[1]);
                } else {
                    let axis = usize::from(action["axis"] == "y");
                    local["at"] = json!(action["at"].as_i64().unwrap() - origin[axis]);
                }
                let mut outcome = step(&mut s, local);
                if outcome["status"] != "rejected" && action["kind"] != "place" {
                    let coordinate = if action["axis"] == "x" {
                        &mut world_anchor.x
                    } else {
                        &mut world_anchor.y
                    };
                    if *coordinate >= action["at"].as_i64().unwrap() {
                        *coordinate += action["count"].as_i64().unwrap()
                            * if action["kind"] == "insert_band" {
                                1
                            } else {
                                -1
                            };
                    }
                }
                outcome.as_object_mut().unwrap().remove("seconds");
                rows.push(json!({"input":identity::digest(&input),"outcome":outcome,"state":identity::digest(&s.state),"best":identity::digest(&s.best)}));
                if rows.len() == 2 && round == 0 && i == 1 {
                    std::fs::write(
                        output.join("legacy-checkpoint.json"),
                        serde_json::to_vec_pretty(&s.checkpoint()).unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    std::fs::write(output.join("rows.json"), serde_json::to_vec(&rows).unwrap()).unwrap();
    println!(
        "{}",
        json!({"seconds":elapsed,"steps":rows.len(),"rows_sha256":identity::digest(&rows),"native_numerical_payload_bytes":numerical_bytes,"mean_payload_bytes":numerical_bytes/rows.len()})
    );
}
