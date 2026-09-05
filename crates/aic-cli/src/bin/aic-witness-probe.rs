//! Read-only geometry and recipe-margin validation; never invokes routing or exact search.
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{WitnessProbeInput, probe_constructive_witness};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{ValidatedRecipeBook, load_recipe_book};
use anyhow::{Context, Result, ensure};
use clap::Parser;
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(
    about = "Diagnostic: validate supplied covered geometry and derive missing recipe terminal rates"
)]
struct Args {
    #[arg(long)]
    witness: PathBuf,
    #[arg(long)]
    facilities: PathBuf,
    #[arg(long)]
    recipes: PathBuf,
    #[arg(long)]
    items: PathBuf,
    #[arg(long)]
    transports: PathBuf,
    #[arg(long)]
    components: PathBuf,
    #[arg(long)]
    report_output: PathBuf,
}

fn run(args: Args) -> Result<bool> {
    ensure!(
        !args.report_output.exists(),
        "Output already exists: {}",
        args.report_output.display()
    );
    let bytes = std::fs::read(&args.witness)
        .with_context(|| format!("Reading {}", args.witness.display()))?;
    let input: WitnessProbeInput = serde_json::from_slice(&bytes)
        .with_context(|| format!("Parsing {}", args.witness.display()))?;
    let facilities =
        ValidatedFacilityCatalog::try_from_catalog(load_facility_catalog(&args.facilities)?)
            .map_err(|e| {
                anyhow::anyhow!("Invalid facilities {}: {e:?}", args.facilities.display())
            })?;
    let recipes = ValidatedRecipeBook::try_from_recipe_book(load_recipe_book(&args.recipes)?)
        .map_err(|e| anyhow::anyhow!("Invalid recipes {}: {e:?}", args.recipes.display()))?;
    let items = ValidatedItemCatalog::try_from_catalog(load_item_catalog(&args.items)?)
        .map_err(|e| anyhow::anyhow!("Invalid items {}: {e:?}", args.items.display()))?;
    let transports =
        ValidatedTransportCatalog::try_from_catalog(load_transport_catalog(&args.transports)?)
            .map_err(|e| {
                anyhow::anyhow!("Invalid transports {}: {e:?}", args.transports.display())
            })?;
    let components = ValidatedLogisticsComponentCatalog::try_from_catalog(
        load_logistics_component_catalog(&args.components)?,
    )
    .map_err(|e| anyhow::anyhow!("Invalid components {}: {e:?}", args.components.display()))?;
    let report = probe_constructive_witness(
        &input,
        &facilities,
        &recipes,
        &items,
        &transports,
        &components,
    );
    let valid = report.covered_geometry_validated;
    let mut value = serde_json::to_value(&report)?;
    let paths = [
        &args.witness,
        &args.facilities,
        &args.recipes,
        &args.items,
        &args.transports,
        &args.components,
    ];
    let provenance = paths
        .into_iter()
        .map(|p| -> Result<_> {
            let hash: String = Sha256::digest(
                std::fs::read(p).with_context(|| format!("Reading {}", p.display()))?,
            )
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
            Ok(serde_json::json!({"path": p,"sha256": hash}))
        })
        .collect::<Result<Vec<_>>>()?;
    value["provenance"] = serde_json::json!({"inputs":provenance});
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.report_output)
        .with_context(|| format!("Creating {}", args.report_output.display()))?;
    let mut output = BufWriter::new(file);
    serde_json::to_writer(&mut output, &value)?;
    output.write_all(b"\n")?;
    output.flush()?;
    println!(
        "{}",
        serde_json::json!({"status":report.status, "covered_geometry_validated":valid,
        "pending_recipe_terminals": if valid { Some(report.recipe_terminals.iter().filter(|t| !t.pending_rate.is_zero()).count()) } else { None },
        "factory_validated":false,"elapsed_ms":report.elapsed_ms,"report_output":args.report_output})
    );
    Ok(valid)
}

fn main() -> std::process::ExitCode {
    match run(Args::parse()) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::from(2),
        Err(e) => {
            eprintln!(
                "{}",
                serde_json::json!({"code":"witness-probe-failed","severity":"error","stage":"witness-validation","message":format!("{e:#}")})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
