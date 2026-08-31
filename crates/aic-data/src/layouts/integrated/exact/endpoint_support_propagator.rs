use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use pumpkin_solver::core::declare_inference_label;
use pumpkin_solver::core::predicates::{Predicate, PredicateConstructor, PropositionalConjunction};
use pumpkin_solver::core::proof::{ConstraintTag, InferenceCode};
use pumpkin_solver::core::propagation::{
    DomainEvents, EnqueueDecision, EventsToRegister, LocalId, NotificationContext,
    OpaqueDomainEvent, Priority, PropagationContext, Propagator, PropagatorConstructor,
    PropagatorConstructorContext, PropagatorSpec, ReadDomains, RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

use super::super::research::EndpointSupportPropagationStatistics;

declare_inference_label!(SparseEndpointSupport);

#[derive(Debug, Default)]
pub(in crate::layouts::integrated) struct EndpointSupportPropagationCounters {
    executions: AtomicU64,
    notifications: AtomicU64,
    values_checked: AtomicU64,
    rows_scanned: AtomicU64,
    support_checks: AtomicU64,
    residue_hits: AtomicU64,
    residue_misses: AtomicU64,
    removed_values: AtomicU64,
    conflicts: AtomicU64,
    maximum_reason_predicates: AtomicU64,
}

impl EndpointSupportPropagationCounters {
    pub(in crate::layouts::integrated) fn snapshot(&self) -> EndpointSupportPropagationStatistics {
        EndpointSupportPropagationStatistics {
            executions: self.executions.load(Ordering::Relaxed),
            notifications: self.notifications.load(Ordering::Relaxed),
            values_checked: self.values_checked.load(Ordering::Relaxed),
            rows_scanned: self.rows_scanned.load(Ordering::Relaxed),
            support_checks: self.support_checks.load(Ordering::Relaxed),
            residue_hits: self.residue_hits.load(Ordering::Relaxed),
            residue_misses: self.residue_misses.load(Ordering::Relaxed),
            removed_values: self.removed_values.load(Ordering::Relaxed),
            conflicts: self.conflicts.load(Ordering::Relaxed),
            maximum_reason_predicates: self.maximum_reason_predicates.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated) struct SparseEndpointSupportPropagatorArgs {
    pub name: String,
    pub variables: [DomainId; 3],
    pub domain_values: [Vec<i32>; 3],
    pub rows: Vec<[i32; 3]>,
    pub counters: Arc<EndpointSupportPropagationCounters>,
    pub constraint_tag: ConstraintTag,
}

impl PropagatorConstructor for SparseEndpointSupportPropagatorArgs {
    type PropagatorImpl = SparseEndpointSupportPropagator;

    fn create(
        mut self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        self.rows.sort_unstable();
        self.rows.dedup();

        let values = self.domain_values.map(|column_values| {
            column_values
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        });
        for row in &self.rows {
            for column in 0..3 {
                assert!(
                    values[column].binary_search(&row[column]).is_ok(),
                    "every legal row value belongs to its declared variable domain"
                );
            }
        }
        let mut rows_by_value: [Vec<Vec<usize>>; 3] =
            std::array::from_fn(|column| vec![Vec::new(); values[column].len()]);
        for (row_index, row) in self.rows.iter().enumerate() {
            for column in 0..3 {
                let value_index = values[column]
                    .binary_search(&row[column])
                    .expect("every row value was indexed");
                rows_by_value[column][value_index].push(row_index);
            }
        }
        let residues = std::array::from_fn(|column| vec![None; values[column].len()]);
        let registration = EventsToRegister::builder()
            .add(&self.variables[0], DomainEvents::ANY_INT, LocalId::from(0))
            .add(&self.variables[1], DomainEvents::ANY_INT, LocalId::from(1))
            .add(&self.variables[2], DomainEvents::ANY_INT, LocalId::from(2))
            .build();

        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: SparseEndpointSupportPropagator {
                name: self.name,
                variables: self.variables,
                rows: self.rows,
                values,
                rows_by_value,
                residues,
                counters: self.counters,
                inference_code: InferenceCode::new(self.constraint_tag, SparseEndpointSupport),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated) struct SparseEndpointSupportPropagator {
    name: String,
    variables: [DomainId; 3],
    rows: Vec<[i32; 3]>,
    values: [Vec<i32>; 3],
    rows_by_value: [Vec<Vec<usize>>; 3],
    residues: [Vec<Option<usize>>; 3],
    counters: Arc<EndpointSupportPropagationCounters>,
    inference_code: InferenceCode,
}

#[derive(Default)]
struct ColumnCounters {
    values_checked: u64,
    rows_scanned: u64,
    support_checks: u64,
    residue_hits: u64,
    residue_misses: u64,
    removed_values: u64,
    maximum_reason_predicates: u64,
}

impl ColumnCounters {
    fn flush(&self, counters: &EndpointSupportPropagationCounters) {
        counters
            .values_checked
            .fetch_add(self.values_checked, Ordering::Relaxed);
        counters
            .rows_scanned
            .fetch_add(self.rows_scanned, Ordering::Relaxed);
        counters
            .support_checks
            .fetch_add(self.support_checks, Ordering::Relaxed);
        counters
            .residue_hits
            .fetch_add(self.residue_hits, Ordering::Relaxed);
        counters
            .residue_misses
            .fetch_add(self.residue_misses, Ordering::Relaxed);
        counters
            .removed_values
            .fetch_add(self.removed_values, Ordering::Relaxed);
        counters
            .maximum_reason_predicates
            .fetch_max(self.maximum_reason_predicates, Ordering::Relaxed);
    }
}

impl SparseEndpointSupportPropagator {
    fn row_is_supported(
        &self,
        context: &impl ReadDomains,
        target_column: usize,
        row_index: usize,
    ) -> (bool, u64) {
        let row = self.rows[row_index];
        let mut checks = 0;
        for column in (0..3).filter(|column| *column != target_column) {
            checks += 1;
            if !context.contains(&self.variables[column], row[column]) {
                return (false, checks);
            }
        }
        (true, checks)
    }

    fn unsupported_reason(
        &self,
        context: &impl ReadDomains,
        target_column: usize,
        row_indices: &[usize],
    ) -> PropositionalConjunction {
        let mut predicates = Vec::<Predicate>::new();
        let mut seen = HashSet::<Predicate>::new();
        for row_index in row_indices {
            let row = self.rows[*row_index];
            let blocker = (0..3)
                .filter(|column| *column != target_column)
                .find_map(|column| {
                    (!context.contains(&self.variables[column], row[column]))
                        .then(|| self.variables[column].disequality_predicate(row[column]))
                })
                .expect("an unsupported row has a blocker outside the target column");
            if seen.insert(blocker) {
                predicates.push(blocker);
            }
        }
        PropositionalConjunction::new(predicates)
    }

    fn propagate_column(
        &mut self,
        context: &mut PropagationContext,
        column: usize,
        use_residues: bool,
    ) -> PropagationStatusCP {
        let mut measured = ColumnCounters::default();
        for value_index in 0..self.values[column].len() {
            let value = self.values[column][value_index];
            if !context.contains(&self.variables[column], value) {
                continue;
            }
            measured.values_checked += 1;

            if use_residues {
                if let Some(row_index) = self.residues[column][value_index] {
                    let (supported, checks) = self.row_is_supported(context, column, row_index);
                    measured.support_checks += checks;
                    if supported {
                        measured.residue_hits += 1;
                        continue;
                    }
                    measured.residue_misses += 1;
                }
            }

            let row_indices = &self.rows_by_value[column][value_index];
            let mut support = None;
            for row_index in row_indices {
                measured.rows_scanned += 1;
                let (supported, checks) = self.row_is_supported(context, column, *row_index);
                measured.support_checks += checks;
                if supported {
                    support = Some(*row_index);
                    break;
                }
            }
            if let Some(row_index) = support {
                if use_residues {
                    self.residues[column][value_index] = Some(row_index);
                }
                continue;
            }

            let reason = self.unsupported_reason(context, column, row_indices);
            measured.removed_values += 1;
            measured.maximum_reason_predicates = measured
                .maximum_reason_predicates
                .max(reason.len().try_into().unwrap_or(u64::MAX));
            if let Err(conflict) = context.post(
                self.variables[column].disequality_predicate(value),
                (reason, &self.inference_code),
            ) {
                measured.flush(&self.counters);
                self.counters.conflicts.fetch_add(1, Ordering::Relaxed);
                return Err(conflict.into());
            }
        }
        measured.flush(&self.counters);
        Ok(())
    }

    fn propagate_all(
        &mut self,
        context: &mut PropagationContext,
        use_residues: bool,
    ) -> PropagationStatusCP {
        self.counters.executions.fetch_add(1, Ordering::Relaxed);
        for column in 0..3 {
            self.propagate_column(context, column, use_residues)?;
        }
        Ok(())
    }
}

impl Propagator for SparseEndpointSupportPropagator {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::Medium
    }

    fn notify(
        &mut self,
        _context: NotificationContext,
        _local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        self.counters.notifications.fetch_add(1, Ordering::Relaxed);
        EnqueueDecision::Enqueue
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        let mut scratch = self.clone();
        scratch.propagate_all(&mut context, false)
    }

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        self.propagate_all(&mut context, true)
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::Solver;
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::results::CSPSolverExecutionFlag;

    use super::*;

    const ROWS: [[i32; 3]; 5] = [[0, 0, 10], [0, 1, 11], [1, 0, 12], [1, 1, 13], [2, 1, 14]];

    fn build_sparse() -> (Solver, [DomainId; 3]) {
        let mut solver = Solver::default();
        let variables = [
            solver.new_named_sparse_integer([0, 1, 2], "placement"),
            solver.new_named_sparse_integer([0, 1], "port"),
            solver.new_named_sparse_integer([10, 11, 12, 13, 14], "geometry"),
        ];
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(SparseEndpointSupportPropagatorArgs {
            name: "controlled-sparse-endpoint-support".to_string(),
            variables,
            domain_values: [vec![0, 1, 2], vec![0, 1], vec![10, 11, 12, 13, 14]],
            rows: ROWS.to_vec(),
            counters: Arc::default(),
            constraint_tag: tag,
        });
        (solver, variables)
    }

    #[test]
    fn filters_in_all_three_directions() {
        let (mut solver, [placement, port, geometry]) = build_sparse();
        let tag = solver.new_constraint_tag();
        solver.add_clause([port.equality_predicate(0)], tag);
        solver.add_clause([geometry.disequality_predicate(12)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(!solver.contains(&placement, 1));
        assert!(!solver.contains(&placement, 2));
        assert!(!solver.contains(&geometry, 11));
        assert!(!solver.contains(&geometry, 13));
        assert!(!solver.contains(&geometry, 14));
    }

    #[test]
    fn complete_assignment_relation_is_exact() {
        for placement_value in [0, 1, 2] {
            for port_value in [0, 1] {
                for geometry_value in [10, 11, 12, 13, 14] {
                    let (mut solver, variables) = build_sparse();
                    let tag = solver.new_constraint_tag();
                    for (variable, value) in
                        variables
                            .into_iter()
                            .zip([placement_value, port_value, geometry_value])
                    {
                        solver.add_clause([variable.equality_predicate(value)], tag);
                    }
                    let observed =
                        solver.propagate_to_fixpoint() == CSPSolverExecutionFlag::Feasible;
                    assert_eq!(
                        observed,
                        ROWS.contains(&[placement_value, port_value, geometry_value]),
                        "tuple=({placement_value},{port_value},{geometry_value})"
                    );
                }
            }
        }
    }

    #[test]
    fn values_outside_the_row_projection_are_removed_at_root() {
        let mut solver = Solver::default();
        let variables = [
            solver.new_named_sparse_integer([0, 1, 2, 3], "placement"),
            solver.new_named_sparse_integer([0, 1, 2], "port"),
            solver.new_named_sparse_integer([9, 10, 11, 12, 13, 14, 15], "geometry"),
        ];
        let counters = Arc::default();
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(SparseEndpointSupportPropagatorArgs {
            name: "projection-pruning-sparse-endpoint-support".to_string(),
            variables,
            domain_values: [
                vec![0, 1, 2, 3],
                vec![0, 1, 2],
                vec![9, 10, 11, 12, 13, 14, 15],
            ],
            rows: ROWS.to_vec(),
            counters,
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(!solver.contains(&variables[0], 3));
        assert!(!solver.contains(&variables[1], 2));
        assert!(!solver.contains(&variables[2], 9));
        assert!(!solver.contains(&variables[2], 15));
    }

    #[test]
    fn empty_relation_reports_infeasible_without_panicking() {
        let mut solver = Solver::default();
        let variables = [
            solver.new_named_sparse_integer([0, 1], "placement"),
            solver.new_named_sparse_integer([0], "port"),
            solver.new_named_sparse_integer([10], "geometry"),
        ];
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(SparseEndpointSupportPropagatorArgs {
            name: "empty-sparse-endpoint-support".to_string(),
            variables,
            domain_values: [vec![0, 1], vec![0], vec![10]],
            rows: Vec::new(),
            counters: Arc::default(),
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
    }
}
