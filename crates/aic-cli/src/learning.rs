//! Argument parsing only; implementation lives in the learning/backend crates.
use aic_learning::update;
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum LearningCommand {
    /// Collect native rollouts and apply one PPO/SIL minibatch per chunk.
    Collect {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        seed: PathBuf,
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
}
pub fn run(command: LearningCommand) -> Result<()> {
    let LearningCommand::Collect { config, .. } = &command;
    let entity = config.display().to_string();
    let report = execute(command).map_err(|error| {
        let message = format!("{error:#}");
        let code = error.chain().find_map(|cause| {
            let text = cause.to_string();
            let prefix = text.split(':').next()?;
            (prefix.starts_with("aic-learning.") || prefix.starts_with("aic-torch.")).then(|| prefix.to_owned())
        }).unwrap_or_else(|| "aic-learning.failure".into());
        anyhow::anyhow!(serde_json::json!({"code":code,"severity":"error","stage":"native-collection","message":message,"entity":entity}).to_string())
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn execute(command: LearningCommand) -> Result<serde_json::Value> {
    match command {
        LearningCommand::Collect {
            config,
            seed,
            output,
            device,
            chunks,
            resume,
            workers,
        } => aic_learning::collection::run::run(
            &config,
            &seed,
            &output,
            update::parse_device(&device)?,
            chunks,
            resume.as_deref(),
            workers,
        ),
    }
}
