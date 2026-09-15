//! Argument parsing only; implementation lives in the learning/backend crates.
use aic_learning::update;
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum LearningCommand {
    /// Recompute physical reservations from verified v1 panel payloads into a new directory.
    MigratePanel {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// JSON file containing max_individualization_nodes.
        #[arg(long)]
        identity_limits: PathBuf,
        /// Directory containing the five normalized runtime catalog files.
        #[arg(long)]
        catalog_root: PathBuf,
    },
    /// Ask an active training run to save and stop at its next complete wave.
    Stop {
        #[arg(long)]
        output: PathBuf,
    },
    /// Train the native policy through full PPO epochs, replay and evaluation.
    Train {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
        /// Total completed update target; otherwise stop at the configured duration.
        #[arg(long)]
        updates: Option<u64>,
        /// Restore this output's latest completed training checkpoint.
        #[arg(long)]
        resume: bool,
        #[arg(long)]
        workers: Option<usize>,
    },
    /// Generate an immutable held-out panel with a separate seed namespace.
    PreparePanel {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        cases: usize,
        #[arg(long)]
        seed: u64,
    },
}
pub fn run(command: LearningCommand) -> Result<()> {
    let config = match &command {
        LearningCommand::MigratePanel { input: config, .. }
        | LearningCommand::Stop { output: config }
        | LearningCommand::Train { config, .. }
        | LearningCommand::PreparePanel { config, .. } => config,
    };
    let entity = config.display().to_string();
    let report = execute(command).map_err(|error| {
        if let Some(diagnostic) = error.chain().find_map(|cause| {
            let value: serde_json::Value = serde_json::from_str(&cause.to_string()).ok()?;
            (value["code"].is_string() && value["severity"].is_string()
                && value["stage"].is_string() && value["entity"].is_string()).then_some(value)
        }) {
            return anyhow::anyhow!(diagnostic.to_string());
        }
        let message = format!("{error:#}");
        let code = error.chain().find_map(|cause| {
            let text = cause.to_string();
            let prefix = text.split(':').next()?;
            (prefix.starts_with("aic-learning.") || prefix.starts_with("aic-torch.")).then(|| prefix.to_owned())
        }).unwrap_or_else(|| "aic-learning.failure".into());
        anyhow::anyhow!(serde_json::json!({"code":code,"severity":"error","stage":"native-learning","message":message,"entity":entity}).to_string())
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn execute(command: LearningCommand) -> Result<serde_json::Value> {
    match command {
        LearningCommand::MigratePanel {
            input,
            output,
            identity_limits,
            catalog_root,
        } => aic_learning::training::evaluation::migrate(
            &input,
            &output,
            &identity_limits,
            &catalog_root,
        ),
        LearningCommand::Stop { output } => {
            anyhow::ensure!(
                output.is_dir(),
                "aic-learning.stop-output: missing {}",
                output.display()
            );
            std::fs::write(output.join("STOP"), b"requested\n")?;
            Ok(
                serde_json::json!({"schema":"aic-training-stop-v1","status":"requested","output":output}),
            )
        }
        LearningCommand::Train {
            config,
            output,
            device,
            updates,
            resume,
            workers,
        } => aic_learning::training::run::run(
            &config,
            &output,
            update::parse_device(&device)?,
            updates,
            resume,
            workers,
        ),
        LearningCommand::PreparePanel {
            config,
            output,
            cases,
            seed,
        } => aic_learning::training::evaluation::prepare(&config, &output, cases, seed),
    }
}
