use std::sync::Arc;
use std::time::Instant;

use pumpkin_solver::Solver;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::CSPSolverExecutionFlag;
use pumpkin_solver::core::variables::DomainId;

use super::super::research::{
    ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION, EndpointChannelCaseReport,
    EndpointChannelDomainSnapshot, EndpointChannelEncoding, EndpointChannelEndpointSnapshot,
    EndpointChannelProbeReport, EndpointChannelRestriction,
};
use super::endpoint_support_propagator::{
    EndpointSupportPropagationCounters, SparseEndpointSupportPropagatorArgs,
};

const PLACEMENTS: [i32; 4] = [0, 1, 2, 3];
const PORTS: [i32; 3] = [0, 1, 2];
const GEOMETRIES: [i32; 12] = [0, 1, 3, 4, 5, 6, 9, 10, 11, 12, 14, 15];
const RELATION: [[i32; 3]; 12] = [
    [0, 0, 0],
    [0, 1, 4],
    [0, 2, 1],
    [1, 0, 5],
    [1, 1, 9],
    [1, 2, 6],
    [2, 0, 10],
    [2, 1, 14],
    [2, 2, 11],
    [3, 0, 15],
    [3, 1, 3],
    [3, 2, 12],
];

struct EndpointVariables {
    port: DomainId,
    geometry: DomainId,
    direction: Option<DomainId>,
}

struct ProbeModel {
    solver: Solver,
    placement: DomainId,
    endpoints: Vec<EndpointVariables>,
    authored_integer_variables: usize,
    element_constraints: usize,
    direct_clauses: usize,
    table_rows: usize,
    estimated_table_clauses: usize,
    support_counters: Arc<EndpointSupportPropagationCounters>,
}

pub(in crate::layouts::integrated) fn probe_endpoint_channels() -> EndpointChannelProbeReport {
    let encodings = [
        EndpointChannelEncoding::NestedElement,
        EndpointChannelEncoding::DirectTupleClauses,
        EndpointChannelEncoding::DirectionChannel,
        EndpointChannelEncoding::PositiveTable,
        EndpointChannelEncoding::SparseSupport,
    ];
    let restrictions = [
        EndpointChannelRestriction::FixedPlacementAndPort,
        EndpointChannelRestriction::InteriorGeometryHole,
        EndpointChannelRestriction::DirectionClassOnly,
        EndpointChannelRestriction::RemoveAllPlacementSupports,
        EndpointChannelRestriction::PlacementHoleForward,
        EndpointChannelRestriction::SharedPlacementConflict,
    ];
    let mut cases = Vec::with_capacity(encodings.len() * restrictions.len());
    for restriction in restrictions {
        let endpoint_count = if matches!(
            restriction,
            EndpointChannelRestriction::SharedPlacementConflict
        ) {
            PLACEMENTS.len()
        } else {
            1
        };
        for encoding in encodings {
            let mut model = build_model(encoding, endpoint_count);
            assert_eq!(
                model.solver.propagate_to_fixpoint(),
                CSPSolverExecutionFlag::Feasible
            );
            let before = snapshot(&model);
            apply_restriction(&mut model, restriction);
            let started = Instant::now();
            let status = model.solver.propagate_to_fixpoint();
            let root_propagation_us =
                started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            cases.push(EndpointChannelCaseReport {
                encoding,
                restriction,
                before,
                after: snapshot(&model),
                inconsistent: status == CSPSolverExecutionFlag::Infeasible,
                root_propagation_us,
                authored_integer_variables: model.authored_integer_variables,
                element_constraints: model.element_constraints,
                direct_clauses: model.direct_clauses,
                table_rows: model.table_rows,
                estimated_hidden_table_literals: model.table_rows,
                estimated_table_clauses: model.estimated_table_clauses,
                support_propagation: model.support_counters.snapshot(),
                matches_positive_table_oracle: false,
            });
        }
    }

    for restriction in restrictions {
        let oracle = cases
            .iter()
            .find(|case| {
                case.restriction == restriction
                    && case.encoding == EndpointChannelEncoding::PositiveTable
            })
            .expect("every restriction has a positive-table oracle")
            .clone();
        for case in cases
            .iter_mut()
            .filter(|case| case.restriction == restriction)
        {
            case.matches_positive_table_oracle = case.inconsistent == oracle.inconsistent
                && (case.inconsistent || case.after == oracle.after);
        }
    }

    EndpointChannelProbeReport {
        schema_version: ENDPOINT_CHANNEL_PROBE_SCHEMA_VERSION,
        placement_values: PLACEMENTS.len(),
        port_values: PORTS.len(),
        reachable_geometry_values: GEOMETRIES.to_vec(),
        legal_tuples: RELATION.to_vec(),
        cases,
    }
}

