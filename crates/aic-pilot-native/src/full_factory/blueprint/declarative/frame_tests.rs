use super::tests::{place, session, step};
use super::*;

fn shifted(mut witness: Witness, dx: i64, dy: i64) -> Witness {
    let shift = |p: &mut WorldGridPosition| {
        p.x += dx;
        p.y += dy;
    };
    for p in &mut witness.placements {
        p.x += dx;
        p.y += dy;
    }
    for n in &mut witness.transport_networks {
        for p in &mut n.cells {
            shift(p);
        }
        for s in &mut n.segments {
            shift(&mut s.from);
            shift(&mut s.to);
        }
        for t in &mut n.terminals {
            shift(&mut t.position);
        }
    }
    for c in &mut witness.logistics_components {
        shift(&mut c.position);
    }
    witness
}

fn options() -> learning::Options {
    serde_json::from_value(json!({"translation_steps":[-2,-1,1,2],"band_counts":[1,2],"masking":"structural-bands-v1"})).unwrap()
}
fn input(s: &Session, learning: &mut learning::Learning) -> Value {
    let input = learning.observe(s).unwrap();
    json!(input)
}
fn outcome(s: &mut Session, action: Value) -> Value {
    let mut result = step(s, action);
    result.as_object_mut().unwrap().remove("seconds");
    result
}

#[test]
fn complete_pending_domains_actions_work_and_paths_ignore_global_translation() {
    let base = session();
    for offset in [
        (0, 0),
        (100, 200),
        (-100, -200),
        (i64::MIN + 1024, i64::MIN + 1024),
        (i64::MAX - 1024, i64::MAX - 1024),
    ] {
        let raw = shifted(base.original.clone(), offset.0, offset.1);
        let mut b = Session::new(
            base.request.clone(),
            raw.clone(),
            base.catalogs.clone(),
            base.config.clone(),
        )
        .unwrap();
        let mut a = session();
        assert_eq!(json!(b.original), json!(raw));
        assert_eq!(a.state, b.state);
        assert_eq!(a.blueprint_sha256, b.blueprint_sha256);
        assert_eq!(a.best_metrics.score, b.best_metrics.score);
        if offset != (0, 0) {
            assert_ne!(
                a.checkpoint()["input_sha256"],
                b.checkpoint()["input_sha256"]
            );
        }
        let mut la = learning::Learning::new(&a, options()).unwrap();
        let mut lb = learning::Learning::new(&b, options()).unwrap();
        let mut saw_pending = false;
        let mut reassigned_port = false;
        for turn in 0..16 {
            assert_eq!(
                input(&a, &mut la),
                input(&b, &mut lb),
                "offset={offset:?}, turn={turn}"
            );
            // Local destinations include negative axes, rotations, port reassignment,
            // valid/rejected bands, and an unchanged complete mapping.
            let mut action = match turn % 8 {
                0 => place(&a, "A0", 2, 7, 90),
                1 => place(&a, "A1", 2, 11, 90),
                2 => place(&a, "A0", -2, 2, 90),
                3 => place(&a, "C1", 27, 13, 180),
                4 => json!({"kind":"insert_band","axis":"x","at":24,"count":2}),
                5 => json!({"kind":"delete_band","axis":"y","at":0,"count":1}),
                6 => place(&a, "A0", 3, 2, 90),
                _ => {
                    let p = &a.state.geometry.placements[0];
                    place(&a, &p.instance, p.x, p.y, p.rotation)
                }
            };
            if turn == 3 {
                let node_id = action["ports"][0]["node"].as_u64().unwrap() as usize;
                let node = &a.blueprint.nodes[node_id];
                let terminal = node.terminal.as_ref().unwrap();
                let owner = a
                    .state
                    .geometry
                    .placements
                    .iter()
                    .find(|p| p.instance == "C1")
                    .unwrap();
                let definition = a
                    .catalogs
                    .data
                    .facilities
                    .facility(&owner.facility)
                    .unwrap();
                if let Some(other) = definition.ports.iter().find(|port| {
                    port.direction == terminal.direction
                        && json!(port.transport) == a.blueprint.networks[node.network]["transport"]
                        && !action["ports"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|p| p["port"] == port.id)
                }) {
                    action["ports"][0]["port"] = json!(other.id);
                    reassigned_port = true;
                }
            }
            assert_eq!(
                outcome(&mut a, action.clone()),
                outcome(&mut b, action),
                "offset={offset:?}, turn={turn}"
            );
            assert_eq!(a.state, b.state);
            assert_eq!(json!(&a.best), json!(&b.best));
            saw_pending |= !a.validated;
        }
        assert!(saw_pending);
        assert!(reassigned_port);
    }
}

