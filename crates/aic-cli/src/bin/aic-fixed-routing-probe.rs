//! Explicit fixed-placement diagnostic; not a production construction command.
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{FixedRoutingInput, collect_constructive_profile, probe_fixed_routing};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use anyhow::{Context, Result, ensure};
use clap::Parser;
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(
    about = "Diagnostic only: route internal requirements on frozen placement; never claims factory completion"
)]
struct Args {
    #[arg(long)]
    placement: PathBuf,
    #[arg(long)]
    facilities: PathBuf,
    #[arg(long)]
    items: PathBuf,
    #[arg(long)]
    transports: PathBuf,
    #[arg(long)]
    components: PathBuf,
    #[arg(long)]
    report_output: PathBuf,
    #[arg(long, default_value_t = 16)]
    search_padding: i64,
    #[arg(long, default_value_t = 64)]
    max_astar_per_connection: u64,
    #[arg(long)]
    profile: bool,
}

fn sha(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn run(args: Args) -> Result<()> {
    ensure!(
        !args.report_output.exists(),
        "Output already exists: {}",
        args.report_output.display()
    );
    let bytes = std::fs::read(&args.placement)
        .with_context(|| format!("Reading {}", args.placement.display()))?;
    let raw: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("Parsing {}", args.placement.display()))?;
    let catalog_hash = sha(std::fs::read(&args.facilities)?);
    ensure!(
        raw["provenance"]["inputs"]["facilities"]["sha256"].as_str() == Some(catalog_hash.as_str()),
        "Frozen placement {} does not match runtime facility catalog {}",
        args.placement.display(),
        args.facilities.display()
    );
    let input: FixedRoutingInput = serde_json::from_value(raw)?;
    let facilities =
        ValidatedFacilityCatalog::try_from_catalog(load_facility_catalog(&args.facilities)?)
            .map_err(|e| {
                anyhow::anyhow!("Invalid facilities {}: {e:?}", args.facilities.display())
            })?;
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
    let action = || {
        probe_fixed_routing(
            &input,
            &facilities,
            &items,
            &transports,
            &components,
            args.search_padding,
            args.max_astar_per_connection,
        )
    };
    let (result, profile) = if args.profile {
        let (result, profile) = collect_constructive_profile(action)?;
        (result, Some(profile))
    } else {
        (action(), None)
    };
    let report =
        result.map_err(|e| anyhow::anyhow!("Probe input {}: {e:?}", args.placement.display()))?;
    let connected = report.required_internal - report.unresolved_internal.len();
    let mut value = serde_json::to_value(&report)?;
    if let Some(profile) = profile {
        value["profile"] = serde_json::to_value(profile)?;
    }
    let catalogs = [
        &args.facilities,
        &args.items,
        &args.transports,
        &args.components,
    ]
    .iter()
    .map(|path| Ok(serde_json::json!({"path":path, "sha256":sha(std::fs::read(path)?)})))
    .collect::<Result<Vec<_>>>()?;
    value["provenance"] =
        serde_json::json!({"placement_sha256": sha(&bytes), "catalogs": catalogs});
    let file: File = OpenOptions::new()
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
        serde_json::json!({"status":"fixed-placement-routing-diagnostic", "connected_internal":connected,
        "required_internal":report.required_internal,"pending_external":report.unresolved_external.len(),
        "factory_validated":false,"elapsed_ms":report.elapsed_ms,"report_output":args.report_output})
    );
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run(Args::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"code":"fixed-routing-probe-failed","stage":"fixed-placement-routing","message":format!("{error:#}")})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
