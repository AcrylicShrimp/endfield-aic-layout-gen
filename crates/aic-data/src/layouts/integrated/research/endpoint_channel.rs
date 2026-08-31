use serde::Serialize;

use super::exact;

pub const ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointChannelEncoding {
    NestedElement,
    DirectTupleClauses,
    DirectionChannel,
    PositiveTable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointChannelRestriction {
    FixedPlacementAndPort,
    InteriorGeometryHole,
    DirectionClassOnly,
    RemoveAllPlacementSupports,
    PlacementHoleForward,
    SharedPlacementConflict,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EndpointChannelEndpointSnapshot {
    pub port_values: Vec<i32>,
    pub geometry_values: Vec<i32>,
    pub direction_values: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EndpointChannelDomainSnapshot {
    pub placement_values: Vec<i32>,
    pub endpoints: Vec<EndpointChannelEndpointSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EndpointChannelCaseReport {
    pub encoding: EndpointChannelEncoding,
    pub restriction: EndpointChannelRestriction,
    pub before: EndpointChannelDomainSnapshot,
    pub after: EndpointChannelDomainSnapshot,
    pub inconsistent: bool,
    pub root_propagation_us: u64,
    pub authored_integer_variables: usize,
    pub element_constraints: usize,
    pub direct_clauses: usize,
    pub table_rows: usize,
    pub estimated_hidden_table_literals: usize,
    pub estimated_table_clauses: usize,
    pub matches_positive_table_oracle: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EndpointChannelProbeReport {
    pub schema_version: u32,
    pub placement_values: usize,
    pub port_values: usize,
    pub reachable_geometry_values: Vec<i32>,
    pub legal_tuples: Vec<[i32; 3]>,
    pub cases: Vec<EndpointChannelCaseReport>,
}

pub fn run_endpoint_channel_probe() -> EndpointChannelProbeReport {
    exact::probe_endpoint_channels()
}

pub fn render_endpoint_channel_probe_html(
    report: &EndpointChannelProbeReport,
) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    let rows = report
        .cases
        .iter()
        .map(|case| {
            let placement = case
                .after
                .placement_values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let endpoints = case
                .after
                .endpoints
                .iter()
                .map(|endpoint| {
                    format!(
                        "P{:?} G{:?} D{:?}",
                        endpoint.port_values,
                        endpoint.geometry_values,
                        endpoint.direction_values
                    )
                })
                .collect::<Vec<_>>()
                .join("<br>");
            format!(
                "<tr><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.restriction,
                case.encoding,
                placement,
                endpoints,
                case.inconsistent,
                case.matches_positive_table_oracle,
                case.root_propagation_us,
                case.estimated_hidden_table_literals,
                case.estimated_table_clauses,
            )
        })
        .collect::<String>();
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Endpoint channel propagation probe</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Endpoint channel propagation probe</h1><div class="meta">placements={} · ports={} · legal tuples={} · reachable geometry values={}</div><table><thead><tr><th>restriction</th><th>encoding</th><th>placement domain</th><th>endpoint domains</th><th>conflict</th><th>oracle match</th><th>root μs</th><th>hidden table literals</th><th>estimated table clauses</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.placement_values,
        report.port_values,
        report.legal_tuples.len(),
        report.reachable_geometry_values.len(),
        rows,
        json,
    ))
}
