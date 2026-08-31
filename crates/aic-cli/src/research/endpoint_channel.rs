use std::path::PathBuf;

use aic_data::layouts::{render_endpoint_channel_probe_html, run_endpoint_channel_probe};
use anyhow::{Context, Result};

use super::first_phase::{write_bytes, write_json};

pub(super) fn run(output: PathBuf, visualization_output: PathBuf) -> Result<bool> {
    let report = run_endpoint_channel_probe();
    let html = render_endpoint_channel_probe_html(&report)
        .context("failed to render endpoint channel propagation HTML")?;
    write_json(&output, &report)?;
    write_bytes(
        &visualization_output,
        html.as_bytes(),
        "endpoint channel propagation visualization",
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write endpoint channel propagation report")?;
    println!();
    Ok(true)
}
