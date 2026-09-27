use std::{fs, path::Path, process::Command};

#[test]
fn cli_generates_validates_and_preserves_seed_identity_across_worker_counts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("target").join(format!(
        "layout-gen-cli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let cli = env!("CARGO_BIN_EXE_aic-layout-gen");
    let config = root.join("crates/aic-layout-gen/examples/small.json");
    for workers in [1, 2] {
        let out = dir.join(format!("workers-{workers}"));
        let result = Command::new(cli)
            .current_dir(&root)
            .arg("generate")
            .arg("--config")
            .arg(&config)
            .args([
                "--seed",
                "1000",
                "--count",
                "2",
                "--workers",
                &workers.to_string(),
            ])
            .arg("--output")
            .arg(&out)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(out.join("index.html").is_file());
        let result = Command::new(cli)
            .arg("validate")
            .arg("--layout")
            .arg(out.join("layout-00000.json"))
            .arg("--catalog-dir")
            .arg(out.join("catalogs"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["physical_validated"], true);
    }
    for i in 0..2 {
        assert_eq!(
            fs::read(dir.join(format!("workers-1/layout-{i:05}.json"))).unwrap(),
            fs::read(dir.join(format!("workers-2/layout-{i:05}.json"))).unwrap()
        );
    }
    let mut old: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("workers-1/layout-00000.json")).unwrap())
            .unwrap();
    old["schema"] = "aic-physical-layout-v2".into();
    old["solution"].as_object_mut().unwrap().remove("junctions");
    let old_path = dir.join("old-layout.json");
    fs::write(&old_path, serde_json::to_vec(&old).unwrap()).unwrap();
    let migration = Command::new(cli)
        .arg("validate")
        .arg("--layout")
        .arg(&old_path)
        .arg("--catalog-dir")
        .arg(root.join("data/game/normalized"))
        .output()
        .unwrap();
    assert_eq!(migration.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&migration.stderr)
            .contains("aic-layout-gen.source-schema-migration")
    );
    let invalid = Command::new(cli)
        .arg("generate")
        .arg("--config")
        .arg(&config)
        .args(["--seed", "1000", "--count", "0"])
        .arg("--output")
        .arg(dir.join("invalid"))
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(!dir.join("invalid").exists());
    let mut oversized: serde_json::Value =
        serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    oversized["facilities"] = serde_json::json!([8, 999]);
    let oversized_path = dir.join("oversized.json");
    fs::write(&oversized_path, serde_json::to_vec(&oversized).unwrap()).unwrap();
    let rejected = Command::new(cli)
        .current_dir(&root)
        .arg("generate")
        .arg("--config")
        .arg(oversized_path)
        .args(["--seed", "1000"])
        .arg("--output")
        .arg(dir.join("oversized"))
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("facilities"));
    assert!(!dir.join("oversized").exists());
    let existing = Command::new(cli)
        .current_dir(&root)
        .arg("generate")
        .arg("--config")
        .arg(&config)
        .args(["--seed", "1000"])
        .arg("--output")
        .arg(dir.join("workers-1"))
        .output()
        .unwrap();
    assert!(!existing.status.success());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn survey_records_equal_slots_without_persisting_layouts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("target").join(format!(
        "layout-gen-survey-cli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let mut config: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("crates/aic-layout-gen/examples/survey.json")).unwrap(),
    )
    .unwrap();
    config["facility_buckets"] = serde_json::json!([[2, 3], [4, 5]]);
    config["slots_per_bucket"] = serde_json::json!(2);
    config["workers"] = serde_json::json!(2);
    config["max_elapsed_seconds"] = serde_json::json!(60);
    let config_path = dir.join("config.json");
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let output = dir.join("output");
    let result = Command::new(env!("CARGO_BIN_EXE_aic-layout-gen"))
        .current_dir(&root)
        .arg("survey")
        .arg("--config")
        .arg(&config_path)
        .args(["--seed", "4000"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["requested"], 4);
    assert_eq!(report["completed"], 4);
    assert_eq!(report["incomplete"], 0);
    assert_eq!(report["summaries"][0]["requested"], 2);
    assert_eq!(report["summaries"][1]["requested"], 2);
    assert_eq!(report["rows"][0]["seed"], 4000);
    assert_eq!(report["rows"][3]["seed"], 4003);
    assert!(
        report["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["attempt_count"].as_u64().unwrap() <= 1
                && row["resolved"]["schema"] == "aic-layout-generator-scale-policy-v4")
    );
    assert!(!output.join("layout-00000.json").exists());
    fs::remove_dir_all(&dir).unwrap();
}
