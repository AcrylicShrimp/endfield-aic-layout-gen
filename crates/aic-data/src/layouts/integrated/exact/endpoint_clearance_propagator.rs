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
use pumpkin_solver::core::variables::{DomainId, Literal};

use super::ladder::EndpointClearancePropagationStatistics;

declare_inference_label!(EndpointRectangleClearance);

#[derive(Debug, Default)]
pub(in crate::layouts::integrated) struct EndpointClearancePropagationCounters {
    relations: AtomicU64,
    executions: AtomicU64,
    notifications: AtomicU64,
    orientation_checks: AtomicU64,
    rejected_orientations: AtomicU64,
    forced_separation_detections: AtomicU64,
    bound_updates: AtomicU64,
    conflicts: AtomicU64,
    maximum_reason_predicates: AtomicU64,
}

impl EndpointClearancePropagationCounters {
    pub(in crate::layouts::integrated) fn snapshot(
        &self,
    ) -> EndpointClearancePropagationStatistics {
        EndpointClearancePropagationStatistics {
            relations: self.relations.load(Ordering::Relaxed),
            executions: self.executions.load(Ordering::Relaxed),
            notifications: self.notifications.load(Ordering::Relaxed),
            orientation_checks: self.orientation_checks.load(Ordering::Relaxed),
            rejected_orientations: self.rejected_orientations.load(Ordering::Relaxed),
            forced_separation_detections: self.forced_separation_detections.load(Ordering::Relaxed),
            bound_updates: self.bound_updates.load(Ordering::Relaxed),
            conflicts: self.conflicts.load(Ordering::Relaxed),
            maximum_reason_predicates: self.maximum_reason_predicates.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::layouts::integrated) struct EndpointClearanceOrientation {
    pub selected: Literal,
    pub selected_parent: DomainId,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated) struct EndpointRectangleClearancePropagatorArgs {
    pub name: String,
    pub connection_x: DomainId,
    pub connection_y: DomainId,
    pub facility_x: DomainId,
    pub facility_y: DomainId,
    pub orientations: Vec<EndpointClearanceOrientation>,
    pub counters: Arc<EndpointClearancePropagationCounters>,
    pub constraint_tag: ConstraintTag,
}

impl PropagatorConstructor for EndpointRectangleClearancePropagatorArgs {
    type PropagatorImpl = EndpointRectangleClearancePropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        assert!(!self.orientations.is_empty());
        for orientation in &self.orientations {
            assert!(orientation.width > 0);
            assert!(orientation.height > 0);
        }
        let mut registration = EventsToRegister::builder()
            .add(&self.connection_x, DomainEvents::BOUNDS, LocalId::from(0))
            .add(&self.connection_y, DomainEvents::BOUNDS, LocalId::from(1))
            .add(&self.facility_x, DomainEvents::BOUNDS, LocalId::from(2))
            .add(&self.facility_y, DomainEvents::BOUNDS, LocalId::from(3));
        for (index, orientation) in self.orientations.iter().enumerate() {
            registration = registration.add(
                &orientation.selected_parent,
                DomainEvents::ANY_INT,
                LocalId::from(u32::try_from(index + 4).expect("orientation count fits u32")),
            );
        }
        self.counters.relations.fetch_add(1, Ordering::Relaxed);
        PropagatorSpec {
            registration: registration.build(),
            checkers: RuntimeCheckers::empty(),
            propagator: EndpointRectangleClearancePropagator {
                name: self.name,
                connection_x: self.connection_x,
                connection_y: self.connection_y,
                facility_x: self.facility_x,
                facility_y: self.facility_y,
                orientations: self.orientations,
                counters: self.counters,
                inference_code: InferenceCode::new(self.constraint_tag, EndpointRectangleClearance),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated) struct EndpointRectangleClearancePropagator {
    name: String,
    connection_x: DomainId,
    connection_y: DomainId,
    facility_x: DomainId,
    facility_y: DomainId,
    orientations: Vec<EndpointClearanceOrientation>,
    counters: Arc<EndpointClearancePropagationCounters>,
    inference_code: InferenceCode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Separation {
    Left,
    Right,
    Above,
    Below,
}

#[derive(Clone, Copy)]
struct Bounds {
    connection_x_lower: i32,
    connection_x_upper: i32,
    connection_y_lower: i32,
    connection_y_upper: i32,
    facility_x_lower: i32,
    facility_x_upper: i32,
    facility_y_lower: i32,
    facility_y_upper: i32,
}

impl EndpointRectangleClearancePropagator {
    fn bounds(&self, context: &impl ReadDomains) -> Bounds {
        Bounds {
            connection_x_lower: context.lower_bound(&self.connection_x),
            connection_x_upper: context.upper_bound(&self.connection_x),
            connection_y_lower: context.lower_bound(&self.connection_y),
            connection_y_upper: context.upper_bound(&self.connection_y),
            facility_x_lower: context.lower_bound(&self.facility_x),
            facility_x_upper: context.upper_bound(&self.facility_x),
            facility_y_lower: context.lower_bound(&self.facility_y),
            facility_y_upper: context.upper_bound(&self.facility_y),
        }
    }

    fn possible_separations(
        bounds: Bounds,
        orientation: EndpointClearanceOrientation,
    ) -> Vec<Separation> {
        let mut possible = Vec::with_capacity(4);
        if bounds.connection_x_lower < bounds.facility_x_upper {
            possible.push(Separation::Left);
        }
        if bounds.connection_x_upper >= bounds.facility_x_lower + orientation.width {
            possible.push(Separation::Right);
        }
        if bounds.connection_y_lower < bounds.facility_y_upper {
            possible.push(Separation::Above);
        }
        if bounds.connection_y_upper >= bounds.facility_y_lower + orientation.height {
            possible.push(Separation::Below);
        }
        possible
    }

    fn impossible_reason(&self, bounds: Bounds, separation: Separation) -> [Predicate; 2] {
        match separation {
            Separation::Left => [
                self.connection_x
                    .lower_bound_predicate(bounds.connection_x_lower),
                self.facility_x
                    .upper_bound_predicate(bounds.facility_x_upper),
            ],
            Separation::Right => [
                self.connection_x
                    .upper_bound_predicate(bounds.connection_x_upper),
                self.facility_x
                    .lower_bound_predicate(bounds.facility_x_lower),
            ],
            Separation::Above => [
                self.connection_y
                    .lower_bound_predicate(bounds.connection_y_lower),
                self.facility_y
                    .upper_bound_predicate(bounds.facility_y_upper),
            ],
            Separation::Below => [
                self.connection_y
                    .upper_bound_predicate(bounds.connection_y_upper),
                self.facility_y
                    .lower_bound_predicate(bounds.facility_y_lower),
            ],
        }
    }

    fn all_impossible_reason(&self, bounds: Bounds) -> PropositionalConjunction {
        let predicates = [
            Separation::Left,
            Separation::Right,
            Separation::Above,
            Separation::Below,
        ]
        .into_iter()
        .flat_map(|separation| self.impossible_reason(bounds, separation))
        .collect();
        PropositionalConjunction::new(predicates)
    }

    fn forced_reason(
        &self,
        bounds: Bounds,
        orientation: EndpointClearanceOrientation,
        forced: Separation,
        supporting_bound: Predicate,
    ) -> PropositionalConjunction {
        let mut predicates = vec![orientation.selected.get_true_predicate(), supporting_bound];
        for separation in [
            Separation::Left,
            Separation::Right,
            Separation::Above,
            Separation::Below,
        ] {
            if separation != forced {
                predicates.extend(self.impossible_reason(bounds, separation));
            }
        }
        predicates.sort_unstable();
        predicates.dedup();
        PropositionalConjunction::new(predicates)
    }

    fn note_reason(&self, reason: &PropositionalConjunction) {
        self.counters.maximum_reason_predicates.fetch_max(
            reason.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    fn post(
        &self,
        context: &mut PropagationContext,
        conclusion: Predicate,
        reason: PropositionalConjunction,
    ) -> PropagationStatusCP {
        self.note_reason(&reason);
        self.counters.bound_updates.fetch_add(1, Ordering::Relaxed);
        if let Err(conflict) = context.post(conclusion, (reason, &self.inference_code)) {
            self.counters.conflicts.fetch_add(1, Ordering::Relaxed);
            return Err(conflict.into());
        }
        Ok(())
    }

    fn force_separation(
        &self,
        context: &mut PropagationContext,
        bounds: Bounds,
        orientation: EndpointClearanceOrientation,
        separation: Separation,
    ) -> PropagationStatusCP {
        self.counters
            .forced_separation_detections
            .fetch_add(1, Ordering::Relaxed);
        match separation {
            Separation::Left => {
                let connection_upper = bounds.facility_x_upper - 1;
                if connection_upper < bounds.connection_x_upper {
                    self.post(
                        context,
                        self.connection_x.upper_bound_predicate(connection_upper),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.facility_x
                                .upper_bound_predicate(bounds.facility_x_upper),
                        ),
                    )?;
                }
                let facility_lower = bounds.connection_x_lower + 1;
                if facility_lower > bounds.facility_x_lower {
                    self.post(
                        context,
                        self.facility_x.lower_bound_predicate(facility_lower),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.connection_x
                                .lower_bound_predicate(bounds.connection_x_lower),
                        ),
                    )?;
                }
            }
            Separation::Right => {
                let connection_lower = bounds.facility_x_lower + orientation.width;
                if connection_lower > bounds.connection_x_lower {
                    self.post(
                        context,
                        self.connection_x.lower_bound_predicate(connection_lower),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.facility_x
                                .lower_bound_predicate(bounds.facility_x_lower),
                        ),
                    )?;
                }
                let facility_upper = bounds.connection_x_upper - orientation.width;
                if facility_upper < bounds.facility_x_upper {
                    self.post(
                        context,
                        self.facility_x.upper_bound_predicate(facility_upper),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.connection_x
                                .upper_bound_predicate(bounds.connection_x_upper),
                        ),
                    )?;
                }
            }
            Separation::Above => {
                let connection_upper = bounds.facility_y_upper - 1;
                if connection_upper < bounds.connection_y_upper {
                    self.post(
                        context,
                        self.connection_y.upper_bound_predicate(connection_upper),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.facility_y
                                .upper_bound_predicate(bounds.facility_y_upper),
                        ),
                    )?;
                }
                let facility_lower = bounds.connection_y_lower + 1;
                if facility_lower > bounds.facility_y_lower {
                    self.post(
                        context,
                        self.facility_y.lower_bound_predicate(facility_lower),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.connection_y
                                .lower_bound_predicate(bounds.connection_y_lower),
                        ),
                    )?;
                }
            }
            Separation::Below => {
                let connection_lower = bounds.facility_y_lower + orientation.height;
                if connection_lower > bounds.connection_y_lower {
                    self.post(
                        context,
                        self.connection_y.lower_bound_predicate(connection_lower),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.facility_y
                                .lower_bound_predicate(bounds.facility_y_lower),
                        ),
                    )?;
                }
                let facility_upper = bounds.connection_y_upper - orientation.height;
                if facility_upper < bounds.facility_y_upper {
                    self.post(
                        context,
                        self.facility_y.upper_bound_predicate(facility_upper),
                        self.forced_reason(
                            bounds,
                            orientation,
                            separation,
                            self.connection_y
                                .upper_bound_predicate(bounds.connection_y_upper),
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn propagate_all(&self, context: &mut PropagationContext) -> PropagationStatusCP {
        self.counters.executions.fetch_add(1, Ordering::Relaxed);
        for orientation in &self.orientations {
            if context.evaluate_predicate(orientation.selected.get_false_predicate()) == Some(true)
            {
                continue;
            }
            self.counters
                .orientation_checks
                .fetch_add(1, Ordering::Relaxed);
            let bounds = self.bounds(context);
            let possible = Self::possible_separations(bounds, *orientation);
            if possible.is_empty() {
                let reason = self.all_impossible_reason(bounds);
                self.note_reason(&reason);
                self.counters
                    .rejected_orientations
                    .fetch_add(1, Ordering::Relaxed);
                if let Err(conflict) = context.post(
                    orientation.selected.get_false_predicate(),
                    (reason, &self.inference_code),
                ) {
                    self.counters.conflicts.fetch_add(1, Ordering::Relaxed);
                    return Err(conflict.into());
                }
            } else if possible.len() == 1
                && context.evaluate_predicate(orientation.selected.get_true_predicate())
                    == Some(true)
            {
                self.force_separation(context, bounds, *orientation, possible[0])?;
            }
        }
        Ok(())
    }
}

impl Propagator for EndpointRectangleClearancePropagator {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::High
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

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        self.propagate_all(&mut context)
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.propagate_all(&mut context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_solver::Solver;
    use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
    use pumpkin_solver::core::branching::Brancher;
    use pumpkin_solver::core::branching::branchers::dynamic_brancher::DynamicBrancher;
    use pumpkin_solver::core::branching::branchers::warm_start::WarmStart;
    use pumpkin_solver::core::constraints::NegatableConstraint;
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::results::{
        CSPSolverExecutionFlag, ProblemSolution, SatisfactionResult,
    };
    use pumpkin_solver::core::termination::Indefinite;
    use pumpkin_solver::core::variables::TransformableVariable;

    fn add_single_orientation(
        solver: &mut Solver,
        connection_x: DomainId,
        connection_y: DomainId,
        facility_x: DomainId,
        facility_y: DomainId,
        width: i32,
        height: i32,
    ) -> Literal {
        let selected = solver.new_named_literal("selected-orientation");
        let selected_parent = *selected.get_integer_variable().inner();
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
            name: "controlled-endpoint-clearance".to_string(),
            connection_x,
            connection_y,
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent,
                width,
                height,
            }],
            counters: Arc::default(),
            constraint_tag: tag,
        });
        solver.add_clause([selected.get_true_predicate()], tag);
        selected
    }

    fn add_reified_orientation(
        solver: &mut Solver,
        selected: Literal,
        connection_x: DomainId,
        connection_y: DomainId,
        facility_x: DomainId,
        facility_y: DomainId,
        width: i32,
        height: i32,
    ) {
        let tag = solver.new_constraint_tag();
        let directions = [
            (vec![connection_x.scaled(1), facility_x.scaled(-1)], -1),
            (vec![facility_x.scaled(1), connection_x.scaled(-1)], -width),
            (vec![connection_y.scaled(1), facility_y.scaled(-1)], -1),
            (vec![facility_y.scaled(1), connection_y.scaled(-1)], -height),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (terms, rhs))| {
            let direction = solver.new_named_literal(format!("direction-{index}"));
            pumpkin_solver::less_than_or_equals(terms, rhs, tag).reify(solver, direction);
            direction
        })
        .collect::<Vec<_>>();
        let mut clause = vec![selected.get_false_predicate()];
        clause.extend(
            directions
                .into_iter()
                .map(|direction| direction.get_true_predicate()),
        );
        solver.add_clause(clause, tag);
    }

    fn fixed_multiple_orientation_case(
        propagated: bool,
        connection: (i32, i32),
        facility: (i32, i32),
        selected_index: usize,
    ) -> bool {
        let mut solver = Solver::default();
        let connection_x = solver.new_named_bounded_integer(0, 3, "connection-x");
        let connection_y = solver.new_named_bounded_integer(0, 3, "connection-y");
        let facility_x = solver.new_named_bounded_integer(0, 3, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 3, "facility-y");
        let selected = [
            solver.new_named_literal("orientation-2x3"),
            solver.new_named_literal("orientation-3x2"),
        ];
        let dimensions = [(2, 3), (3, 2)];
        let tag = solver.new_constraint_tag();
        solver.add_clause(
            selected.iter().map(|literal| literal.get_true_predicate()),
            tag,
        );
        solver.add_clause(
            selected.iter().map(|literal| literal.get_false_predicate()),
            tag,
        );
        solver.add_clause([selected[selected_index].get_true_predicate()], tag);
        if propagated {
            let orientations = selected
                .into_iter()
                .zip(dimensions)
                .map(|(selected, (width, height))| EndpointClearanceOrientation {
                    selected,
                    selected_parent: *selected.get_integer_variable().inner(),
                    width,
                    height,
                })
                .collect();
            let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
                name: "differential-endpoint-clearance".to_string(),
                connection_x,
                connection_y,
                facility_x,
                facility_y,
                orientations,
                counters: Arc::default(),
                constraint_tag: tag,
            });
        } else {
            for (selected, (width, height)) in selected.into_iter().zip(dimensions) {
                add_reified_orientation(
                    &mut solver,
                    selected,
                    connection_x,
                    connection_y,
                    facility_x,
                    facility_y,
                    width,
                    height,
                );
            }
        }
        for (variable, value) in [
            (connection_x, connection.0),
            (connection_y, connection.1),
            (facility_x, facility.0),
            (facility_y, facility.1),
        ] {
            solver.add_clause([variable.equality_predicate(value)], tag);
        }
        solver.propagate_to_fixpoint() == CSPSolverExecutionFlag::Feasible
    }

