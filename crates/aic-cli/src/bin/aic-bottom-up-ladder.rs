use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    BottomUpExperimentReport, BottomUpRootDomainSnapshot, BottomUpRotationPartitionReport,
    BottomUpRotationProvenanceReport, BottomUpRotationRootComparisonReport, BottomUpRungKind,
    BottomUpRungOutcome, BottomUpRungWitness, EndpointClearanceSchedulingPriority,
    FacilityPlacementRequest, diagnose_bottom_up_rotation_partition,
    diagnose_bottom_up_rotation_provenance, diagnose_bottom_up_rotation_root_comparison,
    diagnose_bottom_up_rung,
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
use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RungArg {
    FacilityGeometry,
    FacilityPortGeometry,
    FacilityPorts,
    FacilityPortsPropagated,
}

impl From<RungArg> for BottomUpRungKind {
    fn from(value: RungArg) -> Self {
        match value {
            RungArg::FacilityGeometry => Self::FacilityGeometry,
            RungArg::FacilityPortGeometry => Self::FacilityPortGeometry,
            RungArg::FacilityPorts => Self::FacilityPorts,
            RungArg::FacilityPortsPropagated => Self::FacilityPortsPropagated,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum EndpointClearancePriorityArg {
    High,
    Medium,
}

impl From<EndpointClearancePriorityArg> for EndpointClearanceSchedulingPriority {
    fn from(value: EndpointClearancePriorityArg) -> Self {
        match value {
            EndpointClearancePriorityArg::High => Self::High,
            EndpointClearancePriorityArg::Medium => Self::Medium,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "aic-bottom-up-ladder",
    about = "Run independent bottom-up AIC solver cliff experiments."
)]
struct Args {
    /// Semantic ladder rung to solve independently.
    #[arg(long, value_enum, default_value = "facility-geometry")]
    rung: RungArg,
    /// Scheduling priority for the propagated endpoint-clearance rung.
    #[arg(long, value_enum, default_value = "high")]
    endpoint_clearance_priority: EndpointClearancePriorityArg,
    /// Disable endpoint-clearance diagnostic counters for instrumentation-cost experiments.
    #[arg(long)]
    disable_endpoint_clearance_counters: bool,
    /// Skip scheduling when an orientation event only proves that orientation false.
    #[arg(long)]
    endpoint_clearance_false_event_filter: bool,
    /// Introduced facility ID whose directional rotations form an exact partition dimension.
    #[arg(long = "partition-facility", value_name = "INSTANCE", action = clap::ArgAction::Append)]
    partition_facilities: Vec<String>,
    /// Parallel solver instances used by an exact rotation partition.
    #[arg(long, value_name = "COUNT", default_value_t = 4)]
    partition_workers: usize,
    /// Observe parent and exact rotation children after root propagation without search decisions.
    #[arg(long)]
    partition_root_snapshot: bool,
    /// Trace the unchanged parent and exact rotation-child searches without changing decisions.
    #[arg(long)]
    partition_search_provenance: bool,
    /// Maximum number of detailed decision predicates retained per provenance case.
    #[arg(long, value_name = "COUNT", default_value_t = 256)]
    trace_detailed_decisions: usize,
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
    workload_id: String,
    workload_manifest_sha256: String,
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
        .context("bottom-up rung time_limit_ms must be positive")?;
    validate_search_settings(&args)?;
    let loaded = load_inputs(&args)?;
    if args.partition_search_provenance {
        let mut report = diagnose_bottom_up_rotation_provenance(
            &loaded.wiring,
            &loaded.facilities,
            &loaded.items,
            &loaded.transports,
            &loaded.components,
            &loaded.placement_request,
            args.target_phase,
            &args.partition_facilities,
            args.endpoint_clearance_priority.into(),
            !args.disable_endpoint_clearance_counters,
            args.endpoint_clearance_false_event_filter,
            Duration::from_millis(time_limit.get()),
            args.trace_detailed_decisions,
        )
        .map_err(|layout| {
            anyhow::anyhow!("bottom-up rotation search provenance failed: {layout:?}")
        })?;
        report.workload_id = Some(loaded.workload_id.clone());
        report.workload_manifest_sha256 = Some(loaded.workload_manifest_sha256.clone());
        write_json(&args.output_dir.join("summary.json"), &report)?;
        write_bytes(
            &args.output_dir.join("summary.html"),
            render_rotation_provenance_html(&report)?.as_bytes(),
            "bottom-up rotation search provenance HTML evidence",
        )?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
            .context("failed to write bottom-up rotation search provenance report")?;
        println!();
        return Ok(report.partition_complete && report.cases_pairwise_disjoint);
    }
    if args.partition_root_snapshot {
        let mut report = diagnose_bottom_up_rotation_root_comparison(
            &loaded.wiring,
            &loaded.facilities,
            &loaded.items,
            &loaded.transports,
            &loaded.components,
            &loaded.placement_request,
            args.target_phase,
            &args.partition_facilities,
            args.endpoint_clearance_priority.into(),
            !args.disable_endpoint_clearance_counters,
            args.endpoint_clearance_false_event_filter,
        )
        .map_err(|layout| {
            anyhow::anyhow!("bottom-up rotation root comparison failed: {layout:?}")
        })?;
        report.workload_id = Some(loaded.workload_id.clone());
        report.workload_manifest_sha256 = Some(loaded.workload_manifest_sha256.clone());
        write_json(&args.output_dir.join("summary.json"), &report)?;
        write_bytes(
            &args.output_dir.join("summary.html"),
            render_rotation_root_html(&report)?.as_bytes(),
            "bottom-up rotation root comparison HTML evidence",
        )?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
            .context("failed to write bottom-up rotation root comparison report")?;
        println!();
        return Ok(report.partition_complete && report.cases_pairwise_disjoint);
    }
    if !args.partition_facilities.is_empty() {
        let mut report = diagnose_bottom_up_rotation_partition(
            &loaded.wiring,
            &loaded.facilities,
            &loaded.items,
            &loaded.transports,
            &loaded.components,
            &loaded.placement_request,
            args.target_phase,
            &args.partition_facilities,
            args.endpoint_clearance_priority.into(),
            !args.disable_endpoint_clearance_counters,
            args.endpoint_clearance_false_event_filter,
            Duration::from_millis(time_limit.get()),
            args.partition_workers,
        )
        .map_err(|layout| anyhow::anyhow!("bottom-up rotation partition failed: {layout:?}"))?;
        report.workload_id = Some(loaded.workload_id.clone());
        report.workload_manifest_sha256 = Some(loaded.workload_manifest_sha256.clone());
        write_json(&args.output_dir.join("summary.json"), &report)?;
        write_bytes(
            &args.output_dir.join("summary.html"),
            render_rotation_partition_html(&report)?.as_bytes(),
            "bottom-up rotation partition HTML evidence",
        )?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
            .context("failed to write bottom-up rotation partition report")?;
        println!();
        return Ok(matches!(
            report.combined_outcome,
            BottomUpRungOutcome::Feasible
        ));
    }
    let mut report = diagnose_bottom_up_rung(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        args.rung.into(),
        args.endpoint_clearance_priority.into(),
        !args.disable_endpoint_clearance_counters,
        args.endpoint_clearance_false_event_filter,
        args.target_phase,
        Duration::from_millis(time_limit.get()),
    )
    .map_err(|layout| anyhow::anyhow!("bottom-up rung failed: {layout:?}"))?;
    report.workload_id = Some(loaded.workload_id.clone());
    report.workload_manifest_sha256 = Some(loaded.workload_manifest_sha256.clone());

    write_json(&args.output_dir.join("summary.json"), &report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_html(&report, loaded.localization.as_ref())?.as_bytes(),
        "bottom-up rung HTML evidence",
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write bottom-up rung report")?;
    println!();
    Ok(matches!(report.rung.outcome, BottomUpRungOutcome::Feasible))
}

fn validate_search_settings(args: &Args) -> Result<()> {
    ensure!(
        !(args.partition_root_snapshot && args.partition_search_provenance),
        "partition root snapshot and search provenance are mutually exclusive"
    );
    ensure!(
        !(args.partition_root_snapshot || args.partition_search_provenance)
            || !args.partition_facilities.is_empty(),
        "partition diagnostics require at least one --partition-facility"
    );
    ensure!(
        !args.partition_search_provenance || args.partition_facilities.len() == 1,
        "partition search provenance currently requires exactly one --partition-facility"
    );
    if !args.partition_facilities.is_empty() {
        ensure!(
            matches!(args.rung, RungArg::FacilityPortsPropagated),
            "rotation partition applies only to rung 'facility-ports-propagated'"
        );
    } else {
        ensure!(
            matches!(args.rung, RungArg::FacilityPortsPropagated)
                || (args.endpoint_clearance_priority == EndpointClearancePriorityArg::High
                    && !args.disable_endpoint_clearance_counters
                    && !args.endpoint_clearance_false_event_filter),
            "endpoint-clearance search settings apply only to rung 'facility-ports-propagated'"
        );
    }
    Ok(())
}

fn load_inputs(args: &Args) -> Result<LoadedInputs> {
    let manifest = load_benchmark_workload_manifest(&args.workload)?;
    let validated = ValidatedBenchmarkWorkloadManifest::try_from_manifest(manifest)
        .map_err(|report| anyhow::anyhow!("benchmark workload validation failed: {report:?}"))?;
    let workload_id = validated.manifest().id.clone();
    let workload_manifest_sha256 = validated.manifest_sha256().to_string();
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
        workload_id,
        workload_manifest_sha256,
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
    write_bytes(path, &bytes, "bottom-up rung JSON report")
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
    report: &BottomUpExperimentReport,
    localization: Option<&ValidatedLocalizationCatalog>,
) -> Result<String> {
    let rung = &report.rung;
    let mut geometry = String::new();
    if let Some(witness) = &rung.witness {
        let cell = 36_i64;
        render_grid(&mut geometry, rung.ceiling, cell)?;
        match witness {
            BottomUpRungWitness::FacilityGeometry { witness } => {
                for placement in &witness.placements {
                    render_facility(
                        &mut geometry,
                        placement.x,
                        placement.y,
                        placement.width,
                        placement.height,
                        &placement.facility,
                        &facility_orientation_label(placement),
                        localization,
                        cell,
                    )?;
                }
            }
            BottomUpRungWitness::FacilityPorts { witness } => {
                for placement in &witness.placements {
                    render_facility(
                        &mut geometry,
                        placement.x,
                        placement.y,
                        placement.width,
                        placement.height,
                        &placement.facility,
                        &format!("rotation {}°", placement.rotation),
                        localization,
                        cell,
                    )?;
                }
                for endpoint in &witness.endpoints {
                    let x = endpoint.connection_x * cell;
                    let y = endpoint.connection_y * cell;
                    let class = match endpoint.direction {
                        aic_data::facilities::FacilityPortDirection::Input => "input",
                        aic_data::facilities::FacilityPortDirection::Output => "output",
                    };
                    let (arm_dx, arm_dy) = direction_delta(endpoint.arm_direction);
                    let center_x = x + cell / 2;
                    let center_y = y + cell / 2;
                    let boundary_x = center_x + arm_dx * cell / 2;
                    let boundary_y = center_y + arm_dy * cell / 2;
                    let (arrow_start_x, arrow_start_y, arrow_end_x, arrow_end_y) =
                        if class == "input" {
                            (center_x, center_y, boundary_x, boundary_y)
                        } else {
                            (boundary_x, boundary_y, center_x, center_y)
                        };
                    writeln!(
                        geometry,
                        r#"<g class="endpoint {class}" data-terminal="{}" data-port="{}" data-transport="{:?}"><rect x="{x}" y="{y}" width="{cell}" height="{cell}"/><line x1="{arrow_start_x}" y1="{arrow_start_y}" x2="{arrow_end_x}" y2="{arrow_end_y}" marker-end="url(#arrow-{class})"/><circle cx="{boundary_x}" cy="{boundary_y}" r="3"/></g>"#,
                        escape_html(&endpoint.terminal),
                        escape_html(&endpoint.port),
                        endpoint.transport,
                    )?;
                }
            }
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
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Bottom-up solver rung</title><style>body{{margin:0;background:#07131d;color:#d5e8f5;font:14px ui-monospace,SFMono-Regular,Menlo,monospace}}header{{padding:18px 22px;border-bottom:1px solid #315066}}h1{{font-size:20px;margin:0 0 10px}}.meta{{color:#8fb2c8}}.metrics{{display:flex;gap:18px;flex-wrap:wrap;margin-top:12px}}.metric{{padding:8px 10px;border:1px solid #315066;background:#102535}}main{{padding:22px}}svg{{display:block;max-width:100%;height:auto;border:3px solid #58758a;background:#08141f}}.grid{{stroke:#193244;stroke-width:1}}.facility rect{{fill:#173f37;stroke:#65f0bd;stroke-width:2}}.facility text{{fill:#e4f6ff;text-anchor:middle;dominant-baseline:middle}}.facility .name{{font-size:17px;font-weight:700}}.facility .instance{{fill:#8fb2c8;font-size:11px}}.endpoint rect{{stroke-width:3}}.endpoint line{{stroke-width:4}}.endpoint circle{{stroke:none}}.endpoint.input rect{{fill:#214d43;stroke:#65f0bd}}.endpoint.input line,.endpoint.input circle{{stroke:#65f0bd;fill:#65f0bd}}.endpoint.output rect{{fill:#51293e;stroke:#ff6b9d}}.endpoint.output line,.endpoint.output circle{{stroke:#ff6b9d;fill:#ff6b9d}}.empty{{padding:40px;border:1px solid #ff6b9d;color:#ff6b9d}}details{{margin-top:22px}}pre{{white-space:pre-wrap;word-break:break-word;color:#8fb2c8}}.good{{color:#65f0bd}}.bad{{color:#ff6b9d}}</style></head><body><header><h1>Bottom-up {rung:?}</h1><div class="meta">Phase {phase}/{total} · ceiling {width}×{height} · {facilities} facilities · {terminals} terminals · formulation {formulation}</div><div class="metrics"><div class="metric {outcome_class}">outcome {outcome:?}</div><div class="metric">termination {termination:?}</div><div class="metric">build {build} ms</div><div class="metric">search {search} ms</div><div class="metric">first witness {first}</div><div class="metric">semantic log₂ upper bound {semantic_log2}</div><div class="metric">semantic decimal orders {semantic_log10}</div><div class="metric">port-choice contribution {port_log2} bits</div><div class="metric">rotation-equivalence reduction {rotation_reduction} bits</div><div class="metric">variables {variables}</div><div class="metric">model log₂ domain {domain:.2}</div><div class="metric">decisions {decisions}</div><div class="metric">backtracks {backtracks}</div><div class="metric">conflicts {conflicts}</div><div class="metric">propagations {propagations}</div></div></header><main>{geometry}<details><summary>Machine-readable report</summary><pre>{json}</pre></details></main></body></html>"#,
        rung = rung.rung,
        phase = report.target_phase_index,
        total = report.total_phase_count,
        width = rung.ceiling[0],
        height = rung.ceiling[1],
        facilities = rung.facility_count,
        terminals = rung.facility_terminal_count,
        formulation = rung.formulation,
        outcome_class = if matches!(rung.outcome, BottomUpRungOutcome::Feasible) {
            "good"
        } else {
            "bad"
        },
        outcome = rung.outcome,
        termination = rung.termination_reason,
        build = rung.construction_ms,
        search = rung.search_ms,
        first = rung
            .first_witness_ms
            .map_or_else(|| "—".to_string(), |value| format!("{value} ms")),
        semantic_log2 =
            display_optional_float(rung.search_space.semantic_assignment_upper_bound_log2),
        semantic_log10 =
            display_optional_float(rung.search_space.semantic_assignment_upper_bound_log10),
        rotation_reduction =
            display_optional_float(rung.search_space.rotation_equivalence_reduction_log2),
        port_log2 = display_optional_float(rung.search_space.facility_port_choice_upper_bound_log2),
        variables = model.total_variables,
        domain = model.log2_domain_volume,
        decisions = display_optional(stats.branch_decisions),
        backtracks = display_optional(stats.backtracks),
        conflicts = display_optional(stats.conflicts),
        propagations = display_optional(stats.solver_propagations),
    ))
}

fn render_rotation_partition_html(report: &BottomUpRotationPartitionReport) -> Result<String> {
    let mut rows = String::new();
    for case in &report.cases {
        let fixed = case
            .fixed_rotations
            .iter()
            .map(|(facility, rotation)| format!("{}={rotation}°", escape_html(facility)))
            .collect::<Vec<_>>()
            .join("<br>");
        writeln!(
            rows,
            "<tr><td>{}</td><td>{fixed}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            case.case_index,
            case.rung.outcome,
            case.rung.search_ms,
            display_optional(case.rung.search_statistics.branch_decisions),
            display_optional(case.rung.search_statistics.conflicts),
            display_optional(case.rung.search_statistics.solver_propagations),
        )?;
    }
    let domains = report
        .partitioned_rotation_domains
        .iter()
        .map(|(facility, rotations)| format!("{}={rotations:?}", escape_html(facility)))
        .collect::<Vec<_>>()
        .join("<br>");
    let json = escape_html(&serde_json::to_string_pretty(report)?);
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Bottom-up rotation partition</title><style>body{{margin:24px;background:#07131d;color:#d5e8f5;font:14px ui-monospace,SFMono-Regular,Menlo,monospace}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:16px}}.certificate{{padding:10px;border:1px solid #65f0bd;color:#65f0bd;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}details{{margin-top:20px}}pre{{white-space:pre-wrap;word-break:break-word;color:#8fb2c8}}</style></head><body><h1>Bottom-up exact rotation partition</h1><div class="meta">Phase {phase}/{total} · {facilities} cumulative facilities · {cases} cases · {workers} workers · {budget} ms/case · first feasible {first_feasible} · full wall {wall} ms</div><div class="certificate">complete={complete} · pairwise-disjoint={disjoint} · combined={combined:?} · feasible/infeasible/unknown/invalid={feasible}/{infeasible}/{unknown}/{invalid}<br>{domains}</div><table><thead><tr><th>case</th><th>fixed directional rotations</th><th>outcome</th><th>search ms</th><th>decisions</th><th>conflicts</th><th>propagations</th></tr></thead><tbody>{rows}</tbody></table><details><summary>Machine-readable report</summary><pre>{json}</pre></details></body></html>"#,
        phase = report.target_phase_index,
        total = report.total_phase_count,
        facilities = report.cumulative_facility_count,
        cases = report.expected_case_count,
        workers = report.worker_count,
        budget = report.case_search_budget_ms,
        first_feasible = report.first_feasible_wall_ms.map_or_else(
            || "—".to_string(),
            |milliseconds| format!("{milliseconds} ms")
        ),
        wall = report.wall_time_ms,
        complete = report.partition_complete,
        disjoint = report.cases_pairwise_disjoint,
        combined = report.combined_outcome,
        feasible = report.feasible_cases,
        infeasible = report.infeasible_cases,
        unknown = report.unknown_cases,
        invalid = report.invalid_cases,
    ))
}

fn render_rotation_root_html(report: &BottomUpRotationRootComparisonReport) -> Result<String> {
    let selected = report
        .partitioned_rotation_domains
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut rows = String::new();
    write_rotation_root_row(&mut rows, "parent", "—", &report.parent, &selected)?;
    for case in &report.cases {
        let fixed = case
            .fixed_rotations
            .iter()
            .map(|(facility, rotation)| format!("{}={rotation}°", escape_html(facility)))
            .collect::<Vec<_>>()
            .join("<br>");
        write_rotation_root_row(
            &mut rows,
            &format!("case {}", case.case_index),
            &fixed,
            &case.snapshot,
            &selected,
        )?;
    }
    let json = escape_html(&serde_json::to_string_pretty(report)?);
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Bottom-up rotation root comparison</title><style>body{{margin:24px;background:#07131d;color:#d5e8f5;font:14px ui-monospace,SFMono-Regular,Menlo,monospace}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:16px}}.certificate{{padding:10px;border:1px solid #65f0bd;color:#65f0bd;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}details{{margin-top:20px}}pre{{white-space:pre-wrap;word-break:break-word;color:#8fb2c8}}</style></head><body><h1>Bottom-up exact rotation root comparison</h1><div class="meta">Phase {phase}/{total} · {facilities} cumulative facilities · parent + {cases} exact children · clearance={priority:?}, counters={counters}, false-event-filter={filter}</div><div class="certificate">complete={complete} · pairwise-disjoint={disjoint}</div><table><thead><tr><th>model</th><th>fixed rotations</th><th>root status</th><th>root μs</th><th>selected rotation domains</th><th>owned endpoint domains</th><th>clearance opportunities</th><th>current clearance effects</th></tr></thead><tbody>{rows}</tbody></table><details><summary>Machine-readable report</summary><pre>{json}</pre></details></body></html>"#,
        phase = report.target_phase_index,
        total = report.total_phase_count,
        facilities = report.cumulative_facility_count,
        cases = report.expected_case_count,
        priority = report.endpoint_clearance_priority,
        counters = report.endpoint_clearance_counters_enabled,
        filter = report.endpoint_clearance_false_event_filter_enabled,
        complete = report.partition_complete,
        disjoint = report.cases_pairwise_disjoint,
    ))
}

fn render_rotation_provenance_html(report: &BottomUpRotationProvenanceReport) -> Result<String> {
    let mut rows = String::new();
    for (label, case) in std::iter::once(("parent".to_string(), &report.parent)).chain(
        report.cases.iter().map(|case| {
            (
                format!("case {}", case.case_index.expect("child case has index")),
                case,
            )
        }),
    ) {
        let fixed = case
            .fixed_rotations
            .values()
            .next()
            .map_or_else(|| "—".to_string(), |rotation| format!("{rotation}°"));
        let families = case
            .trace
            .decision_family_counts
            .iter()
            .map(|(family, count)| format!("{}={count}", escape_html(family)))
            .collect::<Vec<_>>()
            .join("<br>");
        let first_singletons = case
            .trace
            .first_singleton_decision
            .iter()
            .map(|(rotation, decision)| format!("{rotation}°@{decision}"))
            .collect::<Vec<_>>()
            .join(", ");
        let entries = case
            .trace
            .singleton_rotation_entries
            .iter()
            .map(|(rotation, count)| format!("{rotation}°×{count}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            rows,
            "<tr><td>{label}</td><td>{fixed}</td><td>{outcome:?}</td><td>{search}</td><td>{decisions}</td><td>{target_rotation_decisions}</td><td>{unrecorded}</td><td>{conflicts}</td><td>{first}</td><td>{entries}</td><td>{widenings}</td><td>{checks}</td><td>{families}</td></tr>",
            label = escape_html(&label),
            outcome = case.rung.outcome,
            search = case.rung.search_ms,
            decisions = case.trace.decisions,
            target_rotation_decisions = case.trace.target_rotation_decisions,
            unrecorded = case.trace.unrecorded_decisions,
            conflicts = case.trace.conflict_callbacks,
            first = escape_html(&first_singletons),
            entries = escape_html(&entries),
            widenings = case.trace.rotation_widening_transitions,
            checks = case.trace.observer_contains_checks,
        )?;
    }
    let json = escape_html(&serde_json::to_string_pretty(report)?);
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Bottom-up rotation search provenance</title><style>body{{margin:24px;background:#07131d;color:#d5e8f5;font:14px ui-monospace,SFMono-Regular,Menlo,monospace}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:16px}}.certificate{{padding:10px;border:1px solid #65f0bd;color:#65f0bd;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}details{{margin-top:20px}}pre{{white-space:pre-wrap;word-break:break-word;color:#8fb2c8}}</style></head><body><h1>Bottom-up exact rotation search provenance</h1><div class="meta">Phase {phase}/{total} · {facilities} cumulative facilities · {budget} ms/case · detailed prefix cap {detail}</div><div class="certificate">observational wrapper · complete={complete} · pairwise-disjoint={disjoint} · trace wall time is descriptive only</div><table><thead><tr><th>model</th><th>fixed rotation</th><th>outcome</th><th>search ms</th><th>decisions</th><th>target rotation decisions</th><th>unrecorded decisions</th><th>conflict callbacks</th><th>first singleton</th><th>singleton entries</th><th>widenings</th><th>contains checks</th><th>decision families</th></tr></thead><tbody>{rows}</tbody></table><details><summary>Machine-readable report</summary><pre>{json}</pre></details></body></html>"#,
        phase = report.target_phase_index,
        total = report.total_phase_count,
        facilities = report.cumulative_facility_count,
        budget = report.search_budget_ms,
        detail = report.maximum_detailed_decisions,
        complete = report.partition_complete,
        disjoint = report.cases_pairwise_disjoint,
    ))
}

fn write_rotation_root_row(
    rows: &mut String,
    label: &str,
    fixed: &str,
    snapshot: &BottomUpRootDomainSnapshot,
    selected: &BTreeSet<String>,
) -> Result<()> {
    let rotations = snapshot
        .facilities
        .iter()
        .filter(|facility| selected.contains(&facility.instance))
        .map(|facility| {
            format!(
                "{}={:?}",
                escape_html(&facility.instance),
                facility.rotation.ranges
            )
        })
        .collect::<Vec<_>>()
        .join("<br>");
    let owned = snapshot
        .endpoints
        .iter()
        .filter(|endpoint| selected.contains(&endpoint.instance))
        .collect::<Vec<_>>();
    let owned_summary = format!(
        "{} endpoints · Σport={} · Σlocal={} · Σx={} · Σy={}",
        owned.len(),
        owned
            .iter()
            .map(|endpoint| endpoint.port_choice.cardinality)
            .sum::<usize>(),
        owned
            .iter()
            .map(|endpoint| endpoint.local_key.cardinality)
            .sum::<usize>(),
        owned
            .iter()
            .map(|endpoint| endpoint.connection_x.cardinality)
            .sum::<usize>(),
        owned
            .iter()
            .map(|endpoint| endpoint.connection_y.cardinality)
            .sum::<usize>(),
    );
    writeln!(
        rows,
        "<tr><td>{}</td><td>{fixed}</td><td>{}</td><td>{}</td><td>{rotations}</td><td>{owned_summary}</td><td>{}</td><td>rejected orientations={} · bound updates={}</td></tr>",
        escape_html(label),
        snapshot.root_status,
        snapshot.root_propagation_us,
        snapshot.clearance_opportunities.len(),
        snapshot.endpoint_clearance_statistics.rejected_orientations,
        snapshot.endpoint_clearance_statistics.bound_updates,
    )?;
    Ok(())
}

fn render_grid(output: &mut String, ceiling: [i32; 2], cell: i64) -> Result<()> {
    let width = i64::from(ceiling[0]) * cell;
    let height = i64::from(ceiling[1]) * cell;
    writeln!(
        output,
        r##"<svg viewBox="0 0 {width} {height}" role="img" aria-label="bottom-up solver witness"><defs><marker id="arrow-input" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#65f0bd"/></marker><marker id="arrow-output" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#ff6b9d"/></marker></defs>"##
    )?;
    for x in 0..=ceiling[0] {
        let position = i64::from(x) * cell;
        writeln!(
            output,
            r#"<line x1="{position}" y1="0" x2="{position}" y2="{height}" class="grid"/>"#
        )?;
    }
    for y in 0..=ceiling[1] {
        let position = i64::from(y) * cell;
        writeln!(
            output,
            r#"<line x1="0" y1="{position}" x2="{width}" y2="{position}" class="grid"/>"#
        )?;
    }
    Ok(())
}

fn direction_delta(direction: aic_data::logistics::CardinalDirection) -> (i64, i64) {
    match direction {
        aic_data::logistics::CardinalDirection::North => (0, -1),
        aic_data::logistics::CardinalDirection::East => (1, 0),
        aic_data::logistics::CardinalDirection::South => (0, 1),
        aic_data::logistics::CardinalDirection::West => (-1, 0),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_facility(
    output: &mut String,
    placement_x: i64,
    placement_y: i64,
    placement_width: i64,
    placement_height: i64,
    facility: &str,
    orientation_label: &str,
    localization: Option<&ValidatedLocalizationCatalog>,
    cell: i64,
) -> Result<()> {
    let x = placement_x * cell;
    let y = placement_y * cell;
    let facility_width = placement_width * cell;
    let facility_height = placement_height * cell;
    let label = localization
        .and_then(|catalog| catalog.facility(facility))
        .map_or(facility, |entry| entry.facility_name.as_str());
    writeln!(
        output,
        r#"<g class="facility"><rect x="{x}" y="{y}" width="{facility_width}" height="{facility_height}"/><text x="{}" y="{}" class="name">{}</text><text x="{}" y="{}" class="instance">{}</text></g>"#,
        x + facility_width / 2,
        y + facility_height / 2 - 5,
        escape_html(label),
        x + facility_width / 2,
        y + facility_height / 2 + 18,
        escape_html(orientation_label),
    )?;
    Ok(())
}

fn facility_orientation_label(placement: &aic_data::layouts::FacilityGeometryPlacement) -> String {
    if placement.equivalent_rotations.len() == 1 {
        format!(
            "{} · rotation {}°",
            placement.instance, placement.representative_rotation
        )
    } else {
        format!(
            "{} · {} equivalent rotations",
            placement.instance,
            placement.equivalent_rotations.len()
        )
    }
}

fn display_optional(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| value.to_string())
}

fn display_optional_float(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| format!("{value:.2}"))
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

    #[test]
    fn parses_the_independent_facility_port_rung() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-ports",
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
        .expect("independent facility-port rung should parse");

        assert!(matches!(args.rung, RungArg::FacilityPorts));
    }

    #[test]
    fn parses_the_independent_facility_port_geometry_rung() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-port-geometry",
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
        .expect("independent facility-port geometry rung should parse");

        assert!(matches!(args.rung, RungArg::FacilityPortGeometry));
    }

    #[test]
    fn parses_the_propagated_facility_port_rung() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-ports-propagated",
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
        .expect("propagated facility-port rung should parse");

        assert!(matches!(args.rung, RungArg::FacilityPortsPropagated));
    }

    #[test]
    fn parses_the_medium_priority_propagated_facility_port_rung() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-ports-propagated",
            "--endpoint-clearance-priority",
            "medium",
            "--disable-endpoint-clearance-counters",
            "--endpoint-clearance-false-event-filter",
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
        .expect("medium-priority propagated facility-port rung should parse");

        assert!(matches!(args.rung, RungArg::FacilityPortsPropagated));
        assert_eq!(
            args.endpoint_clearance_priority,
            EndpointClearancePriorityArg::Medium
        );
        assert!(args.disable_endpoint_clearance_counters);
        assert!(args.endpoint_clearance_false_event_filter);
    }

    #[test]
    fn parses_an_exact_rotation_partition() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-ports-propagated",
            "--partition-facility",
            "seed-collector",
            "--partition-facility",
            "planter-0",
            "--partition-workers",
            "3",
            "--disable-endpoint-clearance-counters",
            "--workload",
            "workload.json",
            "--placement-request",
            "placement.json",
            "--target-phase",
            "30",
            "--time-limit-ms",
            "5000",
            "--output-dir",
            "artifacts",
        ])
        .expect("rotation partition should parse");

        assert_eq!(args.partition_facilities, ["seed-collector", "planter-0"]);
        assert_eq!(args.partition_workers, 3);
        assert!(args.disable_endpoint_clearance_counters);
        validate_search_settings(&args).expect("partition settings should be accepted");
    }

    #[test]
    fn rejects_a_rotation_partition_on_the_wrong_rung_with_its_specific_error() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-geometry",
            "--partition-facility",
            "seed-collector",
            "--workload",
            "workload.json",
            "--placement-request",
            "placement.json",
            "--target-phase",
            "30",
            "--time-limit-ms",
            "5000",
            "--output-dir",
            "artifacts",
        ])
        .expect("CLI syntax should parse before semantic validation");

        let error = validate_search_settings(&args)
            .expect_err("partition must require the propagated facility-port rung");
        assert!(
            error
                .to_string()
                .contains("rotation partition applies only to rung")
        );
    }

    #[test]
    fn parses_a_rotation_root_snapshot_without_a_search_budget_change() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-ports-propagated",
            "--partition-facility",
            "seed-collector",
            "--partition-root-snapshot",
            "--workload",
            "workload.json",
            "--placement-request",
            "placement.json",
            "--target-phase",
            "30",
            "--time-limit-ms",
            "5000",
            "--output-dir",
            "artifacts",
        ])
        .expect("rotation root snapshot should parse");

        assert!(args.partition_root_snapshot);
        validate_search_settings(&args).expect("root snapshot settings should be accepted");
    }

    #[test]
    fn parses_a_rotation_search_provenance_diagnosis() {
        let args = Args::try_parse_from([
            "aic-bottom-up-ladder",
            "--rung",
            "facility-ports-propagated",
            "--partition-facility",
            "seed-collector",
            "--partition-search-provenance",
            "--trace-detailed-decisions",
            "128",
            "--workload",
            "workload.json",
            "--placement-request",
            "placement.json",
            "--target-phase",
            "30",
            "--time-limit-ms",
            "5000",
            "--output-dir",
            "artifacts",
        ])
        .expect("rotation search provenance should parse");

        assert!(args.partition_search_provenance);
        assert_eq!(args.trace_detailed_decisions, 128);
        validate_search_settings(&args).expect("search provenance settings should be accepted");
    }

    #[test]
    fn labels_a_geometry_class_without_claiming_a_selected_rotation() {
        let placement = aic_data::layouts::FacilityGeometryPlacement {
            instance: "facility-0".to_string(),
            recipe: "recipe".to_string(),
            facility: "facility".to_string(),
            x: 0,
            y: 0,
            width: 3,
            height: 3,
            representative_rotation: 0,
            equivalent_rotations: vec![0, 90, 180, 270],
        };

        assert_eq!(
            facility_orientation_label(&placement),
            "facility-0 · 4 equivalent rotations"
        );
    }
}
