use std::fmt::Write as _;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    BottomUpFacilityGeometryExperimentReport, BottomUpRungOutcome, FacilityPlacementRequest,
    diagnose_bottom_up_facility_geometry,
};
use aic_data::localization::{ValidatedLocalizationCatalog, load_localization_catalog};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    RecipeSourcePlanRequest, ValidatedRecipeBook, build_contextual_facility_instance_wiring,
    calculate_contextual_facility_requirements, load_recipe_book,
};
use aic_data::research::{
    BenchmarkWorkloadInputs, ValidatedBenchmarkWorkloadManifest, load_benchmark_workload_manifest,
};
use anyhow::{Context, Result, ensure};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "aic-bottom-up-ladder",
    about = "Run independent bottom-up AIC solver cliff experiments."
)]
struct Args {
    /// Benchmark workload manifest JSON file.
    #[arg(long, value_name = "FILE")]
    workload: PathBuf,
    /// Root used to resolve portable input paths in the workload manifest.
    #[arg(long, value_name = "DIR", default_value = ".")]
    workspace_root: PathBuf,
    /// Hard maximum layout bounds for this experiment only.
    #[arg(long, value_name = "FILE")]
    placement_request: PathBuf,
    /// Zero-based cumulative SCC target phase.
    #[arg(long, value_name = "INDEX")]
    target_phase: usize,
    /// Exact solver wall-clock budget in milliseconds.
    #[arg(long, value_name = "MILLISECONDS")]
    time_limit_ms: u64,
    /// Directory receiving summary JSON and self-contained HTML evidence.
    #[arg(long, value_name = "DIR")]
    output_dir: PathBuf,
}

struct LoadedInputs {
    wiring: aic_data::recipes::FacilityInstanceWiringReport,
    facilities: ValidatedFacilityCatalog,
    items: ValidatedItemCatalog,
    transports: ValidatedTransportCatalog,
    components: ValidatedLogisticsComponentCatalog,
    placement_request: FacilityPlacementRequest,
    localization: Option<ValidatedLocalizationCatalog>,
}

