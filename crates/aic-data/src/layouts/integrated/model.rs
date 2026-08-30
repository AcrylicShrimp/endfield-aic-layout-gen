use std::collections::{BTreeMap, BTreeSet};

use crate::facilities::{
    FacilityDefinition, FacilityPortDefinition, FacilityPortDirection, ValidatedFacilityCatalog,
};
use crate::layouts::{FacilityPlacementRequest, validate_facility_placement_request};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedItemCatalog,
    ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
    FacilityInstanceWiringNode, FacilityInstanceWiringProjection, FacilityInstanceWiringReport,
    Rate, facility_instance_wiring_edge_id,
};

use super::{IntegratedLayoutDiagnostic, networks};

#[derive(Clone)]
pub(super) struct ModelInput {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) cell_count: i32,
    pub(super) instances: Vec<InstanceInput>,
    pub(super) edges: Vec<EdgeInput>,
    pub(super) networks: Vec<networks::RoutingNetworkInput>,
}

#[derive(Clone)]
pub(super) struct EdgeInput {
    pub(super) requirement_id: String,
    pub(super) edge: FacilityInstanceWiringEdge,
    pub(super) source: EndpointInput,
    pub(super) target: EndpointInput,
    pub(super) transport: TransportKind,
    pub(super) capacity_rate: Rate,
    pub(super) component_capacity_rates: ComponentCapacityRates,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ComponentCapacityRates {
    pub(super) splitter: Rate,
    pub(super) converger: Rate,
    pub(super) bridge: Rate,
}

impl ModelInput {
    pub(super) fn select_network_indices(
        mut self,
        indices: &[usize],
    ) -> Result<(Self, Vec<String>), IntegratedLayoutDiagnostic> {
        let mut selected_indices = BTreeSet::new();
        for index in indices {
            if *index >= self.networks.len() {
                return Err(IntegratedLayoutDiagnostic::error(
                    "research-network-index-out-of-range",
                    "/network_indices",
                    Some(index.to_string()),
                    format!(
                        "research network index {index} is outside the available range 0..{}",
                        self.networks.len()
                    ),
                ));
            }
            if !selected_indices.insert(*index) {
                return Err(IntegratedLayoutDiagnostic::error(
                    "duplicate-research-network-index",
                    "/network_indices",
                    Some(index.to_string()),
                    format!("research network index {index} was selected more than once"),
                ));
            }
        }
        if selected_indices.is_empty() {
            return Err(IntegratedLayoutDiagnostic::error(
                "empty-research-network-selection",
                "/network_indices",
                None,
                "research network selection must contain at least one network",
            ));
        }

        let selected_network_ids = selected_indices
            .iter()
            .map(|index| self.networks[*index].id().to_string())
            .collect::<Vec<_>>();
        let selected_edges = selected_indices
            .iter()
            .flat_map(|index| self.networks[*index].route_indices().iter().copied())
            .collect::<BTreeSet<_>>();
        self.edges = self
            .edges
            .into_iter()
            .enumerate()
            .filter_map(|(index, edge)| selected_edges.contains(&index).then_some(edge))
            .collect();
        self.networks = networks::normalize(&self.edges)?;

        let rebuilt_ids = self
            .networks
            .iter()
            .map(|network| network.id().to_string())
            .collect::<BTreeSet<_>>();
        if selected_network_ids
            .iter()
            .any(|network| !rebuilt_ids.contains(network))
        {
            return Err(IntegratedLayoutDiagnostic::error(
                "research-network-selection-rebuild-mismatch",
                "/network_indices",
                None,
                "selected research networks did not survive edge-level model reconstruction",
            ));
        }
        Ok((self, selected_network_ids))
    }
}

impl ComponentCapacityRates {
    pub(super) fn values(self) -> [Rate; 3] {
        [self.splitter, self.converger, self.bridge]
    }
}

#[derive(Clone)]
pub(super) enum EndpointInput {
    Facility {
        instance: String,
        ports: Vec<FacilityPortDefinition>,
    },
    External {
        node: String,
    },
}

#[derive(Clone)]
pub(super) struct InstanceInput {
    pub(super) id: String,
    pub(super) recipe: String,
    pub(super) facility: String,
    pub(super) definition: FacilityDefinition,
}

pub(super) fn required_facility_area(
    input: &ModelInput,
) -> Result<i64, IntegratedLayoutDiagnostic> {
    input.instances.iter().try_fold(0_i64, |total, instance| {
        let area = instance
            .definition
            .footprint
            .width
            .checked_mul(instance.definition.footprint.height)
            .ok_or_else(|| facility_area_overflow(&instance.id))?;
        total
            .checked_add(area)
            .ok_or_else(|| facility_area_overflow(&instance.id))
    })
}

pub(super) fn prepare_model(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
) -> Result<ModelInput, IntegratedLayoutDiagnostic> {
    if !instance_wiring.success {
        return Err(IntegratedLayoutDiagnostic::error(
            "upstream-instance-wiring-failed",
            "/",
            None,
            "integrated layout requires successful facility instance wiring",
        ));
    }
    if instance_wiring.schema_version != FACILITY_INSTANCE_WIRING_SCHEMA_VERSION {
        return Err(IntegratedLayoutDiagnostic::error(
            "unsupported-facility-instance-wiring-schema-version",
            "/schema_version",
            None,
            format!(
                "facility instance wiring schema version {} is unsupported; expected {}",
                instance_wiring.schema_version, FACILITY_INSTANCE_WIRING_SCHEMA_VERSION
            ),
        ));
    }
    if let Some(diagnostic) = validate_facility_placement_request(request).first() {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-layout-bounds",
            diagnostic.path.clone(),
            diagnostic.entity.clone(),
            diagnostic.message.clone(),
        ));
    }

    let mut node_by_id = BTreeMap::new();
    for (index, node) in instance_wiring.nodes.iter().enumerate() {
        let id = wiring_node_id(node);
        if node_by_id.insert(id, node).is_some() {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-wiring-node",
                format!("/nodes/{index}/id"),
                Some(id.to_string()),
                format!("wiring node '{id}' appears more than once"),
            ));
        }
    }

    let mut instances = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, node) in instance_wiring.nodes.iter().enumerate() {
        let FacilityInstanceWiringNode::Facility {
            id,
            recipe,
            facility,
            ..
        } = node
        else {
            continue;
        };
        if !seen.insert(id.clone()) {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-facility-instance",
                format!("/nodes/{index}/id"),
                Some(id.clone()),
                format!("facility instance '{id}' appears more than once"),
            ));
        }
        let definition = facilities.facility(facility).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-facility-definition",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                format!("facility '{facility}' is absent from the validated catalog"),
            )
        })?;
        i32::try_from(definition.footprint.width).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "solver-domain-out-of-range",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                "facility width does not fit the solver's 32-bit integer domain",
            )
        })?;
        i32::try_from(definition.footprint.height).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "solver-domain-out-of-range",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                "facility height does not fit the solver's 32-bit integer domain",
            )
        })?;
        instances.push(InstanceInput {
            id: id.clone(),
            recipe: recipe.clone(),
            facility: facility.clone(),
            definition: definition.clone(),
        });
    }
    instances.sort_by(|left, right| left.id.cmp(&right.id));

    let mut edges = Vec::new();
    let mut edge_ids = BTreeSet::new();
    for (edge_index, edge) in instance_wiring.edges.iter().cloned().enumerate() {
        if !edge_ids.insert(edge.id.clone()) {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-wiring-edge-id",
                format!("/edges/{edge_index}/id"),
                Some(edge.id.clone()),
                format!("wiring edge ID '{}' appears more than once", edge.id),
            ));
        }
        if matches!(&edge.projection, FacilityInstanceWiringProjection::Original)
            && edge.id
                != facility_instance_wiring_edge_id(
                    &edge.source,
                    &edge.target,
                    &edge.kind,
                    &edge.item,
                )
        {
            return Err(IntegratedLayoutDiagnostic::error(
                "invalid-wiring-edge-id",
                format!("/edges/{edge_index}/id"),
                Some(edge.id.clone()),
                "original wiring edge ID does not match its canonical endpoint, kind, and item tuple",
            ));
        }
        let source_node = node_by_id
            .get(edge.source.as_str())
            .ok_or_else(|| missing_route_endpoint(edge_index, "source", edge.source.as_str()))?;
        let target_node = node_by_id
            .get(edge.target.as_str())
            .ok_or_else(|| missing_route_endpoint(edge_index, "target", edge.target.as_str()))?;
        if edge.source == edge.target {
            return Err(IntegratedLayoutDiagnostic::error(
                "unsupported-self-route",
                format!("/edges/{edge_index}"),
                Some(edge.source.clone()),
                "integrated routing does not support a route from a node to itself",
            ));
        }

        let item = items.item(&edge.item).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-item-definition",
                format!("/edges/{edge_index}/item"),
                Some(edge.item.clone()),
                format!(
                    "item '{}' is absent from the validated item catalog",
                    edge.item
                ),
            )
        })?;
        let capacity = transports.capacity(item.transport);
        let capacity_rate =
            Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms).map_err(
                |_| {
                    IntegratedLayoutDiagnostic::error(
                        "transport-capacity-out-of-range",
                        format!("/edges/{edge_index}/rate"),
                        Some(format!("{:?}", item.transport).to_lowercase()),
                        "transport capacity cannot be represented in the exact rate domain",
                    )
                },
            )?;
        let component_capacity_rates = ComponentCapacityRates {
            splitter: component_capacity_rate(
                logistics_components,
                item.transport,
                LogisticsComponentKind::Splitter,
                edge_index,
            )?,
            converger: component_capacity_rate(
                logistics_components,
                item.transport,
                LogisticsComponentKind::Converger,
                edge_index,
            )?,
            bridge: component_capacity_rate(
                logistics_components,
                item.transport,
                LogisticsComponentKind::Bridge,
                edge_index,
            )?,
        };
        let source = prepare_endpoint(
            edge_index,
            "source",
            source_node,
            &instances,
            FacilityPortDirection::Output,
            item.transport,
            &edge.item,
        )?;
        let target = prepare_endpoint(
            edge_index,
            "target",
            target_node,
            &instances,
            FacilityPortDirection::Input,
            item.transport,
            &edge.item,
        )?;
        if matches!(
            (&source, &target),
            (
                EndpointInput::External { .. },
                EndpointInput::External { .. }
            )
        ) {
            return Err(IntegratedLayoutDiagnostic::error(
                "unsupported-external-to-external-route",
                format!("/edges/{edge_index}"),
                Some(edge.id.clone()),
                "integrated routing requires at least one facility endpoint",
            ));
        }

        let mut remaining_rate = edge.rate;
        let mut lane_index = 0_usize;
        while !remaining_rate.is_zero() {
            let route_rate = remaining_rate.min(capacity_rate);
            let requirement_id = format!("{}:lane:{lane_index:04}", edge.id);
            edges.push(EdgeInput {
                requirement_id,
                source: source.clone(),
                target: target.clone(),
                transport: item.transport,
                capacity_rate,
                component_capacity_rates,
                edge: FacilityInstanceWiringEdge {
                    rate: route_rate,
                    ..edge.clone()
                },
            });
            remaining_rate = remaining_rate.checked_sub(route_rate).map_err(|_| {
                IntegratedLayoutDiagnostic::error(
                    "route-rate-arithmetic-overflow",
                    format!("/edges/{edge_index}/rate"),
                    Some(edge.item.clone()),
                    "route capacity splitting exceeded the exact rate domain",
                )
            })?;
            lane_index += 1;
        }
    }

    let width = i32::try_from(request.max_width).map_err(|_| solver_domain_error("max_width"))?;
    let height =
        i32::try_from(request.max_height).map_err(|_| solver_domain_error("max_height"))?;
    let cell_count = width
        .checked_mul(height)
        .ok_or_else(grid_area_domain_error)?;
    let networks = networks::normalize(&edges)?;

    Ok(ModelInput {
        width,
        height,
        cell_count,
        instances,
        edges,
        networks,
    })
}

