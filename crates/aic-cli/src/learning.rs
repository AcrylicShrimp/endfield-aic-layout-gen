//! Argument parsing only; implementation lives in the learning/backend crates.
use aic_learning::{update, verify};
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum LearningCommand {
    /// Collect actual native rollouts and verify one PPO/SIL minibatch per chunk.
    VerifyCollection {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        kernel_input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
        #[arg(long, default_value_t = 1)]
        chunks: usize,
        #[arg(long)]
        resume: Option<PathBuf>,
        #[arg(long)]
        workers: Option<usize>,
    },
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
    let stage = if matches!(&command, LearningCommand::VerifyCollection { .. }) {
        "native-collection"
    } else {
        "numerical-kernel"
    };
    let entity = match &command {
        LearningCommand::VerifyCollection { config, .. } => config.display().to_string(),
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
        anyhow::anyhow!(serde_json::json!({"code":code,"severity":"error","stage":stage,"message":message,"entity":entity}).to_string())
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn execute(command: LearningCommand) -> Result<serde_json::Value> {
    match command {
        LearningCommand::VerifyCollection {
            config,
            kernel_input,
            output,
            device,
            chunks,
            resume,
            workers,
        } => aic_learning::collection::verify::run(
            &config,
            &kernel_input,
            &output,
            update::parse_device(&device)?,
            chunks,
            resume.as_deref(),
            workers,
        ),
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
