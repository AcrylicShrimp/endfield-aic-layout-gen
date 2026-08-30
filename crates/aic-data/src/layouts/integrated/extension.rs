use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacement, FacilityPlacementRequest};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::{
    CandidateRank, CumulativeGraphFingerprint, CumulativeGraphKey, DeterministicCandidateKey,
    EndpointInput, IncumbentProvenance, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    LayoutScore, ModelInput, RefinementKind, RetainedRoutingState, RoutingConflict,
    candidate_port_connections, prepare_model, retained, sparse, witness,
};

#[derive(Debug, Clone, Copy)]
enum ExtensionPlacementPolicy {
    ConnectedFirst,
    CompactFirst,
}

const EXTENSION_PLACEMENT_POLICIES: [ExtensionPlacementPolicy; 2] = [
    ExtensionPlacementPolicy::ConnectedFirst,
    ExtensionPlacementPolicy::CompactFirst,
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PhaseIncumbent {
    pub cumulative_graph_key: CumulativeGraphKey,
    pub cumulative_graph_fingerprint: CumulativeGraphFingerprint,
    pub witness: IntegratedLayoutReport,
    pub score: LayoutScore,
    pub candidate_key: DeterministicCandidateKey,
    pub provenance: IncumbentProvenance,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct IncumbentExtensionCounts {
    pub reused_facilities: usize,
    pub newly_placed_facilities: usize,
    pub reused_routes: usize,
    pub invalidated_routes: usize,
    pub rerouted_routes: usize,
    pub new_routes: usize,
    pub reused_components: usize,
    pub invalidated_components: usize,
    pub new_components: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IncumbentExtensionResult {
    pub incumbent: Option<PhaseIncumbent>,
    pub counts: IncumbentExtensionCounts,
    pub conflict: Option<RoutingConflict>,
    pub diagnostics: Vec<IntegratedLayoutDiagnostic>,
}

#[allow(clippy::too_many_arguments)]
pub fn extend_phase_incumbent(
    previous_wiring: &FacilityInstanceWiringReport,
    current_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    prior_witness: &IntegratedLayoutReport,
    phase_index: usize,
    deadline: Instant,
) -> IncumbentExtensionResult {
    let previous_input =
        match prepare_model(previous_wiring, facilities, items, transports, request) {
            Ok(input) => input,
            Err(diagnostic) => return extension_failure(diagnostic, None),
        };
    if let Err(diagnostic) = witness::validate(&previous_input, logistics_components, prior_witness)
    {
        return extension_failure(diagnostic, None);
    }
    let retained = match RetainedRoutingState::from_validated_report(&previous_input, prior_witness)
    {
        Ok(retained) => retained,
        Err(diagnostic) => return extension_failure(diagnostic, None),
    };
    let current_input = match prepare_model(current_wiring, facilities, items, transports, request)
    {
        Ok(input) => input,
        Err(diagnostic) => return extension_failure(diagnostic, None),
    };
    let graph_key = retained::graph_key(&current_input);
    let graph_fingerprint = retained::graph_fingerprint(&graph_key);
    let invalidated = changed_requirements(&current_input, &retained);
    let retained_obstacles = retained_route_obstacles(&retained, &invalidated);
    let reused_facilities = prior_witness.placements.len();
    let mut best = None::<(CandidateRank, PhaseIncumbent, IncumbentExtensionCounts)>;
    let mut last_failure = None;
    for (policy_index, policy) in EXTENSION_PLACEMENT_POLICIES.into_iter().enumerate() {
        let placements = match extend_placements(
            &current_input,
            prior_witness.placements.clone(),
            &retained_obstacles,
            policy,
            deadline,
        ) {
            Ok(placements) => placements,
            Err(failure) => {
                let (diagnostic, conflict) = *failure;
                last_failure = Some(extension_failure(diagnostic, Some(conflict)));
                continue;
            }
        };
        let newly_placed_facilities = placements.len().saturating_sub(reused_facilities);
        let candidate_input =
            match prepare_model(current_wiring, facilities, items, transports, request) {
                Ok(input) => input,
                Err(diagnostic) => return extension_failure(diagnostic, None),
            };
        let routed = sparse::construct_from_retained(
            candidate_input,
            logistics_components,
            placements,
            &retained,
            &invalidated,
            deadline,
        );
        let counts = extension_counts(
            reused_facilities,
            newly_placed_facilities,
            &retained,
            &routed,
        );
        if !routed.report.success {
            last_failure = Some(IncumbentExtensionResult {
                incumbent: None,
                counts,
                conflict: routed.conflict,
                diagnostics: routed.report.diagnostics,
            });
            continue;
        }
        let score = LayoutScore::from_report(&routed.report, &prior_witness.placements)
            .expect("validated incumbent extension must be scoreable");
        let candidate_key = DeterministicCandidateKey {
            phase_index,
            refinement_kind: RefinementKind::IncumbentExtension,
            neighborhood_rank: 0,
            restart_index: 0,
            policy_index,
            attempt_index: policy_index,
            yield_index: 0,
        };
        let rank = CandidateRank {
            score,
            deterministic_candidate_key: candidate_key,
        };
        let incumbent = PhaseIncumbent {
            cumulative_graph_key: graph_key.clone(),
            cumulative_graph_fingerprint: graph_fingerprint.clone(),
            witness: routed.report,
            score,
            candidate_key,
            provenance: IncumbentProvenance::ExtendedPriorPhase,
        };
        if best
            .as_ref()
            .is_none_or(|(best_rank, _, _)| rank < *best_rank)
        {
            best = Some((rank, incumbent, counts));
        }
    }
    if let Some((_, incumbent, counts)) = best {
        return IncumbentExtensionResult {
            incumbent: Some(incumbent),
            counts,
            conflict: None,
            diagnostics: Vec::new(),
        };
    }
    last_failure.unwrap_or_else(|| {
        let conflict = placement_conflict(
            "incumbent-extension-no-candidate",
            "phase",
            "incumbent extension produced no constructive placement candidate",
        );
        extension_failure(conflict_diagnostic(&conflict), Some(conflict))
    })
}

fn extension_counts(
    reused_facilities: usize,
    newly_placed_facilities: usize,
    retained: &RetainedRoutingState,
    routed: &super::RetainedRoutingResult,
) -> IncumbentExtensionCounts {
    let resulting_component_ids = routed
        .report
        .logistics_components
        .iter()
        .map(|component| component.id.as_str())
        .collect::<BTreeSet<_>>();
    IncumbentExtensionCounts {
        reused_facilities,
        newly_placed_facilities,
        reused_routes: routed.reused_requirement_ids.len(),
        invalidated_routes: routed.invalidated_requirement_ids.len(),
        rerouted_routes: routed
            .invalidated_requirement_ids
            .iter()
            .filter(|requirement_id| retained.retained_routes.contains_key(*requirement_id))
            .count(),
        new_routes: routed
            .invalidated_requirement_ids
            .iter()
            .filter(|requirement_id| !retained.retained_routes.contains_key(*requirement_id))
            .count(),
        reused_components: retained
            .retained_components
            .keys()
            .filter(|component_id| resulting_component_ids.contains(component_id.as_str()))
            .count(),
        invalidated_components: retained
            .retained_components
            .keys()
            .filter(|component_id| !resulting_component_ids.contains(component_id.as_str()))
            .count(),
        new_components: resulting_component_ids
            .iter()
            .filter(|component_id| !retained.retained_components.contains_key(**component_id))
            .count(),
    }
}

fn changed_requirements(input: &ModelInput, retained: &RetainedRoutingState) -> BTreeSet<String> {
    input
        .edges
        .iter()
        .filter(|edge| {
            retained
                .retained_routes
                .get(&edge.requirement_id)
                .is_none_or(|route| route.requirement_fingerprint != edge.requirement_fingerprint)
        })
        .map(|edge| edge.requirement_id.clone())
        .collect()
}

fn retained_route_obstacles(
    retained: &RetainedRoutingState,
    invalidated: &BTreeSet<String>,
) -> BTreeSet<(i64, i64)> {
    retained
        .retained_routes
        .iter()
        .filter(|(requirement_id, _)| !invalidated.contains(*requirement_id))
        .flat_map(|(_, route)| route.cells.iter().map(|cell| (cell.x, cell.y)))
        .chain(
            retained
                .retained_components
                .values()
                .filter(|component| {
                    component
                        .owner_requirement_ids
                        .iter()
                        .all(|owner| !invalidated.contains(owner))
                })
                .map(|component| (component.cell.x, component.cell.y)),
        )
        .collect()
}

fn extend_placements(
    input: &ModelInput,
    mut placements: Vec<FacilityPlacement>,
    retained_obstacles: &BTreeSet<(i64, i64)>,
    policy: ExtensionPlacementPolicy,
    deadline: Instant,
) -> Result<Vec<FacilityPlacement>, Box<(IntegratedLayoutDiagnostic, RoutingConflict)>> {
    let placed_ids = placements
        .iter()
        .map(|placement| placement.instance.as_str())
        .collect::<BTreeSet<_>>();
    let new_instances = input
        .instances
        .iter()
        .filter(|instance| !placed_ids.contains(instance.id.as_str()))
        .collect::<Vec<_>>();
    for instance in new_instances {
        if Instant::now() >= deadline {
            let conflict = placement_conflict(
                "incumbent-extension-time-limit",
                &instance.id,
                "incumbent extension exhausted its deadline while placing new facilities",
            );
            return Err(Box::new((conflict_diagnostic(&conflict), conflict)));
        }
        let mut best = None;
        let mut rotations = instance.definition.allowed_rotations.clone();
        rotations.sort_unstable();
        for rotation in rotations {
            let (width, height) = oriented_dimensions(
                instance.definition.footprint.width,
                instance.definition.footprint.height,
                rotation,
            );
            for y in 0..=(i64::from(input.height) - height) {
                if Instant::now() >= deadline {
                    let conflict = placement_conflict(
                        "incumbent-extension-time-limit",
                        &instance.id,
                        "incumbent extension exhausted its deadline while placing new facilities",
                    );
                    return Err(Box::new((conflict_diagnostic(&conflict), conflict)));
                }
                for x in 0..=(i64::from(input.width) - width) {
                    if !placement_candidate_is_legal(
                        input,
                        instance,
                        x,
                        y,
                        width,
                        height,
                        rotation,
                        &placements,
                        retained_obstacles,
                    ) {
                        continue;
                    }
                    let score = placement_extension_score(
                        input,
                        &instance.id,
                        x,
                        y,
                        width,
                        height,
                        &placements,
                        policy,
                    );
                    let candidate = (score, rotation, x, y, width, height);
                    if best.as_ref().is_none_or(|best| candidate < *best) {
                        best = Some(candidate);
                    }
                }
            }
        }
        let Some((_, rotation, x, y, width, height)) = best else {
            let conflict = placement_conflict(
                "incumbent-extension-placement-failed",
                &instance.id,
                "new facility has no legal placement around retained facility and route geometry",
            );
            return Err(Box::new((conflict_diagnostic(&conflict), conflict)));
        };
        placements.push(FacilityPlacement {
            instance: instance.id.clone(),
            recipe: instance.recipe.clone(),
            facility: instance.facility.clone(),
            x,
            y,
            width,
            height,
            rotation,
        });
    }
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));
    Ok(placements)
}

#[allow(clippy::too_many_arguments)]
fn placement_candidate_is_legal(
    input: &ModelInput,
    instance: &super::InstanceInput,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    rotation: i64,
    placements: &[FacilityPlacement],
    retained_obstacles: &BTreeSet<(i64, i64)>,
) -> bool {
    if placements.iter().any(|placed| {
        !rectangles_have_gap(
            x,
            y,
            width,
            height,
            placed.x,
            placed.y,
            placed.width,
            placed.height,
        )
    }) {
        return false;
    }
    if (y..y + height)
        .any(|cell_y| (x..x + width).any(|cell_x| retained_obstacles.contains(&(cell_x, cell_y))))
    {
        return false;
    }
    let connections = candidate_port_connections(
        &instance.definition,
        rotation,
        x as i32,
        y as i32,
        input.width,
        input.height,
    );
    input.edges.iter().all(|edge| {
        endpoint_has_connection(&edge.source, &instance.id, &connections)
            && endpoint_has_connection(&edge.target, &instance.id, &connections)
    })
}

fn endpoint_has_connection(
    endpoint: &EndpointInput,
    instance: &str,
    connections: &BTreeMap<String, usize>,
) -> bool {
    match endpoint {
        EndpointInput::Facility {
            instance: endpoint_instance,
            ports,
        } if endpoint_instance == instance => {
            ports.iter().any(|port| connections.contains_key(&port.id))
        }
        _ => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn rectangles_have_gap(
    left_x: i64,
    left_y: i64,
    left_width: i64,
    left_height: i64,
    right_x: i64,
    right_y: i64,
    right_width: i64,
    right_height: i64,
) -> bool {
    left_x + left_width < right_x
        || right_x + right_width < left_x
        || left_y + left_height < right_y
        || right_y + right_height < left_y
}

fn oriented_dimensions(width: i64, height: i64, rotation: i64) -> (i64, i64) {
    if matches!(rotation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

#[allow(clippy::too_many_arguments)]
fn placement_extension_score(
    input: &ModelInput,
    instance: &str,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    placements: &[FacilityPlacement],
    policy: ExtensionPlacementPolicy,
) -> (u64, u64, i64, i64) {
    let by_instance = placements
        .iter()
        .map(|placement| (placement.instance.as_str(), placement))
        .collect::<BTreeMap<_, _>>();
    let connection_distance = input
        .edges
        .iter()
        .filter_map(|edge| match (&edge.source, &edge.target) {
            (
                EndpointInput::Facility {
                    instance: source, ..
                },
                EndpointInput::Facility {
                    instance: target, ..
                },
            ) if source == instance => by_instance.get(target.as_str()),
            (
                EndpointInput::Facility {
                    instance: source, ..
                },
                EndpointInput::Facility {
                    instance: target, ..
                },
            ) if target == instance => by_instance.get(source.as_str()),
            _ => None,
        })
        .map(|placement| {
            (x + width / 2).abs_diff(placement.x + placement.width / 2)
                + (y + height / 2).abs_diff(placement.y + placement.height / 2)
        })
        .min()
        .unwrap_or(0);
    let maximum_x = placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .max()
        .unwrap_or(0)
        .max(x + width);
    let maximum_y = placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .max()
        .unwrap_or(0)
        .max(y + height);
    let area = u64::try_from(maximum_x)
        .ok()
        .and_then(|maximum_x| {
            u64::try_from(maximum_y)
                .ok()
                .and_then(|maximum_y| maximum_x.checked_mul(maximum_y))
        })
        .unwrap_or(u64::MAX);
    match policy {
        ExtensionPlacementPolicy::ConnectedFirst => (connection_distance, area, y, x),
        ExtensionPlacementPolicy::CompactFirst => (area, connection_distance, y, x),
    }
}

fn placement_conflict(code: &str, facility: &str, message: &str) -> RoutingConflict {
    RoutingConflict {
        code: code.to_string(),
        failed_requirement_ids: Vec::new(),
        related_facility_ids: vec![facility.to_string()],
        related_scc_ids: Vec::new(),
        blocked_cells: Vec::new(),
        blocking_requirement_ids: Vec::new(),
        blocking_component_ids: Vec::new(),
        message: message.to_string(),
    }
}

fn conflict_diagnostic(conflict: &RoutingConflict) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "incumbent-extension-failed",
        "/placements",
        conflict.related_facility_ids.first().cloned(),
        conflict.message.clone(),
    )
}

fn extension_failure(
    diagnostic: IntegratedLayoutDiagnostic,
    conflict: Option<RoutingConflict>,
) -> IncumbentExtensionResult {
    IncumbentExtensionResult {
        incumbent: None,
        counts: IncumbentExtensionCounts::default(),
        conflict,
        diagnostics: vec![diagnostic],
    }
}
