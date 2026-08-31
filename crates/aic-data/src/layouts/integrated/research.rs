use std::{collections::BTreeSet, time::Duration};

use serde::Serialize;

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{FacilityInstanceWiringReport, Rate};

use super::{
    IntegratedLayoutDiagnostic, IntegratedLayoutPhase, IntegratedLayoutReport,
    IntegratedLayoutStatus, exact, harness, prepare_exact_model,
};

pub const EXACT_ABLATION_MATRIX_SCHEMA_VERSION: u32 = 1;
pub const SHARED_LAYER_COMPARISON_SCHEMA_VERSION: u32 = 1;
pub const FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION: u32 = 1;
pub const FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION: u32 = 1;
pub const FACTORED_REQUIREMENT_DECOMPOSITION_SCHEMA_VERSION: u32 = 1;
pub const EXTERNAL_CONNECTOR_SUBSET_SCHEMA_VERSION: u32 = 1;
pub const EXTERNAL_CONNECTOR_PORT_DOMAIN_SCHEMA_VERSION: u32 = 1;
pub const CUMULATIVE_SCC_GROWTH_SCHEMA_VERSION: u32 = 1;
pub const PHYSICAL_OCCUPANCY_PROBE_SCHEMA_VERSION: u32 = 2;
pub const EXACT_DIMENSION_PARTITION_SCHEMA_VERSION: u32 = 1;

