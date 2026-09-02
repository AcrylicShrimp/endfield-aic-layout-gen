use std::collections::{BTreeMap, BTreeSet};

use crate::facilities::FacilityPortDirection;
use crate::logistics::{TransportKind, ValidatedTransportCatalog};
use crate::recipes::Rate;

use super::{
    CONSTRUCTIVE_PORT_DEMAND_ANALYSIS_SCHEMA_VERSION, ConstructiveFrontierDiagnostic,
    ConstructivePortDemandAnalysis, ConstructivePortDemandGroup, ConstructivePortDemandScope,
    ConstructiveProcessModuleBoundary, ConstructiveTransportCapacityViolation, TransportNetwork,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    inside_instance: String,
    direction: u8,
    item: String,
    transport: TransportKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScopeKey {
    inside_instance: String,
    direction: u8,
    transport: TransportKind,
}

struct GroupAccumulator {
    direction: FacilityPortDirection,
    requirements: BTreeSet<String>,
    total_rate: Rate,
    ports: BTreeSet<String>,
}

struct ScopeAccumulator {
    direction: FacilityPortDirection,
    item_groups: usize,
    required_ports: usize,
    ports: BTreeSet<String>,
}

pub fn analyze_constructive_port_demands(
    boundaries: &[ConstructiveProcessModuleBoundary],
    networks: &[TransportNetwork],
    transports: &ValidatedTransportCatalog,
) -> ConstructivePortDemandAnalysis {
    let mut diagnostics = Vec::new();
    let mut accumulators = BTreeMap::<GroupKey, GroupAccumulator>::new();

    for (boundary_index, boundary) in boundaries.iter().enumerate() {
        let key = GroupKey {
            inside_instance: boundary.inside_instance.clone(),
            direction: direction_key(boundary.direction),
            item: boundary.item.clone(),
            transport: boundary.transport,
        };
        let accumulator = accumulators.entry(key).or_insert_with(|| GroupAccumulator {
            direction: boundary.direction,
            requirements: BTreeSet::new(),
            total_rate: Rate::zero(),
            ports: BTreeSet::new(),
        });
        accumulator
            .requirements
            .insert(boundary.requirement.clone());
        match accumulator.total_rate.checked_add(boundary.rate) {
            Ok(total) => accumulator.total_rate = total,
            Err(error) => diagnostics.push(ConstructiveFrontierDiagnostic::error(
                "constructive-port-demand-rate-overflow",
                format!("/boundary_requirements/{boundary_index}/rate"),
                Some(boundary.requirement.clone()),
                error.message,
            )),
        }
        for port in &boundary.port_options {
            if port.instance == boundary.inside_instance
                && port.direction == boundary.direction
                && port.transport == boundary.transport
            {
                accumulator.ports.insert(port.port.clone());
            } else {
                diagnostics.push(ConstructiveFrontierDiagnostic::error(
                    "constructive-port-demand-option-mismatch",
                    format!("/boundary_requirements/{boundary_index}/port_options"),
                    Some(boundary.requirement.clone()),
                    format!(
                        "port option '{}' does not match boundary instance, direction, and transport",
                        port.port
                    ),
                ));
            }
        }
    }

    if !diagnostics.is_empty() {
        return failure(boundaries.len(), diagnostics);
    }

    let mut groups = Vec::with_capacity(accumulators.len());
    let mut scopes = BTreeMap::<ScopeKey, ScopeAccumulator>::new();
    for (key, accumulator) in accumulators {
        let capacity_definition = transports.capacity(key.transport);
        let line_capacity = match Rate::from_quantity_per_duration_ms(
            capacity_definition.quantity,
            capacity_definition.duration_ms,
        ) {
            Ok(capacity) => capacity,
            Err(error) => {
                diagnostics.push(ConstructiveFrontierDiagnostic::error(
                    "constructive-port-demand-capacity-overflow",
                    "/transport_catalog",
                    Some(format!("{:?}", key.transport).to_lowercase()),
                    error.message,
                ));
                continue;
            }
        };
        let required_ports = match required_port_count(accumulator.total_rate, line_capacity) {
            Some(required_ports) => required_ports,
            None => {
                diagnostics.push(ConstructiveFrontierDiagnostic::error(
                    "constructive-port-demand-count-overflow",
                    "/boundary_requirements",
                    Some(key.inside_instance.clone()),
                    "capacity-grouped required port count does not fit usize",
                ));
                continue;
            }
        };
        let edge_implied_ports = accumulator.requirements.len();
        let scope_key = ScopeKey {
            inside_instance: key.inside_instance.clone(),
            direction: key.direction,
            transport: key.transport,
        };
        let scope = scopes.entry(scope_key).or_insert_with(|| ScopeAccumulator {
            direction: accumulator.direction,
            item_groups: 0,
            required_ports: 0,
            ports: BTreeSet::new(),
        });
        scope.item_groups += 1;
        let Some(scope_required_ports) = scope.required_ports.checked_add(required_ports) else {
            diagnostics.push(ConstructiveFrontierDiagnostic::error(
                "constructive-port-demand-scope-count-overflow",
                "/boundary_requirements",
                Some(key.inside_instance.clone()),
                "facility port demand count does not fit usize",
            ));
            continue;
        };
        scope.required_ports = scope_required_ports;
        scope.ports.extend(accumulator.ports.iter().cloned());
        groups.push(ConstructivePortDemandGroup {
            inside_instance: key.inside_instance,
            direction: accumulator.direction,
            item: key.item,
            transport: key.transport,
            logical_requirements: accumulator.requirements.into_iter().collect(),
            total_rate: accumulator.total_rate,
            line_capacity,
            edge_implied_ports,
            required_ports,
            available_distinct_ports: accumulator.ports.len(),
        });
    }

    if !diagnostics.is_empty() {
        return failure(boundaries.len(), diagnostics);
    }

    let scopes = scopes
        .into_iter()
        .map(|(key, accumulator)| ConstructivePortDemandScope {
            inside_instance: key.inside_instance,
            direction: accumulator.direction,
            transport: key.transport,
            item_groups: accumulator.item_groups,
            required_ports: accumulator.required_ports,
            available_distinct_ports: accumulator.ports.len(),
            capacity_sufficient: accumulator.ports.len() >= accumulator.required_ports,
        })
        .collect::<Vec<_>>();
    let edge_implied_ports = groups.iter().map(|group| group.edge_implied_ports).sum();
    let required_ports = groups.iter().map(|group| group.required_ports).sum();

    let over_capacity_routed_networks = match analyze_routed_network_capacity(networks, transports)
    {
        Ok(violations) => violations,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            Vec::new()
        }
    };
    if !over_capacity_routed_networks.is_empty() {
        diagnostics.push(ConstructiveFrontierDiagnostic::error(
            "constructive-routed-network-over-capacity",
            "/transport_networks",
            None,
            format!(
                "{} routed transport networks exceed one line's catalog capacity",
                over_capacity_routed_networks.len()
            ),
        ));
    }

    ConstructivePortDemandAnalysis {
        schema_version: CONSTRUCTIVE_PORT_DEMAND_ANALYSIS_SCHEMA_VERSION,
        success: diagnostics.is_empty(),
        boundary_requirements: boundaries.len(),
        capacity_groups: groups.len(),
        edge_implied_ports,
        required_ports,
        edge_implied_port_excess: edge_implied_ports.saturating_sub(required_ports),
        edge_implied_port_deficit: required_ports.saturating_sub(edge_implied_ports),
        routed_networks: networks.len(),
        over_capacity_routed_networks,
        groups,
        scopes,
        diagnostics,
    }
}

