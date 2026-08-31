use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    ExternalBoundaryKeyLegalSupportAbReport, ExternalBoundarySidePartitionReport,
    FacilityPlacementRequest, PriorInputPairRootSnapshotReport, PriorInputPortControlsReport,
    PriorInputPortPairPortfolioReport, PriorSourcePortPortfolioReport,
    PriorTerminalCompletionPortfolioReport, PriorTerminalPairValuePortfolioReport,
    ResidualFacilityPortTuplePortfolioReport, diagnose_external_boundary_key_legal_support_ab,
    diagnose_external_boundary_side_partition, diagnose_prior_input_pair_root_snapshot,
    diagnose_prior_input_port_controls, diagnose_prior_input_port_pair_portfolio,
    diagnose_prior_source_port_portfolio, diagnose_prior_terminal_completion_portfolio,
    diagnose_prior_terminal_pair_value_portfolio, diagnose_residual_facility_port_tuple_portfolio,
    render_integrated_layout_html_with_localization,
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
    /// Expand every non-infeasible target completion by all old same-lane source ports.
    #[arg(long)]
    split_prior_source_port: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    source_case_time_limit_ms: Option<u64>,
    /// Partition either remaining old-facility belt input while the other remains free.
    #[arg(long)]
    control_prior_input_ports: bool,
    #[arg(long, value_name = "INDEX")]
    representative_source_leaf_index: Option<usize>,
    #[arg(long, value_name = "MILLISECONDS")]
    input_control_case_time_limit_ms: Option<u64>,
    /// Enumerate the proof-derived Cartesian product of both remaining old-facility inputs.
    #[arg(long)]
    pair_prior_input_ports: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    input_pair_case_time_limit_ms: Option<u64>,
    /// Reproduce the lowest-index Unknown input pair and capture root domains before branching.
    #[arg(long)]
    root_domain_snapshot: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    root_snapshot_case_time_limit_ms: Option<u64>,
    /// Enumerate every root-surviving residual facility-port tuple exactly.
    #[arg(long)]
    partition_residual_facility_ports: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    residual_facility_port_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    residual_facility_port_observation_time_limit_ms: Option<u64>,
    /// Compare bounded and sparse legal external boundary-key domains on the selected tuple.
    #[arg(long)]
    compare_external_boundary_key_support: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_key_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_key_observation_time_limit_ms: Option<u64>,
    /// Split one selected external demand into exact north/east/south/west cases.
    #[arg(long)]
    partition_external_boundary_side: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_side_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_side_observation_time_limit_ms: Option<u64>,
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
    ensure!(
        !args.root_domain_snapshot || args.pair_prior_input_ports,
        "--root-domain-snapshot requires --pair-prior-input-ports"
    );
    ensure!(
        args.root_domain_snapshot == args.root_snapshot_case_time_limit_ms.is_some(),
        "--root-snapshot-case-time-limit-ms must be supplied exactly when --root-domain-snapshot is enabled"
    );
    ensure!(
        !args.partition_residual_facility_ports || args.root_domain_snapshot,
        "--partition-residual-facility-ports requires --root-domain-snapshot"
    );
    ensure!(
        args.partition_residual_facility_ports
            == args.residual_facility_port_case_time_limit_ms.is_some(),
        "--residual-facility-port-case-time-limit-ms must be supplied exactly when --partition-residual-facility-ports is enabled"
    );
    ensure!(
        args.partition_residual_facility_ports
            == args
                .residual_facility_port_observation_time_limit_ms
                .is_some(),
        "--residual-facility-port-observation-time-limit-ms must be supplied exactly when --partition-residual-facility-ports is enabled"
    );
    ensure!(
        !args.compare_external_boundary_key_support || args.partition_residual_facility_ports,
        "--compare-external-boundary-key-support requires --partition-residual-facility-ports"
    );
    ensure!(
        args.compare_external_boundary_key_support
            == args.boundary_key_case_time_limit_ms.is_some(),
        "--boundary-key-case-time-limit-ms must be supplied exactly when --compare-external-boundary-key-support is enabled"
    );
    ensure!(
        args.compare_external_boundary_key_support
            == args.boundary_key_observation_time_limit_ms.is_some(),
        "--boundary-key-observation-time-limit-ms must be supplied exactly when --compare-external-boundary-key-support is enabled"
    );
    ensure!(
        !args.partition_external_boundary_side || args.compare_external_boundary_key_support,
        "--partition-external-boundary-side requires --compare-external-boundary-key-support"
    );
    ensure!(
        args.partition_external_boundary_side == args.boundary_side_case_time_limit_ms.is_some(),
        "--boundary-side-case-time-limit-ms must be supplied exactly when --partition-external-boundary-side is enabled"
    );
    ensure!(
        args.partition_external_boundary_side
            == args.boundary_side_observation_time_limit_ms.is_some(),
        "--boundary-side-observation-time-limit-ms must be supplied exactly when --partition-external-boundary-side is enabled"
    );
    let terminal_bits = parse_terminal_pair(&args.terminal_pair)?;
    let worker_count = NonZeroUsize::new(args.worker_count)
        .context("prior-terminal pair worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(args.prefix_case_time_limit_ms)
        .context("prior-terminal pair prefix_case_time_limit_ms must be positive")?;
    let pair_budget = NonZeroU64::new(args.pair_case_time_limit_ms)
        .context("prior-terminal pair pair_case_time_limit_ms must be positive")?;
    let loaded = load_inputs(&args)?;
    if args.pair_prior_input_ports {
        run_input_pair(
            &args,
            terminal_bits,
            worker_count,
            prefix_budget,
            pair_budget,
            &loaded,
        )
    } else if args.control_prior_input_ports {
        run_input_controls(
            &args,
            terminal_bits,
            worker_count,
            prefix_budget,
            pair_budget,
            &loaded,
        )
    } else if args.split_prior_source_port {
        run_source_port(
            &args,
            terminal_bits,
            worker_count,
            prefix_budget,
            pair_budget,
            &loaded,
        )
    } else if args.complete_target_ports {
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
    ensure!(
        args.source_case_time_limit_ms.is_none(),
        "--source-case-time-limit-ms requires --split-prior-source-port"
    );
    ensure!(
        args.representative_source_leaf_index.is_none()
            && args.input_control_case_time_limit_ms.is_none()
            && args.input_pair_case_time_limit_ms.is_none(),
        "prior-input control arguments require --control-prior-input-ports"
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
    ensure!(
        args.source_case_time_limit_ms.is_none(),
        "--source-case-time-limit-ms requires --split-prior-source-port"
    );
    ensure!(
        args.representative_source_leaf_index.is_none()
            && args.input_control_case_time_limit_ms.is_none()
            && args.input_pair_case_time_limit_ms.is_none(),
        "prior-input control arguments require --control-prior-input-ports"
    );
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

fn run_source_port(
    args: &Args,
    terminal_bits: [usize; 2],
    worker_count: NonZeroUsize,
    prefix_budget: NonZeroU64,
    pair_budget: NonZeroU64,
    loaded: &LoadedInputs,
) -> Result<()> {
    ensure!(
        args.representative_source_leaf_index.is_none()
            && args.input_control_case_time_limit_ms.is_none()
            && args.input_pair_case_time_limit_ms.is_none(),
        "prior-input control arguments require --control-prior-input-ports"
    );
    ensure!(
        args.complete_target_ports,
        "--split-prior-source-port requires --complete-target-ports"
    );
    let completion_budget = NonZeroU64::new(
        args.child_case_time_limit_ms
            .context("source-port portfolio requires --child-case-time-limit-ms")?,
    )
    .context("prior-source completion child_case_time_limit_ms must be positive")?;
    let source_budget = NonZeroU64::new(
        args.source_case_time_limit_ms
            .context("source-port portfolio requires --source-case-time-limit-ms")?,
    )
    .context("prior-source source_case_time_limit_ms must be positive")?;
    let report = diagnose_prior_source_port_portfolio(
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
        Duration::from_millis(completion_budget.get()),
        Duration::from_millis(source_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("prior-source port diagnosis failed: {report:?}"))?;

    write_json(&args.output_dir.join("summary.json"), &report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_source_port_summary(&report)?.as_bytes(),
        "prior-source port summary",
    )?;
    for case in &report.cases {
        let html = render_integrated_layout_html_with_localization(
            &case.layout,
            loaded.localization.as_ref(),
        )
        .map_err(|diagnostic| {
            anyhow::anyhow!(
                "prior-source port case visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &args.output_dir.join(format!(
                "case.source-leaf-{:03}.html",
                case.source_leaf_index
            )),
            html.as_bytes(),
            "prior-source port case",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write prior-source port report")?;
    println!();
    Ok(())
}

fn run_input_controls(
    args: &Args,
    terminal_bits: [usize; 2],
    worker_count: NonZeroUsize,
    prefix_budget: NonZeroU64,
    pair_budget: NonZeroU64,
    loaded: &LoadedInputs,
) -> Result<()> {
    ensure!(
        args.input_pair_case_time_limit_ms.is_none(),
        "--input-pair-case-time-limit-ms requires --pair-prior-input-ports"
    );
    ensure!(
        args.complete_target_ports && args.split_prior_source_port,
        "--control-prior-input-ports requires --complete-target-ports and --split-prior-source-port"
    );
    let completion_budget = NonZeroU64::new(
        args.child_case_time_limit_ms
            .context("prior-input controls require --child-case-time-limit-ms")?,
    )
    .context("prior-input controls child_case_time_limit_ms must be positive")?;
    let source_budget = NonZeroU64::new(
        args.source_case_time_limit_ms
            .context("prior-input controls require --source-case-time-limit-ms")?,
    )
    .context("prior-input controls source_case_time_limit_ms must be positive")?;
    let representative_source_leaf_index = args
        .representative_source_leaf_index
        .context("prior-input controls require --representative-source-leaf-index")?;
    let control_budget = NonZeroU64::new(
        args.input_control_case_time_limit_ms
            .context("prior-input controls require --input-control-case-time-limit-ms")?,
    )
    .context("prior-input controls input_control_case_time_limit_ms must be positive")?;
    let report = diagnose_prior_input_port_controls(
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
        representative_source_leaf_index,
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(pair_budget.get()),
        Duration::from_millis(completion_budget.get()),
        Duration::from_millis(source_budget.get()),
        Duration::from_millis(control_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("prior-input control diagnosis failed: {report:?}"))?;

    write_json(&args.output_dir.join("summary.json"), &report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_input_controls_summary(&report)?.as_bytes(),
        "prior-input controls summary",
    )?;
    for suite in &report.suites {
        for case in &suite.cases {
            let html = render_integrated_layout_html_with_localization(
                &case.layout,
                loaded.localization.as_ref(),
            )
            .map_err(|diagnostic| {
                anyhow::anyhow!(
                    "prior-input control case visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args.output_dir.join(format!(
                    "case.suite-{:02}.value-{:02}.html",
                    suite.suite_index, case.case_index
                )),
                html.as_bytes(),
                "prior-input control case",
            )?;
        }
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write prior-input control report")?;
    println!();
    Ok(())
}

fn run_input_pair(
    args: &Args,
    terminal_bits: [usize; 2],
    worker_count: NonZeroUsize,
    prefix_budget: NonZeroU64,
    pair_budget: NonZeroU64,
    loaded: &LoadedInputs,
) -> Result<()> {
    ensure!(
        args.complete_target_ports
            && args.split_prior_source_port
            && args.control_prior_input_ports,
        "--pair-prior-input-ports requires --complete-target-ports, --split-prior-source-port, and --control-prior-input-ports"
    );
    let completion_budget = NonZeroU64::new(
        args.child_case_time_limit_ms
            .context("prior-input pair requires --child-case-time-limit-ms")?,
    )
    .context("prior-input pair child_case_time_limit_ms must be positive")?;
    let source_budget = NonZeroU64::new(
        args.source_case_time_limit_ms
            .context("prior-input pair requires --source-case-time-limit-ms")?,
    )
    .context("prior-input pair source_case_time_limit_ms must be positive")?;
    let representative_source_leaf_index = args
        .representative_source_leaf_index
        .context("prior-input pair requires --representative-source-leaf-index")?;
    let control_budget = NonZeroU64::new(
        args.input_control_case_time_limit_ms
            .context("prior-input pair requires --input-control-case-time-limit-ms")?,
    )
    .context("prior-input pair input_control_case_time_limit_ms must be positive")?;
    let residual_pair_budget = NonZeroU64::new(
        args.input_pair_case_time_limit_ms
            .context("prior-input pair requires --input-pair-case-time-limit-ms")?,
    )
    .context("prior-input pair input_pair_case_time_limit_ms must be positive")?;
    if args.partition_residual_facility_ports {
        let parent_observation_budget =
            NonZeroU64::new(args.root_snapshot_case_time_limit_ms.context(
                "residual facility-port portfolio requires --root-snapshot-case-time-limit-ms",
            )?)
            .context("root-domain snapshot case time limit must be positive")?;
        let authoritative_budget = NonZeroU64::new(
            args.residual_facility_port_case_time_limit_ms.context(
                "residual facility-port portfolio requires --residual-facility-port-case-time-limit-ms",
            )?,
        )
        .context("residual facility-port authoritative case time limit must be positive")?;
        let observation_budget = NonZeroU64::new(
            args.residual_facility_port_observation_time_limit_ms.context(
                "residual facility-port portfolio requires --residual-facility-port-observation-time-limit-ms",
            )?,
        )
        .context("residual facility-port observation case time limit must be positive")?;
        if args.compare_external_boundary_key_support {
            let ab_authoritative_budget = NonZeroU64::new(
                args.boundary_key_case_time_limit_ms
                    .context("boundary-key A/B requires --boundary-key-case-time-limit-ms")?,
            )
            .context("boundary-key authoritative case time limit must be positive")?;
            let ab_observation_budget =
                NonZeroU64::new(args.boundary_key_observation_time_limit_ms.context(
                    "boundary-key A/B requires --boundary-key-observation-time-limit-ms",
                )?)
                .context("boundary-key observation case time limit must be positive")?;
            if args.partition_external_boundary_side {
                let side_authoritative_budget =
                    NonZeroU64::new(args.boundary_side_case_time_limit_ms.context(
                        "boundary-side partition requires --boundary-side-case-time-limit-ms",
                    )?)
                    .context("boundary-side authoritative case time limit must be positive")?;
                let side_observation_budget = NonZeroU64::new(
                    args.boundary_side_observation_time_limit_ms.context(
                        "boundary-side partition requires --boundary-side-observation-time-limit-ms",
                    )?,
                )
                .context("boundary-side observation case time limit must be positive")?;
                let report = diagnose_external_boundary_side_partition(
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
                    representative_source_leaf_index,
                    worker_count.get(),
                    Duration::from_millis(prefix_budget.get()),
                    Duration::from_millis(pair_budget.get()),
                    Duration::from_millis(completion_budget.get()),
                    Duration::from_millis(source_budget.get()),
                    Duration::from_millis(control_budget.get()),
                    Duration::from_millis(residual_pair_budget.get()),
                    Duration::from_millis(parent_observation_budget.get()),
                    Duration::from_millis(authoritative_budget.get()),
                    Duration::from_millis(observation_budget.get()),
                    Duration::from_millis(ab_authoritative_budget.get()),
                    Duration::from_millis(ab_observation_budget.get()),
                    Duration::from_millis(side_authoritative_budget.get()),
                    Duration::from_millis(side_observation_budget.get()),
                )
                .map_err(|report| {
                    anyhow::anyhow!("external boundary-side diagnosis failed: {report:?}")
                })?;
                write_external_boundary_side_artifacts(args, loaded, &report)?;
                serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                    .context("failed to write external boundary-side report")?;
                println!();
                return Ok(());
            }
            let report = diagnose_external_boundary_key_legal_support_ab(
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
                representative_source_leaf_index,
                worker_count.get(),
                Duration::from_millis(prefix_budget.get()),
                Duration::from_millis(pair_budget.get()),
                Duration::from_millis(completion_budget.get()),
                Duration::from_millis(source_budget.get()),
                Duration::from_millis(control_budget.get()),
                Duration::from_millis(residual_pair_budget.get()),
                Duration::from_millis(parent_observation_budget.get()),
                Duration::from_millis(authoritative_budget.get()),
                Duration::from_millis(observation_budget.get()),
                Duration::from_millis(ab_authoritative_budget.get()),
                Duration::from_millis(ab_observation_budget.get()),
            )
            .map_err(|report| {
                anyhow::anyhow!("external boundary-key A/B diagnosis failed: {report:?}")
            })?;
            write_external_boundary_key_ab_artifacts(args, loaded, &report)?;
            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                .context("failed to write external boundary-key A/B report")?;
            println!();
            return Ok(());
        }
        let report = diagnose_residual_facility_port_tuple_portfolio(
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
            representative_source_leaf_index,
            worker_count.get(),
            Duration::from_millis(prefix_budget.get()),
            Duration::from_millis(pair_budget.get()),
            Duration::from_millis(completion_budget.get()),
            Duration::from_millis(source_budget.get()),
            Duration::from_millis(control_budget.get()),
            Duration::from_millis(residual_pair_budget.get()),
            Duration::from_millis(parent_observation_budget.get()),
            Duration::from_millis(authoritative_budget.get()),
            Duration::from_millis(observation_budget.get()),
        )
        .map_err(|report| {
            anyhow::anyhow!("residual facility-port tuple diagnosis failed: {report:?}")
        })?;
        write_residual_facility_port_tuple_artifacts(args, loaded, &report)?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
            .context("failed to write residual facility-port tuple report")?;
        println!();
        return Ok(());
    }
    if args.root_domain_snapshot {
        let observation_budget = NonZeroU64::new(
            args.root_snapshot_case_time_limit_ms
                .context("root-domain snapshot requires --root-snapshot-case-time-limit-ms")?,
        )
        .context("root-domain snapshot case time limit must be positive")?;
        let report = diagnose_prior_input_pair_root_snapshot(
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
            representative_source_leaf_index,
            worker_count.get(),
            Duration::from_millis(prefix_budget.get()),
            Duration::from_millis(pair_budget.get()),
            Duration::from_millis(completion_budget.get()),
            Duration::from_millis(source_budget.get()),
            Duration::from_millis(control_budget.get()),
            Duration::from_millis(residual_pair_budget.get()),
            Duration::from_millis(observation_budget.get()),
        )
        .map_err(|report| anyhow::anyhow!("root-domain snapshot diagnosis failed: {report:?}"))?;
        write_json(&args.output_dir.join("summary.json"), &report)?;
        write_bytes(
            &args.output_dir.join("summary.html"),
            render_root_snapshot_summary(&report)?.as_bytes(),
            "root-domain snapshot summary",
        )?;
        let html = render_integrated_layout_html_with_localization(
            &report.observed_layout,
            loaded.localization.as_ref(),
        )
        .map_err(|diagnostic| {
            anyhow::anyhow!(
                "root-domain observed layout visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &args.output_dir.join("observed-layout.html"),
            html.as_bytes(),
            "root-domain observed layout",
        )?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
            .context("failed to write root-domain snapshot report")?;
        println!();
        return Ok(());
    }
    let report = diagnose_prior_input_port_pair_portfolio(
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
        representative_source_leaf_index,
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(pair_budget.get()),
        Duration::from_millis(completion_budget.get()),
        Duration::from_millis(source_budget.get()),
        Duration::from_millis(control_budget.get()),
        Duration::from_millis(residual_pair_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("prior-input pair diagnosis failed: {report:?}"))?;

    write_json(&args.output_dir.join("summary.json"), &report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_input_pair_summary(&report)?.as_bytes(),
        "prior-input pair summary",
    )?;
    for case in &report.cases {
        let html = render_integrated_layout_html_with_localization(
            &case.layout,
            loaded.localization.as_ref(),
        )
        .map_err(|diagnostic| {
            anyhow::anyhow!(
                "prior-input pair case visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &args
                .output_dir
                .join(format!("case.input-pair-{:03}.html", case.pair_index)),
            html.as_bytes(),
            "prior-input pair case",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write prior-input pair report")?;
    println!();
    Ok(())
}

fn write_residual_facility_port_tuple_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &ResidualFacilityPortTuplePortfolioReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_residual_facility_port_tuple_summary(report)?.as_bytes(),
        "residual facility-port tuple summary",
    )?;
    for case in &report.cases {
        for (kind, layout) in [
            ("authoritative", &case.authoritative_layout),
            ("observation", &case.observation_layout),
        ] {
            let html = render_integrated_layout_html_with_localization(
                layout,
                loaded.localization.as_ref(),
            )
            .map_err(|diagnostic| {
                anyhow::anyhow!(
                    "residual facility-port case {} {kind} visualization failed with {}: {}",
                    case.case_index,
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args
                    .output_dir
                    .join(format!("case-{:02}.{kind}.html", case.case_index)),
                html.as_bytes(),
                "residual facility-port tuple case",
            )?;
        }
    }
    Ok(())
}

fn write_external_boundary_key_ab_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &ExternalBoundaryKeyLegalSupportAbReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_external_boundary_key_ab_summary(report)?.as_bytes(),
        "external boundary-key A/B summary",
    )?;
    for (name, layout) in [
        (
            "bounded.authoritative",
            &report.bounded.authoritative_layout,
        ),
        ("bounded.observation", &report.bounded.observation_layout),
        ("sparse.authoritative", &report.sparse.authoritative_layout),
        ("sparse.observation", &report.sparse.observation_layout),
    ] {
        let html =
            render_integrated_layout_html_with_localization(layout, loaded.localization.as_ref())
                .map_err(|diagnostic| {
                anyhow::anyhow!(
                    "external boundary-key {name} visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
        write_bytes(
            &args.output_dir.join(format!("{name}.html")),
            html.as_bytes(),
            "external boundary-key A/B layout",
        )?;
    }
    Ok(())
}

fn write_external_boundary_side_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &ExternalBoundarySidePartitionReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_external_boundary_side_summary(report)?.as_bytes(),
        "external boundary-side summary",
    )?;
    for case in &report.cases {
        for (kind, layout) in [
            ("authoritative", &case.solve.authoritative_layout),
            ("observation", &case.solve.observation_layout),
        ] {
            let html = render_integrated_layout_html_with_localization(
                layout,
                loaded.localization.as_ref(),
            )
            .map_err(|diagnostic| {
                anyhow::anyhow!(
                    "external boundary-side {} {kind} visualization failed with {}: {}",
                    case.side,
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args.output_dir.join(format!("{}.{kind}.html", case.side)),
                html.as_bytes(),
                "external boundary-side layout",
            )?;
        }
    }
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

fn render_source_port_summary(report: &PriorSourcePortPortfolioReport) -> Result<String> {
    let source_values = report
        .source_ports
        .iter()
        .zip(&report.source_port_positions)
        .map(|(port, position)| {
            format!(
                "<li><code>{port}</code> at ({},{})</li>",
                position.x, position.y
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
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>({},{})</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td></tr>",
                case.source_leaf_index,
                case.parent_completion_leaf_index,
                pair,
                case.source_assignment.port,
                case.source_position.x,
                case.source_position.y,
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms,
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
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 prior-source port portfolio</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} prior-source port portfolio</h1><div class="meta">source=<code>{}</code> · facility=<code>{}</code> · closed pair={} · closed target completion={} · expanded parents={} · source values={} · coverage regions={} · fixed terminals/leaf={} · selected lanes fixed={} · other free facility terminals={} · workers={} · source wall={}ms · total={}ms</div><h2>Source endpoint values</h2><ul>{}</ul><p>source feasible={} · source infeasible={} · source unknown={} · source invalid={} · witness={} · selected-state proof={}</p><table><thead><tr><th>leaf</th><th>parent</th><th>demand pair</th><th>source port</th><th>source cell</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.source_terminal,
        report.source_facility,
        report.closed_pair_region_count,
        report.closed_completion_region_count,
        report.expanded_completion_parent_count,
        report.source_assignment_count_per_parent,
        report.coverage_region_count,
        report.fixed_terminal_count_per_source_leaf,
        report.selected_lane_terminals_fully_fixed,
        report.unfixed_facility_terminal_domains.len(),
        report.worker_count,
        report.source_portfolio_wall_ms,
        report.total_wall_ms,
        source_values,
        report.source_child_validated_feasible_count,
        report.source_child_proven_infeasible_count,
        report.source_child_unknown_count,
        report.source_child_invalid_witness_count,
        report.validated_witness_found,
        report.selected_state_infeasibility_proven,
        rows,
        json,
    ))
}

fn render_input_controls_summary(report: &PriorInputPortControlsReport) -> Result<String> {
    let domains = report
        .controlled_domains
        .iter()
        .map(|domain| {
            format!(
                "<li><code>{}</code><br>domain=<code>{}</code></li>",
                domain.terminal,
                domain.ports.join(", ")
            )
        })
        .collect::<String>();
    let suites = report
        .suites
        .iter()
        .map(|suite| {
            let rows = suite
                .cases
                .iter()
                .map(|case| {
                    let connection = case.connection_position.as_ref().map_or_else(
                        || "out of bounds".to_string(),
                        |position| format!("({}, {})", position.x, position.y),
                    );
                    format!(
                        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                        case.case_index,
                        case.assignment.port,
                        connection,
                        case.outcome,
                        case.construction_ms,
                        case.search_ms,
                        case.first_incumbent_ms,
                        case.search_statistics.branch_decisions,
                        case.search_statistics.backtracks,
                        case.search_statistics.conflicts,
                        case.search_statistics.learned_clauses,
                        case.search_statistics.solver_propagations,
                        case.search_statistics.restarts,
                        case.model_scale.variables,
                        case.model_scale.constraints,
                        case.model_scale.incidences,
                        case.model_scale.placement_routing_incidences,
                    )
                })
                .collect::<String>();
            format!(
                "<section><h2>Suite {}: <code>{}</code></h2><p>ports={} · feasible={} · infeasible={} · unknown={} · invalid={} · complete infeasibility={}</p><table><thead><tr><th>value</th><th>port</th><th>cell</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th><th>restarts</th><th>variables</th><th>constraints</th><th>incidences</th><th>placement-routing</th></tr></thead><tbody>{}</tbody></table></section>",
                suite.suite_index,
                suite.terminal,
                suite.ports.len(),
                suite.validated_feasible_count,
                suite.proven_infeasible_count,
                suite.unknown_count,
                suite.invalid_witness_count,
                suite.complete_infeasibility_proven,
                rows,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 representative prior-input controls</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} representative prior-input controls</h1><div class="meta">source leaf={} · parent={:?} · inherited terminals={} · suites={} · cases/suite={} · workers={} · case budget={}ms · control wall={}ms · total={}ms</div><p class="warning">Each five-case suite separately partitions the same representative leaf. The two suites overlap; their ten cases are not ten disjoint proof regions.</p><h2>Controlled domains</h2><ul>{}</ul><p>witness={} · representative infeasibility={} · invalid witness={}</p>{}<details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.representative_source_leaf_index,
        report.representative_parent_outcome,
        report.inherited_terminal_count,
        report.suite_count,
        report.cases_per_suite,
        report.worker_count,
        report.case_search_budget_ms,
        report.control_wave_wall_ms,
        report.total_wall_ms,
        domains,
        report.representative_witness_found,
        report.representative_infeasibility_proven,
        report.invalid_witness_found,
        suites,
        json,
    ))
}

fn render_input_pair_summary(report: &PriorInputPortPairPortfolioReport) -> Result<String> {
    let exclusions = report
        .proof_exclusions
        .iter()
        .map(|exclusion| {
            format!(
                "<li>suite {} · <code>{}</code> = <code>{}</code> · {:?}</li>",
                exclusion.suite_index, exclusion.terminal, exclusion.port, exclusion.outcome
            )
        })
        .collect::<String>();
    let domains = report
        .residual_domains
        .iter()
        .map(|domain| {
            format!(
                "<li>suite {} · <code>{}</code><br>residual=<code>{}</code></li>",
                domain.suite_index,
                domain.terminal,
                domain.ports.join(", ")
            )
        })
        .collect::<String>();
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let assignments = case
                .assignments
                .iter()
                .map(|assignment| assignment.port.as_str())
                .collect::<Vec<_>>()
                .join(" / ");
            let cells = case
                .connection_positions
                .iter()
                .map(|position| {
                    position.as_ref().map_or_else(
                        || "out of bounds".to_string(),
                        |position| format!("({}, {})", position.x, position.y),
                    )
                })
                .collect::<Vec<_>>()
                .join(" / ");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.pair_index,
                assignments,
                cells,
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms,
                case.search_statistics.branch_decisions,
                case.search_statistics.backtracks,
                case.search_statistics.conflicts,
                case.search_statistics.learned_clauses,
                case.search_statistics.solver_propagations,
                case.search_statistics.restarts,
                case.model_scale.variables,
                case.model_scale.constraints,
                case.model_scale.incidences,
                case.model_scale.placement_routing_incidences,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 representative prior-input pair</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} representative prior-input pair</h1><div class="meta">source leaf={} · excluded atomic pairs={} · residual pairs={} · fixed terminals/pair={} · workers={} · budget={}ms · pair wall={}ms · total={}ms</div><p class="warning">Only control cases proven infeasible with the other terminal free are excluded. Every ordered residual pair, including equal-port assignments, is executed.</p><h2>Proof exclusions</h2><ul>{}</ul><h2>Residual domains</h2><ul>{}</ul><p>feasible={} · infeasible={} · unknown={} · invalid={} · witness={} · representative infeasibility={}</p><table><thead><tr><th>pair</th><th>ports</th><th>cells</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th><th>restarts</th><th>variables</th><th>constraints</th><th>incidences</th><th>placement-routing</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.representative_source_leaf_index,
        report.excluded_atomic_pair_count,
        report.residual_pair_count,
        report.fixed_terminal_count_per_pair,
        report.worker_count,
        report.pair_case_search_budget_ms,
        report.pair_wave_wall_ms,
        report.total_wall_ms,
        exclusions,
        domains,
        report.validated_feasible_count,
        report.proven_infeasible_count,
        report.unknown_count,
        report.invalid_witness_count,
        report.representative_witness_found,
        report.representative_infeasibility_proven,
        rows,
        json,
    ))
}

fn render_root_snapshot_summary(report: &PriorInputPairRootSnapshotReport) -> Result<String> {
    let assignment = report
        .assignments
        .iter()
        .map(|value| format!("{} = {}", value.terminal, value.port))
        .collect::<Vec<_>>()
        .join("<br>");
    let families = report
        .root_snapshot
        .variable_families
        .iter()
        .map(|family| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><code>{:?}</code></td><td><code>{:?}</code></td></tr>",
                family.family,
                family.total,
                family.fixed,
                family.unresolved,
                family.root_cardinality_histogram,
                family.root_span_histogram,
            )
        })
        .collect::<String>();
    let facilities = report
        .root_snapshot
        .facilities
        .iter()
        .map(|facility| {
            format!(
                "<tr><td><code>{}</code></td><td>{}</td><td><code>{:?}</code></td><td><code>{:?}</code></td><td><code>{:?}</code></td><td>{}</td></tr>",
                facility.instance,
                facility.placement_choice.cardinality,
                facility.possible_x_values,
                facility.possible_y_values,
                facility.possible_rotations,
                facility.fixed_contract_satisfied,
            )
        })
        .collect::<String>();
    let terminals = report
        .root_snapshot
        .terminals
        .iter()
        .map(|terminal| {
            let port_cardinality = terminal
                .port_choice
                .as_ref()
                .map_or_else(|| "-".to_string(), |domain| domain.cardinality.to_string());
            let external = terminal.external_geometry.as_ref().map_or_else(
                || "-".to_string(),
                |geometry| {
                    format!(
                        "{:?} / {} routable cells",
                        geometry.routable_sides, geometry.routable_unique_cells
                    )
                },
            );
            format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}/{}/{}</td><td>{}</td><td>{}</td></tr>",
                terminal.terminal,
                terminal.endpoint_kind,
                terminal.direction,
                terminal.geometry.cardinality,
                port_cardinality,
                terminal.geometry_unavailable_port_count,
                terminal.routing_options.fixed_true,
                terminal.routing_options.fixed_false,
                terminal.routing_options.unresolved,
                external,
                terminal.fixed_contract_satisfied,
            )
        })
        .collect::<String>();
    let layers = report
        .root_snapshot
        .layers
        .iter()
        .map(|layer| {
            format!(
                "<tr><td>{:?}</td><td>{}/{}/{}</td><td>{}/{}/{}</td><td>{}/{}/{}</td><td>{}/{}/{}</td><td><code>{:?}</code></td><td><code>{:?}</code></td></tr>",
                layer.transport,
                layer.route_cells.fixed_true,
                layer.route_cells.fixed_false,
                layer.route_cells.unresolved,
                layer.boundary_route_cells.fixed_true,
                layer.boundary_route_cells.fixed_false,
                layer.boundary_route_cells.unresolved,
                layer.interior_route_cells.fixed_true,
                layer.interior_route_cells.fixed_false,
                layer.interior_route_cells.unresolved,
                layer.route_arcs.fixed_true,
                layer.route_arcs.fixed_false,
                layer.route_arcs.unresolved,
                layer.arm_item_cardinality_histogram,
                layer.flows.width_histogram,
            )
        })
        .collect::<String>();
    let networks = report
        .root_snapshot
        .networks
        .iter()
        .map(|network| {
            format!(
                "<tr><td>{}</td><td><code>{}</code></td><td>{:?}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                network.network_index,
                network.network_id,
                network.transport,
                network.item,
                network.possible_supply_options,
                network.possible_demand_options,
                network.material_capable_possible_arcs,
                network.reachable_demand_options,
                network.unreachable_demand_options,
            )
        })
        .collect::<String>();
    let first = report.root_snapshot.first_decision.as_ref().map_or_else(
        || "none".to_string(),
        |decision| {
            format!(
                "{} / <code>{}</code> / <code>{}</code>",
                decision.semantic_family, decision.semantic_name, decision.predicate
            )
        },
    );
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 root-domain snapshot</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} prior-input pair root snapshot</h1><div class="meta">selected pair={} · baseline={:?} · observed={:?} · status={} · fixed terminals={} · blocked={}</div><p>{}</p><p>first decision: {}</p><p>domain coverage: {} registered / {} solver domains · {} unregistered</p><p class="warning">This is a root census from one deterministic Unknown pair. Family counts cover registered semantic domains only; broad domains and the first predicate are candidates, not runtime-cause proof.</p><h2>Variable families</h2><table><thead><tr><th>family</th><th>total</th><th>fixed</th><th>unresolved</th><th>cardinality histogram</th><th>span histogram</th></tr></thead><tbody>{}</tbody></table><h2>Facilities</h2><table><thead><tr><th>instance</th><th>placement cardinality</th><th>x</th><th>y</th><th>rotation</th><th>fixed assertion</th></tr></thead><tbody>{}</tbody></table><h2>Terminals</h2><table><thead><tr><th>terminal</th><th>kind</th><th>direction</th><th>geometry cardinality</th><th>port cardinality</th><th>geometry-unavailable ports</th><th>routing T/F/U</th><th>live external sides/cells</th><th>fixed assertion</th></tr></thead><tbody>{}</tbody></table><h2>Layers</h2><table><thead><tr><th>layer</th><th>route T/F/U</th><th>boundary T/F/U</th><th>interior T/F/U</th><th>arcs T/F/U</th><th>item cardinalities</th><th>flow widths</th></tr></thead><tbody>{}</tbody></table><h2>Networks</h2><table><thead><tr><th>#</th><th>network</th><th>layer</th><th>item</th><th>supply options</th><th>demand options</th><th>possible arcs</th><th>reachable demands</th><th>unreachable demands</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.selected_pair_index,
        report.baseline_outcome,
        report.observed_outcome,
        report.root_snapshot.capture_status,
        report.fixed_terminal_count,
        report.interpretation_blocked,
        assignment,
        first,
        report.root_snapshot.variable_coverage.registered_domains,
        report.root_snapshot.variable_coverage.solver_domains,
        report.root_snapshot.variable_coverage.unregistered_domains,
        families,
        facilities,
        terminals,
        layers,
        networks,
        json,
    ))
}

fn render_residual_facility_port_tuple_summary(
    report: &ResidualFacilityPortTuplePortfolioReport,
) -> Result<String> {
    let domains = report
        .residual_domains
        .iter()
        .map(|domain| {
            format!(
                "<li><code>{}</code>: <code>{}</code></li>",
                domain.terminal,
                domain.ports.join(" / ")
            )
        })
        .collect::<String>();
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let assignments = case
                .assignments
                .iter()
                .map(|assignment| format!("{} = {}", assignment.terminal, assignment.port))
                .collect::<Vec<_>>()
                .join("<br>");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td><a href=\"case-{:02}.authoritative.html\">solve</a> · <a href=\"case-{:02}.observation.html\">root</a></td></tr>",
                case.case_index,
                assignments,
                case.authoritative_outcome,
                case.observation_outcome,
                case.combined_outcome,
                case.fixation_observation.capture_status,
                case.fixation_observation.assertion_satisfied,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms,
                case.search_statistics.branch_decisions,
                case.search_statistics.backtracks,
                case.search_statistics.conflicts,
                case.model_scale.variables,
                case.case_index,
                case.case_index,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 residual facility-port tuples</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} residual facility-port tuple portfolio</h1><div class="meta">tuples={} · fixed terminals/case={} · workers={} · authoritative budget={}ms · observation budget={}ms · authoritative wall={}ms · observation wall={}ms · total={}ms</div><p class="warning">The authoritative 5-second solves are uninstrumented. Separate observation solves capture root propagation. Combined logical outcomes preserve either solve's witness or proof but performance counts come only from the authoritative solve.</p><h2>Exact root-surviving domains</h2><ul>{}</ul><p>authoritative feasible/infeasible/unknown/invalid = {}/{}/{}/{}<br>combined feasible/infeasible/unknown/invalid = {}/{}/{}/{}<br>parent witness={} · parent infeasibility proof={} · next unknown={:?} · blocked={}</p><table><thead><tr><th>case</th><th>fixed residual ports</th><th>authoritative</th><th>observation</th><th>combined</th><th>root status</th><th>fixation exact</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>variables</th><th>artifacts</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.tuple_count,
        report.fixed_terminal_count_per_case,
        report.worker_count,
        report.authoritative_case_search_budget_ms,
        report.observation_case_search_budget_ms,
        report.authoritative_wave_wall_ms,
        report.observation_wave_wall_ms,
        report.total_wall_ms,
        domains,
        report.authoritative_validated_feasible_count,
        report.authoritative_proven_infeasible_count,
        report.authoritative_unknown_count,
        report.authoritative_invalid_witness_count,
        report.combined_validated_feasible_count,
        report.combined_proven_infeasible_count,
        report.combined_unknown_count,
        report.combined_invalid_witness_count,
        report.parent_witness_found,
        report.parent_infeasibility_proven,
        report.selected_next_unknown_case_index,
        report.interpretation_blocked,
        rows,
        json,
    ))
}

fn render_external_boundary_key_ab_summary(
    report: &ExternalBoundaryKeyLegalSupportAbReport,
) -> Result<String> {
    let static_rows = report
        .static_certificates
        .iter()
        .map(|certificate| {
            format!(
                "<tr><td><code>{}</code></td><td>{}: <code>{}</code></td><td>{} ({})</td><td>{}</td><td>{}</td><td>{}/{}</td><td>{}/{}</td><td>{}</td></tr>",
                certificate.terminal,
                certificate.network_index,
                certificate.network_id,
                certificate.bounded_declared_count,
                certificate.bounded_declared_is_full_expected_range,
                certificate.sparse_declared_count,
                certificate.legal_key_count,
                certificate.bounded_table_count,
                certificate.sparse_table_count,
                certificate.bounded_option_count,
                certificate.sparse_option_count,
                certificate.exact_legal_set_equality,
            )
        })
        .collect::<String>();
    let root_rows = report
        .root_comparisons
        .iter()
        .map(|comparison| {
            format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{} ({})</td><td>{} ({})</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                comparison.terminal,
                comparison.legal_key_count,
                comparison.bounded_root_observed,
                comparison.bounded_root_values.len(),
                comparison.sparse_root_observed,
                comparison.sparse_root_values.len(),
                comparison.bounded_root_absent_from_legal.len(),
                comparison.sparse_root_absent_from_legal.len(),
                comparison.legal_values_pruned_only_by_sparse.len(),
            )
        })
        .collect::<String>();
    let solve = |label: &str, solve: &aic_data::layouts::ExternalBoundaryKeySolveReport| {
        format!(
            "<tr><td>{label}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td></tr>",
            solve.authoritative_outcome,
            solve.observation_outcome,
            solve.combined_outcome,
            solve.construction_ms,
            solve.search_ms,
            solve.first_incumbent_ms,
            solve.search_statistics.branch_decisions,
            solve.search_statistics.backtracks,
            solve.search_statistics.conflicts,
            solve.search_statistics.learned_clauses,
            solve.search_statistics.solver_propagations,
        )
    };
    let solve_rows = format!(
        "{}{}",
        solve("bounded", &report.bounded),
        solve("sparse", &report.sparse)
    );
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 external boundary-key A/B</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} external boundary-key legal-support A/B</h1><div class="meta">selected tuple={} · authoritative budget={}ms · observation budget={}ms · experiment={}ms · total={}ms</div><p class="warning">Logical evidence is combined symmetrically. Runtime classification uses authoritative cutoff crossing only; two timeouts do not establish a performance winner. Build certificates materialize declared domains, so displayed construction times are instrumented and are not a build-performance comparison.</p><p>combined={:?} · performance=<code>{}</code> · next case={:?} · blocked={}</p><p>static equality={} · model structure={} · root identity observed/satisfied={}/{} · root coverage={} · sparse root support={}</p><p>root totals: A/B observed terminals={}/{} · A/B absent legal support={}/{} · legal values pruned only by B={}</p><h2>Authoritative and observation outcomes</h2><table><thead><tr><th>encoding</th><th>authoritative</th><th>observation</th><th>combined</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th></tr></thead><tbody>{}</tbody></table><h2>Static build certificates</h2><table><thead><tr><th>terminal</th><th>network</th><th>A declared (full)</th><th>B declared</th><th>legal</th><th>A/B table</th><th>A/B options</th><th>exact equality</th></tr></thead><tbody>{}</tbody></table><h2>Root domains</h2><table><thead><tr><th>terminal</th><th>legal</th><th>A observed (count)</th><th>B observed (count)</th><th>A absent</th><th>B absent</th><th>supported pruned only by B</th></tr></thead><tbody>{}</tbody></table><p><a href="bounded.authoritative.html">bounded solve</a> · <a href="bounded.observation.html">bounded root</a> · <a href="sparse.authoritative.html">sparse solve</a> · <a href="sparse.observation.html">sparse root</a></p><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.selected_case_index,
        report.authoritative_case_search_budget_ms,
        report.observation_case_search_budget_ms,
        report.experiment_ms,
        report.total_wall_ms,
        report.combined_outcome,
        report.performance_classification,
        report.selected_next_case_index,
        report.interpretation_blocked,
        report.static_equivalence_satisfied,
        report.model_structure_equivalence_satisfied,
        report.root_semantic_identity_observed,
        report.root_semantic_identity_satisfied,
        report.root_observation_coverage_satisfied,
        report.sparse_root_support_satisfied,
        report.root_totals.bounded_observed_terminal_count,
        report.root_totals.sparse_observed_terminal_count,
        report.root_totals.bounded_root_absent_from_legal,
        report.root_totals.sparse_root_absent_from_legal,
        report.root_totals.legal_values_pruned_only_by_sparse,
        solve_rows,
        static_rows,
        root_rows,
        json,
    ))
}

