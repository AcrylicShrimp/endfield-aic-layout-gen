use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    BoundaryCellWidthSensitivityReport, EndpointContinuationPartitionReport,
    EndpointSourceOnlyControlReport, ExternalBoundaryCellPartitionReport,
    ExternalBoundaryKeyLegalSupportAbReport, ExternalBoundarySidePartitionReport,
    FacilityPlacementRequest, GuardedCoreBoundaryCensusReport, GuardedCoreInitialGateReport,
    GuardedCoreReplayReport, GuardedCoreSequentialShrinkReport, MaterialJunctionContinuationReport,
    MaterialRow5SeparatorReport, MaterialSeparatorCutReport, PriorInputPairRootSnapshotReport,
    PriorInputPortControlsReport, PriorInputPortPairPortfolioReport,
    PriorSourcePortPortfolioReport, PriorTerminalCompletionPortfolioReport,
    PriorTerminalPairValuePortfolioReport, ResidualFacilityPortTuplePortfolioReport,
    diagnose_boundary_cell_width_sensitivity, diagnose_endpoint_continuation_partition,
    diagnose_endpoint_source_only_control, diagnose_external_boundary_cell_partition,
    diagnose_external_boundary_key_legal_support_ab, diagnose_external_boundary_side_partition,
    diagnose_guarded_core_boundary_census, diagnose_guarded_core_initial_gate,
    diagnose_guarded_core_replay, diagnose_guarded_core_sequential_shrinking,
    diagnose_material_junction_continuation, diagnose_material_row5_separator,
    diagnose_material_separator_cut, diagnose_prior_input_pair_root_snapshot,
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
    /// Split the lowest unresolved boundary side into one exact case per cell.
    #[arg(long)]
    partition_external_boundary_cell: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_cell_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_cell_observation_time_limit_ms: Option<u64>,
    /// Compare exact fixed-width models while preserving one semantic boundary-cell endpoint.
    #[arg(long)]
    sweep_boundary_cell_widths: bool,
    /// Strictly increasing widths, for example 13,14,15,16.
    #[arg(long, value_name = "WIDTHS", value_delimiter = ',')]
    boundary_cell_widths: Option<Vec<i32>>,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_cell_width_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    boundary_cell_width_observation_time_limit_ms: Option<u64>,
    /// Partition the mandatory first positive source and demand continuation arcs exactly.
    #[arg(long)]
    partition_endpoint_continuation: bool,
    #[arg(long, value_name = "NETWORK_ID")]
    endpoint_continuation_network: Option<String>,
    #[arg(long, value_name = "MILLISECONDS")]
    endpoint_continuation_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    endpoint_continuation_observation_time_limit_ms: Option<u64>,
    /// Control whether source continuation alone exposes the endpoint contradiction.
    #[arg(long)]
    control_endpoint_source_only: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    endpoint_source_only_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    endpoint_source_only_observation_time_limit_ms: Option<u64>,
    /// Partition the first selected-material crossing of a complete horizontal separator exactly.
    #[arg(long)]
    partition_material_separator: bool,
    #[arg(long, value_name = "ROW")]
    material_separator_after_row: Option<usize>,
    #[arg(long, value_name = "MILLISECONDS")]
    material_separator_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    material_separator_observation_time_limit_ms: Option<u64>,
    /// Partition the immediate selected-material continuation leaving the inherited junction.
    #[arg(long)]
    partition_material_junction: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    material_junction_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    material_junction_observation_time_limit_ms: Option<u64>,
    /// Partition the selected material's complete row-5 crossing inside junction child E.
    #[arg(long)]
    partition_material_row5_separator: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    material_row5_separator_case_time_limit_ms: Option<u64>,
    #[arg(long, value_name = "MILLISECONDS")]
    material_row5_separator_observation_time_limit_ms: Option<u64>,
    /// Rebuild the accepted row-5 case-zero premises as native predicates in the unrestricted base.
    #[arg(long)]
    guarded_core_initial_gate: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    guarded_core_full_time_limit_ms: Option<u64>,
    /// Remove one accepted atom at a time only after a fresh infeasibility proof.
    #[arg(long)]
    shrink_guarded_core: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    guarded_core_shrink_time_limit_ms: Option<u64>,
    /// Compare the unrestricted base against one exact clause derived from the proven core.
    #[arg(long)]
    replay_guarded_core: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    guarded_core_replay_time_limit_ms: Option<u64>,
    /// Enumerate the target external terminal's root-live boundary keys for every residual port tuple.
    #[arg(long)]
    census_guarded_core_boundary_keys: bool,
    #[arg(long, value_name = "MILLISECONDS")]
    guarded_core_boundary_census_time_limit_ms: Option<u64>,
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
    ensure!(
        !args.partition_external_boundary_cell || args.partition_external_boundary_side,
        "--partition-external-boundary-cell requires --partition-external-boundary-side"
    );
    ensure!(
        args.partition_external_boundary_cell == args.boundary_cell_case_time_limit_ms.is_some(),
        "--boundary-cell-case-time-limit-ms must be supplied exactly when --partition-external-boundary-cell is enabled"
    );
    ensure!(
        args.partition_external_boundary_cell
            == args.boundary_cell_observation_time_limit_ms.is_some(),
        "--boundary-cell-observation-time-limit-ms must be supplied exactly when --partition-external-boundary-cell is enabled"
    );
    ensure!(
        !args.sweep_boundary_cell_widths || args.partition_external_boundary_cell,
        "--sweep-boundary-cell-widths requires --partition-external-boundary-cell"
    );
    ensure!(
        args.sweep_boundary_cell_widths == args.boundary_cell_widths.is_some(),
        "--boundary-cell-widths must be supplied exactly when --sweep-boundary-cell-widths is enabled"
    );
    ensure!(
        args.sweep_boundary_cell_widths == args.boundary_cell_width_case_time_limit_ms.is_some(),
        "--boundary-cell-width-case-time-limit-ms must be supplied exactly when --sweep-boundary-cell-widths is enabled"
    );
    ensure!(
        args.sweep_boundary_cell_widths
            == args.boundary_cell_width_observation_time_limit_ms.is_some(),
        "--boundary-cell-width-observation-time-limit-ms must be supplied exactly when --sweep-boundary-cell-widths is enabled"
    );
    ensure!(
        !args.partition_endpoint_continuation || args.partition_external_boundary_cell,
        "--partition-endpoint-continuation requires --partition-external-boundary-cell"
    );
    ensure!(
        !args.partition_endpoint_continuation || !args.sweep_boundary_cell_widths,
        "endpoint-continuation partition and width sensitivity are separate experiments"
    );
    ensure!(
        args.partition_endpoint_continuation == args.endpoint_continuation_network.is_some(),
        "--endpoint-continuation-network must be supplied exactly when --partition-endpoint-continuation is enabled"
    );
    ensure!(
        args.partition_endpoint_continuation
            == args.endpoint_continuation_case_time_limit_ms.is_some(),
        "--endpoint-continuation-case-time-limit-ms must be supplied exactly when --partition-endpoint-continuation is enabled"
    );
    ensure!(
        args.partition_endpoint_continuation
            == args
                .endpoint_continuation_observation_time_limit_ms
                .is_some(),
        "--endpoint-continuation-observation-time-limit-ms must be supplied exactly when --partition-endpoint-continuation is enabled"
    );
    ensure!(
        !args.control_endpoint_source_only || args.partition_endpoint_continuation,
        "--control-endpoint-source-only requires --partition-endpoint-continuation"
    );
    ensure!(
        args.control_endpoint_source_only == args.endpoint_source_only_case_time_limit_ms.is_some(),
        "--endpoint-source-only-case-time-limit-ms must be supplied exactly when --control-endpoint-source-only is enabled"
    );
    ensure!(
        args.control_endpoint_source_only
            == args
                .endpoint_source_only_observation_time_limit_ms
                .is_some(),
        "--endpoint-source-only-observation-time-limit-ms must be supplied exactly when --control-endpoint-source-only is enabled"
    );
    ensure!(
        !args.partition_material_separator || args.control_endpoint_source_only,
        "--partition-material-separator requires --control-endpoint-source-only"
    );
    ensure!(
        args.partition_material_separator == args.material_separator_after_row.is_some(),
        "--material-separator-after-row must be supplied exactly when --partition-material-separator is enabled"
    );
    ensure!(
        args.partition_material_separator == args.material_separator_case_time_limit_ms.is_some(),
        "--material-separator-case-time-limit-ms must be supplied exactly when --partition-material-separator is enabled"
    );
    ensure!(
        args.partition_material_separator
            == args.material_separator_observation_time_limit_ms.is_some(),
        "--material-separator-observation-time-limit-ms must be supplied exactly when --partition-material-separator is enabled"
    );
    ensure!(
        !args.partition_material_junction || args.partition_material_separator,
        "--partition-material-junction requires --partition-material-separator"
    );
    ensure!(
        args.partition_material_junction == args.material_junction_case_time_limit_ms.is_some(),
        "--material-junction-case-time-limit-ms must be supplied exactly when --partition-material-junction is enabled"
    );
    ensure!(
        args.partition_material_junction
            == args.material_junction_observation_time_limit_ms.is_some(),
        "--material-junction-observation-time-limit-ms must be supplied exactly when --partition-material-junction is enabled"
    );
    ensure!(
        !args.partition_material_row5_separator || args.partition_material_junction,
        "--partition-material-row5-separator requires --partition-material-junction"
    );
    validate_row5_parent_separator(
        args.partition_material_row5_separator,
        args.material_separator_after_row,
    )?;
    ensure!(
        args.partition_material_row5_separator
            == args.material_row5_separator_case_time_limit_ms.is_some(),
        "--material-row5-separator-case-time-limit-ms must be supplied exactly when --partition-material-row5-separator is enabled"
    );
    ensure!(
        args.partition_material_row5_separator
            == args
                .material_row5_separator_observation_time_limit_ms
                .is_some(),
        "--material-row5-separator-observation-time-limit-ms must be supplied exactly when --partition-material-row5-separator is enabled"
    );
    ensure!(
        !args.guarded_core_initial_gate || args.partition_material_row5_separator,
        "--guarded-core-initial-gate requires --partition-material-row5-separator"
    );
    ensure!(
        args.guarded_core_initial_gate == args.guarded_core_full_time_limit_ms.is_some(),
        "--guarded-core-full-time-limit-ms must be supplied exactly when --guarded-core-initial-gate is enabled"
    );
    ensure!(
        !args.shrink_guarded_core || args.guarded_core_initial_gate,
        "--shrink-guarded-core requires --guarded-core-initial-gate"
    );
    ensure!(
        args.shrink_guarded_core == args.guarded_core_shrink_time_limit_ms.is_some(),
        "--guarded-core-shrink-time-limit-ms must be supplied exactly when --shrink-guarded-core is enabled"
    );
    ensure!(
        !args.replay_guarded_core || args.shrink_guarded_core,
        "--replay-guarded-core requires --shrink-guarded-core"
    );
    ensure!(
        args.replay_guarded_core == args.guarded_core_replay_time_limit_ms.is_some(),
        "--guarded-core-replay-time-limit-ms must be supplied exactly when --replay-guarded-core is enabled"
    );
    ensure!(
        !args.census_guarded_core_boundary_keys || args.replay_guarded_core,
        "--census-guarded-core-boundary-keys requires --replay-guarded-core"
    );
    ensure!(
        args.census_guarded_core_boundary_keys
            == args.guarded_core_boundary_census_time_limit_ms.is_some(),
        "--guarded-core-boundary-census-time-limit-ms must be supplied exactly when --census-guarded-core-boundary-keys is enabled"
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

fn validate_row5_parent_separator(enabled: bool, separator_after_row: Option<usize>) -> Result<()> {
    ensure!(
        !enabled || separator_after_row == Some(4),
        "--partition-material-row5-separator requires --material-separator-after-row 4"
    );
    Ok(())
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
                if args.partition_external_boundary_cell {
                    let cell_authoritative_budget =
                        NonZeroU64::new(args.boundary_cell_case_time_limit_ms.context(
                            "boundary-cell partition requires --boundary-cell-case-time-limit-ms",
                        )?)
                        .context("boundary-cell authoritative case time limit must be positive")?;
                    let cell_observation_budget = NonZeroU64::new(
                        args.boundary_cell_observation_time_limit_ms.context(
                            "boundary-cell partition requires --boundary-cell-observation-time-limit-ms",
                        )?,
                    )
                    .context("boundary-cell observation case time limit must be positive")?;
                    if args.partition_endpoint_continuation {
                        let continuation_authoritative_budget = NonZeroU64::new(
                            args.endpoint_continuation_case_time_limit_ms.context(
                                "endpoint-continuation partition requires --endpoint-continuation-case-time-limit-ms",
                            )?,
                        )
                        .context(
                            "endpoint-continuation authoritative case time limit must be positive",
                        )?;
                        let continuation_observation_budget = NonZeroU64::new(
                            args.endpoint_continuation_observation_time_limit_ms.context(
                                "endpoint-continuation partition requires --endpoint-continuation-observation-time-limit-ms",
                            )?,
                        )
                        .context(
                            "endpoint-continuation observation case time limit must be positive",
                        )?;
                        if args.control_endpoint_source_only {
                            let source_only_authoritative_budget = NonZeroU64::new(
                                args.endpoint_source_only_case_time_limit_ms.context(
                                    "source-only control requires --endpoint-source-only-case-time-limit-ms",
                                )?,
                            )
                            .context(
                                "source-only authoritative case time limit must be positive",
                            )?;
                            let source_only_observation_budget = NonZeroU64::new(
                                args.endpoint_source_only_observation_time_limit_ms.context(
                                    "source-only control requires --endpoint-source-only-observation-time-limit-ms",
                                )?,
                            )
                            .context(
                                "source-only observation case time limit must be positive",
                            )?;
                            if args.partition_material_separator {
                                let separator_authoritative_budget = NonZeroU64::new(
                                    args.material_separator_case_time_limit_ms.context(
                                        "material-separator partition requires --material-separator-case-time-limit-ms",
                                    )?,
                                )
                                .context(
                                    "material-separator authoritative case time limit must be positive",
                                )?;
                                let separator_observation_budget = NonZeroU64::new(
                                    args.material_separator_observation_time_limit_ms.context(
                                        "material-separator partition requires --material-separator-observation-time-limit-ms",
                                    )?,
                                )
                                .context(
                                    "material-separator observation case time limit must be positive",
                                )?;
                                if args.partition_material_junction {
                                    let junction_authoritative_budget = NonZeroU64::new(
                                        args.material_junction_case_time_limit_ms.context(
                                            "material-junction partition requires --material-junction-case-time-limit-ms",
                                        )?,
                                    )
                                    .context(
                                        "material-junction authoritative case time limit must be positive",
                                    )?;
                                    let junction_observation_budget = NonZeroU64::new(
                                        args.material_junction_observation_time_limit_ms.context(
                                            "material-junction partition requires --material-junction-observation-time-limit-ms",
                                        )?,
                                    )
                                    .context(
                                        "material-junction observation case time limit must be positive",
                                    )?;
                                    if args.partition_material_row5_separator {
                                        let row5_authoritative_budget = NonZeroU64::new(
                                            args.material_row5_separator_case_time_limit_ms
                                                .context(
                                                    "row-5 separator requires --material-row5-separator-case-time-limit-ms",
                                                )?,
                                        )
                                        .context(
                                            "row-5 separator authoritative case time limit must be positive",
                                        )?;
                                        let row5_observation_budget = NonZeroU64::new(
                                            args.material_row5_separator_observation_time_limit_ms
                                                .context(
                                                    "row-5 separator requires --material-row5-separator-observation-time-limit-ms",
                                                )?,
                                        )
                                        .context(
                                            "row-5 separator observation case time limit must be positive",
                                        )?;
                                        if args.guarded_core_initial_gate {
                                            let full_core_budget = NonZeroU64::new(
                                                args.guarded_core_full_time_limit_ms.context(
                                                    "guarded-core initial gate requires --guarded-core-full-time-limit-ms",
                                                )?,
                                            )
                                            .context(
                                                "guarded-core full-model time limit must be positive",
                                            )?;
                                            let report = diagnose_guarded_core_initial_gate(
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
                                                Duration::from_millis(
                                                    parent_observation_budget.get(),
                                                ),
                                                Duration::from_millis(authoritative_budget.get()),
                                                Duration::from_millis(observation_budget.get()),
                                                Duration::from_millis(
                                                    ab_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(ab_observation_budget.get()),
                                                Duration::from_millis(
                                                    side_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    side_observation_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    cell_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    cell_observation_budget.get(),
                                                ),
                                                args.endpoint_continuation_network
                                                    .clone()
                                                    .context(
                                                        "endpoint-continuation network is required",
                                                    )?,
                                                Duration::from_millis(
                                                    continuation_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    continuation_observation_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    source_only_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    source_only_observation_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    separator_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    separator_observation_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    junction_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    junction_observation_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    row5_authoritative_budget.get(),
                                                ),
                                                Duration::from_millis(
                                                    row5_observation_budget.get(),
                                                ),
                                                Duration::from_millis(full_core_budget.get()),
                                            )
                                            .map_err(|report| {
                                                anyhow::anyhow!(
                                                    "guarded-core initial gate failed: {report:?}"
                                                )
                                            })?;
                                            write_guarded_core_initial_gate_artifacts(
                                                args, loaded, &report,
                                            )?;
                                            if args.shrink_guarded_core {
                                                let shrink_budget = NonZeroU64::new(
                                                    args.guarded_core_shrink_time_limit_ms.context(
                                                        "guarded-core shrinking requires --guarded-core-shrink-time-limit-ms",
                                                    )?,
                                                )
                                                .context(
                                                    "guarded-core shrinking time limit must be positive",
                                                )?;
                                                let shrink_report =
                                                    diagnose_guarded_core_sequential_shrinking(
                                                        &loaded.wiring,
                                                        &loaded.facilities,
                                                        &loaded.items,
                                                        &loaded.transports,
                                                        &loaded.components,
                                                        &loaded.placement_request,
                                                        args.target_phase,
                                                        report,
                                                        Duration::from_millis(shrink_budget.get()),
                                                    )
                                                    .map_err(|report| {
                                                        anyhow::anyhow!(
                                                            "guarded-core sequential shrinking failed: {report:?}"
                                                        )
                                                    })?;
                                                write_guarded_core_shrinking_artifacts(
                                                    args,
                                                    loaded,
                                                    &shrink_report,
                                                )?;
                                                if args.replay_guarded_core {
                                                    let replay_budget = NonZeroU64::new(
                                                        args.guarded_core_replay_time_limit_ms
                                                            .context(
                                                                "guarded-core replay requires --guarded-core-replay-time-limit-ms",
                                                            )?,
                                                    )
                                                    .context(
                                                        "guarded-core replay time limit must be positive",
                                                    )?;
                                                    let replay_report = diagnose_guarded_core_replay(
                                                        &loaded.wiring,
                                                        &loaded.facilities,
                                                        &loaded.items,
                                                        &loaded.transports,
                                                        &loaded.components,
                                                        &loaded.placement_request,
                                                        args.target_phase,
                                                        shrink_report,
                                                        Duration::from_millis(replay_budget.get()),
                                                    )
                                                    .map_err(|report| {
                                                        anyhow::anyhow!(
                                                            "guarded-core replay failed: {report:?}"
                                                        )
                                                    })?;
                                                    write_guarded_core_replay_artifacts(
                                                        args,
                                                        loaded,
                                                        &replay_report,
                                                    )?;
                                                    if args.census_guarded_core_boundary_keys {
                                                        let census_budget = NonZeroU64::new(
                                                            args.guarded_core_boundary_census_time_limit_ms
                                                                .context(
                                                                    "guarded-core boundary census requires --guarded-core-boundary-census-time-limit-ms",
                                                                )?,
                                                        )
                                                        .context(
                                                            "guarded-core boundary census time limit must be positive",
                                                        )?;
                                                        let census_report =
                                                            diagnose_guarded_core_boundary_census(
                                                                &loaded.wiring,
                                                                &loaded.facilities,
                                                                &loaded.items,
                                                                &loaded.transports,
                                                                &loaded.components,
                                                                &loaded.placement_request,
                                                                args.target_phase,
                                                                replay_report,
                                                                worker_count.get(),
                                                                Duration::from_millis(
                                                                    census_budget.get(),
                                                                ),
                                                            )
                                                            .map_err(|report| {
                                                                anyhow::anyhow!(
                                                                    "guarded-core boundary census failed: {report:?}"
                                                                )
                                                            })?;
                                                        write_guarded_core_boundary_census_artifacts(
                                                            args,
                                                            loaded,
                                                            &census_report,
                                                        )?;
                                                        serde_json::to_writer_pretty(
                                                            std::io::stdout().lock(),
                                                            &census_report,
                                                        )
                                                        .context(
                                                            "failed to write guarded-core boundary census report",
                                                        )?;
                                                        println!();
                                                        return Ok(());
                                                    }
                                                    serde_json::to_writer_pretty(
                                                        std::io::stdout().lock(),
                                                        &replay_report,
                                                    )
                                                    .context(
                                                        "failed to write guarded-core replay report",
                                                    )?;
                                                    println!();
                                                    return Ok(());
                                                }
                                                serde_json::to_writer_pretty(
                                                    std::io::stdout().lock(),
                                                    &shrink_report,
                                                )
                                                .context(
                                                    "failed to write guarded-core shrinking report",
                                                )?;
                                                println!();
                                                return Ok(());
                                            }
                                            serde_json::to_writer_pretty(
                                                std::io::stdout().lock(),
                                                &report,
                                            )
                                            .context(
                                                "failed to write guarded-core initial-gate report",
                                            )?;
                                            println!();
                                            return Ok(());
                                        }
                                        let report = diagnose_material_row5_separator(
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
                                            Duration::from_millis(cell_authoritative_budget.get()),
                                            Duration::from_millis(cell_observation_budget.get()),
                                            args.endpoint_continuation_network.clone().context(
                                                "endpoint-continuation network is required",
                                            )?,
                                            Duration::from_millis(
                                                continuation_authoritative_budget.get(),
                                            ),
                                            Duration::from_millis(
                                                continuation_observation_budget.get(),
                                            ),
                                            Duration::from_millis(
                                                source_only_authoritative_budget.get(),
                                            ),
                                            Duration::from_millis(
                                                source_only_observation_budget.get(),
                                            ),
                                            Duration::from_millis(
                                                separator_authoritative_budget.get(),
                                            ),
                                            Duration::from_millis(
                                                separator_observation_budget.get(),
                                            ),
                                            Duration::from_millis(
                                                junction_authoritative_budget.get(),
                                            ),
                                            Duration::from_millis(
                                                junction_observation_budget.get(),
                                            ),
                                            Duration::from_millis(row5_authoritative_budget.get()),
                                            Duration::from_millis(row5_observation_budget.get()),
                                        )
                                        .map_err(|report| {
                                            anyhow::anyhow!(
                                                "row-5 material separator failed: {report:?}"
                                            )
                                        })?;
                                        write_material_row5_artifacts(args, loaded, &report)?;
                                        serde_json::to_writer_pretty(
                                            std::io::stdout().lock(),
                                            &report,
                                        )
                                        .context("failed to write row-5 separator report")?;
                                        println!();
                                        return Ok(());
                                    }
                                    let report = diagnose_material_junction_continuation(
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
                                        Duration::from_millis(cell_authoritative_budget.get()),
                                        Duration::from_millis(cell_observation_budget.get()),
                                        args.endpoint_continuation_network.clone().context(
                                            "endpoint-continuation network is required",
                                        )?,
                                        Duration::from_millis(
                                            continuation_authoritative_budget.get(),
                                        ),
                                        Duration::from_millis(
                                            continuation_observation_budget.get(),
                                        ),
                                        Duration::from_millis(
                                            source_only_authoritative_budget.get(),
                                        ),
                                        Duration::from_millis(
                                            source_only_observation_budget.get(),
                                        ),
                                        args.material_separator_after_row.context(
                                            "material-separator partition requires --material-separator-after-row",
                                        )?,
                                        Duration::from_millis(
                                            separator_authoritative_budget.get(),
                                        ),
                                        Duration::from_millis(
                                            separator_observation_budget.get(),
                                        ),
                                        Duration::from_millis(
                                            junction_authoritative_budget.get(),
                                        ),
                                        Duration::from_millis(junction_observation_budget.get()),
                                    )
                                    .map_err(|report| {
                                        anyhow::anyhow!(
                                            "material-junction partition failed: {report:?}"
                                        )
                                    })?;
                                    write_material_junction_artifacts(args, loaded, &report)?;
                                    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                                        .context("failed to write material-junction report")?;
                                    println!();
                                    return Ok(());
                                }
                                let report = diagnose_material_separator_cut(
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
                                    Duration::from_millis(cell_authoritative_budget.get()),
                                    Duration::from_millis(cell_observation_budget.get()),
                                    args.endpoint_continuation_network
                                        .clone()
                                        .context("endpoint-continuation network is required")?,
                                    Duration::from_millis(
                                        continuation_authoritative_budget.get(),
                                    ),
                                    Duration::from_millis(
                                        continuation_observation_budget.get(),
                                    ),
                                    Duration::from_millis(
                                        source_only_authoritative_budget.get(),
                                    ),
                                    Duration::from_millis(
                                        source_only_observation_budget.get(),
                                    ),
                                    args.material_separator_after_row.context(
                                        "material-separator partition requires --material-separator-after-row",
                                    )?,
                                    Duration::from_millis(separator_authoritative_budget.get()),
                                    Duration::from_millis(separator_observation_budget.get()),
                                )
                                .map_err(|report| {
                                    anyhow::anyhow!(
                                        "material-separator partition failed: {report:?}"
                                    )
                                })?;
                                write_material_separator_artifacts(args, loaded, &report)?;
                                serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                                    .context("failed to write material-separator report")?;
                                println!();
                                return Ok(());
                            }
                            let report = diagnose_endpoint_source_only_control(
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
                                Duration::from_millis(cell_authoritative_budget.get()),
                                Duration::from_millis(cell_observation_budget.get()),
                                args.endpoint_continuation_network
                                    .clone()
                                    .context("endpoint-continuation network is required")?,
                                Duration::from_millis(continuation_authoritative_budget.get()),
                                Duration::from_millis(continuation_observation_budget.get()),
                                Duration::from_millis(source_only_authoritative_budget.get()),
                                Duration::from_millis(source_only_observation_budget.get()),
                            )
                            .map_err(|report| {
                                anyhow::anyhow!("endpoint source-only control failed: {report:?}")
                            })?;
                            write_endpoint_source_only_artifacts(args, loaded, &report)?;
                            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                                .context("failed to write endpoint source-only report")?;
                            println!();
                            return Ok(());
                        }
                        let report = diagnose_endpoint_continuation_partition(
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
                            Duration::from_millis(cell_authoritative_budget.get()),
                            Duration::from_millis(cell_observation_budget.get()),
                            args.endpoint_continuation_network
                                .clone()
                                .context("endpoint-continuation network is required")?,
                            Duration::from_millis(continuation_authoritative_budget.get()),
                            Duration::from_millis(continuation_observation_budget.get()),
                        )
                        .map_err(|report| {
                            anyhow::anyhow!(
                                "endpoint-continuation partition diagnosis failed: {report:?}"
                            )
                        })?;
                        write_endpoint_continuation_artifacts(args, loaded, &report)?;
                        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                            .context("failed to write endpoint-continuation partition report")?;
                        println!();
                        return Ok(());
                    }
                    if args.sweep_boundary_cell_widths {
                        let width_authoritative_budget = NonZeroU64::new(
                            args.boundary_cell_width_case_time_limit_ms.context(
                                "boundary-cell width sensitivity requires --boundary-cell-width-case-time-limit-ms",
                            )?,
                        )
                        .context(
                            "boundary-cell width authoritative case time limit must be positive",
                        )?;
                        let width_observation_budget = NonZeroU64::new(
                            args.boundary_cell_width_observation_time_limit_ms.context(
                                "boundary-cell width sensitivity requires --boundary-cell-width-observation-time-limit-ms",
                            )?,
                        )
                        .context(
                            "boundary-cell width observation case time limit must be positive",
                        )?;
                        let report = diagnose_boundary_cell_width_sensitivity(
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
                            Duration::from_millis(cell_authoritative_budget.get()),
                            Duration::from_millis(cell_observation_budget.get()),
                            args.boundary_cell_widths
                                .clone()
                                .context("boundary-cell width list is required")?,
                            Duration::from_millis(width_authoritative_budget.get()),
                            Duration::from_millis(width_observation_budget.get()),
                        )
                        .map_err(|report| {
                            anyhow::anyhow!(
                                "boundary-cell width-sensitivity diagnosis failed: {report:?}"
                            )
                        })?;
                        write_boundary_cell_width_artifacts(args, loaded, &report)?;
                        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                            .context("failed to write boundary-cell width-sensitivity report")?;
                        println!();
                        return Ok(());
                    }
                    let report = diagnose_external_boundary_cell_partition(
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
                        Duration::from_millis(cell_authoritative_budget.get()),
                        Duration::from_millis(cell_observation_budget.get()),
                    )
                    .map_err(|report| {
                        anyhow::anyhow!("external boundary-cell diagnosis failed: {report:?}")
                    })?;
                    write_external_boundary_cell_artifacts(args, loaded, &report)?;
                    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                        .context("failed to write external boundary-cell report")?;
                    println!();
                    return Ok(());
                }
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

fn write_external_boundary_cell_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &ExternalBoundaryCellPartitionReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_external_boundary_cell_summary(report)?.as_bytes(),
        "external boundary-cell summary",
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
                    "external boundary key {} {kind} visualization failed with {}: {}",
                    case.key,
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args
                    .output_dir
                    .join(format!("key-{}.{kind}.html", case.key)),
                html.as_bytes(),
                "external boundary-cell layout",
            )?;
        }
    }
    Ok(())
}

