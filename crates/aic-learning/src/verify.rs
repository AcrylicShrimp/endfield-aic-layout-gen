//! Explicit bounded numerical diagnostics, not a normal training loop.
use crate::{checkpoint, input::Input, update::Learner};
use aic_torch::{
    deterministic,
    tch::{self, Device, Tensor, no_grad},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Instant};

pub fn run(
    input_path: &Path,
    output: &Path,
    device: Device,
    steps: u64,
    resume: Option<&Path>,
    adam_backend: Option<&str>,
) -> Result<Value> {
    ensure!(
        (1..=32).contains(&steps),
        "aic-learning.probe-steps: expected 1..32 diagnostic steps"
    );
    ensure!(
        !output.exists(),
        "aic-learning.probe-output: {} already exists",
        output.display()
    );
    tch::set_num_threads(1);
    tch::set_num_interop_threads(1);
    deterministic()?;
    let began = Instant::now();
    let mut input = Input::load(input_path, device)?;
    if let Some(backend) = adam_backend {
        input.manifest.update.adam_backend = backend.into();
        input.manifest.update.validate()?;
    }
    let mut learner = if let Some(path) = resume {
        checkpoint::restore(path, &input, device)?
    } else {
        Learner::load(
            &input.manifest.model,
            &input.manifest.update,
            &input.manifest.parameter_order,
            &input.tensors,
            device,
            0,
        )?
    };
    let load_seconds = began.elapsed().as_secs_f64();
    fs::create_dir_all(output)?;
    let initial = learner.model.predict(&input.ppo)?;
    let outputs = [
        ("logp", initial.logp.detach()),
        ("entropy", initial.entropy.detach()),
        ("values", initial.values.detach()),
    ];
    Tensor::write_safetensors(
        &outputs
            .iter()
            .map(|(n, t)| (*n, t.to_device(Device::Cpu).contiguous()))
            .collect::<Vec<_>>(),
        output.join("forward.safetensors"),
    )?;
    drop(initial);
    let mut update_seconds = 0.;
    let mut step_seconds = Vec::new();
    let mut records = Vec::new();
    for index in 0..steps {
        // Exercise the existing actor-safe critic tail every third diagnostic
        // step. The production epoch/KL scheduler is not replaced by this probe.
        let actor_updated = learner.completed_steps % 3 != 2;
        let actor_before = if !actor_updated {
            Some(
                learner
                    .snapshot()?
                    .into_iter()
                    .filter(|(n, _)| {
                        n.starts_with("adam.actor.")
                            || (n.starts_with("model.") && !n.starts_with("model.critic."))
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        let compute = Instant::now();
        records.push(learner.step(&input, actor_updated)?);
        let seconds = compute.elapsed().as_secs_f64();
        update_seconds += seconds;
        step_seconds.push(seconds);
        if index == 0 {
            let gradients = learner
                .model
                .parameters
                .iter()
                .filter_map(|(name, p)| {
                    let g = p.grad();
                    g.defined()
                        .then(|| (name.clone(), g.detach().to_device(Device::Cpu).contiguous()))
                })
                .collect::<Vec<_>>();
            Tensor::write_safetensors(&gradients, output.join("first-gradients.safetensors"))?;
        }

        if let Some(before) = actor_before {
            let after = learner.snapshot()?;
            ensure!(
                before
                    .iter()
                    .all(|(n, t)| after.get(n).is_some_and(|v| t.equal(v))),
                "aic-learning.critic-tail: actor parameters or Adam state changed"
            );
        }
    }

    let persistence = Instant::now();
    let manifest = checkpoint::save(&mut learner, &input, &output.join("checkpoint"))?;
    let checkpoint_seconds = persistence.elapsed().as_secs_f64();
    let final_prediction = no_grad(|| learner.model.predict(&input.ppo))?;
    Tensor::write_safetensors(
        &[
            ("logp", final_prediction.logp.to_device(Device::Cpu)),
            ("entropy", final_prediction.entropy.to_device(Device::Cpu)),
            ("values", final_prediction.values.to_device(Device::Cpu)),
        ],
        output.join("final-forward.safetensors"),
    )?;
    let report = json!({"schema":"aic-rust-learner-kernel-report-v1","status":"completed","stage":"numerical-kernel",
        "device":checkpoint::device_name(device)?,"input_sha256":input.identity,"resumed":resume.is_some(),"completed_steps":manifest.completed_steps,
        "load_seconds":load_seconds,"update_seconds":update_seconds,"step_seconds":step_seconds,"checkpoint_seconds":checkpoint_seconds,
        "total_seconds":began.elapsed().as_secs_f64(),"records":records,"production_cutover":false});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

pub fn compare(left: &Path, right: &Path) -> Result<Value> {
    let (a, x) = checkpoint::read(left, 2 * 1024 * 1024 * 1024)?;
    let (b, y) = checkpoint::read(right, 2 * 1024 * 1024 * 1024)?;
    ensure!(
        a.input_sha256 == b.input_sha256
            && a.completed_steps == b.completed_steps
            && a.model == b.model
            && a.update == b.update
            && a.device == b.device
            && a.parameter_order == b.parameter_order,
        "aic-learning.compare-binding: checkpoint metadata differs"
    );
    ensure!(
        x.len() == y.len(),
        "aic-learning.compare-fields: checkpoint field counts differ"
    );
    for (name, tensor) in &x {
        ensure!(
            y.get(name)
                .is_some_and(|value| tensor.kind() == value.kind() && tensor.equal(value)),
            "aic-learning.compare-state: {name} differs"
        );
    }
    Ok(
        json!({"schema":"aic-rust-learner-recovery-v1","status":"passed","exact":true,"tensor_fields":x.len(),"completed_steps":a.completed_steps,
        "left":left,"right":right}),
    )
}
