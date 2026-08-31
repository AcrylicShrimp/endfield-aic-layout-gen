use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{TransportTileCapDiagnosisReport, diagnose_cumulative_transport_tile_caps};
use anyhow::{Context, Result, ensure};

use super::dimension_sweep::{load_inputs, write_layout_html};
use super::first_phase::{write_bytes, write_json};

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    workload_path: PathBuf,
    workspace_root: PathBuf,
    placement_request_path: PathBuf,
    target_phase: usize,
    used_width: i32,
    used_height: i32,
    transport_tile_caps: Vec<u32>,
    prefix_worker_count: usize,
    prefix_case_time_limit_ms: u64,
    case_time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let prefix_worker_count = NonZeroUsize::new(prefix_worker_count)
        .context("transport tile cap prefix_worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("transport tile cap prefix budget must be positive")?;
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("transport tile cap case budget must be positive")?;
    ensure!(
        !transport_tile_caps.is_empty(),
        "transport tile cap diagnosis requires at least one --transport-tile-cap"
    );
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let report = diagnose_cumulative_transport_tile_caps(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        target_phase,
        used_width,
        used_height,
        &transport_tile_caps,
        prefix_worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(case_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("transport tile cap diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "transport tile cap diagnosis summary",
    )?;
    for case in &report.cases {
        write_layout_html(
            &output_dir.join(format!("{}.html", case_id(case.transport_tile_upper_bound))),
            &case.layout,
            loaded.localization.as_ref(),
            "transport tile cap diagnosis layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write transport tile cap diagnosis report")?;
    println!();
    Ok(true)
}

fn case_id(cap: Option<u32>) -> String {
    cap.map_or_else(|| "baseline".to_string(), |cap| format!("cap-{cap}"))
}

fn render_summary(report: &TransportTileCapDiagnosisReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let id = case_id(case.transport_tile_upper_bound);
            let label = case
                .transport_tile_upper_bound
                .map_or_else(|| "none (baseline)".to_string(), |cap| cap.to_string());
            let observed = case.observed_objective.map_or_else(
                || "-".to_string(),
                |objective| objective.physical_transport_tiles.to_string(),
            );
            format!(
                "<tr><td><a href=\"{id}.html\">{label}</a></td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{observed}</td><td>{}</td><td>{}</td></tr>",
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms
                    .map_or_else(|| "-".to_string(), |value| value.to_string()),
                case.model_scale.variables,
                case.model_scale.constraints,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Transport tile cap diagnosis</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left}}th{{background:#102535;color:#8fd9ff}}a{{color:#8fd9ff}}pre{{white-space:pre-wrap}}</style></head><body><h1>Transport tile cap diagnosis</h1><p>phase {} · exact size {}x{} · sequential cases · per-case budget {}ms · wall {}ms</p><p>Only the physical transport tile upper bound changes. Placement, rotation, ports, terminals, and routing remain solver decisions.</p><table><thead><tr><th>tile cap</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first witness ms</th><th>observed tiles</th><th>variables</th><th>constraints</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.case_search_budget_ms,
        report.outer_wall_ms,
        rows,
        json,
    ))
}
