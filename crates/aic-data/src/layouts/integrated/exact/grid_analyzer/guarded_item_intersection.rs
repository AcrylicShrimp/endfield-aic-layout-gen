use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::propagation::{
    DomainEvents, EnqueueDecision, EventsToRegister, LocalId, NotificationContext,
    OpaqueDomainEvent, Priority, PropagationContext, Propagator, PropagatorConstructor,
    PropagatorConstructorContext, PropagatorSpec, ReadDomains, RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

use super::LayerGridAnalyzerCounters;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::layouts::integrated::exact) enum GuardedItemEqualityKind {
    RouteArc,
    BridgeAxis,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::layouts::integrated::exact) struct GuardedItemEquality {
    pub guard: DomainId,
    pub left: DomainId,
    pub right: DomainId,
    pub maximum_item_code: i32,
    pub kind: GuardedItemEqualityKind,
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct GuardedItemIntersectionObserverArgs {
    pub name: String,
    pub relations: Vec<GuardedItemEquality>,
    pub counters: Arc<LayerGridAnalyzerCounters>,
}

impl PropagatorConstructor for GuardedItemIntersectionObserverArgs {
    type PropagatorImpl = GuardedItemIntersectionObserver;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let mut impacts = BTreeMap::<DomainId, BTreeSet<usize>>::new();
        for (relation_index, relation) in self.relations.iter().enumerate() {
            for variable in [relation.guard, relation.left, relation.right] {
                impacts.entry(variable).or_default().insert(relation_index);
            }
        }
        let mut entries = impacts.into_iter();
        let (first_variable, first_impacts) = entries
            .next()
            .expect("a guarded item intersection observer has at least one relation");
        let mut event_impacts = vec![first_impacts.into_iter().collect::<Vec<_>>()];
        let mut registration = EventsToRegister::builder()
            .add(&first_variable, DomainEvents::ANY_INT, LocalId::from(0))
            .build();
        for (index, (variable, relation_indices)) in entries.enumerate() {
            registration.add(
                &variable,
                DomainEvents::ANY_INT,
                LocalId::from(u32::try_from(index + 1).expect("observer variable count fits u32")),
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
            propagator: GuardedItemIntersectionObserver {
                name: self.name,
                relations: self.relations,
                counters: self.counters,
                event_impacts,
                dirty_relations: (0..relation_count).collect(),
                relation_is_dirty: vec![true; relation_count],
                ever_disjoint: vec![false; relation_count],
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct GuardedItemIntersectionObserver {
    name: String,
    relations: Vec<GuardedItemEquality>,
    counters: Arc<LayerGridAnalyzerCounters>,
    event_impacts: Vec<Vec<usize>>,
    dirty_relations: Vec<usize>,
    relation_is_dirty: Vec<bool>,
    ever_disjoint: Vec<bool>,
}

impl GuardedItemIntersectionObserver {
    fn has_common_support(
        &self,
        context: &impl ReadDomains,
        relation: GuardedItemEquality,
    ) -> bool {
        let mut membership_checks = 0;
        // A selected route arc activates both endpoint arms, and a selected bridge activates
        // every axis arm. The arm-item table admits zero only when both arm presences are false,
        // so zero is not a support for either composite guarded relation observed here.
        for item_code in 1..=relation.maximum_item_code {
            membership_checks += 1;
            if !context.contains(&relation.left, item_code) {
                continue;
            }
            membership_checks += 1;
            if context.contains(&relation.right, item_code) {
                self.counters
                    .guarded_intersection_membership_checks
                    .fetch_add(membership_checks, Ordering::Relaxed);
                return true;
            }
        }
        self.counters
            .guarded_intersection_membership_checks
            .fetch_add(membership_checks, Ordering::Relaxed);
        false
    }

    fn observe_relation(&mut self, context: &impl ReadDomains, relation_index: usize) {
        let relation = self.relations[relation_index];
        self.counters
            .guarded_intersection_relation_checks
            .fetch_add(1, Ordering::Relaxed);
        match context.evaluate_predicate(relation.guard.equality_predicate(1)) {
            Some(false) => {}
            Some(true) => {}
            None => {
                self.counters
                    .guarded_intersection_unresolved_guard_checks
                    .fetch_add(1, Ordering::Relaxed);
                if self.has_common_support(context, relation) {
                    self.counters
                        .guarded_intersection_supported_checks
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
                self.counters
                    .guarded_intersection_disjoint_checks
                    .fetch_add(1, Ordering::Relaxed);
                if !self.ever_disjoint[relation_index] {
                    self.ever_disjoint[relation_index] = true;
                    self.counters
                        .guarded_intersection_unique_disjoint_relations
                        .fetch_add(1, Ordering::Relaxed);
                    match relation.kind {
                        GuardedItemEqualityKind::RouteArc => {
                            self.counters
                                .guarded_intersection_unique_disjoint_route_arcs
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        GuardedItemEqualityKind::BridgeAxis => {
                            self.counters
                                .guarded_intersection_unique_disjoint_bridge_axes
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }
}

impl Propagator for GuardedItemIntersectionObserver {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::Low
    }

    fn notify(
        &mut self,
        _context: NotificationContext,
        local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        self.counters
            .guarded_intersection_notifications
            .fetch_add(1, Ordering::Relaxed);
        let impacts = &self.event_impacts[local_id.unpack() as usize];
        for relation_index in impacts {
            if !self.relation_is_dirty[*relation_index] {
                self.relation_is_dirty[*relation_index] = true;
                self.dirty_relations.push(*relation_index);
            }
        }
        EnqueueDecision::Enqueue
    }

    fn propagate(&mut self, context: PropagationContext) -> PropagationStatusCP {
        self.counters
            .guarded_intersection_executions
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .guarded_intersection_maximum_dirty_relations
            .fetch_max(
                self.dirty_relations.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        let dirty_relations = std::mem::take(&mut self.dirty_relations);
        for relation_index in dirty_relations {
            self.relation_is_dirty[relation_index] = false;
            self.observe_relation(&context, relation_index);
        }
        Ok(())
    }

    fn propagate_from_scratch(&self, _context: PropagationContext) -> PropagationStatusCP {
        Ok(())
    }

    fn synchronise(&mut self, _context: NotificationContext<'_>) {
        // Backtracking only restores removed values, so it cannot create a new empty domain
        // intersection. Pending forward-event work belongs to the abandoned branch and must be
        // discarded. Future shrinking events will dirty only their affected relations again.
        for relation_index in self.dirty_relations.drain(..) {
            self.relation_is_dirty[relation_index] = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pumpkin_solver::Solver;
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::results::CSPSolverExecutionFlag;

    use super::*;

    fn propagate(solver: &mut Solver) {
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
    }

    #[test]
    fn observes_disjoint_interior_domains_without_pruning_the_guard() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 2], "left");
        let right = solver.new_named_sparse_integer([1, 3], "right");
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let _ = solver.add_propagator(GuardedItemIntersectionObserverArgs {
            name: "test-observer".to_string(),
            relations: vec![GuardedItemEquality {
                guard,
                left,
                right,
                maximum_item_code: 3,
                kind: GuardedItemEqualityKind::RouteArc,
            }],
            counters: Arc::clone(&counters),
        });

        propagate(&mut solver);

        let guard_literal = solver.new_literal_for_predicate(guard.equality_predicate(1), tag);
        assert_eq!(solver.get_literal_value(guard_literal), None);
        let statistics = counters.snapshot();
        assert_eq!(statistics.guarded_intersection_unique_disjoint_relations, 1);
        assert_eq!(
            statistics.guarded_intersection_unique_disjoint_route_arcs,
            1
        );
    }

    #[test]
    fn zero_is_not_support_for_an_active_transport_relation() {
        let mut solver = Solver::default();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 2], "left");
        let right = solver.new_named_sparse_integer([0, 3], "right");
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let _ = solver.add_propagator(GuardedItemIntersectionObserverArgs {
            name: "test-zero-support-observer".to_string(),
            relations: vec![GuardedItemEquality {
                guard,
                left,
                right,
                maximum_item_code: 3,
                kind: GuardedItemEqualityKind::BridgeAxis,
            }],
            counters: Arc::clone(&counters),
        });

        propagate(&mut solver);

        let statistics = counters.snapshot();
        assert_eq!(statistics.guarded_intersection_unique_disjoint_relations, 1);
        assert!(statistics.guarded_intersection_disjoint_checks >= 1);
    }

    #[test]
    fn interior_support_removal_wakes_the_observer() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let guard = solver.new_named_bounded_integer(0, 1, "guard");
        let left = solver.new_named_sparse_integer([0, 1, 2], "left");
        let right = solver.new_named_sparse_integer([1, 3], "right");
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let _ = solver.add_propagator(GuardedItemIntersectionObserverArgs {
            name: "test-hole-observer".to_string(),
            relations: vec![GuardedItemEquality {
                guard,
                left,
                right,
                maximum_item_code: 3,
                kind: GuardedItemEqualityKind::RouteArc,
            }],
            counters: Arc::clone(&counters),
        });
        propagate(&mut solver);
        assert_eq!(
            counters
                .snapshot()
                .guarded_intersection_unique_disjoint_relations,
            0
        );

        solver.add_clause([left.disequality_predicate(1)], tag);
        propagate(&mut solver);

        assert_eq!(
            counters
                .snapshot()
                .guarded_intersection_unique_disjoint_relations,
            1
        );
    }
}