fn write_boundary_cell_width_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &BoundaryCellWidthSensitivityReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_boundary_cell_width_summary(report)?.as_bytes(),
        "boundary-cell width-sensitivity summary",
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
                    "boundary-cell width {} {kind} visualization failed with {}: {}",
                    case.width,
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args
                    .output_dir
                    .join(format!("width-{}.{kind}.html", case.width)),
                html.as_bytes(),
                "boundary-cell width-sensitivity layout",
            )?;
        }
    }
    Ok(())
}

fn write_endpoint_continuation_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &EndpointContinuationPartitionReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_endpoint_continuation_summary(report)?.as_bytes(),
        "endpoint-continuation partition summary",
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
                    "endpoint-continuation case {} {kind} visualization failed with {}: {}",
                    case.case_index,
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args
                    .output_dir
                    .join(format!("case-{}.{kind}.html", case.case_index)),
                html.as_bytes(),
                "endpoint-continuation layout",
            )?;
        }
    }
    Ok(())
}

fn write_endpoint_source_only_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &EndpointSourceOnlyControlReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_endpoint_source_only_summary(report)?.as_bytes(),
        "endpoint source-only summary",
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
                    "endpoint source-only case {} {kind} visualization failed with {}: {}",
                    case.case_index,
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args
                    .output_dir
                    .join(format!("case-{}.{kind}.html", case.case_index)),
                html.as_bytes(),
                "endpoint source-only layout",
            )?;
        }
    }
    Ok(())
}

