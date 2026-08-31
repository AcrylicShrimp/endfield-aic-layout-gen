use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{ConnectivityWitnessDiagnosisReport, diagnose_phase2_connectivity_witness};
use anyhow::{Context, Result};

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
    facility_x: i32,
    facility_y: i32,
    port_assignment_index: usize,
    prefix_case_time_limit_ms: u64,
    reference_time_limit_ms: u64,
    case_time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("connectivity witness prefix budget must be positive")?;
    let reference_budget = NonZeroU64::new(reference_time_limit_ms)
        .context("connectivity witness reference budget must be positive")?;
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("connectivity witness case budget must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let report = diagnose_phase2_connectivity_witness(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        target_phase,
        used_width,
        used_height,
        facility_x,
        facility_y,
        port_assignment_index,
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(reference_budget.get()),
        Duration::from_millis(case_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("connectivity witness diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "connectivity witness diagnosis summary",
    )?;
    for case in &report.cases {
        let name = format!("{:?}", case.kind).to_lowercase();
        write_layout_html(
            &output_dir.join(format!("{name}.html")),
            &case.layout,
            loaded.localization.as_ref(),
            "connectivity witness layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write connectivity witness diagnosis")?;
    println!();
    Ok(true)
}

fn render_summary(report: &ConnectivityWitnessDiagnosisReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let name = format!("{:?}", case.kind).to_lowercase();
            let observed = case.observed_objective.map_or_else(
                || "-".to_string(),
                |objective| {
                    format!(
                        "{} / {} / {} / {} / {}",
                        objective.used_bounding_box_area,
                        objective.physical_transport_tiles,
                        objective.total_route_turns,
                        objective.maximum_used_side,
                        objective.logistics_component_count,
                    )
                },
            );
            let state = case.witness_state;
            format!(
                "<tr><td><a href=\"{name}.html\">{:?}</a></td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}/{}/{}/{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.kind,
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms
                    .map_or_else(|| "-".to_string(), |value| value.to_string()),
                state.reachability_variables,
                state.root_variables,
                state.parent_variables,
                state.depth_variables,
                state.constraints,
                case.model_scale.variables,
                observed,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 2 connectivity witness diagnosis</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left}}th{{background:#102535;color:#8fd9ff}}a{{color:#8fd9ff}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase 2 connectivity witness diagnosis</h1><p>exact size {}x{} · reference={}ms · per-case budget={}ms · sequential wall={}ms</p><table><thead><tr><th>case</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first witness ms</th><th>reach/root/parent/depth vars</th><th>witness constraints</th><th>total vars</th><th>objective A/T/R/S/C</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.reference_search_ms,
        report.case_search_budget_ms,
        report.outer_wall_ms,
        rows,
        json,
    ))
}