    #[test]
    fn complete_assignments_match_the_direct_rectangle_oracle() {
        for connection_x_value in 0..4 {
            for connection_y_value in 0..4 {
                for facility_x_value in 0..4 {
                    for facility_y_value in 0..4 {
                        let mut solver = Solver::default();
                        let connection_x = solver.new_named_bounded_integer(0, 3, "connection-x");
                        let connection_y = solver.new_named_bounded_integer(0, 3, "connection-y");
                        let facility_x = solver.new_named_bounded_integer(0, 3, "facility-x");
                        let facility_y = solver.new_named_bounded_integer(0, 3, "facility-y");
                        add_single_orientation(
                            &mut solver,
                            connection_x,
                            connection_y,
                            facility_x,
                            facility_y,
                            2,
                            3,
                        );
                        let tag = solver.new_constraint_tag();
                        for (variable, value) in [
                            (connection_x, connection_x_value),
                            (connection_y, connection_y_value),
                            (facility_x, facility_x_value),
                            (facility_y, facility_y_value),
                        ] {
                            solver.add_clause([variable.equality_predicate(value)], tag);
                        }
                        let observed =
                            solver.propagate_to_fixpoint() == CSPSolverExecutionFlag::Feasible;
                        let expected = connection_x_value < facility_x_value
                            || connection_x_value >= facility_x_value + 2
                            || connection_y_value < facility_y_value
                            || connection_y_value >= facility_y_value + 3;
                        assert_eq!(
                            observed, expected,
                            "connection=({connection_x_value},{connection_y_value}) facility=({facility_x_value},{facility_y_value})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn multiple_orientations_match_the_reified_baseline_exhaustively() {
        for connection_x in 0..4 {
            for connection_y in 0..4 {
                for facility_x in 0..4 {
                    for facility_y in 0..4 {
                        for selected_index in 0..2 {
                            let connection = (connection_x, connection_y);
                            let facility = (facility_x, facility_y);
                            assert_eq!(
                                fixed_multiple_orientation_case(
                                    true,
                                    connection,
                                    facility,
                                    selected_index,
                                ),
                                fixed_multiple_orientation_case(
                                    false,
                                    connection,
                                    facility,
                                    selected_index,
                                ),
                                "connection={connection:?} facility={facility:?} selected={selected_index}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn half_open_rectangle_boundaries_are_exact() {
        for (connection, expected) in [
            ((0, 1), true),
            ((1, 1), false),
            ((2, 1), false),
            ((3, 1), true),
            ((1, 0), true),
            ((1, 3), false),
            ((1, 4), true),
        ] {
            let mut solver = Solver::default();
            let connection_x =
                solver.new_named_bounded_integer(connection.0, connection.0, "connection-x");
            let connection_y =
                solver.new_named_bounded_integer(connection.1, connection.1, "connection-y");
            let facility_x = solver.new_named_bounded_integer(1, 1, "facility-x");
            let facility_y = solver.new_named_bounded_integer(1, 1, "facility-y");
            add_single_orientation(
                &mut solver,
                connection_x,
                connection_y,
                facility_x,
                facility_y,
                2,
                3,
            );
            assert_eq!(
                solver.propagate_to_fixpoint() == CSPSolverExecutionFlag::Feasible,
                expected,
                "connection={connection:?}"
            );
        }
    }

    #[test]
    fn an_unsupported_orientation_is_removed_without_rejecting_a_supported_one() {
        let mut solver = Solver::default();
        let connection_x = solver.new_named_bounded_integer(1, 1, "connection-x");
        let connection_y = solver.new_named_bounded_integer(1, 1, "connection-y");
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let blocked = solver.new_named_literal("blocked-orientation");
        let supported = solver.new_named_literal("supported-orientation");
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
            name: "orientation-pruning-endpoint-clearance".to_string(),
            connection_x,
            connection_y,
            facility_x,
            facility_y,
            orientations: vec![
                EndpointClearanceOrientation {
                    selected: blocked,
                    selected_parent: *blocked.get_integer_variable().inner(),
                    width: 3,
                    height: 3,
                },
                EndpointClearanceOrientation {
                    selected: supported,
                    selected_parent: *supported.get_integer_variable().inner(),
                    width: 1,
                    height: 1,
                },
            ],
            counters: Arc::default(),
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(
            solver.upper_bound(blocked.get_integer_variable().inner()),
            0
        );
        assert_eq!(
            solver.upper_bound(supported.get_integer_variable().inner()),
            1
        );
    }

    #[test]
    fn an_unselected_orientation_does_not_restrict_an_inside_point() {
        let mut solver = Solver::default();
        let connection_x = solver.new_named_bounded_integer(1, 1, "connection-x");
        let connection_y = solver.new_named_bounded_integer(1, 1, "connection-y");
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let selected = solver.new_named_literal("unselected-orientation");
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
            name: "unselected-endpoint-clearance".to_string(),
            connection_x,
            connection_y,
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 3,
                height: 3,
            }],
            counters: Arc::default(),
            constraint_tag: tag,
        });
        solver.add_clause([selected.get_false_predicate()], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
    }

    #[test]
    fn every_unique_separation_propagates_coordinate_bounds() {
        let cases = [
            (Separation::Left, (5, 8), (3, 3), (4, 6), (2, 3)),
            (Separation::Right, (6, 10), (3, 3), (4, 6), (2, 3)),
            (Separation::Above, (3, 3), (5, 8), (2, 3), (4, 6)),
            (Separation::Below, (3, 3), (6, 10), (2, 3), (4, 6)),
        ];
        for (separation, cx, cy, fx, fy) in cases {
            let mut solver = Solver::default();
            let connection_x = solver.new_named_bounded_integer(cx.0, cx.1, "connection-x");
            let connection_y = solver.new_named_bounded_integer(cy.0, cy.1, "connection-y");
            let facility_x = solver.new_named_bounded_integer(fx.0, fx.1, "facility-x");
            let facility_y = solver.new_named_bounded_integer(fy.0, fy.1, "facility-y");
            add_single_orientation(
                &mut solver,
                connection_x,
                connection_y,
                facility_x,
                facility_y,
                5,
                5,
            );
            assert_eq!(
                solver.propagate_to_fixpoint(),
                CSPSolverExecutionFlag::Feasible,
                "separation={separation:?}"
            );
            match separation {
                Separation::Left => {
                    assert_eq!(solver.upper_bound(&connection_x), 5);
                    assert_eq!(solver.lower_bound(&facility_x), 6);
                }
                Separation::Right => {
                    assert_eq!(solver.lower_bound(&connection_x), 9);
                    assert_eq!(solver.upper_bound(&facility_x), 5);
                }
                Separation::Above => {
                    assert_eq!(solver.upper_bound(&connection_y), 5);
                    assert_eq!(solver.lower_bound(&facility_y), 6);
                }
                Separation::Below => {
                    assert_eq!(solver.lower_bound(&connection_y), 9);
                    assert_eq!(solver.upper_bound(&facility_y), 5);
                }
            }
        }
    }

    #[test]
    fn a_custom_reason_survives_conflict_learning_and_backtracking() {
        let mut solver = Solver::default();
        let connection_x = solver.new_named_bounded_integer(1, 3, "connection-x");
        let connection_y = solver.new_named_bounded_integer(1, 1, "connection-y");
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let choice = solver.new_named_literal("try-blocked-orientation-first");
        let blocked = solver.new_named_literal("blocked-orientation");
        let supported = solver.new_named_literal("supported-orientation");
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
            name: "backtracking-endpoint-clearance".to_string(),
            connection_x,
            connection_y,
            facility_x,
            facility_y,
            orientations: vec![
                EndpointClearanceOrientation {
                    selected: blocked,
                    selected_parent: *blocked.get_integer_variable().inner(),
                    width: 3,
                    height: 3,
                },
                EndpointClearanceOrientation {
                    selected: supported,
                    selected_parent: *supported.get_integer_variable().inner(),
                    width: 1,
                    height: 1,
                },
            ],
            counters: Arc::clone(&counters),
            constraint_tag: tag,
        });
        solver.add_clause(
            [choice.get_false_predicate(), blocked.get_true_predicate()],
            tag,
        );
        solver.add_clause(
            [choice.get_true_predicate(), supported.get_true_predicate()],
            tag,
        );
        solver.add_clause(
            [blocked.get_true_predicate(), supported.get_true_predicate()],
            tag,
        );
        solver.add_clause(
            [
                blocked.get_false_predicate(),
                supported.get_false_predicate(),
            ],
            tag,
        );
        solver
            .add_constraint(pumpkin_solver::equals(vec![connection_x.scaled(1)], 1, tag))
            .implied_by(choice);
        let choice_parent = *choice.get_integer_variable().inner();
        let branchers: Vec<Box<dyn Brancher>> = vec![
            Box::new(WarmStart::new(&[choice_parent], &[1])),
            Box::new(solver.default_brancher()),
        ];
        let mut brancher = DynamicBrancher::new(branchers);
        let mut resolver = ResolutionResolver::default();
        let result = solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver);
        let SatisfactionResult::Satisfiable(result) = result else {
            panic!("supported sibling should survive the blocked branch");
        };
        let solution = result.solution();
        assert!(!solution.get_literal_value(choice));
        assert!(solution.get_literal_value(supported));
        assert!(counters.conflicts.load(Ordering::Relaxed) > 0);
    }
}