fn write_material_separator_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &MaterialSeparatorCutReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_material_separator_summary(report)?.as_bytes(),
        "material-separator summary",
    )?;
    let cases = std::iter::once((&report.control, "control".to_string())).chain(
        report.cases.iter().map(|case| {
            (
                case,
                format!("case-{}", case.case_index.expect("child case index")),
            )
        }),
    );
    for (case, stem) in cases {
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
                    "material-separator {stem} {kind} visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args.output_dir.join(format!("{stem}.{kind}.html")),
                html.as_bytes(),
                "material-separator layout",
            )?;
        }
    }
    Ok(())
}

fn write_material_junction_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &MaterialJunctionContinuationReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_material_junction_summary(report)?.as_bytes(),
        "material-junction summary",
    )?;
    let cases = std::iter::once((&report.control, "control".to_string())).chain(
        report.cases.iter().map(|case| {
            (
                case,
                format!("case-{}", case.case_index.expect("child case index")),
            )
        }),
    );
    for (case, stem) in cases {
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
                    "material-junction {stem} {kind} visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args.output_dir.join(format!("{stem}.{kind}.html")),
                html.as_bytes(),
                "material-junction layout",
            )?;
        }
    }
    Ok(())
}

fn write_material_row5_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &MaterialRow5SeparatorReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_material_row5_summary(report)?.as_bytes(),
        "row-5 material-separator summary",
    )?;
    let cases = std::iter::once((&report.control, "control".to_string())).chain(
        report.cases.iter().map(|case| {
            (
                case,
                format!("case-{}", case.case_index.expect("child case index")),
            )
        }),
    );
    for (case, stem) in cases {
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
                    "row-5 separator {stem} {kind} visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
            write_bytes(
                &args.output_dir.join(format!("{stem}.{kind}.html")),
                html.as_bytes(),
                "row-5 material-separator layout",
            )?;
        }
    }
    Ok(())
}

