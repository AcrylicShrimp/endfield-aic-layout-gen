use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    FacilityPlacementRequest, IntegratedLayoutReport,
    render_integrated_layout_html_with_localization,
    solve_first_integrated_layout_phase_with_time_limit,
};
use aic_data::localization::{ValidatedLocalizationCatalog, load_localization_catalog};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    build_contextual_facility_instance_wiring, calculate_contextual_facility_requirements,
};
use aic_data::research::{
    BenchmarkRequestBounds, ValidatedBenchmarkWorkloadManifest, load_benchmark_workload_manifest,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;

use super::{load_contextual_recipe_request, resolve_workload_paths};

const FIRST_PHASE_EXPERIMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
struct FirstPhaseExperimentReport<'a> {
    schema_version: u32,
    workload_id: &'a str,
    request_bounds: BenchmarkRequestBounds,
    search_budget_ms: u64,
    layout: &'a IntegratedLayoutReport,
}

pub(super) fn solve(
    workload_path: PathBuf,
    workspace_root: PathBuf,
    placement_request_path: PathBuf,
    time_limit_ms: u64,
    output: PathBuf,
    visualization_output: Option<PathBuf>,
) -> Result<bool> {
    let time_limit = NonZeroU64::new(time_limit_ms)
        .context("research first-phase time_limit_ms must be positive")?;
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

    let layout = solve_first_integrated_layout_phase_with_time_limit(
        &wiring,
        &facilities,
        &items,
        &transports,
        &components,
        &placement_request,
        Duration::from_millis(time_limit.get()),
    );
    let report = FirstPhaseExperimentReport {
        schema_version: FIRST_PHASE_EXPERIMENT_SCHEMA_VERSION,
        workload_id: &manifest.id,
        request_bounds: BenchmarkRequestBounds {
            max_width: u32::try_from(placement_request.max_width)
                .context("research max_width does not fit report domain")?,
            max_height: u32::try_from(placement_request.max_height)
                .context("research max_height does not fit report domain")?,
        },
        search_budget_ms: time_limit.get(),
        layout: &layout,
    };
    write_json(&output, &report)?;
    if let Some(path) = visualization_output {
        let localization = load_localization(paths.localization_catalog.as_ref())?;
        let html = render_integrated_layout_html_with_localization(&layout, localization.as_ref())
            .map_err(|diagnostic| {
                anyhow::anyhow!(
                    "layout visualization failed with {}: {}",
                    diagnostic.code,
                    diagnostic.message
                )
            })?;
        write_bytes(&path, html.as_bytes(), "visualization")?;
    }
    Ok(layout.success)
}

fn load_localization(path: Option<&PathBuf>) -> Result<Option<ValidatedLocalizationCatalog>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let catalog = load_localization_catalog(path).with_context(|| {
        format!(
            "failed to load research localization catalog '{}'",
            path.display()
        )
    })?;
    ValidatedLocalizationCatalog::try_from_catalog(catalog)
        .map(Some)
        .map_err(|report| anyhow::anyhow!("localization catalog validation failed: {report:?}"))
}

fn write_json(path: &PathBuf, report: &impl Serialize) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(report)
        .context("failed to serialize first-phase experiment report")?;
    write_bytes(path, &encoded, "first-phase experiment")
}

fn write_bytes(path: &PathBuf, bytes: &[u8], kind: &str) -> Result<()> {
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
