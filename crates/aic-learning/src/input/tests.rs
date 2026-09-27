//! Explicit regression on retained actual arrays; no runtime diagnostic command.
use super::*;
use crate::{
    schema::{ModelConfig, UpdateConfig},
    update::Learner,
};

#[test]
fn trained_model_catalog_and_checkpoint_identity_must_match() -> Result<()> {
    let folder = std::env::temp_dir().join(format!(
        "aic-training-artifact-identity-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder)?;
    fs::write(
        folder.join("report.json"),
        r#"{"status":"complete","catalog_sha256":"catalog-a","checkpoint_sha256":"checkpoint-a"}"#,
    )?;
    assert!(verify_training_artifact(&folder, "catalog-a", "checkpoint-a").is_ok());
    assert!(verify_training_artifact(&folder, "catalog-b", "checkpoint-a").is_err());
    assert!(verify_training_artifact(&folder, "catalog-a", "checkpoint-b").is_err());
    fs::write(folder.join("report.json"), b"not json")?;
    let error = verify_training_artifact(&folder, "catalog-a", "checkpoint-a").unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&folder.join("report.json").display().to_string())
    );
    fs::remove_file(folder.join("report.json"))?;
    assert!(verify_training_artifact(&folder, "catalog-a", "checkpoint-a").is_err());
    fs::remove_dir_all(folder)?;
    Ok(())
}

#[test]
fn generation_children_require_the_committed_parent_identity() -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "aic-generation-artifact-identity-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let generation = root.join("generation");
    let lower = generation.join("lower");
    let outer = generation.join("outer");
    fs::create_dir_all(&lower)?;
    fs::create_dir_all(&outer)?;
    let report = generation.join("report.json");
    let write_parent = |status: &str, catalog: &str, lower_digest: &str| -> Result<()> {
        fs::write(
            &report,
            serde_json::to_vec(&serde_json::json!({
                "schema": crate::hierarchical_generation::REPORT_SCHEMA,
                "status": status,
                "catalog_sha256": catalog,
                "lower_checkpoint_sha256": lower_digest,
                "outer_checkpoint_sha256": "outer-digest"
            }))?,
        )?;
        Ok(())
    };
    write_parent("complete", "catalog", "lower-digest")?;
    assert!(verify_training_artifact(&lower, "catalog", "lower-digest").is_ok());
    assert!(verify_training_artifact(&outer, "catalog", "outer-digest").is_ok());
    let wrong_role = generation.join("other");
    fs::create_dir_all(&wrong_role)?;
    assert!(verify_training_artifact(&wrong_role, "catalog", "lower-digest").is_err());

    write_parent("running", "catalog", "lower-digest")?;
    assert!(verify_training_artifact(&lower, "catalog", "lower-digest").is_err());
    write_parent("complete", "wrong-catalog", "lower-digest")?;
    assert!(verify_training_artifact(&lower, "catalog", "lower-digest").is_err());
    write_parent("complete", "catalog", "wrong-digest")?;
    assert!(verify_training_artifact(&lower, "catalog", "lower-digest").is_err());
    write_parent("complete", "catalog", "lower-digest")?;
    fs::write(
        &report,
        serde_json::to_vec(&serde_json::json!({
            "schema": "wrong-schema",
            "status": "complete",
            "catalog_sha256": "catalog",
            "lower_checkpoint_sha256": "lower-digest",
            "outer_checkpoint_sha256": "outer-digest"
        }))?,
    )?;
    assert!(verify_training_artifact(&lower, "catalog", "lower-digest").is_err());
    write_parent("complete", "catalog", "lower-digest")?;

    let direct_report = lower.join("report.json");
    fs::write(&direct_report, b"not json")?;
    let error = verify_training_artifact(&lower, "catalog", "lower-digest").unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&direct_report.display().to_string())
    );
    fs::remove_file(&direct_report)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("missing-report.json", &direct_report)?;
        let error = verify_training_artifact(&lower, "catalog", "lower-digest").unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&direct_report.display().to_string())
        );
        fs::remove_file(&direct_report)?;
        let alias = root.join("alias");
        fs::create_dir_all(&alias)?;
        fs::copy(&report, alias.join("report.json"))?;
        std::os::unix::fs::symlink(&lower, alias.join("lower"))?;
        assert!(verify_training_artifact(&alias.join("lower"), "catalog", "lower-digest").is_err());
    }
    fs::write(&report, b"not json")?;
    let error = verify_training_artifact(&lower, "catalog", "lower-digest").unwrap_err();
    assert!(error.to_string().contains(&report.display().to_string()));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
#[ignore = "requires AIC_RETAINED_COLLECTION and the recorded CUDA backend"]
fn retained_collection_updates_remain_exact() -> Result<()> {
    aic_torch::tch::set_num_threads(1);
    aic_torch::deterministic()?;
    let root = std::path::PathBuf::from(std::env::var("AIC_RETAINED_COLLECTION")?);
    for chunk in 0..2 {
        let folder = root.join(format!("chunk-{chunk:04}"));
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join("manifest.json"))?)?;
        let model: ModelConfig = serde_json::from_value(manifest["model"].clone())?;
        let update: UpdateConfig = serde_json::from_value(manifest["update"].clone())?;
        let order: Vec<String> = serde_json::from_value(manifest["parameter_order"].clone())?;
        let tensors: Tensors = Tensor::read_safetensors(folder.join("input.safetensors"))?
            .into_iter()
            .collect();
        let packed = |name: &str| {
            Packed::load(
                &tensors,
                name,
                manifest[format!("{name}_graphs")].as_i64().unwrap(),
                manifest[format!("{name}_factors")].as_i64().unwrap(),
                Device::Cuda(0),
                &model,
            )
        };
        let target = |name: &str| tensor(&tensors, name).map(|t| t.to_device(Device::Cuda(0)));
        let ppo = PpoBatch {
            packed: packed("ppo")?,
            old_logp: target("ppo.old_logp")?,
            advantages: target("ppo.advantages")?,
            targets: target("ppo.targets")?,
        };
        let replay = ReplayBatch {
            packed: packed("sil")?,
            returns: target("sil.returns")?,
            weights: target("sil.weights")?,
        };
        let mut learner = Learner::load(&model, &update, &order, &tensors, Device::Cuda(0), chunk)?;
        learner.apply(&ppo, &replay, true)?;
        let actual = learner.snapshot()?;
        let expected = Tensor::read_safetensors(folder.join("after.safetensors"))?;
        for (name, value) in expected {
            // This test applies the recorded minibatch; collection already used
            // the sampler in the original run. Every other field must be exact.
            if name != "rng.sampler" {
                assert!(actual[&name].equal(&value), "chunk {chunk}: {name}");
            }
        }
    }
    Ok(())
}