fn build_model(encoding: EndpointChannelEncoding, endpoint_count: usize) -> ProbeModel {
    let mut solver = Solver::default();
    let placement = solver.new_named_sparse_integer(PLACEMENTS, "endpoint-probe-placement");
    let mut endpoints = Vec::with_capacity(endpoint_count);
    let mut authored_integer_variables = 1;
    let mut element_constraints = 0;
    let mut direct_clauses = 0;
    let mut table_rows = 0;
    let mut estimated_table_clauses = 0;
    let support_counters = Arc::new(EndpointSupportPropagationCounters::default());
    for endpoint_index in 0..endpoint_count {
        let port =
            solver.new_named_sparse_integer(PORTS, format!("endpoint-probe-{endpoint_index}-port"));
        let geometry = solver.new_named_sparse_integer(
            GEOMETRIES,
            format!("endpoint-probe-{endpoint_index}-geometry"),
        );
        authored_integer_variables += 2;
        let direction = match encoding {
            EndpointChannelEncoding::NestedElement => {
                post_nested_elements(
                    &mut solver,
                    placement,
                    port,
                    geometry,
                    endpoint_index,
                    &mut authored_integer_variables,
                    &mut element_constraints,
                );
                None
            }
            EndpointChannelEncoding::DirectTupleClauses => {
                post_nested_elements(
                    &mut solver,
                    placement,
                    port,
                    geometry,
                    endpoint_index,
                    &mut authored_integer_variables,
                    &mut element_constraints,
                );
                let tag = solver.new_constraint_tag();
                for [placement_value, port_value, geometry_value] in RELATION {
                    solver.add_clause(
                        [
                            placement.disequality_predicate(placement_value),
                            port.disequality_predicate(port_value),
                            geometry.equality_predicate(geometry_value),
                        ],
                        tag,
                    );
                    direct_clauses += 1;
                }
                None
            }
            EndpointChannelEncoding::DirectionChannel => {
                post_nested_elements(
                    &mut solver,
                    placement,
                    port,
                    geometry,
                    endpoint_index,
                    &mut authored_integer_variables,
                    &mut element_constraints,
                );
                let direction = solver.new_named_bounded_integer(
                    0,
                    3,
                    format!("endpoint-probe-{endpoint_index}-direction"),
                );
                authored_integer_variables += 1;
                let geometry_direction_rows = GEOMETRIES
                    .iter()
                    .map(|geometry| vec![*geometry, geometry.rem_euclid(4)])
                    .collect::<Vec<_>>();
                let endpoint_direction_rows = RELATION
                    .iter()
                    .map(|[placement, port, geometry]| {
                        vec![*placement, *port, geometry.rem_euclid(4)]
                    })
                    .collect::<Vec<_>>();
                post_counted_table(
                    &mut solver,
                    vec![geometry, direction],
                    geometry_direction_rows,
                    &mut table_rows,
                    &mut estimated_table_clauses,
                );
                post_counted_table(
                    &mut solver,
                    vec![placement, port, direction],
                    endpoint_direction_rows,
                    &mut table_rows,
                    &mut estimated_table_clauses,
                );
                Some(direction)
            }
            EndpointChannelEncoding::PositiveTable => {
                post_counted_table(
                    &mut solver,
                    vec![placement, port, geometry],
                    RELATION.iter().map(|row| row.to_vec()).collect(),
                    &mut table_rows,
                    &mut estimated_table_clauses,
                );
                None
            }
            EndpointChannelEncoding::SparseSupport => {
                let tag = solver.new_constraint_tag();
                let _ = solver.add_propagator(SparseEndpointSupportPropagatorArgs {
                    name: format!("endpoint-probe-{endpoint_index}-sparse-support"),
                    variables: [placement, port, geometry],
                    domain_values: [PLACEMENTS.to_vec(), PORTS.to_vec(), GEOMETRIES.to_vec()],
                    rows: RELATION.to_vec(),
                    counters: Arc::clone(&support_counters),
                    constraint_tag: tag,
                });
                None
            }
        };
        endpoints.push(EndpointVariables {
            port,
            geometry,
            direction,
        });
    }
    ProbeModel {
        solver,
        placement,
        endpoints,
        authored_integer_variables,
        element_constraints,
        direct_clauses,
        table_rows,
        estimated_table_clauses,
        support_counters,
    }
}

