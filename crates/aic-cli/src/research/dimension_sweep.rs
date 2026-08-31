use std::io::Write;
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    ExactDimensionCaseDisposition, FacilityPlacementRequest,
    render_integrated_layout_html_with_localization,
    sweep_first_integrated_layout_phase_fixed_dimensions,
};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    build_contextual_facility_instance_wiring, calculate_contextual_facility_requirements,
};
use aic_data::research::{ValidatedBenchmarkWorkloadManifest, load_benchmark_workload_manifest};
use anyhow::{Context, Result, ensure};

use super::first_phase::{load_localization, write_bytes, write_json};
use super::{load_contextual_recipe_request, resolve_workload_paths};

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    workload_path: PathBuf,
    workspace_root: PathBuf,
    placement_request_path: PathBuf,
    network_indices: Vec<usize>,
    worker_count: usize,
    case_time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let worker_count = NonZeroUsize::new(worker_count)
        .context("parallel dimension sweep worker_count must be positive")?;
    let case_time_limit = NonZeroU64::new(case_time_limit_ms)
        .context("parallel dimension sweep case_time_limit_ms must be positive")?;
    ensure!(
        !network_indices.is_empty(),
        "parallel dimension sweep requires at least one --network-index"
    );
    let manifest = load_benchmark_workload_manifest(&workload_path)?;
    let validated = ValidatedBenchmarkWorkloadManifest::try_from_manifest(manifest)
        .map_err(|report| anyhow::anyhow!("benchmark workload validation failed: {report:?}"))?;
    let manifest = validated.manifest();
    let paths = resolve_workload_paths(&workspace_root, &manifest.inputs);
    let (book, source_plan) = load_contextual_recipe_request(&paths.recipes, &paths.source_plan)?;
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
    let placement_request_path = workspace_root.join(placement_request_path);
    let placement_request_json =
        std::fs::read_to_string(&placement_request_path).with_context(|| {
            format!(
                "failed to read research placement request '{}'",
                placement_request_path.display()
            )
        })?;
    let placement_request = serde_json::from_str::<FacilityPlacementRequest>(
        &placement_request_json,
    )
    .with_context(|| {
        format!(
            "failed to parse research placement request '{}'",
            placement_request_path.display()
        )
    })?;

    let report = sweep_first_integrated_layout_phase_fixed_dimensions(
        &wiring,
        &facilities,
        &items,
        &transports,
        &components,
        &placement_request,
        &network_indices,
        worker_count.get(),
        Duration::from_millis(case_time_limit.get()),
    )
    .map_err(|report| anyhow::anyhow!("parallel dimension sweep failed: {report:?}"))?;

    let localization = load_localization(paths.localization_catalog.as_ref())?;
    for case in &report.cases {
        if case.disposition != ExactDimensionCaseDisposition::Executed {
            continue;
        }
        write_json(
            &output_dir.join(format!(
                "case.{}x{}.json",
                case.candidate.width, case.candidate.height
            )),
            case,
        )?;
        let layout = case
            .layout
            .as_ref()
            .expect("executed dimension case has a layout report");
        let html = render_integrated_layout_html_with_localization(layout, localization.as_ref())
            .map_err(|diagnostic| {
            anyhow::anyhow!(
                "parallel dimension case visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &output_dir.join(format!(
                "case.{}x{}.html",
                case.candidate.width, case.candidate.height
            )),
            html.as_bytes(),
            "parallel dimension case visualization",
        )?;
    }
    let summary_layout = report
        .selected_incumbent
        .as_ref()
        .or_else(|| report.cases.iter().find_map(|case| case.layout.as_ref()));
    if let Some(layout) = summary_layout {
        let html = render_integrated_layout_html_with_localization(layout, localization.as_ref())
            .map_err(|diagnostic| {
            anyhow::anyhow!(
                "parallel dimension summary visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &output_dir.join("summary.html"),
            html.as_bytes(),
            "parallel dimension summary visualization",
        )?;
    }
    write_json(&output_dir.join("summary.json"), &report)?;
    let encoded = serde_json::to_vec_pretty(&report)
        .context("failed to serialize parallel dimension sweep report")?;
    std::io::stdout()
        .lock()
        .write_all(&encoded)
        .context("failed to write parallel dimension sweep report")?;
    println!();
    Ok(true)
}
