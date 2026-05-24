use std::{path::PathBuf, process::ExitCode};

use aic_data::recipes::load_recipe_book;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "aic",
    version,
    about = "Generate layouts for Endfield AIC production plans."
)]
struct Cli {
    /// Directory containing external facility and recipe data.
    #[arg(long, value_name = "DIR", default_value = "data")]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check whether the external data directory can be read.
    CheckData,
    /// Work with recipe data.
    Recipes {
        #[command(subcommand)]
        command: RecipesCommand,
    },
}

#[derive(Debug, Subcommand)]
enum RecipesCommand {
    /// Load and validate a recipe JSON file.
    Validate {
        /// Recipe JSON file to validate.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::CheckData => check_data(cli.data_dir),
        Command::Recipes { command } => match command {
            RecipesCommand::Validate { file } => validate_recipes(file),
        },
    }
}

fn check_data(data_dir: PathBuf) -> Result<()> {
    let metadata = std::fs::metadata(&data_dir).with_context(|| {
        format!(
            "failed to access external data directory '{}'",
            data_dir.display()
        )
    })?;

    ensure!(
        metadata.is_dir(),
        "external data path '{}' is not a directory",
        data_dir.display()
    );

    println!("external data directory: {}", data_dir.display());
    Ok(())
}

fn validate_recipes(file: PathBuf) -> Result<()> {
    let recipe_book = load_recipe_book(&file)?;
    let report = recipe_book.validate();

    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write validation report")?;
    println!();

    ensure!(report.valid, "recipe validation failed");

    Ok(())
}
