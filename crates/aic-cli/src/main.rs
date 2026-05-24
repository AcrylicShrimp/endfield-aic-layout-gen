use std::{path::PathBuf, process::ExitCode};

use aic_data::recipes::{
    ValidatedRecipeBook, load_recipe_book, validate_recipe_book, validate_target_item_id,
};
use anyhow::{Context, Result, bail, ensure};
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
    /// Resolve the recipe dependency graph for a target item.
    Graph {
        /// Recipe JSON file to load.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,

        /// Target item ID to resolve.
        #[arg(long, short, value_name = "ITEM")]
        target: String,
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
            RecipesCommand::Graph { file, target } => graph_recipes(file, target),
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
    let report = validate_recipe_book(&recipe_book);

    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write validation report")?;
    println!();

    ensure!(report.valid, "recipe validation failed");

    Ok(())
}

fn graph_recipes(file: PathBuf, target: String) -> Result<()> {
    let recipe_book = load_recipe_book(&file)?;
    validate_target_item_id(&target).context("failed to resolve recipe graph")?;

    let validated_recipe_book = match ValidatedRecipeBook::try_from_recipe_book(recipe_book) {
        Ok(validated_recipe_book) => validated_recipe_book,
        Err(report) => {
            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                .context("failed to write validation report")?;
            println!();
            bail!("recipe validation failed")
        }
    };

    let graph = validated_recipe_book
        .resolve_graph(&target)
        .context("failed to resolve recipe graph")?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &graph)
        .context("failed to write recipe graph")?;
    println!();

    Ok(())
}