fn component_capacity_rate(
    components: &ValidatedLogisticsComponentCatalog,
    transport: TransportKind,
    kind: LogisticsComponentKind,
    edge_index: usize,
) -> Result<Rate, IntegratedLayoutDiagnostic> {
    let capacity = &components
        .component_by_kind(transport, kind)
        .expect("validated catalog contains every logistics component capability")
        .capacity;
    Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms).map_err(|_| {
        IntegratedLayoutDiagnostic::error(
            "logistics-component-capacity-out-of-range",
            format!("/edges/{edge_index}/rate"),
            Some(format!("{transport:?}-{kind:?}").to_lowercase()),
            "logistics component capacity cannot be represented in the exact rate domain",
        )
    })
}

fn facility_area_overflow(instance: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "facility-area-arithmetic-overflow",
        "/",
        Some(instance.to_string()),
        "required facility area exceeds the exact layout area domain",
    )
}

fn wiring_node_id(node: &FacilityInstanceWiringNode) -> &str {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id,
    }
}

fn prepare_endpoint(
    edge_index: usize,
    endpoint_kind: &str,
    node: &FacilityInstanceWiringNode,
    instances: &[InstanceInput],
    port_direction: FacilityPortDirection,
    transport: TransportKind,
    item: &str,
) -> Result<EndpointInput, IntegratedLayoutDiagnostic> {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. } => {
            let instance = instances
                .iter()
                .find(|instance| instance.id == *id)
                .expect("every prepared facility node has an instance");
            let ports = compatible_ports(&instance.definition, port_direction, transport);
            if ports.is_empty() {
                let direction = match port_direction {
                    FacilityPortDirection::Input => "input",
                    FacilityPortDirection::Output => "output",
                };
                return Err(missing_compatible_port(
                    edge_index, id, direction, transport,
                ));
            }
            Ok(EndpointInput::Facility {
                instance: id.clone(),
                ports,
            })
        }
        FacilityInstanceWiringNode::External {
            id,
            item: node_item,
        } if endpoint_kind == "source" => {
            prepare_external_endpoint(edge_index, endpoint_kind, id, node_item, item)
        }
        FacilityInstanceWiringNode::Target {
            id,
            item: node_item,
        }
        | FacilityInstanceWiringNode::Surplus {
            id,
            item: node_item,
        } if endpoint_kind == "target" => {
            prepare_external_endpoint(edge_index, endpoint_kind, id, node_item, item)
        }
        _ => Err(IntegratedLayoutDiagnostic::error(
            "invalid-route-endpoint-kind",
            format!("/edges/{edge_index}/{endpoint_kind}"),
            Some(wiring_node_id(node).to_string()),
            format!(
                "wiring node '{}' cannot be used as a route {endpoint_kind}",
                wiring_node_id(node)
            ),
        )),
    }
}

