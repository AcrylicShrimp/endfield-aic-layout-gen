use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::EndpointChannelEncoding;
use crate::layouts::integrated::{IntegratedLayoutDiagnostic, exact, harness, prepare_exact_model};

pub const SCALED_ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScaledEndpointDomainSnapshot {
    pub placement_values: usize,
    pub terminals: Vec<ScaledEndpointTerminalDomainSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScaledEndpointTerminalDomainSnapshot {
    pub terminal: String,
    pub port_values: usize,
    pub geometry_values: usize,
    pub direction_values: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScaledEndpointRestrictionReport {
    pub restriction: super::EndpointChannelRestriction,
    pub applicable: bool,
    pub description: String,
    pub before: ScaledEndpointDomainSnapshot,
    pub after: ScaledEndpointDomainSnapshot,
    pub inconsistent: bool,
    pub root_propagation_us: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScaledEndpointTerminalScale {
    pub terminal: String,
    pub port_values: usize,
    pub reachable_geometry_values: usize,
    pub legal_tuple_rows: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScaledEndpointChannelProbeReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub used_width: i32,
    pub used_height: i32,
    pub facility: String,
    pub encoding: EndpointChannelEncoding,
    pub unprojected_placement_values: usize,
    pub placement_values: usize,
    pub bounded_geometry_domain_values_per_terminal: usize,
    pub terminals: Vec<ScaledEndpointTerminalScale>,
    pub authored_integer_variables: usize,
    pub element_constraints: usize,
    pub table_constraints: usize,
    pub table_rows: usize,
    pub estimated_hidden_table_literals: usize,
    pub estimated_table_clauses: usize,
    pub build_us: u64,
    pub search_performed: bool,
    pub branch_decisions: u64,
    pub backtracks: u64,
    pub conflicts: u64,
    pub learned_clauses: u64,
    pub solver_propagations: u64,
    pub cases: Vec<ScaledEndpointRestrictionReport>,
}

#[allow(clippy::too_many_arguments)]
pub fn run_scaled_endpoint_channel_probe(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    encoding: EndpointChannelEncoding,
) -> Result<ScaledEndpointChannelProbeReport, IntegratedLayoutDiagnostic> {
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success {
        return Err(IntegratedLayoutDiagnostic::error(
            "research-scaled-endpoint-growth-failed",
            "/",
            None,
            "facility growth planning failed for the scaled endpoint-channel probe",
        ));
    }
    let phase = growth.phases.get(target_phase_index).ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "research-scaled-endpoint-phase-out-of-range",
            "/target_phase_index",
            Some(target_phase_index.to_string()),
            format!(
                "target phase {target_phase_index} is outside the growth range 0..{}",
                growth.phases.len()
            ),
        )
    })?;
    if phase.facilities.len() != 1 {
        return Err(IntegratedLayoutDiagnostic::error(
            "research-scaled-endpoint-phase-facility-count",
            format!("/phases/{target_phase_index}/facilities"),
            None,
            format!(
                "scaled endpoint-channel probe requires exactly one introduced facility; phase {target_phase_index} has {}",
                phase.facilities.len()
            ),
        ));
    }
    let cumulative_facilities = growth
        .phases
        .iter()
        .take(target_phase_index + 1)
        .flat_map(|phase| phase.facilities.iter().cloned())
        .collect();
    let total_facilities = growth
        .components
        .iter()
        .map(|component| component.facilities.len())
        .sum();
    let partial_wiring = harness::project_cumulative_wiring(
        instance_wiring,
        &cumulative_facilities,
        total_facilities,
    )?;
    let input = prepare_exact_model(
        &partial_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )
    .map_err(|report| {
        report.diagnostics.into_iter().next().unwrap_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "research-scaled-endpoint-model-preparation-failed",
                "/",
                None,
                "exact model preparation failed without a diagnostic",
            )
        })
    })?;
    exact::probe_scaled_endpoint_channels(
        &input,
        &phase.facilities[0],
        target_phase_index,
        encoding,
    )
}

pub fn render_scaled_endpoint_channel_probe_html(
    report: &ScaledEndpointChannelProbeReport,
) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    let rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.restriction,
                case.applicable,
                case.before.placement_values,
                case.after.placement_values,
                case.inconsistent,
                case.root_propagation_us,
            )
        })
        .collect::<String>();
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Scaled endpoint-channel probe</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Actual Phase 3 endpoint-channel probe</h1><div class="meta">facility=<code>{}</code> · encoding=<code>{:?}</code> · used={}×{} · placements={} · terminals={} · rows={} · build={}μs</div><table><thead><tr><th>restriction</th><th>applicable</th><th>placement before</th><th>placement after</th><th>root conflict</th><th>root μs</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.facility,
        report.encoding,
        report.used_width,
        report.used_height,
        report.placement_values,
        report.terminals.len(),
        report.table_rows,
        report.build_us,
        rows,
        json,
    ))
}