#[test]
fn negative_candidates_are_exact_and_consumed_handles_stay_invalid() {
    let s = session();
    let mut runtime = learning::runtime::Runtime::open(
        s.request.clone(),
        s.original.clone(),
        s.catalogs.clone(),
        s.config.clone(),
        options(),
    )
    .unwrap();
    let observed = runtime.observe().unwrap();
    let scale = frame::from_state(&s.blueprint, &s.state, &s.catalogs.data)
        .unwrap()
        .scale();
    let index = observed
        .input
        .root_meta
        .iter()
        .enumerate()
        .find(|(i, meta)| {
            if meta[0] != 0 {
                return false;
            }
            let p = &s.state.geometry.placements[meta[1] as usize];
            let features = observed.input.action_features[*i];
            p.x as f64 + features[3] as f64 * scale < -0.5
                || p.y as f64 + features[4] as f64 * scale < -0.5
        })
        .unwrap()
        .0;
    let owner = observed.input.root_meta[index][1] as usize;
    let ports: Vec<_> = observed.domain.groups[owner]
        .iter()
        .map(|g| {
            let node = g.graph_node - s.state.geometry.placements.len();
            g.ports
                .iter()
                .position(|p| Some(p) == s.state.ports[node].as_ref())
                .unwrap()
        })
        .collect();
    let result = runtime.step(observed.handle, index, &ports).unwrap();
    assert!(
        result["action"]["x"].as_i64().unwrap() < 0 || result["action"]["y"].as_i64().unwrap() < 0
    );
    assert_ne!(result["outcome"]["status"], "rejected", "{result}");
    assert!(runtime.step(observed.handle, index, &ports).is_err());
    let state: State = serde_json::from_value(runtime.checkpoint()["state"].clone()).unwrap();
    assert_eq!(
        frame::from_state(&s.blueprint, &state, &s.catalogs.data)
            .unwrap()
            .origin,
        [0, 0]
    );
}

#[test]
fn reference_anchors_do_not_change_physical_extents_or_span_guards() {
    let s = session();
    let initial = frame::from_state(&s.blueprint, &s.state, &s.catalogs.data).unwrap();
    let mut state = s.state.clone();
    for node in &s.blueprint.nodes {
        if matches!(
            node.terminal.as_ref().map(|t| &t.endpoint),
            Some(TransportNetworkEndpoint::Facility { .. })
        ) {
            state.anchors[node.id] = Point {
                x: -10_000_000,
                y: 10_000_000,
            };
        }
    }
    s.check_state(&state).unwrap();
    let changed = frame::from_state(&s.blueprint, &state, &s.catalogs.data).unwrap();
    assert_eq!(
        (initial.origin, initial.width, initial.height),
        (changed.origin, changed.width, changed.height)
    );
}

#[test]
fn old_absolute_decision_is_rejected_and_raw_input_overflow_is_bounded() {
    let mut s = session();
    let action = place(&s, "A0", 2, 7, 90);
    let before = s.state.clone();
    for schema in [None, Some("declarative-edit-v1")] {
        let mut envelope =
            json!({"blueprint_sha256":s.blueprint_sha256,"revision":s.revision,"action":action});
        if let Some(schema) = schema {
            envelope["schema"] = json!(schema);
        }
        let decision = serde_json::from_value(envelope)
            .map_err(|e| error("action-schema", e.to_string(), "action"));
        let result = s.step(|| decision, None);
        assert_eq!(result["status"], "rejected");
        assert_eq!(s.state, before);
    }
    let mut raw = s.original.clone();
    raw.placements[0].x = i64::MAX;
    assert!(Session::new(s.request.clone(), raw, s.catalogs.clone(), s.config.clone()).is_err());
    let action = place(&s, "A0", i64::MIN, i64::MAX, 90);
    assert_eq!(step(&mut s, action)["status"], "rejected");
    assert_eq!(s.state, before);
}

#[test]
fn physical_v2_pending_snapshot_converts_without_regenerating_saved_state() {
    let s = session();
    let raw = shifted(s.original.clone(), 100, 200);
    let (bp, mut geometry) = topology::compile(&raw).unwrap();
    geometry.paths[0] = None;
    let pending = State {
        geometry,
        ports: s.state.ports.clone(),
        anchors: bp.nodes.iter().map(|n| n.initial_position).collect(),
        components: bp.components.iter().map(|c| c.position.clone()).collect(),
    };
    let complete = State {
        geometry: topology::compile(&raw).unwrap().1,
        ..pending.clone()
    };
    let mut saved = s.checkpoint();
    saved["schema"] = json!("declarative-edit-checkpoint-v2");
    saved["config"]["schema"] = json!("declarative-edit-v1");
    saved["original"] = json!(&raw);
    saved["input_sha256"] = json!(identity::digest(
        &json!({"request":s.request,"original":raw})
    ));
    saved["blueprint_sha256"] = json!(identity::digest(&bp));
    saved["state"] = json!(pending);
    saved["best_state"] = json!(complete);
    saved["attempts"] = json!(7);
    saved["revision"] = json!(3);
    saved["validated"] = json!(false);
    saved["work"] =
        json!({"route_expansions":13,"preflight_expansions":5,"route_calls":2,"validations":1});
    let restored = Session::from_checkpoint(&saved.to_string(), s.catalogs.clone()).unwrap();
    assert_eq!(restored.attempts, 7);
    assert_eq!(restored.revision, 3);
    assert!(!restored.validated);
    assert!(restored.state.geometry.paths[0].is_none());
    assert_eq!(json!(&restored.work), saved["work"]);
    assert_eq!(restored.best_state, s.state);
    for i in 1..restored.state.geometry.paths.len() {
        assert_eq!(restored.state.geometry.paths[i], s.state.geometry.paths[i]);
    }
    assert_eq!(
        restored.checkpoint()["schema"],
        "declarative-edit-checkpoint-v3"
    );
    let again =
        Session::from_checkpoint(&restored.checkpoint().to_string(), s.catalogs.clone()).unwrap();
    assert_eq!(again.state, restored.state);
    assert_eq!(again.best_state, restored.best_state);
    saved["blueprint_sha256"] = json!("changed");
    assert!(Session::from_checkpoint(&saved.to_string(), s.catalogs.clone()).is_err());
}