const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PhysicalOccupancyEncoding {
    CandidateCollision,
    CanonicalSharedOccupancy,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PhysicalOccupancyRestriction {
    None,
    BeltUsed,
    PipeUsed,
    ExactPlacement,
    SameFootprintDomain,
    NonCoveringControl,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PhysicalOccupancyDomainSnapshot {
    pub supported_placement_candidates: usize,
    pub fixed_false_placement_candidates: usize,
    pub fixed_true_placement_candidates: usize,
    pub supported_placement_choice_values: usize,
    pub distinct_x_values: usize,
    pub distinct_y_values: usize,
    pub distinct_rotation_values: usize,
    pub facility_cells_fixed_true: usize,
    pub facility_cells_fixed_false: usize,
    pub facility_cells_free: usize,
    pub belt_cells_fixed_true: usize,
    pub belt_cells_fixed_false: usize,
    pub belt_cells_free: usize,
    pub pipe_cells_fixed_true: usize,
    pub pipe_cells_fixed_false: usize,
    pub pipe_cells_free: usize,
    pub target_belt_domain: Vec<i32>,
    pub target_pipe_domain: Vec<i32>,
    pub target_facility_domain: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PhysicalOccupancyCaseReport {
    pub restriction: PhysicalOccupancyRestriction,
    pub before: PhysicalOccupancyDomainSnapshot,
    pub after: PhysicalOccupancyDomainSnapshot,
    pub removed_target_covering_candidates: usize,
    pub removed_non_covering_candidates: usize,
    pub newly_forbidden_belt_cells_inside_selected_footprint: usize,
    pub newly_forbidden_pipe_cells_inside_selected_footprint: usize,
    pub changed_collision_rows: usize,
    pub fully_decided_collision_rows: usize,
    pub incident_collision_rows: usize,
    pub incident_collision_terms: usize,
    pub propagation_time_us: u64,
    pub inconsistent: bool,
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PhysicalOccupancyProbeReport {
    pub schema_version: u32,
    pub encoding: PhysicalOccupancyEncoding,
    pub facility_id: String,
    pub request_bounds: crate::research::BenchmarkRequestBounds,
    pub target_cell: [i32; 2],
    pub same_footprint_origin: [i32; 2],
    pub non_covering_origin: [i32; 2],
    pub candidate_count: usize,
    pub analytically_target_covering_candidates: usize,
    pub collision_rows: usize,
    pub collision_terms: usize,
    pub cases: Vec<PhysicalOccupancyCaseReport>,
}

pub fn run_physical_occupancy_probe(
    facilities: &ValidatedFacilityCatalog,
    facility_id: &str,
    request: &FacilityPlacementRequest,
    encoding: PhysicalOccupancyEncoding,
) -> Result<PhysicalOccupancyProbeReport, String> {
    let diagnostics = crate::layouts::validate_facility_placement_request(request);
    if let Some(diagnostic) = diagnostics.first() {
        return Err(format!("{}: {}", diagnostic.code, diagnostic.message));
    }
    let facility = facilities
        .facility(facility_id)
        .ok_or_else(|| format!("facility '{facility_id}' is absent from the facility catalog"))?;
    exact::probe_physical_occupancy(facility, request, encoding)
}

pub fn render_physical_occupancy_probe_html(
    report: &PhysicalOccupancyProbeReport,
) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    let rows = report
        .cases
        .iter()
        .map(|case| {
            format!(
                "<tr><td>{:?}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                case.restriction,
                case.removed_target_covering_candidates,
                case.removed_non_covering_candidates,
                case.after.target_belt_domain,
                case.after.target_pipe_domain,
                case.newly_forbidden_belt_cells_inside_selected_footprint,
                case.newly_forbidden_pipe_cells_inside_selected_footprint,
                case.propagation_time_us,
                case.verdict,
            )
        })
        .collect::<String>();
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Physical occupancy propagation probe</title><style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;background:#07131d;color:#d5e8f5;margin:24px}}h1{{font-size:20px}}.meta{{color:#8fb2c8;margin-bottom:18px}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #315066;padding:8px;text-align:left;vertical-align:top}}th{{background:#102535;color:#8fd9ff}}tr:nth-child(even){{background:#0b1c28}}code{{color:#ffd166}}details{{margin-top:20px}}pre{{white-space:pre-wrap}}</style></head><body><h1>Physical occupancy propagation probe</h1><div class="meta">encoding=<code>{:?}</code> · facility=<code>{}</code> · bounds={}×{} · candidates={} · target-covering={}</div><table><thead><tr><th>restriction</th><th>covering candidates removed</th><th>other candidates removed</th><th>target belt</th><th>target pipe</th><th>belt footprint cells newly forbidden</th><th>pipe footprint cells newly forbidden</th><th>root μs</th><th>verdict</th></tr></thead><tbody>{}</tbody></table><details><summary>Machine-readable report</summary><pre id="json"></pre></details><script>const report={};document.getElementById('json').textContent=JSON.stringify(report,null,2);</script></body></html>"#,
        report.encoding,
        report.facility_id,
        report.request_bounds.max_width,
        report.request_bounds.max_height,
        report.candidate_count,
        report.analytically_target_covering_candidates,
        rows,
        json,
    ))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CumulativeSccGrowthReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub total_phase_count: usize,
    pub phase_search_budget_ms: u64,
    pub layout: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn solve_cumulative_scc_growth_v2(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    phase_search_budget: Duration,
) -> Result<CumulativeSccGrowthReport, IntegratedLayoutReport> {
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success {
        let diagnostic = growth.diagnostics.into_iter().next().map_or_else(
            || {
                IntegratedLayoutDiagnostic::error(
                    "research-scc-growth-planning-failed",
                    "/",
                    None,
                    "SCC growth planning failed without a diagnostic",
                )
            },
            |diagnostic| {
                IntegratedLayoutDiagnostic::error(
                    "research-scc-growth-planning-failed",
                    diagnostic.path,
                    diagnostic.entity,
                    diagnostic.message,
                )
            },
        );
        return Err(IntegratedLayoutReport::invalid(diagnostic));
    }
    let total_phase_count = growth.phases.len();
    if target_phase_index >= total_phase_count {
        return Err(IntegratedLayoutReport::invalid(
            IntegratedLayoutDiagnostic::error(
                "research-scc-target-phase-out-of-range",
                "/target_phase_index",
                Some(target_phase_index.to_string()),
                format!(
                    "target phase {target_phase_index} is outside the cumulative SCC phase range 0..{total_phase_count}"
                ),
            ),
        ));
    }

    let total_facilities = growth
        .components
        .iter()
        .map(|component| component.facilities.len())
        .sum();
    let mut cumulative_facilities = BTreeSet::new();
    let mut previous_solution = None;
    let mut snapshots = Vec::with_capacity(target_phase_index + 1);

    for phase in growth.phases.iter().take(target_phase_index + 1) {
        cumulative_facilities.extend(phase.facilities.iter().cloned());
        let partial_wiring = harness::project_cumulative_wiring(
            instance_wiring,
            &cumulative_facilities,
            total_facilities,
        )
        .map_err(IntegratedLayoutReport::invalid)?;
        let input = prepare_exact_model(
            &partial_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
        )?;
        let mut phase_report = exact::shared_layer::solve_factored_endpoints_with_prior(
            input,
            logistics_components,
            Some(phase_search_budget),
            previous_solution.as_ref(),
        );
        if !phase_report.success {
            phase_report
                .diagnostics
                .push(IntegratedLayoutDiagnostic::error(
                    "research-cumulative-scc-phase-unsolved",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    format!(
                        "cumulative SCC phase {} returned without a complete validated incumbent; no fallback was attempted",
                        phase.index,
                    ),
                ));
            phase_report.phases = snapshots;
            return Ok(CumulativeSccGrowthReport {
                schema_version: CUMULATIVE_SCC_GROWTH_SCHEMA_VERSION,
                target_phase_index,
                total_phase_count,
                phase_search_budget_ms: millis(phase_search_budget),
                layout: phase_report,
            });
        }

        let bounds = phase_report
            .bounds
            .clone()
            .expect("a successful exact solve has canonical used bounds");
        let exact = phase_report
            .exact
            .clone()
            .expect("a successful exact solve has exact metrics");
        snapshots.push(IntegratedLayoutPhase {
            index: phase.index,
            introduced_components: phase.components.clone(),
            introduced_facilities: phase.facilities.clone(),
            cumulative_facility_count: exact.model.facility_count,
            cumulative_route_requirement_count: exact.model.route_requirement_count,
            bounds,
            placements: phase_report.placements.clone(),
            logistics_components: phase_report.logistics_components.clone(),
            transport_networks: phase_report.transport_networks.clone(),
            exact,
        });
        previous_solution = Some(phase_report);
    }

    let mut layout = previous_solution.unwrap_or_else(|| {
        IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::InvalidInput,
            IntegratedLayoutDiagnostic::error(
                "research-empty-scc-growth-plan",
                "/",
                None,
                "the cumulative SCC growth experiment requires at least one phase",
            ),
        )
    });
    layout.phases = snapshots;
    layout.diagnostics.push(IntegratedLayoutDiagnostic::info(
        "research-cumulative-scc-v2-complete",
        format!(
            "solved cumulative SCC phases 0 through {target_phase_index} with independent per-phase budgets and placement-only non-binding hints"
        ),
    ));
    Ok(CumulativeSccGrowthReport {
        schema_version: CUMULATIVE_SCC_GROWTH_SCHEMA_VERSION,
        target_phase_index,
        total_phase_count,
        phase_search_budget_ms: millis(phase_search_budget),
        layout,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalConnectorRequirementDescriptor {
    pub route_index: usize,
    pub requirement_id: String,
    pub item: String,
    pub transport: crate::logistics::TransportKind,
    pub direction: FacilityPortDirection,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalConnectorSubsetReport {
    pub schema_version: u32,
    pub route_indices: Vec<usize>,
    pub selected_requirements: Vec<ExternalConnectorRequirementDescriptor>,
    pub search_budget_ms: u64,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalConnectorPortDomainClassification {
    FaithfulBaseline,
    DiagnosticOnly,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalConnectorPortDomainReport {
    pub schema_version: u32,
    pub case_id: String,
    pub classification: ExternalConnectorPortDomainClassification,
    pub selected_requirement: ExternalConnectorRequirementDescriptor,
    pub requested_port_ids: Vec<String>,
    pub retained_port_ids: Vec<String>,
    pub search_budget_ms: u64,
    pub layout: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn solve_first_integrated_layout_phase_external_connector_subset(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    route_indices: &[usize],
    search_budget: Duration,
) -> Result<ExternalConnectorSubsetReport, IntegratedLayoutReport> {
    let (route_indices, descriptors, input) = prepare_external_connector_subset(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        route_indices,
    )?;
    let layout = exact::shared_layer::solve_factored_endpoints(
        input,
        logistics_components,
        Some(search_budget),
    );
    Ok(ExternalConnectorSubsetReport {
        schema_version: EXTERNAL_CONNECTOR_SUBSET_SCHEMA_VERSION,
        route_indices,
        selected_requirements: descriptors,
        search_budget_ms: millis(search_budget),
        layout,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn solve_first_integrated_layout_phase_external_connector_port_domain(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    case_id: &str,
    classification: ExternalConnectorPortDomainClassification,
    route_index: usize,
    requested_port_ids: &[String],
    search_budget: Duration,
) -> Result<ExternalConnectorPortDomainReport, IntegratedLayoutReport> {
    let (route_indices, mut descriptors, mut input) = prepare_external_connector_subset(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        &[route_index],
    )?;
    let (retained_port_ids, retains_full_domain) =
        restrict_external_port_domain(&mut input, requested_port_ids)
            .map_err(IntegratedLayoutReport::invalid)?;
    if matches!(
        classification,
        ExternalConnectorPortDomainClassification::FaithfulBaseline
    ) != retains_full_domain
    {
        return Err(IntegratedLayoutReport::invalid(
            super::IntegratedLayoutDiagnostic::error(
                "research-port-domain-classification-mismatch",
                "/classification",
                Some(format!("{classification:?}")),
                "faithful-baseline must retain the complete compatible port domain and every restricted domain must be diagnostic-only",
            ),
        ));
    }
    let selected_requirement = descriptors
        .pop()
        .expect("one selected route has one requirement descriptor");
    debug_assert_eq!(route_indices, vec![route_index]);
    let layout = exact::shared_layer::solve_factored_endpoints(
        input,
        logistics_components,
        Some(search_budget),
    );
    Ok(ExternalConnectorPortDomainReport {
        schema_version: EXTERNAL_CONNECTOR_PORT_DOMAIN_SCHEMA_VERSION,
        case_id: case_id.to_string(),
        classification,
        selected_requirement,
        requested_port_ids: requested_port_ids.to_vec(),
        retained_port_ids,
        search_budget_ms: millis(search_budget),
        layout,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_external_connector_subset(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    route_indices: &[usize],
) -> Result<
    (
        Vec<usize>,
        Vec<ExternalConnectorRequirementDescriptor>,
        super::ModelInput,
    ),
    IntegratedLayoutReport,
> {
    let mut route_indices = route_indices.to_vec();
    route_indices.sort_unstable();
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let mut descriptors = Vec::with_capacity(route_indices.len());
    for index in &route_indices {
        let Some(edge) = input.edges.get(*index) else {
            return Err(IntegratedLayoutReport::invalid(
                super::IntegratedLayoutDiagnostic::error(
                    "research-route-index-out-of-range",
                    "/route_indices",
                    Some(index.to_string()),
                    format!(
                        "research route index {index} is outside the available range 0..{}",
                        input.edges.len()
                    ),
                ),
            ));
        };
        let direction = match (&edge.source, &edge.target) {
            (super::EndpointInput::External { .. }, super::EndpointInput::Facility { .. }) => {
                FacilityPortDirection::Input
            }
            (super::EndpointInput::Facility { .. }, super::EndpointInput::External { .. }) => {
                FacilityPortDirection::Output
            }
            _ => {
                return Err(IntegratedLayoutReport::invalid(
                    super::IntegratedLayoutDiagnostic::error(
                        "research-route-is-not-external",
                        "/route_indices",
                        Some(index.to_string()),
                        format!(
                            "research route index {index} does not describe exactly one external endpoint"
                        ),
                    ),
                ));
            }
        };
        descriptors.push(ExternalConnectorRequirementDescriptor {
            route_index: *index,
            requirement_id: edge.requirement_id.clone(),
            item: edge.edge.item.clone(),
            transport: edge.transport,
            direction,
            rate: edge.edge.rate,
        });
    }
    let (input, selected_requirements) = input
        .select_route_indices(&route_indices)
        .map_err(IntegratedLayoutReport::invalid)?;
    if descriptors
        .iter()
        .map(|descriptor| descriptor.requirement_id.as_str())
        .ne(selected_requirements.iter().map(String::as_str))
    {
        return Err(IntegratedLayoutReport::invalid(
            super::IntegratedLayoutDiagnostic::error(
                "research-requirement-selection-order-mismatch",
                "/route_indices",
                None,
                "selected external requirements did not preserve canonical route-index order",
            ),
        ));
    }
    Ok((route_indices, descriptors, input))
}

fn restrict_external_port_domain(
    input: &mut super::ModelInput,
    requested_port_ids: &[String],
) -> Result<(Vec<String>, bool), super::IntegratedLayoutDiagnostic> {
    if requested_port_ids.is_empty() {
        return Err(super::IntegratedLayoutDiagnostic::error(
            "empty-research-port-domain",
            "/port_ids",
            None,
            "diagnostic port domain must retain at least one compatible port",
        ));
    }
    let requested = requested_port_ids.iter().cloned().collect::<BTreeSet<_>>();
    if requested.len() != requested_port_ids.len() {
        return Err(super::IntegratedLayoutDiagnostic::error(
            "duplicate-research-port-id",
            "/port_ids",
            None,
            "diagnostic port domain contains a duplicate port ID",
        ));
    }
    let edge = input
        .edges
        .first_mut()
        .expect("one selected route produces one edge");
    let ports = match (&mut edge.source, &mut edge.target) {
        (super::EndpointInput::External { .. }, super::EndpointInput::Facility { ports, .. })
        | (super::EndpointInput::Facility { ports, .. }, super::EndpointInput::External { .. }) => {
            ports
        }
        _ => unreachable!("external subset preparation accepted exactly one external endpoint"),
    };
    let available = ports
        .iter()
        .map(|port| port.id.clone())
        .collect::<BTreeSet<_>>();
    let unknown = requested
        .difference(&available)
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(super::IntegratedLayoutDiagnostic::error(
            "unknown-research-port-id",
            "/port_ids",
            Some(unknown.join(",")),
            "diagnostic port domain names a port outside the selected requirement's compatible domain",
        ));
    }
    let retains_full_domain = requested == available;
    ports.retain(|port| requested.contains(&port.id));
    Ok((
        ports.iter().map(|port| port.id.clone()).collect(),
        retains_full_domain,
    ))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredRequirementSubsetCaseReport {
    pub id: String,
    pub route_indices: Vec<usize>,
    pub selected_requirements: Vec<String>,
    pub search_budget_ms: u64,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredRequirementDecompositionReport {
    pub schema_version: u32,
    pub selected_network_index: usize,
    pub selected_network: String,
    pub search_budget_ms_per_case: u64,
    pub cases: Vec<FactoredRequirementSubsetCaseReport>,
}

#[allow(clippy::too_many_arguments)]
pub fn decompose_first_integrated_layout_phase_factored_requirements(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    network_index: usize,
    search_budget: Duration,
) -> Result<FactoredRequirementDecompositionReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let Some(network) = input.networks.get(network_index) else {
        return Err(IntegratedLayoutReport::invalid(
            super::IntegratedLayoutDiagnostic::error(
                "research-network-index-out-of-range",
                "/network_index",
                Some(network_index.to_string()),
                format!(
                    "research network index {network_index} is outside the available range 0..{}",
                    input.networks.len()
                ),
            ),
        ));
    };
    let selected_network = network.id().to_string();
    let route_indices = network.route_indices().to_vec();
    let mut selections = route_indices
        .iter()
        .copied()
        .map(|index| vec![index])
        .collect::<Vec<_>>();
    if route_indices.len() > 1 {
        selections.push(route_indices);
    }

    let mut cases = Vec::with_capacity(selections.len());
    for indices in selections {
        let (case_input, selected_requirements) = input
            .clone()
            .select_route_indices(&indices)
            .map_err(IntegratedLayoutReport::invalid)?;
        let id = if indices.len() == 1 {
            format!("requirement-{}", indices[0])
        } else {
            "combined".to_string()
        };
        let layout = exact::shared_layer::solve_factored_endpoints(
            case_input,
            logistics_components,
            Some(search_budget),
        );
        cases.push(FactoredRequirementSubsetCaseReport {
            id,
            route_indices: indices,
            selected_requirements,
            search_budget_ms: millis(search_budget),
            layout,
        });
    }

    Ok(FactoredRequirementDecompositionReport {
        schema_version: FACTORED_REQUIREMENT_DECOMPOSITION_SCHEMA_VERSION,
        selected_network_index: network_index,
        selected_network,
        search_budget_ms_per_case: millis(search_budget),
        cases,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredNetworkSubsetCaseReport {
    pub id: String,
    pub network_indices: Vec<usize>,
    pub selected_networks: Vec<String>,
    pub search_budget_ms: u64,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredNetworkDecompositionReport {
    pub schema_version: u32,
    pub search_budget_ms_per_case: u64,
    pub cases: Vec<FactoredNetworkSubsetCaseReport>,
}

pub const SEARCH_MODE_DIAGNOSIS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSearchMode {
    Optimize,
    FeasibilityOnly,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchModeDiagnosisCaseReport {
    pub schema_version: u32,
    pub selected_network_indices: Vec<usize>,
    pub selected_networks: Vec<String>,
    pub search_mode: DiagnosticSearchMode,
    pub search_budget_ms: u64,
    pub diagnostic_only: bool,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExactDimensionLowerBoundsReport {
    pub minimum_width: i32,
    pub minimum_height: i32,
    pub facility_area: i64,
    pub mandatory_transport_cells: i64,
    pub minimum_area: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExactUsedDimensionCandidate {
    pub width: i32,
    pub height: i32,
    pub area: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExactDimensionPartitionCaseReport {
    pub schema_version: u32,
    pub selected_network_indices: Vec<usize>,
    pub selected_networks: Vec<String>,
    pub request_width: i32,
    pub request_height: i32,
    pub lower_bounds: ExactDimensionLowerBoundsReport,
    pub candidates: Vec<ExactUsedDimensionCandidate>,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub search_budget_ms: u64,
    pub diagnostic_only: bool,
    pub layout: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn solve_first_integrated_layout_phase_fixed_dimensions(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    network_indices: &[usize],
    used_width: i32,
    used_height: i32,
    search_budget: Duration,
) -> Result<ExactDimensionPartitionCaseReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let (input, selected_networks) = input
        .select_network_indices(network_indices)
        .map_err(IntegratedLayoutReport::invalid)?;
    let lower_bounds = exact_dimension_lower_bounds(&input)?;
    let candidates = enumerate_exact_dimension_candidates(input.width, input.height, &lower_bounds);
    let fixed_dimensions = ExactUsedDimensionCandidate {
        width: used_width,
        height: used_height,
        area: i64::from(used_width) * i64::from(used_height),
    };
    if !candidates.contains(&fixed_dimensions) {
        return Err(IntegratedLayoutReport::invalid(
            IntegratedLayoutDiagnostic::error(
                "research-fixed-dimensions-outside-proven-domain",
                "/fixed_dimensions",
                Some(format!("{used_width}x{used_height}")),
                "fixed research dimensions are outside the request ceilings or violate a proven lower bound",
            ),
        ));
    }
    let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_feasibility_only(
        input,
        logistics_components,
        Some(search_budget),
        exact::shared_layer::FixedUsedDimensions {
            width: used_width,
            height: used_height,
        },
    );
    Ok(ExactDimensionPartitionCaseReport {
        schema_version: EXACT_DIMENSION_PARTITION_SCHEMA_VERSION,
        selected_network_indices: network_indices.to_vec(),
        selected_networks,
        request_width: i32::try_from(request.max_width).expect("validated request width fits i32"),
        request_height: i32::try_from(request.max_height)
            .expect("validated request height fits i32"),
        lower_bounds,
        candidates,
        fixed_dimensions,
        search_budget_ms: millis(search_budget),
        diagnostic_only: true,
        layout,
    })
}

fn exact_dimension_lower_bounds(
    input: &super::ModelInput,
) -> Result<ExactDimensionLowerBoundsReport, IntegratedLayoutReport> {
    let minimum_width = input
        .instances
        .iter()
        .map(|instance| {
            instance
                .definition
                .allowed_rotations
                .iter()
                .map(|rotation| {
                    if matches!(rotation, 90 | 270) {
                        instance.definition.footprint.height
                    } else {
                        instance.definition.footprint.width
                    }
                })
                .min()
                .expect("validated facility has an allowed rotation")
        })
        .max()
        .unwrap_or(1);
    let minimum_height = input
        .instances
        .iter()
        .map(|instance| {
            instance
                .definition
                .allowed_rotations
                .iter()
                .map(|rotation| {
                    if matches!(rotation, 90 | 270) {
                        instance.definition.footprint.width
                    } else {
                        instance.definition.footprint.height
                    }
                })
                .min()
                .expect("validated facility has an allowed rotation")
        })
        .max()
        .unwrap_or(1);
    let facility_area =
        super::required_facility_area(input).map_err(IntegratedLayoutReport::invalid)?;
    let mandatory_transport_cells = i64::from(!input.networks.is_empty());
    let minimum_area = facility_area
        .checked_add(mandatory_transport_cells)
        .expect("validated request area fits i64");
    Ok(ExactDimensionLowerBoundsReport {
        minimum_width: i32::try_from(minimum_width).expect("validated facility width fits i32"),
        minimum_height: i32::try_from(minimum_height).expect("validated facility height fits i32"),
        facility_area,
        mandatory_transport_cells,
        minimum_area,
    })
}

fn enumerate_exact_dimension_candidates(
    request_width: i32,
    request_height: i32,
    lower_bounds: &ExactDimensionLowerBoundsReport,
) -> Vec<ExactUsedDimensionCandidate> {
    let mut candidates = Vec::new();
    for width in lower_bounds.minimum_width..=request_width {
        for height in lower_bounds.minimum_height..=request_height {
            let area = i64::from(width) * i64::from(height);
            if area < lower_bounds.minimum_area {
                continue;
            }
            candidates.push(ExactUsedDimensionCandidate {
                width,
                height,
                area,
            });
        }
    }
    candidates.sort_by_key(|candidate| {
        (
            candidate.area,
            candidate.width.max(candidate.height),
            candidate.width,
            candidate.height,
        )
    });
    candidates
}

#[allow(clippy::too_many_arguments)]
pub fn solve_first_integrated_layout_phase_search_mode(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    network_indices: &[usize],
    search_mode: DiagnosticSearchMode,
    search_budget: Duration,
) -> Result<SearchModeDiagnosisCaseReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let (input, selected_networks) = input
        .select_network_indices(network_indices)
        .map_err(IntegratedLayoutReport::invalid)?;
    let layout = match search_mode {
        DiagnosticSearchMode::Optimize => exact::shared_layer::solve_factored_endpoints(
            input,
            logistics_components,
            Some(search_budget),
        ),
        DiagnosticSearchMode::FeasibilityOnly => {
            exact::shared_layer::solve_factored_endpoints_feasibility_only(
                input,
                logistics_components,
                Some(search_budget),
            )
        }
    };
    Ok(SearchModeDiagnosisCaseReport {
        schema_version: SEARCH_MODE_DIAGNOSIS_SCHEMA_VERSION,
        selected_network_indices: network_indices.to_vec(),
        selected_networks,
        search_mode,
        search_budget_ms: millis(search_budget),
        diagnostic_only: search_mode == DiagnosticSearchMode::FeasibilityOnly,
        layout,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn decompose_first_integrated_layout_phase_factored_networks(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    search_budget: Duration,
    case_id: Option<&str>,
) -> Result<FactoredNetworkDecompositionReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let network_count = input.networks.len();
    let mut selections = (0..network_count)
        .map(|index| vec![index])
        .collect::<Vec<_>>();
    for first in 0..network_count {
        for second in (first + 1)..network_count {
            selections.push(vec![first, second]);
        }
    }
    if network_count > 2 {
        selections.push((0..network_count).collect());
    }
    if let Some(case_id) = case_id {
        selections.retain(|indices| factored_network_case_id(indices) == case_id);
        if selections.is_empty() {
            return Err(IntegratedLayoutReport::invalid(
                super::IntegratedLayoutDiagnostic::error(
                    "research-network-case-not-found",
                    "/case_id",
                    Some(case_id.to_string()),
                    format!("research network-subset case '{case_id}' does not exist"),
                ),
            ));
        }
    }

    let mut cases = Vec::with_capacity(selections.len());
    for indices in selections {
        let (case_input, selected_networks) = input
            .clone()
            .select_network_indices(&indices)
            .map_err(IntegratedLayoutReport::invalid)?;
        let id = factored_network_case_id(&indices);
        let layout = exact::shared_layer::solve_factored_endpoints(
            case_input,
            logistics_components,
            Some(search_budget),
        );
        cases.push(FactoredNetworkSubsetCaseReport {
            id,
            network_indices: indices,
            selected_networks,
            search_budget_ms: millis(search_budget),
            layout,
        });
    }

    Ok(FactoredNetworkDecompositionReport {
        schema_version: FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION,
        search_budget_ms_per_case: millis(search_budget),
        cases,
    })
}

fn factored_network_case_id(indices: &[usize]) -> String {
    match indices {
        [index] => format!("single-{index}"),
        [first, second] => format!("pair-{first}-{second}"),
        _ => "full".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredEndpointComparisonReport {
    pub schema_version: u32,
    pub search_budget_ms_per_formulation: u64,
    pub flattened: IntegratedLayoutReport,
    pub factored: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn compare_first_integrated_layout_phase_factored_endpoints(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    search_budget: Duration,
) -> Result<FactoredEndpointComparisonReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let flattened =
        exact::shared_layer::solve(input.clone(), logistics_components, Some(search_budget));
    let factored = exact::shared_layer::solve_factored_endpoints(
        input,
        logistics_components,
        Some(search_budget),
    );
    Ok(FactoredEndpointComparisonReport {
        schema_version: FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION,
        search_budget_ms_per_formulation: millis(search_budget),
        flattened,
        factored,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SharedLayerComparisonReport {
    pub schema_version: u32,
    pub search_budget_ms_per_formulation: u64,
    pub dense: IntegratedLayoutReport,
    pub shared_layer: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn compare_first_integrated_layout_phase_shared_layer(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    search_budget: Duration,
) -> Result<SharedLayerComparisonReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let dense = exact::solve_with_prior_solution(
        input.clone(),
        logistics_components,
        Some(search_budget),
        None,
    );
    let shared_layer = exact::shared_layer::solve(input, logistics_components, Some(search_budget));
    Ok(SharedLayerComparisonReport {
        schema_version: SHARED_LAYER_COMPARISON_SCHEMA_VERSION,
        search_budget_ms_per_formulation: millis(search_budget),
        dense,
        shared_layer,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExactAblationFixation {
    None,
    Placements,
    PlacementsAndTerminals,
    NetworkRoute {
        network_id: String,
    },
    ZeroNetworkArcs {
        network_ids: Vec<String>,
    },
    ReferenceWithZeroNetworkArcs {
        placements: bool,
        terminals: bool,
        network_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExactAblationCaseReport {
    pub id: String,
    pub search_budget_ms: u64,
    pub selected_networks: Vec<String>,
    pub fixation: ExactAblationFixation,
    pub diagnostic_only: bool,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExactAblationMatrixReport {
    pub schema_version: u32,
    pub selected_pair: Vec<String>,
    pub case_budget_ms: u64,
    pub reference_budget_ms: u64,
    pub reference_case_id: Option<String>,
    pub cases: Vec<ExactAblationCaseReport>,
}

#[allow(clippy::too_many_arguments)]
pub fn decompose_first_integrated_layout_phase_pair(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    network_indices: [usize; 2],
    case_budget: Duration,
    reference_budget: Duration,
) -> Result<ExactAblationMatrixReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let (pair_input, selected_pair) = input
        .clone()
        .select_network_indices(&network_indices)
        .map_err(IntegratedLayoutReport::invalid)?;
    let (first_input, first_network) = input
        .clone()
        .select_network_indices(&[network_indices[0]])
        .map_err(IntegratedLayoutReport::invalid)?;
    let (second_input, second_network) = input
        .select_network_indices(&[network_indices[1]])
        .map_err(IntegratedLayoutReport::invalid)?;

    let mut cases = Vec::new();
    let baseline = run_case(
        "pair-free",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::None,
        None,
        case_budget,
        logistics_components,
    );
    cases.push(baseline.clone());
    let first_single = run_case(
        "single-first-free",
        first_input,
        &first_network,
        ExactAblationFixation::None,
        None,
        case_budget,
        logistics_components,
    );
    cases.push(first_single.clone());
    let second_single = run_case(
        "single-second-free",
        second_input,
        &second_network,
        ExactAblationFixation::None,
        None,
        case_budget,
        logistics_components,
    );
    cases.push(second_single.clone());

    let zero_first = run_case(
        "pair-zero-first-network-arcs",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::ZeroNetworkArcs {
            network_ids: vec![selected_pair[0].clone()],
        },
        None,
        case_budget,
        logistics_components,
    );
    cases.push(zero_first);
    let zero_second = run_case(
        "pair-zero-second-network-arcs",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::ZeroNetworkArcs {
            network_ids: vec![selected_pair[1].clone()],
        },
        None,
        case_budget,
        logistics_components,
    );
    cases.push(zero_second);
    let zero_both = run_case(
        "pair-zero-both-network-arcs",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::ZeroNetworkArcs {
            network_ids: selected_pair.clone(),
        },
        None,
        case_budget,
        logistics_components,
    );
    cases.push(zero_both.clone());

    let mut reference_case_id = baseline.layout.success.then(|| baseline.id.clone());
    let mut reference_layout = baseline.layout.success.then(|| baseline.layout.clone());

    if reference_layout.is_none() && zero_both.layout.success {
        reference_case_id = Some(zero_both.id.clone());
        reference_layout = Some(zero_both.layout.clone());
    }

    if reference_layout.is_none() {
        let extended = run_case(
            "pair-free-reference-budget",
            pair_input.clone(),
            &selected_pair,
            ExactAblationFixation::None,
            None,
            reference_budget,
            logistics_components,
        );
        if extended.layout.success {
            reference_case_id = Some(extended.id.clone());
            reference_layout = Some(extended.layout.clone());
        }
        cases.push(extended);
    }

    if reference_layout.is_none() {
        let placement_source = first_single
            .layout
            .success
            .then_some(&first_single.layout)
            .or_else(|| {
                second_single
                    .layout
                    .success
                    .then_some(&second_single.layout)
            });
        if let Some(placement_source) = placement_source {
            let extended = run_case(
                "pair-placement-reference-budget",
                pair_input.clone(),
                &selected_pair,
                ExactAblationFixation::Placements,
                Some(placement_source),
                reference_budget,
                logistics_components,
            );
            if extended.layout.success {
                reference_case_id = Some(extended.id.clone());
                reference_layout = Some(extended.layout.clone());
            }
            cases.push(extended);
        }
    }

    if let Some(reference) = reference_layout.as_ref() {
        for (id, placements, terminals) in [
            ("reference-check-placement-zero-arcs", true, false),
            ("reference-check-terminals-zero-arcs", false, true),
            ("reference-check-all-zero-arcs", true, true),
        ] {
            cases.push(run_case(
                id,
                pair_input.clone(),
                &selected_pair,
                ExactAblationFixation::ReferenceWithZeroNetworkArcs {
                    placements,
                    terminals,
                    network_ids: selected_pair.clone(),
                },
                Some(reference),
                case_budget,
                logistics_components,
            ));
        }
        for (id, fixation) in [
            ("pair-fixed-placements", ExactAblationFixation::Placements),
            (
                "pair-fixed-placements-terminals",
                ExactAblationFixation::PlacementsAndTerminals,
            ),
            (
                "pair-fixed-first-network-route",
                ExactAblationFixation::NetworkRoute {
                    network_id: selected_pair[0].clone(),
                },
            ),
            (
                "pair-fixed-second-network-route",
                ExactAblationFixation::NetworkRoute {
                    network_id: selected_pair[1].clone(),
                },
            ),
        ] {
            cases.push(run_case(
                id,
                pair_input.clone(),
                &selected_pair,
                fixation,
                Some(reference),
                case_budget,
                logistics_components,
            ));
        }
    }

    Ok(ExactAblationMatrixReport {
        schema_version: EXACT_ABLATION_MATRIX_SCHEMA_VERSION,
        selected_pair,
        case_budget_ms: millis(case_budget),
        reference_budget_ms: millis(reference_budget),
        reference_case_id,
        cases,
    })
}

fn run_case(
    id: &str,
    input: super::ModelInput,
    selected_networks: &[String],
    fixation: ExactAblationFixation,
    reference: Option<&IntegratedLayoutReport>,
    budget: Duration,
    logistics_components: &ValidatedLogisticsComponentCatalog,
) -> ExactAblationCaseReport {
    let layout = exact::solve_with_research_fixation(
        input,
        logistics_components,
        Some(budget),
        reference,
        &fixation,
    );
    ExactAblationCaseReport {
        id: id.to_string(),
        search_budget_ms: millis(budget),
        selected_networks: selected_networks.to_vec(),
        fixation,
        diagnostic_only: true,
        layout,
    }
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_dimension_candidates_apply_only_proven_bounds_and_are_area_ordered() {
        let lower_bounds = ExactDimensionLowerBoundsReport {
            minimum_width: 5,
            minimum_height: 5,
            facility_area: 25,
            mandatory_transport_cells: 1,
            minimum_area: 26,
        };
        let candidates = enumerate_exact_dimension_candidates(12, 12, &lower_bounds);

        assert_eq!(candidates.len(), 63);
        assert_eq!(
            candidates.first(),
            Some(&ExactUsedDimensionCandidate {
                width: 5,
                height: 6,
                area: 30,
            })
        );
        assert_eq!(
            candidates.get(1),
            Some(&ExactUsedDimensionCandidate {
                width: 6,
                height: 5,
                area: 30,
            })
        );
        assert_eq!(
            candidates.last(),
            Some(&ExactUsedDimensionCandidate {
                width: 12,
                height: 12,
                area: 144,
            })
        );
        assert!(!candidates.iter().any(|candidate| candidate.area < 26));
    }
}
