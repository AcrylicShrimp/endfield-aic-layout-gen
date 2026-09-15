use super::*;

#[test]
fn migrate_actual_payloads_preserves_bytes_and_seeds_and_rejects_new_duplicates() -> Result<()> {
    use aic_pilot_native::generation::source::Source;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config =
        crate::training::config::Config::load(&repo.join("tools/native-training/training.json"))?;
    let catalogs = load_catalogs(&repo, &config.catalogs).map_err(anyhow::Error::msg)?;
    let source = Source::load(
        &repo,
        catalogs.clone(),
        config.generation.clone(),
        BTreeSet::new(),
    )
    .map_err(anyhow::Error::msg)?;
    let root = std::env::temp_dir().join(format!("aic-panel-migration-{}", std::process::id()));
    fs::create_dir(&root)?;
    let mut actual = source
        .generate(0, &root.join("generated"))
        .map_err(anyhow::Error::msg)?;
    actual.admission = None;
    let bytes = serde_json::to_vec(&actual)?;
    fs::write(root.join("legacy-input.json"), &bytes)?;
    let case = json!({"id":"case-original","product":actual.metadata["draw"]["product"],"stratum":"small","action_seed":1701,
        "geometry_sha256":legacy_digest(&actual.original),"input":{"path":"legacy-input.json","sha256":sha256(&root.join("legacy-input.json"))?}});
    let panel = root.join("legacy-panel.json");
    write(
        &panel,
        &json!({"schema":"aic-evaluation-panel-v1","cases":[case.clone()]}),
    )?;
    let panel_bytes = fs::read(&panel)?;
    let limits = &config.generation.identity_limits;
    assert!(
        Panel::load(&panel, 64 * 1024 * 1024, &catalogs, limits)
            .err()
            .unwrap()
            .to_string()
            .contains("migrate-panel")
    );
    migrate_with(&panel, &root.join("migrated"), limits, &catalogs)?;
    assert_eq!(fs::read(&panel)?, panel_bytes);
    assert_eq!(fs::read(root.join("legacy-input.json"))?, bytes);
    assert_eq!(fs::read(root.join("migrated/inputs/0000.json"))?, bytes);
    let loaded = Panel::load(
        &root.join("migrated/panel.json"),
        64 * 1024 * 1024,
        &catalogs,
        limits,
    )?;
    assert_eq!(loaded.manifest.cases[0].id, "case-original");
    assert_eq!(loaded.manifest.cases[0].action_seed, 1701);
    assert_eq!(loaded.inputs[0].metadata, actual.metadata);
    let mut duplicate_v2 = serde_json::to_value(&loaded.manifest)?;
    let mut duplicate_case = duplicate_v2["cases"][0].clone();
    duplicate_case["id"] = json!("duplicate-v2");
    duplicate_v2["cases"]
        .as_array_mut()
        .unwrap()
        .push(duplicate_case);
    write(&root.join("migrated/duplicate-panel.json"), &duplicate_v2)?;
    let err = Panel::load(
        &root.join("migrated/duplicate-panel.json"),
        64 * 1024 * 1024,
        &catalogs,
        limits,
    )
    .err()
    .unwrap()
    .to_string();
    assert!(
        err.contains("evaluation-duplicate") && err.contains("duplicate-v2"),
        "{err}"
    );
    let mut translated = serde_json::to_value(&actual)?;
    fn move_points(v: &mut Value) {
        match v {
            Value::Object(o) => {
                if let (Some(x), Some(y)) = (
                    o.get("x").and_then(Value::as_i64),
                    o.get("y").and_then(Value::as_i64),
                ) {
                    o.insert("x".into(), json!(x + 100));
                    o.insert("y".into(), json!(y - 91));
                }
                for v in o.values_mut() {
                    move_points(v);
                }
            }
            Value::Array(a) => {
                for v in a {
                    move_points(v)
                }
            }
            _ => {}
        }
    }
    move_points(&mut translated["original"]);
    let translated: Generated = serde_json::from_value(translated)?;
    write(&root.join("translated.json"), &translated)?;
    let mut other = case.clone();
    other["id"] = json!("case-translated");
    other["action_seed"] = json!(1801);
    other["geometry_sha256"] = json!(legacy_digest(&translated.original));
    assert_ne!(other["geometry_sha256"], case["geometry_sha256"]);
    other["input"] =
        json!({"path":"translated.json","sha256":sha256(&root.join("translated.json"))?});
    write(
        &panel,
        &json!({"schema":"aic-evaluation-panel-v1","cases":[case,other]}),
    )?;
    let err = migrate_with(&panel, &root.join("duplicate"), limits, &catalogs)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("case-original")
            && err.contains("case-translated")
            && err.contains("evaluation-duplicate"),
        "{err}"
    );
    assert!(!root.join("duplicate").exists());
    let err = migrate_with(
        &panel,
        &root.join("budget"),
        &IdentityLimits {
            max_individualization_nodes: 1,
        },
        &catalogs,
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("physical-identity-budget") && err.contains("case-original"),
        "{err}"
    );
    assert!(!root.join("budget").exists());
    // v2 metadata cannot certify a modified exact problem or waive a reserved root.
    let mut changed = loaded.inputs[0].clone();
    changed.admission.as_mut().unwrap().physical.exact_problem = "a".repeat(64);
    assert!(
        changed
            .admit(&catalogs, limits, "tampered", None)
            .unwrap_err()
            .contains("changed payload")
    );
    let admitted = loaded.inputs[0].admitted().map_err(anyhow::Error::msg)?;
    assert!(admitted.is_reserved(&loaded.reserved));
    let mut descendant = admitted.clone();
    descendant.physical.structural = "f".repeat(64);
    descendant.root.episode = "a-renumbered-episode".into();
    assert!(descendant.is_reserved(&loaded.reserved));
    fs::remove_dir_all(root)?;
    Ok(())
}
