//! Argument parsing only; implementation lives in the learning/backend crates.
use aic_learning::{update, verify};
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum LearningCommand {
    /// Run a bounded native PPO/SIL kernel and save complete learner state.
    VerifyKernel {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cpu")]
        device: String,
        #[arg(long, default_value_t = 4)]
        steps: u64,
        #[arg(long)]
        resume: Option<PathBuf>,
        #[arg(long,value_parser=["foreach","fused"])]
        adam_backend: Option<String>,
    },
    /// Compare model, both Adam states and independent RNGs exactly.
    CompareCheckpoints {
        #[arg(long)]
        left: PathBuf,
        #[arg(long)]
        right: PathBuf,
    },
}
pub fn run(command: LearningCommand) -> Result<()> {
    let entity = match &command {
        LearningCommand::VerifyKernel { input, .. } => input.display().to_string(),
        LearningCommand::CompareCheckpoints { left, right } => {
            format!("{} / {}", left.display(), right.display())
        }
    };
    let report = execute(command).map_err(|error| {
        let message = format!("{error:#}");
        let code = error.chain().find_map(|cause| {
            let text = cause.to_string();
            let prefix = text.split(':').next()?;
            (prefix.starts_with("aic-learning.") || prefix.starts_with("aic-torch.")).then(|| prefix.to_owned())
        }).unwrap_or_else(|| "aic-learning.failure".into());
        anyhow::anyhow!(serde_json::json!({"code":code,"severity":"error","stage":"numerical-kernel","message":message,"entity":entity}).to_string())
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn execute(command: LearningCommand) -> Result<serde_json::Value> {
    match command {
        LearningCommand::VerifyKernel {
            input,
            output,
            device,
            steps,
            resume,
            adam_backend,
        } => {
            let device = update::parse_device(&device)?;
            verify::run(
                &input,
                &output,
                device,
                steps,
                resume.as_deref(),
                adam_backend.as_deref(),
            )
        }
        LearningCommand::CompareCheckpoints { left, right } => verify::compare(&left, &right),
    }
}
