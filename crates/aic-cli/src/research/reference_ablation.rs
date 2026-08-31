use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{
    Phase2ReferenceAblationKind, Phase2ReferenceAblationReport, diagnose_phase2_reference_ablation,
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
        .context("reference ablation prefix budget must be positive")?;
    let reference_budget = NonZeroU64::new(reference_time_limit_ms)
        .context("reference ablation reference budget must be positive")?;
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("reference ablation case budget must be positive")?;
    let loaded = load_inputs(workload_path, workspace_root, placement_request_path)?;
    let report = diagnose_phase2_reference_ablation(
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
    .map_err(|report| anyhow::anyhow!("reference ablation failed: {report:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "reference ablation summary",
    )?;
    for case in &report.cases {
        write_layout_html(
            &output_dir.join(format!("{}.html", kind_id(case.kind))),
            &case.layout,
            loaded.localization.as_ref(),
            "reference ablation layout",
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write reference ablation report")?;
    println!();
    Ok(report.cases.iter().all(|case| case.layout.success))
}

fn kind_id(kind: Phase2ReferenceAblationKind) -> &'static str {
    match kind {
        Phase2ReferenceAblationKind::Placements => "placements",
        Phase2ReferenceAblationKind::PlacementsAndFacilityPorts => "placements-facility-ports",
        Phase2ReferenceAblationKind::PlacementsAndAllTerminals => "placements-all-terminals",
    }
}

fn render_summary(report: &Phase2ReferenceAblationReport) -> Result<String> {
    let rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td><a href=\"{}.html\">{}</a></td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                kind_id(case.kind),
                kind_id(case.kind),
                case.outcome,
                case.construction_ms,
                case.search_ms,
                case.first_incumbent_ms.map_or_else(|| "-".into(), |v| v.to_string()),
            )
        })
        .collect::<String>();
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Phase 2 reference ablation</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left}}th{{background:#102535;color:#8fd9ff}}a{{color:#8fd9ff}}pre{{white-space:pre-wrap}}</style></head><body><h1>Phase 2 reference ablation</h1><p>reference first witness={}ms · parallel case wall={}ms · per-case budget={}ms</p><table><thead><tr><th>fixation</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first witness ms</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.reference_search_ms, report.outer_wall_ms, report.case_search_budget_ms, rows, json,
    ))
}
