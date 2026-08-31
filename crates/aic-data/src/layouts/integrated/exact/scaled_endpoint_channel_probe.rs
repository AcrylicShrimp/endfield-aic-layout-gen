use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use pumpkin_solver::Solver;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::CSPSolverExecutionFlag;
use pumpkin_solver::core::variables::DomainId;

use crate::facilities::{FacilityPortDefinition, FacilityPortEdge};
use crate::logistics::CardinalDirection;

use super::super::research::{
    EndpointChannelEncoding, EndpointChannelRestriction, EndpointSupportPropagationStatistics,
    SCALED_ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION, ScaledEndpointChannelProbeReport,
    ScaledEndpointDomainSnapshot, ScaledEndpointRestrictionReport,
    ScaledEndpointTerminalDomainSnapshot, ScaledEndpointTerminalScale,
};
use super::super::{EndpointInput, IntegratedLayoutDiagnostic, ModelInput};
use super::endpoint_support_propagator::{
    EndpointSupportPropagationCounters, SparseEndpointSupportPropagatorArgs,
};
use super::formulation::generate_candidate_geometries;

#[derive(Clone)]
struct TerminalRelation {
    id: String,
    port_values: Vec<i32>,
    geometry_values: Vec<i32>,
    values_by_port: Vec<Vec<i32>>,
    rows: Vec<[i32; 3]>,
}

struct TerminalVariables {
    port: DomainId,
    geometry: DomainId,
}

struct ProbeModel {
    solver: Solver,
    placement: DomainId,
    terminals: Vec<TerminalVariables>,
    support_counters: Arc<EndpointSupportPropagationCounters>,
}

#[derive(Clone)]
struct ModelScale {
    authored_integer_variables: usize,
    element_constraints: usize,
    table_constraints: usize,
    table_rows: usize,
    estimated_table_clauses: usize,
}