fn post_nested_elements(
    solver: &mut Solver,
    placement: DomainId,
    port: DomainId,
    geometry: DomainId,
    endpoint_index: usize,
    authored_integer_variables: &mut usize,
    element_constraints: &mut usize,
) {
    let tag = solver.new_constraint_tag();
    let mut port_geometries = Vec::with_capacity(PORTS.len());
    for port_value in PORTS {
        let values = RELATION
            .iter()
            .filter_map(|[_, relation_port, geometry]| {
                (*relation_port == port_value).then_some(*geometry)
            })
            .collect::<Vec<_>>();
        let port_geometry = solver.new_named_sparse_integer(
            values.clone(),
            format!("endpoint-probe-{endpoint_index}-port-{port_value}-geometry"),
        );
        solver
            .add_constraint(pumpkin_solver::element(
                placement,
                values,
                port_geometry,
                tag,
            ))
            .post();
        *authored_integer_variables += 1;
        *element_constraints += 1;
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
    *element_constraints += 1;
}

fn post_counted_table(
    solver: &mut Solver,
    variables: Vec<DomainId>,
    rows: Vec<Vec<i32>>,
    table_rows: &mut usize,
    estimated_table_clauses: &mut usize,
) {
    let row_count = rows.len();
    let column_count = variables.len();
    let distinct_values = (0..column_count)
        .map(|column| {
            rows.iter()
                .map(|row| row[column])
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        })
        .sum::<usize>();
    *table_rows += row_count;
    *estimated_table_clauses += row_count * column_count + distinct_values + 1;
    let tag = solver.new_constraint_tag();
    solver
        .add_constraint(pumpkin_solver::table(variables, rows, tag))
        .post();
}

fn apply_restriction(model: &mut ProbeModel, restriction: EndpointChannelRestriction) {
    let tag = model.solver.new_constraint_tag();
    match restriction {
        EndpointChannelRestriction::FixedPlacementAndPort => {
            model
                .solver
                .add_clause([model.placement.equality_predicate(2)], tag);
            model
                .solver
                .add_clause([model.endpoints[0].port.equality_predicate(2)], tag);
        }
        EndpointChannelRestriction::InteriorGeometryHole => {
            model
                .solver
                .add_clause([model.endpoints[0].port.equality_predicate(0)], tag);
            model
                .solver
                .add_clause([model.endpoints[0].geometry.disequality_predicate(10)], tag);
        }
        EndpointChannelRestriction::DirectionClassOnly => {
            retain_geometry_values(
                &mut model.solver,
                model.endpoints[0].geometry,
                &[1, 5, 9],
                tag,
            );
        }
        EndpointChannelRestriction::RemoveAllPlacementSupports => {
            for geometry in [0, 4, 1] {
                model.solver.add_clause(
                    [model.endpoints[0].geometry.disequality_predicate(geometry)],
                    tag,
                );
            }
        }
        EndpointChannelRestriction::PlacementHoleForward => {
            model
                .solver
                .add_clause([model.endpoints[0].port.equality_predicate(0)], tag);
            model
                .solver
                .add_clause([model.placement.disequality_predicate(1)], tag);
        }
        EndpointChannelRestriction::SharedPlacementConflict => {
            for (placement_value, endpoint) in PLACEMENTS.iter().zip(&model.endpoints) {
                let removed = RELATION
                    .iter()
                    .filter_map(|[placement, _, geometry]| {
                        (placement == placement_value).then_some(*geometry)
                    })
                    .collect::<Vec<_>>();
                let retained = GEOMETRIES
                    .iter()
                    .copied()
                    .filter(|geometry| !removed.contains(geometry))
                    .collect::<Vec<_>>();
                retain_geometry_values(&mut model.solver, endpoint.geometry, &retained, tag);
            }
        }
    }
}

fn retain_geometry_values(
    solver: &mut Solver,
    geometry: DomainId,
    retained: &[i32],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for value in GEOMETRIES {
        if !retained.contains(&value) {
            solver.add_clause([geometry.disequality_predicate(value)], tag);
        }
    }
}

fn snapshot(model: &ProbeModel) -> EndpointChannelDomainSnapshot {
    EndpointChannelDomainSnapshot {
        placement_values: retained_values(&model.solver, model.placement, &PLACEMENTS),
        endpoints: model
            .endpoints
            .iter()
            .map(|endpoint| {
                let geometry_values =
                    retained_values(&model.solver, endpoint.geometry, &GEOMETRIES);
                let direction_values = endpoint.direction.map_or_else(
                    || {
                        geometry_values
                            .iter()
                            .map(|geometry| geometry.rem_euclid(4))
                            .collect::<std::collections::BTreeSet<_>>()
                            .into_iter()
                            .collect()
                    },
                    |direction| retained_values(&model.solver, direction, &[0, 1, 2, 3]),
                );
                EndpointChannelEndpointSnapshot {
                    port_values: retained_values(&model.solver, endpoint.port, &PORTS),
                    geometry_values,
                    direction_values,
                }
            })
            .collect(),
    }
}

fn retained_values(solver: &Solver, variable: DomainId, values: &[i32]) -> Vec<i32> {
    values
        .iter()
        .copied()
        .filter(|value| solver.contains(&variable, *value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_separates_bounds_direction_and_full_support_propagation() {
        let report = probe_endpoint_channels();
        let case = |restriction, encoding| {
            report
                .cases
                .iter()
                .find(|case| case.restriction == restriction && case.encoding == encoding)
                .expect("probe case exists")
        };

        assert!(
            case(
                EndpointChannelRestriction::FixedPlacementAndPort,
                EndpointChannelEncoding::NestedElement
            )
            .matches_positive_table_oracle
        );
        assert!(
            case(
                EndpointChannelRestriction::InteriorGeometryHole,
                EndpointChannelEncoding::NestedElement
            )
            .after
            .placement_values
            .contains(&2)
        );
        assert!(
            !case(
                EndpointChannelRestriction::InteriorGeometryHole,
                EndpointChannelEncoding::PositiveTable
            )
            .after
            .placement_values
            .contains(&2)
        );
        assert_eq!(
            case(
                EndpointChannelRestriction::DirectionClassOnly,
                EndpointChannelEncoding::DirectionChannel
            )
            .after
            .placement_values,
            vec![0, 1]
        );
        assert!(
            case(
                EndpointChannelRestriction::SharedPlacementConflict,
                EndpointChannelEncoding::PositiveTable
            )
            .inconsistent
        );
        for restriction in [
            EndpointChannelRestriction::FixedPlacementAndPort,
            EndpointChannelRestriction::InteriorGeometryHole,
            EndpointChannelRestriction::DirectionClassOnly,
            EndpointChannelRestriction::RemoveAllPlacementSupports,
            EndpointChannelRestriction::PlacementHoleForward,
            EndpointChannelRestriction::SharedPlacementConflict,
        ] {
            assert!(
                case(restriction, EndpointChannelEncoding::SparseSupport)
                    .matches_positive_table_oracle,
                "sparse support differs from the positive-table oracle for {restriction:?}"
            );
        }
        assert!(
            !case(
                EndpointChannelRestriction::SharedPlacementConflict,
                EndpointChannelEncoding::NestedElement
            )
            .inconsistent
        );
    }

    #[test]
    fn every_encoding_preserves_the_exact_complete_tuple_relation() {
        for encoding in [
            EndpointChannelEncoding::NestedElement,
            EndpointChannelEncoding::DirectTupleClauses,
            EndpointChannelEncoding::DirectionChannel,
            EndpointChannelEncoding::PositiveTable,
            EndpointChannelEncoding::SparseSupport,
        ] {
            for placement_value in PLACEMENTS {
                for port_value in PORTS {
                    for geometry_value in GEOMETRIES {
                        let mut model = build_model(encoding, 1);
                        let tag = model.solver.new_constraint_tag();
                        model
                            .solver
                            .add_clause([model.placement.equality_predicate(placement_value)], tag);
                        model.solver.add_clause(
                            [model.endpoints[0].port.equality_predicate(port_value)],
                            tag,
                        );
                        model.solver.add_clause(
                            [model.endpoints[0]
                                .geometry
                                .equality_predicate(geometry_value)],
                            tag,
                        );
                        let observed = model.solver.propagate_to_fixpoint()
                            == CSPSolverExecutionFlag::Feasible;
                        let expected =
                            RELATION.contains(&[placement_value, port_value, geometry_value]);
                        assert_eq!(
                            observed, expected,
                            "encoding={encoding:?} tuple=({placement_value},{port_value},{geometry_value})"
                        );
                    }
                }
            }
        }
    }
}
