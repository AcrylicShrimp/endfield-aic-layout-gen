use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use pumpkin_solver::core::declare_inference_label;
use pumpkin_solver::core::predicates::{PredicateConstructor, PropositionalConjunction};
use pumpkin_solver::core::proof::InferenceCode;
use pumpkin_solver::core::propagation::{
    DomainEvents, EnqueueDecision, EventsToRegister, LocalId, NotificationContext,
    OpaqueDomainEvent, Priority, PropagationContext, Propagator, PropagatorConstructor,
    PropagatorConstructorContext, PropagatorSpec, ReadDomains, RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

use super::LayerGridAnalyzerCounters;

declare_inference_label!(GuardedPositiveItemIntersection);

#[derive(Clone, Copy, Debug)]
pub(in crate::layouts::integrated::exact) struct GuardedPositiveItemPair {
    pub left: DomainId,
    pub right: DomainId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::layouts::integrated::exact) enum GuardedPositiveItemRelationKind {
    RouteArc,
    Bridge,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::layouts::integrated::exact) struct GuardedPositiveItemRelation {
    pub guard: DomainId,
    pub first: GuardedPositiveItemPair,
    pub second: Option<GuardedPositiveItemPair>,
    pub maximum_item_code: i32,
    pub kind: GuardedPositiveItemRelationKind,
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct GuardedPositiveItemIntersectionPropagatorArgs {
    pub name: String,
    pub relations: Vec<GuardedPositiveItemRelation>,
    pub counters: Arc<LayerGridAnalyzerCounters>,
    pub constraint_tag: pumpkin_solver::core::proof::ConstraintTag,
}

impl PropagatorConstructor for GuardedPositiveItemIntersectionPropagatorArgs {
    type PropagatorImpl = GuardedPositiveItemIntersectionPropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let mut impacts = BTreeMap::<DomainId, BTreeSet<usize>>::new();
        for (relation_index, relation) in self.relations.iter().enumerate() {
            for variable in [Some(relation.first), relation.second]
                .into_iter()
                .flatten()
                .flat_map(|pair| [pair.left, pair.right])
            {
                impacts.entry(variable).or_default().insert(relation_index);
            }
        }
        let mut entries = impacts.into_iter();
        let (first_variable, first_impacts) = entries
            .next()
            .expect("a guarded positive-item propagator has at least one relation");
        let mut event_impacts = vec![first_impacts.into_iter().collect::<Vec<_>>()];
        let mut registration = EventsToRegister::builder()
            .add(&first_variable, DomainEvents::ANY_INT, LocalId::from(0))
            .build();
        for (index, (variable, relation_indices)) in entries.enumerate() {
            registration.add(
                &variable,
                DomainEvents::ANY_INT,
                LocalId::from(
                    u32::try_from(index + 1).expect("guarded item variable count fits u32"),
                ),
            );
            event_impacts.push(relation_indices.into_iter().collect());
        }
        self.counters
            .guarded_intersection_registered_relations
            .fetch_add(
                self.relations.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        self.counters
            .guarded_intersection_registered_domain_variables
            .fetch_add(
                event_impacts.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        let relation_count = self.relations.len();
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: GuardedPositiveItemIntersectionPropagator {
                name: self.name,
                relations: self.relations,
                counters: self.counters,
                event_impacts,
                dirty_relations: (0..relation_count).collect(),
                relation_is_dirty: vec![true; relation_count],
                inference_code: InferenceCode::new(
                    self.constraint_tag,
                    GuardedPositiveItemIntersection,
                ),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct GuardedPositiveItemIntersectionPropagator {
    name: String,
    relations: Vec<GuardedPositiveItemRelation>,
    counters: Arc<LayerGridAnalyzerCounters>,
    event_impacts: Vec<Vec<usize>>,
    dirty_relations: Vec<usize>,
    relation_is_dirty: Vec<bool>,
    inference_code: InferenceCode,
}

impl GuardedPositiveItemIntersectionPropagator {
    fn disjoint_reason(
        &self,
        context: &impl ReadDomains,
        pair: GuardedPositiveItemPair,
        maximum_item_code: i32,
    ) -> Option<PropositionalConjunction> {
        let mut reason = PropositionalConjunction::default();
        let mut membership_checks = 0;
        for item_code in 1..=maximum_item_code {
            membership_checks += 1;
            if !context.contains(&pair.left, item_code) {
                reason.push(pair.left.disequality_predicate(item_code));
                continue;
            }
            membership_checks += 1;
            if !context.contains(&pair.right, item_code) {
                reason.push(pair.right.disequality_predicate(item_code));
                continue;
            }
            self.counters
                .guarded_intersection_membership_checks
                .fetch_add(membership_checks, Ordering::Relaxed);
            return None;
        }
        self.counters
            .guarded_intersection_membership_checks
            .fetch_add(membership_checks, Ordering::Relaxed);
        self.counters
            .guarded_intersection_maximum_reason_predicates
            .fetch_max(
                reason.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        Some(reason)
    }

    fn propagate_relation(
        &self,
        context: &mut PropagationContext,
        relation_index: usize,
    ) -> PropagationStatusCP {
        let relation = self.relations[relation_index];
        self.counters
            .guarded_intersection_relation_checks
            .fetch_add(1, Ordering::Relaxed);
        match context.evaluate_predicate(relation.guard.equality_predicate(1)) {
            Some(false) => return Ok(()),
            Some(true) => {}
            None => {
                self.counters
                    .guarded_intersection_unresolved_guard_checks
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        let mut disjoint_reason = None;
        for pair in [Some(relation.first), relation.second]
            .into_iter()
            .flatten()
        {
            if let Some(reason) = self.disjoint_reason(context, pair, relation.maximum_item_code) {
                disjoint_reason = Some(reason);
                break;
            }
        }
        let Some(reason) = disjoint_reason else {
            self.counters
                .guarded_intersection_supported_checks
                .fetch_add(1, Ordering::Relaxed);
            return Ok(());
        };
        self.counters
            .guarded_intersection_disjoint_checks
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .guarded_intersection_forced_guard_rejections
            .fetch_add(1, Ordering::Relaxed);
        match relation.kind {
            GuardedPositiveItemRelationKind::RouteArc => self
                .counters
                .guarded_intersection_forced_route_arc_rejections
                .fetch_add(1, Ordering::Relaxed),
            GuardedPositiveItemRelationKind::Bridge => self
                .counters
                .guarded_intersection_forced_bridge_rejections
                .fetch_add(1, Ordering::Relaxed),
        };
        if let Err(conflict) = context.post(
            relation.guard.upper_bound_predicate(0),
            (reason, &self.inference_code),
        ) {
            self.counters
                .guarded_intersection_active_conflicts
                .fetch_add(1, Ordering::Relaxed);
            return Err(conflict.into());
        }
        Ok(())
    }
}

impl Propagator for GuardedPositiveItemIntersectionPropagator {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::High
    }

    fn notify(
        &mut self,
        context: NotificationContext,
        local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        self.counters
            .guarded_intersection_notifications
            .fetch_add(1, Ordering::Relaxed);
        let mut dirtied_any = false;
        for relation_index in &self.event_impacts[local_id.unpack() as usize] {
            if self.relation_is_dirty[*relation_index]
                || context
                    .evaluate_predicate(self.relations[*relation_index].guard.equality_predicate(1))
                    == Some(false)
            {
                continue;
            }
            self.relation_is_dirty[*relation_index] = true;
            self.dirty_relations.push(*relation_index);
            dirtied_any = true;
        }
        if dirtied_any {
            EnqueueDecision::Enqueue
        } else {
            EnqueueDecision::Skip
        }
    }

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        self.counters
            .guarded_intersection_executions
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .guarded_intersection_maximum_dirty_relations
            .fetch_max(
                self.dirty_relations.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        while let Some(relation_index) = self.dirty_relations.pop() {
            self.relation_is_dirty[relation_index] = false;
            self.propagate_relation(&mut context, relation_index)?;
        }
        Ok(())
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.counters
            .guarded_intersection_executions
            .fetch_add(1, Ordering::Relaxed);
        for relation_index in 0..self.relations.len() {
            self.propagate_relation(&mut context, relation_index)?;
        }
        Ok(())
    }

    fn synchronise(&mut self, _context: NotificationContext<'_>) {
        for relation_index in self.dirty_relations.drain(..) {
            self.relation_is_dirty[relation_index] = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pumpkin_solver::Solver;
    use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
    use pumpkin_solver::core::branching::Brancher;
    use pumpkin_solver::core::branching::branchers::dynamic_brancher::DynamicBrancher;
    use pumpkin_solver::core::branching::branchers::warm_start::WarmStart;
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::results::{CSPSolverExecutionFlag, SatisfactionResult};
    use pumpkin_solver::core::termination::Indefinite;

    use super::*;

    fn add_propagator(
        solver: &mut Solver,
        guard: DomainId,
        left: DomainId,
        right: DomainId,
        maximum_item_code: i32,
        counters: Arc<LayerGridAnalyzerCounters>,
    ) {
        let constraint_tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(GuardedPositiveItemIntersectionPropagatorArgs {
            name: "test-active-guarded-item-intersection".to_string(),
            relations: vec![GuardedPositiveItemRelation {
                guard,
                first: GuardedPositiveItemPair { left, right },
                second: None,
                maximum_item_code,
                kind: GuardedPositiveItemRelationKind::RouteArc,
            }],
            counters,
            constraint_tag,
        });
    }

    fn add_bridge_propagator(
        solver: &mut Solver,
        guard: DomainId,
        horizontal: GuardedPositiveItemPair,
        vertical: GuardedPositiveItemPair,
        maximum_item_code: i32,
        counters: Arc<LayerGridAnalyzerCounters>,
    ) {
        let constraint_tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(GuardedPositiveItemIntersectionPropagatorArgs {
            name: "test-active-guarded-bridge-item-intersection".to_string(),
            relations: vec![GuardedPositiveItemRelation {
                guard,
                first: horizontal,
                second: Some(vertical),
                maximum_item_code,
                kind: GuardedPositiveItemRelationKind::Bridge,
            }],
            counters,
            constraint_tag,
        });
    }

    #[test]
    fn rejects_an_unresolved_guard_with_disjoint_positive_domains() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 2], "left");
        let right = solver.new_named_sparse_integer([1, 3], "right");
        let guard_literal = solver.new_literal_for_predicate(guard.equality_predicate(0), tag);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        add_propagator(&mut solver, guard, left, right, 3, Arc::clone(&counters));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.get_literal_value(guard_literal), Some(true));
        assert!(
            counters
                .snapshot()
                .guarded_intersection_forced_guard_rejections
                >= 1
        );
    }

    #[test]
    fn zero_only_overlap_rejects_the_transport_guard() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 2], "left");
        let right = solver.new_named_sparse_integer([0, 3], "right");
        let guard_literal = solver.new_literal_for_predicate(guard.equality_predicate(0), tag);
        add_propagator(
            &mut solver,
            guard,
            left,
            right,
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.get_literal_value(guard_literal), Some(true));
    }

    #[test]
    fn common_positive_item_preserves_the_guard() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 1, 2], "left");
        let right = solver.new_named_sparse_integer([1, 3], "right");
        add_propagator(
            &mut solver,
            guard,
            left,
            right,
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let guard_literal = solver.new_literal_for_predicate(guard.equality_predicate(1), tag);
        assert_eq!(solver.get_literal_value(guard_literal), None);
    }

    #[test]
    fn last_positive_support_removal_wakes_and_rejects_the_guard() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 1, 2], "left");
        let right = solver.new_named_sparse_integer([1, 3], "right");
        let guard_literal = solver.new_literal_for_predicate(guard.equality_predicate(0), tag);
        add_propagator(
            &mut solver,
            guard,
            left,
            right,
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );

        solver.add_clause([left.disequality_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.get_literal_value(guard_literal), Some(true));
    }

    #[test]
    fn selected_disjoint_relation_conflicts() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 2], "left");
        let right = solver.new_named_sparse_integer([0, 3], "right");
        solver.add_clause([guard.equality_predicate(1)], tag);
        add_propagator(
            &mut solver,
            guard,
            left,
            right,
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
    }

    #[test]
    fn selected_relation_with_positive_support_remains_feasible() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 1, 2], "left");
        let right = solver.new_named_sparse_integer([1, 3], "right");
        solver.add_clause([guard.equality_predicate(1)], tag);
        add_propagator(
            &mut solver,
            guard,
            left,
            right,
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
    }

    #[test]
    fn bridge_with_positive_support_on_both_axes_preserves_the_guard() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let west = solver.new_named_sparse_integer([0, 1, 2], "west");
        let east = solver.new_named_sparse_integer([1, 3], "east");
        let north = solver.new_named_sparse_integer([0, 2, 3], "north");
        let south = solver.new_named_sparse_integer([2, 4], "south");
        add_bridge_propagator(
            &mut solver,
            guard,
            GuardedPositiveItemPair {
                left: west,
                right: east,
            },
            GuardedPositiveItemPair {
                left: north,
                right: south,
            },
            4,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let selected = solver.new_literal_for_predicate(guard.equality_predicate(1), tag);
        assert_eq!(solver.get_literal_value(selected), None);
    }

    #[test]
    fn bridge_with_a_disjoint_horizontal_axis_is_rejected() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let west = solver.new_named_sparse_integer([0, 1], "west");
        let east = solver.new_named_sparse_integer([0, 2], "east");
        let north = solver.new_named_sparse_integer([0, 3], "north");
        let south = solver.new_named_sparse_integer([0, 3], "south");
        let rejected = solver.new_literal_for_predicate(guard.equality_predicate(0), tag);
        add_bridge_propagator(
            &mut solver,
            guard,
            GuardedPositiveItemPair {
                left: west,
                right: east,
            },
            GuardedPositiveItemPair {
                left: north,
                right: south,
            },
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.get_literal_value(rejected), Some(true));
    }

    #[test]
    fn bridge_with_a_disjoint_vertical_axis_is_rejected() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let west = solver.new_named_sparse_integer([0, 1], "west");
        let east = solver.new_named_sparse_integer([0, 1], "east");
        let north = solver.new_named_sparse_integer([0, 2], "north");
        let south = solver.new_named_sparse_integer([0, 3], "south");
        let rejected = solver.new_literal_for_predicate(guard.equality_predicate(0), tag);
        add_bridge_propagator(
            &mut solver,
            guard,
            GuardedPositiveItemPair {
                left: west,
                right: east,
            },
            GuardedPositiveItemPair {
                left: north,
                right: south,
            },
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.get_literal_value(rejected), Some(true));
    }

    #[test]
    fn last_vertical_bridge_support_removal_wakes_and_rejects_the_guard() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let west = solver.new_named_sparse_integer([0, 1], "west");
        let east = solver.new_named_sparse_integer([0, 1], "east");
        let north = solver.new_named_sparse_integer([0, 2, 3], "north");
        let south = solver.new_named_sparse_integer([0, 2], "south");
        let rejected = solver.new_literal_for_predicate(guard.equality_predicate(0), tag);
        add_bridge_propagator(
            &mut solver,
            guard,
            GuardedPositiveItemPair {
                left: west,
                right: east,
            },
            GuardedPositiveItemPair {
                left: north,
                right: south,
            },
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );

        solver.add_clause([north.disequality_predicate(2)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.get_literal_value(rejected), Some(true));
    }

    #[test]
    fn last_horizontal_bridge_support_removal_wakes_and_rejects_the_guard() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let west = solver.new_named_sparse_integer([0, 1, 2], "west");
        let east = solver.new_named_sparse_integer([0, 2], "east");
        let north = solver.new_named_sparse_integer([0, 3], "north");
        let south = solver.new_named_sparse_integer([0, 3], "south");
        let rejected = solver.new_literal_for_predicate(guard.equality_predicate(0), tag);
        add_bridge_propagator(
            &mut solver,
            guard,
            GuardedPositiveItemPair {
                left: west,
                right: east,
            },
            GuardedPositiveItemPair {
                left: north,
                right: south,
            },
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );

        solver.add_clause([west.disequality_predicate(2)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.get_literal_value(rejected), Some(true));
    }

    #[test]
    fn selected_bridge_with_a_disjoint_axis_conflicts() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let west = solver.new_named_sparse_integer([0, 1], "west");
        let east = solver.new_named_sparse_integer([0, 1], "east");
        let north = solver.new_named_sparse_integer([0, 2], "north");
        let south = solver.new_named_sparse_integer([0, 3], "south");
        solver.add_clause([guard.equality_predicate(1)], tag);
        add_bridge_propagator(
            &mut solver,
            guard,
            GuardedPositiveItemPair {
                left: west,
                right: east,
            },
            GuardedPositiveItemPair {
                left: north,
                right: south,
            },
            3,
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
    }

    #[test]
    fn conflict_backtracking_reenables_unprocessed_dirty_relations() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let shared = solver.new_named_sparse_integer([1, 2, 3], "shared");
        let first_guard = solver.new_bounded_integer(1, 1);
        let second_guard = solver.new_bounded_integer(1, 1);
        let first_right = solver.new_named_sparse_integer([1, 2], "first-right");
        let second_right = solver.new_named_sparse_integer([1, 3], "second-right");
        solver.add_clause([shared.disequality_predicate(1)], tag);
        let constraint_tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(GuardedPositiveItemIntersectionPropagatorArgs {
            name: "test-conflict-tail-restoration".to_string(),
            relations: vec![
                GuardedPositiveItemRelation {
                    guard: first_guard,
                    first: GuardedPositiveItemPair {
                        left: shared,
                        right: first_right,
                    },
                    second: None,
                    maximum_item_code: 3,
                    kind: GuardedPositiveItemRelationKind::RouteArc,
                },
                GuardedPositiveItemRelation {
                    guard: second_guard,
                    first: GuardedPositiveItemPair {
                        left: shared,
                        right: second_right,
                    },
                    second: None,
                    maximum_item_code: 3,
                    kind: GuardedPositiveItemRelationKind::RouteArc,
                },
            ],
            counters: Arc::new(LayerGridAnalyzerCounters::default()),
            constraint_tag,
        });

        let branchers: Vec<Box<dyn Brancher>> = vec![
            Box::new(WarmStart::new(&[shared], &[2])),
            Box::new(solver.default_brancher()),
        ];
        let mut brancher = DynamicBrancher::new(branchers);
        let mut resolver = ResolutionResolver::default();
        assert!(matches!(
            solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver),
            SatisfactionResult::Unsatisfiable(_, _, _)
        ));
    }
}