fn analyze_routed_network_capacity(
    networks: &[TransportNetwork],
    transports: &ValidatedTransportCatalog,
) -> Result<Vec<ConstructiveTransportCapacityViolation>, ConstructiveFrontierDiagnostic> {
    networks
        .iter()
        .map(|network| {
            let capacity_definition = transports.capacity(network.transport);
            let line_capacity = Rate::from_quantity_per_duration_ms(
                capacity_definition.quantity,
                capacity_definition.duration_ms,
            )
            .map_err(|error| {
                ConstructiveFrontierDiagnostic::error(
                    "constructive-routed-network-capacity-overflow",
                    "/transport_catalog",
                    Some(network.id.clone()),
                    error.message,
                )
            })?;
            let peak_reported_rate = network
                .terminals
                .iter()
                .map(|terminal| terminal.rate)
                .chain(network.segments.iter().map(|segment| segment.rate))
                .max()
                .unwrap_or_else(Rate::zero);
            let required_parallel_lines = required_port_count(peak_reported_rate, line_capacity)
                .ok_or_else(|| {
                    ConstructiveFrontierDiagnostic::error(
                        "constructive-routed-network-line-count-overflow",
                        "/transport_networks",
                        Some(network.id.clone()),
                        "routed network line count does not fit usize",
                    )
                })?;
            Ok(
                (required_parallel_lines > 1).then(|| ConstructiveTransportCapacityViolation {
                    network: network.id.clone(),
                    item: network.item.clone(),
                    transport: network.transport,
                    peak_reported_rate,
                    line_capacity,
                    required_parallel_lines,
                }),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|violations| violations.into_iter().flatten().collect())
}

fn required_port_count(total_rate: Rate, line_capacity: Rate) -> Option<usize> {
    let numerator = (total_rate.numerator as i128) * (line_capacity.denominator as i128);
    let denominator = (total_rate.denominator as i128) * (line_capacity.numerator as i128);
    let rounded_up = numerator.checked_add(denominator - 1)? / denominator;
    usize::try_from(rounded_up).ok()
}

fn direction_key(direction: FacilityPortDirection) -> u8 {
    match direction {
        FacilityPortDirection::Input => 0,
        FacilityPortDirection::Output => 1,
    }
}

pub(super) fn unavailable_constructive_port_demand_analysis(
    code: &'static str,
    message: impl Into<String>,
) -> ConstructivePortDemandAnalysis {
    failure(
        0,
        vec![ConstructiveFrontierDiagnostic::error(
            code, "/", None, message,
        )],
    )
}

fn failure(
    boundary_requirements: usize,
    diagnostics: Vec<ConstructiveFrontierDiagnostic>,
) -> ConstructivePortDemandAnalysis {
    ConstructivePortDemandAnalysis {
        schema_version: CONSTRUCTIVE_PORT_DEMAND_ANALYSIS_SCHEMA_VERSION,
        success: false,
        boundary_requirements,
        capacity_groups: 0,
        edge_implied_ports: 0,
        required_ports: 0,
        edge_implied_port_excess: 0,
        edge_implied_port_deficit: 0,
        routed_networks: 0,
        over_capacity_routed_networks: Vec::new(),
        groups: Vec::new(),
        scopes: Vec::new(),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::FacilityPortEdge;
    use crate::layouts::{PlacedFacilityPort, TransportNetworkSegment, WorldGridPosition};
    use crate::logistics::{
        SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
        TransportDefinition,
    };

    fn transports() -> ValidatedTransportCatalog {
        ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 2_000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        })
        .expect("transport catalog validates")
    }

    fn boundary(requirement: &str, item: &str, rate: Rate) -> ConstructiveProcessModuleBoundary {
        ConstructiveProcessModuleBoundary {
            requirement: requirement.to_string(),
            item: item.to_string(),
            transport: TransportKind::Belt,
            rate,
            direction: FacilityPortDirection::Input,
            inside_instance: "machine".to_string(),
            port_options: ["input-1", "input-2"]
                .into_iter()
                .map(|port| PlacedFacilityPort {
                    instance: "machine".to_string(),
                    facility: "facility".to_string(),
                    port: port.to_string(),
                    direction: FacilityPortDirection::Input,
                    transport: TransportKind::Belt,
                    position: WorldGridPosition { x: 0, y: 0 },
                    edge: FacilityPortEdge::West,
                    connection: WorldGridPosition { x: -1, y: 0 },
                })
                .collect(),
        }
    }

    #[test]
    fn merges_same_item_edges_before_counting_ports() {
        let analysis = analyze_constructive_port_demands(
            &[
                boundary(
                    "edge-a",
                    "ore",
                    Rate {
                        numerator: 1,
                        denominator: 4,
                    },
                ),
                boundary(
                    "edge-b",
                    "ore",
                    Rate {
                        numerator: 1,
                        denominator: 4,
                    },
                ),
            ],
            &[],
            &transports(),
        );

        assert!(analysis.success, "{:?}", analysis.diagnostics);
        assert_eq!(analysis.edge_implied_ports, 2);
        assert_eq!(analysis.required_ports, 1);
        assert_eq!(analysis.edge_implied_port_excess, 1);
        assert_eq!(analysis.groups.len(), 1);
        assert_eq!(analysis.groups[0].available_distinct_ports, 2);
        assert!(analysis.scopes[0].capacity_sufficient);
    }

    #[test]
    fn sums_required_ports_across_distinct_items() {
        let analysis = analyze_constructive_port_demands(
            &[
                boundary(
                    "edge-a",
                    "ore-a",
                    Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                ),
                boundary(
                    "edge-b",
                    "ore-b",
                    Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                ),
            ],
            &[],
            &transports(),
        );

        assert_eq!(analysis.capacity_groups, 2);
        assert_eq!(analysis.required_ports, 2);
        assert_eq!(analysis.scopes.len(), 1);
        assert_eq!(analysis.scopes[0].required_ports, 2);
        assert!(analysis.scopes[0].capacity_sufficient);
    }

    #[test]
    fn rate_above_one_line_reports_multiple_required_ports() {
        let analysis = analyze_constructive_port_demands(
            &[boundary(
                "edge-a",
                "ore",
                Rate {
                    numerator: 5,
                    denominator: 4,
                },
            )],
            &[],
            &transports(),
        );

        assert_eq!(analysis.edge_implied_ports, 1);
        assert_eq!(analysis.required_ports, 3);
        assert_eq!(analysis.edge_implied_port_deficit, 2);
        assert!(!analysis.scopes[0].capacity_sufficient);
    }

    #[test]
    fn reports_routed_networks_that_exceed_line_capacity() {
        let network = TransportNetwork {
            id: "network".to_string(),
            requirement_ids: vec!["edge".to_string()],
            item: "ore".to_string(),
            transport: TransportKind::Belt,
            cells: vec![
                WorldGridPosition { x: 0, y: 0 },
                WorldGridPosition { x: 1, y: 0 },
            ],
            segments: vec![TransportNetworkSegment {
                from: WorldGridPosition { x: 0, y: 0 },
                to: WorldGridPosition { x: 1, y: 0 },
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
            }],
            terminals: Vec::new(),
            component_ids: Vec::new(),
        };

        let analysis = analyze_constructive_port_demands(&[], &[network], &transports());

        assert!(!analysis.success);
        assert_eq!(analysis.routed_networks, 1);
        assert_eq!(analysis.over_capacity_routed_networks.len(), 1);
        assert_eq!(
            analysis.over_capacity_routed_networks[0].required_parallel_lines,
            2
        );
        assert_eq!(
            analysis.diagnostics[0].code,
            "constructive-routed-network-over-capacity"
        );
    }
}