fn write_guarded_core_initial_gate_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &GuardedCoreInitialGateReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_guarded_core_initial_gate_summary(report)?.as_bytes(),
        "guarded-core initial-gate summary",
    )?;
    let html = render_integrated_layout_html_with_localization(
        &report.full_core_layout,
        loaded.localization.as_ref(),
    )
    .map_err(|diagnostic| {
        anyhow::anyhow!(
            "guarded-core full-model visualization failed with {}: {}",
            diagnostic.code,
            diagnostic.message
        )
    })?;
    write_bytes(
        &args.output_dir.join("initial-full-core.authoritative.html"),
        html.as_bytes(),
        "guarded-core full-model layout",
    )?;
    let observation_html = render_integrated_layout_html_with_localization(
        &report.observation_layout,
        loaded.localization.as_ref(),
    )
    .map_err(|diagnostic| {
        anyhow::anyhow!(
            "guarded-core observation visualization failed with {}: {}",
            diagnostic.code,
            diagnostic.message
        )
    })?;
    write_bytes(
        &args.output_dir.join("initial-full-core.observation.html"),
        observation_html.as_bytes(),
        "guarded-core observation layout",
    )?;
    let control_html = render_integrated_layout_html_with_localization(
        &report.control_layout,
        loaded.localization.as_ref(),
    )
    .map_err(|diagnostic| {
        anyhow::anyhow!(
            "guarded-core unrestricted-control visualization failed with {}: {}",
            diagnostic.code,
            diagnostic.message
        )
    })?;
    write_bytes(
        &args
            .output_dir
            .join("unrestricted-control.observation.html"),
        control_html.as_bytes(),
        "guarded-core unrestricted-control layout",
    )?;
    Ok(())
}

