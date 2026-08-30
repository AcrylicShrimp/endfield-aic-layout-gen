use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    FacilityPlacementRequest, compare_first_integrated_layout_phase_shared_layer,
    render_integrated_layout_html_with_localization,
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

pub(super) fn run(
    workload_path: PathBuf,
    workspace_root: PathBuf,
    placement_request_path: PathBuf,
    time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let time_limit = NonZeroU64::new(time_limit_ms)
        .context("research shared-layer time_limit_ms must be positive")?;
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

    let report = compare_first_integrated_layout_phase_shared_layer(
        &wiring,
        &facilities,
        &items,
        &transports,
        &components,
        &placement_request,
        Duration::from_millis(time_limit.get()),
    )
    .map_err(|layout| anyhow::anyhow!("failed to prepare shared-layer comparison: {layout:?}"))?;
    write_json(&output_dir.join("comparison.json"), &report)?;
    let localization = load_localization(paths.localization_catalog.as_ref())?;
    for (name, layout) in [
        ("dense", &report.dense),
        ("shared-layer", &report.shared_layer),
    ] {
        let html = render_integrated_layout_html_with_localization(layout, localization.as_ref())
            .map_err(|diagnostic| {
            anyhow::anyhow!(
                "{name} visualization failed with {}: {}",
                diagnostic.code,
                diagnostic.message
            )
        })?;
        write_bytes(
            &output_dir.join(format!("{name}.html")),
            html.as_bytes(),
            "shared-layer comparison visualization",
        )?;
    }
    Ok(report.shared_layer.success)
}
