use std::path::PathBuf;

use aic_data::layouts::{
    EndpointChannelEncoding, render_scaled_endpoint_channel_probe_html,
    run_scaled_endpoint_channel_probe,
};
use anyhow::{Context, Result};

use super::EndpointChannelEncodingArg;
use super::dimension_sweep::load_inputs;
use super::first_phase::{write_bytes, write_json};

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    workload: PathBuf,
    workspace_root: PathBuf,
    placement_request: PathBuf,
    target_phase: usize,
    encoding: EndpointChannelEncodingArg,
    output: PathBuf,
    visualization_output: PathBuf,
) -> Result<bool> {
    let loaded = load_inputs(workload, workspace_root, placement_request)?;
    let encoding = match encoding {
        EndpointChannelEncodingArg::NestedElement => EndpointChannelEncoding::NestedElement,
        EndpointChannelEncodingArg::PositiveTable => EndpointChannelEncoding::PositiveTable,
        EndpointChannelEncodingArg::SparseSupport => EndpointChannelEncoding::SparseSupport,
    };
    let report = run_scaled_endpoint_channel_probe(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        target_phase,
        encoding,
    )
    .map_err(|diagnostic| {
        anyhow::anyhow!(
            "scaled endpoint-channel probe failed with {}: {}",
            diagnostic.code,
            diagnostic.message
        )
    })?;
    let html = render_scaled_endpoint_channel_probe_html(&report)
        .context("failed to render scaled endpoint-channel HTML")?;
    write_json(&output, &report)?;
    write_bytes(
        &visualization_output,
        html.as_bytes(),
        "scaled endpoint-channel visualization",
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write scaled endpoint-channel report")?;
    println!();
    Ok(true)
}