fn prepare_external_endpoint(
    edge_index: usize,
    endpoint_kind: &str,
    node: &str,
    node_item: &str,
    edge_item: &str,
) -> Result<EndpointInput, IntegratedLayoutDiagnostic> {
    if node_item != edge_item {
        return Err(IntegratedLayoutDiagnostic::error(
            "external-item-mismatch",
            format!("/edges/{edge_index}/item"),
            Some(node.to_string()),
            format!(
                "external {endpoint_kind} node '{node}' carries item '{node_item}' but the route carries '{edge_item}'"
            ),
        ));
    }
    Ok(EndpointInput::External {
        node: node.to_string(),
    })
}

fn compatible_ports(
    definition: &FacilityDefinition,
    direction: FacilityPortDirection,
    transport: TransportKind,
) -> Vec<FacilityPortDefinition> {
    definition
        .ports
        .iter()
        .filter(|port| port.direction == direction && port.transport == transport)
        .cloned()
        .collect()
}

fn missing_route_endpoint(
    edge_index: usize,
    kind: &str,
    endpoint: &str,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-route-endpoint",
        format!("/edges/{edge_index}/{kind}"),
        Some(endpoint.to_string()),
        format!("route {kind} node '{endpoint}' is absent from the wiring graph"),
    )
}

fn missing_compatible_port(
    edge_index: usize,
    instance: &str,
    direction: &str,
    transport: TransportKind,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-compatible-port",
        format!("/edges/{edge_index}"),
        Some(instance.to_string()),
        format!("facility instance '{instance}' has no {direction} {transport:?} port"),
    )
}

fn solver_domain_error(field: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "solver-domain-out-of-range",
        format!("/{field}"),
        None,
        format!("layout {field} does not fit the solver's 32-bit integer domain"),
    )
}

fn grid_area_domain_error() -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "solver-grid-area-out-of-range",
        "/",
        None,
        "max_width multiplied by max_height does not fit the solver's 32-bit integer domain",
    )
}
