use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::facilities::FacilityPortEdge;
use crate::layouts::FacilityPlacement;
use crate::logistics::{LogisticsComponentKind, TransportKind};
use crate::recipes::{FacilityInstanceWiringProjectedEndpoint, FacilityInstanceWiringProjection};

use super::{
    IntegratedLayoutDiagnostic, IntegratedLayoutReport, IntegratedRoute, IntegratedRouteEndpoint,
    ModelInput, PlacedLogisticsComponent, RouteRequirementFingerprint,
};

pub const CUMULATIVE_GRAPH_KEY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CumulativeGraphKey {
    pub schema_version: u32,
    pub facilities: Vec<FacilityGraphRecord>,
    pub requirements: Vec<RequirementGraphRecord>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityGraphRecord {
    pub facility_instance_id: String,
    pub flattened_facility_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RequirementGraphRecord {
    pub requirement_id: String,
    pub fingerprint: RouteRequirementFingerprint,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CumulativeGraphFingerprint {
    pub sha256_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedRoutingState {
    pub graph_key: CumulativeGraphKey,
    pub graph_fingerprint: CumulativeGraphFingerprint,
    pub retained_placements: BTreeMap<String, FacilityPlacement>,
    pub retained_routes: BTreeMap<String, IntegratedRoute>,
    pub retained_components: BTreeMap<String, RetainedComponent>,
    pub occupied_cells_by_transport:
        BTreeMap<TransportKind, BTreeMap<GridCellKey, RetainedOccupant>>,
    pub selected_ports: BTreeMap<String, SelectedPortAssignment>,
    pub invalidated_requirement_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoutingConflict {
    pub code: String,
    pub failed_requirement_ids: Vec<String>,
    pub related_facility_ids: Vec<String>,
    pub related_scc_ids: Vec<String>,
    pub blocked_cells: Vec<GridCellKey>,
    pub blocking_requirement_ids: Vec<String>,
    pub blocking_component_ids: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RetainedRoutingResult {
    pub report: IntegratedLayoutReport,
    pub invalidated_requirement_ids: Vec<String>,
    pub reused_requirement_ids: Vec<String>,
    pub conflict: Option<RoutingConflict>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SelectedPortAssignment {
    pub source: EndpointPortSelection,
    pub target: EndpointPortSelection,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EndpointPortSelection {
    FacilityPort {
        facility_instance_id: String,
        port_id: String,
    },
    ExternalDangling {
        facility_instance_id: String,
        port_id: String,
        side: FacilityPortEdge,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RetainedComponent {
    pub id: String,
    pub component: String,
    pub kind: LogisticsComponentKind,
    pub transport: TransportKind,
    pub cell: GridCellKey,
    pub rotation: i64,
    pub owner_requirement_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct GridCellKey {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RetainedOccupant {
    RetainedRoute { requirement_id: String },
    RetainedComponent { component_id: String },
}

impl RetainedRoutingState {
    pub(super) fn from_validated_report(
        input: &ModelInput,
        report: &IntegratedLayoutReport,
    ) -> Result<Self, IntegratedLayoutDiagnostic> {
        let graph_key = graph_key(input);
        let graph_fingerprint = graph_fingerprint(&graph_key);
        let retained_placements = report
            .placements
            .iter()
            .cloned()
            .map(|placement| (placement.instance.clone(), placement))
            .collect::<BTreeMap<_, _>>();
        if retained_placements.len() != report.placements.len() {
            return Err(invalid(
                "/placements",
                "retained routing state cannot contain duplicate facility instances",
            ));
        }
        let retained_routes = report
            .routes
            .iter()
            .cloned()
            .map(|route| (route.requirement_id.clone(), route))
            .collect::<BTreeMap<_, _>>();
        if retained_routes.len() != report.routes.len() {
            return Err(invalid(
                "/routes",
                "retained routing state cannot contain duplicate requirement IDs",
            ));
        }

        let selected_ports = report
            .routes
            .iter()
            .map(|route| {
                selected_port_assignment(route)
                    .map(|assignment| (route.requirement_id.clone(), assignment))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        validate_selected_port_uniqueness(&selected_ports)?;

        let mut retained_components = BTreeMap::new();
        for component in &report.logistics_components {
            let owners = component_owners(component, &retained_routes);
            if owners.is_empty() {
                return Err(invalid_for(
                    "/logistics_components",
                    &component.id,
                    "retained logistics component has no owning route requirement",
                ));
            }
            let expected_id = logistics_component_id(
                component.kind,
                component.transport,
                component.position.x,
                component.position.y,
                &owners,
            );
            if component.id != expected_id {
                return Err(invalid_for(
                    "/logistics_components",
                    &component.id,
                    format!(
                        "retained logistics component ID does not match its kind, layer, cell, and owner set; expected '{expected_id}'"
                    ),
                ));
            }
            let retained = RetainedComponent {
                id: component.id.clone(),
                component: component.component.clone(),
                kind: component.kind,
                transport: component.transport,
                cell: GridCellKey {
                    x: component.position.x,
                    y: component.position.y,
                },
                rotation: component.rotation,
                owner_requirement_ids: owners,
            };
            if retained_components
                .insert(retained.id.clone(), retained)
                .is_some()
            {
                return Err(invalid_for(
                    "/logistics_components",
                    &component.id,
                    "retained logistics component ID appears more than once",
                ));
            }
        }

        let occupied_cells_by_transport = build_occupancy(&retained_routes, &retained_components)?;
        Ok(Self {
            graph_key,
            graph_fingerprint,
            retained_placements,
            retained_routes,
            retained_components,
            occupied_cells_by_transport,
            selected_ports,
            invalidated_requirement_ids: BTreeSet::new(),
        })
    }
}

fn graph_key(input: &ModelInput) -> CumulativeGraphKey {
    let mut facilities = input
        .instances
        .iter()
        .map(|instance| FacilityGraphRecord {
            facility_instance_id: instance.id.clone(),
            flattened_facility_id: instance.facility.clone(),
        })
        .collect::<Vec<_>>();
    facilities.sort_by(|left, right| left.facility_instance_id.cmp(&right.facility_instance_id));
    let mut requirements = input
        .edges
        .iter()
        .map(|edge| RequirementGraphRecord {
            requirement_id: edge.requirement_id.clone(),
            fingerprint: edge.requirement_fingerprint.clone(),
        })
        .collect::<Vec<_>>();
    requirements.sort_by(|left, right| left.requirement_id.cmp(&right.requirement_id));
    CumulativeGraphKey {
        schema_version: CUMULATIVE_GRAPH_KEY_SCHEMA_VERSION,
        facilities,
        requirements,
    }
}

fn graph_fingerprint(key: &CumulativeGraphKey) -> CumulativeGraphFingerprint {
    let mut digest = Sha256::new();
    digest.update(key.schema_version.to_be_bytes());
    for facility in &key.facilities {
        hash_text(&mut digest, &facility.facility_instance_id);
        hash_text(&mut digest, &facility.flattened_facility_id);
    }
    for requirement in &key.requirements {
        hash_text(&mut digest, &requirement.requirement_id);
        hash_text(&mut digest, &requirement.fingerprint.source);
        hash_text(&mut digest, &requirement.fingerprint.target);
        hash_text(&mut digest, &requirement.fingerprint.item);
        digest.update(requirement.fingerprint.rate.numerator.to_be_bytes());
        digest.update(requirement.fingerprint.rate.denominator.to_be_bytes());
        hash_text(
            &mut digest,
            match requirement.fingerprint.transport {
                TransportKind::Belt => "belt",
                TransportKind::Pipe => "pipe",
            },
        );
        match &requirement.fingerprint.projection {
            FacilityInstanceWiringProjection::Original => hash_text(&mut digest, "original"),
            FacilityInstanceWiringProjection::FrontierExternal {
                missing_facility,
                original_endpoint,
            } => {
                hash_text(&mut digest, "frontier-external");
                hash_text(&mut digest, missing_facility);
                hash_text(
                    &mut digest,
                    match original_endpoint {
                        FacilityInstanceWiringProjectedEndpoint::Source => "source",
                        FacilityInstanceWiringProjectedEndpoint::Target => "target",
                    },
                );
            }
        }
    }
    CumulativeGraphFingerprint {
        sha256_hex: hex_digest(digest),
    }
}

fn hash_text(digest: &mut Sha256, text: &str) {
    digest.update((text.len() as u64).to_be_bytes());
    digest.update(text.as_bytes());
}

fn selected_port_assignment(
    route: &IntegratedRoute,
) -> Result<SelectedPortAssignment, IntegratedLayoutDiagnostic> {
    Ok(SelectedPortAssignment {
        source: endpoint_selection(&route.source, &route.target, &route.requirement_id)?,
        target: endpoint_selection(&route.target, &route.source, &route.requirement_id)?,
    })
}

fn endpoint_selection(
    endpoint: &IntegratedRouteEndpoint,
    peer: &IntegratedRouteEndpoint,
    requirement_id: &str,
) -> Result<EndpointPortSelection, IntegratedLayoutDiagnostic> {
    match endpoint {
        IntegratedRouteEndpoint::Facility { instance, port } => {
            Ok(EndpointPortSelection::FacilityPort {
                facility_instance_id: instance.clone(),
                port_id: port.clone(),
            })
        }
        IntegratedRouteEndpoint::External { side, .. } => {
            let IntegratedRouteEndpoint::Facility { instance, port } = peer else {
                return Err(invalid_for(
                    "/routes",
                    requirement_id,
                    "external-to-external route cannot define a dangling selected port",
                ));
            };
            Ok(EndpointPortSelection::ExternalDangling {
                facility_instance_id: instance.clone(),
                port_id: port.clone(),
                side: *side,
            })
        }
    }
}

fn validate_selected_port_uniqueness(
    assignments: &BTreeMap<String, SelectedPortAssignment>,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let mut owner_by_port = BTreeMap::<(String, String), &str>::new();
    for (requirement_id, assignment) in assignments {
        let ports = [&assignment.source, &assignment.target]
            .into_iter()
            .map(port_identity)
            .collect::<BTreeSet<_>>();
        for port in ports {
            if let Some(owner) = owner_by_port.insert(port.clone(), requirement_id) {
                return Err(invalid_for(
                    "/routes",
                    requirement_id,
                    format!(
                        "selected facility port '{}:{}' is already owned by requirement '{owner}'",
                        port.0, port.1
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn port_identity(selection: &EndpointPortSelection) -> (String, String) {
    match selection {
        EndpointPortSelection::FacilityPort {
            facility_instance_id,
            port_id,
        }
        | EndpointPortSelection::ExternalDangling {
            facility_instance_id,
            port_id,
            ..
        } => (facility_instance_id.clone(), port_id.clone()),
    }
}

fn component_owners(
    component: &PlacedLogisticsComponent,
    routes: &BTreeMap<String, IntegratedRoute>,
) -> BTreeSet<String> {
    routes
        .iter()
        .filter(|(_, route)| {
            route.transport == component.transport
                && route
                    .cells
                    .iter()
                    .any(|cell| cell.x == component.position.x && cell.y == component.position.y)
        })
        .map(|(requirement_id, _)| requirement_id.clone())
        .collect()
}

fn build_occupancy(
    routes: &BTreeMap<String, IntegratedRoute>,
    components: &BTreeMap<String, RetainedComponent>,
) -> Result<
    BTreeMap<TransportKind, BTreeMap<GridCellKey, RetainedOccupant>>,
    IntegratedLayoutDiagnostic,
> {
    let mut occupancy = BTreeMap::<TransportKind, BTreeMap<GridCellKey, RetainedOccupant>>::new();
    for component in components.values() {
        let layer = occupancy.entry(component.transport).or_default();
        if layer
            .insert(
                component.cell,
                RetainedOccupant::RetainedComponent {
                    component_id: component.id.clone(),
                },
            )
            .is_some()
        {
            return Err(invalid_for(
                "/logistics_components",
                &component.id,
                "more than one retained component occupies the same transport-layer cell",
            ));
        }
    }
    for (requirement_id, route) in routes {
        let layer = occupancy.entry(route.transport).or_default();
        for cell in &route.cells {
            let key = GridCellKey {
                x: cell.x,
                y: cell.y,
            };
            match layer.get(&key) {
                Some(RetainedOccupant::RetainedComponent { component_id }) => {
                    let component = &components[component_id];
                    if !component.owner_requirement_ids.contains(requirement_id) {
                        return Err(invalid_for(
                            "/routes",
                            requirement_id,
                            format!(
                                "route occupies retained component '{}' without ownership",
                                component.id
                            ),
                        ));
                    }
                }
                Some(RetainedOccupant::RetainedRoute {
                    requirement_id: owner,
                }) => {
                    return Err(invalid_for(
                        "/routes",
                        requirement_id,
                        format!(
                            "route shares a transport-layer cell with retained requirement '{owner}' without a component"
                        ),
                    ));
                }
                None => {
                    layer.insert(
                        key,
                        RetainedOccupant::RetainedRoute {
                            requirement_id: requirement_id.clone(),
                        },
                    );
                }
            }
        }
    }
    Ok(occupancy)
}

pub(super) fn logistics_component_id(
    kind: LogisticsComponentKind,
    transport: TransportKind,
    x: i64,
    y: i64,
    owners: &BTreeSet<String>,
) -> String {
    let mut digest = Sha256::new();
    hash_text(
        &mut digest,
        match kind {
            LogisticsComponentKind::Splitter => "splitter",
            LogisticsComponentKind::Converger => "converger",
            LogisticsComponentKind::Bridge => "bridge",
        },
    );
    hash_text(
        &mut digest,
        match transport {
            TransportKind::Belt => "belt",
            TransportKind::Pipe => "pipe",
        },
    );
    digest.update(x.to_be_bytes());
    digest.update(y.to_be_bytes());
    for owner in owners {
        hash_text(&mut digest, owner);
    }
    format!("component:{}", hex_digest(digest))
}

fn hex_digest(digest: Sha256) -> String {
    use std::fmt::Write;

    digest
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}

fn invalid(path: impl Into<String>, message: impl Into<String>) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error("invalid-retained-routing-state", path, None, message)
}

fn invalid_for(
    path: impl Into<String>,
    entity: &str,
    message: impl Into<String>,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "invalid-retained-routing-state",
        path,
        Some(entity.to_string()),
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_identity_is_owner_order_independent_and_cell_sensitive() {
        let owners = ["route-b".to_string(), "route-a".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let same = ["route-a".to_string(), "route-b".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();

        let first = logistics_component_id(
            LogisticsComponentKind::Bridge,
            TransportKind::Belt,
            4,
            7,
            &owners,
        );
        assert_eq!(
            first,
            logistics_component_id(
                LogisticsComponentKind::Bridge,
                TransportKind::Belt,
                4,
                7,
                &same,
            )
        );
        assert_ne!(
            first,
            logistics_component_id(
                LogisticsComponentKind::Bridge,
                TransportKind::Belt,
                5,
                7,
                &same,
            )
        );
    }
}
