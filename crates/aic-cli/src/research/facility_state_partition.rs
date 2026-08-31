use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{
    CumulativeFacilityStatePartitionReport, FacilityCoordinateCaseDisposition,
    diagnose_cumulative_facility_state_partitions_with_local_continuation,
    diagnose_cumulative_facility_state_partitions_with_prior_overlap_facility_state,
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
    facility_x: i32,
    facility_y: i32,
    worker_count: usize,
    prefix_case_time_limit_ms: u64,
    state_case_time_limit_ms: u64,
    fix_prior_overlap_facility_state: bool,
    output_dir: PathBuf,
) -> Result<bool> {
    let worker_count = NonZeroUsize::new(worker_count)
        .context("facility state partition worker_count must be positive")?;
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("facility state partition prefix_case_time_limit_ms must be positive")?;
    let state_budget = NonZeroU64::new(state_case_time_limit_ms)
        .context("facility state partition state_case_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let diagnose = if fix_prior_overlap_facility_state {
        diagnose_cumulative_facility_state_partitions_with_prior_overlap_facility_state
    } else {
        diagnose_cumulative_facility_state_partitions_with_local_continuation
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
        Duration::from_millis(state_budget.get()),
    )
    .map_err(|report| anyhow::anyhow!("facility state partition diagnosis failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "facility state partition summary",
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
            "facility state partition representative layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write facility state partition report")?;
    println!();
    Ok(report.validated_witness_found)
}

fn render_summary(report: &CumulativeFacilityStatePartitionReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let disposition = match case.disposition {
                FacilityCoordinateCaseDisposition::Executed => "executed",
                FacilityCoordinateCaseDisposition::SkippedAfterWitness => "skipped",
            };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.assignment_index,
                case.rotation,
                disposition,
                case.outcome,
                case.construction_ms.map_or_else(|| "-".into(), |value| value.to_string()),
                case.search_ms.map_or_else(|| "-".into(), |value| value.to_string()),
                case.first_incumbent_ms.map_or_else(|| "-".into(), |value| value.to_string()),
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Facility state partition</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.ok{{color:#78f0c0}}.bad{{color:#ff719b}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}a{{color:#8fd9ff}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase {} facility-state exact portfolio</h1><div class="meta">facility=<code>{}</code> · fixed={}×{} · coordinate={},{} · prior-state-fixed={} · assignments={} · rotations={} · cases={} · workers={} · wall={}ms</div><p class="{}">witness={} · all-infeasible-proven={} · unknown={} · invalid={}</p><p><a href="representative-layout.html">Open representative layout</a></p><table><thead><tr><th>port assignment</th><th>rotation</th><th>disposition</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first witness ms</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.target_phase_index,
        report.partitioned_facility,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.fixed_coordinate[0],
        report.fixed_coordinate[1],
        report.prior_overlap_facility_state_fixed,
        report.port_assignments.len(),
        report.legal_rotations.len(),
        report.legal_state_count,
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
