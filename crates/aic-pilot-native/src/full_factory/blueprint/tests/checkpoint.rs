use super::*;

#[test]
fn checkpoint_retains_pending_draft_improved_best_and_elapsed_charge() {
    let mut s = session();
    step(
        &mut s,
        json!({"kind": "pose", "instance": "C1", "x": 27, "y": 13, "rotation": 90}),
    );
    step(&mut s, json!({"kind": "repair_all"}));
    assert_eq!(
        step(&mut s, json!({"kind": "commit"}))["status"],
        "improved"
    );
    step(
        &mut s,
        json!({"kind": "pose", "instance": "A0", "x": 2, "y": 7, "rotation": 90}),
    );
    let before = s.observation().unwrap();
    let saved = s.checkpoint();
    let (_, catalogs, _) = fixture();
    let mut restored = Session::from_checkpoint(&saved.to_string(), Arc::new(catalogs)).unwrap();
    let after = restored.observation().unwrap();
    for key in [
        "revision",
        "attempts",
        "blueprint",
        "blueprint_sha256",
        "facilities",
        "nodes",
        "edges",
        "conflicts",
        "best_metrics",
        "draft_factory_validated",
    ] {
        assert_eq!(before[key], after[key], "{key}");
    }
    assert!(
        restored.checkpoint()["elapsed_ns"].as_u64().unwrap()
            >= saved["elapsed_ns"].as_u64().unwrap()
    );
    assert_eq!(
        step(&mut restored, json!({"kind": "restore_best"}))["status"],
        "draft_accepted"
    );
    assert_eq!(restored.finish()["best_metrics"]["score"][0], 595);
}

#[test]
fn checkpoint_tampering_cannot_change_topology_or_claim_invalid_best() {
    let mut s = session();
    step(
        &mut s,
        json!({"kind": "pose", "instance": "A0", "x": 2, "y": 7, "rotation": 90}),
    );
    let saved = s.checkpoint();
    let (_, catalogs, _) = fixture();
    let catalogs = Arc::new(catalogs);
    for change in [
        "recipe",
        "dimensions",
        "hash",
        "catalog",
        "validated",
        "best",
        "endpoint",
        "counters",
    ] {
        let mut changed = saved.clone();
        match change {
            "recipe" => changed["draft"]["placements"][0]["recipe"] = json!("changed"),
            "dimensions" => changed["draft"]["placements"][0]["width"] = json!(1),
            "hash" => changed["blueprint_sha256"] = json!("foreign"),
            "catalog" => changed["catalog_sha256"] = json!("foreign"),
            "validated" => changed["draft_validated"] = json!(true),
            "best" => changed["best_draft"] = changed["draft"].clone(),
            "endpoint" => changed["best_draft"]["paths"][0][0]["x"] = json!(999),
            "counters" => changed["revision"] = json!(999),
            _ => unreachable!(),
        }
        assert!(
            Session::from_checkpoint(&changed.to_string(), catalogs.clone()).is_err(),
            "{change}"
        );
    }
}