pub(in crate::layouts::integrated) fn probe_scaled_endpoint_channels(
    input: &ModelInput,
    facility_id: &str,
    target_phase_index: usize,
    encoding: EndpointChannelEncoding,
) -> Result<ScaledEndpointChannelProbeReport, IntegratedLayoutDiagnostic> {
    if !matches!(
        encoding,
        EndpointChannelEncoding::NestedElement
            | EndpointChannelEncoding::PositiveTable
            | EndpointChannelEncoding::SparseSupport
    ) {
        return Err(diagnostic(
            "research-scaled-endpoint-encoding-unsupported",
            Some(format!("{encoding:?}")),
            "scaled endpoint-channel probe supports nested-element, positive-table, and sparse-support encodings",
        ));
    }
    let instance = input
        .instances
        .iter()
        .find(|instance| instance.id == facility_id)
        .ok_or_else(|| {
            diagnostic(
                "research-scaled-endpoint-facility-missing",
                Some(facility_id.to_string()),
                "introduced facility is absent from the cumulative exact model",
            )
        })?;
    let candidates = generate_candidate_geometries(instance, input.width, input.height);
    let unprojected_placement_values = candidates.len();
    let terminal_ports = input
        .edges
        .iter()
        .flat_map(|edge| {
            [("source", &edge.source), ("target", &edge.target)]
                .into_iter()
                .filter_map(|(kind, endpoint)| match endpoint {
                    EndpointInput::Facility { instance, ports } if instance == facility_id => {
                        Some((format!("{}:{kind}", edge.requirement_id), ports.clone()))
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    if terminal_ports.is_empty() {
        return Err(diagnostic(
            "research-scaled-endpoint-terminal-missing",
            Some(facility_id.to_string()),
            "introduced facility has no logical terminal in the cumulative exact model",
        ));
    }
    let relations = terminal_ports
        .into_iter()
        .map(|(id, ports)| build_relation(&id, &ports, &candidates))
        .collect::<Vec<_>>();
    let (placement_values, relations) =
        project_common_placement_domain(relations, candidates.len());

    let build_started = Instant::now();
    let (mut representative, scale) = build_model(encoding, &placement_values, &relations);
    let build_us = build_started
        .elapsed()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64;
    if representative.solver.propagate_to_fixpoint() == CSPSolverExecutionFlag::Infeasible {
        return Err(diagnostic(
            "research-scaled-endpoint-root-infeasible",
            Some(facility_id.to_string()),
            "unrestricted endpoint-channel model is unexpectedly infeasible",
        ));
    }

    let restrictions = [
        EndpointChannelRestriction::FixedPlacementAndPort,
        EndpointChannelRestriction::InteriorGeometryHole,
        EndpointChannelRestriction::DirectionClassOnly,
        EndpointChannelRestriction::RemoveAllPlacementSupports,
        EndpointChannelRestriction::PlacementHoleForward,
        EndpointChannelRestriction::SharedPlacementConflict,
    ];
    let mut cases = Vec::with_capacity(restrictions.len());
    for restriction in restrictions {
        let (mut model, _) = build_model(encoding, &placement_values, &relations);
        assert_eq!(
            model.solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let before = snapshot(&model, &placement_values, &relations);
        let counters_before = model.support_counters.snapshot();
        let (applicable, description) = apply_restriction(
            &mut model,
            &placement_values,
            &relations,
            input.width,
            input.height,
            restriction,
        );
        let started = Instant::now();
        let status = if applicable {
            model.solver.propagate_to_fixpoint()
        } else {
            CSPSolverExecutionFlag::Feasible
        };
        let root_propagation_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        cases.push(ScaledEndpointRestrictionReport {
            restriction,
            applicable,
            description,
            before,
            after: snapshot(&model, &placement_values, &relations),
            inconsistent: status == CSPSolverExecutionFlag::Infeasible,
            root_propagation_us,
            support_propagation: statistics_delta(
                model.support_counters.snapshot(),
                counters_before,
            ),
        });
    }

    Ok(ScaledEndpointChannelProbeReport {
        schema_version: SCALED_ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION,
        target_phase_index,
        used_width: input.width,
        used_height: input.height,
        facility: facility_id.to_string(),
        encoding,
        unprojected_placement_values,
        placement_values: placement_values.len(),
        bounded_geometry_domain_values_per_terminal: usize::try_from(input.cell_count)
            .expect("validated cell count fits usize")
            * 4,
        terminals: relations
            .iter()
            .map(|relation| ScaledEndpointTerminalScale {
                terminal: relation.id.clone(),
                port_values: relation.port_values.len(),
                reachable_geometry_values: relation.geometry_values.len(),
                legal_tuple_rows: relation.rows.len(),
            })
            .collect(),
        authored_integer_variables: scale.authored_integer_variables,
        element_constraints: scale.element_constraints,
        table_constraints: scale.table_constraints,
        table_rows: scale.table_rows,
        estimated_hidden_table_literals: scale.table_rows,
        estimated_table_clauses: scale.estimated_table_clauses,
        build_us,
        support_propagation: representative.support_counters.snapshot(),
        search_performed: false,
        branch_decisions: 0,
        backtracks: 0,
        conflicts: 0,
        learned_clauses: 0,
        solver_propagations: 0,
        cases,
    })
}

fn statistics_delta(
    after: EndpointSupportPropagationStatistics,
    before: EndpointSupportPropagationStatistics,
) -> EndpointSupportPropagationStatistics {
    EndpointSupportPropagationStatistics {
        executions: after.executions.saturating_sub(before.executions),
        notifications: after.notifications.saturating_sub(before.notifications),
        values_checked: after.values_checked.saturating_sub(before.values_checked),
        rows_scanned: after.rows_scanned.saturating_sub(before.rows_scanned),
        support_checks: after.support_checks.saturating_sub(before.support_checks),
        residue_hits: after.residue_hits.saturating_sub(before.residue_hits),
        residue_misses: after.residue_misses.saturating_sub(before.residue_misses),
        removed_values: after.removed_values.saturating_sub(before.removed_values),
        conflicts: after.conflicts.saturating_sub(before.conflicts),
        maximum_reason_predicates: after.maximum_reason_predicates,
    }
}

fn project_common_placement_domain(
    relations: Vec<TerminalRelation>,
    candidate_count: usize,
) -> (Vec<i32>, Vec<TerminalRelation>) {
    let supported = relations
        .iter()
        .map(|relation| {
            relation
                .rows
                .iter()
                .map(|row| usize::try_from(row[0]).expect("placement is non-negative"))
                .collect::<BTreeSet<_>>()
        })
        .reduce(|left, right| left.intersection(&right).copied().collect())
        .unwrap_or_default();
    let retained_old = (0..candidate_count)
        .filter(|placement| supported.contains(placement))
        .collect::<Vec<_>>();
    let old_to_new = retained_old
        .iter()
        .enumerate()
        .map(|(new, old)| {
            (
                *old,
                i32::try_from(new).expect("projected placement count fits i32"),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let projected = relations
        .into_iter()
        .map(|relation| {
            let rows = relation
                .rows
                .into_iter()
                .filter_map(|[placement, port, geometry]| {
                    old_to_new
                        .get(&usize::try_from(placement).expect("placement is non-negative"))
                        .map(|projected| [*projected, port, geometry])
                })
                .collect::<Vec<_>>();
            let geometry_values = rows
                .iter()
                .map(|row| row[2])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let values_by_port = relation
                .values_by_port
                .into_iter()
                .map(|values| retained_old.iter().map(|old| values[*old]).collect())
                .collect();
            TerminalRelation {
                id: relation.id,
                port_values: relation.port_values,
                geometry_values,
                values_by_port,
                rows,
            }
        })
        .collect::<Vec<_>>();
    let placement_values = (0..retained_old.len())
        .map(|value| i32::try_from(value).expect("projected placement count fits i32"))
        .collect();
    (placement_values, projected)
}

fn build_relation(
    id: &str,
    ports: &[FacilityPortDefinition],
    candidates: &[super::CandidateGeometry],
) -> TerminalRelation {
    let port_values = (0..ports.len())
        .map(|index| i32::try_from(index).expect("port count fits i32"))
        .collect::<Vec<_>>();
    let mut rows = Vec::with_capacity(candidates.len() * ports.len());
    let mut geometry_values = BTreeSet::new();
    let mut values_by_port = vec![Vec::with_capacity(candidates.len()); ports.len()];
    for (placement_index, candidate) in candidates.iter().enumerate() {
        let placement = i32::try_from(placement_index).expect("placement count fits i32");
        for (port_index, port) in ports.iter().enumerate() {
            let geometry = candidate.port_connections.get(&port.id).map_or(-1, |cell| {
                let outward = edge_direction(port.edge.rotated_clockwise(candidate.rotation));
                geometry_key(*cell, opposite_direction(outward))
            });
            values_by_port[port_index].push(geometry);
            if geometry >= 0 {
                let port = i32::try_from(port_index).expect("port count fits i32");
                rows.push([placement, port, geometry]);
                geometry_values.insert(geometry);
            }
        }
    }
    TerminalRelation {
        id: id.to_string(),
        port_values,
        geometry_values: geometry_values.into_iter().collect(),
        values_by_port,
        rows,
    }
}

fn build_model(
    encoding: EndpointChannelEncoding,
    placement_values: &[i32],
    relations: &[TerminalRelation],
) -> (ProbeModel, ModelScale) {
    let mut solver = Solver::default();
    let support_counters = Arc::new(EndpointSupportPropagationCounters::default());
    let placement = solver.new_named_sparse_integer(placement_values, "scaled-endpoint-placement");
    let mut terminals = Vec::with_capacity(relations.len());
    let mut scale = ModelScale {
        authored_integer_variables: 1,
        element_constraints: 0,
        table_constraints: 0,
        table_rows: 0,
        estimated_table_clauses: 0,
    };
    for relation in relations {
        let port = solver.new_named_sparse_integer(
            relation.port_values.clone(),
            format!("scaled-endpoint-{}-port", relation.id),
        );
        let geometry = solver.new_named_sparse_integer(
            relation.geometry_values.clone(),
            format!("scaled-endpoint-{}-geometry", relation.id),
        );
        scale.authored_integer_variables += 2;
        match encoding {
            EndpointChannelEncoding::NestedElement => {
                let tag = solver.new_constraint_tag();
                let mut port_geometries = Vec::with_capacity(relation.values_by_port.len());
                for (port_index, values) in relation.values_by_port.iter().enumerate() {
                    let sparse_values = values.iter().copied().collect::<BTreeSet<_>>();
                    let port_geometry = solver.new_named_sparse_integer(
                        sparse_values.into_iter().collect::<Vec<_>>(),
                        format!("scaled-endpoint-{}-port-{port_index}-geometry", relation.id),
                    );
                    solver
                        .add_constraint(pumpkin_solver::element(
                            placement,
                            values.clone(),
                            port_geometry,
                            tag,
                        ))
                        .post();
                    scale.authored_integer_variables += 1;
                    scale.element_constraints += 1;
                    port_geometries.push(port_geometry);
                }
                solver
                    .add_constraint(pumpkin_solver::element(
                        port,
                        port_geometries,
                        geometry,
                        tag,
                    ))
                    .post();
                scale.element_constraints += 1;
            }
            EndpointChannelEncoding::PositiveTable => {
                let rows = relation
                    .rows
                    .iter()
                    .map(|row| row.to_vec())
                    .collect::<Vec<_>>();
                let distinct_values = [0, 1, 2]
                    .into_iter()
                    .map(|column| {
                        rows.iter()
                            .map(|row| row[column])
                            .collect::<BTreeSet<_>>()
                            .len()
                    })
                    .sum::<usize>();
                scale.table_constraints += 1;
                scale.table_rows += rows.len();
                scale.estimated_table_clauses += rows.len() * 3 + distinct_values + 1;
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_solver::table(
                        vec![placement, port, geometry],
                        rows,
                        tag,
                    ))
                    .post();
            }
            EndpointChannelEncoding::SparseSupport => {
                let tag = solver.new_constraint_tag();
                let _ = solver.add_propagator(SparseEndpointSupportPropagatorArgs {
                    name: format!("scaled-endpoint-{}-sparse-support", relation.id),
                    variables: [placement, port, geometry],
                    domain_values: [
                        placement_values.to_vec(),
                        relation.port_values.clone(),
                        relation.geometry_values.clone(),
                    ],
                    rows: relation.rows.clone(),
                    counters: Arc::clone(&support_counters),
                    constraint_tag: tag,
                });
            }
            _ => unreachable!("encoding was validated before model construction"),
        }
        terminals.push(TerminalVariables { port, geometry });
    }
    (
        ProbeModel {
            solver,
            placement,
            terminals,
            support_counters,
        },
        scale,
    )
}

fn apply_restriction(
    model: &mut ProbeModel,
    placement_values: &[i32],
    relations: &[TerminalRelation],
    width: i32,
    height: i32,
    restriction: EndpointChannelRestriction,
) -> (bool, String) {
    let tag = model.solver.new_constraint_tag();
    let widest_terminal = relations
        .iter()
        .enumerate()
        .max_by_key(|(_, relation)| relation.port_values.len())
        .map(|(index, _)| index)
        .unwrap_or(0);
    let placement = placement_values[placement_values.len() / 2];
    match restriction {
        EndpointChannelRestriction::FixedPlacementAndPort => {
            let port = relations[widest_terminal]
                .rows
                .iter()
                .find_map(|row| (row[0] == placement).then_some(row[1]))
                .expect("a projected placement has terminal support");
            model
                .solver
                .add_clause([model.placement.equality_predicate(placement)], tag);
            model.solver.add_clause(
                [model.terminals[widest_terminal]
                    .port
                    .equality_predicate(port)],
                tag,
            );
            (
                true,
                format!("placement={placement}, terminal={widest_terminal}, port={port}"),
            )
        }
        EndpointChannelRestriction::InteriorGeometryHole => {
            let relation = &relations[widest_terminal];
            let values = relation
                .rows
                .iter()
                .filter_map(|row| (row[1] == 0).then_some(row[2]))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let Some(geometry) = values.get(values.len() / 2).copied() else {
                return (false, "no geometry value is available".to_string());
            };
            model.solver.add_clause(
                [model.terminals[widest_terminal].port.equality_predicate(0)],
                tag,
            );
            model.solver.add_clause(
                [model.terminals[widest_terminal]
                    .geometry
                    .disequality_predicate(geometry)],
                tag,
            );
            (
                true,
                format!("terminal={widest_terminal}, port=0, geometry!={geometry}"),
            )
        }
        EndpointChannelRestriction::DirectionClassOnly => {
            let relation = &relations[widest_terminal];
            let chosen = (0..4)
                .map(|direction| {
                    let supported = relation
                        .rows
                        .iter()
                        .filter(|row| row[2].rem_euclid(4) == direction)
                        .map(|row| row[0])
                        .collect::<BTreeSet<_>>();
                    (direction, supported.len())
                })
                .filter(|(_, count)| *count > 0 && *count < placement_values.len())
                .min_by_key(|(_, count)| *count);
            let Some((direction, _)) = chosen else {
                return (
                    false,
                    "no direction class partially restricts placement".to_string(),
                );
            };
            let retained = relation
                .geometry_values
                .iter()
                .copied()
                .filter(|geometry| geometry.rem_euclid(4) == direction)
                .collect::<BTreeSet<_>>();
            retain_geometry(model, relations, widest_terminal, &retained, tag);
            (
                true,
                format!("terminal={widest_terminal}, direction={direction}"),
            )
        }
        EndpointChannelRestriction::RemoveAllPlacementSupports => {
            let relation = &relations[widest_terminal];
            let removed = relation
                .rows
                .iter()
                .filter_map(|row| (row[0] == placement).then_some(row[2]))
                .collect::<BTreeSet<_>>();
            for geometry in &removed {
                model.solver.add_clause(
                    [model.terminals[widest_terminal]
                        .geometry
                        .disequality_predicate(*geometry)],
                    tag,
                );
            }
            (
                !removed.is_empty(),
                format!(
                    "terminal={widest_terminal}, removed {} geometry values supporting placement={placement}",
                    removed.len()
                ),
            )
        }
        EndpointChannelRestriction::PlacementHoleForward => {
            model
                .solver
                .add_clause([model.placement.disequality_predicate(placement)], tag);
            model.solver.add_clause(
                [model.terminals[widest_terminal].port.equality_predicate(0)],
                tag,
            );
            (
                true,
                format!("placement!={placement}, terminal={widest_terminal}, port=0"),
            )
        }
        EndpointChannelRestriction::SharedPlacementConflict => {
            if relations.len() < 2 {
                return (false, "fewer than two shared terminals".to_string());
            }
            let Some(restriction) = find_disjoint_terminal_supports(relations, width, height)
            else {
                return (
                    false,
                    "no derived disjoint two-terminal support restriction".to_string(),
                );
            };
            model.solver.add_clause(
                [model.terminals[restriction.left_terminal]
                    .port
                    .equality_predicate(restriction.left_port)],
                tag,
            );
            model.solver.add_clause(
                [model.terminals[restriction.right_terminal]
                    .port
                    .equality_predicate(restriction.right_port)],
                tag,
            );
            retain_geometry(
                model,
                relations,
                restriction.left_terminal,
                &restriction.left_geometry,
                tag,
            );
            retain_geometry(
                model,
                relations,
                restriction.right_terminal,
                &restriction.right_geometry,
                tag,
            );
            (
                true,
                format!(
                    "terminal={}/port={} on low {} side and terminal={}/port={} on high {} side at split={}",
                    restriction.left_terminal,
                    restriction.left_port,
                    restriction.axis,
                    restriction.right_terminal,
                    restriction.right_port,
                    restriction.axis,
                    restriction.split,
                ),
            )
        }
    }
}

struct DisjointSupportRestriction {
    left_terminal: usize,
    left_port: i32,
    left_geometry: BTreeSet<i32>,
    right_terminal: usize,
    right_port: i32,
    right_geometry: BTreeSet<i32>,
    axis: &'static str,
    split: i32,
}

fn find_disjoint_terminal_supports(
    relations: &[TerminalRelation],
    width: i32,
    height: i32,
) -> Option<DisjointSupportRestriction> {
    for (axis, extent) in [("x", width), ("y", height)] {
        for split in 1..extent {
            for left_terminal in 0..relations.len() {
                for right_terminal in (left_terminal + 1)..relations.len() {
                    for left_port in &relations[left_terminal].port_values {
                        let left_geometry = relations[left_terminal]
                            .geometry_values
                            .iter()
                            .copied()
                            .filter(|geometry| geometry_coordinate(*geometry, width, axis) < split)
                            .collect::<BTreeSet<_>>();
                        let left_support = supported_placements(
                            &relations[left_terminal],
                            *left_port,
                            &left_geometry,
                        );
                        if left_support.is_empty() {
                            continue;
                        }
                        for right_port in &relations[right_terminal].port_values {
                            let right_geometry = relations[right_terminal]
                                .geometry_values
                                .iter()
                                .copied()
                                .filter(|geometry| {
                                    geometry_coordinate(*geometry, width, axis) >= split
                                })
                                .collect::<BTreeSet<_>>();
                            let right_support = supported_placements(
                                &relations[right_terminal],
                                *right_port,
                                &right_geometry,
                            );
                            if !right_support.is_empty() && left_support.is_disjoint(&right_support)
                            {
                                return Some(DisjointSupportRestriction {
                                    left_terminal,
                                    left_port: *left_port,
                                    left_geometry,
                                    right_terminal,
                                    right_port: *right_port,
                                    right_geometry,
                                    axis,
                                    split,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn supported_placements(
    relation: &TerminalRelation,
    port: i32,
    geometry: &BTreeSet<i32>,
) -> BTreeSet<i32> {
    relation
        .rows
        .iter()
        .filter_map(|row| (row[1] == port && geometry.contains(&row[2])).then_some(row[0]))
        .collect()
}

fn geometry_coordinate(geometry: i32, width: i32, axis: &str) -> i32 {
    let cell = geometry.div_euclid(4);
    match axis {
        "x" => cell % width,
        "y" => cell / width,
        _ => unreachable!("diagnostic axis is x or y"),
    }
}

fn retain_geometry(
    model: &mut ProbeModel,
    relations: &[TerminalRelation],
    terminal_index: usize,
    retained: &BTreeSet<i32>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for geometry in &relations[terminal_index].geometry_values {
        if !retained.contains(geometry) {
            model.solver.add_clause(
                [model.terminals[terminal_index]
                    .geometry
                    .disequality_predicate(*geometry)],
                tag,
            );
        }
    }
}

fn snapshot(
    model: &ProbeModel,
    placement_values: &[i32],
    relations: &[TerminalRelation],
) -> ScaledEndpointDomainSnapshot {
    ScaledEndpointDomainSnapshot {
        placement_values: retained_count(&model.solver, model.placement, placement_values),
        terminals: model
            .terminals
            .iter()
            .zip(relations)
            .map(|(variables, relation)| {
                let geometry_values = relation
                    .geometry_values
                    .iter()
                    .copied()
                    .filter(|value| model.solver.contains(&variables.geometry, *value))
                    .collect::<Vec<_>>();
                ScaledEndpointTerminalDomainSnapshot {
                    terminal: relation.id.clone(),
                    port_values: retained_count(
                        &model.solver,
                        variables.port,
                        &relation.port_values,
                    ),
                    geometry_values: geometry_values.len(),
                    direction_values: geometry_values
                        .iter()
                        .map(|value| value.rem_euclid(4))
                        .collect::<BTreeSet<_>>()
                        .len(),
                }
            })
            .collect(),
    }
}

fn retained_count(solver: &Solver, variable: DomainId, values: &[i32]) -> usize {
    values
        .iter()
        .filter(|value| solver.contains(&variable, **value))
        .count()
}

fn geometry_key(cell: usize, direction: CardinalDirection) -> i32 {
    i32::try_from(cell * 4 + direction_index(direction)).expect("geometry key fits i32")
}

fn direction_index(direction: CardinalDirection) -> usize {
    match direction {
        CardinalDirection::North => 0,
        CardinalDirection::East => 1,
        CardinalDirection::South => 2,
        CardinalDirection::West => 3,
    }
}

fn opposite_direction(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::South,
        CardinalDirection::East => CardinalDirection::West,
        CardinalDirection::South => CardinalDirection::North,
        CardinalDirection::West => CardinalDirection::East,
    }
}

fn edge_direction(edge: FacilityPortEdge) -> CardinalDirection {
    match edge {
        FacilityPortEdge::North => CardinalDirection::North,
        FacilityPortEdge::East => CardinalDirection::East,
        FacilityPortEdge::South => CardinalDirection::South,
        FacilityPortEdge::West => CardinalDirection::West,
    }
}

fn diagnostic(
    code: &'static str,
    entity: Option<String>,
    message: impl Into<String>,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(code, "/scaled_endpoint_channel", entity, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_projection_removes_unsupported_placements_and_remaps_rows() {
        let relations = vec![
            TerminalRelation {
                id: "a".to_string(),
                port_values: vec![0],
                geometry_values: vec![10, 11, 12],
                values_by_port: vec![vec![10, 11, 12]],
                rows: vec![[0, 0, 10], [1, 0, 11], [2, 0, 12]],
            },
            TerminalRelation {
                id: "b".to_string(),
                port_values: vec![0],
                geometry_values: vec![20, 22],
                values_by_port: vec![vec![20, -1, 22]],
                rows: vec![[0, 0, 20], [2, 0, 22]],
            },
        ];

        let (placements, projected) = project_common_placement_domain(relations, 3);

        assert_eq!(placements, vec![0, 1]);
        assert_eq!(projected[0].rows, vec![[0, 0, 10], [1, 0, 12]]);
        assert_eq!(projected[0].values_by_port, vec![vec![10, 12]]);
        assert_eq!(projected[1].rows, vec![[0, 0, 20], [1, 0, 22]]);
        assert_eq!(projected[1].values_by_port, vec![vec![20, 22]]);
    }
}