struct WorkloadPaths {
    recipes: PathBuf,
    source_plan: PathBuf,
    facility_catalog: PathBuf,
    item_catalog: PathBuf,
    transport_catalog: PathBuf,
    logistics_component_catalog: PathBuf,
    localization_catalog: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<bool> {
    let time_limit = NonZeroU64::new(args.time_limit_ms)
        .context("bottom-up facility geometry time_limit_ms must be positive")?;
    let loaded = load_inputs(&args)?;
    let report = diagnose_bottom_up_facility_geometry(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        args.target_phase,
        Duration::from_millis(time_limit.get()),
    )
    .map_err(|layout| anyhow::anyhow!("bottom-up facility geometry failed: {layout:?}"))?;

    write_json(&args.output_dir.join("summary.json"), &report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_html(&report, loaded.localization.as_ref())?.as_bytes(),
        "bottom-up facility geometry HTML evidence",
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write bottom-up facility geometry report")?;
    println!();
    Ok(matches!(report.rung.outcome, BottomUpRungOutcome::Feasible))
}

fn load_inputs(args: &Args) -> Result<LoadedInputs> {
    let manifest = load_benchmark_workload_manifest(&args.workload)?;
    let validated = ValidatedBenchmarkWorkloadManifest::try_from_manifest(manifest)
        .map_err(|report| anyhow::anyhow!("benchmark workload validation failed: {report:?}"))?;
    let manifest = validated.manifest();
    let paths = resolve_paths(&args.workspace_root, &manifest.inputs);

    let book = ValidatedRecipeBook::try_from_recipe_book(load_recipe_book(&paths.recipes)?)
        .map_err(|report| anyhow::anyhow!("recipe validation failed: {report:?}"))?;
    let source_plan_json = std::fs::read_to_string(&paths.source_plan).with_context(|| {
        format!(
            "failed to read recipe source-plan request '{}'",
            paths.source_plan.display()
        )
    })?;
    let source_plan = serde_json::from_str::<RecipeSourcePlanRequest>(&source_plan_json)
        .with_context(|| {
            format!(
                "failed to parse recipe source-plan request '{}'",
                paths.source_plan.display()
            )
        })?;
    ensure!(
        source_plan.target.item == manifest.expected_target.item
            && source_plan.target.quantity == manifest.expected_target.quantity
            && source_plan.target.duration_ms == manifest.expected_target.duration_ms,
        "benchmark workload '{}' expected target does not match source plan '{}'",
        manifest.id,
        paths.source_plan.display()
    );
    let throughput = book.calculate_contextual_throughput(&source_plan);
    ensure!(throughput.success, "benchmark contextual throughput failed");
    let requirements = calculate_contextual_facility_requirements(&throughput);
    ensure!(
        requirements.success,
        "benchmark facility requirements failed"
    );
    let wiring = build_contextual_facility_instance_wiring(&throughput, &requirements);
    ensure!(wiring.success, "benchmark facility instance wiring failed");

    let facilities =
        ValidatedFacilityCatalog::try_from_catalog(load_facility_catalog(&paths.facility_catalog)?)
            .map_err(|report| anyhow::anyhow!("facility catalog validation failed: {report:?}"))?;
    let items = ValidatedItemCatalog::try_from_catalog(load_item_catalog(&paths.item_catalog)?)
        .map_err(|report| anyhow::anyhow!("item catalog validation failed: {report:?}"))?;
    let transports = ValidatedTransportCatalog::try_from_catalog(load_transport_catalog(
        &paths.transport_catalog,
    )?)
    .map_err(|report| anyhow::anyhow!("transport catalog validation failed: {report:?}"))?;
    let components = ValidatedLogisticsComponentCatalog::try_from_catalog(
        load_logistics_component_catalog(&paths.logistics_component_catalog)?,
    )
    .map_err(|report| anyhow::anyhow!("logistics component validation failed: {report:?}"))?;
    let placement_request_path = args.workspace_root.join(&args.placement_request);
    let placement_request_json =
        std::fs::read_to_string(&placement_request_path).with_context(|| {
            format!(
                "failed to read research placement request '{}'",
                placement_request_path.display()
            )
        })?;
    let placement_request = serde_json::from_str(&placement_request_json).with_context(|| {
        format!(
            "failed to parse research placement request '{}'",
            placement_request_path.display()
        )
    })?;
    let localization = paths
        .localization_catalog
        .as_ref()
        .map(|path| {
            ValidatedLocalizationCatalog::try_from_catalog(load_localization_catalog(path)?)
                .map_err(|report| {
                    anyhow::anyhow!("localization catalog validation failed: {report:?}")
                })
        })
        .transpose()?;

    Ok(LoadedInputs {
        wiring,
        facilities,
        items,
        transports,
        components,
        placement_request,
        localization,
    })
}

fn resolve_paths(root: &Path, inputs: &BenchmarkWorkloadInputs) -> WorkloadPaths {
    WorkloadPaths {
        recipes: root.join(&inputs.recipes),
        source_plan: root.join(&inputs.source_plan),
        facility_catalog: root.join(&inputs.facility_catalog),
        item_catalog: root.join(&inputs.item_catalog),
        transport_catalog: root.join(&inputs.transport_catalog),
        logistics_component_catalog: root.join(&inputs.logistics_component_catalog),
        localization_catalog: inputs
            .localization_catalog
            .as_ref()
            .map(|path| root.join(path)),
    }
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("failed to encode research report")?;
    write_bytes(path, &bytes, "bottom-up facility geometry JSON report")
}

fn write_bytes(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create {} directory '{}'",
                label,
                parent.display()
            )
        })?;
    }
    std::fs::write(path, bytes)
        .with_context(|| format!("failed to write {label} '{}'", path.display()))
}

