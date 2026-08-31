use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{
    EndpointChannelEncoding, ResidualFacilityStateAblationReport,
    diagnose_residual_facility_state_ablation,
};
use anyhow::{Context, Result};

use super::EndpointChannelEncodingArg;
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
    facility_rotation: i64,
    endpoint_encoding: EndpointChannelEncodingArg,
    worker_count: usize,
    prefix_case_time_limit_ms: u64,
    case_time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let worker_count = NonZeroUsize::new(worker_count)
        .context("residual facility-state worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("residual facility-state prefix_case_time_limit_ms must be positive")?;
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("residual facility-state case_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let endpoint_encoding = match endpoint_encoding {
        EndpointChannelEncodingArg::NestedElement => EndpointChannelEncoding::NestedElement,
        EndpointChannelEncodingArg::SparseSupport => EndpointChannelEncoding::SparseSupport,
        EndpointChannelEncodingArg::PositiveTable => {
            anyhow::bail!("residual facility-state diagnosis does not support positive-table")
        }
    };
    let report = diagnose_residual_facility_state_ablation(
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
        facility_rotation,
        endpoint_encoding,
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(case_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("residual facility-state diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "residual facility-state summary",
    )?;
    for case in &report.cases {
        write_layout_html(
            &output_dir.join(format!("case.{:?}.html", case.kind).to_ascii_lowercase()),
            &case.layout,
            loaded.localization.as_ref(),
            "residual facility-state case",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write residual facility-state report")?;
    println!();
    Ok(report.cases.iter().any(|case| case.layout.success))
}

fn render_summary(report: &ResidualFacilityStateAblationReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td></tr>",
                case.kind,
                case.outcome,
                case.added_constraint_count_from_baseline,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms.map_or_else(|| "-".into(), |value| value.to_string()),
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
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Residual facility-state ablation</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} residual facility-state ablation</h1><div class="meta">facility=<code>{}</code> · encoding={:?} · fixed={}x{} · coordinate={},{} · assignment={} · rotation={} · prior placements={} · prior facility terminals={} · wall={}ms</div><table><thead><tr><th>case</th><th>outcome</th><th>added constraints</th><th>build ms</th><th>search ms</th><th>first witness ms</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.partitioned_facility,
        report.endpoint_encoding,
        report.fixed_dimensions[0],
        report.fixed_dimensions[1],
        report.fixed_coordinate[0],
        report.fixed_coordinate[1],
        report.port_assignment_index,
        report.fixed_rotation,
        report.prior_placement_count,
        report.prior_facility_terminal_count,
        report.outer_wall_ms,
        rows,
        json,
    ))
}
