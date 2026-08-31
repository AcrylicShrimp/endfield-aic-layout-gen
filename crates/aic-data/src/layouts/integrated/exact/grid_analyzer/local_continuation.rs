use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use pumpkin_solver::core::propagation::{
    DomainEvents, EventsToRegister, LocalId, Priority, PropagationContext, Propagator,
    PropagatorConstructor, PropagatorConstructorContext, PropagatorSpec, ReadDomains,
    RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;

use super::{LayerGridMaterial, LayerGridRuleArgs};
use crate::layouts::integrated::exact::connectivity_propagator::{
    PossibleRouteArc, PossibleTerminalOption,
};

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct LocalPositiveFlowContinuationAnalyzerArgs {
    pub rule: LayerGridRuleArgs,
    pub bridge_selected_by_cell: Vec<Option<pumpkin_solver::core::variables::DomainId>>,
}

impl PropagatorConstructor for LocalPositiveFlowContinuationAnalyzerArgs {
    type PropagatorImpl = LocalPositiveFlowContinuationAnalyzer;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let variables = self
            .rule
            .variables()
            .chain(self.bridge_selected_by_cell.iter().flatten().copied())
            .collect::<BTreeSet<_>>();
        self.rule
            .counters
            .local_registered_domain_variables
            .fetch_add(
                variables.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        let mut variables = variables.into_iter();
        let first = variables
            .next()
            .expect("a local continuation analyzer has terminals or arcs");
        let mut registration = EventsToRegister::builder()
            .add(&first, DomainEvents::ANY_INT, LocalId::from(0))
            .build();
        for (index, variable) in variables.enumerate() {
            registration.add(
                &variable,
                DomainEvents::ANY_INT,
                LocalId::from(
                    u32::try_from(index + 1).expect("local continuation variable count fits u32"),
                ),
            );
        }

        let mut incoming = vec![Vec::new(); self.rule.cell_count];
        let mut outgoing = vec![Vec::new(); self.rule.cell_count];
        for arc in &self.rule.arcs {
            incoming[arc.to].push(*arc);
            outgoing[arc.from].push(*arc);
        }
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: LocalPositiveFlowContinuationAnalyzer {
                name: format!("{}-local-positive-flow-continuation", self.rule.name),
                cell_count: self.rule.cell_count,
                incoming,
                outgoing,
                materials: self.rule.materials,
                bridge_selected_by_cell: self.bridge_selected_by_cell,
                counters: self.rule.counters,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct LocalPositiveFlowContinuationAnalyzer {
    name: String,
    cell_count: usize,
    incoming: Vec<Vec<PossibleRouteArc>>,
    outgoing: Vec<Vec<PossibleRouteArc>>,
    materials: Vec<LayerGridMaterial>,
    bridge_selected_by_cell: Vec<Option<pumpkin_solver::core::variables::DomainId>>,
    counters: std::sync::Arc<super::LayerGridAnalyzerCounters>,
}

impl LocalPositiveFlowContinuationAnalyzer {
    fn arc_is_possible(context: &impl ReadDomains, arc: &PossibleRouteArc, item_code: i32) -> bool {
        context.contains(&arc.selected, 1)
            && context.contains(&arc.from_item, item_code)
            && context.contains(&arc.to_item, item_code)
    }

    fn arc_is_positive_material_witness(
        context: &impl ReadDomains,
        arc: &PossibleRouteArc,
        item_code: i32,
    ) -> bool {
        context.lower_bound(&arc.selected) == 1
            && context.lower_bound(&arc.from_item) == item_code
            && context.upper_bound(&arc.from_item) == item_code
            && context.lower_bound(&arc.to_item) == item_code
            && context.upper_bound(&arc.to_item) == item_code
    }

    fn terminal_is_positive(context: &impl ReadDomains, terminal: &PossibleTerminalOption) -> bool {
        context.lower_bound(&terminal.selected) == 1
    }

    fn terminal_is_possible(context: &impl ReadDomains, terminal: &PossibleTerminalOption) -> bool {
        context.contains(&terminal.selected, 1)
    }

    fn unresolved_support_predicates(
        context: &impl ReadDomains,
        arc: &PossibleRouteArc,
        item_code: i32,
    ) -> u64 {
        [
            (arc.selected, 1),
            (arc.from_item, item_code),
            (arc.to_item, item_code),
        ]
        .into_iter()
        .filter(|(variable, value)| {
            context.lower_bound(variable) != *value || context.upper_bound(variable) != *value
        })
        .count() as u64
    }

    fn exclusion_reason_size(
        context: &impl ReadDomains,
        alternatives: &[PossibleRouteArc],
        terminals: &[PossibleTerminalOption],
        item_code: i32,
    ) -> u64 {
        let arc_predicates = alternatives
            .iter()
            .filter(|arc| !Self::arc_is_possible(context, arc, item_code))
            .count() as u64;
        let terminal_predicates = terminals
            .iter()
            .filter(|terminal| !Self::terminal_is_possible(context, terminal))
            .count() as u64;
        arc_predicates + terminal_predicates
    }

    fn analyze_material(&self, context: &impl ReadDomains, material: &LayerGridMaterial) {
        self.counters
            .local_continuation_material_passes
            .fetch_add(1, Ordering::Relaxed);
        for cell in 0..self.cell_count {
            if self.bridge_selected_by_cell[cell]
                .is_some_and(|selected| context.contains(&selected, 1))
            {
                self.counters
                    .local_bridge_possible_cell_skips
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let bridge_exclusion_size = u64::from(self.bridge_selected_by_cell[cell].is_some());
            let supply_witness = material.supplies.iter().any(|terminal| {
                terminal.cell == cell && Self::terminal_is_positive(context, terminal)
            });
            let incoming_arc_witness = self.incoming[cell].iter().any(|arc| {
                Self::arc_is_positive_material_witness(context, arc, material.item_code)
            });
            if supply_witness || incoming_arc_witness {
                self.counters
                    .local_positive_inflow_cells
                    .fetch_add(1, Ordering::Relaxed);
                let possible_demands = material
                    .demands
                    .iter()
                    .filter(|terminal| terminal.cell == cell)
                    .copied()
                    .collect::<Vec<_>>();
                if !possible_demands
                    .iter()
                    .any(|terminal| Self::terminal_is_possible(context, terminal))
                {
                    self.counters
                        .local_forward_continuation_cells
                        .fetch_add(1, Ordering::Relaxed);
                    let possible = self.outgoing[cell]
                        .iter()
                        .filter(|arc| Self::arc_is_possible(context, arc, material.item_code))
                        .collect::<Vec<_>>();
                    match possible.as_slice() {
                        [] => {
                            self.counters
                                .local_forward_zero_supports
                                .fetch_add(1, Ordering::Relaxed);
                            let witness_size = if supply_witness { 1 } else { 3 };
                            let reason_size = witness_size
                                + bridge_exclusion_size
                                + Self::exclusion_reason_size(
                                    context,
                                    &self.outgoing[cell],
                                    &possible_demands,
                                    material.item_code,
                                );
                            self.counters
                                .local_maximum_reason_predicates
                                .fetch_max(reason_size, Ordering::Relaxed);
                        }
                        [support] => {
                            self.counters
                                .local_forward_unique_supports
                                .fetch_add(1, Ordering::Relaxed);
                            let unresolved = Self::unresolved_support_predicates(
                                context,
                                support,
                                material.item_code,
                            );
                            if unresolved > 0 {
                                self.counters
                                    .distinct_local_forward_support_arcs
                                    .lock()
                                    .expect("local forward support-arc counter is not poisoned")
                                    .insert((material.item_code, support.selected));
                            }
                            self.counters
                                .local_forward_unresolved_predicates
                                .fetch_add(unresolved, Ordering::Relaxed);
                            let witness_size = if supply_witness { 1 } else { 3 };
                            let reason_size = witness_size
                                + bridge_exclusion_size
                                + Self::exclusion_reason_size(
                                    context,
                                    &self.outgoing[cell],
                                    &possible_demands,
                                    material.item_code,
                                );
                            self.counters
                                .local_maximum_reason_predicates
                                .fetch_max(reason_size, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                }
            }

            let demand_witness = material.demands.iter().any(|terminal| {
                terminal.cell == cell && Self::terminal_is_positive(context, terminal)
            });
            let outgoing_arc_witness = self.outgoing[cell].iter().any(|arc| {
                Self::arc_is_positive_material_witness(context, arc, material.item_code)
            });
            if demand_witness || outgoing_arc_witness {
                self.counters
                    .local_positive_outflow_cells
                    .fetch_add(1, Ordering::Relaxed);
                let possible_supplies = material
                    .supplies
                    .iter()
                    .filter(|terminal| terminal.cell == cell)
                    .copied()
                    .collect::<Vec<_>>();
                if !possible_supplies
                    .iter()
                    .any(|terminal| Self::terminal_is_possible(context, terminal))
                {
                    self.counters
                        .local_backward_continuation_cells
                        .fetch_add(1, Ordering::Relaxed);
                    let possible = self.incoming[cell]
                        .iter()
                        .filter(|arc| Self::arc_is_possible(context, arc, material.item_code))
                        .collect::<Vec<_>>();
                    match possible.as_slice() {
                        [] => {
                            self.counters
                                .local_backward_zero_supports
                                .fetch_add(1, Ordering::Relaxed);
                            let witness_size = if demand_witness { 1 } else { 3 };
                            let reason_size = witness_size
                                + bridge_exclusion_size
                                + Self::exclusion_reason_size(
                                    context,
                                    &self.incoming[cell],
                                    &possible_supplies,
                                    material.item_code,
                                );
                            self.counters
                                .local_maximum_reason_predicates
                                .fetch_max(reason_size, Ordering::Relaxed);
                        }
                        [support] => {
                            self.counters
                                .local_backward_unique_supports
                                .fetch_add(1, Ordering::Relaxed);
                            let unresolved = Self::unresolved_support_predicates(
                                context,
                                support,
                                material.item_code,
                            );
                            if unresolved > 0 {
                                self.counters
                                    .distinct_local_backward_support_arcs
                                    .lock()
                                    .expect("local backward support-arc counter is not poisoned")
                                    .insert((material.item_code, support.selected));
                            }
                            self.counters
                                .local_backward_unresolved_predicates
                                .fetch_add(unresolved, Ordering::Relaxed);
                            let witness_size = if demand_witness { 1 } else { 3 };
                            let reason_size = witness_size
                                + bridge_exclusion_size
                                + Self::exclusion_reason_size(
                                    context,
                                    &self.incoming[cell],
                                    &possible_supplies,
                                    material.item_code,
                                );
                            self.counters
                                .local_maximum_reason_predicates
                                .fetch_max(reason_size, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

impl Propagator for LocalPositiveFlowContinuationAnalyzer {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::VeryLow
    }

    fn propagate_from_scratch(&self, context: PropagationContext) -> PropagationStatusCP {
        self.counters
            .local_continuation_executions
            .fetch_add(1, Ordering::Relaxed);
        for material in &self.materials {
            self.analyze_material(&context, material);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pumpkin_solver::Solver;
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::results::CSPSolverExecutionFlag;

    use super::*;
    use crate::layouts::integrated::exact::grid_analyzer::LayerGridAnalyzerCounters;

    #[test]
    fn observes_a_supply_rooted_forward_continuation_without_forcing_it() {
        let mut solver = Solver::default();
        let outgoing = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationAnalyzerArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-forward-continuation".to_string(),
                cell_count: 2,
                arcs: vec![PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected: outgoing,
                    from_item: item,
                    to_item: item,
                }],
                materials: vec![LayerGridMaterial {
                    item_code: 1,
                    supplies: vec![PossibleTerminalOption {
                        cell: 0,
                        selected: supply,
                    }],
                    demands: vec![],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            bridge_selected_by_cell: vec![None; 2],
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&outgoing, 0));
        let statistics = counters.snapshot();
        assert!(statistics.local_forward_unique_supports >= 1);
        assert_eq!(statistics.distinct_local_forward_support_arcs, 1);
        assert!(statistics.local_forward_unresolved_predicates >= 1);
    }

    #[test]
    fn observes_a_demand_rooted_backward_continuation_without_forcing_it() {
        let mut solver = Solver::default();
        let incoming = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationAnalyzerArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-backward-continuation".to_string(),
                cell_count: 2,
                arcs: vec![PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected: incoming,
                    from_item: item,
                    to_item: item,
                }],
                materials: vec![LayerGridMaterial {
                    item_code: 1,
                    supplies: vec![],
                    demands: vec![PossibleTerminalOption {
                        cell: 1,
                        selected: demand,
                    }],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            bridge_selected_by_cell: vec![None; 2],
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&incoming, 0));
        let statistics = counters.snapshot();
        assert!(statistics.local_backward_unique_supports >= 1);
        assert_eq!(statistics.distinct_local_backward_support_arcs, 1);
    }

    #[test]
    fn observes_arc_rooted_continuation_and_branch_or_zero_stops() {
        let mut solver = Solver::default();
        let incoming = solver.new_bounded_integer(1, 1);
        let first_outgoing = solver.new_bounded_integer(0, 1);
        let second_outgoing = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationAnalyzerArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-arc-rooted-branch".to_string(),
                cell_count: 4,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 1,
                        selected: incoming,
                        from_item: item,
                        to_item: item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 2,
                        selected: first_outgoing,
                        from_item: item,
                        to_item: item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 3,
                        selected: second_outgoing,
                        from_item: item,
                        to_item: item,
                    },
                ],
                materials: vec![LayerGridMaterial {
                    item_code: 1,
                    supplies: vec![],
                    demands: vec![],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            bridge_selected_by_cell: vec![None; 4],
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert!(statistics.local_forward_continuation_cells >= 1);
        assert_eq!(statistics.local_forward_unique_supports, 0);
        assert!(statistics.local_backward_zero_supports >= 1);
        assert!(solver.contains(&first_outgoing, 0));
        assert!(solver.contains(&second_outgoing, 0));
    }

    #[test]
    fn observes_a_selected_circulation_without_rejecting_it() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(1, 1);
        let second = solver.new_bounded_integer(1, 1);
        let item = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationAnalyzerArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-local-circulation".to_string(),
                cell_count: 2,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 1,
                        selected: first,
                        from_item: item,
                        to_item: item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 0,
                        selected: second,
                        from_item: item,
                        to_item: item,
                    },
                ],
                materials: vec![LayerGridMaterial {
                    item_code: 1,
                    supplies: vec![],
                    demands: vec![],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            bridge_selected_by_cell: vec![None; 2],
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert!(statistics.local_forward_unique_supports >= 2);
        assert!(statistics.local_backward_unique_supports >= 2);
        assert_eq!(statistics.local_forward_unresolved_predicates, 0);
        assert_eq!(statistics.local_backward_unresolved_predicates, 0);
    }

    #[test]
    fn skips_a_cell_while_a_same_layer_bridge_remains_possible() {
        let mut solver = Solver::default();
        let outgoing = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let bridge = solver.new_bounded_integer(0, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationAnalyzerArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-bridge-skip".to_string(),
                cell_count: 2,
                arcs: vec![PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected: outgoing,
                    from_item: item,
                    to_item: item,
                }],
                materials: vec![LayerGridMaterial {
                    item_code: 1,
                    supplies: vec![PossibleTerminalOption {
                        cell: 0,
                        selected: supply,
                    }],
                    demands: vec![],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            bridge_selected_by_cell: vec![Some(bridge), None],
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert!(statistics.local_bridge_possible_cell_skips >= 1);
        assert_eq!(statistics.local_forward_unique_supports, 0);
        assert!(solver.contains(&outgoing, 0));

        solver.add_clause([bridge.upper_bound_predicate(0)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert_eq!(statistics.distinct_local_forward_support_arcs, 1);
        assert!(statistics.local_forward_unique_supports >= 1);
    }

    #[test]
    fn counts_a_proven_bridge_exclusion_in_the_reason_estimate() {
        let mut solver = Solver::default();
        let outgoing = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let bridge = solver.new_bounded_integer(0, 0);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationAnalyzerArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-bridge-exclusion-reason".to_string(),
                cell_count: 2,
                arcs: vec![PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected: outgoing,
                    from_item: item,
                    to_item: item,
                }],
                materials: vec![LayerGridMaterial {
                    item_code: 1,
                    supplies: vec![PossibleTerminalOption {
                        cell: 0,
                        selected: supply,
                    }],
                    demands: vec![],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            bridge_selected_by_cell: vec![Some(bridge), None],
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert_eq!(statistics.distinct_local_forward_support_arcs, 1);
        assert!(statistics.local_maximum_reason_predicates >= 2);
        assert!(solver.contains(&outgoing, 0));
    }
}
