use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{
    RouteCellBreakdownReport, RoutingStateBreakdownReport, diagnose_phase2_route_cell_breakdown,
    diagnose_phase2_routing_state_breakdown,
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
    port_assignment_index: usize,
    prefix_case_time_limit_ms: u64,
    reference_time_limit_ms: u64,
    case_time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let prefix_budget = NonZeroU64::new(prefix_case_time_limit_ms)
        .context("routing state breakdown prefix budget must be positive")?;
    let reference_budget = NonZeroU64::new(reference_time_limit_ms)
        .context("routing state breakdown reference budget must be positive")?;
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("routing state breakdown case budget must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let report = diagnose_phase2_routing_state_breakdown(
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
    .map_err(|report| anyhow::anyhow!("routing state breakdown failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "routing state breakdown summary",
    )?;
    for case in &report.cases {
        write_layout_html(
            &output_dir.join(format!("{}.html", case.id)),
            &case.layout,
            loaded.localization.as_ref(),
            "routing state breakdown layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write routing state breakdown report")?;
    println!();
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_route_cells(
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
        .context("route-cell breakdown prefix budget must be positive")?;
    let reference_budget = NonZeroU64::new(reference_time_limit_ms)
        .context("route-cell breakdown reference budget must be positive")?;
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("route-cell breakdown case budget must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let report = diagnose_phase2_route_cell_breakdown(
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
    .map_err(|report| anyhow::anyhow!("route-cell breakdown failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_route_cell_summary(&report)?.as_bytes(),
        "route-cell breakdown summary",
    )?;
    for case in &report.cases {
        write_layout_html(
            &output_dir.join(format!("{}.html", case.id)),
            &case.layout,
            loaded.localization.as_ref(),
            "route-cell breakdown layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write route-cell breakdown report")?;
    println!();
    Ok(true)
}

fn render_summary(report: &RoutingStateBreakdownReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let families = case
                .fixed_families
                .iter()
                .map(|family| format!("{family:?}"))
                .collect::<Vec<_>>()
                .join(" + ");
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
            format!(
                "<tr><td><a href=\"{}.html\">{}</a></td><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.id,
                case.id,
                case.matrix,
                if families.is_empty() { "-" } else { &families },
                case.added_routing_fixation_equalities,
                case.outcome,
                case.search_ms,
                case.first_incumbent_ms
                    .map_or_else(|| "-".to_string(), |value| value.to_string()),
                observed,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 2 routing state breakdown</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left}}th{{background:#102535;color:#8fd9ff}}a{{color:#8fd9ff}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase 2 routing state breakdown</h1><p>exact size {}x{} · reference={}ms · per-case budget={}ms · sequential wall={}ms</p><table><thead><tr><th>case</th><th>matrix</th><th>fixed routing state</th><th>added equalities</th><th>outcome</th><th>search ms</th><th>first witness ms</th><th>objective A/T/R/S/C</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.reference_search_ms,
        report.case_search_budget_ms,
        report.outer_wall_ms,
        rows,
        json,
    ))
}

fn render_route_cell_summary(report: &RouteCellBreakdownReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let cell = case
                .cell
                .as_ref()
                .map_or_else(|| "-".to_string(), |cell| format!("{},{}", cell.x, cell.y));
            let observed = case.observed_objective.map_or_else(
                || "-".to_string(),
                |objective| objective.physical_transport_tiles.to_string(),
            );
            format!(
                "<tr><td><a href=\"{}.html\">{}</a></td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.id,
                case.id,
                case.layer,
                case.value,
                case.network_id.as_deref().unwrap_or("-"),
                cell,
                case.added_route_cell_equalities,
                case.outcome,
                case.search_ms,
                case.first_incumbent_ms
                    .map_or_else(|| "-".to_string(), |value| value.to_string()),
                observed,
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 2 route-cell breakdown</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left}}th{{background:#102535;color:#8fd9ff}}a{{color:#8fd9ff}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase 2 route-cell breakdown</h1><p>exact size {}x{} · reference={}ms · per-case budget={}ms · sequential wall={}ms</p><table><thead><tr><th>case</th><th>layer</th><th>value</th><th>network</th><th>cell</th><th>added equalities</th><th>outcome</th><th>search ms</th><th>first witness ms</th><th>observed tiles</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.fixed_dimensions.width,
        report.fixed_dimensions.height,
        report.reference_search_ms,
        report.case_search_budget_ms,
        report.outer_wall_ms,
        rows,
        json,
    ))
}
