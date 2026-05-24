use std::{path::PathBuf, process::ExitCode};

use aic_data::recipes::{
    FacilityRequirementReport, RecipeThroughputReport, RecipeThroughputRequest,
    ThroughputDiagnostic, ValidatedRecipeBook, calculate_facility_requirements, load_recipe_book,
    validate_recipe_book, validate_target_item_id, validate_throughput_request,
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
    /// Calculate required recipe and item throughput for a target request.
    Throughput {
        /// Recipe JSON file to load.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,

        /// Throughput request JSON file to load.
        #[arg(long, short, value_name = "FILE")]
        request: PathBuf,
    },
    /// Calculate recipe-dedicated facility requirements for a target request.
    Facilities {
        /// Recipe JSON file to load.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,

        /// Throughput request JSON file to load.
        #[arg(long, short, value_name = "FILE")]
        request: PathBuf,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(CommandStatus::Success) => ExitCode::SUCCESS,
        Ok(CommandStatus::Failure) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

enum CommandStatus {
    Success,
    Failure,
}

fn run() -> Result<CommandStatus> {
    let cli = Cli::parse();

    match cli.command {
        Command::CheckData => check_data(cli.data_dir).map(|()| CommandStatus::Success),
        Command::Recipes { command } => match command {
            RecipesCommand::Validate { file } => {
                validate_recipes(file).map(|()| CommandStatus::Success)
            }
            RecipesCommand::Graph { file, target } => {
                graph_recipes(file, target).map(|()| CommandStatus::Success)
            }
            RecipesCommand::Throughput { file, request } => throughput_recipes(file, request),
            RecipesCommand::Facilities { file, request } => facilities_recipes(file, request),
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

fn throughput_recipes(file: PathBuf, request: PathBuf) -> Result<CommandStatus> {
    let report = calculate_throughput_report(file, request)?;
    let success = report.success;
    write_throughput_report(&report)?;

    if success {
        Ok(CommandStatus::Success)
    } else {
        Ok(CommandStatus::Failure)
    }
}

fn facilities_recipes(file: PathBuf, request: PathBuf) -> Result<CommandStatus> {
    let throughput_report = calculate_throughput_report(file, request)?;
    if !throughput_report.success {
        write_throughput_report(&throughput_report)?;
        return Ok(CommandStatus::Failure);
    }

    let report = calculate_facility_requirements(&throughput_report);
    let success = report.success;
    write_facility_requirement_report(&report)?;

    if success {
        Ok(CommandStatus::Success)
    } else {
        Ok(CommandStatus::Failure)
    }
}

fn calculate_throughput_report(file: PathBuf, request: PathBuf) -> Result<RecipeThroughputReport> {
    let recipe_book = load_recipe_book(&file)?;
    let request_json = std::fs::read_to_string(&request).with_context(|| {
        format!(
            "failed to read throughput request file '{}'",
            request.display()
        )
    })?;
    let request = match serde_json::from_str::<RecipeThroughputRequest>(&request_json) {
        Ok(request) => request,
        Err(error) => {
            let report = RecipeThroughputReport::failure(ThroughputDiagnostic::error(
                "invalid-throughput-request-json",
                "/",
                None,
                error.to_string(),
            ));
            return Ok(report);
        }
    };

    let request_diagnostics = validate_throughput_request(&request);
    if !request_diagnostics.is_empty() {
        return Ok(RecipeThroughputReport::failure_many(request_diagnostics));
    }

    let validated_recipe_book = match ValidatedRecipeBook::try_from_recipe_book(recipe_book) {
        Ok(validated_recipe_book) => validated_recipe_book,
        Err(report) => {
            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                .context("failed to write validation report")?;
            println!();
            bail!("recipe validation failed")
        }
    };

    Ok(validated_recipe_book.calculate_throughput(&request))
}

fn write_throughput_report(report: &RecipeThroughputReport) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), report)
        .context("failed to write throughput report")?;
    println!();

    Ok(())
}

fn write_facility_requirement_report(report: &FacilityRequirementReport) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), report)
        .context("failed to write facility requirement report")?;
    println!();

    Ok(())
}