fn render_html(
    report: &BottomUpFacilityGeometryExperimentReport,
    localization: Option<&ValidatedLocalizationCatalog>,
) -> Result<String> {
    let rung = &report.rung;
    let mut geometry = String::new();
    if let Some(witness) = &rung.witness {
        let cell = 36_i64;
        let width = i64::from(rung.ceiling[0]) * cell;
        let height = i64::from(rung.ceiling[1]) * cell;
        writeln!(
            geometry,
            r#"<svg viewBox="0 0 {width} {height}" role="img" aria-label="facility geometry witness">"#
        )?;
        for x in 0..=rung.ceiling[0] {
            let position = i64::from(x) * cell;
            writeln!(
                geometry,
                r#"<line x1="{position}" y1="0" x2="{position}" y2="{height}" class="grid"/>"#
            )?;
        }
        for y in 0..=rung.ceiling[1] {
            let position = i64::from(y) * cell;
            writeln!(
                geometry,
                r#"<line x1="0" y1="{position}" x2="{width}" y2="{position}" class="grid"/>"#
            )?;
        }
        for placement in &witness.placements {
            let x = placement.x * cell;
            let y = placement.y * cell;
            let facility_width = placement.width * cell;
            let facility_height = placement.height * cell;
            let label = localization
                .and_then(|catalog| catalog.facility(&placement.facility))
                .map_or(placement.facility.as_str(), |entry| {
                    entry.facility_name.as_str()
                });
            writeln!(
                geometry,
                r#"<g class="facility"><rect x="{x}" y="{y}" width="{facility_width}" height="{facility_height}"/><text x="{}" y="{}" class="name">{}</text><text x="{}" y="{}" class="instance">{} · {}°</text></g>"#,
                x + facility_width / 2,
                y + facility_height / 2 - 5,
                escape_html(label),
                x + facility_width / 2,
                y + facility_height / 2 + 18,
                escape_html(&placement.instance),
                placement.rotation,
            )?;
        }
        geometry.push_str("</svg>");
    } else {
        let message = rung
            .diagnostics
            .first()
            .map_or("No validated facility witness was found.", |diagnostic| {
                diagnostic.message.as_str()
            });
        writeln!(
            geometry,
            "<div class=\"empty\">{}</div>",
            escape_html(message)
        )?;
    }

    let model = &rung.model_complexity.variables;
    let stats = &rung.search_statistics;
    let json = escape_html(&serde_json::to_string_pretty(report)?);
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Bottom-up facility geometry</title><style>body{{margin:0;background:#07131d;color:#d5e8f5;font:14px ui-monospace,SFMono-Regular,Menlo,monospace}}header{{padding:18px 22px;border-bottom:1px solid #315066}}h1{{font-size:20px;margin:0 0 10px}}.meta{{color:#8fb2c8}}.metrics{{display:flex;gap:18px;flex-wrap:wrap;margin-top:12px}}.metric{{padding:8px 10px;border:1px solid #315066;background:#102535}}main{{padding:22px}}svg{{display:block;max-width:100%;height:auto;border:3px solid #58758a;background:#08141f}}.grid{{stroke:#193244;stroke-width:1}}.facility rect{{fill:#173f37;stroke:#65f0bd;stroke-width:2}}.facility text{{fill:#e4f6ff;text-anchor:middle;dominant-baseline:middle}}.facility .name{{font-size:17px;font-weight:700}}.facility .instance{{fill:#8fb2c8;font-size:11px}}.empty{{padding:40px;border:1px solid #ff6b9d;color:#ff6b9d}}details{{margin-top:22px}}pre{{white-space:pre-wrap;word-break:break-word;color:#8fb2c8}}.good{{color:#65f0bd}}.bad{{color:#ff6b9d}}</style></head><body><header><h1>Bottom-up Rung 0 · facility geometry only</h1><div class="meta">Phase {phase}/{total} · ceiling {width}×{height} · {facilities} facilities · formulation {formulation}</div><div class="metrics"><div class="metric {outcome_class}">outcome {outcome:?}</div><div class="metric">build {build} ms</div><div class="metric">search {search} ms</div><div class="metric">first witness {first}</div><div class="metric">variables {variables}</div><div class="metric">log₂ domain {domain:.2}</div><div class="metric">decisions {decisions}</div><div class="metric">backtracks {backtracks}</div><div class="metric">conflicts {conflicts}</div><div class="metric">propagations {propagations}</div></div></header><main>{geometry}<details><summary>Machine-readable report</summary><pre>{json}</pre></details></main></body></html>"#,
        phase = report.target_phase_index,
        total = report.total_phase_count,
        width = rung.ceiling[0],
        height = rung.ceiling[1],
        facilities = rung.facility_count,
        formulation = rung.formulation,
        outcome_class = if matches!(rung.outcome, BottomUpRungOutcome::Feasible) {
            "good"
        } else {
            "bad"
        },
        outcome = rung.outcome,
        build = rung.construction_ms,
        search = rung.search_ms,
        first = rung
            .first_witness_ms
            .map_or_else(|| "—".to_string(), |value| format!("{value} ms")),
        variables = model.total_variables,
        domain = model.log2_domain_volume,
        decisions = display_optional(stats.branch_decisions),
        backtracks = display_optional(stats.backtracks),
        conflicts = display_optional(stats.conflicts),
        propagations = display_optional(stats.solver_propagations),
    ))
}

fn display_optional(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| value.to_string())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_independent_facility_geometry_command() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--workload",
            "workload.json",
            "--placement-request",
            "placement.json",
            "--target-phase",
            "3",
            "--time-limit-ms",
            "5000",
            "--output-dir",
            "artifacts",
        ])
        .expect("independent bottom-up ladder CLI should parse");
        assert_eq!(args.target_phase, 3);
        assert_eq!(args.time_limit_ms, 5000);
        assert_eq!(args.output_dir, PathBuf::from("artifacts"));
    }
}
