use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{
    EndpointChannelEncoding, IntegratedEndpointChannelCaseReport,
    run_integrated_endpoint_channel_case,
};
use anyhow::{Context, Result};

use super::EndpointChannelEncodingArg;
use super::dimension_sweep::{load_inputs, write_layout_html};
use super::first_phase::{write_bytes, write_json};

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    workload: PathBuf,
    workspace_root: PathBuf,
    placement_request: PathBuf,
    target_phase: usize,
    used_width: i32,
    used_height: i32,
    encoding: EndpointChannelEncodingArg,
    track_row_selectors: bool,
    case_time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let budget = NonZeroU64::new(case_time_limit_ms)
        .context("integrated endpoint-channel case_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload, workspace_root, placement_request)?;
    let encoding = match encoding {
        EndpointChannelEncodingArg::NestedElement => EndpointChannelEncoding::NestedElement,
        EndpointChannelEncodingArg::PositiveTable => EndpointChannelEncoding::PositiveTable,
    };
    let report = run_integrated_endpoint_channel_case(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        target_phase,
        used_width,
        used_height,
        encoding,
        track_row_selectors,
        Duration::from_millis(budget.get()),
    )
    .map_err(|layout| anyhow::anyhow!("integrated endpoint-channel case failed: {layout:?}"))?;
    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "integrated endpoint-channel comparison summary",
    )?;
    write_layout_html(
        &output_dir.join("layout.html"),
        &report.layout,
        loaded.localization.as_ref(),
        "integrated endpoint-channel comparison layout",
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write integrated endpoint-channel report")?;
    println!();
    Ok(true)
}

fn render_summary(report: &IntegratedEndpointChannelCaseReport) -> Result<String> {
    let exact = report
        .layout
        .exact
        .as_ref()
        .expect("integrated comparison report has exact metrics");
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Integrated endpoint-channel comparison</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse}}th,td{{border:1px solid #315066;padding:8px;text-align:left}}th{{background:#102535;color:#8fd9ff}}code{{color:#ffd166}}a{{color:#8fd9ff}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Integrated endpoint-channel comparison</h1><div class="meta">encoding=<code>{:?}</code> · tracked={} · fixed={}×{} · outcome={:?} · formulation=<code>{}</code></div><p><a href="layout.html">Open automatic layout/failure view</a></p><table><tr><th>endpoint relations</th><td>{}</td></tr><tr><th>legal relation rows</th><td>{}</td></tr><tr><th>encoding-specific hidden row literals</th><td>{}</td></tr><tr><th>encoding-specific generated clauses</th><td>{}</td></tr><tr><th>build ms</th><td>{}</td></tr><tr><th>search ms</th><td>{}</td></tr><tr><th>first incumbent</th><td>{}</td></tr><tr><th>decisions</th><td>{:?}</td></tr><tr><th>backtracks</th><td>{:?}</td></tr><tr><th>conflicts</th><td>{:?}</td></tr><tr><th>learned clauses</th><td>{:?}</td></tr><tr><th>registered propagator calls</th><td>{:?}</td></tr><tr><th>row selectors (root unfixed / total)</th><td>{:?} / {:?}</td></tr><tr><th>row-selector decisions</th><td>{:?}</td></tr><tr><th>non-row decisions</th><td>{:?}</td></tr><tr><th>row decisions (true / false / unclassified)</th><td>{:?} / {:?} / {:?}</td></tr><tr><th>maximum consecutive row decisions</th><td>{:?}</td></tr><tr><th>row predicates in conflict analysis</th><td>{:?}</td></tr></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.encoding,
        report.row_selector_tracking,
        report.fixed_dimensions[0],
        report.fixed_dimensions[1],
        report.outcome,
        exact.formulation,
        report.endpoint_table_scale.facility_endpoint_tables,
        report.endpoint_table_scale.legal_tuple_rows,
        report.endpoint_table_scale.estimated_hidden_row_literals,
        report.endpoint_table_scale.estimated_table_clauses,
        report.construction_ms,
        report.search_ms,
        report
            .first_incumbent_ms
            .map_or_else(|| "-".to_string(), |value| value.to_string()),
        report.search_statistics.branch_decisions,
        report.search_statistics.backtracks,
        report.search_statistics.conflicts,
        report.search_statistics.learned_clauses,
        report.search_statistics.solver_propagations,
        report.search_statistics.row_selector_root_unfixed,
        report.search_statistics.row_selector_total,
        report.search_statistics.row_selector_decisions,
        report.search_statistics.non_row_selector_decisions,
        report.search_statistics.row_selector_true_decisions,
        report.search_statistics.row_selector_false_decisions,
        report.search_statistics.row_selector_unclassified_decisions,
        report
            .search_statistics
            .maximum_consecutive_row_selector_decisions,
        report.search_statistics.row_selector_conflict_appearances,
        json,
    ))
}