#[test]
fn routing_can_detour_above_the_local_origin_and_keep_zero_length_edges() {
    let s = session();
    let mut bp = s.blueprint.clone();
    bp.nodes = [Point { x: 0, y: 0 }, Point { x: 2, y: 0 }]
        .into_iter()
        .enumerate()
        .map(|(id, position)| Node {
            id,
            network: 0,
            initial_position: position,
            terminal: None,
            components: vec![],
        })
        .collect();
    let mut edge = bp.edges[0].clone();
    edge.id = 0;
    edge.network = 0;
    edge.source = 0;
    edge.target = 1;
    edge.first = None;
    edge.last = None;
    edge.zero_length = false;
    bp.edges = vec![edge.clone()];
    let mut obstacle = bp.components[0].clone();
    obstacle.position = WorldGridPosition { x: 1, y: 0 };
    bp.networks[0]["transport"] = json!(obstacle.transport);
    bp.components = vec![obstacle];
    let mut body = s.state.geometry.placements[0].clone();
    body.x = -1;
    body.y = 1;
    let mut draft = Draft {
        placements: vec![body],
        paths: vec![None],
    };
    let route = geometry::repair(&bp, &draft, &s.catalogs.data, &edge, 10_000, 1).unwrap();
    let cells = route.cells.unwrap();
    assert!(cells.iter().any(|p| p.y < 0), "{cells:?}");
    geometry::check_path(&bp, &draft, &s.catalogs.data, &edge, &cells).unwrap();
    // The same guard permits a single-cell edge between compatible anchors.
    bp.nodes[1].initial_position = bp.nodes[0].initial_position;
    for node in &mut bp.nodes {
        node.components = vec!["shared".into()];
    }
    bp.components.clear();
    draft.placements.clear();
    edge.zero_length = true;
    bp.edges[0] = edge.clone();
    let route = geometry::repair(&bp, &draft, &s.catalogs.data, &edge, 10_000, 1).unwrap();
    assert_eq!(route.cells.as_ref().unwrap(), &[Point { x: 0, y: 0 }]);
    geometry::check_path(
        &bp,
        &draft,
        &s.catalogs.data,
        &edge,
        route.cells.as_ref().unwrap(),
    )
    .unwrap();
}

#[test]
fn larger_components_remain_rejected_by_catalog_admission() {
    let s = session();
    let mut catalog = s.catalogs.data.components.catalog().clone();
    catalog.components[0].footprint.width = 2;
    let rejection =
        aic_data::logistics::ValidatedLogisticsComponentCatalog::try_from_catalog(catalog)
            .unwrap_err();
    assert!(
        rejection
            .diagnostics
            .iter()
            .any(|d| d.code == "unsupported-logistics-component-footprint")
    );
}

/// Explicit evidence import; the historical file is retained outside Git.
#[test]
#[ignore]
fn retained_physical_checkpoint_conversion_and_payload_size() {
    let path = std::env::var("AIC_LEGACY_CHECKPOINT").expect("retained physical checkpoint path");
    let saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let s = session();
    let restored = Session::from_checkpoint(&saved.to_string(), s.catalogs.clone()).unwrap();
    let mut expected: State = serde_json::from_value(saved["state"].clone()).unwrap();
    frame::canonicalize(&restored.blueprint, &mut expected, &restored.catalogs.data).unwrap();
    assert_eq!(restored.state, expected);
    assert_eq!(json!(&restored.work), saved["work"]);
    assert_eq!(json!(restored.attempts), saved["attempts"]);
    assert_eq!(json!(restored.revision), saved["revision"]);
    assert_eq!(json!(restored.validated), saved["validated"]);
    assert_eq!(restored.checkpoint()["input_sha256"], saved["input_sha256"]);
    let mut learning = learning::Learning::new(&restored, options()).unwrap();
    let input = learning.observe(&restored).unwrap();
    let mut payload = 0;
    macro_rules! size { ($($field:ident),*) => { $(payload += std::mem::size_of_val(input.$field.as_slice());)* }; }
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
    println!(
        "{}",
        json!({"actual_legacy_conversion":"passed", "pending":input.pending,"native_numerical_payload_bytes":payload,"globals":input.global_features.len(),"roots":input.root_meta.len(),"retained_work":restored.work})
    );
}
