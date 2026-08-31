use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use pumpkin_solver::core::declare_inference_label;
use pumpkin_solver::core::predicates::{PredicateConstructor, PropositionalConjunction};
use pumpkin_solver::core::proof::{ConstraintTag, InferenceCode};
use pumpkin_solver::core::propagation::{
    DomainEvents, EventsToRegister, LocalId, Priority, PropagationContext, Propagator,
    PropagatorConstructor, PropagatorConstructorContext, PropagatorSpec, ReadDomains,
    RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

use super::connectivity_propagator::{PossibleRouteArc, PossibleTerminalOption};

declare_inference_label!(TerminalGridSupport);

#[derive(Debug, Default)]
pub(super) struct LayerGridAnalyzerCounters {
    executions: AtomicU64,
    material_passes: AtomicU64,
    selected_demand_options: AtomicU64,
    selected_demand_cells: AtomicU64,
    reachable_selected_demand_cells: AtomicU64,
    unique_support_steps: AtomicU64,
    unresolved_predicate_observations: AtomicU64,
    terminal_support_steps: AtomicU64,
    terminal_unresolved_predicate_observations: AtomicU64,
    maximum_unique_support_chain: AtomicU64,
    registered_domain_variables: AtomicU64,
    forced_predicate_attempts: AtomicU64,
    forcing_conflicts: AtomicU64,
    maximum_reason_predicates: AtomicU64,
    distinct_support_arcs: Mutex<BTreeSet<(i32, DomainId)>>,
    distinct_unresolved_predicates: Mutex<BTreeSet<(DomainId, i32)>>,
    distinct_terminal_support_arcs: Mutex<BTreeSet<(i32, DomainId)>>,
    distinct_terminal_unresolved_predicates: Mutex<BTreeSet<(DomainId, i32)>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct LayerGridAnalyzerStatistics {
    pub executions: u64,
    pub material_passes: u64,
    pub selected_demand_options: u64,
    pub selected_demand_cells: u64,
    pub reachable_selected_demand_cells: u64,
    pub unique_support_steps: u64,
    pub unresolved_predicate_observations: u64,
    pub terminal_support_steps: u64,
    pub terminal_unresolved_predicate_observations: u64,
    pub distinct_support_arcs: u64,
    pub distinct_unresolved_predicates: u64,
    pub distinct_terminal_support_arcs: u64,
    pub distinct_terminal_unresolved_predicates: u64,
    pub maximum_unique_support_chain: u64,
    pub registered_domain_variables: u64,
    pub forced_predicate_attempts: u64,
    pub forcing_conflicts: u64,
    pub maximum_reason_predicates: u64,
}

impl LayerGridAnalyzerCounters {
    pub(super) fn snapshot(&self) -> LayerGridAnalyzerStatistics {
        LayerGridAnalyzerStatistics {
            executions: self.executions.load(Ordering::Relaxed),
            material_passes: self.material_passes.load(Ordering::Relaxed),
            selected_demand_options: self.selected_demand_options.load(Ordering::Relaxed),
            selected_demand_cells: self.selected_demand_cells.load(Ordering::Relaxed),
            reachable_selected_demand_cells: self
                .reachable_selected_demand_cells
                .load(Ordering::Relaxed),
            unique_support_steps: self.unique_support_steps.load(Ordering::Relaxed),
            unresolved_predicate_observations: self
                .unresolved_predicate_observations
                .load(Ordering::Relaxed),
            terminal_support_steps: self.terminal_support_steps.load(Ordering::Relaxed),
            terminal_unresolved_predicate_observations: self
                .terminal_unresolved_predicate_observations
                .load(Ordering::Relaxed),
            distinct_support_arcs: self
                .distinct_support_arcs
                .lock()
                .expect("grid analyzer support-arc counter is not poisoned")
                .len() as u64,
            distinct_unresolved_predicates: self
                .distinct_unresolved_predicates
                .lock()
                .expect("grid analyzer predicate counter is not poisoned")
                .len() as u64,
            distinct_terminal_support_arcs: self
                .distinct_terminal_support_arcs
                .lock()
                .expect("grid analyzer terminal support-arc counter is not poisoned")
                .len() as u64,
            distinct_terminal_unresolved_predicates: self
                .distinct_terminal_unresolved_predicates
                .lock()
                .expect("grid analyzer terminal predicate counter is not poisoned")
                .len() as u64,
            maximum_unique_support_chain: self.maximum_unique_support_chain.load(Ordering::Relaxed),
            registered_domain_variables: self.registered_domain_variables.load(Ordering::Relaxed),
            forced_predicate_attempts: self.forced_predicate_attempts.load(Ordering::Relaxed),
            forcing_conflicts: self.forcing_conflicts.load(Ordering::Relaxed),
            maximum_reason_predicates: self.maximum_reason_predicates.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LayerGridAnalyzerMode {
    Observe,
    ForceTerminalSupport,
}

#[derive(Clone, Debug)]
pub(super) struct LayerGridMaterial {
    pub item_code: i32,
    pub supplies: Vec<PossibleTerminalOption>,
    pub demands: Vec<PossibleTerminalOption>,
}

#[derive(Clone, Debug)]
pub(super) struct LayerGridAnalyzerArgs {
    pub name: String,
    pub cell_count: usize,
    pub arcs: Vec<PossibleRouteArc>,
    pub materials: Vec<LayerGridMaterial>,
    pub counters: Arc<LayerGridAnalyzerCounters>,
    pub mode: LayerGridAnalyzerMode,
    pub constraint_tag: ConstraintTag,
}

impl LayerGridAnalyzerArgs {
    pub(super) fn variables(&self) -> impl Iterator<Item = DomainId> + '_ {
        self.arcs
            .iter()
            .flat_map(|arc| [arc.selected, arc.from_item, arc.to_item])
            .chain(
                self.materials
                    .iter()
                    .flat_map(|material| &material.supplies)
                    .map(|option| option.selected),
            )
            .chain(
                self.materials
                    .iter()
                    .flat_map(|material| &material.demands)
                    .map(|option| option.selected),
            )
    }

    fn terminal_support_variables(&self) -> impl Iterator<Item = DomainId> + '_ {
        let demand_cells = self
            .materials
            .iter()
            .flat_map(|material| &material.demands)
            .map(|demand| demand.cell)
            .collect::<BTreeSet<_>>();
        let arc_demand_cells = demand_cells.clone();
        self.arcs
            .iter()
            .filter(move |arc| arc_demand_cells.contains(&arc.to))
            .flat_map(|arc| [arc.selected, arc.from_item, arc.to_item])
            .chain(
                self.materials
                    .iter()
                    .flat_map(|material| &material.supplies)
                    .filter(move |supply| demand_cells.contains(&supply.cell))
                    .map(|supply| supply.selected),
            )
            .chain(
                self.materials
                    .iter()
                    .flat_map(|material| &material.demands)
                    .map(|demand| demand.selected),
            )
    }
}

impl PropagatorConstructor for LayerGridAnalyzerArgs {
    type PropagatorImpl = LayerGridAnalyzer;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let variables = match self.mode {
            LayerGridAnalyzerMode::Observe => self.variables().collect::<BTreeSet<_>>(),
            LayerGridAnalyzerMode::ForceTerminalSupport => {
                self.terminal_support_variables().collect::<BTreeSet<_>>()
            }
        };
        self.counters.registered_domain_variables.fetch_add(
            variables.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        let mut variables = variables.into_iter();
        let first = variables
            .next()
            .expect("a layer grid analyzer has terminals or arcs");
        let mut registration = EventsToRegister::builder()
            .add(&first, DomainEvents::ANY_INT, LocalId::from(0))
            .build();
        for (index, variable) in variables.enumerate() {
            registration.add(
                &variable,
                DomainEvents::ANY_INT,
                LocalId::from(
                    u32::try_from(index + 1).expect("grid analyzer variable count fits u32"),
                ),
            );
        }

        let mut outgoing_arc_indices = vec![Vec::new(); self.cell_count];
        let mut incoming_arc_indices = vec![Vec::new(); self.cell_count];
        for (index, arc) in self.arcs.iter().enumerate() {
            outgoing_arc_indices[arc.from].push(index);
            incoming_arc_indices[arc.to].push(index);
        }

        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: LayerGridAnalyzer {
                name: self.name,
                cell_count: self.cell_count,
                arcs: self.arcs,
                outgoing_arc_indices,
                incoming_arc_indices,
                materials: self.materials,
                counters: self.counters,
                mode: self.mode,
                inference_code: InferenceCode::new(self.constraint_tag, TerminalGridSupport),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LayerGridAnalyzer {
    name: String,
    cell_count: usize,
    arcs: Vec<PossibleRouteArc>,
    outgoing_arc_indices: Vec<Vec<usize>>,
    incoming_arc_indices: Vec<Vec<usize>>,
    materials: Vec<LayerGridMaterial>,
    counters: Arc<LayerGridAnalyzerCounters>,
    mode: LayerGridAnalyzerMode,
    inference_code: InferenceCode,
}

impl LayerGridAnalyzer {
    fn arc_is_possible(context: &impl ReadDomains, arc: &PossibleRouteArc, item_code: i32) -> bool {
        context.contains(&arc.selected, 1)
            && context.contains(&arc.from_item, item_code)
            && context.contains(&arc.to_item, item_code)
    }

    fn predicate_is_unresolved(context: &impl ReadDomains, variable: DomainId, value: i32) -> bool {
        context.contains(&variable, value)
            && (context.lower_bound(&variable) != value || context.upper_bound(&variable) != value)
    }

    fn build_terminal_support_reason(
        &self,
        context: &impl ReadDomains,
        material: &LayerGridMaterial,
        selected_demand: DomainId,
        demand_cell: usize,
        support_arc_index: usize,
    ) -> PropositionalConjunction {
        let reason = std::iter::once(selected_demand.lower_bound_predicate(1))
            .chain(
                self.incoming_arc_indices[demand_cell]
                    .iter()
                    .filter_map(|&arc_index| {
                        if arc_index == support_arc_index {
                            return None;
                        }
                        let arc = &self.arcs[arc_index];
                        if !context.contains(&arc.selected, 1) {
                            Some(arc.selected.upper_bound_predicate(0))
                        } else if !context.contains(&arc.from_item, material.item_code) {
                            Some(arc.from_item.disequality_predicate(material.item_code))
                        } else if !context.contains(&arc.to_item, material.item_code) {
                            Some(arc.to_item.disequality_predicate(material.item_code))
                        } else {
                            None
                        }
                    }),
            )
            .chain(
                material
                    .supplies
                    .iter()
                    .filter(|supply| {
                        supply.cell == demand_cell && !context.contains(&supply.selected, 1)
                    })
                    .map(|supply| supply.selected.upper_bound_predicate(0)),
            )
            .collect::<PropositionalConjunction>();
        self.counters.maximum_reason_predicates.fetch_max(
            reason.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        reason
    }

    fn analyze_material(
        &self,
        context: &mut PropagationContext,
        material: &LayerGridMaterial,
    ) -> PropagationStatusCP {
        self.counters
            .material_passes
            .fetch_add(1, Ordering::Relaxed);
        let mut possible_supply_cells = vec![false; self.cell_count];
        let mut reachable = vec![false; self.cell_count];
        let mut frontier = VecDeque::new();
        for supply in &material.supplies {
            if context.contains(&supply.selected, 1) {
                possible_supply_cells[supply.cell] = true;
                if !reachable[supply.cell] {
                    reachable[supply.cell] = true;
                    frontier.push_back(supply.cell);
                }
            }
        }
        while let Some(cell) = frontier.pop_front() {
            for &arc_index in &self.outgoing_arc_indices[cell] {
                let arc = &self.arcs[arc_index];
                if Self::arc_is_possible(context, arc, material.item_code) && !reachable[arc.to] {
                    reachable[arc.to] = true;
                    frontier.push_back(arc.to);
                }
            }
        }

        let mut selected_demand_cells = Vec::new();
        let mut selected_cell = vec![false; self.cell_count];
        for demand in &material.demands {
            if context.lower_bound(&demand.selected) != 1 {
                continue;
            }
            self.counters
                .selected_demand_options
                .fetch_add(1, Ordering::Relaxed);
            if !selected_cell[demand.cell] {
                selected_cell[demand.cell] = true;
                selected_demand_cells.push(demand.cell);
            }
        }
        self.counters.selected_demand_cells.fetch_add(
            selected_demand_cells.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );

        for demand_cell in selected_demand_cells {
            if !reachable[demand_cell] {
                continue;
            }
            self.counters
                .reachable_selected_demand_cells
                .fetch_add(1, Ordering::Relaxed);
            let mut required_cell = demand_cell;
            let mut visited = vec![false; self.cell_count];
            let mut chain_length = 0_u64;
            loop {
                if possible_supply_cells[required_cell] || visited[required_cell] {
                    break;
                }
                visited[required_cell] = true;
                let mut unique_support = None;
                for &arc_index in &self.incoming_arc_indices[required_cell] {
                    let arc = &self.arcs[arc_index];
                    if !reachable[arc.from]
                        || !Self::arc_is_possible(context, arc, material.item_code)
                    {
                        continue;
                    }
                    if unique_support.is_some() {
                        unique_support = None;
                        break;
                    }
                    unique_support = Some(arc_index);
                }
                let Some(arc_index) = unique_support else {
                    break;
                };
                let arc = &self.arcs[arc_index];
                chain_length += 1;
                self.counters
                    .unique_support_steps
                    .fetch_add(1, Ordering::Relaxed);

                let candidates = [
                    (arc.selected, 1),
                    (arc.from_item, material.item_code),
                    (arc.to_item, material.item_code),
                ];
                let unresolved = candidates
                    .into_iter()
                    .filter(|(variable, value)| {
                        Self::predicate_is_unresolved(context, *variable, *value)
                    })
                    .collect::<Vec<_>>();
                if !unresolved.is_empty() {
                    self.counters
                        .distinct_support_arcs
                        .lock()
                        .expect("grid analyzer support-arc counter is not poisoned")
                        .insert((material.item_code, arc.selected));
                    self.counters
                        .unresolved_predicate_observations
                        .fetch_add(unresolved.len() as u64, Ordering::Relaxed);
                    let mut predicates = self
                        .counters
                        .distinct_unresolved_predicates
                        .lock()
                        .expect("grid analyzer predicate counter is not poisoned");
                    predicates.extend(unresolved.iter().copied());
                }
                if chain_length == 1 {
                    self.counters
                        .terminal_support_steps
                        .fetch_add(1, Ordering::Relaxed);
                    if !unresolved.is_empty() {
                        self.counters
                            .terminal_unresolved_predicate_observations
                            .fetch_add(unresolved.len() as u64, Ordering::Relaxed);
                        self.counters
                            .distinct_terminal_support_arcs
                            .lock()
                            .expect("grid analyzer terminal support-arc counter is not poisoned")
                            .insert((material.item_code, arc.selected));
                        self.counters
                            .distinct_terminal_unresolved_predicates
                            .lock()
                            .expect("grid analyzer terminal predicate counter is not poisoned")
                            .extend(unresolved.iter().copied());
                    }
                }
                required_cell = arc.from;
            }
            self.counters
                .maximum_unique_support_chain
                .fetch_max(chain_length, Ordering::Relaxed);
        }
        Ok(())
    }

    fn propagate_terminal_support(
        &self,
        context: &mut PropagationContext,
        material: &LayerGridMaterial,
    ) -> PropagationStatusCP {
        self.counters
            .material_passes
            .fetch_add(1, Ordering::Relaxed);
        for demand in &material.demands {
            if context.lower_bound(&demand.selected) != 1 {
                continue;
            }
            self.counters
                .selected_demand_options
                .fetch_add(1, Ordering::Relaxed);
            if material
                .supplies
                .iter()
                .any(|supply| supply.cell == demand.cell && context.contains(&supply.selected, 1))
            {
                continue;
            }

            let mut unique_support = None;
            for &arc_index in &self.incoming_arc_indices[demand.cell] {
                let arc = &self.arcs[arc_index];
                if !Self::arc_is_possible(context, arc, material.item_code) {
                    continue;
                }
                if unique_support.is_some() {
                    unique_support = None;
                    break;
                }
                unique_support = Some(arc_index);
            }
            let Some(arc_index) = unique_support else {
                continue;
            };
            self.counters
                .reachable_selected_demand_cells
                .fetch_add(1, Ordering::Relaxed);
            self.counters
                .unique_support_steps
                .fetch_add(1, Ordering::Relaxed);
            self.counters
                .terminal_support_steps
                .fetch_add(1, Ordering::Relaxed);
            let arc = &self.arcs[arc_index];
            let unresolved = [
                (arc.selected, 1),
                (arc.from_item, material.item_code),
                (arc.to_item, material.item_code),
            ]
            .into_iter()
            .filter(|(variable, value)| Self::predicate_is_unresolved(context, *variable, *value))
            .collect::<Vec<_>>();
            if unresolved.is_empty() {
                continue;
            }
            self.counters
                .terminal_unresolved_predicate_observations
                .fetch_add(unresolved.len() as u64, Ordering::Relaxed);
            self.counters
                .distinct_terminal_support_arcs
                .lock()
                .expect("grid analyzer terminal support-arc counter is not poisoned")
                .insert((material.item_code, arc.selected));
            self.counters
                .distinct_terminal_unresolved_predicates
                .lock()
                .expect("grid analyzer terminal predicate counter is not poisoned")
                .extend(unresolved.iter().copied());

            let reason = self.build_terminal_support_reason(
                context,
                material,
                demand.selected,
                demand.cell,
                arc_index,
            );
            for (variable, value) in unresolved {
                self.counters
                    .forced_predicate_attempts
                    .fetch_add(1, Ordering::Relaxed);
                if let Err(conflict) = context.post(
                    variable.equality_predicate(value),
                    (reason.clone(), &self.inference_code),
                ) {
                    self.counters
                        .forcing_conflicts
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(conflict.into());
                }
            }
        }
        Ok(())
    }
}

impl Propagator for LayerGridAnalyzer {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::Low
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.counters.executions.fetch_add(1, Ordering::Relaxed);
        for material in &self.materials {
            match self.mode {
                LayerGridAnalyzerMode::Observe => {
                    self.analyze_material(&mut context, material)?;
                }
                LayerGridAnalyzerMode::ForceTerminalSupport => {
                    self.propagate_terminal_support(&mut context, material)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::Solver;
    use pumpkin_solver::core::results::CSPSolverExecutionFlag;

    use super::*;

    #[test]
    fn observes_but_does_not_force_a_unique_grid_support_chain() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LayerGridAnalyzerArgs {
            name: "controlled-layer-grid".to_string(),
            cell_count: 3,
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
                    to: 2,
                    selected: second,
                    from_item: item,
                    to_item: item,
                },
            ],
            materials: vec![LayerGridMaterial {
                item_code: 1,
                supplies: vec![PossibleTerminalOption {
                    cell: 0,
                    selected: supply,
                }],
                demands: vec![PossibleTerminalOption {
                    cell: 2,
                    selected: demand,
                }],
            }],
            counters: Arc::clone(&counters),
            mode: LayerGridAnalyzerMode::Observe,
            constraint_tag: tag,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert_eq!(statistics.unique_support_steps, 2);
        assert_eq!(statistics.distinct_support_arcs, 2);
        assert_eq!(statistics.distinct_unresolved_predicates, 2);
        assert_eq!(statistics.maximum_unique_support_chain, 2);
        assert!(solver.contains(&first, 0));
        assert!(solver.contains(&second, 0));
    }

    #[test]
    fn forces_only_the_unique_arc_entering_a_selected_demand() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let terminal_support = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LayerGridAnalyzerArgs {
            name: "controlled-terminal-grid-support".to_string(),
            cell_count: 3,
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
                    to: 2,
                    selected: terminal_support,
                    from_item: item,
                    to_item: item,
                },
            ],
            materials: vec![LayerGridMaterial {
                item_code: 1,
                supplies: vec![PossibleTerminalOption {
                    cell: 0,
                    selected: supply,
                }],
                demands: vec![PossibleTerminalOption {
                    cell: 2,
                    selected: demand,
                }],
            }],
            counters: Arc::clone(&counters),
            mode: LayerGridAnalyzerMode::ForceTerminalSupport,
            constraint_tag: tag,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&first, 0));
        assert_eq!(solver.lower_bound(&terminal_support), 1);
        assert!(counters.snapshot().forced_predicate_attempts >= 1);
    }

    #[test]
    fn does_not_force_when_an_unreachable_predecessor_can_still_enter_the_demand() {
        let mut solver = Solver::default();
        let reachable_support = solver.new_bounded_integer(0, 1);
        let unreachable_support = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(LayerGridAnalyzerArgs {
            name: "controlled-terminal-grid-alternative".to_string(),
            cell_count: 3,
            arcs: vec![
                PossibleRouteArc {
                    from: 0,
                    to: 2,
                    selected: reachable_support,
                    from_item: item,
                    to_item: item,
                },
                PossibleRouteArc {
                    from: 1,
                    to: 2,
                    selected: unreachable_support,
                    from_item: item,
                    to_item: item,
                },
            ],
            materials: vec![LayerGridMaterial {
                item_code: 1,
                supplies: vec![PossibleTerminalOption {
                    cell: 0,
                    selected: supply,
                }],
                demands: vec![PossibleTerminalOption {
                    cell: 2,
                    selected: demand,
                }],
            }],
            counters: Arc::clone(&counters),
            mode: LayerGridAnalyzerMode::ForceTerminalSupport,
            constraint_tag: tag,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&reachable_support, 0));
        assert!(solver.contains(&unreachable_support, 0));
        assert_eq!(counters.snapshot().forced_predicate_attempts, 0);
    }
}
