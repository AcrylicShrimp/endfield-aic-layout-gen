//! Explicit regression on retained actual arrays; no runtime diagnostic command.
use super::*;
use crate::{
    schema::{ModelConfig, UpdateConfig},
    update::Learner,
};

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
