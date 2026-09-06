//! Read-only geometry and recipe-margin validation; never invokes routing or exact search.
use std::fs::OpenOptions;
use std::io::{BufRead, BufWriter, Read, Write};
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
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(
    about = "Diagnostic: validate supplied covered geometry and derive missing recipe terminal rates"
)]
struct Args {
    #[arg(long, required_unless_present = "stream", conflicts_with = "stream")]
    witness: Option<PathBuf>,
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
    #[arg(long, required_unless_present = "stream", conflicts_with = "stream")]
    report_output: Option<PathBuf>,
    /// Read schema-1 JSON-line requests from stdin and write independent reports to stdout.
    #[arg(long)]
    stream: bool,
}

const MAX_STREAM_LINE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StreamRequest {
    schema_version: u32,
    id: u64,
    witness: WitnessProbeInput,
}

fn next_request(input: &mut impl BufRead) -> Result<Option<StreamRequest>> {
    let mut line = String::new();
    let count = input
        .take(MAX_STREAM_LINE_BYTES + 1)
        .read_line(&mut line)
        .context("Reading witness stream stdin")?;
    if count == 0 {
        return Ok(None);
    }
    ensure!(
        count as u64 <= MAX_STREAM_LINE_BYTES,
        "Witness stream stdin line exceeds {MAX_STREAM_LINE_BYTES} bytes"
    );
    let request: StreamRequest =
        serde_json::from_str(&line).context("Parsing witness stream stdin line")?;
    ensure!(
        request.schema_version == 1,
        "Unsupported witness stream schema"
    );
    Ok(Some(request))
}

fn run(args: Args) -> Result<bool> {
    if let Some(output) = &args.report_output {
        ensure!(
            !output.exists(),
            "Output already exists: {}",
            output.display()
        );
    }
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
    let catalog_paths = [
        &args.facilities,
        &args.recipes,
        &args.items,
        &args.transports,
        &args.components,
    ];
    let catalog_provenance = catalog_paths
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
    // Both modes use the same catalogs and the exact same independent checking entry point.
    let probe = |input: &WitnessProbeInput| {
        probe_constructive_witness(
            input,
            &facilities,
            &recipes,
            &items,
            &transports,
            &components,
        )
    };
    if args.stream {
        let mut input = std::io::stdin().lock();
        let mut output = BufWriter::new(std::io::stdout().lock());
        while let Some(request) = next_request(&mut input)? {
            let report = probe(&request.witness);
            serde_json::to_writer(
                &mut output,
                &serde_json::json!({
                    "schema_version": 1, "id": request.id, "report": report,
                    "catalog_provenance": catalog_provenance
                }),
            )?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
        return Ok(true);
    }
    let witness_path = args.witness.context("Missing --witness")?;
    let report_output = args.report_output.context("Missing --report-output")?;
    let bytes = std::fs::read(&witness_path)
        .with_context(|| format!("Reading {}", witness_path.display()))?;
    let input: WitnessProbeInput = serde_json::from_slice(&bytes)
        .with_context(|| format!("Parsing {}", witness_path.display()))?;
    let report = probe(&input);
    let valid = report.covered_geometry_validated;
    let mut value = serde_json::to_value(&report)?;
    let witness_hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut provenance = vec![serde_json::json!({"path": witness_path, "sha256": witness_hash})];
    provenance.extend(catalog_provenance);
    value["provenance"] = serde_json::json!({"inputs":provenance});
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report_output)
        .with_context(|| format!("Creating {}", report_output.display()))?;
    let mut output = BufWriter::new(file);
    serde_json::to_writer(&mut output, &value)?;
    output.write_all(b"\n")?;
    output.flush()?;
    println!(
        "{}",
        serde_json::json!({"status":report.status, "covered_geometry_validated":valid,
        "pending_recipe_terminals": if valid { Some(report.recipe_terminals.iter().filter(|t| !t.pending_rate.is_zero()).count()) } else { None },
        "factory_validated":false,"elapsed_ms":report.elapsed_ms,"report_output":report_output})
    );
    Ok(valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn request() -> serde_json::Value {
        serde_json::json!({"schema_version":1,"id":7,"witness":{
            "schema_version":1,"placements":[],"utilizations":{},
            "transport_networks":[],"logistics_components":[]
        }})
    }

    #[test]
    fn stream_frames_are_separate_and_eof_is_clean() {
        let mut input = Cursor::new(format!("{}\n{}\n", request(), request()));
        assert_eq!(next_request(&mut input).unwrap().unwrap().id, 7);
        assert_eq!(next_request(&mut input).unwrap().unwrap().id, 7);
        assert!(next_request(&mut input).unwrap().is_none());
    }

    #[test]
    fn stream_rejects_schema_unknown_fields_and_oversized_input() {
        let mut bad = request();
        bad["schema_version"] = 2.into();
        assert!(next_request(&mut Cursor::new(bad.to_string())).is_err());
        bad = request();
        bad["ignored"] = true.into();
        assert!(next_request(&mut Cursor::new(bad.to_string())).is_err());
        assert!(
            next_request(&mut Cursor::new(vec![
                b' ';
                MAX_STREAM_LINE_BYTES as usize + 1
            ]))
            .is_err()
        );
        assert!(next_request(&mut Cursor::new("not json\n")).is_err());
    }
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
