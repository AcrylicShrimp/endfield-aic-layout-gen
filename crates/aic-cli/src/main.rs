use std::{num::NonZeroU64, path::PathBuf, process::ExitCode, time::Duration};

use aic_data::facilities::{
    FacilityCatalogValidationReport, ValidatedFacilityCatalog, load_facility_catalog,
    validate_facility_catalog,
};
use aic_data::layouts::{
    FacilityPlacementDiagnostic, FacilityPlacementReport, FacilityPlacementRequest,
    IntegratedLayoutDiagnostic, IntegratedLayoutReport, solve_facility_placement,
    solve_integrated_layout_with_time_limit,
};
use aic_data::logistics::{
    ItemCatalogValidationReport, ValidatedItemCatalog, load_item_catalog, validate_item_catalog,
};
use aic_data::recipes::{
    FacilityInstanceWiringReport, FacilityRequirementReport, RecipeThroughputReport,
    RecipeThroughputRequest, RecipeWiringGraphReport, ThroughputDiagnostic, ValidatedRecipeBook,
    build_facility_instance_wiring, build_recipe_wiring_graph, calculate_facility_requirements,
    load_recipe_book, validate_recipe_book, validate_target_item_id, validate_throughput_request,
};
use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use serde::Serialize;

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
    /// Work with facility catalog data.
    Facilities {
        #[command(subcommand)]
        command: FacilitiesCommand,
    },
    /// Work with item transport data.
    Items {
        #[command(subcommand)]
        command: ItemsCommand,
    },
    /// Generate spatial layouts.
    Layouts {
        #[command(subcommand)]
        command: LayoutsCommand,
    },
    /// Work with recipe data.
    Recipes {
        #[command(subcommand)]
        command: RecipesCommand,
    },
}

