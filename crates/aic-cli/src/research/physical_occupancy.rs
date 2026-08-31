use std::path::PathBuf;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    FacilityPlacementRequest, render_physical_occupancy_probe_html, run_physical_occupancy_probe,
};
use anyhow::{Context, Result};

use super::first_phase::{write_bytes, write_json};

pub(super) fn run(
    facility_catalog_path: PathBuf,
    facility_id: String,
    placement_request_path: PathBuf,
    output: PathBuf,
    visualization_output: PathBuf,
) -> Result<bool> {
    let facilities =
        ValidatedFacilityCatalog::try_from_catalog(load_facility_catalog(&facility_catalog_path)?)
            .map_err(|report| anyhow::anyhow!("facility catalog validation failed: {report:?}"))?;
    let request_json = std::fs::read_to_string(&placement_request_path).with_context(|| {
        format!(
            "failed to read physical occupancy probe request '{}'",
            placement_request_path.display()
        )
    })?;
    let request =
        serde_json::from_str::<FacilityPlacementRequest>(&request_json).with_context(|| {
            format!(
                "failed to parse physical occupancy probe request '{}'",
                placement_request_path.display()
            )
        })?;
    let report = run_physical_occupancy_probe(&facilities, &facility_id, &request)
        .map_err(anyhow::Error::msg)?;
    let html = render_physical_occupancy_probe_html(&report)
        .context("failed to render physical occupancy propagation HTML")?;
    write_json(&output, &report)?;
    write_bytes(
        &visualization_output,
        html.as_bytes(),
        "physical occupancy propagation visualization",
    )?;
    Ok(true)
}