fn render_external_boundary_side_summary(
    report: &ExternalBoundarySidePartitionReport,
) -> Result<String> {
    let side_rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td><a href=\"{}.authoritative.html\">solve</a> · <a href=\"{}.observation.html\">root</a></td></tr>",
                case.case_index,
                case.side,
                case.allowed_keys.len(),
                case.solve.authoritative_outcome,
                case.solve.observation_outcome,
                case.solve.combined_outcome,
                case.solve.construction_ms,
                case.solve.search_ms,
                case.solve.first_incumbent_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.backtracks,
                case.solve.search_statistics.conflicts,
                case.root_restriction_satisfied,
                case.facility_fixation_satisfied,
                case.side,
                case.side,
            )
        })
        .collect::<String>();
    let domain_rows = report
        .sides
        .iter()
        .map(|side| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>{:?}</code></td></tr>",
                side.case_index,
                side.side,
                side.keys.len(),
                side.keys,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 external boundary-side partition</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} external boundary-side exact partition</h1><div class="meta">network={}: <code>{}</code> · terminal=<code>{}</code> · parent keys={} · authoritative budget={}ms · observation budget={}ms · experiment={}ms · total={}ms</div><p class="warning">Each child preserves every parent-root value on one compass side. The four disjoint children exactly cover the selected terminal domain. Every child inherits the parent's four fixed placements/rotations and fifteen fixed facility ports; routing, flow, and all other external terminals remain solver decisions.</p><p>partition non-empty/disjoint/cover = {}/{}/{} · static certificates={} · controlled model contract={} · combined={:?} · feasible/infeasible/unknown/invalid={}/{}/{}/{} · blocked={}</p><h2>Partition domains</h2><table><thead><tr><th>case</th><th>side</th><th>keys</th><th>values</th></tr></thead><tbody>{}</tbody></table><h2>Child outcomes</h2><table><thead><tr><th>case</th><th>side</th><th>keys</th><th>authoritative</th><th>observation</th><th>combined</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>root restriction</th><th>facility fixation</th><th>artifacts</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.selected_network_index,
        report.selected_network_id,
        report.selected_terminal,
        report.parent_root_keys.len(),
        report.authoritative_case_search_budget_ms,
        report.observation_case_search_budget_ms,
        report.experiment_ms,
        report.total_wall_ms,
        report.partition_non_empty,
        report.partition_pairwise_disjoint,
        report.partition_exact_cover,
        report.common_static_certificates_satisfied,
        report.controlled_model_contract_satisfied,
        report.combined_outcome,
        report.validated_feasible_count,
        report.proven_infeasible_count,
        report.unknown_count,
        report.invalid_witness_count,
        report.interpretation_blocked,
        domain_rows,
        side_rows,
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
