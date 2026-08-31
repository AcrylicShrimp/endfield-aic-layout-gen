use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;

use pumpkin_solver::core::declare_inference_label;
use pumpkin_solver::core::predicates::{Predicate, PredicateConstructor, PropositionalConjunction};
use pumpkin_solver::core::proof::InferenceCode;
use pumpkin_solver::core::propagation::{
    DomainEvents, EnqueueDecision, EventsToRegister, LocalId, NotificationContext,
    OpaqueDomainEvent, Priority, PropagationContext, Propagator, PropagatorConstructor,
    PropagatorConstructorContext, PropagatorSpec, ReadDomains, RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

use super::local_continuation::LocalPositiveFlowContinuationAnalyzerArgs;
use super::{LayerGridMaterial, LayerGridRuleArgs};
use crate::layouts::integrated::exact::connectivity_propagator::{
    PossibleRouteArc, PossibleTerminalOption,
};

declare_inference_label!(LocalPositiveFlowContinuation);

type DirtyKey = (usize, usize);

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct LocalPositiveFlowContinuationPropagatorArgs(
    pub LocalPositiveFlowContinuationAnalyzerArgs,
);

impl PropagatorConstructor for LocalPositiveFlowContinuationPropagatorArgs {
    type PropagatorImpl = LocalPositiveFlowContinuationPropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let LocalPositiveFlowContinuationAnalyzerArgs {
            rule,
            bridge_selected_by_cell,
        } = self.0;
        let (registration, event_impacts) = event_registration(&rule, &bridge_selected_by_cell);
        let mut incoming_arc_indices = vec![Vec::new(); rule.cell_count];
        let mut outgoing_arc_indices = vec![Vec::new(); rule.cell_count];
        for (arc_index, arc) in rule.arcs.iter().enumerate() {
            incoming_arc_indices[arc.to].push(arc_index);
            outgoing_arc_indices[arc.from].push(arc_index);
        }
        let dirty_keys = (0..rule.materials.len())
            .flat_map(|material_index| (0..rule.cell_count).map(move |cell| (material_index, cell)))
            .collect();
        let inference_code = InferenceCode::new(rule.constraint_tag, LocalPositiveFlowContinuation);
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: LocalPositiveFlowContinuationPropagator {
                name: format!("{}-active-local-positive-flow-continuation", rule.name),
                arcs: rule.arcs,
                incoming_arc_indices,
                outgoing_arc_indices,
                materials: rule.materials,
                bridge_selected_by_cell,
                counters: rule.counters,
                event_impacts,
                dirty_keys,
                inference_code,
            },
        }
    }
}

