use std::collections::{BTreeMap, BTreeSet};

use crate::facilities::{FacilityPortDefinition, FacilityPortEdge, ValidatedFacilityCatalog};
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedItemCatalog,
    ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{FacilityInstanceWiringNode, FacilityInstanceWiringReport};
use crate::research::{
    AnalysisDiagnostic, AnalysisDiagnosticSeverity, CountDistribution, DomainCardinalitySummary,
    GraphStructureMetrics, IrComplexityMetrics, MetricCoverage, ModelComplexityMetrics,
    PhaseFormulationEstimate, PhaseGrowthMetrics, StaticSearchSpaceAnalysis, VariableDomainMetrics,
    VariableFamilyMetrics,
};

use super::{
    EndpointInput, InstanceInput, IntegratedLayoutDiagnostic, ModelInput, prepare_model,
    required_facility_area,
};

const MAX_NEW_FACILITIES_PER_PHASE: usize = 1;

#[allow(clippy::too_many_arguments)]
pub fn analyze_integrated_layout_search_space(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
) -> Result<StaticSearchSpaceAnalysis, IntegratedLayoutDiagnostic> {
    let full_input = prepare_model(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_PHASE);
    if !growth.success {
        let detail = growth.diagnostics.first().map_or_else(
            || "growth planning failed without a diagnostic".to_string(),
            |diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message),
        );
        return Err(IntegratedLayoutDiagnostic::error(
            "search-analysis-growth-planning-failed",
            "/",
            None,
            detail,
        ));
    }

    let graph = graph_metrics(instance_wiring, &growth.components);
    let phases = phase_metrics(
        instance_wiring,
        &growth.phases,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let (model_estimate, _) = model_estimate(&full_input, logistics_components)?;
    let placement_counts = placement_counts(&full_input)?;
    let endpoint_groups = endpoint_group_counts(&full_input)?;
    let compatible_ports = endpoint_groups
        .iter()
        .map(|group| group.compatible_ports)
        .collect::<Vec<_>>();
    let endpoint_options = endpoint_groups
        .iter()
        .map(|group| group.options)
        .collect::<Vec<_>>();
    let terminal_counts = full_input
        .networks
        .iter()
        .map(|network| network.terminal_count() as u64)
        .collect::<Vec<_>>();
    let required_area = to_u64(required_facility_area(&full_input)?)?;
    let grid_cells = to_u64(full_input.cell_count)?;
    let facility_area_slack = grid_cells.checked_sub(required_area);
    let facility_types = full_input
        .instances
        .iter()
        .map(|instance| instance.facility.as_str())
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let rotation_counts = full_input
        .instances
        .iter()
        .map(|instance| instance.definition.allowed_rotations.len() as u64)
        .collect::<Vec<_>>();

    let ir = IrComplexityMetrics {
        facility_count: full_input.instances.len() as u64,
        facility_type_count: facility_types,
        required_facility_area: required_area,
        grid_cell_count: grid_cells,
        facility_area_slack,
        rotations_per_facility: distribution(&rotation_counts),
        placement_candidates_per_facility: distribution(&placement_counts),
        placement_log2_volume: log2_product(&placement_counts),
        compatible_ports_per_facility_endpoint: distribution(&compatible_ports),
        endpoint_options_per_facility_endpoint: distribution(&endpoint_options),
        endpoint_log2_volume: log2_product(&endpoint_options),
        logical_wiring_edges: instance_wiring.edges.len() as u64,
        capacity_split_lanes: full_input.edges.len() as u64,
        commodity_networks: full_input.networks.len() as u64,
        belt_networks: full_input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Belt)
            .count() as u64,
        pipe_networks: full_input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Pipe)
            .count() as u64,
        terminals_per_network: distribution(&terminal_counts),
        external_terminals: full_input
            .networks
            .iter()
            .map(|network| network.external_terminal_count() as u64)
            .sum(),
        maximum_flow_scale: full_input
            .networks
            .iter()
            .map(|network| network.flow_scale() as u64)
            .max()
            .unwrap_or(0),
        maximum_line_capacity_units: full_input
            .networks
            .iter()
            .map(|network| network.line_capacity_units() as u64)
            .max()
            .unwrap_or(0),
        total_terminal_flow_units: full_input
            .networks
            .iter()
            .map(|network| network.total_terminal_flow_units() as u64)
            .sum(),
        graph,
        phases,
    };

    Ok(StaticSearchSpaceAnalysis {
        ir,
        model_estimate,
        diagnostics: vec![
            AnalysisDiagnostic {
                stage: "ir-search-space-analysis".to_string(),
                severity: AnalysisDiagnosticSeverity::Info,
                code: "static-search-space-analysis-complete".to_string(),
                path: "/".to_string(),
                entity: None,
                message: format!(
                    "estimated the current dense formulation across {} cumulative SCC phases without constructing a Pumpkin model",
                    growth.phases.len()
                ),
            },
            AnalysisDiagnostic {
                stage: "ir-search-space-analysis".to_string(),
                severity: AnalysisDiagnosticSeverity::Warning,
                code: "partial-formulation-estimate".to_string(),
                path: "/model_estimate".to_string(),
                entity: None,
                message: "variable totals are a covered lower bound; objective auxiliaries, exact constraints, factor-graph incidence, coupling, and symmetry require the authoritative model recorder"
                    .to_string(),
            },
        ],
    })
}