fn write_guarded_core_shrinking_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &GuardedCoreSequentialShrinkReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_guarded_core_shrinking_summary(report)?.as_bytes(),
        "guarded-core shrinking summary",
    )?;
    for attempt in &report.attempts {
        let html = render_integrated_layout_html_with_localization(
            &attempt.layout,
            loaded.localization.as_ref(),
        )
        .map_err(|diagnostic| {
            anyhow::anyhow!(
                "guarded-core shrink attempt {} visualization failed with {}: {}",
                attempt.attempt_index,
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &args
                .output_dir
                .join(guarded_core_attempt_artifact_name(attempt.attempt_index)),
            html.as_bytes(),
            "guarded-core shrink attempt layout",
        )?;
    }
    for (name, layout) in [
        (
            "final-core.authoritative.html",
            report.final_authoritative_layout.as_ref(),
        ),
        (
            "final-core.observation.html",
            report.final_observation_layout.as_ref(),
        ),
    ] {
        let Some(layout) = layout else {
            continue;
        };
        let html =
            render_integrated_layout_html_with_localization(layout, loaded.localization.as_ref())
                .map_err(|diagnostic| {
                anyhow::anyhow!(
                    "guarded-core final visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
        write_bytes(
            &args.output_dir.join(name),
            html.as_bytes(),
            "guarded-core final layout",
        )?;
    }
    Ok(())
}

fn write_guarded_core_replay_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &GuardedCoreReplayReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_guarded_core_replay_summary(report)?.as_bytes(),
        "guarded-core replay summary",
    )?;
    for (name, layout) in guarded_core_replay_artifact_names().into_iter().zip([
        &report.baseline_authoritative_layout,
        &report.replay_authoritative_layout,
        &report.reverse_replay_authoritative_layout,
        &report.reverse_baseline_authoritative_layout,
        &report.baseline_observation_layout,
        &report.replay_observation_layout,
    ]) {
        let html =
            render_integrated_layout_html_with_localization(layout, loaded.localization.as_ref())
                .map_err(|diagnostic| {
                anyhow::anyhow!(
                    "guarded-core replay visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
        write_bytes(
            &args.output_dir.join(name),
            html.as_bytes(),
            "guarded-core replay layout",
        )?;
    }
    Ok(())
}

fn write_guarded_core_boundary_census_artifacts(
    args: &Args,
    loaded: &LoadedInputs,
    report: &GuardedCoreBoundaryCensusReport,
) -> Result<()> {
    write_json(&args.output_dir.join("summary.json"), report)?;
    write_bytes(
        &args.output_dir.join("summary.html"),
        render_guarded_core_boundary_census_summary(report)?.as_bytes(),
        "guarded-core boundary census summary",
    )?;
    for case in &report.cases {
        let html = render_integrated_layout_html_with_localization(
            &case.layout,
            loaded.localization.as_ref(),
        )
        .map_err(|diagnostic| {
            anyhow::anyhow!(
                "guarded-core boundary census visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &args
                .output_dir
                .join(guarded_core_boundary_census_artifact_name(case.case_index)),
            html.as_bytes(),
            "guarded-core boundary census layout",
        )?;
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

fn render_external_boundary_cell_summary(
    report: &ExternalBoundaryCellPartitionReport,
) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td><a href=\"key-{}.authoritative.html\">solve</a> · <a href=\"key-{}.observation.html\">root</a></td></tr>",
                case.case_index,
                case.key,
                case.solve.authoritative_outcome,
                case.solve.observation_outcome,
                case.solve.combined_outcome,
                case.solve.construction_ms,
                case.solve.search_ms,
                case.solve.first_incumbent_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.backtracks,
                case.solve.search_statistics.conflicts,
                case.solve.search_statistics.learned_clauses,
                case.solve.search_statistics.solver_propagations,
                case.root_restriction_satisfied,
                case.facility_fixation_satisfied,
                case.key,
                case.key,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 external boundary-cell partition</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} external boundary-cell exact partition</h1><div class="meta">side={} (case {}) · terminal=<code>{}</code> · keys={} · authoritative budget={}ms · observation budget={}ms · experiment={}ms · total={}ms</div><p class="warning">Every singleton child inherits the parent's fixed placements, rotations, and fifteen facility ports. Routing, flow, components, and every other external terminal remain solver decisions. The children exactly cover the selected side.</p><p>partition non-empty/disjoint/cover = {}/{}/{} · static certificates={} · controlled model contract={} · selected side outcome={:?} · feasible/infeasible/unknown/invalid={}/{}/{}/{} · unresolved keys=<code>{:?}</code> · blocked={}</p><table><thead><tr><th>case</th><th>key</th><th>authoritative</th><th>observation</th><th>combined</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th><th>root restriction</th><th>facility fixation</th><th>artifacts</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.selected_side,
        report.selected_side_case_index,
        report.selected_terminal,
        report.parent_side_keys.len(),
        report.authoritative_case_search_budget_ms,
        report.observation_case_search_budget_ms,
        report.experiment_ms,
        report.total_wall_ms,
        report.partition_non_empty,
        report.partition_pairwise_disjoint,
        report.partition_exact_cover,
        report.common_static_certificates_satisfied,
        report.controlled_model_contract_satisfied,
        report.selected_side_outcome,
        report.validated_feasible_count,
        report.proven_infeasible_count,
        report.unknown_count,
        report.invalid_witness_count,
        report.unresolved_keys,
        report.interpretation_blocked,
        rows,
        json,
    ))
}

fn render_boundary_cell_width_summary(
    report: &BoundaryCellWidthSensitivityReport,
) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let unresolved_route_cells = case
                .solve
                .root_snapshot
                .layers
                .iter()
                .map(|layer| layer.route_cells.unresolved)
                .sum::<usize>();
            let unresolved_route_arcs = case
                .solve
                .root_snapshot
                .layers
                .iter()
                .map(|layer| layer.route_arcs.unresolved)
                .sum::<usize>();
            let unresolved_flows = case
                .solve
                .root_snapshot
                .layers
                .iter()
                .map(|layer| layer.flows.unresolved)
                .sum::<usize>();
            let external_domains = case
                .solve
                .root_snapshot
                .terminals
                .iter()
                .filter(|terminal| terminal.endpoint_kind == "external")
                .map(|terminal| terminal.geometry.cardinality.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let first_decision = case
                .solve
                .root_snapshot
                .first_decision
                .as_ref()
                .map_or("-", |decision| decision.semantic_name.as_str());
            format!(
                "<tr><td>{}</td><td>{}×{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"width-{}.authoritative.html\">solve</a> · <a href=\"width-{}.observation.html\">root</a></td></tr>",
                case.case_index,
                case.width,
                case.height,
                case.encoded_key,
                case.solve.combined_outcome,
                case.solve.construction_ms,
                case.solve.search_ms,
                case.solve.first_incumbent_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.backtracks,
                case.solve.search_statistics.conflicts,
                case.solve.search_statistics.learned_clauses,
                case.solve.search_statistics.solver_propagations,
                case.solve.model_scale.variables,
                case.solve.model_scale.constraints,
                case.solve.model_scale.incidences,
                unresolved_route_cells,
                unresolved_route_arcs,
                unresolved_flows,
                external_domains,
                first_decision,
                case.width,
                case.width,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 boundary-cell width sensitivity</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} boundary-cell exact width sensitivity</h1><div class="meta">semantic endpoint={} (x={}, y={}, direction={}) · parent key={} · height={} · widths={:?} · authoritative budget={}ms · observation budget={}ms · experiment={}ms · total={}ms</div><p class="warning">Each row is a separate exact fixed-size problem, not a partition or monotonic proof. Four placements/rotations, fifteen facility ports, and the semantic endpoint are preserved. Routing and all other external terminals remain solver decisions.</p><p>width checks positive/increasing/ceiling/parent={}/{}/{}/{} · logical input identity={} · certificate identity={} · semantic model contract={} · feasible/infeasible/unknown/invalid={}/{}/{}/{} · witness={} · blocked={}</p><table><thead><tr><th>case</th><th>size</th><th>key</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th><th>vars</th><th>constraints</th><th>incidences</th><th>root route cells</th><th>root arcs</th><th>root flows</th><th>external domains</th><th>first decision</th><th>artifacts</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.semantic_side,
        report.semantic_x,
        report.semantic_y,
        report.semantic_direction_index,
        report.selected_parent_key,
        report.fixed_height,
        report.requested_widths,
        report.authoritative_case_search_budget_ms,
        report.observation_case_search_budget_ms,
        report.experiment_ms,
        report.total_wall_ms,
        report.widths_positive,
        report.widths_strictly_increasing,
        report.widths_within_request_ceiling,
        report.includes_parent_width,
        report.common_logical_input_identity_satisfied,
        report.common_certificate_identity_satisfied,
        report.common_semantic_model_contract_satisfied,
        report.validated_feasible_count,
        report.proven_infeasible_count,
        report.unknown_count,
        report.invalid_witness_count,
        report.witness_found,
        report.interpretation_blocked,
        rows,
        json,
    ))
}

fn render_endpoint_continuation_summary(
    report: &EndpointContinuationPartitionReport,
) -> Result<String> {
    let candidate_rows = report
        .source_candidates
        .iter()
        .map(|candidate| ("source", candidate))
        .chain(
            report
                .demand_candidates
                .iter()
                .map(|candidate| ("demand", candidate)),
        )
        .map(|(endpoint, candidate)| {
            format!(
                "<tr><td>{endpoint}</td><td>{}</td><td>{}</td><td>{}→{}</td><td><code>{:?}</code></td></tr>",
                candidate.case_index,
                candidate.terminal_cell,
                candidate.from,
                candidate.to,
                candidate.preceding,
            )
        })
        .collect::<String>();
    let case_rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{}</td><td>{}→{} / {}→{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"case-{}.authoritative.html\">solve</a> · <a href=\"case-{}.observation.html\">root</a></td></tr>",
                case.case_index,
                case.source_selected[0],
                case.source_selected[1],
                case.demand_selected[0],
                case.demand_selected[1],
                case.solve.combined_outcome,
                case.solve.construction_ms,
                case.solve.search_ms,
                case.solve.first_incumbent_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.backtracks,
                case.solve.search_statistics.conflicts,
                case.solve.search_statistics.learned_clauses,
                case.solve.search_statistics.solver_propagations,
                case.root_restriction_satisfied,
                case.continuation_certificate_satisfied,
                case.facility_fixation_satisfied,
                case.semantic_model_contract_satisfied,
                case.controlled_axis_model_satisfied,
                case.case_index,
                case.case_index,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 endpoint-continuation partition</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} endpoint-continuation exact partition</h1><div class="meta">network=<code>{}</code> · item=<code>{}</code> · boundary key={} · source={} (cell {}, flow {}) · demand={} (cell {}, flow {}) · workers={} · authoritative wave={}ms · observation wave={}ms · total={}ms</div><p class="warning">Each child fixes only the canonical first positive source arc and demand arc. Later endpoint arcs, the complete route interior, branches, bridges, cycles, placements, ports, and unrelated terminals retain the parent solver freedom.</p><p>geometry singleton={} · terminal presence={} · positive flow={} · distinct cells={} · one source/demand={} · non-empty={} · disjoint={} · exact cover={} · mandatory proof={} · feasible/infeasible/unknown/invalid={}/{}/{}/{} · blocked={}</p><h2>Canonical endpoint cases</h2><table><thead><tr><th>endpoint</th><th>case</th><th>cell</th><th>selected arc</th><th>earlier arcs fixed to zero</th></tr></thead><tbody>{}</tbody></table><h2>Child outcomes</h2><table><thead><tr><th>case</th><th>source / demand arc</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th><th>root restriction</th><th>certificate</th><th>facility/port fixation</th><th>within-case identity</th><th>controlled-axis model</th><th>artifacts</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.selected_network_id,
        report.selected_item,
        report.selected_boundary_key,
        report.source_terminal,
        report.source_cell,
        report.source_flow_units,
        report.demand_terminal,
        report.demand_cell,
        report.demand_flow_units,
        report.worker_count,
        report.authoritative_wave_wall_ms,
        report.observation_wave_wall_ms,
        report.total_wall_ms,
        report.endpoint_geometry_singleton,
        report.terminal_presence_fixed,
        report.positive_terminal_flow,
        report.source_and_demand_cells_distinct,
        report.selected_network_has_one_source_and_one_demand,
        report.continuation_sets_non_empty,
        report.canonical_partition_pairwise_disjoint,
        report.canonical_partition_exact_cover,
        report.mandatory_continuation_proof_satisfied,
        report.validated_feasible_count,
        report.proven_infeasible_count,
        report.unknown_count,
        report.invalid_witness_count,
        report.interpretation_blocked,
        candidate_rows,
        case_rows,
        json,
    ))
}

fn render_endpoint_source_only_summary(report: &EndpointSourceOnlyControlReport) -> Result<String> {
    let region_rows = report
        .parent_source_regions
        .iter()
        .map(|region| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td></tr>",
                region.source_case_index,
                region.parent_case_count,
                region.parent_validated_feasible_count,
                region.parent_proven_infeasible_count,
                region.parent_unknown_count,
                region.parent_invalid_witness_count,
                region.source_only_outcome,
                region.logical_evidence_compatible,
            )
        })
        .collect::<String>();
    let candidate_rows = report
        .source_candidates
        .iter()
        .map(|candidate| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}→{}</td><td><code>{:?}</code></td></tr>",
                candidate.case_index,
                candidate.terminal_cell,
                candidate.from,
                candidate.to,
                candidate.preceding,
            )
        })
        .collect::<String>();
    let case_rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{}</td><td>{}→{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"case-{}.authoritative.html\">solve</a> · <a href=\"case-{}.observation.html\">root</a></td></tr>",
                case.case_index,
                case.source_selected[0],
                case.source_selected[1],
                case.solve.authoritative_outcome,
                case.solve.observation_outcome,
                case.solve.combined_outcome,
                case.solve.construction_ms,
                case.solve.search_ms,
                case.solve.first_incumbent_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.conflicts,
                case.solve.search_statistics.solver_propagations,
                case.root_infeasible,
                case.root_source_restriction_satisfied,
                case.source_only_certificate_satisfied,
                case.facility_fixation_satisfied,
                case.semantic_model_contract_satisfied,
                case.controlled_axis_model_satisfied,
                case.case_index,
                case.case_index,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 endpoint source-only control</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {phase} endpoint source-only exact control</h1><div class="meta">network=<code>{network}</code> · item=<code>{item}</code> · boundary key={key} · source={source} (cell {source_cell}, flow {source_flow}) · demand={demand} (cell {demand_cell}, flow {demand_flow}) · workers={workers} · authoritative wave={auth_wave}ms · observation wave={obs_wave}ms · total={total}ms</div><p class="warning">Each child fixes only one canonical source continuation. Demand continuation, route interior, branches, bridges, cycles, placements, ports, and unrelated terminals retain the parent solver freedom.</p><p>source non-empty={non_empty} · disjoint={disjoint} · exact cover={exact_cover} · demand unrestricted={demand_free} · parent evidence complete={parent_complete} · cross evidence compatible={cross_compatible} · root infeasible={root_infeasible} · feasible/infeasible/unknown/invalid={feasible}/{infeasible}/{unknown}/{invalid} · blocked={blocked}</p><h2>Parent source regions</h2><table><thead><tr><th>source case</th><th>parent children</th><th>feasible</th><th>infeasible</th><th>unknown</th><th>invalid</th><th>source-only outcome</th><th>compatible</th></tr></thead><tbody>{region_rows}</tbody></table><h2>Canonical source cases</h2><table><thead><tr><th>case</th><th>cell</th><th>selected arc</th><th>earlier arcs fixed to zero</th></tr></thead><tbody>{candidate_rows}</tbody></table><h2>Source-only outcomes</h2><table><thead><tr><th>case</th><th>source arc</th><th>authoritative</th><th>observation</th><th>combined</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>conflicts</th><th>propagations</th><th>root infeasible</th><th>root restriction</th><th>source-only certificate</th><th>facility/port fixation</th><th>within-case identity</th><th>controlled-axis model</th><th>artifacts</th></tr></thead><tbody>{case_rows}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        phase = report.target_phase_index,
        network = report.selected_network_id,
        item = report.selected_item,
        key = report.selected_boundary_key,
        source = report.source_terminal,
        source_cell = report.source_cell,
        source_flow = report.source_flow_units,
        demand = report.demand_terminal,
        demand_cell = report.demand_cell,
        demand_flow = report.demand_flow_units,
        workers = report.worker_count,
        auth_wave = report.authoritative_wave_wall_ms,
        obs_wave = report.observation_wave_wall_ms,
        total = report.total_wall_ms,
        non_empty = report.source_partition_non_empty,
        disjoint = report.source_partition_pairwise_disjoint,
        exact_cover = report.source_partition_exact_cover,
        demand_free = report.demand_continuation_unrestricted,
        parent_complete = report.parent_region_evidence_complete,
        cross_compatible = report.cross_experiment_evidence_compatible,
        root_infeasible = report.root_infeasible_count,
        feasible = report.validated_feasible_count,
        infeasible = report.proven_infeasible_count,
        unknown = report.unknown_count,
        invalid = report.invalid_witness_count,
        blocked = report.interpretation_blocked,
        region_rows = region_rows,
        candidate_rows = candidate_rows,
        case_rows = case_rows,
        json = json,
    ))
}