fn event_registration(
    rule: &LayerGridRuleArgs,
    bridges: &[Option<DomainId>],
) -> (EventsToRegister, Vec<Vec<DirtyKey>>) {
    let mut impacts = BTreeMap::<DomainId, BTreeSet<DirtyKey>>::new();
    let all_materials = 0..rule.materials.len();
    for arc in &rule.arcs {
        for variable in [arc.selected, arc.from_item, arc.to_item] {
            for material_index in all_materials.clone() {
                impacts
                    .entry(variable)
                    .or_default()
                    .extend([(material_index, arc.from), (material_index, arc.to)]);
            }
        }
    }
    for (material_index, material) in rule.materials.iter().enumerate() {
        for terminal in material.supplies.iter().chain(&material.demands) {
            impacts
                .entry(terminal.selected)
                .or_default()
                .insert((material_index, terminal.cell));
        }
    }
    for (cell, bridge) in bridges.iter().enumerate() {
        let Some(bridge) = bridge else {
            continue;
        };
        for material_index in all_materials.clone() {
            impacts
                .entry(*bridge)
                .or_default()
                .insert((material_index, cell));
        }
    }

    let mut entries = impacts.into_iter();
    let (first_variable, first_impacts) = entries
        .next()
        .expect("an active local continuation rule has terminals or arcs");
    let mut event_impacts = vec![first_impacts.into_iter().collect::<Vec<_>>()];
    let mut registration = EventsToRegister::builder()
        .add(&first_variable, DomainEvents::ANY_INT, LocalId::from(0))
        .build();
    for (index, (variable, variable_impacts)) in entries.enumerate() {
        registration.add(
            &variable,
            DomainEvents::ANY_INT,
            LocalId::from(
                u32::try_from(index + 1).expect("local continuation variable count fits u32"),
            ),
        );
        event_impacts.push(variable_impacts.into_iter().collect());
    }
    rule.counters.local_registered_domain_variables.fetch_add(
        event_impacts.len().try_into().unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    (registration, event_impacts)
}

#[derive(Clone, Copy, Debug)]
enum PositiveWitness {
    Terminal(DomainId),
    Arc(usize),
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated::exact) struct LocalPositiveFlowContinuationPropagator {
    name: String,
    arcs: Vec<PossibleRouteArc>,
    incoming_arc_indices: Vec<Vec<usize>>,
    outgoing_arc_indices: Vec<Vec<usize>>,
    materials: Vec<LayerGridMaterial>,
    bridge_selected_by_cell: Vec<Option<DomainId>>,
    counters: std::sync::Arc<super::LayerGridAnalyzerCounters>,
    event_impacts: Vec<Vec<DirtyKey>>,
    dirty_keys: BTreeSet<DirtyKey>,
    inference_code: InferenceCode,
}

impl LocalPositiveFlowContinuationPropagator {
    fn arc_is_possible(context: &impl ReadDomains, arc: &PossibleRouteArc, item: i32) -> bool {
        context.contains(&arc.selected, 1)
            && context.contains(&arc.from_item, item)
            && context.contains(&arc.to_item, item)
    }

    fn arc_is_positive(context: &impl ReadDomains, arc: &PossibleRouteArc, item: i32) -> bool {
        context.lower_bound(&arc.selected) == 1
            && context.lower_bound(&arc.from_item) == item
            && context.upper_bound(&arc.from_item) == item
            && context.lower_bound(&arc.to_item) == item
            && context.upper_bound(&arc.to_item) == item
    }

    fn blocker(context: &impl ReadDomains, arc: &PossibleRouteArc, item: i32) -> Predicate {
        if !context.contains(&arc.selected, 1) {
            arc.selected.upper_bound_predicate(0)
        } else if !context.contains(&arc.from_item, item) {
            arc.from_item.disequality_predicate(item)
        } else {
            debug_assert!(!context.contains(&arc.to_item, item));
            arc.to_item.disequality_predicate(item)
        }
    }

    fn push_witness(
        &self,
        reason: &mut PropositionalConjunction,
        material: &LayerGridMaterial,
        witness: PositiveWitness,
        include_selection: bool,
    ) {
        match witness {
            PositiveWitness::Terminal(selected) => {
                if include_selection {
                    reason.push(selected.lower_bound_predicate(1));
                }
            }
            PositiveWitness::Arc(arc_index) => {
                let arc = &self.arcs[arc_index];
                if include_selection {
                    reason.push(arc.selected.lower_bound_predicate(1));
                }
                reason.push(arc.from_item.equality_predicate(material.item_code));
                reason.push(arc.to_item.equality_predicate(material.item_code));
            }
        }
    }

    fn build_reason(
        &self,
        context: &impl ReadDomains,
        material: &LayerGridMaterial,
        cell: usize,
        witness: PositiveWitness,
        include_witness_selection: bool,
        opposing_terminals: &[PossibleTerminalOption],
        continuation_arcs: &[usize],
        support: Option<usize>,
    ) -> PropositionalConjunction {
        let mut reason = PropositionalConjunction::default();
        self.push_witness(&mut reason, material, witness, include_witness_selection);
        if let Some(bridge) = self.bridge_selected_by_cell[cell] {
            reason.push(bridge.upper_bound_predicate(0));
        }
        reason.extend(
            opposing_terminals
                .iter()
                .map(|terminal| terminal.selected.upper_bound_predicate(0)),
        );
        reason.extend(
            continuation_arcs
                .iter()
                .copied()
                .filter(|arc_index| Some(*arc_index) != support)
                .map(|arc_index| Self::blocker(context, &self.arcs[arc_index], material.item_code)),
        );
        self.counters
            .local_active_maximum_reason_predicates
            .fetch_max(
                reason.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        reason
    }

    fn force_support(
        &self,
        context: &mut PropagationContext,
        material: &LayerGridMaterial,
        support: usize,
        reason: PropositionalConjunction,
    ) -> PropagationStatusCP {
        let arc = self.arcs[support];
        for (variable, value) in [
            (arc.selected, 1),
            (arc.from_item, material.item_code),
            (arc.to_item, material.item_code),
        ] {
            if context.lower_bound(&variable) == value && context.upper_bound(&variable) == value {
                continue;
            }
            self.counters
                .local_active_forced_predicate_attempts
                .fetch_add(1, Ordering::Relaxed);
            if let Err(conflict) = context.post(
                variable.equality_predicate(value),
                (reason.clone(), &self.inference_code),
            ) {
                self.counters
                    .local_active_conflicts
                    .fetch_add(1, Ordering::Relaxed);
                return Err(conflict.into());
            }
        }
        Ok(())
    }

    fn reject_witness(
        &self,
        context: &mut PropagationContext,
        witness: PositiveWitness,
        reason: PropositionalConjunction,
    ) -> PropagationStatusCP {
        let selected = match witness {
            PositiveWitness::Terminal(selected) => selected,
            PositiveWitness::Arc(arc_index) => self.arcs[arc_index].selected,
        };
        self.counters
            .local_active_forced_predicate_attempts
            .fetch_add(1, Ordering::Relaxed);
        if let Err(conflict) = context.post(
            selected.upper_bound_predicate(0),
            (reason, &self.inference_code),
        ) {
            self.counters
                .local_active_conflicts
                .fetch_add(1, Ordering::Relaxed);
            return Err(conflict.into());
        }
        Ok(())
    }

    fn propagate_key(
        &self,
        context: &mut PropagationContext,
        material_index: usize,
        cell: usize,
    ) -> PropagationStatusCP {
        if self.bridge_selected_by_cell[cell].is_some_and(|bridge| context.contains(&bridge, 1)) {
            return Ok(());
        }
        let material = &self.materials[material_index];
        self.propagate_forward(context, material, cell)?;
        self.propagate_backward(context, material, cell)
    }

    fn propagate_forward(
        &self,
        context: &mut PropagationContext,
        material: &LayerGridMaterial,
        cell: usize,
    ) -> PropagationStatusCP {
        let witness = material
            .supplies
            .iter()
            .find(|terminal| terminal.cell == cell && context.lower_bound(&terminal.selected) == 1)
            .map(|terminal| PositiveWitness::Terminal(terminal.selected))
            .or_else(|| {
                self.incoming_arc_indices[cell]
                    .iter()
                    .copied()
                    .find(|arc_index| {
                        Self::arc_is_positive(context, &self.arcs[*arc_index], material.item_code)
                    })
                    .map(PositiveWitness::Arc)
            });
        let Some(witness) = witness else {
            return Ok(());
        };
        let opposing = material
            .demands
            .iter()
            .filter(|terminal| terminal.cell == cell)
            .copied()
            .collect::<Vec<_>>();
        if opposing
            .iter()
            .any(|terminal| context.contains(&terminal.selected, 1))
        {
            return Ok(());
        }
        let possible = self.outgoing_arc_indices[cell]
            .iter()
            .copied()
            .filter(|arc_index| {
                Self::arc_is_possible(context, &self.arcs[*arc_index], material.item_code)
            })
            .collect::<Vec<_>>();
        match possible.as_slice() {
            [] => {
                let reason = self.build_reason(
                    context,
                    material,
                    cell,
                    witness,
                    false,
                    &opposing,
                    &self.outgoing_arc_indices[cell],
                    None,
                );
                self.reject_witness(context, witness, reason)
            }
            [support] => {
                let reason = self.build_reason(
                    context,
                    material,
                    cell,
                    witness,
                    true,
                    &opposing,
                    &self.outgoing_arc_indices[cell],
                    Some(*support),
                );
                self.force_support(context, material, *support, reason)
            }
            _ => Ok(()),
        }
    }

    fn propagate_backward(
        &self,
        context: &mut PropagationContext,
        material: &LayerGridMaterial,
        cell: usize,
    ) -> PropagationStatusCP {
        let witness = material
            .demands
            .iter()
            .find(|terminal| terminal.cell == cell && context.lower_bound(&terminal.selected) == 1)
            .map(|terminal| PositiveWitness::Terminal(terminal.selected))
            .or_else(|| {
                self.outgoing_arc_indices[cell]
                    .iter()
                    .copied()
                    .find(|arc_index| {
                        Self::arc_is_positive(context, &self.arcs[*arc_index], material.item_code)
                    })
                    .map(PositiveWitness::Arc)
            });
        let Some(witness) = witness else {
            return Ok(());
        };
        let opposing = material
            .supplies
            .iter()
            .filter(|terminal| terminal.cell == cell)
            .copied()
            .collect::<Vec<_>>();
        if opposing
            .iter()
            .any(|terminal| context.contains(&terminal.selected, 1))
        {
            return Ok(());
        }
        let possible = self.incoming_arc_indices[cell]
            .iter()
            .copied()
            .filter(|arc_index| {
                Self::arc_is_possible(context, &self.arcs[*arc_index], material.item_code)
            })
            .collect::<Vec<_>>();
        match possible.as_slice() {
            [] => {
                let reason = self.build_reason(
                    context,
                    material,
                    cell,
                    witness,
                    false,
                    &opposing,
                    &self.incoming_arc_indices[cell],
                    None,
                );
                self.reject_witness(context, witness, reason)
            }
            [support] => {
                let reason = self.build_reason(
                    context,
                    material,
                    cell,
                    witness,
                    true,
                    &opposing,
                    &self.incoming_arc_indices[cell],
                    Some(*support),
                );
                self.force_support(context, material, *support, reason)
            }
            _ => Ok(()),
        }
    }
}

impl Propagator for LocalPositiveFlowContinuationPropagator {
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
            .local_active_notifications
            .fetch_add(1, Ordering::Relaxed);
        self.dirty_keys.extend(
            self.event_impacts[local_id.unpack() as usize]
                .iter()
                .copied(),
        );
        self.counters.local_active_maximum_dirty_keys.fetch_max(
            self.dirty_keys.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        EnqueueDecision::Enqueue
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.counters
            .local_active_executions
            .fetch_add(1, Ordering::Relaxed);
        for material_index in 0..self.materials.len() {
            for cell in 0..self.incoming_arc_indices.len() {
                self.counters
                    .local_active_dirty_keys
                    .fetch_add(1, Ordering::Relaxed);
                self.propagate_key(&mut context, material_index, cell)?;
            }
        }
        Ok(())
    }

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        self.counters
            .local_active_executions
            .fetch_add(1, Ordering::Relaxed);
        let dirty_keys = std::mem::take(&mut self.dirty_keys);
        self.counters.local_active_dirty_keys.fetch_add(
            dirty_keys.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        for (material_index, cell) in dirty_keys {
            self.propagate_key(&mut context, material_index, cell)?;
        }
        Ok(())
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
    use pumpkin_solver::core::results::{
        CSPSolverExecutionFlag, ProblemSolution, SatisfactionResult,
    };
    use pumpkin_solver::core::termination::Indefinite;

    use super::*;
    use crate::layouts::integrated::exact::grid_analyzer::LayerGridAnalyzerCounters;
    use crate::layouts::integrated::exact::search_statistics::{
        MeteredBrancher, SearchEventCounters, capture_search_statistics,
    };

    fn add_rule(
        solver: &mut Solver,
        cell_count: usize,
        arcs: Vec<PossibleRouteArc>,
        supplies: Vec<PossibleTerminalOption>,
        demands: Vec<PossibleTerminalOption>,
        bridges: Vec<Option<DomainId>>,
        counters: Arc<LayerGridAnalyzerCounters>,
    ) -> pumpkin_solver::core::proof::ConstraintTag {
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationPropagatorArgs(
            LocalPositiveFlowContinuationAnalyzerArgs {
                rule: LayerGridRuleArgs {
                    name: "controlled-active-local-continuation".to_string(),
                    cell_count,
                    arcs,
                    materials: vec![LayerGridMaterial {
                        item_code: 1,
                        supplies,
                        demands,
                    }],
                    counters,
                    constraint_tag: tag,
                },
                bridge_selected_by_cell: bridges,
            },
        ));
        tag
    }

    #[test]
    fn supply_rooted_chain_forces_both_forward_arcs() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        add_rule(
            &mut solver,
            3,
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
            vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            vec![PossibleTerminalOption {
                cell: 2,
                selected: demand,
            }],
            vec![None; 3],
            Arc::clone(&counters),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&first), 1);
        assert_eq!(solver.lower_bound(&second), 1);
        assert!(counters.snapshot().local_active_forced_predicate_attempts >= 2);
    }

    #[test]
    fn demand_rooted_chain_reaches_backward_across_a_later_notification() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        add_rule(
            &mut solver,
            3,
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
            vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            vec![PossibleTerminalOption {
                cell: 2,
                selected: demand,
            }],
            vec![None; 3],
            Arc::clone(&counters),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&first), 1);
        assert_eq!(solver.lower_bound(&second), 1);
        assert!(counters.snapshot().local_active_executions >= 2);
    }

    #[test]
    fn branch_with_two_possible_continuations_is_not_forced() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        add_rule(
            &mut solver,
            3,
            vec![
                PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected: first,
                    from_item: item,
                    to_item: item,
                },
                PossibleRouteArc {
                    from: 0,
                    to: 2,
                    selected: second,
                    from_item: item,
                    to_item: item,
                },
            ],
            vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            vec![],
            vec![None; 3],
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&first, 0));
        assert!(solver.contains(&second, 0));
    }

    #[test]
    fn bridge_exclusion_event_enables_forward_forcing() {
        let mut solver = Solver::default();
        let outgoing = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let bridge = solver.new_bounded_integer(0, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = add_rule(
            &mut solver,
            2,
            vec![PossibleRouteArc {
                from: 0,
                to: 1,
                selected: outgoing,
                from_item: item,
                to_item: item,
            }],
            vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            vec![PossibleTerminalOption {
                cell: 1,
                selected: demand,
            }],
            vec![Some(bridge), None],
            Arc::clone(&counters),
        );
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&outgoing, 0));

        solver.add_clause([bridge.upper_bound_predicate(0)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&outgoing), 1);
        assert!(counters.snapshot().local_active_notifications >= 1);
    }

    #[test]
    fn selected_supply_without_a_continuation_conflicts() {
        let mut solver = Solver::default();
        let supply = solver.new_bounded_integer(1, 1);
        add_rule(
            &mut solver,
            1,
            vec![],
            vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            vec![],
            vec![None],
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
    }

    #[test]
    fn selected_arc_without_local_support_conflicts_from_its_item_witness() {
        let mut solver = Solver::default();
        let selected = solver.new_bounded_integer(1, 1);
        let item = solver.new_bounded_integer(1, 1);
        add_rule(
            &mut solver,
            2,
            vec![PossibleRouteArc {
                from: 0,
                to: 1,
                selected,
                from_item: item,
                to_item: item,
            }],
            vec![],
            vec![],
            vec![None; 2],
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
    }

    #[test]
    fn selected_circulation_remains_feasible() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(1, 1);
        let second = solver.new_bounded_integer(1, 1);
        let item = solver.new_bounded_integer(1, 1);
        add_rule(
            &mut solver,
            2,
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
                    to: 0,
                    selected: second,
                    from_item: item,
                    to_item: item,
                },
            ],
            vec![],
            vec![],
            vec![None; 2],
            Arc::new(LayerGridAnalyzerCounters::default()),
        );

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
    }

    #[test]
    fn item_loss_event_turns_a_branch_into_one_forward_support() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let fixed_item = solver.new_bounded_integer(1, 1);
        let shared_item = solver.new_bounded_integer(1, 2);
        let supply = solver.new_bounded_integer(1, 1);
        let first_demand = solver.new_bounded_integer(0, 1);
        let second_demand = solver.new_bounded_integer(0, 1);
        let tag = add_rule(
            &mut solver,
            3,
            vec![
                PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected: first,
                    from_item: fixed_item,
                    to_item: fixed_item,
                },
                PossibleRouteArc {
                    from: 0,
                    to: 2,
                    selected: second,
                    from_item: shared_item,
                    to_item: shared_item,
                },
            ],
            vec![PossibleTerminalOption {
                cell: 0,
                selected: supply,
            }],
            vec![
                PossibleTerminalOption {
                    cell: 1,
                    selected: first_demand,
                },
                PossibleTerminalOption {
                    cell: 2,
                    selected: second_demand,
                },
            ],
            vec![None; 3],
            Arc::new(LayerGridAnalyzerCounters::default()),
        );
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&first, 0));

        solver.add_clause([shared_item.disequality_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&first), 1);
        assert!(solver.contains(&second, 0));
    }

    #[test]
    fn backtracking_discards_a_conflicting_positive_witness() {
        let mut solver = Solver::default();
        let choice_and_supply = solver.new_bounded_integer(0, 1);
        let outgoing = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let tag = add_rule(
            &mut solver,
            2,
            vec![PossibleRouteArc {
                from: 0,
                to: 1,
                selected: outgoing,
                from_item: item,
                to_item: item,
            }],
            vec![PossibleTerminalOption {
                cell: 0,
                selected: choice_and_supply,
            }],
            vec![],
            vec![None; 2],
            Arc::new(LayerGridAnalyzerCounters::default()),
        );
        solver.add_clause(
            [
                choice_and_supply.upper_bound_predicate(0),
                outgoing.upper_bound_predicate(0),
            ],
            tag,
        );

        let search_counters = Arc::new(std::sync::Mutex::new(SearchEventCounters::default()));
        let branchers: Vec<Box<dyn Brancher>> = vec![
            Box::new(WarmStart::new(&[choice_and_supply], &[1])),
            Box::new(solver.default_brancher()),
        ];
        let mut brancher = MeteredBrancher::new(
            DynamicBrancher::new(branchers),
            Arc::clone(&search_counters),
        );
        let mut resolver = ResolutionResolver::default();
        match solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver) {
            SatisfactionResult::Satisfiable(result) => {
                assert_eq!(result.solution().get_integer_value(choice_and_supply), 0);
                let statistics = capture_search_statistics(
                    result.solver(),
                    result.brancher(),
                    result.conflict_resolver(),
                    &search_counters,
                );
                assert!(statistics.backtracks.unwrap_or_default() >= 1);
            }
            _ => panic!("controlled local-continuation backtracking fixture must be satisfiable"),
        }
    }

    #[test]
    fn shared_item_loss_notifies_every_material_at_the_arc_endpoint() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item_two = solver.new_bounded_integer(2, 2);
        let shared_item = solver.new_bounded_integer(1, 2);
        let supply = solver.new_bounded_integer(1, 1);
        let possible_demand = solver.new_bounded_integer(0, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LocalPositiveFlowContinuationPropagatorArgs(
            LocalPositiveFlowContinuationAnalyzerArgs {
                rule: LayerGridRuleArgs {
                    name: "controlled-cross-material-notification".to_string(),
                    cell_count: 3,
                    arcs: vec![
                        PossibleRouteArc {
                            from: 0,
                            to: 1,
                            selected: first,
                            from_item: item_two,
                            to_item: item_two,
                        },
                        PossibleRouteArc {
                            from: 0,
                            to: 2,
                            selected: second,
                            from_item: shared_item,
                            to_item: shared_item,
                        },
                    ],
                    materials: vec![
                        LayerGridMaterial {
                            item_code: 1,
                            supplies: vec![],
                            demands: vec![],
                        },
                        LayerGridMaterial {
                            item_code: 2,
                            supplies: vec![PossibleTerminalOption {
                                cell: 0,
                                selected: supply,
                            }],
                            demands: vec![PossibleTerminalOption {
                                cell: 1,
                                selected: possible_demand,
                            }],
                        },
                    ],
                    counters: Arc::clone(&counters),
                    constraint_tag: tag,
                },
                bridge_selected_by_cell: vec![None; 3],
            },
        ));
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&first, 0));
        let initial_dirty_keys = counters.snapshot().local_active_dirty_keys;

        solver.add_clause([shared_item.disequality_predicate(2)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&first), 1);
        assert!(counters.snapshot().local_active_dirty_keys >= initial_dirty_keys + 2);
    }
}
