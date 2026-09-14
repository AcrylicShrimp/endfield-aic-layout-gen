#![cfg(feature = "learning")]
//! Actual-input CLI lifecycle checks; kept outside normal collection execution.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn invoke(
    config: &Path,
    seed: &Path,
    output: &Path,
    resume: Option<&Path>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aic-cli"));
    command
        .args(["learning", "collect", "--config"])
        .arg(config)
        .arg("--seed")
        .arg(seed)
        .arg("--output")
        .arg(output)
        .args(["--chunks", "1", "--workers", "2"]);
    if let Some(path) = resume {
        command.arg("--resume").arg(path);
    }
    command.output().unwrap()
}
fn success(output: std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires AIC_COLLECTION_CONFIG, AIC_LEARNER_SEED and CUDA"]
fn collection_resume_skips_initial_state_and_failure_preserves_partial_inputs() {
    let source = PathBuf::from(std::env::var("AIC_COLLECTION_CONFIG").unwrap());
    let seed_source = PathBuf::from(std::env::var("AIC_LEARNER_SEED").unwrap());
    let root =
        std::env::temp_dir().join(format!("aic-collection-lifecycle-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let config = root.join("config.json");
    let seed = root.join("seed.json");
    let mut value = read(&source);
    for e in value["episodes"].as_array_mut().unwrap() {
        for field in ["request", "original"] {
            e[field]["path"] = source
                .parent()
                .unwrap()
                .join(e[field]["path"].as_str().unwrap())
                .to_string_lossy()
                .into_owned()
                .into();
        }
    }
    write(&config, &value);
    let seed_value = read(&seed_source);
    write(&seed, &seed_value);
    for field in ["state", "replay"] {
        let name = seed_value[field]["path"].as_str().unwrap();
        fs::copy(seed_source.parent().unwrap().join(name), root.join(name)).unwrap();
    }
    let prefix = root.join("prefix");
    success(invoke(&config, &seed, &prefix, None));
    let chunk = prefix.join("chunk-0000");
    assert_eq!(fs::read_dir(&chunk).unwrap().count(), 1);
    assert!(chunk.join("evidence.json").is_file());
    let state = root.join(seed_value["state"]["path"].as_str().unwrap());
    fs::remove_file(&state).unwrap();
    let resumed = root.join("resumed");
    success(invoke(
        &config,
        &seed,
        &resumed,
        Some(&prefix.join("checkpoint")),
    ));
    let report = read(&resumed.join("report.json"));
    assert_eq!(report["completed_chunks"], 2);
    assert_eq!(
        report["records"][0]["true_terminals"],
        value["episodes"].as_array().unwrap().len()
    );
    fs::copy(
        seed_source
            .parent()
            .unwrap()
            .join(seed_value["state"]["path"].as_str().unwrap()),
        &state,
    )
    .unwrap();
    let mut reserved = value.clone();
    reserved["reserved_geometry"]
        .as_array_mut()
        .unwrap()
        .push(value["episodes"][0]["geometry_sha256"].clone());
    write(&config, &reserved);
    let result = invoke(&config, &seed, &root.join("reserved"), None);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("collection-reservation"));
    let mut corrupt = value.clone();
    corrupt["episodes"][0]["request"]["sha256"] = "changed".into();
    write(&config, &corrupt);
    let result = invoke(&config, &seed, &root.join("corrupt-input"), None);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("collection-input-hash"));
    value["max_observation_bytes"] = 1.into();
    write(&config, &value);
    let failed = root.join("failed");
    let result = invoke(&config, &seed, &failed, None);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("collection-memory"));
    assert!(!failed.join("checkpoint").exists());
    let failure = read(&failed.join("failure.json"));
    let n = value["episodes"].as_array().unwrap().len();
    assert_eq!(failure["samples"], n);
    assert_eq!(failure["snapshots"].as_array().unwrap().len(), n);
    assert_eq!(
        fs::read_to_string(failed.join("partial.jsonl"))
            .unwrap()
            .lines()
            .count(),
        2 * n
    );
    assert!(failed.join("partial-learner.safetensors").is_file());
    // Preserve failure outputs when assertions fail; successful fixtures are disposable.
    fs::remove_dir_all(root).unwrap();
}