#[derive(Clone, Copy)]
struct EndpointGroup {
    compatible_ports: u64,
    options: u64,
}

fn placement_counts(input: &ModelInput) -> Result<Vec<u64>, IntegratedLayoutDiagnostic> {
    input
        .instances
        .iter()
        .map(|instance| placement_count(instance, input.width, input.height))
        .collect()
}

fn placement_count(
    instance: &InstanceInput,
    max_width: i32,
    max_height: i32,
) -> Result<u64, IntegratedLayoutDiagnostic> {
    let mut total = 0_u64;
    for rotation in &instance.definition.allowed_rotations {
        let (width, height) = rotated_dimensions(instance, *rotation)?;
        if width > max_width || height > max_height {
            continue;
        }
        let x_origins = to_u64(max_width - width + 1)?;
        let y_origins = to_u64(max_height - height + 1)?;
        total = checked_add(total, checked_mul(x_origins, y_origins)?)?;
    }
    Ok(total)
}

fn endpoint_group_counts(
    input: &ModelInput,
) -> Result<Vec<EndpointGroup>, IntegratedLayoutDiagnostic> {
    let instances = input
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut groups = Vec::new();
    for edge in &input.edges {
        for endpoint in [&edge.source, &edge.target] {
            let EndpointInput::Facility { instance, ports } = endpoint else {
                continue;
            };
            let instance = instances[instance.as_str()];
            groups.push(EndpointGroup {
                compatible_ports: ports.len() as u64,
                options: endpoint_option_count(instance, ports, input.width, input.height)?,
            });
        }
    }
    Ok(groups)
}

fn endpoint_option_count(
    instance: &InstanceInput,
    ports: &[FacilityPortDefinition],
    max_width: i32,
    max_height: i32,
) -> Result<u64, IntegratedLayoutDiagnostic> {
    let mut total = 0_u64;
    for rotation in &instance.definition.allowed_rotations {
        let (width, height) = rotated_dimensions(instance, *rotation)?;
        if width > max_width || height > max_height {
            continue;
        }
        let x_origins = to_u64(max_width - width + 1)?;
        let y_origins = to_u64(max_height - height + 1)?;
        for port in ports {
            let edge = port.edge.rotated_clockwise(*rotation);
            let options = match edge {
                FacilityPortEdge::North | FacilityPortEdge::South => {
                    checked_mul(x_origins, y_origins.saturating_sub(1))?
                }
                FacilityPortEdge::East | FacilityPortEdge::West => {
                    checked_mul(x_origins.saturating_sub(1), y_origins)?
                }
            };
            total = checked_add(total, options)?;
        }
    }
    Ok(total)
}

fn rotated_dimensions(
    instance: &InstanceInput,
    rotation: i64,
) -> Result<(i32, i32), IntegratedLayoutDiagnostic> {
    let width = i32::try_from(instance.definition.footprint.width).map_err(|_| overflow())?;
    let height = i32::try_from(instance.definition.footprint.height).map_err(|_| overflow())?;
    Ok(if matches!(rotation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    })
}

