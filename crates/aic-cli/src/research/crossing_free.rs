use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Duration;

use aic_data::layouts::{
    CrossingFreeRestrictionExperimentReport, CrossingRestrictionCaseReport,
    diagnose_crossing_free_restriction,
};
use anyhow::{Context, Result};

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
    case_time_limit_ms: u64,
    observation_time_limit_ms: u64,
    output_dir: PathBuf,
) -> Result<bool> {
    let case_budget = NonZeroU64::new(case_time_limit_ms)
        .context("crossing-free case_time_limit_ms must be positive")?;
    let observation_budget = NonZeroU64::new(observation_time_limit_ms)
        .context("crossing-free observation_time_limit_ms must be positive")?;
    let loaded = load_inputs(workload, workspace_root, placement_request)?;
    let report = diagnose_crossing_free_restriction(
        &loaded.wiring,
        &loaded.facilities,
        &loaded.items,
        &loaded.transports,
        &loaded.components,
        &loaded.placement_request,
        target_phase,
        used_width,
        used_height,
        Duration::from_millis(case_budget.get()),
        Duration::from_millis(observation_budget.get()),
    )
    .map_err(|layout| anyhow::anyhow!("crossing-free restriction experiment failed: {layout:?}"))?;

    write_json(&output_dir.join("summary.json"), &report)?;
    write_bytes(
        &output_dir.join("summary.html"),
        render_summary(&report)?.as_bytes(),
        "crossing-free restriction summary",
    )?;
    for case in &report.cases {
        write_case_layout(&output_dir, case, loaded.localization.as_ref())?;
    }
    if let Some(observation) = &report.crossing_free_observation {
        let filename = observation_filename(report.observation_budget_ms);
        write_layout_html(
            &output_dir.join(&filename),
            &observation.layout,
            loaded.localization.as_ref(),
            &format!(
                "crossing-free {}ms observation",
                report.observation_budget_ms
            ),
        )?;
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write crossing-free restriction report")?;
    println!();
    Ok(!report.interpretation_blocked)
}

fn write_case_layout(
    output_dir: &std::path::Path,
    case: &CrossingRestrictionCaseReport,
    localization: Option<&aic_data::localization::ValidatedLocalizationCatalog>,
) -> Result<()> {
    write_layout_html(
        &output_dir.join(format!("run-{:02}-{}.html", case.run_index, case.label)),
        &case.layout,
        localization,
        &format!(
            "crossing restriction run {} ({})",
            case.run_index, case.label
        ),
    )
}

fn render_summary(report: &CrossingFreeRestrictionExperimentReport) -> Result<String> {
    let mut rows = String::new();
    for case in &report.cases {
        rows.push_str(&render_case_row(case, None));
    }
    if let Some(observation) = &report.crossing_free_observation {
        rows.push_str(&render_case_row(
            observation,
            Some(report.observation_budget_ms),
        ));
    }
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Crossing-free restriction experiment</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}.warning{{border:1px solid #ffd166;color:#ffd166;padding:10px}}.pass{{color:#65f0bd}}.block{{color:#ff6b9d}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:7px;text-align:left}}th{{background:#102535;color:#8fd9ff}}a,code{{color:#ffd166}}</style></head><body><h1>Phase {phase} crossing-free restriction experiment</h1><div class="meta">fixed={width}x{height} · order={order} · case budget={budget}ms · observation={observation}ms · total={total}ms</div><p class="warning">B forbids same-layer bridges only. It is an auxiliary restricted witness generator, not an exact replacement for A. B Unknown or infeasible says nothing about unrestricted feasibility. No witness is used as a hint in this slice.</p><p class="{gate_class}">model identity={identity} · exact restriction delta={delta} · certificates={certificates} · outcomes consistent={consistency} · all found witnesses valid={validation} · hint progression={progression} · blocked={blocked}</p><p>A witnesses={a_witnesses} · B witnesses={b_witnesses} · B witness found={found} · all 5s B unknown={all_unknown}</p><table><thead><tr><th>run</th><th>kind</th><th>outcome</th><th>build ms</th><th>search ms</th><th>first</th><th>decisions</th><th>backtracks</th><th>conflicts</th><th>learned</th><th>propagations</th><th>bridges</th><th>layout</th></tr></thead><tbody>{rows}</tbody></table></body></html>"#,
        phase = report.target_phase_index,
        width = report.fixed_dimensions[0],
        height = report.fixed_dimensions[1],
        order = report.run_order.join(""),
        budget = report.case_search_budget_ms,
        observation = report.observation_budget_ms,
        total = report.outer_wall_ms,
        gate_class = if report.interpretation_blocked {
            "block"
        } else {
            "pass"
        },
        identity = report.model_identity_satisfied,
        delta = report.exact_restriction_delta_satisfied,
        certificates = report.crossing_free_certificates_satisfied,
        consistency = report.outcome_consistency_satisfied,
        validation = report.all_found_crossing_free_witnesses_valid,
        progression = report.hint_progression_authorized,
        blocked = report.interpretation_blocked,
        a_witnesses = report.unrestricted_witness_count,
        b_witnesses = report.crossing_free_witness_count,
        found = report.crossing_free_witness_found,
        all_unknown = report.all_crossing_free_cases_unknown,
    ))
}

fn render_case_row(
    case: &CrossingRestrictionCaseReport,
    observation_budget_ms: Option<u64>,
) -> String {
    let stats = &case.search_statistics;
    let layout = if let Some(budget_ms) = observation_budget_ms {
        observation_filename(budget_ms)
    } else {
        format!("run-{:02}-{}.html", case.run_index, case.label)
    };
    format!(
        "<tr><td>{}{}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"{}\">open</a></td></tr>",
        case.label,
        if observation_budget_ms.is_some() {
            " (observation)"
        } else {
            ""
        },
        case.kind,
        case.outcome,
        case.construction_ms,
        case.search_ms,
        case.first_incumb_ms_display(),
        optional(stats.branch_decisions),
        optional(stats.backtracks),
        optional(stats.conflicts),
        optional(stats.learned_clauses),
        optional(stats.solver_propagations),
        case.bridge_component_count,
        layout,
    )
}

fn observation_filename(budget_ms: u64) -> String {
    format!("crossing-free-observation-{budget_ms}ms.html")
}

trait CaseDisplay {
    fn first_incumb_ms_display(&self) -> String;
}

impl CaseDisplay for CrossingRestrictionCaseReport {
    fn first_incumb_ms_display(&self) -> String {
        self.first_incumbent_ms
            .map_or_else(|| "-".to_string(), |value| value.to_string())
    }
}

fn optional(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_string(), |value| value.to_string())
}
