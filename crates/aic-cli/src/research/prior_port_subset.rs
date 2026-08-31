use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{PriorPortSubsetAblationReport, diagnose_prior_port_subset_ablation};
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
    if !matches!(endpoint_encoding, EndpointChannelEncodingArg::SparseSupport) {
        anyhow::bail!("prior-port subset ablation requires --endpoint-encoding sparse-support");
    }
    let worker_count = NonZeroUsize::new(worker_count)
        .context("prior-port subset worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("prior-port subset prefix_case_time_limit_ms must be positive")?;
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("prior-port subset state_case_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let report = diagnose_prior_port_subset_ablation(
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
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(case_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("prior-port subset diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "prior-port subset summary",
    )?;
    for case in &report.cases {
        write_layout_html(
            &output_dir.join(format!("case.mask-{:02x}.html", case.facility_mask)),
            &case.layout,
            loaded.localization.as_ref(),
            "prior-port subset case",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write prior-port subset report")?;
    println!();
    Ok(report.cases.iter().any(|case| case.layout.success))
}

fn render_summary(report: &PriorPortSubsetAblationReport) -> Result<String> {
    let mapping = report
        .prior_facilities
        .iter()
        .map(|facility| {
            format!(
                "<li>bit {}: <code>{}</code> ({} terminals)</li>",
                facility.bit_index, facility.instance, facility.matching_terminal_count
            )
        })
        .collect::<String>();
    let rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>0x{:02x}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td><td>{:?}</td></tr>",
                case.facility_mask,
                case.selected_facilities.join("<br>"),
                case.fixed_terminal_count,
                case.outcome,
                case.added_constraint_count_from_no_ports,
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
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 3 prior-port subset diagnosis</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} prior-port subset diagnosis</h1><div class="meta">facility=<code>{}</code> · fixed={}x{} · coordinate={},{} · assignment={} · rotation={} · workers={} · wall={}ms</div><h2>Stable bit mapping</h2><ul>{}</ul><table><thead><tr><th>mask</th><th>fixed facilities</th><th>fixed terminals</th><th>outcome</th><th>added constraints</th><th>build ms</th><th>search ms</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.partitioned_facility,
        report.fixed_dimensions[0],
        report.fixed_dimensions[1],
        report.fixed_coordinate[0],
        report.fixed_coordinate[1],
        report.port_assignment_index,
        report.fixed_rotation,
        report.worker_count,
        report.outer_wall_ms,
        mapping,
        rows,
        json,
    ))
}
