use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc as SyncArc, Mutex};

use pumpkin_solver::core::branching::{Brancher, BrancherEvent, SelectionContext};
use pumpkin_solver::core::predicates::Predicate;
use pumpkin_solver::core::results::SolutionReference;
use pumpkin_solver::core::statistics::StatisticLogger;
use pumpkin_solver::core::variables::DomainId;
use serde::Serialize;

use super::{
    FactoredEndpointKind, MaterialJunctionArcProbe, MaterialJunctionProbe, MaterialSeparatorProbe,
    ModelInput, ModelInstance, PlacementChoice, SharedLayer, SharedTerminal,
    SharedTerminalEndpoint, TransportKind, direction_between, direction_index, edge_direction,
    geometry_key, opposite_direction,
};
use crate::facilities::FacilityPortDirection;
use crate::layouts::integrated::exact::recorder::RecordedVariableDescriptor;

pub const ROOT_DOMAIN_SNAPSHOT_SCHEMA_VERSION: u32 = 6;

pub(in crate::layouts::integrated) type RootDomainSnapshotCollector =
    SyncArc<Mutex<Option<RootDomainSnapshot>>>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootDomainCardinality {
    pub lower_bound: i32,
    pub upper_bound: i32,
    pub span: i64,
    pub cardinality: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootBooleanDomainCounts {
    pub fixed_true: usize,
    pub fixed_false: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootFlowDomainCounts {
    pub positive_lower_bound: usize,
    pub zero_upper_bound: usize,
    pub unresolved: usize,
    pub width_histogram: BTreeMap<i64, usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootVariableFamilySnapshot {
    pub family: String,
    pub total: usize,
    pub fixed: usize,
    pub unresolved: usize,
    pub declared_cardinality_sum: u64,
    pub root_cardinality_histogram: BTreeMap<usize, usize>,
    pub root_span_histogram: BTreeMap<i64, usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootVariableCoverageSnapshot {
    pub solver_domains: usize,
    pub registered_domains: usize,
    pub unregistered_domains: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootFacilityStateSnapshot {
    pub instance: String,
    pub placement_choice: RootDomainCardinality,
    pub possible_x_values: Vec<i32>,
    pub possible_y_values: Vec<i32>,
    pub possible_rotations: Vec<i64>,
    pub expected_fixed: bool,
    pub fixed_contract_satisfied: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootExternalGeometrySnapshot {
    pub routable_sides: Vec<String>,
    pub routable_unique_cells: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootTerminalDomainSnapshot {
    pub terminal: String,
    pub network_index: usize,
    pub network_id: String,
    pub transport: TransportKind,
    pub direction: FacilityPortDirection,
    pub flow_units: i32,
    pub endpoint_kind: String,
    pub facility_instance: Option<String>,
    pub external_node: Option<String>,
    pub geometry: RootDomainCardinality,
    pub root_geometry_values: Vec<i32>,
    pub port_choice: Option<RootDomainCardinality>,
    pub declared_port_count: usize,
    pub root_excluded_port_count: usize,
    pub geometry_unavailable_port_count: usize,
    pub port_ids: Vec<String>,
    pub root_surviving_port_ids: Vec<String>,
    pub singleton_geometry_key: Option<i32>,
    pub expected_geometry_keys: Vec<i32>,
    pub routing_options: RootBooleanDomainCounts,
    pub endpoint_continuation_arcs: Vec<RootEndpointContinuationArcSnapshot>,
    pub external_geometry: Option<RootExternalGeometrySnapshot>,
    pub requested_fixed_port: Option<String>,
    pub explicitly_fixed_facility_terminal: bool,
    pub fixed_contract_satisfied: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootEndpointContinuationArcSnapshot {
    pub terminal_cell: usize,
    pub terminal_arm_direction: usize,
    pub from: usize,
    pub to: usize,
    pub route_selected: RootDomainCardinality,
    pub flow: RootDomainCardinality,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootTransportLayerSnapshot {
    pub transport: TransportKind,
    pub grid_cells: usize,
    pub route_cells: RootBooleanDomainCounts,
    pub boundary_route_cells: RootBooleanDomainCounts,
    pub interior_route_cells: RootBooleanDomainCounts,
    pub route_arcs: RootBooleanDomainCounts,
    pub boundary_route_arcs: RootBooleanDomainCounts,
    pub interior_route_arcs: RootBooleanDomainCounts,
    pub arm_item_cardinality_histogram: BTreeMap<usize, usize>,
    pub flows: RootFlowDomainCounts,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootMaterialNetworkSnapshot {
    pub network_index: usize,
    pub network_id: String,
    pub transport: TransportKind,
    pub item: String,
    pub possible_supply_options: usize,
    pub possible_demand_options: usize,
    pub material_capable_possible_arcs: usize,
    pub reachable_demand_options: usize,
    pub unreachable_demand_options: usize,
    pub all_possible_demands_reachable: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootMaterialSeparatorArcSnapshot {
    pub case_index: usize,
    pub from: usize,
    pub to: usize,
    pub route_selected: RootDomainCardinality,
    pub flow: RootDomainCardinality,
    pub from_item: RootDomainCardinality,
    pub from_item_values: Vec<i32>,
    pub selected_item_code: i32,
    pub selected_item_possible: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootMaterialSeparatorSnapshot {
    pub network_id: String,
    pub network_index: usize,
    pub transport: TransportKind,
    pub item: String,
    pub selected_item_code: i32,
    pub separator_after_row: usize,
    pub selected_case_index: Option<usize>,
    pub candidates: Vec<RootMaterialSeparatorArcSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootMaterialJunctionArcSnapshot {
    pub case_index: Option<usize>,
    pub from: usize,
    pub to: usize,
    pub direction: String,
    pub route_selected: RootDomainCardinality,
    pub flow: RootDomainCardinality,
    pub from_item: RootDomainCardinality,
    pub from_item_values: Vec<i32>,
    pub selected_item_code: i32,
    pub selected_item_possible: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootMaterialJunctionSnapshot {
    pub network_id: String,
    pub network_index: usize,
    pub transport: TransportKind,
    pub item: String,
    pub selected_item_code: i32,
    pub junction_cell: usize,
    pub selected_case_index: Option<usize>,
    pub incoming: RootMaterialJunctionArcSnapshot,
    pub candidates: Vec<RootMaterialJunctionArcSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootFirstDecisionSnapshot {
    pub domain_id: u32,
    pub semantic_family: String,
    pub semantic_name: String,
    pub predicate: String,
    pub domain: RootDomainCardinality,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RootDomainSnapshot {
    pub schema_version: u32,
    pub capture_status: String,
    pub explicitly_fixed_facility_terminal_count: usize,
    pub fixed_terminal_contract_satisfied: bool,
    pub fixed_facility_contract_satisfied: bool,
    pub non_singleton_facility_terminal_count: usize,
    pub non_singleton_external_terminal_count: usize,
    pub boundary_facility_occupancy: RootBooleanDomainCounts,
    pub interior_facility_occupancy: RootBooleanDomainCounts,
    pub variable_coverage: RootVariableCoverageSnapshot,
    pub variable_families: Vec<RootVariableFamilySnapshot>,
    pub facilities: Vec<RootFacilityStateSnapshot>,
    pub terminals: Vec<RootTerminalDomainSnapshot>,
    pub layers: Vec<RootTransportLayerSnapshot>,
    pub networks: Vec<RootMaterialNetworkSnapshot>,
    pub material_separator: Option<RootMaterialSeparatorSnapshot>,
    pub material_junction: Option<RootMaterialJunctionSnapshot>,
    pub first_decision: Option<RootFirstDecisionSnapshot>,
}

impl RootDomainSnapshot {
    pub(in crate::layouts::integrated) fn root_infeasible_without_brancher_call() -> Self {
        Self {
            schema_version: ROOT_DOMAIN_SNAPSHOT_SCHEMA_VERSION,
            capture_status: "root-infeasible".to_string(),
            explicitly_fixed_facility_terminal_count: 0,
            fixed_terminal_contract_satisfied: false,
            fixed_facility_contract_satisfied: false,
            non_singleton_facility_terminal_count: 0,
            non_singleton_external_terminal_count: 0,
            boundary_facility_occupancy: empty_boolean_counts(),
            interior_facility_occupancy: empty_boolean_counts(),
            variable_coverage: RootVariableCoverageSnapshot {
                solver_domains: 0,
                registered_domains: 0,
                unregistered_domains: 0,
            },
            variable_families: Vec::new(),
            facilities: Vec::new(),
            terminals: Vec::new(),
            layers: Vec::new(),
            networks: Vec::new(),
            material_separator: None,
            material_junction: None,
            first_decision: None,
        }
    }
}

#[derive(Debug, Clone)]
struct TerminalRoutingOptionProbe {
    cell: usize,
    direction: usize,
    selected: DomainId,
}

#[derive(Debug, Clone)]
struct TerminalProbe {
    terminal: String,
    network_index: usize,
    network_id: String,
    transport: TransportKind,
    direction: FacilityPortDirection,
    flow_units: i32,
    key: DomainId,
    kind: FactoredEndpointKind,
    routing_options: Vec<TerminalRoutingOptionProbe>,
    requested_fixed_port: Option<String>,
}

#[derive(Debug, Clone)]
struct FacilityCandidateProbe {
    x: i32,
    y: i32,
    rotation: i64,
    available_ports: BTreeSet<String>,
    port_geometry_keys: BTreeMap<String, i32>,
}

#[derive(Debug, Clone)]
struct FacilityProbe {
    instance: String,
    choice: DomainId,
    candidates: Vec<FacilityCandidateProbe>,
}

#[derive(Debug, Clone)]
struct LayerProbe {
    transport: TransportKind,
    network_indices: Vec<usize>,
    arcs: Vec<super::Arc>,
    route_cells: Vec<DomainId>,
    arm_items: Vec<[DomainId; 4]>,
}

#[derive(Debug, Clone)]
pub(super) struct RootDomainProbe {
    width: i32,
    height: i32,
    facilities: Vec<FacilityProbe>,
    facility_occupancy: Vec<DomainId>,
    terminals: Vec<TerminalProbe>,
    layers: Vec<LayerProbe>,
    variable_catalog: Vec<RecordedVariableDescriptor>,
    variable_descriptors: BTreeMap<DomainId, RecordedVariableDescriptor>,
    network_ids: Vec<String>,
    network_items: Vec<String>,
    material_separator: Option<MaterialSeparatorProbe>,
    material_junction: Option<MaterialJunctionProbe>,
}

impl RootDomainProbe {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        input: &ModelInput,
        instances: &[ModelInstance],
        placement_choices: &BTreeMap<String, PlacementChoice>,
        terminals: &[Vec<SharedTerminal>],
        layers: &[SharedLayer],
        facility_occupancy: &[DomainId],
        explicitly_fixed_ports: &BTreeMap<String, String>,
        material_separator: Option<&MaterialSeparatorProbe>,
        material_junction: Option<&MaterialJunctionProbe>,
        variable_catalog: Vec<RecordedVariableDescriptor>,
    ) -> Self {
        let facilities = instances
            .iter()
            .map(|instance| FacilityProbe {
                instance: instance.input.id.clone(),
                choice: placement_choices[&instance.input.id].choice,
                candidates: instance
                    .candidates
                    .iter()
                    .map(|candidate| FacilityCandidateProbe {
                        x: candidate.x,
                        y: candidate.y,
                        rotation: candidate.rotation,
                        available_ports: candidate.port_connections.keys().cloned().collect(),
                        port_geometry_keys: instance
                            .input
                            .definition
                            .ports
                            .iter()
                            .filter_map(|port| {
                                candidate.port_connections.get(&port.id).map(|cell| {
                                    let outward = edge_direction(
                                        port.edge.rotated_clockwise(candidate.rotation),
                                    );
                                    (
                                        port.id.clone(),
                                        geometry_key(*cell, opposite_direction(outward)),
                                    )
                                })
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
        let terminals = terminals
            .iter()
            .enumerate()
            .flat_map(|(network_index, network_terminals)| {
                network_terminals.iter().filter_map(move |terminal| {
                    let SharedTerminalEndpoint::Factored { key, kind } = &terminal.endpoint else {
                        return None;
                    };
                    Some(TerminalProbe {
                        terminal: terminal.id.clone(),
                        network_index,
                        network_id: input.networks[network_index].id().to_string(),
                        transport: input.networks[network_index].transport(),
                        direction: terminal.direction,
                        flow_units: terminal.flow_units,
                        key: *key,
                        kind: kind.clone(),
                        routing_options: terminal
                            .routing_options
                            .iter()
                            .map(|option| TerminalRoutingOptionProbe {
                                cell: option.cell,
                                direction: direction_index(option.arm_direction),
                                selected: option.selected,
                            })
                            .collect(),
                        requested_fixed_port: explicitly_fixed_ports.get(&terminal.id).cloned(),
                    })
                })
            })
            .collect();
        let layers = layers
            .iter()
            .map(|layer| LayerProbe {
                transport: layer.transport,
                network_indices: layer.network_indices.clone(),
                arcs: layer.arcs.clone(),
                route_cells: layer.route_cells.clone(),
                arm_items: layer.arm_items.clone(),
            })
            .collect();
        let variable_descriptors = variable_catalog
            .iter()
            .cloned()
            .map(|descriptor| (descriptor.domain, descriptor))
            .collect();
        Self {
            width: input.width,
            height: input.height,
            facilities,
            facility_occupancy: facility_occupancy.to_vec(),
            terminals,
            layers,
            variable_catalog,
            variable_descriptors,
            network_ids: input
                .networks
                .iter()
                .map(|network| network.id().to_string())
                .collect(),
            network_items: input
                .networks
                .iter()
                .map(|network| network.item().to_string())
                .collect(),
            material_separator: material_separator.cloned(),
            material_junction: material_junction.cloned(),
        }
    }

    fn capture(&self, context: &SelectionContext) -> RootDomainSnapshot {
        let solver_domains = context.get_domains().count();
        let registered_domains = self.variable_catalog.len();
        let variable_coverage = RootVariableCoverageSnapshot {
            solver_domains,
            registered_domains,
            unregistered_domains: solver_domains.saturating_sub(registered_domains),
        };
        let variable_families = self.capture_variable_families(context);
        let facilities = self.capture_facilities(context);
        let terminals = self.capture_terminals(context);
        let layers = self.capture_layers(context);
        let networks = self.capture_networks(context);
        let material_separator = self.capture_material_separator(context);
        let material_junction = self.capture_material_junction(context);
        let boundary_cells = (0..self.facility_occupancy.len())
            .filter(|cell| self.is_boundary_cell(*cell))
            .collect::<BTreeSet<_>>();
        let boundary_facility_domains = boundary_cells
            .iter()
            .map(|cell| self.facility_occupancy[*cell])
            .collect::<Vec<_>>();
        let interior_facility_domains = (0..self.facility_occupancy.len())
            .filter(|cell| !boundary_cells.contains(cell))
            .map(|cell| self.facility_occupancy[cell])
            .collect::<Vec<_>>();
        let explicitly_fixed_facility_terminal_count = terminals
            .iter()
            .filter(|terminal| terminal.explicitly_fixed_facility_terminal)
            .count();
        let fixed_terminal_contract_satisfied = terminals
            .iter()
            .filter(|terminal| terminal.explicitly_fixed_facility_terminal)
            .all(|terminal| terminal.fixed_contract_satisfied);
        let fixed_facility_contract_satisfied = facilities
            .iter()
            .filter(|facility| facility.expected_fixed)
            .all(|facility| facility.fixed_contract_satisfied);
        let non_singleton_facility_terminal_count = terminals
            .iter()
            .filter(|terminal| {
                terminal.endpoint_kind == "facility"
                    && (terminal.geometry.cardinality > 1
                        || terminal
                            .port_choice
                            .as_ref()
                            .is_some_and(|domain| domain.cardinality > 1))
            })
            .count();
        let non_singleton_external_terminal_count = terminals
            .iter()
            .filter(|terminal| {
                terminal.endpoint_kind == "external" && terminal.geometry.cardinality > 1
            })
            .count();

        RootDomainSnapshot {
            schema_version: ROOT_DOMAIN_SNAPSHOT_SCHEMA_VERSION,
            capture_status: "captured-before-first-decision".to_string(),
            explicitly_fixed_facility_terminal_count,
            fixed_terminal_contract_satisfied,
            fixed_facility_contract_satisfied,
            non_singleton_facility_terminal_count,
            non_singleton_external_terminal_count,
            boundary_facility_occupancy: boolean_counts(context, &boundary_facility_domains),
            interior_facility_occupancy: boolean_counts(context, &interior_facility_domains),
            variable_coverage,
            variable_families,
            facilities,
            terminals,
            layers,
            networks,
            material_separator,
            material_junction,
            first_decision: None,
        }
    }

    fn capture_material_separator(
        &self,
        context: &SelectionContext,
    ) -> Option<RootMaterialSeparatorSnapshot> {
        self.material_separator.as_ref().map(|probe| {
            let candidates = probe
                .candidates
                .iter()
                .map(|candidate| {
                    let from_item_values = domain_values(context, candidate.from_item);
                    RootMaterialSeparatorArcSnapshot {
                        case_index: candidate.case_index,
                        from: candidate.from,
                        to: candidate.to,
                        route_selected: cardinality(context, candidate.route_selected),
                        flow: cardinality(context, candidate.flow),
                        from_item: cardinality(context, candidate.from_item),
                        selected_item_possible: from_item_values
                            .contains(&candidate.selected_item_code),
                        from_item_values,
                        selected_item_code: candidate.selected_item_code,
                    }
                })
                .collect();
            RootMaterialSeparatorSnapshot {
                network_id: probe.network_id.clone(),
                network_index: probe.network_index,
                transport: probe.transport,
                item: probe.item.clone(),
                selected_item_code: probe.selected_item_code,
                separator_after_row: probe.separator_after_row,
                selected_case_index: probe.selected_case_index,
                candidates,
            }
        })
    }

    fn capture_material_junction(
        &self,
        context: &SelectionContext,
    ) -> Option<RootMaterialJunctionSnapshot> {
        self.material_junction.as_ref().map(|probe| {
            let capture = |candidate: &MaterialJunctionArcProbe| {
                let from_item_values = domain_values(context, candidate.from_item);
                RootMaterialJunctionArcSnapshot {
                    case_index: candidate.case_index,
                    from: candidate.from,
                    to: candidate.to,
                    direction: candidate.direction.clone(),
                    route_selected: cardinality(context, candidate.route_selected),
                    flow: cardinality(context, candidate.flow),
                    from_item: cardinality(context, candidate.from_item),
                    selected_item_possible: from_item_values
                        .contains(&candidate.selected_item_code),
                    from_item_values,
                    selected_item_code: candidate.selected_item_code,
                }
            };
            RootMaterialJunctionSnapshot {
                network_id: probe.network_id.clone(),
                network_index: probe.network_index,
                transport: probe.transport,
                item: probe.item.clone(),
                selected_item_code: probe.selected_item_code,
                junction_cell: probe.junction_cell,
                selected_case_index: probe.selected_case_index,
                incoming: capture(&probe.incoming),
                candidates: probe.candidates.iter().map(capture).collect(),
            }
        })
    }

    fn capture_variable_families(
        &self,
        context: &SelectionContext,
    ) -> Vec<RootVariableFamilySnapshot> {
        let mut families = BTreeMap::<String, RootVariableFamilySnapshot>::new();
        for descriptor in &self.variable_catalog {
            let domain = cardinality(context, descriptor.domain);
            let family = descriptor.family.name().to_string();
            let report =
                families
                    .entry(family.clone())
                    .or_insert_with(|| RootVariableFamilySnapshot {
                        family,
                        total: 0,
                        fixed: 0,
                        unresolved: 0,
                        declared_cardinality_sum: 0,
                        root_cardinality_histogram: BTreeMap::new(),
                        root_span_histogram: BTreeMap::new(),
                    });
            report.total += 1;
            if domain.cardinality == 1 {
                report.fixed += 1;
            } else {
                report.unresolved += 1;
            }
            report.declared_cardinality_sum += descriptor.declared_cardinality;
            *report
                .root_cardinality_histogram
                .entry(domain.cardinality)
                .or_default() += 1;
            *report.root_span_histogram.entry(domain.span).or_default() += 1;
        }
        families.into_values().collect()
    }

    fn capture_facilities(&self, context: &SelectionContext) -> Vec<RootFacilityStateSnapshot> {
        self.facilities
            .iter()
            .map(|facility| {
                let surviving = domain_values(context, facility.choice)
                    .into_iter()
                    .map(|index| usize::try_from(index).expect("placement index is non-negative"))
                    .collect::<Vec<_>>();
                let possible_x_values = surviving
                    .iter()
                    .map(|index| facility.candidates[*index].x)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let possible_y_values = surviving
                    .iter()
                    .map(|index| facility.candidates[*index].y)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let possible_rotations = surviving
                    .iter()
                    .map(|index| facility.candidates[*index].rotation)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let placement_choice = cardinality(context, facility.choice);
                RootFacilityStateSnapshot {
                    instance: facility.instance.clone(),
                    fixed_contract_satisfied: placement_choice.cardinality == 1
                        && possible_x_values.len() == 1
                        && possible_y_values.len() == 1
                        && possible_rotations.len() == 1,
                    placement_choice,
                    possible_x_values,
                    possible_y_values,
                    possible_rotations,
                    expected_fixed: true,
                }
            })
            .collect()
    }

    fn capture_terminals(&self, context: &SelectionContext) -> Vec<RootTerminalDomainSnapshot> {
        self.terminals
            .iter()
            .map(|probe| {
                let root_geometry_values = domain_values(context, probe.key);
                let geometry = cardinality(context, probe.key);
                let (
                    endpoint_kind,
                    facility_instance,
                    external_node,
                    port_choice,
                    port_ids,
                    root_surviving_port_ids,
                    expected_geometry_keys,
                    geometry_unavailable_port_count,
                ) = match &probe.kind {
                    FactoredEndpointKind::Facility {
                        instance,
                        port_choice,
                        port_ids,
                    } => {
                        let facility = self
                            .facilities
                            .iter()
                            .find(|facility| facility.instance == *instance)
                            .expect("facility terminal has a modeled facility");
                        let surviving_candidates = domain_values(context, facility.choice)
                            .into_iter()
                            .map(|index| {
                                &facility.candidates[usize::try_from(index)
                                    .expect("placement index is non-negative")]
                            })
                            .collect::<Vec<_>>();
                        let unavailable = port_ids
                            .iter()
                            .filter(|port| {
                                !surviving_candidates
                                    .iter()
                                    .any(|candidate| candidate.available_ports.contains(*port))
                            })
                            .count();
                        let surviving_port_indices = domain_values(context, *port_choice);
                        let root_surviving_port_ids = surviving_port_indices
                            .iter()
                            .map(|index| {
                                port_ids
                                    [usize::try_from(*index).expect("port index is non-negative")]
                                .clone()
                            })
                            .collect::<Vec<_>>();
                        let expected_geometry_keys = surviving_candidates
                            .iter()
                            .flat_map(|candidate| {
                                root_surviving_port_ids.iter().filter_map(|port| {
                                    candidate.port_geometry_keys.get(port).copied()
                                })
                            })
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>();
                        (
                            "facility".to_string(),
                            Some(instance.clone()),
                            None,
                            Some(cardinality(context, *port_choice)),
                            port_ids.clone(),
                            root_surviving_port_ids,
                            expected_geometry_keys,
                            unavailable,
                        )
                    }
                    FactoredEndpointKind::External { node } => (
                        "external".to_string(),
                        None,
                        Some(node.clone()),
                        None,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        0,
                    ),
                };
                let declared_port_count = port_ids.len();
                let root_excluded_port_count = port_choice.as_ref().map_or(0, |domain| {
                    declared_port_count.saturating_sub(domain.cardinality)
                });
                let external_geometry = (endpoint_kind == "external").then(|| {
                    let routable_options = probe
                        .routing_options
                        .iter()
                        .filter(|option| context.contains(option.selected, 1))
                        .collect::<Vec<_>>();
                    let routable_sides = routable_options
                        .iter()
                        .map(|option| {
                            side_name(
                                i32::try_from(option.direction)
                                    .expect("routing option direction fits i32"),
                            )
                            .to_string()
                        })
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    let routable_unique_cells = routable_options
                        .iter()
                        .map(|option| option.cell)
                        .collect::<BTreeSet<_>>()
                        .len();
                    RootExternalGeometrySnapshot {
                        routable_sides,
                        routable_unique_cells,
                    }
                });
                let singleton_geometry_key =
                    (geometry.cardinality == 1).then_some(geometry.lower_bound);
                let mut endpoint_continuation_arcs = self
                    .layers
                    .iter()
                    .find(|layer| layer.transport == probe.transport)
                    .into_iter()
                    .flat_map(|layer| {
                        probe.routing_options.iter().filter_map(move |option| {
                            context
                                .contains(option.selected, 1)
                                .then_some((layer, option))
                        })
                    })
                    .flat_map(|(layer, option)| {
                        layer.arcs.iter().filter_map(move |arc| {
                            let incident = match probe.direction {
                                FacilityPortDirection::Output => arc.from == option.cell,
                                FacilityPortDirection::Input => arc.to == option.cell,
                            };
                            (incident
                                && context.contains(arc.selected, 1)
                                && context.upper_bound(arc.flow) > 0)
                                .then(|| RootEndpointContinuationArcSnapshot {
                                    terminal_cell: option.cell,
                                    terminal_arm_direction: option.direction,
                                    from: arc.from,
                                    to: arc.to,
                                    route_selected: cardinality(context, arc.selected),
                                    flow: cardinality(context, arc.flow),
                                })
                        })
                    })
                    .collect::<Vec<_>>();
                endpoint_continuation_arcs.sort_by_key(|arc| {
                    (
                        arc.terminal_cell,
                        arc.terminal_arm_direction,
                        arc.from,
                        arc.to,
                    )
                });
                endpoint_continuation_arcs.dedup_by_key(|arc| {
                    (
                        arc.terminal_cell,
                        arc.terminal_arm_direction,
                        arc.from,
                        arc.to,
                    )
                });
                let fixed_contract_satisfied =
                    probe
                        .requested_fixed_port
                        .as_ref()
                        .is_none_or(|requested_port| {
                            root_surviving_port_ids.len() == 1
                                && root_surviving_port_ids[0] == *requested_port
                                && expected_geometry_keys.len() == 1
                                && singleton_geometry_key == expected_geometry_keys.first().copied()
                        });
                RootTerminalDomainSnapshot {
                    terminal: probe.terminal.clone(),
                    network_index: probe.network_index,
                    network_id: probe.network_id.clone(),
                    transport: probe.transport,
                    direction: probe.direction,
                    flow_units: probe.flow_units,
                    endpoint_kind,
                    facility_instance,
                    external_node,
                    geometry,
                    root_geometry_values,
                    port_choice,
                    declared_port_count,
                    root_excluded_port_count,
                    geometry_unavailable_port_count,
                    port_ids,
                    root_surviving_port_ids,
                    singleton_geometry_key,
                    expected_geometry_keys,
                    routing_options: boolean_counts(
                        context,
                        &probe
                            .routing_options
                            .iter()
                            .map(|option| option.selected)
                            .collect::<Vec<_>>(),
                    ),
                    endpoint_continuation_arcs,
                    external_geometry,
                    requested_fixed_port: probe.requested_fixed_port.clone(),
                    explicitly_fixed_facility_terminal: probe.requested_fixed_port.is_some(),
                    fixed_contract_satisfied,
                }
            })
            .collect()
    }

    fn capture_layers(&self, context: &SelectionContext) -> Vec<RootTransportLayerSnapshot> {
        self.layers
            .iter()
            .map(|layer| {
                let boundary_cells = layer
                    .route_cells
                    .iter()
                    .enumerate()
                    .filter_map(|(cell, domain)| self.is_boundary_cell(cell).then_some(*domain))
                    .collect::<Vec<_>>();
                let interior_cells = layer
                    .route_cells
                    .iter()
                    .enumerate()
                    .filter_map(|(cell, domain)| (!self.is_boundary_cell(cell)).then_some(*domain))
                    .collect::<Vec<_>>();
                let boundary_arcs = layer
                    .arcs
                    .iter()
                    .filter(|arc| self.is_boundary_cell(arc.from) || self.is_boundary_cell(arc.to))
                    .map(|arc| arc.selected)
                    .collect::<Vec<_>>();
                let interior_arcs = layer
                    .arcs
                    .iter()
                    .filter(|arc| {
                        !self.is_boundary_cell(arc.from) && !self.is_boundary_cell(arc.to)
                    })
                    .map(|arc| arc.selected)
                    .collect::<Vec<_>>();
                let mut histogram = BTreeMap::new();
                for item in layer.arm_items.iter().flatten() {
                    *histogram
                        .entry(cardinality(context, *item).cardinality)
                        .or_default() += 1;
                }
                RootTransportLayerSnapshot {
                    transport: layer.transport,
                    grid_cells: layer.route_cells.len(),
                    route_cells: boolean_counts(context, &layer.route_cells),
                    boundary_route_cells: boolean_counts(context, &boundary_cells),
                    interior_route_cells: boolean_counts(context, &interior_cells),
                    route_arcs: boolean_counts(
                        context,
                        &layer
                            .arcs
                            .iter()
                            .map(|arc| arc.selected)
                            .collect::<Vec<_>>(),
                    ),
                    boundary_route_arcs: boolean_counts(context, &boundary_arcs),
                    interior_route_arcs: boolean_counts(context, &interior_arcs),
                    arm_item_cardinality_histogram: histogram,
                    flows: flow_counts(
                        context,
                        &layer.arcs.iter().map(|arc| arc.flow).collect::<Vec<_>>(),
                    ),
                }
            })
            .collect()
    }

    fn capture_networks(&self, context: &SelectionContext) -> Vec<RootMaterialNetworkSnapshot> {
        let terminals_by_network = self.terminals.iter().fold(
            BTreeMap::<usize, Vec<&TerminalProbe>>::new(),
            |mut map, terminal| {
                map.entry(terminal.network_index)
                    .or_default()
                    .push(terminal);
                map
            },
        );
        let mut reports = Vec::new();
        for layer in &self.layers {
            for (local_index, network_index) in layer.network_indices.iter().copied().enumerate() {
                let item_code = i32::try_from(local_index + 1).expect("item code fits i32");
                let terminals = terminals_by_network
                    .get(&network_index)
                    .cloned()
                    .unwrap_or_default();
                let cells_for = |direction| {
                    terminals
                        .iter()
                        .filter(|terminal| terminal.direction == direction)
                        .flat_map(|terminal| {
                            terminal.routing_options.iter().filter_map(|option| {
                                context.contains(option.selected, 1).then_some(option.cell)
                            })
                        })
                        .collect::<Vec<_>>()
                };
                let supply_cells = cells_for(FacilityPortDirection::Output);
                let demand_cells = cells_for(FacilityPortDirection::Input);
                let mut adjacency = vec![Vec::new(); layer.route_cells.len()];
                let mut possible_arc_count = 0;
                for arc in &layer.arcs {
                    let from_direction =
                        direction_index(direction_between(arc.from, arc.to, self.width));
                    let to_direction =
                        direction_index(direction_between(arc.to, arc.from, self.width));
                    if context.contains(arc.selected, 1)
                        && context.upper_bound(arc.flow) > 0
                        && context.contains(layer.arm_items[arc.from][from_direction], item_code)
                        && context.contains(layer.arm_items[arc.to][to_direction], item_code)
                    {
                        adjacency[arc.from].push(arc.to);
                        possible_arc_count += 1;
                    }
                }
                let reachable = reachable_cells(&adjacency, &supply_cells);
                let reachable_demand_options = demand_cells
                    .iter()
                    .filter(|cell| reachable.contains(cell))
                    .count();
                let unreachable_demand_options =
                    demand_cells.len().saturating_sub(reachable_demand_options);
                reports.push(RootMaterialNetworkSnapshot {
                    network_index,
                    network_id: self.network_ids[network_index].clone(),
                    transport: layer.transport,
                    item: self.network_items[network_index].clone(),
                    possible_supply_options: supply_cells.len(),
                    possible_demand_options: demand_cells.len(),
                    material_capable_possible_arcs: possible_arc_count,
                    reachable_demand_options,
                    unreachable_demand_options,
                    all_possible_demands_reachable: !demand_cells.is_empty()
                        && unreachable_demand_options == 0,
                });
            }
        }
        reports.sort_by_key(|report| report.network_index);
        reports
    }

    fn describe_decision(
        &self,
        context: &SelectionContext,
        decision: Predicate,
    ) -> RootFirstDecisionSnapshot {
        let domain = decision.get_domain();
        let descriptor = self.variable_descriptors.get(&domain);
        RootFirstDecisionSnapshot {
            domain_id: domain.id(),
            semantic_family: descriptor.map_or_else(
                || "unregistered".to_string(),
                |descriptor| descriptor.family.name().to_string(),
            ),
            semantic_name: descriptor.map_or_else(
                || format!("domain-{}", domain.id()),
                |descriptor| descriptor.name.clone(),
            ),
            predicate: format!("{decision:?}"),
            domain: cardinality(context, domain),
        }
    }

    fn is_boundary_cell(&self, cell: usize) -> bool {
        let width = usize::try_from(self.width).expect("validated width is positive");
        let height = usize::try_from(self.height).expect("validated height is positive");
        let x = cell % width;
        let y = cell / width;
        x == 0 || y == 0 || x + 1 == width || y + 1 == height
    }
}

#[derive(Debug)]
pub(super) struct RootSnapshotBrancher<B> {
    inner: B,
    snapshot: Option<(RootDomainProbe, RootDomainSnapshotCollector)>,
}

impl<B> RootSnapshotBrancher<B> {
    pub(super) fn new(
        inner: B,
        snapshot: Option<(RootDomainProbe, RootDomainSnapshotCollector)>,
    ) -> Self {
        Self { inner, snapshot }
    }
}

impl<B: Brancher> Brancher for RootSnapshotBrancher<B> {
    fn log_statistics(&self, statistic_logger: StatisticLogger) {
        self.inner.log_statistics(statistic_logger);
    }

    fn next_decision(&mut self, context: &mut SelectionContext) -> Option<Predicate> {
        let Some((probe, collector)) = self.snapshot.take() else {
            return self.inner.next_decision(context);
        };
        let mut snapshot = probe.capture(context);
        let decision = self.inner.next_decision(context);
        snapshot.first_decision =
            decision.map(|decision| probe.describe_decision(context, decision));
        if decision.is_none() {
            snapshot.capture_status = "root-solved-without-decision".to_string();
        }
        *collector
            .lock()
            .expect("root-domain snapshot collector is not poisoned") = Some(snapshot);
        decision
    }

    fn on_conflict(&mut self) {
        self.inner.on_conflict();
    }

    fn on_backtrack(&mut self) {
        self.inner.on_backtrack();
    }

    fn on_solution(&mut self, solution: SolutionReference) {
        self.inner.on_solution(solution);
    }

    fn on_unassign_integer(&mut self, variable: DomainId, value: i32) {
        self.inner.on_unassign_integer(variable, value);
    }

    fn on_appearance_in_conflict_predicate(&mut self, predicate: Predicate) {
        self.inner.on_appearance_in_conflict_predicate(predicate);
    }

    fn on_restart(&mut self) {
        self.inner.on_restart();
    }

    fn synchronise(&mut self, context: &mut SelectionContext) {
        self.inner.synchronise(context);
    }

    fn is_restart_pointless(&mut self) -> bool {
        self.inner.is_restart_pointless()
    }

    fn subscribe_to_events(&self) -> Vec<BrancherEvent> {
        self.inner.subscribe_to_events()
    }
}

fn cardinality(context: &SelectionContext, domain: DomainId) -> RootDomainCardinality {
    let lower_bound = context.lower_bound(domain);
    let upper_bound = context.upper_bound(domain);
    RootDomainCardinality {
        lower_bound,
        upper_bound,
        span: i64::from(upper_bound) - i64::from(lower_bound),
        cardinality: (lower_bound..=upper_bound)
            .filter(|value| context.contains(domain, *value))
            .count(),
    }
}

fn domain_values(context: &SelectionContext, domain: DomainId) -> Vec<i32> {
    (context.lower_bound(domain)..=context.upper_bound(domain))
        .filter(|value| context.contains(domain, *value))
        .collect()
}

fn empty_boolean_counts() -> RootBooleanDomainCounts {
    RootBooleanDomainCounts {
        fixed_true: 0,
        fixed_false: 0,
        unresolved: 0,
    }
}

fn boolean_counts(context: &SelectionContext, domains: &[DomainId]) -> RootBooleanDomainCounts {
    let mut counts = empty_boolean_counts();
    for domain in domains {
        match (context.contains(*domain, 0), context.contains(*domain, 1)) {
            (false, true) => counts.fixed_true += 1,
            (true, false) => counts.fixed_false += 1,
            (true, true) => counts.unresolved += 1,
            (false, false) => unreachable!("consistent root has no empty Boolean domain"),
        }
    }
    counts
}

fn flow_counts(context: &SelectionContext, domains: &[DomainId]) -> RootFlowDomainCounts {
    let mut counts = RootFlowDomainCounts {
        positive_lower_bound: 0,
        zero_upper_bound: 0,
        unresolved: 0,
        width_histogram: BTreeMap::new(),
    };
    for domain in domains {
        let lower = context.lower_bound(*domain);
        let upper = context.upper_bound(*domain);
        if lower > 0 {
            counts.positive_lower_bound += 1;
        } else if upper == 0 {
            counts.zero_upper_bound += 1;
        } else {
            counts.unresolved += 1;
        }
        *counts
            .width_histogram
            .entry(i64::from(upper) - i64::from(lower))
            .or_default() += 1;
    }
    counts
}

fn side_name(direction: i32) -> &'static str {
    match direction {
        0 => "north",
        1 => "east",
        2 => "south",
        3 => "west",
        _ => unreachable!("geometry direction is cardinal"),
    }
}

fn reachable_cells(adjacency: &[Vec<usize>], starts: &[usize]) -> BTreeSet<usize> {
    let mut reached = BTreeSet::new();
    let mut queue = VecDeque::new();
    for start in starts {
        if reached.insert(*start) {
            queue.push_back(*start);
        }
    }
    while let Some(cell) = queue.pop_front() {
        for next in &adjacency[cell] {
            if reached.insert(*next) {
                queue.push_back(*next);
            }
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::reachable_cells;

    #[test]
    fn reachability_follows_only_directed_possible_arcs() {
        let adjacency = vec![vec![1], vec![2], Vec::new(), vec![0]];
        assert_eq!(
            reachable_cells(&adjacency, &[0])
                .into_iter()
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }
}
