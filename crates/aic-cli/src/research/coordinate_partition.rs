use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{
    CumulativeFacilityCoordinatePartitionReport, CumulativeFacilityPortPartitionReport,
    CumulativeFacilityRotationPartitionReport, FacilityCoordinateCaseDisposition,
    diagnose_cumulative_facility_coordinate_partitions,
    diagnose_cumulative_facility_coordinate_partitions_with_local_continuation,
    diagnose_cumulative_facility_port_partitions,
    diagnose_cumulative_facility_port_partitions_with_local_continuation,
    diagnose_cumulative_facility_rotation_partitions,
    diagnose_cumulative_facility_rotation_partitions_with_local_continuation,
};
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
    worker_count: usize,
    prefix_case_time_limit_ms: u64,
    coordinate_case_time_limit_ms: u64,
    active_local_continuation: bool,
    output_dir: PathBuf,
) -> Result<bool> {
    let worker_count = NonZeroUsize::new(worker_count)
        .context("coordinate partition worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("coordinate partition prefix_case_time_limit_ms must be positive")?;
    let coordinate_budget = NonZeroU64::new(coordinate_case_time_limit_ms)
        .context("coordinate partition coordinate_case_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let diagnose = if active_local_continuation {
        diagnose_cumulative_facility_coordinate_partitions_with_local_continuation
    } else {
        diagnose_cumulative_facility_coordinate_partitions
    };
    let report = diagnose(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        target_phase,
        used_width,
        used_height,
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(coordinate_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("coordinate partition diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "coordinate partition summary",
    )?;
    if let Some(layout) = report
        .selected_witness
        .as_ref()
        .or(report.representative_layout.as_ref())
    {
        write_layout_html(
            &output_dir.join("representative-layout.html"),
            layout,
            loaded.localization.as_ref(),
            "coordinate partition representative layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write coordinate partition report")?;
    println!();
    Ok(report.validated_witness_found)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_ports(
    workload_path: PathBuf,
    workspace_root: PathBuf,
    placement_request_path: PathBuf,
    target_phase: usize,
    used_width: i32,
    used_height: i32,
    facility_x: i32,
    facility_y: i32,
    worker_count: usize,
    prefix_case_time_limit_ms: u64,
    port_case_time_limit_ms: u64,
    active_local_continuation: bool,
    output_dir: PathBuf,
) -> Result<bool> {
    let worker_count =
        NonZeroUsize::new(worker_count).context("port partition worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("port partition prefix_case_time_limit_ms must be positive")?;
    let port_budget = NonZeroU64::new(port_case_time_limit_ms)
        .context("port partition port_case_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let diagnose = if active_local_continuation {
        diagnose_cumulative_facility_port_partitions_with_local_continuation
    } else {
        diagnose_cumulative_facility_port_partitions
    };
    let report = diagnose(
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
        worker_count.get(),
        Duration::from_millis(prefix_budget.get()),
        Duration::from_millis(port_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("port partition diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_port_summary(&report)?.as_bytes(),
        "port partition summary",
    )?;
    if let Some(layout) = report
        .selected_witness
        .as_ref()
        .or(report.representative_layout.as_ref())
    {
        write_layout_html(
            &output_dir.join("representative-layout.html"),
            layout,
            loaded.localization.as_ref(),
            "port partition representative layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write port partition report")?;
    println!();
    Ok(report.validated_witness_found)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_rotations(
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
    rotation_case_time_limit_ms: u64,
    active_local_continuation: bool,
    output_dir: PathBuf,
) -> Result<bool> {
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("rotation partition prefix_case_time_limit_ms must be positive")?;
    let rotation_budget = NonZeroU64::new(rotation_case_time_limit_ms)
        .context("rotation partition rotation_case_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let diagnose = if active_local_continuation {
        diagnose_cumulative_facility_rotation_partitions_with_local_continuation
    } else {
        diagnose_cumulative_facility_rotation_partitions
    };
    let report = diagnose(
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
        Duration::from_millis(rotation_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("rotation partition diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_rotation_summary(&report)?.as_bytes(),
        "rotation partition summary",
    )?;
    write_layout_html(
        &output_dir.join("representative-layout.html"),
        &report.representative_layout,
        loaded.localization.as_ref(),
        "rotation partition representative layout",
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write rotation partition report")?;
    println!();
    Ok(report.validated_witness_found)
}

fn render_summary(report: &CumulativeFacilityCoordinatePartitionReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let disposition = match case.disposition {
                FacilityCoordinateCaseDisposition::Executed => "executed",
                FacilityCoordinateCaseDisposition::SkippedAfterWitness => "skipped-after-witness",
            };
            format!(
                "<tr><td>{}</td><td>{},{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.coordinate_index,
                case.x,
                case.y,
                disposition,
                case.outcome,
                case.construction_ms.map_or_else(|| "-".into(), |v| v.to_string()),
                case.search_ms.map_or_else(|| "-".into(), |v| v.to_string()),
                case.first_incumbent_ms.map_or_else(|| "-".into(), |v| v.to_string()),
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Facility coordinate partition</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.ok{{color:#78f0c0}}.bad{{color:#ff719b}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}a{{color:#8fd9ff}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} facility-coordinate partition</h1><div class="meta">facility=<code>{}</code> · fixed={}×{} · coordinates={} · workers={} · wall={}ms</div><p class="{}">witness={} · all-infeasible-proven={} · unknown={} · invalid={}</p><p><a href="representative-layout.html">Open representative layout</a></p><table><thead><tr><th>#</th><th>coordinate</th><th>disposition</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first witness ms</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.partitioned_facility,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.legal_coordinate_count,
        report.actual_worker_count,
        report.outer_wall_ms,
        if report.validated_witness_found {
            "ok"
        } else {
            "bad"
        },
        report.validated_witness_found,
        report.complete_infeasibility_proven,
        report.unknown_count,
        report.invalid_witness_count,
        rows,
        json,
    ))
}

fn render_port_summary(report: &CumulativeFacilityPortPartitionReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let assignments = case
                .assignments
                .iter()
                .map(|assignment| format!("{}={}", assignment.terminal, assignment.port))
                .collect::<Vec<_>>()
                .join("<br>");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.assignment_index,
                assignments,
                case.outcome,
                case.construction_ms
                    .map_or_else(|| "-".into(), |v| v.to_string()),
                case.search_ms.map_or_else(|| "-".into(), |v| v.to_string()),
                case.first_incumbent_ms
                    .map_or_else(|| "-".into(), |v| v.to_string()),
            )
        })
        .collect::<String>();
    let domains = report
        .terminal_domains
        .iter()
        .map(|domain| {
            format!(
                "<li><code>{}</code>: {}</li>",
                domain.terminal,
                domain.ports.join(", ")
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Facility port partition</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.ok{{color:#78f0c0}}.bad{{color:#ff719b}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}a{{color:#8fd9ff}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} facility-port partition</h1><div class="meta">facility=<code>{}</code> · fixed={}×{} · coordinate={},{} · assignments={} · workers={} · wall={}ms</div><p class="{}">witness={} · all-infeasible-proven={} · unknown={} · invalid={}</p><h2>Port domains</h2><ul>{}</ul><p><a href="representative-layout.html">Open representative layout</a></p><table><thead><tr><th>#</th><th>assignments</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first witness ms</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.partitioned_facility,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.fixed_coordinate[0],
        report.fixed_coordinate[1],
        report.legal_assignment_count,
        report.actual_worker_count,
        report.outer_wall_ms,
        if report.validated_witness_found {
            "ok"
        } else {
            "bad"
        },
        report.validated_witness_found,
        report.complete_infeasibility_proven,
        report.unknown_count,
        report.invalid_witness_count,
        domains,
        rows,
        json,
    ))
}

fn render_rotation_summary(report: &CumulativeFacilityRotationPartitionReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.rotation,
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms
                    .map_or_else(|| "-".into(), |v| v.to_string()),
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Facility rotation partition</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}a{{color:#8fd9ff}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} facility-rotation partition</h1><div class="meta">facility=<code>{}</code> · fixed={}×{} · coordinate={},{} · port assignment={} · wall={}ms</div><p>witness={} · all-infeasible-proven={} · unknown={}</p><p><a href="representative-layout.html">Open representative layout</a></p><table><thead><tr><th>rotation</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first witness ms</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.partitioned_facility,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.fixed_coordinate[0],
        report.fixed_coordinate[1],
        report.fixed_port_assignment_index,
        report.outer_wall_ms,
        report.validated_witness_found,
        report.complete_infeasibility_proven,
        report.unknown_count,
        rows,
        json,
    ))
}