fn phase_metrics(
    wiring: &FacilityInstanceWiringReport,
    phases: &[crate::layouts::FacilityGrowthPhase],
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
) -> Result<Vec<PhaseGrowthMetrics>, IntegratedLayoutDiagnostic> {
    let total_facilities = wiring
        .nodes
        .iter()
        .filter(|node| matches!(node, FacilityInstanceWiringNode::Facility { .. }))
        .count();
    let all_facilities = wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            FacilityInstanceWiringNode::Facility { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut included = BTreeSet::new();
    let mut result = Vec::with_capacity(phases.len());
    let mut previous_networks = 0_u64;
    let mut previous_terminals = 0_u64;

    for phase in phases {
        included.extend(phase.facilities.iter().cloned());
        let partial =
            super::harness::project_cumulative_wiring(wiring, &included, total_facilities)?;
        let input = prepare_model(
            &partial,
            facilities,
            items,
            transports,
            logistics_components,
            request,
        )?;
        let (_, formulation) = model_estimate(&input, logistics_components)?;
        let networks = input.networks.len() as u64;
        let terminals = input
            .networks
            .iter()
            .map(|network| network.terminal_count() as u64)
            .sum::<u64>();
        let (frontier_cut_logical_edges, frontier_cut_networks) =
            frontier_cut(wiring, &included, &all_facilities, items);
        result.push(PhaseGrowthMetrics {
            phase_index: phase.index as u64,
            introduced_scc_ids: phase.components.clone(),
            introduced_facilities: phase.facilities.len() as u64,
            cumulative_facilities: input.instances.len() as u64,
            introduced_networks: networks.saturating_sub(previous_networks),
            cumulative_networks: networks,
            introduced_terminals: terminals.saturating_sub(previous_terminals),
            cumulative_terminals: terminals,
            frontier_cut_logical_edges,
            frontier_cut_networks,
            formulation,
        });
        previous_networks = networks;
        previous_terminals = terminals;
    }
    Ok(result)
}

fn frontier_cut(
    wiring: &FacilityInstanceWiringReport,
    included: &BTreeSet<String>,
    all_facilities: &BTreeSet<&str>,
    items: &ValidatedItemCatalog,
) -> (u64, u64) {
    let mut lanes = 0_u64;
    let mut networks = BTreeSet::new();
    for edge in &wiring.edges {
        let source_facility = all_facilities.contains(edge.source.as_str());
        let target_facility = all_facilities.contains(edge.target.as_str());
        let crosses = source_facility
            && target_facility
            && (included.contains(&edge.source) != included.contains(&edge.target));
        if !crosses {
            continue;
        }
        lanes += 1;
        if let Some(item) = items.item(&edge.item) {
            networks.insert((edge.item.as_str(), item.transport));
        }
    }
    (lanes, networks.len() as u64)
}

fn model_estimate(
    input: &ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
) -> Result<(ModelComplexityMetrics, PhaseFormulationEstimate), IntegratedLayoutDiagnostic> {
    let grid_cells = to_u64(input.cell_count)?;
    let placement_variables = placement_counts(input)?
        .into_iter()
        .try_fold(0_u64, |total, value| checked_add(total, value))?;
    let endpoint_variables = endpoint_group_counts(input)?
        .into_iter()
        .try_fold(0_u64, |total, group| checked_add(total, group.options))?;
    let network_count = input.networks.len() as u64;
    let undirected_edges = checked_add(
        checked_mul(
            to_u64(input.width.saturating_sub(1))?,
            to_u64(input.height)?,
        )?,
        checked_mul(
            to_u64(input.width)?,
            to_u64(input.height.saturating_sub(1))?,
        )?,
    )?;
    let directed_arcs = checked_mul(undirected_edges, 2)?;
    let route_cell_variables = checked_mul(network_count, grid_cells)?;
    let route_arc_variables = checked_mul(network_count, directed_arcs)?;
    let flow_variables = route_arc_variables;
    let route_order_variables = route_cell_variables;
    let terminal_presence_and_arm_variables = checked_mul(route_cell_variables, 16)?;
    let mut branch_component_variables = 0_u64;
    let mut flow_domains = Vec::new();
    for network in &input.networks {
        let rotations = [
            LogisticsComponentKind::Splitter,
            LogisticsComponentKind::Converger,
        ]
        .into_iter()
        .map(|kind| {
            logistics_components
                .component_by_kind(network.transport(), kind)
                .expect("validated component catalog contains branch capabilities")
                .allowed_rotations
                .len() as u64
        })
        .sum::<u64>();
        branch_component_variables = checked_add(
            branch_component_variables,
            checked_mul(grid_cells, rotations)?,
        )?;
        flow_domains.push(network.line_capacity_units() as u64 + 1);
    }
    let bridge_variables = checked_mul(grid_cells, 2)?;
    let bridge_rotation_count = [TransportKind::Belt, TransportKind::Pipe]
        .into_iter()
        .map(|transport| {
            logistics_components
                .component_by_kind(transport, LogisticsComponentKind::Bridge)
                .expect("validated component catalog contains bridge capabilities")
                .allowed_rotations
                .len() as u64
        })
        .sum::<u64>();
    let bridge_rotation_variables = checked_mul(grid_cells, bridge_rotation_count)?;
    let crossing_owner_variables = checked_mul(checked_mul(grid_cells, network_count)?, 2)?;
    let boolean_variables = [
        placement_variables,
        endpoint_variables,
        route_cell_variables,
        route_arc_variables,
        terminal_presence_and_arm_variables,
        branch_component_variables,
        bridge_variables,
        bridge_rotation_variables,
        crossing_owner_variables,
    ]
    .into_iter()
    .try_fold(0_u64, checked_add)?;
    let integer_variables = checked_add(flow_variables, route_order_variables)?;
    let covered_variable_lower_bound = checked_add(boolean_variables, integer_variables)?;
    let flow_log2 = flow_domains
        .iter()
        .map(|domain| (*domain as f64).log2() * directed_arcs as f64)
        .sum::<f64>();
    let order_log2 = if grid_cells > 0 {
        route_order_variables as f64 * (grid_cells as f64).log2()
    } else {
        0.0
    };
    let covered_log2_domain_volume = boolean_variables as f64 + flow_log2 + order_log2;

    let formulation = PhaseFormulationEstimate {
        coverage: MetricCoverage::PartialLowerBound,
        grid_cells,
        placement_variables,
        endpoint_variables,
        route_cell_variables,
        route_arc_variables,
        flow_variables,
        route_order_variables,
        terminal_presence_and_arm_variables,
        branch_component_variables,
        bridge_variables,
        bridge_rotation_variables,
        crossing_owner_variables,
        covered_variable_lower_bound,
        covered_log2_domain_volume,
    };
    let mut by_family = vec![
        boolean_family("placement", placement_variables),
        boolean_family("endpoint", endpoint_variables),
        boolean_family("route-cell", route_cell_variables),
        boolean_family("route-arc", route_arc_variables),
        integer_family(
            "flow",
            flow_variables,
            weighted_domain_summary(&flow_domains, directed_arcs),
        ),
        integer_family(
            "route-order",
            route_order_variables,
            constant_domain_summary(grid_cells, route_order_variables),
        ),
        boolean_family(
            "terminal-presence-and-arm",
            terminal_presence_and_arm_variables,
        ),
        boolean_family("branch-component", branch_component_variables),
        boolean_family("bridge", bridge_variables),
        boolean_family("bridge-rotation", bridge_rotation_variables),
        boolean_family("crossing-owner", crossing_owner_variables),
    ];
    by_family.retain(|family| family.total_variables > 0);

    Ok((
        ModelComplexityMetrics {
            variables: VariableDomainMetrics {
                coverage: MetricCoverage::PartialLowerBound,
                total_variables: covered_variable_lower_bound,
                boolean_variables,
                integer_variables,
                log2_domain_volume: covered_log2_domain_volume,
                by_family,
            },
            constraints: None,
            factor_graph: None,
            coupling: None,
            symmetry: None,
            estimated_bytes: None,
        },
        formulation,
    ))
}

fn boolean_family(family: &str, variables: u64) -> VariableFamilyMetrics {
    VariableFamilyMetrics {
        family: family.to_string(),
        total_variables: variables,
        boolean_variables: variables,
        integer_variables: 0,
        domains: constant_domain_summary(2, variables),
    }
}

fn integer_family(
    family: &str,
    variables: u64,
    domains: DomainCardinalitySummary,
) -> VariableFamilyMetrics {
    VariableFamilyMetrics {
        family: family.to_string(),
        total_variables: variables,
        boolean_variables: 0,
        integer_variables: variables,
        domains,
    }
}

fn constant_domain_summary(cardinality: u64, variables: u64) -> DomainCardinalitySummary {
    DomainCardinalitySummary {
        minimum: cardinality,
        maximum: cardinality,
        p50: cardinality,
        p95: cardinality,
        log2_volume: if cardinality > 0 {
            variables as f64 * (cardinality as f64).log2()
        } else {
            0.0
        },
    }
}

fn weighted_domain_summary(domains: &[u64], weight: u64) -> DomainCardinalitySummary {
    if domains.is_empty() {
        return constant_domain_summary(0, 0);
    }
    let mut sorted = domains.to_vec();
    sorted.sort_unstable();
    DomainCardinalitySummary {
        minimum: sorted[0],
        maximum: *sorted.last().expect("non-empty domains have a maximum"),
        p50: percentile(&sorted, 50),
        p95: percentile(&sorted, 95),
        log2_volume: sorted
            .iter()
            .map(|domain| (*domain as f64).log2() * weight as f64)
            .sum(),
    }
}

fn graph_metrics(
    wiring: &FacilityInstanceWiringReport,
    components: &[crate::layouts::FacilityGrowthComponent],
) -> GraphStructureMetrics {
    let facilities = wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            FacilityInstanceWiringNode::Facility { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let index = facilities
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut directed = vec![BTreeSet::new(); facilities.len()];
    let mut undirected = vec![BTreeSet::new(); facilities.len()];
    for edge in &wiring.edges {
        let (Some(source), Some(target)) = (
            index.get(edge.source.as_str()),
            index.get(edge.target.as_str()),
        ) else {
            continue;
        };
        directed[*source].insert(*target);
        undirected[*source].insert(*target);
        undirected[*target].insert(*source);
    }
    let edge_count = directed.iter().map(BTreeSet::len).sum::<usize>() as u64;
    let degree_values = (0..facilities.len())
        .map(|node| {
            let incoming = directed
                .iter()
                .filter(|targets| targets.contains(&node))
                .count();
            (incoming + directed[node].len()) as u64
        })
        .collect::<Vec<_>>();
    let weak_components = count_components(&undirected);
    let component_by_id = components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect::<BTreeMap<_, _>>();
    let mut depth_memo = BTreeMap::new();
    let depths = components
        .iter()
        .map(|component| component_depth(&component.id, &component_by_id, &mut depth_memo))
        .collect::<Vec<_>>();
    let mut width_by_depth = BTreeMap::<u64, u64>::new();
    for depth in depths {
        *width_by_depth.entry(depth).or_default() += 1;
    }
    let cyclic_scc_count = components
        .iter()
        .filter(|component| {
            component.facilities.len() > 1
                || component.facilities.iter().any(|facility| {
                    let node = index[facility.as_str()];
                    directed[node].contains(&node)
                })
        })
        .count() as u64;
    let vertices = facilities.len() as u64;
    let density_denominator = vertices.saturating_mul(vertices.saturating_sub(1));

    GraphStructureMetrics {
        vertices,
        edges: edge_count,
        weak_components,
        mean_degree: if vertices == 0 {
            0.0
        } else {
            degree_values.iter().sum::<u64>() as f64 / vertices as f64
        },
        maximum_degree: degree_values.iter().copied().max().unwrap_or(0),
        p95_degree: percentile(&degree_values, 95),
        density: if density_denominator == 0 {
            0.0
        } else {
            edge_count as f64 / density_denominator as f64
        },
        articulation_points: None,
        biconnected_blocks: None,
        scc_count: components.len() as u64,
        cyclic_scc_count,
        maximum_scc_size: components
            .iter()
            .map(|component| component.facilities.len() as u64)
            .max()
            .unwrap_or(0),
        condensation_depth: depth_memo.values().copied().max().unwrap_or(0),
        maximum_condensation_width: width_by_depth.values().copied().max().unwrap_or(0),
    }
}

fn count_components(adjacency: &[BTreeSet<usize>]) -> u64 {
    let mut visited = vec![false; adjacency.len()];
    let mut components = 0_u64;
    for start in 0..adjacency.len() {
        if visited[start] {
            continue;
        }
        components += 1;
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(node) = stack.pop() {
            for neighbor in &adjacency[node] {
                if !visited[*neighbor] {
                    visited[*neighbor] = true;
                    stack.push(*neighbor);
                }
            }
        }
    }
    components
}

fn component_depth(
    id: &str,
    components: &BTreeMap<&str, &crate::layouts::FacilityGrowthComponent>,
    memo: &mut BTreeMap<String, u64>,
) -> u64 {
    if let Some(depth) = memo.get(id) {
        return *depth;
    }
    let component = components[id];
    let depth = component
        .downstream_components
        .iter()
        .map(|downstream| component_depth(downstream, components, memo) + 1)
        .max()
        .unwrap_or(0);
    memo.insert(id.to_string(), depth);
    depth
}

fn distribution(values: &[u64]) -> CountDistribution {
    if values.is_empty() {
        return CountDistribution {
            samples: 0,
            total: 0,
            minimum: 0,
            maximum: 0,
            p50: 0,
            p95: 0,
        };
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    CountDistribution {
        samples: sorted.len() as u64,
        total: sorted.iter().sum(),
        minimum: sorted[0],
        maximum: *sorted.last().expect("non-empty values have a maximum"),
        p50: percentile(&sorted, 50),
        p95: percentile(&sorted, 95),
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = (sorted.len() - 1) * percentile / 100;
    sorted[index]
}

fn log2_product(values: &[u64]) -> f64 {
    values
        .iter()
        .map(|value| {
            if *value == 0 {
                0.0
            } else {
                (*value as f64).log2()
            }
        })
        .sum()
}

fn checked_add(left: u64, right: u64) -> Result<u64, IntegratedLayoutDiagnostic> {
    left.checked_add(right).ok_or_else(overflow)
}

fn checked_mul(left: u64, right: u64) -> Result<u64, IntegratedLayoutDiagnostic> {
    left.checked_mul(right).ok_or_else(overflow)
}

fn to_u64<T>(value: T) -> Result<u64, IntegratedLayoutDiagnostic>
where
    u64: TryFrom<T>,
{
    u64::try_from(value).map_err(|_| overflow())
}

fn overflow() -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "search-analysis-arithmetic-overflow",
        "/",
        None,
        "search-space analysis exceeded its checked integer domain",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{
        FacilityDefinition, FacilityFootprint, FacilityPortDirection, FacilityPortPosition,
    };

    fn instance() -> InstanceInput {
        InstanceInput {
            id: "facility:0000".to_string(),
            recipe: "test-recipe".to_string(),
            facility: "test-facility".to_string(),
            definition: FacilityDefinition {
                id: "test-facility".to_string(),
                footprint: FacilityFootprint {
                    width: 4,
                    height: 4,
                },
                allowed_rotations: vec![0],
                ports: Vec::new(),
            },
        }
    }

    #[test]
    fn endpoint_estimate_excludes_flush_boundary_origins() {
        let ports = vec![FacilityPortDefinition {
            id: "north-input".to_string(),
            direction: FacilityPortDirection::Input,
            transport: TransportKind::Belt,
            position: FacilityPortPosition { x: 0, y: 0 },
            edge: FacilityPortEdge::North,
        }];

        let placements = placement_count(&instance(), 50, 50).expect("count should fit");
        let options = endpoint_option_count(&instance(), &ports, 50, 50)
            .expect("endpoint options should fit");

        assert_eq!(placements, 47 * 47);
        assert_eq!(options, 47 * 46);
    }
}