#[derive(Debug, Subcommand)]
enum FacilitiesCommand {
    /// Load and validate a facility catalog JSON file.
    Validate {
        /// Facility catalog JSON file to validate.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ItemsCommand {
    /// Load and validate an item catalog JSON file.
    Validate {
        /// Item catalog JSON file to validate.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum LayoutsCommand {
    /// Place facility instances within a maximum grid width.
    PlaceFacilities {
        /// Recipe JSON file to load.
        #[arg(long, value_name = "FILE")]
        recipes: PathBuf,

        /// Throughput request JSON file to load.
        #[arg(long, value_name = "FILE")]
        throughput_request: PathBuf,

        /// Facility catalog JSON file to load.
        #[arg(long, value_name = "FILE")]
        facility_catalog: PathBuf,

        /// Facility placement request JSON file to load.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
    },
    /// Solve facility placement, port selection, and routing together.
    Solve {
        /// Recipe JSON file to load.
        #[arg(long, value_name = "FILE")]
        recipes: PathBuf,

        /// Throughput request JSON file to load.
        #[arg(long, value_name = "FILE")]
        throughput_request: PathBuf,

        /// Facility catalog JSON file to load.
        #[arg(long, value_name = "FILE")]
        facility_catalog: PathBuf,

        /// Item transport catalog JSON file to load.
        #[arg(long, value_name = "FILE")]
        item_catalog: PathBuf,

        /// Hard maximum layout bounds JSON file to load.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Maximum solver search time before returning the best known status.
        #[arg(long, value_name = "SECONDS", default_value = "10")]
        time_limit_seconds: NonZeroU64,
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
    /// Build a recipe-level wiring graph for a target request.
    WiringGraph {
        /// Recipe JSON file to load.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,

        /// Throughput request JSON file to load.
        #[arg(long, short, value_name = "FILE")]
        request: PathBuf,
    },
    /// Build a logical facility-instance-level wiring plan for a target request.
    InstanceWiring {
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

#[derive(Serialize)]
struct LayoutSolveReport<'a> {
    success: bool,
    bootstrap_item_options: &'a [String],
    layout: &'a IntegratedLayoutReport,
}

fn run() -> Result<CommandStatus> {
    let cli = Cli::parse();

    match cli.command {
        Command::CheckData => check_data(cli.data_dir).map(|()| CommandStatus::Success),
        Command::Facilities { command } => match command {
            FacilitiesCommand::Validate { file } => validate_facilities(file),
        },
        Command::Items { command } => match command {
            ItemsCommand::Validate { file } => validate_items(file),
        },
        Command::Layouts { command } => match command {
            LayoutsCommand::PlaceFacilities {
                recipes,
                throughput_request,
                facility_catalog,
                placement_request,
            } => place_facilities(
                recipes,
                throughput_request,
                facility_catalog,
                placement_request,
            ),
            LayoutsCommand::Solve {
                recipes,
                throughput_request,
                facility_catalog,
                item_catalog,
                placement_request,
                time_limit_seconds,
            } => solve_layout(
                recipes,
                throughput_request,
                facility_catalog,
                item_catalog,
                placement_request,
                time_limit_seconds,
            ),
        },
        Command::Recipes { command } => match command {
            RecipesCommand::Validate { file } => {
                validate_recipes(file).map(|()| CommandStatus::Success)
            }
            RecipesCommand::Graph { file, target } => {
                graph_recipes(file, target).map(|()| CommandStatus::Success)
            }
            RecipesCommand::Throughput { file, request } => throughput_recipes(file, request),
            RecipesCommand::Facilities { file, request } => facilities_recipes(file, request),
            RecipesCommand::WiringGraph { file, request } => wiring_graph_recipes(file, request),
            RecipesCommand::InstanceWiring { file, request } => {
                instance_wiring_recipes(file, request)
            }
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

fn validate_facilities(file: PathBuf) -> Result<CommandStatus> {
    let catalog = load_facility_catalog(&file)?;
    let report = validate_facility_catalog(&catalog);
    let valid = report.valid;
    write_facility_catalog_validation_report(&report)?;

    if valid {
        Ok(CommandStatus::Success)
    } else {
        Ok(CommandStatus::Failure)
    }
}

fn validate_items(file: PathBuf) -> Result<CommandStatus> {
    let catalog = load_item_catalog(&file)?;
    let report = validate_item_catalog(&catalog);
    let valid = report.valid;
    write_item_catalog_validation_report(&report)?;

    if valid {
        Ok(CommandStatus::Success)
    } else {
        Ok(CommandStatus::Failure)
    }
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

fn wiring_graph_recipes(file: PathBuf, request: PathBuf) -> Result<CommandStatus> {
    let throughput_report = calculate_throughput_report(file, request)?;
    if !throughput_report.success {
        write_throughput_report(&throughput_report)?;
        return Ok(CommandStatus::Failure);
    }

    let report = build_recipe_wiring_graph(&throughput_report);
    let success = report.success;
    write_recipe_wiring_graph_report(&report)?;

    if success {
        Ok(CommandStatus::Success)
    } else {
        Ok(CommandStatus::Failure)
    }
}

fn instance_wiring_recipes(file: PathBuf, request: PathBuf) -> Result<CommandStatus> {
    let throughput_report = calculate_throughput_report(file, request)?;
    if !throughput_report.success {
        write_throughput_report(&throughput_report)?;
        return Ok(CommandStatus::Failure);
    }

    let facility_report = calculate_facility_requirements(&throughput_report);
    if !facility_report.success {
        write_facility_requirement_report(&facility_report)?;
        return Ok(CommandStatus::Failure);
    }

    let recipe_wiring_report = build_recipe_wiring_graph(&throughput_report);
    if !recipe_wiring_report.success {
        write_recipe_wiring_graph_report(&recipe_wiring_report)?;
        return Ok(CommandStatus::Failure);
    }

    let report =
        build_facility_instance_wiring(&throughput_report, &facility_report, &recipe_wiring_report);
    let success = report.success;
    write_facility_instance_wiring_report(&report)?;

    if success {
        Ok(CommandStatus::Success)
    } else {
        Ok(CommandStatus::Failure)
    }
}

fn place_facilities(
    recipes: PathBuf,
    throughput_request: PathBuf,
    facility_catalog: PathBuf,
    placement_request: PathBuf,
) -> Result<CommandStatus> {
    let throughput_report = calculate_throughput_report(recipes, throughput_request)?;
    if !throughput_report.success {
        write_throughput_report(&throughput_report)?;
        return Ok(CommandStatus::Failure);
    }

    let facility_report = calculate_facility_requirements(&throughput_report);
    if !facility_report.success {
        write_facility_requirement_report(&facility_report)?;
        return Ok(CommandStatus::Failure);
    }

    let recipe_wiring_report = build_recipe_wiring_graph(&throughput_report);
    if !recipe_wiring_report.success {
        write_recipe_wiring_graph_report(&recipe_wiring_report)?;
        return Ok(CommandStatus::Failure);
    }

    let instance_wiring_report =
        build_facility_instance_wiring(&throughput_report, &facility_report, &recipe_wiring_report);
    if !instance_wiring_report.success {
        write_facility_instance_wiring_report(&instance_wiring_report)?;
        return Ok(CommandStatus::Failure);
    }

    let raw_catalog = load_facility_catalog(&facility_catalog)?;
    let validated_catalog = match ValidatedFacilityCatalog::try_from_catalog(raw_catalog) {
        Ok(catalog) => catalog,
        Err(report) => {
            write_facility_catalog_validation_report(&report)?;
            return Ok(CommandStatus::Failure);
        }
    };

    let request_json = std::fs::read_to_string(&placement_request).with_context(|| {
        format!(
            "failed to read facility placement request file '{}'",
            placement_request.display()
        )
    })?;
    let request = match serde_json::from_str::<FacilityPlacementRequest>(&request_json) {
        Ok(request) => request,
        Err(error) => {
            let report = FacilityPlacementReport::invalid(FacilityPlacementDiagnostic::error(
                "invalid-facility-placement-request-json",
                "/",
                None,
                error.to_string(),
            ));
            write_facility_placement_report(&report)?;
            return Ok(CommandStatus::Failure);
        }
    };

    let report = solve_facility_placement(&instance_wiring_report, &validated_catalog, &request);
    let success = report.success;
    write_facility_placement_report(&report)?;

    if success {
        Ok(CommandStatus::Success)
    } else {
        Ok(CommandStatus::Failure)
    }
}

fn solve_layout(
    recipes: PathBuf,
    throughput_request: PathBuf,
    facility_catalog: PathBuf,
    item_catalog: PathBuf,
    placement_request: PathBuf,
    time_limit_seconds: NonZeroU64,
) -> Result<CommandStatus> {
    let throughput_report = calculate_throughput_report(recipes, throughput_request)?;
    if !throughput_report.success {
        write_throughput_report(&throughput_report)?;
        return Ok(CommandStatus::Failure);
    }

    let facility_report = calculate_facility_requirements(&throughput_report);
    if !facility_report.success {
        write_facility_requirement_report(&facility_report)?;
        return Ok(CommandStatus::Failure);
    }

    let recipe_wiring_report = build_recipe_wiring_graph(&throughput_report);
    if !recipe_wiring_report.success {
        write_recipe_wiring_graph_report(&recipe_wiring_report)?;
        return Ok(CommandStatus::Failure);
    }

    let instance_wiring_report =
        build_facility_instance_wiring(&throughput_report, &facility_report, &recipe_wiring_report);
    if !instance_wiring_report.success {
        write_facility_instance_wiring_report(&instance_wiring_report)?;
        return Ok(CommandStatus::Failure);
    }

    let raw_facilities = load_facility_catalog(&facility_catalog)?;
    let facilities = match ValidatedFacilityCatalog::try_from_catalog(raw_facilities) {
        Ok(catalog) => catalog,
        Err(report) => {
            write_facility_catalog_validation_report(&report)?;
            return Ok(CommandStatus::Failure);
        }
    };
    let raw_items = load_item_catalog(&item_catalog)?;
    let items = match ValidatedItemCatalog::try_from_catalog(raw_items) {
        Ok(catalog) => catalog,
        Err(report) => {
            write_item_catalog_validation_report(&report)?;
            return Ok(CommandStatus::Failure);
        }
    };

    let request_json = std::fs::read_to_string(&placement_request).with_context(|| {
        format!(
            "failed to read facility placement request file '{}'",
            placement_request.display()
        )
    })?;
    let request = match serde_json::from_str::<FacilityPlacementRequest>(&request_json) {
        Ok(request) => request,
        Err(error) => {
            let report = IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
                "invalid-facility-placement-request-json",
                "/",
                None,
                error.to_string(),
            ));
            write_layout_solve_report(&throughput_report.bootstrap_item_options, &report)?;
            return Ok(CommandStatus::Failure);
        }
    };

    let report = solve_integrated_layout_with_time_limit(
        &instance_wiring_report,
        &facilities,
        &items,
        &request,
        Duration::from_secs(time_limit_seconds.get()),
    );
    let success = report.success;
    write_layout_solve_report(&throughput_report.bootstrap_item_options, &report)?;

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

fn write_recipe_wiring_graph_report(report: &RecipeWiringGraphReport) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), report)
        .context("failed to write recipe wiring graph report")?;
    println!();

    Ok(())
}

fn write_facility_instance_wiring_report(report: &FacilityInstanceWiringReport) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), report)
        .context("failed to write facility instance wiring report")?;
    println!();

    Ok(())
}

fn write_facility_catalog_validation_report(
    report: &FacilityCatalogValidationReport,
) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), report)
        .context("failed to write facility catalog validation report")?;
    println!();

    Ok(())
}

fn write_item_catalog_validation_report(report: &ItemCatalogValidationReport) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), report)
        .context("failed to write item catalog validation report")?;
    println!();

    Ok(())
}

fn write_facility_placement_report(report: &FacilityPlacementReport) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), report)
        .context("failed to write facility placement report")?;
    println!();

    Ok(())
}

fn write_layout_solve_report(
    bootstrap_item_options: &[String],
    layout: &IntegratedLayoutReport,
) -> Result<()> {
    let report = LayoutSolveReport {
        success: layout.success,
        bootstrap_item_options,
        layout,
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write layout solve report")?;
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_solve_report_preserves_bootstrap_options() {
        let bootstrap_item_options = vec!["seed".to_string()];
        let layout = IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
            "test-diagnostic",
            "/",
            None,
            "test diagnostic",
        ));
        let value = serde_json::to_value(LayoutSolveReport {
            success: layout.success,
            bootstrap_item_options: &bootstrap_item_options,
            layout: &layout,
        })
        .expect("layout solve report should serialize");

        assert_eq!(value["success"], false);
        assert_eq!(value["bootstrap_item_options"][0], "seed");
        assert_eq!(value["layout"]["status"], "invalid-input");
    }
}