fn render_material_separator_summary(report: &MaterialSeparatorCutReport) -> Result<String> {
    let candidate_rows = report
        .candidates
        .iter()
        .enumerate()
        .map(|(index, arc)| {
            format!(
                "<tr><td>{index}</td><td>{}→{}</td><td>{}</td></tr>",
                arc[0], arc[1], index
            )
        })
        .collect::<String>();
    let control_row = format!(
        "<tr><td>control</td><td>unrestricted</td><td>{:?}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"control.authoritative.html\">solve</a> · <a href=\"control.observation.html\">root</a></td></tr>",
        report.control.solve.combined_outcome,
        report.control.solve.search_ms,
        report.control.solve.search_statistics.branch_decisions,
        report.control.solve.search_statistics.conflicts,
        report.control.solve.search_statistics.solver_propagations,
        report.control.separator_certificate_satisfied,
        report.control.root_separator_restriction_satisfied,
        report.control.controlled_axis_model_satisfied,
        report.control.interpretation_blocked,
    );
    let child_rows = report
        .cases
        .iter()
        .map(|case| {
            let index = case.case_index.expect("child index");
            let arc = case.selected_arc.expect("child selected arc");
            format!(
                "<tr><td>{index}</td><td>{}→{}</td><td>{:?}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"case-{index}.authoritative.html\">solve</a> · <a href=\"case-{index}.observation.html\">root</a></td></tr>",
                arc[0],
                arc[1],
                case.solve.combined_outcome,
                case.solve.search_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.conflicts,
                case.solve.search_statistics.solver_propagations,
                case.separator_certificate_satisfied,
                case.root_separator_restriction_satisfied,
                case.controlled_axis_model_satisfied,
                case.interpretation_blocked,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 material separator cut</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {phase} exact material-separator cut</h1><div class="meta">network=<code>{network}</code> · item=<code>{item}</code> (code {item_code}) · dimensions={width}×{height} · separator=row {row}/{next_row} · source={source_cell}→{source_continuation} · demand={demand_cell} · workers={workers} · total={total}ms</div><p class="warning">The control leaves the cut unrestricted. Child i selects the first south cut arc carrying this material and excludes only earlier same-material crossings. Later crossings, reverse recrossings, branches, cycles, bridges, demand continuation, and all other route state remain free.</p><p>non-empty={non_empty} · disjoint={disjoint} · exact cover={exact_cover} · parent/control compatible={parent_compatible} · children/control compatible={child_compatible} · feasible/infeasible/unknown/invalid={feasible}/{infeasible}/{unknown}/{invalid} · all children infeasible={all_infeasible} · blocked={blocked}</p><h2>Canonical crossings</h2><table><thead><tr><th>case</th><th>south arc</th><th>earlier same-material cases excluded</th></tr></thead><tbody>{candidate_rows}</tbody></table><h2>Outcomes</h2><table><thead><tr><th>case</th><th>selected crossing</th><th>combined</th><th>search ms</th><th>decisions</th><th>conflicts</th><th>propagations</th><th>certificate</th><th>root audit</th><th>model delta</th><th>blocked</th><th>artifacts</th></tr></thead><tbody>{control_row}{child_rows}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        phase = report.target_phase_index,
        network = report.selected_network_id,
        item = report.selected_item,
        item_code = report.selected_item_code,
        width = report.fixed_dimensions[0],
        height = report.fixed_dimensions[1],
        row = report.separator_after_row,
        next_row = report.separator_after_row + 1,
        source_cell = report.source_cell,
        source_continuation = report.source_continuation_cell,
        demand_cell = report.demand_cell,
        workers = report.worker_count,
        total = report.total_wall_ms,
        non_empty = report.partition_non_empty,
        disjoint = report.partition_pairwise_disjoint,
        exact_cover = report.partition_exact_cover,
        parent_compatible = report.control_parent_evidence_compatible,
        child_compatible = report.child_control_evidence_compatible,
        feasible = report.validated_feasible_count,
        infeasible = report.proven_infeasible_count,
        unknown = report.unknown_count,
        invalid = report.invalid_witness_count,
        all_infeasible = report.all_children_proven_infeasible,
        blocked = report.interpretation_blocked,
        candidate_rows = candidate_rows,
        control_row = control_row,
        child_rows = child_rows,
        json = json,
    ))
}

fn render_material_junction_summary(report: &MaterialJunctionContinuationReport) -> Result<String> {
    let candidate_rows = report
        .candidates
        .iter()
        .enumerate()
        .map(|(index, arc)| {
            format!(
                "<tr><td>{index}</td><td>{}→{}</td><td>{}</td></tr>",
                arc[0], arc[1], index
            )
        })
        .collect::<String>();
    let control_row = format!(
        "<tr><td>control</td><td>unrestricted</td><td>{:?}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"control.authoritative.html\">solve</a> · <a href=\"control.observation.html\">root</a></td></tr>",
        report.control.solve.combined_outcome,
        report.control.solve.search_ms,
        report.control.solve.search_statistics.branch_decisions,
        report.control.solve.search_statistics.conflicts,
        report.control.solve.search_statistics.solver_propagations,
        report.control.junction_certificate_satisfied,
        report.control.root_restriction_satisfied,
        report.control.controlled_axis_model_satisfied,
        report.control.interpretation_blocked,
    );
    let child_rows = report
        .cases
        .iter()
        .map(|case| {
            let index = case.case_index.expect("child index");
            let arc = case.selected_arc.expect("child selected arc");
            format!(
                "<tr><td>{index}</td><td>{}→{}</td><td>{:?}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"case-{index}.authoritative.html\">solve</a> · <a href=\"case-{index}.observation.html\">root</a></td></tr>",
                arc[0],
                arc[1],
                case.solve.combined_outcome,
                case.solve.search_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.conflicts,
                case.solve.search_statistics.solver_propagations,
                case.junction_certificate_satisfied,
                case.root_restriction_satisfied,
                case.controlled_axis_model_satisfied,
                case.interpretation_blocked,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 material junction continuation</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%;margin-bottom:24px}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code,a{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {phase} exact material-junction continuation</h1><div class="meta">network=<code>{network}</code> · item=<code>{item}</code> (code {item_code}) · dimensions={width}×{height} · inherited={incoming_from}→{junction} · demand={demand_cell} · workers={workers} · total={total}ms</div><p class="warning">The control retains the same audit object but posts zero junction constraints. E selects material on 80→81. S excludes only same-material east use and selects 80→96. All downstream route state remains free.</p><p>candidate set non-empty={non_empty} · disjoint={disjoint} · exact cover={exact_cover} · parent/control compatible={parent_compatible} · children/control compatible={child_compatible} · feasible/infeasible/unknown/invalid={feasible}/{infeasible}/{unknown}/{invalid} · all children infeasible={all_infeasible} · blocked={blocked}</p><h2>Canonical continuations</h2><table><thead><tr><th>case</th><th>selected material arc</th><th>earlier same-material cases excluded</th></tr></thead><tbody>{candidate_rows}</tbody></table><h2>Outcomes</h2><table><thead><tr><th>case</th><th>selected continuation</th><th>combined</th><th>search ms</th><th>decisions</th><th>conflicts</th><th>propagations</th><th>certificate</th><th>root audit</th><th>model delta</th><th>blocked</th><th>artifacts</th></tr></thead><tbody>{control_row}{child_rows}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        phase = report.target_phase_index,
        network = report.selected_network_id,
        item = report.selected_item,
        item_code = report.selected_item_code,
        width = report.fixed_dimensions[0],
        height = report.fixed_dimensions[1],
        incoming_from = report.inherited_incoming_arc[0],
        junction = report.junction_cell,
        demand_cell = report.demand_cell,
        workers = report.worker_count,
        total = report.total_wall_ms,
        non_empty = report.partition_non_empty,
        disjoint = report.partition_pairwise_disjoint,
        exact_cover = report.partition_exact_cover,
        parent_compatible = report.control_parent_evidence_compatible,
        child_compatible = report.child_control_evidence_compatible,
        feasible = report.validated_feasible_count,
        infeasible = report.proven_infeasible_count,
        unknown = report.unknown_count,
        invalid = report.invalid_witness_count,
        all_infeasible = report.all_children_proven_infeasible,
        blocked = report.interpretation_blocked,
        candidate_rows = candidate_rows,
        control_row = control_row,
        child_rows = child_rows,
        json = json,
    ))
}

fn render_material_row5_summary(report: &MaterialRow5SeparatorReport) -> Result<String> {
    let rows = std::iter::once(("control".to_string(), &report.control))
        .chain(report.cases.iter().map(|case| {
            (
                case.case_index.expect("child index").to_string(),
                case,
            )
        }))
        .map(|(label, case)| {
            let selected = case
                .selected_arc
                .map_or_else(|| "unrestricted".to_string(), |arc| format!("{}→{}", arc[0], arc[1]));
            let stem = case
                .case_index
                .map_or_else(|| "control".to_string(), |index| format!("case-{index}"));
            format!(
                "<tr><td>{label}</td><td>{selected}</td><td>{:?}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"{stem}.authoritative.html\">solve</a> · <a href=\"{stem}.observation.html\">root</a></td></tr>",
                case.solve.combined_outcome,
                case.solve.search_ms,
                case.solve.search_statistics.branch_decisions,
                case.solve.search_statistics.conflicts,
                case.solve.search_statistics.solver_propagations,
                case.ordered_separator_identity_satisfied,
                case.root_restriction_satisfied,
                case.interpretation_blocked,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 row-5 material separator</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;padding:10px;color:#ffd166}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}code,a{{color:#ffd166}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {phase} E-local row-5 exact separator</h1><div class="meta">network=<code>{network}</code> · item=<code>{item}</code> · dimensions={width}×{height} · separator=row 5/6 · source prefix=48→64→80→81 · demand={demand} · workers={workers} · total={total}ms</div><p class="warning">This 16-way partition is complete only inside junction child E. S remains an unresolved sibling. Later crossings, recrossings, branches, cycles, bridges, placement, ports, and other networks remain solver decisions.</p><p>exact cover in E={cover} · S unresolved={sibling} · feasible/infeasible/unknown/invalid={feasible}/{infeasible}/{unknown}/{invalid} · E closed={e_closed} · blocked={blocked}</p><table><thead><tr><th>case</th><th>first row-5 crossing</th><th>outcome</th><th>search ms</th><th>decisions</th><th>conflicts</th><th>propagations</th><th>ordered certificates</th><th>root audit</th><th>blocked</th><th>artifacts</th></tr></thead><tbody>{rows}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        phase = report.target_phase_index,
        network = report.selected_network_id,
        item = report.selected_item,
        width = report.fixed_dimensions[0],
        height = report.fixed_dimensions[1],
        demand = report.demand_cell,
        workers = report.worker_count,
        total = report.total_wall_ms,
        cover = report.partition_exact_cover_within_e,
        sibling = report.sibling_s_unresolved,
        feasible = report.validated_feasible_count,
        infeasible = report.proven_infeasible_count,
        unknown = report.unknown_count,
        invalid = report.invalid_witness_count,
        e_closed = report.e_proven_infeasible,
        blocked = report.interpretation_blocked,
        rows = rows,
        json = json,
    ))
}

fn render_guarded_core_initial_gate_summary(
    report: &GuardedCoreInitialGateReport,
) -> Result<String> {
    let atoms = report
        .atom_ids
        .iter()
        .enumerate()
        .map(|(index, atom)| format!("<tr><td>{index}</td><td><code>{atom}</code></td></tr>"))
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Guarded core initial gate</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.gate{{border:1px solid #315066;padding:12px}}.pass{{color:#65f0bd}}.block{{color:#ff6b9d}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}code,a{{color:#ffd166}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {phase} guarded-core initial proof gate</h1><div class="meta">fixture=<code>{fixture}</code> · ceiling={width}×{height} · budget={budget}ms · authoritative={outcome:?} · observation={observation:?} · atom-free control={control:?} · status={status:?} · total={total}ms</div><div class="gate {class}">accepted fixture={fixture_ok} · atom contract={atoms_ok} · unique={unique} · categories=placement {placements} / ports {ports} / route {routes} · native certificate={certificate} · unrestricted boundary={boundary} · root predicates={root} · model identity={identity} · exact 30-clause delta={delta} · observation compatible={compatible} · control valid={control_valid} · proven infeasible={proven} · blocked={blocked}</div><p><a href="initial-full-core.authoritative.html">Authoritative</a> · <a href="initial-full-core.observation.html">Root observation</a> · <a href="unrestricted-control.observation.html">Atom-free control</a></p><table><thead><tr><th>#</th><th>native premise</th></tr></thead><tbody>{atom_rows}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        phase = report.target_phase_index,
        fixture = report.fixture_id,
        width = report.search_ceiling[0],
        height = report.search_ceiling[1],
        budget = report.search_budget_ms,
        outcome = report.full_core_outcome,
        observation = report.observation_outcome,
        control = report.control_outcome,
        status = report.gate_status,
        total = report.total_wall_ms,
        class = if report.interpretation_blocked {
            "block"
        } else {
            "pass"
        },
        atoms_ok = report.atom_count_satisfied,
        fixture_ok = report.accepted_semantic_fixture_satisfied,
        unique = report.atom_ids_unique,
        placements = report.placement_atom_count,
        ports = report.facility_port_atom_count,
        routes = report.route_atom_count,
        certificate = report.guarded_core_certificate_satisfied,
        boundary = report.unrestricted_boundary_certificate_satisfied,
        root = report.root_predicates_satisfied,
        identity = report.model_identity_satisfied,
        delta = report.guarded_core_delta_satisfied,
        compatible = report.observation_evidence_compatible,
        control_valid = report.control_evidence_valid,
        proven = report.full_core_proven_infeasible,
        blocked = report.interpretation_blocked,
        atom_rows = atoms,
        json = json,
    ))
}

fn render_guarded_core_shrinking_summary(
    report: &GuardedCoreSequentialShrinkReport,
) -> Result<String> {
    let rows = report
        .attempts
        .iter()
        .map(|attempt| {
            let artifact = guarded_core_attempt_artifact_name(attempt.attempt_index);
            format!(
                "<tr><td>{}</td><td><code>{}</code></td><td>{} → {}</td><td>{:?}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td><a href=\"{}\">evidence</a></td></tr>",
                attempt.attempt_index,
                attempt.attempted_atom_id,
                attempt.prior_core_size,
                attempt.candidate_core_size,
                attempt.outcome,
                attempt.removed,
                attempt.search_ms,
                attempt.branch_decisions,
                attempt.backtracks,
                attempt.conflicts,
                attempt.solver_propagations,
                artifact,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Guarded core sequential shrinking</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.gate{{border:1px solid #315066;padding:12px}}.pass{{color:#65f0bd}}.block{{color:#ff6b9d}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}code,a{{color:#ffd166}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase 3 guarded-core sequential shrinking</h1><div class="meta">status={status:?} · core={initial} → {final_size} · removed={removed} · budget={budget}ms/case · shrinking={shrinking}ms</div><div class="gate {class}">final proof={proof} · certificate={certificate} · unrestricted boundary={boundary} · exact delta={delta} · root predicates={root} · model identity={identity} · blocked={blocked}</div><p><a href="initial-full-core.authoritative.html">Initial core</a> · <a href="final-core.authoritative.html">Final authoritative</a> · <a href="final-core.observation.html">Final observation</a></p><table><thead><tr><th>#</th><th>attempted removal</th><th>core size</th><th>outcome</th><th>removed</th><th>search ms</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>propagations</th><th>artifact</th></tr></thead><tbody>{rows}</tbody></table><details><summary>Final retained atoms</summary><pre>{final_atoms}</pre></details><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        status = report.status,
        initial = report.initial_core_size,
        final_size = report.final_core_size,
        removed = report.removed_atom_ids.len(),
        budget = report.search_budget_ms,
        shrinking = report.shrinking_ms,
        class = if report.interpretation_blocked {
            "block"
        } else {
            "pass"
        },
        proof = report.final_proven_infeasible,
        certificate = report.final_certificate_satisfied,
        boundary = report.final_unrestricted_boundary_satisfied,
        delta = report.final_exact_model_delta_satisfied,
        root = report.final_root_predicates_satisfied,
        identity = report.final_model_identity_satisfied,
        blocked = report.interpretation_blocked,
        rows = rows,
        final_atoms = report.final_atom_ids.join("\n"),
        json = json,
    ))
}

fn render_guarded_core_replay_summary(report: &GuardedCoreReplayReport) -> Result<String> {
    let row = |label: &str,
               outcome: aic_data::layouts::ExactDimensionCaseOutcome,
               layout: &aic_data::layouts::IntegratedLayoutReport,
               artifact: &str| {
        let exact = layout.exact.as_ref();
        format!(
            "<tr><td>{label}</td><td>{outcome:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td><a href=\"{artifact}\">evidence</a></td></tr>",
            exact.map(|exact| exact.construction_ms),
            exact.map(|exact| exact.search_ms),
            exact.and_then(|exact| exact.search_statistics.branch_decisions),
            exact.and_then(|exact| exact.search_statistics.backtracks),
            exact.and_then(|exact| exact.search_statistics.conflicts),
            exact.and_then(|exact| exact.search_statistics.solver_propagations),
        )
    };
    let rows = [
        row(
            "AB baseline authoritative",
            report.baseline_authoritative_outcome,
            &report.baseline_authoritative_layout,
            "ab-0.baseline.authoritative.html",
        ),
        row(
            "AB replay authoritative",
            report.replay_authoritative_outcome,
            &report.replay_authoritative_layout,
            "ab-1.replay.authoritative.html",
        ),
        row(
            "BA replay authoritative",
            report.reverse_replay_authoritative_outcome,
            &report.reverse_replay_authoritative_layout,
            "ba-0.replay.authoritative.html",
        ),
        row(
            "BA baseline authoritative",
            report.reverse_baseline_authoritative_outcome,
            &report.reverse_baseline_authoritative_layout,
            "ba-1.baseline.authoritative.html",
        ),
        row(
            "baseline observation",
            report.baseline_observation_outcome,
            &report.baseline_observation_layout,
            "baseline.observation.html",
        ),
        row(
            "replay observation",
            report.replay_observation_outcome,
            &report.replay_observation_layout,
            "replay.observation.html",
        ),
    ]
    .join("");
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Guarded core replay A/B</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.gate{{border:1px solid #315066;padding:12px}}.pass{{color:#65f0bd}}.block{{color:#ff6b9d}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}code,a{{color:#ffd166}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase 3 guarded-core replay A/B</h1><div class="meta">status={status:?} · performance={performance:?} · comparison allowed={comparison_allowed} · atoms={atoms} · budget={budget}ms/run · order=ABBA then observations · experiment={experiment}ms · total={total}ms</div><div class="gate {class}">source proof={source} · baseline certificate={baseline_certificate} · replay clause certificate={replay_certificate} · unrestricted boundary={boundary} · baseline model={baseline_model} · accepted control={accepted_control} · replay model={replay_model} · exact +1 clause/+9 incidence delta={delta} · root contract={root} · root baseline/replay infeasible={baseline_root_infeasible}/{replay_root_infeasible} · newly eliminated={newly_eliminated} · changed core domains={root_changes} · evidence valid={evidence} · repeated outcomes consistent={consistent} · blocked={blocked}</div><p>hint=<code>{hint}</code> · complete forbidden-conjunction match={hint_match:?} · os={os}/{arch} · pid={pid}</p><table><thead><tr><th>run</th><th>outcome</th><th>build ms</th><th>search ms</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>propagations</th><th>artifact</th></tr></thead><tbody>{rows}</tbody></table><details><summary>Retained replay atoms</summary><pre>{atom_list}</pre></details><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        status = report.status,
        performance = report.performance_classification,
        comparison_allowed = report.performance_comparison_allowed,
        atoms = report.replay_atom_ids.len(),
        budget = report.search_budget_ms,
        experiment = report.experiment_ms,
        total = report.total_wall_ms,
        class = if report.interpretation_blocked {
            "block"
        } else {
            "pass"
        },
        source = report.source_proof_satisfied,
        baseline_certificate = report.baseline_certificate_satisfied,
        replay_certificate = report.replay_clause_certificate_satisfied,
        boundary = report.unrestricted_boundary_satisfied,
        baseline_model = report.baseline_model_identity_satisfied,
        accepted_control = report.baseline_matches_accepted_control,
        replay_model = report.replay_model_identity_satisfied,
        delta = report.exact_clause_delta_satisfied,
        root = report.root_snapshot_contract_satisfied,
        baseline_root_infeasible = report.baseline_root_infeasible,
        replay_root_infeasible = report.replay_root_infeasible,
        newly_eliminated = report.replay_newly_root_eliminated,
        root_changes = report.root_changed_atom_count,
        evidence = report.evidence_valid,
        consistent = report.repeated_outcomes_consistent,
        blocked = report.interpretation_blocked,
        hint = report.hint_sha256,
        hint_match = report.hint_matches_complete_replay_conjunction,
        os = report.operating_system,
        arch = report.architecture,
        pid = report.process_id,
        rows = rows,
        atom_list = report.replay_atom_ids.join("\n"),
        json = json,
    ))
}

fn render_guarded_core_boundary_census_summary(
    report: &GuardedCoreBoundaryCensusReport,
) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let assignments = case
                .assignments
                .iter()
                .map(|assignment| format!("{}={}", assignment.terminal, assignment.port))
                .collect::<Vec<_>>()
                .join("\n");
            let artifact = guarded_core_boundary_census_artifact_name(case.case_index);
            format!(
                "<tr><td>{}</td><td><pre>{}</pre></td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td><a href=\"{}\">evidence</a></td></tr>",
                case.case_index,
                escape_html_text(&assignments),
                case.root_status,
                case.outcome,
                case.root_live_key_count,
                case.search_ms,
                case.branch_decisions,
                case.backtracks,
                case.conflicts,
                case.solver_propagations,
                !case.interpretation_blocked,
                artifact,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Guarded core boundary census</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.gate{{border:1px solid #315066;padding:12px}}.pass{{color:#65f0bd}}.block{{color:#ff6b9d}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}code,a{{color:#ffd166}}pre{{white-space:pre-wrap;margin:0}}</style></head><body><h1>Phase 3 guarded-core boundary-key root census</h1><div class="meta">status={status:?} · tuples={tuples} · exact (tuple,key) pairs={pairs} · distinct keys={distinct} · selected tuple keys={selected} · legal sparse keys={legal} · budget={budget}ms/tuple · workers={workers} · census={census}ms · total chain={total}ms</div><div class="gate {class}">source replay={source} · exact tuple enumeration={enumeration} · 15-terminal request composition={fixation} · build certificates={build_certificates} · selected tuple reproduced={reproduced} · census complete={complete} · model identity={model} · unrestricted boundary={boundary} · evidence valid={evidence} · fixed 864 certified={fixed_864} · blocked={blocked}</div><p>captured={captured} · proven root-infeasible={root_infeasible} · invalid/missing={blocked_cases} · all root-live sets equal={sets_equal} · all equal selected tuple={selected_equal}</p><table><thead><tr><th>#</th><th>residual port tuple</th><th>root</th><th>outcome</th><th>|K_t|</th><th>search ms</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>propagations</th><th>valid</th><th>artifact</th></tr></thead><tbody>{rows}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={json};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        status = report.status,
        tuples = report.tuple_count,
        pairs = report.exact_portfolio_pair_count,
        distinct = report.distinct_root_live_key_count,
        selected = report.selected_parent_root_key_count,
        legal = report.unrestricted_legal_key_count,
        budget = report.observation_budget_ms,
        workers = report.worker_count,
        census = report.census_ms,
        total = report.total_wall_ms,
        class = if report.interpretation_blocked {
            "block"
        } else {
            "pass"
        },
        source = report.source_replay_satisfied,
        enumeration = report.tuple_enumeration_satisfied,
        fixation = report.complete_fixation_request_satisfied,
        build_certificates = report.build_certificates_satisfied,
        reproduced = report.selected_parent_root_set_reproduced,
        complete = report.census_complete,
        model = report.model_identity_satisfied,
        boundary = report.unrestricted_boundary_satisfied,
        evidence = report.evidence_valid,
        fixed_864 = report.fixed_864_case_count_certified,
        blocked = report.interpretation_blocked,
        captured = report.captured_case_count,
        root_infeasible = report.proven_root_infeasible_case_count,
        blocked_cases = report.blocked_case_count,
        sets_equal = report.all_root_live_sets_equal,
        selected_equal = report.all_sets_equal_selected_parent,
        rows = rows,
        json = json,
    ))
}

fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn guarded_core_attempt_artifact_name(attempt_index: usize) -> String {
    format!("attempt-{attempt_index:02}.authoritative.html")
}

fn guarded_core_replay_artifact_names() -> [&'static str; 6] {
    [
        "ab-0.baseline.authoritative.html",
        "ab-1.replay.authoritative.html",
        "ba-0.replay.authoritative.html",
        "ba-1.baseline.authoritative.html",
        "baseline.observation.html",
        "replay.observation.html",
    ]
}

fn guarded_core_boundary_census_artifact_name(case_index: usize) -> String {
    format!("census-case-{case_index:02}.observation.html")
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
    use clap::Parser;

    #[test]
    fn parses_two_distinct_terminal_bits() {
        assert_eq!(parse_terminal_pair("2,3").unwrap(), [2, 3]);
        assert!(parse_terminal_pair("2").is_err());
        assert!(parse_terminal_pair("2,2").is_err());
        assert!(parse_terminal_pair("two,3").is_err());
    }

    #[test]
    fn guarded_core_attempt_artifacts_follow_the_design_contract() {
        assert_eq!(
            guarded_core_attempt_artifact_name(0),
            "attempt-00.authoritative.html"
        );
        assert_eq!(
            guarded_core_attempt_artifact_name(29),
            "attempt-29.authoritative.html"
        );
    }

    #[test]
    fn guarded_core_replay_emits_all_six_declared_layout_artifacts() {
        assert_eq!(
            guarded_core_replay_artifact_names(),
            [
                "ab-0.baseline.authoritative.html",
                "ab-1.replay.authoritative.html",
                "ba-0.replay.authoritative.html",
                "ba-1.baseline.authoritative.html",
                "baseline.observation.html",
                "replay.observation.html",
            ]
        );
    }

    #[test]
    fn guarded_core_boundary_census_artifacts_are_stable_and_complete() {
        let names = (0..16)
            .map(guarded_core_boundary_census_artifact_name)
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 16);
        assert_eq!(names.first().unwrap(), "census-case-00.observation.html");
        assert_eq!(names.last().unwrap(), "census-case-15.observation.html");
        assert_eq!(
            names
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            16
        );
    }

    #[test]
    fn row5_separator_rejects_a_non_row4_parent() {
        assert!(validate_row5_parent_separator(false, Some(5)).is_ok());
        assert!(validate_row5_parent_separator(true, Some(4)).is_ok());
        let error = validate_row5_parent_separator(true, Some(5))
            .expect_err("row-5 experiment must reject a non-row-4 parent");
        assert!(
            error
                .to_string()
                .contains("--material-separator-after-row 4")
        );
    }

    #[test]
    fn parses_endpoint_source_only_control_stack() {
        let args = Args::try_parse_from([
            "aic-prior-terminal-pair",
            "--workload",
            "workload.json",
            "--workspace-root",
            ".",
            "--placement-request",
            "placement.json",
            "--target-phase",
            "3",
            "--used-width",
            "16",
            "--used-height",
            "16",
            "--facility-x",
            "8",
            "--facility-y",
            "5",
            "--port-assignment-index",
            "5",
            "--facility-rotation",
            "0",
            "--prior-facility-bit",
            "2",
            "--terminal-pair",
            "2,3",
            "--worker-count",
            "12",
            "--prefix-case-time-limit-ms",
            "10000",
            "--pair-case-time-limit-ms",
            "5000",
            "--complete-target-ports",
            "--child-case-time-limit-ms",
            "5000",
            "--split-prior-source-port",
            "--source-case-time-limit-ms",
            "5000",
            "--control-prior-input-ports",
            "--representative-source-leaf-index",
            "0",
            "--input-control-case-time-limit-ms",
            "5000",
            "--pair-prior-input-ports",
            "--input-pair-case-time-limit-ms",
            "5000",
            "--root-domain-snapshot",
            "--root-snapshot-case-time-limit-ms",
            "5000",
            "--partition-residual-facility-ports",
            "--residual-facility-port-case-time-limit-ms",
            "5000",
            "--residual-facility-port-observation-time-limit-ms",
            "5000",
            "--compare-external-boundary-key-support",
            "--boundary-key-case-time-limit-ms",
            "5000",
            "--boundary-key-observation-time-limit-ms",
            "5000",
            "--partition-external-boundary-side",
            "--boundary-side-case-time-limit-ms",
            "5000",
            "--boundary-side-observation-time-limit-ms",
            "5000",
            "--partition-external-boundary-cell",
            "--boundary-cell-case-time-limit-ms",
            "5000",
            "--boundary-cell-observation-time-limit-ms",
            "5000",
            "--partition-endpoint-continuation",
            "--endpoint-continuation-network",
            "network:pipe:item-liquid-xiranite-poly",
            "--endpoint-continuation-case-time-limit-ms",
            "5000",
            "--endpoint-continuation-observation-time-limit-ms",
            "5000",
            "--control-endpoint-source-only",
            "--endpoint-source-only-case-time-limit-ms",
            "5000",
            "--endpoint-source-only-observation-time-limit-ms",
            "5000",
            "--partition-material-separator",
            "--material-separator-after-row",
            "4",
            "--material-separator-case-time-limit-ms",
            "5000",
            "--material-separator-observation-time-limit-ms",
            "5000",
            "--partition-material-junction",
            "--material-junction-case-time-limit-ms",
            "5000",
            "--material-junction-observation-time-limit-ms",
            "5000",
            "--partition-material-row5-separator",
            "--material-row5-separator-case-time-limit-ms",
            "5000",
            "--material-row5-separator-observation-time-limit-ms",
            "5000",
            "--guarded-core-initial-gate",
            "--guarded-core-full-time-limit-ms",
            "5000",
            "--shrink-guarded-core",
            "--guarded-core-shrink-time-limit-ms",
            "5000",
            "--replay-guarded-core",
            "--guarded-core-replay-time-limit-ms",
            "5000",
            "--census-guarded-core-boundary-keys",
            "--guarded-core-boundary-census-time-limit-ms",
            "5000",
            "--output-dir",
            "out",
        ])
        .expect("source-only control stack should parse");

        assert!(args.partition_endpoint_continuation);
        assert!(args.control_endpoint_source_only);
        assert_eq!(args.endpoint_source_only_case_time_limit_ms, Some(5000));
        assert_eq!(
            args.endpoint_source_only_observation_time_limit_ms,
            Some(5000)
        );
        assert!(args.partition_material_separator);
        assert_eq!(args.material_separator_after_row, Some(4));
        assert_eq!(args.material_separator_case_time_limit_ms, Some(5000));
        assert_eq!(
            args.material_separator_observation_time_limit_ms,
            Some(5000)
        );
        assert!(args.partition_material_junction);
        assert_eq!(args.material_junction_case_time_limit_ms, Some(5000));
        assert_eq!(args.material_junction_observation_time_limit_ms, Some(5000));
        assert!(args.partition_material_row5_separator);
        assert_eq!(args.material_row5_separator_case_time_limit_ms, Some(5000));
        assert_eq!(
            args.material_row5_separator_observation_time_limit_ms,
            Some(5000)
        );
        assert!(args.guarded_core_initial_gate);
        assert_eq!(args.guarded_core_full_time_limit_ms, Some(5000));
        assert!(args.shrink_guarded_core);
        assert_eq!(args.guarded_core_shrink_time_limit_ms, Some(5000));
        assert!(args.replay_guarded_core);
        assert_eq!(args.guarded_core_replay_time_limit_ms, Some(5000));
        assert!(args.census_guarded_core_boundary_keys);
        assert_eq!(args.guarded_core_boundary_census_time_limit_ms, Some(5000));
    }
}
