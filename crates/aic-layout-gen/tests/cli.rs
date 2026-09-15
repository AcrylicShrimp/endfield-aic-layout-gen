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
    oversized["route_work_per_call"] = serde_json::json!(usize::MAX);
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
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("aic-layout-gen.config"));
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
