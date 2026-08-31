use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use pumpkin_solver::core::declare_inference_label;
use pumpkin_solver::core::predicates::{PredicateConstructor, PropositionalConjunction};
use pumpkin_solver::core::proof::{ConstraintTag, InferenceCode};
use pumpkin_solver::core::propagation::{
    DomainEvents, EnqueueDecision, EventsToRegister, LocalId, NotificationContext, PredicateId,
    Priority, PropagationContext, Propagator, PropagatorConstructor, PropagatorConstructorContext,
    PropagatorSpec, ReadDomains, RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

declare_inference_label!(PossibleRouteReachability);

#[derive(Debug, Default)]
pub(super) struct PossibleRouteReachabilityCounters {
    propagations: AtomicU64,
    arcs_scanned: AtomicU64,
    demand_options_checked: AtomicU64,
    demand_pruning_attempts: AtomicU64,
    selected_demand_conflicts: AtomicU64,
    maximum_reason_predicates: AtomicU64,
    predicate_notifications: AtomicU64,
    registered_predicates: AtomicU64,
    reachability_arc_checks: AtomicU64,
    reason_builds: AtomicU64,
    reason_arc_scans: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct PossibleRouteReachabilityStatistics {
    pub propagations: u64,
    pub arcs_scanned: u64,
    pub demand_options_checked: u64,
    pub demand_pruning_attempts: u64,
    pub selected_demand_conflicts: u64,
    pub maximum_reason_predicates: u64,
    pub predicate_notifications: u64,
    pub registered_predicates: u64,
    pub reachability_arc_checks: u64,
    pub reason_builds: u64,
    pub reason_arc_scans: u64,
}

impl PossibleRouteReachabilityCounters {
    pub(super) fn snapshot(&self) -> PossibleRouteReachabilityStatistics {
        PossibleRouteReachabilityStatistics {
            propagations: self.propagations.load(Ordering::Relaxed),
            arcs_scanned: self.arcs_scanned.load(Ordering::Relaxed),
            demand_options_checked: self.demand_options_checked.load(Ordering::Relaxed),
            demand_pruning_attempts: self.demand_pruning_attempts.load(Ordering::Relaxed),
            selected_demand_conflicts: self.selected_demand_conflicts.load(Ordering::Relaxed),
            maximum_reason_predicates: self.maximum_reason_predicates.load(Ordering::Relaxed),
            predicate_notifications: self.predicate_notifications.load(Ordering::Relaxed),
            registered_predicates: self.registered_predicates.load(Ordering::Relaxed),
            reachability_arc_checks: self.reachability_arc_checks.load(Ordering::Relaxed),
            reason_builds: self.reason_builds.load(Ordering::Relaxed),
            reason_arc_scans: self.reason_arc_scans.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PossibleRouteReachabilityWakeMode {
    AnyDomainEvent,
    ExclusionPredicates,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PossibleRouteReachabilityTraversalMode {
    EagerAdjacencyAndReason,
    ReachableArcsAndLazyReason,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PossibleRouteArc {
    pub from: usize,
    pub to: usize,
    pub selected: DomainId,
    pub from_item: DomainId,
    pub to_item: DomainId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PossibleTerminalOption {
    pub cell: usize,
    pub selected: DomainId,
}

#[derive(Clone, Debug)]
pub(super) struct PossibleRouteReachabilityArgs {
    pub name: String,
    pub cell_count: usize,
    pub item_code: i32,
    pub arcs: Vec<PossibleRouteArc>,
    pub supplies: Vec<PossibleTerminalOption>,
    pub demands: Vec<PossibleTerminalOption>,
    pub constraint_tag: ConstraintTag,
    pub counters: Arc<PossibleRouteReachabilityCounters>,
    pub wake_mode: PossibleRouteReachabilityWakeMode,
    pub traversal_mode: PossibleRouteReachabilityTraversalMode,
}

impl PossibleRouteReachabilityArgs {
    pub(super) fn variables(&self) -> impl Iterator<Item = DomainId> + '_ {
        self.arcs
            .iter()
            .flat_map(|arc| [arc.selected, arc.from_item, arc.to_item])
            .chain(self.supplies.iter().map(|option| option.selected))
            .chain(self.demands.iter().map(|option| option.selected))
    }
}

impl PropagatorConstructor for PossibleRouteReachabilityArgs {
    type PropagatorImpl = PossibleRouteReachabilityPropagator;

    fn create(
        self,
        mut context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let registration = match self.wake_mode {
            PossibleRouteReachabilityWakeMode::AnyDomainEvent => {
                let variables = self.variables().collect::<BTreeSet<_>>();
                let mut variables = variables.into_iter();
                let first = variables
                    .next()
                    .expect("a route reachability propagator has terminals or arcs");
                let mut registration = EventsToRegister::builder()
                    .add(&first, DomainEvents::ANY_INT, LocalId::from(0))
                    .build();
                for (index, variable) in variables.enumerate() {
                    registration.add(
                        &variable,
                        DomainEvents::ANY_INT,
                        LocalId::from(
                            u32::try_from(index + 1).expect("propagator variable count fits u32"),
                        ),
                    );
                }
                registration
            }
            PossibleRouteReachabilityWakeMode::ExclusionPredicates => {
                let predicates = self
                    .arcs
                    .iter()
                    .flat_map(|arc| {
                        [
                            arc.selected.upper_bound_predicate(0),
                            arc.from_item.disequality_predicate(self.item_code),
                            arc.to_item.disequality_predicate(self.item_code),
                        ]
                    })
                    .chain(
                        self.supplies
                            .iter()
                            .map(|option| option.selected.upper_bound_predicate(0)),
                    )
                    .collect::<HashSet<_>>();
                self.counters.registered_predicates.fetch_add(
                    predicates.len().try_into().unwrap_or(u64::MAX),
                    Ordering::Relaxed,
                );
                for predicate in predicates {
                    context.register_predicate(predicate);
                }
                EventsToRegister::empty()
            }
        };

        let inference_code = InferenceCode::new(self.constraint_tag, PossibleRouteReachability);
        let mut outgoing_arc_indices = vec![Vec::new(); self.cell_count];
        for (index, arc) in self.arcs.iter().enumerate() {
            outgoing_arc_indices[arc.from].push(index);
        }
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: PossibleRouteReachabilityPropagator {
                name: self.name,
                cell_count: self.cell_count,
                item_code: self.item_code,
                arcs: self.arcs,
                outgoing_arc_indices,
                supplies: self.supplies,
                demands: self.demands,
                inference_code,
                counters: self.counters,
                traversal_mode: self.traversal_mode,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PossibleRouteReachabilityPropagator {
    name: String,
    cell_count: usize,
    item_code: i32,
    arcs: Vec<PossibleRouteArc>,
    outgoing_arc_indices: Vec<Vec<usize>>,
    supplies: Vec<PossibleTerminalOption>,
    demands: Vec<PossibleTerminalOption>,
    inference_code: InferenceCode,
    counters: Arc<PossibleRouteReachabilityCounters>,
    traversal_mode: PossibleRouteReachabilityTraversalMode,
}

impl PossibleRouteReachabilityPropagator {
    fn arc_is_possible(&self, context: &impl ReadDomains, arc: &PossibleRouteArc) -> bool {
        context.contains(&arc.selected, 1)
            && context.contains(&arc.from_item, self.item_code)
            && context.contains(&arc.to_item, self.item_code)
    }

    fn build_reason(&self, context: &impl ReadDomains) -> PropositionalConjunction {
        self.counters.reason_builds.fetch_add(1, Ordering::Relaxed);
        self.counters
            .reason_arc_scans
            .fetch_add(self.arcs.len() as u64, Ordering::Relaxed);
        let reason = self
            .arcs
            .iter()
            .filter_map(|arc| {
                if !context.contains(&arc.selected, 1) {
                    Some(arc.selected.upper_bound_predicate(0))
                } else if !context.contains(&arc.from_item, self.item_code) {
                    Some(arc.from_item.disequality_predicate(self.item_code))
                } else if !context.contains(&arc.to_item, self.item_code) {
                    Some(arc.to_item.disequality_predicate(self.item_code))
                } else {
                    None
                }
            })
            .chain(
                self.supplies
                    .iter()
                    .filter(|supply| !context.contains(&supply.selected, 1))
                    .map(|supply| supply.selected.upper_bound_predicate(0)),
            )
            .collect::<PropositionalConjunction>();
        self.counters.maximum_reason_predicates.fetch_max(
            reason.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        reason
    }

    fn post_unsupported_demands(
        &self,
        mut context: PropagationContext,
        reachable: &[bool],
        reason: &PropositionalConjunction,
    ) -> PropagationStatusCP {
        for demand in &self.demands {
            if !reachable[demand.cell] && context.contains(&demand.selected, 1) {
                self.counters
                    .demand_pruning_attempts
                    .fetch_add(1, Ordering::Relaxed);
                if context.lower_bound(&demand.selected) == 1 {
                    self.counters
                        .selected_demand_conflicts
                        .fetch_add(1, Ordering::Relaxed);
                }
                context.post(
                    demand.selected.upper_bound_predicate(0),
                    (reason.clone(), &self.inference_code),
                )?;
            }
        }
        Ok(())
    }

    fn propagate_eager(&self, context: PropagationContext) -> PropagationStatusCP {
        self.counters
            .arcs_scanned
            .fetch_add(self.arcs.len() as u64, Ordering::Relaxed);
        self.counters
            .reachability_arc_checks
            .fetch_add(self.arcs.len() as u64, Ordering::Relaxed);
        let mut adjacency = vec![Vec::new(); self.cell_count];
        let mut reason = Vec::new();

        for arc in &self.arcs {
            if !context.contains(&arc.selected, 1) {
                reason.push(arc.selected.upper_bound_predicate(0));
            } else if !context.contains(&arc.from_item, self.item_code) {
                reason.push(arc.from_item.disequality_predicate(self.item_code));
            } else if !context.contains(&arc.to_item, self.item_code) {
                reason.push(arc.to_item.disequality_predicate(self.item_code));
            } else {
                adjacency[arc.from].push(arc.to);
            }
        }
        for supply in &self.supplies {
            if !context.contains(&supply.selected, 1) {
                reason.push(supply.selected.upper_bound_predicate(0));
            }
        }
        self.counters.reason_builds.fetch_add(1, Ordering::Relaxed);
        self.counters
            .reason_arc_scans
            .fetch_add(self.arcs.len() as u64, Ordering::Relaxed);
        let reason = PropositionalConjunction::new(reason);
        self.counters.maximum_reason_predicates.fetch_max(
            reason.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        let mut reachable = vec![false; self.cell_count];
        let mut frontier = VecDeque::new();
        for supply in &self.supplies {
            if context.contains(&supply.selected, 1) && !reachable[supply.cell] {
                reachable[supply.cell] = true;
                frontier.push_back(supply.cell);
            }
        }
        while let Some(cell) = frontier.pop_front() {
            for &next in &adjacency[cell] {
                if !reachable[next] {
                    reachable[next] = true;
                    frontier.push_back(next);
                }
            }
        }

        self.post_unsupported_demands(context, &reachable, &reason)
    }

    fn propagate_lazy(&self, context: PropagationContext) -> PropagationStatusCP {
        let mut reachable = vec![false; self.cell_count];
        let mut frontier = VecDeque::new();
        for supply in &self.supplies {
            if context.contains(&supply.selected, 1) && !reachable[supply.cell] {
                reachable[supply.cell] = true;
                frontier.push_back(supply.cell);
            }
        }
        let mut reachability_arc_checks = 0_u64;
        while let Some(cell) = frontier.pop_front() {
            for &arc_index in &self.outgoing_arc_indices[cell] {
                reachability_arc_checks += 1;
                let arc = &self.arcs[arc_index];
                if self.arc_is_possible(&context, arc) && !reachable[arc.to] {
                    reachable[arc.to] = true;
                    frontier.push_back(arc.to);
                }
            }
        }
        self.counters
            .reachability_arc_checks
            .fetch_add(reachability_arc_checks, Ordering::Relaxed);
        self.counters
            .arcs_scanned
            .fetch_add(reachability_arc_checks, Ordering::Relaxed);

        if !self
            .demands
            .iter()
            .any(|demand| !reachable[demand.cell] && context.contains(&demand.selected, 1))
        {
            return Ok(());
        }

        self.counters
            .arcs_scanned
            .fetch_add(self.arcs.len() as u64, Ordering::Relaxed);
        let reason = self.build_reason(&context);
        self.post_unsupported_demands(context, &reachable, &reason)
    }
}

impl Propagator for PossibleRouteReachabilityPropagator {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::Low
    }

    fn notify_predicate_id_satisfied(
        &mut self,
        _context: NotificationContext,
        _predicate_id: PredicateId,
    ) -> EnqueueDecision {
        self.counters
            .predicate_notifications
            .fetch_add(1, Ordering::Relaxed);
        EnqueueDecision::Enqueue
    }

    fn propagate_from_scratch(&self, context: PropagationContext) -> PropagationStatusCP {
        self.counters.propagations.fetch_add(1, Ordering::Relaxed);
        self.counters
            .demand_options_checked
            .fetch_add(self.demands.len() as u64, Ordering::Relaxed);
        match self.traversal_mode {
            PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason => {
                self.propagate_eager(context)
            }
            PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason => {
                self.propagate_lazy(context)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::Solver;
    use pumpkin_solver::core::results::CSPSolverExecutionFlag;

    use super::*;

    fn add_reachability(
        solver: &mut Solver,
        arcs: Vec<PossibleRouteArc>,
        supply: DomainId,
        demand: DomainId,
    ) {
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(PossibleRouteReachabilityArgs {
            name: "controlled-possible-route".to_string(),
            cell_count: 3,
            item_code: 1,
            arcs,
            supplies: vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            demands: vec![PossibleTerminalOption {
                cell: 2,
                selected: demand,
            }],
            constraint_tag: tag,
            counters: Arc::new(PossibleRouteReachabilityCounters::default()),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
        });
    }

    #[test]
    fn retains_a_demand_with_one_possible_path() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        add_reachability(
            &mut solver,
            vec![
                PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected: first,
                    from_item: item,
                    to_item: item,
                },
                PossibleRouteArc {
                    from: 1,
                    to: 2,
                    selected: second,
                    from_item: item,
                    to_item: item,
                },
            ],
            supply,
            demand,
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&demand, 1));
    }

    #[test]
    fn removes_a_demand_after_every_path_is_excluded() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let blocked = solver.new_bounded_integer(0, 0);
        let item = solver.new_bounded_integer(1, 1);
        add_reachability(
            &mut solver,
            vec![PossibleRouteArc {
                from: 0,
                to: 2,
                selected: blocked,
                from_item: item,
                to_item: item,
            }],
            supply,
            demand,
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.upper_bound(&demand), 0);
    }

    #[test]
    fn conflicts_when_a_selected_demand_has_no_possible_path() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let blocked = solver.new_bounded_integer(0, 0);
        let item = solver.new_bounded_integer(1, 1);
        add_reachability(
            &mut solver,
            vec![PossibleRouteArc {
                from: 0,
                to: 2,
                selected: blocked,
                from_item: item,
                to_item: item,
            }],
            supply,
            demand,
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
    }

    #[test]
    fn item_exclusion_removes_the_only_material_path() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let selected = solver.new_bounded_integer(0, 1);
        let wrong_item = solver.new_bounded_integer(0, 0);
        let item = solver.new_bounded_integer(1, 1);
        add_reachability(
            &mut solver,
            vec![PossibleRouteArc {
                from: 0,
                to: 2,
                selected,
                from_item: item,
                to_item: wrong_item,
            }],
            supply,
            demand,
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.upper_bound(&demand), 0);
    }

    #[test]
    fn event_selective_mode_ignores_a_path_preserving_assignment() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let selected = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(PossibleRouteReachabilityCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(PossibleRouteReachabilityArgs {
            name: "event-selective-path-preserving".to_string(),
            cell_count: 3,
            item_code: 1,
            arcs: vec![PossibleRouteArc {
                from: 0,
                to: 2,
                selected,
                from_item: item,
                to_item: item,
            }],
            supplies: vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            demands: vec![PossibleTerminalOption {
                cell: 2,
                selected: demand,
            }],
            constraint_tag: tag,
            counters: Arc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::ExclusionPredicates,
            traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let after_initial = counters.snapshot();
        solver.add_clause([selected.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let after_assignment = counters.snapshot();

        assert_eq!(after_assignment.propagations, after_initial.propagations);
        assert_eq!(after_assignment.predicate_notifications, 0);
        assert!(solver.contains(&demand, 1));
    }

    #[test]
    fn event_selective_mode_wakes_when_the_only_path_disappears() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let selected = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(PossibleRouteReachabilityCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(PossibleRouteReachabilityArgs {
            name: "event-selective-path-removal".to_string(),
            cell_count: 3,
            item_code: 1,
            arcs: vec![PossibleRouteArc {
                from: 0,
                to: 2,
                selected,
                from_item: item,
                to_item: item,
            }],
            supplies: vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            demands: vec![PossibleTerminalOption {
                cell: 2,
                selected: demand,
            }],
            constraint_tag: tag,
            counters: Arc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::ExclusionPredicates,
            traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        solver.add_clause([selected.upper_bound_predicate(0)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();

        assert!(statistics.predicate_notifications >= 1);
        assert_eq!(solver.upper_bound(&demand), 0);
    }

    #[test]
    fn lazy_mode_builds_a_reason_only_when_a_demand_is_unsupported() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let selected = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(PossibleRouteReachabilityCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(PossibleRouteReachabilityArgs {
            name: "lazy-reason-construction".to_string(),
            cell_count: 3,
            item_code: 1,
            arcs: vec![PossibleRouteArc {
                from: 0,
                to: 2,
                selected,
                from_item: item,
                to_item: item,
            }],
            supplies: vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            demands: vec![PossibleTerminalOption {
                cell: 2,
                selected: demand,
            }],
            constraint_tag: tag,
            counters: Arc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let reachable = counters.snapshot();
        assert_eq!(reachable.reason_builds, 0);
        assert_eq!(reachable.reason_arc_scans, 0);
        assert!(solver.contains(&demand, 1));

        solver.add_clause([selected.upper_bound_predicate(0)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let blocked = counters.snapshot();
        assert!(blocked.reason_builds >= 1);
        assert!(blocked.reason_arc_scans >= 1);
        assert_eq!(solver.upper_bound(&demand), 0);
    }
}
