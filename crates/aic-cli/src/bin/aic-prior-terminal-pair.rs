use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    FacilityPlacementRequest, PriorTerminalCompletionPortfolioReport,
    PriorTerminalPairValuePortfolioReport, diagnose_prior_terminal_completion_portfolio,
    diagnose_prior_terminal_pair_value_portfolio, render_integrated_layout_html_with_localization,
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
    name = "aic-prior-terminal-pair",
    about = "Run the exact prior-terminal port-pair diagnostic portfolio."
)]
struct Args {
    #[arg(long, value_name = "FILE")]
    workload: PathBuf,
    #[arg(long, value_name = "DIR", default_value = ".")]
    workspace_root: PathBuf,
    #[arg(long, value_name = "FILE")]
    placement_request: PathBuf,
    #[arg(long, value_name = "INDEX")]
    target_phase: usize,
    #[arg(long, value_name = "CELLS")]
    used_width: i32,
    #[arg(long, value_name = "CELLS")]
    used_height: i32,
    #[arg(long, value_name = "CELL")]
    facility_x: i32,
    #[arg(long, value_name = "CELL")]
    facility_y: i32,
    #[arg(long, value_name = "INDEX")]
    port_assignment_index: usize,
    #[arg(long, value_name = "DEGREES")]
    facility_rotation: i64,
    #[arg(long, value_name = "INDEX")]
    prior_facility_bit: usize,
    /// Stable terminal bits, for example 2,3.
    #[arg(long, value_name = "LEFT,RIGHT")]
    terminal_pair: String,
    #[arg(long, value_name = "COUNT")]
    worker_count: usize,
    #[arg(long, value_name = "MILLISECONDS")]
    prefix_case_time_limit_ms: u64,
    #[arg(long, value_name = "MILLISECONDS")]
    pair_case_time_limit_ms: u64,
    /// Expand every non-infeasible pair into complete target-facility port assignments.
    #[arg(long)]
    complete_target_ports: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    child_case_time_limit_ms: Option<u64>,
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

struct ResolvedWorkloadPaths {
    recipes: PathBuf,
    source_plan: PathBuf,
    facility_catalog: PathBuf,
    item_catalog: PathBuf,
    transport_catalog: PathBuf,
    logistics_component_catalog: PathBuf,
    localization_catalog: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let terminal_bits = parse_terminal_pair(&args.terminal_pair)?;
    let worker_count = NonZeroUsize::new(args.worker_count)
        .context("prior-terminal pair worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(args.prefix_case_time_limit_ms)
        .context("prior-terminal pair prefix_case_time_limit_ms must be positive")?;
    let pair_budget = NonZeroU64::new(args.pair_case_time_limit_ms)
        .context("prior-terminal pair pair_case_time_limit_ms must be positive")?;
    let loaded = load_inputs(&args)?;
    if args.complete_target_ports {
        run_completion(
            &args,
            terminal_bits,
            worker_count,
            prefix_budget,
            pair_budget,
            &loaded,
        )
    } else {
        run_pair(
            &args,
            terminal_bits,
            worker_count,
            prefix_budget,
            pair_budget,
            &loaded,
        )
    }
}

fn run_pair(
    args: &Args,
    terminal_bits: [usize; 2],
    worker_count: NonZeroUsize,
    prefix_budget: NonZeroU64,
    pair_budget: NonZeroU64,
    loaded: &LoadedInputs,
) -> Result<()> {
    ensure!(
        args.child_case_time_limit_ms.is_none(),
        "--child-case-time-limit-ms requires --complete-target-ports"
    );
    let report = diagnose_prior_terminal_pair_value_portfolio(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        args.target_phase,
        args.used_width,
        args.used_height,
        args.facility_x,
        args.facility_y,
        args.port_assignment_index,
        args.facility_rotation,
        args.prior_facility_bit,
        terminal_bits,
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(pair_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("prior-terminal pair diagnosis failed: {report:?}"))?;

    write_json(&args.output_dir.join("summary.json"), &report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_pair_summary(&report)?.as_bytes(),
        "prior-terminal pair summary",
    )?;
    for case in &report.cases {
        let html = render_integrated_layout_html_with_localization(
            &case.layout,
            loaded.localization.as_ref(),
        )
        .map_err(|diagnostic| {
            anyhow::anyhow!(
                "prior-terminal pair case visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &args
                .output_dir
                .join(format!("case.pair-{:03}.html", case.pair_index)),
            html.as_bytes(),
            "prior-terminal pair case",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write prior-terminal pair report")?;
    println!();
    Ok(())
}

fn run_completion(
    args: &Args,
    terminal_bits: [usize; 2],
    worker_count: NonZeroUsize,
    prefix_budget: NonZeroU64,
    pair_budget: NonZeroU64,
    loaded: &LoadedInputs,
) -> Result<()> {
    let child_budget = NonZeroU64::new(
        args.child_case_time_limit_ms
            .context("completion portfolio requires --child-case-time-limit-ms")?,
    )
    .context("prior-terminal completion child_case_time_limit_ms must be positive")?;
    let report = diagnose_prior_terminal_completion_portfolio(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        args.target_phase,
        args.used_width,
        args.used_height,
        args.facility_x,
        args.facility_y,
        args.port_assignment_index,
        args.facility_rotation,
        args.prior_facility_bit,
        terminal_bits,
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(pair_budget.get()),
        Duration::from_millis(child_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("prior-terminal completion diagnosis failed: {report:?}"))?;

    write_json(&args.output_dir.join("summary.json"), &report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_completion_summary(&report)?.as_bytes(),
        "prior-terminal completion summary",
    )?;
    for case in &report.cases {
        let html = render_integrated_layout_html_with_localization(
            &case.layout,
            loaded.localization.as_ref(),
        )
        .map_err(|diagnostic| {
            anyhow::anyhow!(
                "prior-terminal completion case visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &args
                .output_dir
                .join(format!("case.leaf-{:03}.html", case.leaf_index)),
            html.as_bytes(),
            "prior-terminal completion case",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write prior-terminal completion report")?;
    println!();
    Ok(())
}

fn load_inputs(args: &Args) -> Result<LoadedInputs> {
    let manifest = load_benchmark_workload_manifest(&args.workload)?;
    let validated = ValidatedBenchmarkWorkloadManifest::try_from_manifest(manifest)
        .map_err(|report| anyhow::anyhow!("benchmark workload validation failed: {report:?}"))?;
    let manifest = validated.manifest();
    let paths = resolve_workload_paths(&args.workspace_root, &manifest.inputs);
    let recipe_book = load_recipe_book(&paths.recipes)?;
    let book = ValidatedRecipeBook::try_from_recipe_book(recipe_book)
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
    let localization = match &paths.localization_catalog {
        Some(path) => {
            let catalog = load_localization_catalog(path).with_context(|| {
                format!(
                    "failed to load research localization catalog '{}'",
                    path.display()
                )
            })?;
            Some(
                ValidatedLocalizationCatalog::try_from_catalog(catalog).map_err(|report| {
                    anyhow::anyhow!("localization catalog validation failed: {report:?}")
                })?,
            )
        }
        None => None,
    };

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

fn resolve_workload_paths(
    workspace_root: &Path,
    inputs: &BenchmarkWorkloadInputs,
) -> ResolvedWorkloadPaths {
    ResolvedWorkloadPaths {
        recipes: workspace_root.join(&inputs.recipes),
        source_plan: workspace_root.join(&inputs.source_plan),
        facility_catalog: workspace_root.join(&inputs.facility_catalog),
        item_catalog: workspace_root.join(&inputs.item_catalog),
        transport_catalog: workspace_root.join(&inputs.transport_catalog),
        logistics_component_catalog: workspace_root.join(&inputs.logistics_component_catalog),
        localization_catalog: inputs
            .localization_catalog
            .as_ref()
            .map(|path| workspace_root.join(path)),
    }
}

fn parse_terminal_pair(value: &str) -> Result<[usize; 2]> {
    let bits = value
        .split(',')
        .map(|part| {
            part.parse::<usize>()
                .with_context(|| format!("invalid terminal bit index {part:?}"))
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        bits.len() == 2,
        "--terminal-pair requires exactly two comma-separated bit indices"
    );
    ensure!(
        bits[0] != bits[1],
        "--terminal-pair requires two distinct bit indices"
    );
    Ok([bits[0], bits[1]])
}

fn render_pair_summary(report: &PriorTerminalPairValuePortfolioReport) -> Result<String> {
    let domains = report
        .terminal_domains
        .iter()
        .map(|domain| {
            format!(
                "<li>bit {}: <code>{}</code><br>reference=<code>{}</code><br>domain=<code>{}</code></li>",
                domain.terminal_bit_index,
                domain.terminal,
                domain.reference_port,
                domain.ports.join(", ")
            )
        })
        .collect::<String>();
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let assignment = case
                .assignments
                .iter()
                .map(|assignment| format!("{} = {}", assignment.terminal, assignment.port))
                .collect::<Vec<_>>()
                .join("<br>");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td></tr>",
                case.pair_index,
                assignment,
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.search_statistics.branch_decisions,
                case.search_statistics.backtracks,
                case.search_statistics.conflicts,
                case.search_statistics.learned_clauses,
                case.search_statistics.solver_propagations,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 prior-terminal pair values</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} prior-terminal pair-value portfolio</h1><div class="meta">facility=<code>{}</code> · prior=<code>{}</code> · fixed={}x{} · coordinate={},{} · introduced assignment={} · rotation={} · pairs={} · workers={} · wall={}ms</div><h2>Complete port domains</h2><ul>{}</ul><p>feasible={} · complete infeasible={} · unknown={} · invalid={}</p><table><thead><tr><th>pair</th><th>fixed terminal values</th><th>outcome</th><th>build ms</th><th>search ms</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.partitioned_facility,
        report.prior_facility,
        report.fixed_dimensions[0],
        report.fixed_dimensions[1],
        report.fixed_coordinate[0],
        report.fixed_coordinate[1],
        report.port_assignment_index,
        report.fixed_rotation,
        report.legal_pair_count,
        report.worker_count,
        report.portfolio_wall_ms,
        domains,
        report.validated_witness_found,
        report.complete_infeasibility_proven,
        report.unknown_count,
        report.invalid_witness_count,
        rows,
        json,
    ))
}

fn render_completion_summary(report: &PriorTerminalCompletionPortfolioReport) -> Result<String> {
    let domains = report
        .completion_domains
        .iter()
        .map(|domain| {
            format!(
                "<li>bit {}: <code>{}</code><br>reference=<code>{}</code><br>domain=<code>{}</code></li>",
                domain.terminal_bit_index,
                domain.terminal,
                domain.reference_port,
                domain.ports.join(", ")
            )
        })
        .collect::<String>();
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let pair = case
                .pair_assignments
                .iter()
                .map(|assignment| assignment.port.clone())
                .collect::<Vec<_>>()
                .join(" / ");
            let completion = case
                .completion_assignments
                .iter()
                .map(|assignment| assignment.port.clone())
                .collect::<Vec<_>>()
                .join(" / ");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td></tr>",
                case.leaf_index,
                case.parent_pair_index,
                pair,
                completion,
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.search_statistics.branch_decisions,
                case.search_statistics.backtracks,
                case.search_statistics.conflicts,
                case.search_statistics.learned_clauses,
                case.search_statistics.solver_propagations,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 terminal completion</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} prior-terminal completion portfolio</h1><div class="meta">closed parents={} · expanded parents={} · completion assignments={} · coverage regions={} · workers={} · child wall={}ms · total={}ms</div><h2>Remaining terminal domains</h2><ul>{}</ul><p>feasible={} · infeasible={} · unknown={} · invalid={} · selected-state proof={}</p><table><thead><tr><th>leaf</th><th>parent</th><th>demand pair</th><th>completion</th><th>outcome</th><th>build ms</th><th>search ms</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.closed_parent_count,
        report.expanded_parent_count,
        report.completion_assignment_count_per_parent,
        report.coverage_region_count,
        report.worker_count,
        report.child_portfolio_wall_ms,
        report.total_wall_ms,
        domains,
        report.child_validated_feasible_count,
        report.child_proven_infeasible_count,
        report.child_unknown_count,
        report.child_invalid_witness_count,
        report.selected_state_infeasibility_proven,
        rows,
        json,
    ))
}

fn write_json(path: &Path, report: &impl serde::Serialize) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(report).context("failed to serialize report")?;
    write_bytes(path, &encoded, "prior-terminal pair report")
}

fn write_bytes(path: &Path, bytes: &[u8], kind: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create {kind} output directory '{}'",
                parent.display()
            )
        })?;
    }
    std::fs::write(path, bytes)
        .with_context(|| format!("failed to write {kind} output '{}'", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_distinct_terminal_bits() {
        assert_eq!(parse_terminal_pair("2,3").unwrap(), [2, 3]);
        assert!(parse_terminal_pair("2").is_err());
        assert!(parse_terminal_pair("2,2").is_err());
        assert!(parse_terminal_pair("two,3").is_err());
    }
}
