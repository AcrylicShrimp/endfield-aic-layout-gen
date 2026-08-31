use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use pumpkin_solver::core::declare_inference_label;
use pumpkin_solver::core::predicates::{PredicateConstructor, PropositionalConjunction};
use pumpkin_solver::core::proof::{ConstraintTag, InferenceCode};
use pumpkin_solver::core::propagation::{
    DomainEvents, EnqueueDecision, EventsToRegister, LocalId, NotificationContext,
    OpaqueDomainEvent, Priority, PropagationContext, Propagator, PropagatorConstructor,
    PropagatorConstructorContext, PropagatorSpec, ReadDomains, RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

use super::connectivity_propagator::{PossibleRouteArc, PossibleTerminalOption};

mod local_continuation;

pub(super) use local_continuation::LocalPositiveFlowContinuationAnalyzerArgs;

declare_inference_label!(TerminalGridSupport);
declare_inference_label!(UniqueSupportChain);

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
    frontier_notifications: AtomicU64,
    frontier_watcher_hits: AtomicU64,
    frontier_demand_rechecks: AtomicU64,
    frontier_watched_cell_registrations: AtomicU64,
    frontier_maximum_dirty_demands: AtomicU64,
    local_continuation_executions: AtomicU64,
    local_continuation_material_passes: AtomicU64,
    local_positive_inflow_cells: AtomicU64,
    local_positive_outflow_cells: AtomicU64,
    local_forward_continuation_cells: AtomicU64,
    local_backward_continuation_cells: AtomicU64,
    local_forward_zero_supports: AtomicU64,
    local_backward_zero_supports: AtomicU64,
    local_forward_unique_supports: AtomicU64,
    local_backward_unique_supports: AtomicU64,
    local_forward_unresolved_predicates: AtomicU64,
    local_backward_unresolved_predicates: AtomicU64,
    local_bridge_possible_cell_skips: AtomicU64,
    local_maximum_reason_predicates: AtomicU64,
    local_registered_domain_variables: AtomicU64,
    distinct_support_arcs: Mutex<BTreeSet<(i32, DomainId)>>,
    distinct_unresolved_predicates: Mutex<BTreeSet<(DomainId, i32)>>,
    distinct_terminal_support_arcs: Mutex<BTreeSet<(i32, DomainId)>>,
    distinct_terminal_unresolved_predicates: Mutex<BTreeSet<(DomainId, i32)>>,
    distinct_local_forward_support_arcs: Mutex<BTreeSet<(i32, DomainId)>>,
    distinct_local_backward_support_arcs: Mutex<BTreeSet<(i32, DomainId)>>,
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
    pub frontier_notifications: u64,
    pub frontier_watcher_hits: u64,
    pub frontier_demand_rechecks: u64,
    pub frontier_watched_cell_registrations: u64,
    pub frontier_maximum_dirty_demands: u64,
    pub local_continuation_executions: u64,
    pub local_continuation_material_passes: u64,
    pub local_positive_inflow_cells: u64,
    pub local_positive_outflow_cells: u64,
    pub local_forward_continuation_cells: u64,
    pub local_backward_continuation_cells: u64,
    pub local_forward_zero_supports: u64,
    pub local_backward_zero_supports: u64,
    pub local_forward_unique_supports: u64,
    pub local_backward_unique_supports: u64,
    pub local_forward_unresolved_predicates: u64,
    pub local_backward_unresolved_predicates: u64,
    pub distinct_local_forward_support_arcs: u64,
    pub distinct_local_backward_support_arcs: u64,
    pub local_bridge_possible_cell_skips: u64,
    pub local_maximum_reason_predicates: u64,
    pub local_registered_domain_variables: u64,
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
            frontier_notifications: self.frontier_notifications.load(Ordering::Relaxed),
            frontier_watcher_hits: self.frontier_watcher_hits.load(Ordering::Relaxed),
            frontier_demand_rechecks: self.frontier_demand_rechecks.load(Ordering::Relaxed),
            frontier_watched_cell_registrations: self
                .frontier_watched_cell_registrations
                .load(Ordering::Relaxed),
            frontier_maximum_dirty_demands: self
                .frontier_maximum_dirty_demands
                .load(Ordering::Relaxed),
            local_continuation_executions: self
                .local_continuation_executions
                .load(Ordering::Relaxed),
            local_continuation_material_passes: self
                .local_continuation_material_passes
                .load(Ordering::Relaxed),
            local_positive_inflow_cells: self.local_positive_inflow_cells.load(Ordering::Relaxed),
            local_positive_outflow_cells: self.local_positive_outflow_cells.load(Ordering::Relaxed),
            local_forward_continuation_cells: self
                .local_forward_continuation_cells
                .load(Ordering::Relaxed),
            local_backward_continuation_cells: self
                .local_backward_continuation_cells
                .load(Ordering::Relaxed),
            local_forward_zero_supports: self.local_forward_zero_supports.load(Ordering::Relaxed),
            local_backward_zero_supports: self.local_backward_zero_supports.load(Ordering::Relaxed),
            local_forward_unique_supports: self
                .local_forward_unique_supports
                .load(Ordering::Relaxed),
            local_backward_unique_supports: self
                .local_backward_unique_supports
                .load(Ordering::Relaxed),
            local_forward_unresolved_predicates: self
                .local_forward_unresolved_predicates
                .load(Ordering::Relaxed),
            local_backward_unresolved_predicates: self
                .local_backward_unresolved_predicates
                .load(Ordering::Relaxed),
            distinct_local_forward_support_arcs: self
                .distinct_local_forward_support_arcs
                .lock()
                .expect("local forward support-arc counter is not poisoned")
                .len() as u64,
            distinct_local_backward_support_arcs: self
                .distinct_local_backward_support_arcs
                .lock()
                .expect("local backward support-arc counter is not poisoned")
                .len() as u64,
            local_bridge_possible_cell_skips: self
                .local_bridge_possible_cell_skips
                .load(Ordering::Relaxed),
            local_maximum_reason_predicates: self
                .local_maximum_reason_predicates
                .load(Ordering::Relaxed),
            local_registered_domain_variables: self
                .local_registered_domain_variables
                .load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LayerGridRule {
    Observe,
    ForceTerminalSupport,
    ForceUniqueSupportChain,
    ForceUniqueSupportChainSelectiveWake,
    ForceDirtyMaterialUniqueSupportChain,
    ForceWatchedDemandUniqueSupportChain,
    ForceWatchedDemandUniqueSupportChainAndObserveLocalContinuation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UniqueSupportChainWakeMode {
    AnyDomainEvent,
    SupportLossEvents,
}

#[derive(Clone, Debug)]
pub(super) struct LayerGridMaterial {
    pub item_code: i32,
    pub supplies: Vec<PossibleTerminalOption>,
    pub demands: Vec<PossibleTerminalOption>,
}

#[derive(Clone, Debug)]
pub(super) struct LayerGridRuleArgs {
    pub name: String,
    pub cell_count: usize,
    pub arcs: Vec<PossibleRouteArc>,
    pub materials: Vec<LayerGridMaterial>,
    pub counters: Arc<LayerGridAnalyzerCounters>,
    pub constraint_tag: ConstraintTag,
}

impl LayerGridRuleArgs {
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

fn registration(
    counters: &LayerGridAnalyzerCounters,
    registrations: Vec<(DomainId, DomainEvents)>,
) -> EventsToRegister {
    counters.registered_domain_variables.fetch_add(
        registrations.len().try_into().unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    let mut registrations = registrations.into_iter();
    let (first, events) = registrations
        .next()
        .expect("a layer grid rule has terminals or arcs");
    let mut result = EventsToRegister::builder()
        .add(&first, events, LocalId::from(0))
        .build();
    for (index, (variable, events)) in registrations.enumerate() {
        result.add(
            &variable,
            events,
            LocalId::from(u32::try_from(index + 1).expect("grid rule variable count fits u32")),
        );
    }
    result
}

fn broad_registration(args: &LayerGridRuleArgs) -> EventsToRegister {
    registration(
        &args.counters,
        args.variables()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|variable| (variable, DomainEvents::ANY_INT))
            .collect(),
    )
}

fn terminal_registration(args: &LayerGridRuleArgs) -> EventsToRegister {
    registration(
        &args.counters,
        args.terminal_support_variables()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|variable| (variable, DomainEvents::ANY_INT))
            .collect(),
    )
}

#[derive(Default)]
struct WakeFlags {
    any: bool,
    lower: bool,
    upper: bool,
}

fn support_loss_registration(args: &LayerGridRuleArgs) -> EventsToRegister {
    let mut variables = BTreeMap::<DomainId, WakeFlags>::new();
    for arc in &args.arcs {
        variables.entry(arc.selected).or_default().upper = true;
        variables.entry(arc.from_item).or_default().any = true;
        variables.entry(arc.to_item).or_default().any = true;
    }
    for material in &args.materials {
        for supply in &material.supplies {
            variables.entry(supply.selected).or_default().upper = true;
        }
        for demand in &material.demands {
            variables.entry(demand.selected).or_default().lower = true;
        }
    }
    registration(
        &args.counters,
        variables
            .into_iter()
            .map(|(variable, flags)| {
                let events = if flags.any {
                    DomainEvents::ANY_INT
                } else if flags.lower && flags.upper {
                    DomainEvents::BOUNDS
                } else if flags.lower {
                    DomainEvents::LOWER_BOUND
                } else {
                    DomainEvents::UPPER_BOUND
                };
                (variable, events)
            })
            .collect(),
    )
}

fn dirty_material_registration(args: &LayerGridRuleArgs) -> (EventsToRegister, Vec<Vec<usize>>) {
    let mut variables = BTreeMap::<DomainId, (WakeFlags, BTreeSet<usize>)>::new();
    let all_materials = (0..args.materials.len()).collect::<BTreeSet<_>>();
    for arc in &args.arcs {
        variables.entry(arc.selected).or_default().0.upper = true;
        variables
            .entry(arc.selected)
            .or_default()
            .1
            .extend(&all_materials);
        for item in [arc.from_item, arc.to_item] {
            variables.entry(item).or_default().0.any = true;
            variables.entry(item).or_default().1.extend(&all_materials);
        }
    }
    for (material_index, material) in args.materials.iter().enumerate() {
        for supply in &material.supplies {
            variables.entry(supply.selected).or_default().0.upper = true;
            variables
                .entry(supply.selected)
                .or_default()
                .1
                .insert(material_index);
        }
        for demand in &material.demands {
            variables.entry(demand.selected).or_default().0.lower = true;
            variables
                .entry(demand.selected)
                .or_default()
                .1
                .insert(material_index);
        }
    }
    let registrations = variables
        .into_iter()
        .map(|(variable, (flags, materials))| {
            let events = if flags.any {
                DomainEvents::ANY_INT
            } else if flags.lower && flags.upper {
                DomainEvents::BOUNDS
            } else if flags.lower {
                DomainEvents::LOWER_BOUND
            } else {
                DomainEvents::UPPER_BOUND
            };
            (variable, events, materials.into_iter().collect::<Vec<_>>())
        })
        .collect::<Vec<_>>();
    let material_dependencies = registrations
        .iter()
        .map(|(_, _, materials)| materials.clone())
        .collect();
    let registration = registration(
        &args.counters,
        registrations
            .into_iter()
            .map(|(variable, events, _)| (variable, events))
            .collect(),
    );
    (registration, material_dependencies)
}

#[derive(Clone, Debug, Default)]
struct WatchedDemandEventImpact {
    direct_demands: BTreeSet<usize>,
    watch_keys: BTreeSet<(usize, usize)>,
}

#[derive(Clone, Copy, Debug)]
struct WatchedDemandRecord {
    material_index: usize,
    demand: PossibleTerminalOption,
}

fn watched_demand_records(args: &LayerGridRuleArgs) -> Vec<WatchedDemandRecord> {
    args.materials
        .iter()
        .enumerate()
        .flat_map(|(material_index, material)| {
            material
                .demands
                .iter()
                .copied()
                .map(move |demand| WatchedDemandRecord {
                    material_index,
                    demand,
                })
        })
        .collect()
}

fn watched_demand_registration(
    args: &LayerGridRuleArgs,
) -> (EventsToRegister, Vec<WatchedDemandEventImpact>) {
    let mut variables = BTreeMap::<DomainId, (WakeFlags, WatchedDemandEventImpact)>::new();
    for arc in &args.arcs {
        let selected = variables.entry(arc.selected).or_default();
        selected.0.upper = true;
        selected
            .1
            .watch_keys
            .extend((0..args.materials.len()).map(|material_index| (material_index, arc.to)));
        for item in [arc.from_item, arc.to_item] {
            let item = variables.entry(item).or_default();
            item.0.any = true;
            item.1
                .watch_keys
                .extend((0..args.materials.len()).map(|material_index| (material_index, arc.to)));
        }
    }
    let mut demand_id = 0;
    for (material_index, material) in args.materials.iter().enumerate() {
        for supply in &material.supplies {
            let entry = variables.entry(supply.selected).or_default();
            entry.0.upper = true;
            entry.1.watch_keys.insert((material_index, supply.cell));
        }
        for demand in &material.demands {
            let entry = variables.entry(demand.selected).or_default();
            entry.0.lower = true;
            entry.1.direct_demands.insert(demand_id);
            demand_id += 1;
        }
    }
    let registrations = variables
        .into_iter()
        .map(|(variable, (flags, impact))| {
            let events = if flags.any {
                DomainEvents::ANY_INT
            } else if flags.lower && flags.upper {
                DomainEvents::BOUNDS
            } else if flags.lower {
                DomainEvents::LOWER_BOUND
            } else {
                DomainEvents::UPPER_BOUND
            };
            (variable, events, impact)
        })
        .collect::<Vec<_>>();
    let event_impacts = registrations
        .iter()
        .map(|(_, _, impact)| impact.clone())
        .collect();
    let registration = registration(
        &args.counters,
        registrations
            .into_iter()
            .map(|(variable, events, _)| (variable, events))
            .collect(),
    );
    (registration, event_impacts)
}

fn rule_state(args: LayerGridRuleArgs, inference_code: InferenceCode) -> LayerGridRuleState {
    let mut outgoing_arc_indices = vec![Vec::new(); args.cell_count];
    let mut incoming_arc_indices = vec![Vec::new(); args.cell_count];
    for (index, arc) in args.arcs.iter().enumerate() {
        outgoing_arc_indices[arc.from].push(index);
        incoming_arc_indices[arc.to].push(index);
    }
    LayerGridRuleState {
        name: args.name,
        cell_count: args.cell_count,
        arcs: args.arcs,
        outgoing_arc_indices,
        incoming_arc_indices,
        materials: args.materials,
        counters: args.counters,
        inference_code,
    }
}

#[derive(Clone, Debug)]
pub(super) struct LayerGridOpportunityAnalyzerArgs(pub LayerGridRuleArgs);

impl PropagatorConstructor for LayerGridOpportunityAnalyzerArgs {
    type PropagatorImpl = LayerGridOpportunityAnalyzer;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let registration = broad_registration(&self.0);
        let tag = self.0.constraint_tag;
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: LayerGridOpportunityAnalyzer {
                state: rule_state(self.0, InferenceCode::new(tag, TerminalGridSupport)),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TerminalSupportGridPropagatorArgs(pub LayerGridRuleArgs);

impl PropagatorConstructor for TerminalSupportGridPropagatorArgs {
    type PropagatorImpl = TerminalSupportGridPropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let registration = terminal_registration(&self.0);
        let tag = self.0.constraint_tag;
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: TerminalSupportGridPropagator {
                state: rule_state(self.0, InferenceCode::new(tag, TerminalGridSupport)),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct UniqueSupportChainGridPropagatorArgs {
    pub rule: LayerGridRuleArgs,
    pub wake_mode: UniqueSupportChainWakeMode,
}

impl PropagatorConstructor for UniqueSupportChainGridPropagatorArgs {
    type PropagatorImpl = UniqueSupportChainGridPropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let registration = match self.wake_mode {
            UniqueSupportChainWakeMode::AnyDomainEvent => broad_registration(&self.rule),
            UniqueSupportChainWakeMode::SupportLossEvents => support_loss_registration(&self.rule),
        };
        let tag = self.rule.constraint_tag;
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: UniqueSupportChainGridPropagator {
                state: rule_state(self.rule, InferenceCode::new(tag, UniqueSupportChain)),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct DirtyMaterialUniqueSupportChainGridPropagatorArgs(pub LayerGridRuleArgs);

impl PropagatorConstructor for DirtyMaterialUniqueSupportChainGridPropagatorArgs {
    type PropagatorImpl = DirtyMaterialUniqueSupportChainGridPropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let (registration, material_dependencies) = dirty_material_registration(&self.0);
        let dirty_materials = (0..self.0.materials.len()).collect();
        let tag = self.0.constraint_tag;
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: DirtyMaterialUniqueSupportChainGridPropagator {
                state: rule_state(self.0, InferenceCode::new(tag, UniqueSupportChain)),
                material_dependencies,
                dirty_materials,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct WatchedDemandUniqueSupportChainGridPropagatorArgs(pub LayerGridRuleArgs);

impl PropagatorConstructor for WatchedDemandUniqueSupportChainGridPropagatorArgs {
    type PropagatorImpl = WatchedDemandUniqueSupportChainGridPropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        let demand_records = watched_demand_records(&self.0);
        let dirty_demands = (0..demand_records.len()).collect::<BTreeSet<_>>();
        let watchers = (0..self.0.materials.len())
            .map(|_| {
                (0..self.0.cell_count)
                    .map(|_| BTreeSet::new())
                    .collect::<Vec<_>>()
            })
            .collect();
        let (registration, event_impacts) = watched_demand_registration(&self.0);
        let tag = self.0.constraint_tag;
        PropagatorSpec {
            registration,
            checkers: RuntimeCheckers::empty(),
            propagator: WatchedDemandUniqueSupportChainGridPropagator {
                state: rule_state(self.0, InferenceCode::new(tag, UniqueSupportChain)),
                demand_records,
                event_impacts,
                watchers,
                dirty_demands,
            },
        }
    }
}

#[derive(Clone, Debug)]
struct LayerGridRuleState {
    name: String,
    cell_count: usize,
    arcs: Vec<PossibleRouteArc>,
    outgoing_arc_indices: Vec<Vec<usize>>,
    incoming_arc_indices: Vec<Vec<usize>>,
    materials: Vec<LayerGridMaterial>,
    counters: Arc<LayerGridAnalyzerCounters>,
    inference_code: InferenceCode,
}

impl LayerGridRuleState {
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
        let mut reason = PropositionalConjunction::from(selected_demand.lower_bound_predicate(1));
        self.extend_local_support_reason(
            context,
            material,
            demand_cell,
            Some(support_arc_index),
            &mut reason,
        );
        self.counters.maximum_reason_predicates.fetch_max(
            reason.len().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        reason
    }

    fn extend_local_support_reason(
        &self,
        context: &impl ReadDomains,
        material: &LayerGridMaterial,
        required_cell: usize,
        support_arc_index: Option<usize>,
        reason: &mut PropositionalConjunction,
    ) {
        reason.extend(
            self.incoming_arc_indices[required_cell]
                .iter()
                .filter_map(|&arc_index| {
                    if Some(arc_index) == support_arc_index {
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
        );
        reason.extend(
            material
                .supplies
                .iter()
                .filter(|supply| {
                    supply.cell == required_cell && !context.contains(&supply.selected, 1)
                })
                .map(|supply| supply.selected.upper_bound_predicate(0)),
        );
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

            let mut possible_support_count = 0_u8;
            let mut unique_support = None;
            for &arc_index in &self.incoming_arc_indices[demand.cell] {
                let arc = &self.arcs[arc_index];
                if !Self::arc_is_possible(context, arc, material.item_code) {
                    continue;
                }
                possible_support_count += 1;
                if possible_support_count > 1 {
                    break;
                }
                unique_support = Some(arc_index);
            }
            if possible_support_count == 0 {
                let mut reason = PropositionalConjunction::default();
                self.extend_local_support_reason(context, material, demand.cell, None, &mut reason);
                self.counters
                    .forcing_conflicts
                    .fetch_add(1, Ordering::Relaxed);
                context.post(
                    demand.selected.upper_bound_predicate(0),
                    (reason, &self.inference_code),
                )?;
                continue;
            }
            if possible_support_count != 1 {
                continue;
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

    fn propagate_unique_support_chain(
        &self,
        context: &mut PropagationContext,
        material: &LayerGridMaterial,
    ) -> PropagationStatusCP {
        self.counters
            .material_passes
            .fetch_add(1, Ordering::Relaxed);
        for demand in &material.demands {
            self.propagate_unique_support_chain_demand(context, material, demand, None)?;
        }
        Ok(())
    }

    fn propagate_unique_support_chain_demand(
        &self,
        context: &mut PropagationContext,
        material: &LayerGridMaterial,
        demand: &PossibleTerminalOption,
        mut inspected_cells: Option<&mut BTreeSet<usize>>,
    ) -> PropagationStatusCP {
        if context.lower_bound(&demand.selected) != 1 {
            return Ok(());
        }
        self.counters
            .selected_demand_options
            .fetch_add(1, Ordering::Relaxed);
        let mut required_cell = demand.cell;
        let mut visited = vec![false; self.cell_count];
        let mut suffix_reason = PropositionalConjunction::default();
        let mut chain_length = 0_u64;
        loop {
            if let Some(cells) = inspected_cells.as_deref_mut() {
                cells.insert(required_cell);
            }
            if visited[required_cell]
                || material.supplies.iter().any(|supply| {
                    supply.cell == required_cell && context.contains(&supply.selected, 1)
                })
            {
                break;
            }
            visited[required_cell] = true;
            let mut possible_support_count = 0_u8;
            let mut unique_support = None;
            for &arc_index in &self.incoming_arc_indices[required_cell] {
                let arc = &self.arcs[arc_index];
                if !Self::arc_is_possible(context, arc, material.item_code) {
                    continue;
                }
                possible_support_count += 1;
                if possible_support_count > 1 {
                    break;
                }
                unique_support = Some(arc_index);
            }
            if possible_support_count == 0 {
                self.extend_local_support_reason(
                    context,
                    material,
                    required_cell,
                    None,
                    &mut suffix_reason,
                );
                self.counters
                    .forcing_conflicts
                    .fetch_add(1, Ordering::Relaxed);
                context.post(
                    demand.selected.upper_bound_predicate(0),
                    (suffix_reason, &self.inference_code),
                )?;
                break;
            }
            if possible_support_count != 1 {
                break;
            }
            let Some(arc_index) = unique_support else {
                break;
            };
            self.extend_local_support_reason(
                context,
                material,
                required_cell,
                Some(arc_index),
                &mut suffix_reason,
            );
            let mut reason = suffix_reason.clone();
            reason.push(demand.selected.lower_bound_predicate(1));
            self.counters.maximum_reason_predicates.fetch_max(
                reason.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
            chain_length += 1;
            self.counters
                .unique_support_steps
                .fetch_add(1, Ordering::Relaxed);
            if chain_length == 1 {
                self.counters
                    .terminal_support_steps
                    .fetch_add(1, Ordering::Relaxed);
            }
            let arc = &self.arcs[arc_index];
            let unresolved = [
                (arc.selected, 1),
                (arc.from_item, material.item_code),
                (arc.to_item, material.item_code),
            ]
            .into_iter()
            .filter(|(variable, value)| Self::predicate_is_unresolved(context, *variable, *value))
            .collect::<Vec<_>>();
            if !unresolved.is_empty() {
                self.counters
                    .unresolved_predicate_observations
                    .fetch_add(unresolved.len() as u64, Ordering::Relaxed);
                self.counters
                    .distinct_support_arcs
                    .lock()
                    .expect("grid analyzer support-arc counter is not poisoned")
                    .insert((material.item_code, arc.selected));
                self.counters
                    .distinct_unresolved_predicates
                    .lock()
                    .expect("grid analyzer predicate counter is not poisoned")
                    .extend(unresolved.iter().copied());
            }
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
            required_cell = arc.from;
        }
        self.counters
            .maximum_unique_support_chain
            .fetch_max(chain_length, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(super) struct LayerGridOpportunityAnalyzer {
    state: LayerGridRuleState,
}

impl Propagator for LayerGridOpportunityAnalyzer {
    fn name(&self) -> &str {
        &self.state.name
    }

    fn priority(&self) -> Priority {
        Priority::Low
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.state
            .counters
            .executions
            .fetch_add(1, Ordering::Relaxed);
        for material in &self.state.materials {
            self.state.analyze_material(&mut context, material)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(super) struct TerminalSupportGridPropagator {
    state: LayerGridRuleState,
}

impl Propagator for TerminalSupportGridPropagator {
    fn name(&self) -> &str {
        &self.state.name
    }

    fn priority(&self) -> Priority {
        Priority::Low
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.state
            .counters
            .executions
            .fetch_add(1, Ordering::Relaxed);
        for material in &self.state.materials {
            self.state
                .propagate_terminal_support(&mut context, material)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(super) struct UniqueSupportChainGridPropagator {
    state: LayerGridRuleState,
}

#[derive(Clone, Debug)]
pub(super) struct DirtyMaterialUniqueSupportChainGridPropagator {
    state: LayerGridRuleState,
    material_dependencies: Vec<Vec<usize>>,
    dirty_materials: BTreeSet<usize>,
}

impl Propagator for DirtyMaterialUniqueSupportChainGridPropagator {
    fn name(&self) -> &str {
        &self.state.name
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
        self.dirty_materials.extend(
            self.material_dependencies[local_id.unpack() as usize]
                .iter()
                .copied(),
        );
        EnqueueDecision::Enqueue
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.state
            .counters
            .executions
            .fetch_add(1, Ordering::Relaxed);
        for material in &self.state.materials {
            self.state
                .propagate_unique_support_chain(&mut context, material)?;
        }
        Ok(())
    }

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        self.state
            .counters
            .executions
            .fetch_add(1, Ordering::Relaxed);
        let dirty_materials = std::mem::take(&mut self.dirty_materials);
        for material_index in dirty_materials {
            self.state.propagate_unique_support_chain(
                &mut context,
                &self.state.materials[material_index],
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(super) struct WatchedDemandUniqueSupportChainGridPropagator {
    state: LayerGridRuleState,
    demand_records: Vec<WatchedDemandRecord>,
    event_impacts: Vec<WatchedDemandEventImpact>,
    watchers: Vec<Vec<BTreeSet<usize>>>,
    dirty_demands: BTreeSet<usize>,
}

impl Propagator for WatchedDemandUniqueSupportChainGridPropagator {
    fn name(&self) -> &str {
        &self.state.name
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
        self.state
            .counters
            .frontier_notifications
            .fetch_add(1, Ordering::Relaxed);
        let impact = &self.event_impacts[local_id.unpack() as usize];
        let mut relevant = !impact.direct_demands.is_empty();
        self.dirty_demands
            .extend(impact.direct_demands.iter().copied());
        for &(material_index, cell) in &impact.watch_keys {
            let watched = &self.watchers[material_index][cell];
            if !watched.is_empty() {
                relevant = true;
                self.state
                    .counters
                    .frontier_watcher_hits
                    .fetch_add(watched.len() as u64, Ordering::Relaxed);
                self.dirty_demands.extend(watched.iter().copied());
            }
        }
        self.state
            .counters
            .frontier_maximum_dirty_demands
            .fetch_max(
                self.dirty_demands.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        if relevant {
            EnqueueDecision::Enqueue
        } else {
            EnqueueDecision::Skip
        }
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.state
            .counters
            .executions
            .fetch_add(1, Ordering::Relaxed);
        for material in &self.state.materials {
            self.state
                .propagate_unique_support_chain(&mut context, material)?;
        }
        Ok(())
    }

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        self.state
            .counters
            .executions
            .fetch_add(1, Ordering::Relaxed);
        let dirty_demands = std::mem::take(&mut self.dirty_demands);
        self.state
            .counters
            .frontier_maximum_dirty_demands
            .fetch_max(
                dirty_demands.len().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        for demand_id in dirty_demands {
            self.state
                .counters
                .frontier_demand_rechecks
                .fetch_add(1, Ordering::Relaxed);
            let record = self.demand_records[demand_id];
            let mut inspected_cells = BTreeSet::new();
            self.state.propagate_unique_support_chain_demand(
                &mut context,
                &self.state.materials[record.material_index],
                &record.demand,
                Some(&mut inspected_cells),
            )?;
            for cell in inspected_cells {
                if self.watchers[record.material_index][cell].insert(demand_id) {
                    self.state
                        .counters
                        .frontier_watched_cell_registrations
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        Ok(())
    }
}

impl Propagator for UniqueSupportChainGridPropagator {
    fn name(&self) -> &str {
        &self.state.name
    }

    fn priority(&self) -> Priority {
        Priority::Low
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.state
            .counters
            .executions
            .fetch_add(1, Ordering::Relaxed);
        for material in &self.state.materials {
            self.state
                .propagate_unique_support_chain(&mut context, material)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::Solver;
    use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
    use pumpkin_solver::core::branching::Brancher;
    use pumpkin_solver::core::branching::branchers::dynamic_brancher::DynamicBrancher;
    use pumpkin_solver::core::branching::branchers::warm_start::WarmStart;
    use pumpkin_solver::core::results::CSPSolverExecutionFlag;
    use pumpkin_solver::core::results::{ProblemSolution, SatisfactionResult};
    use pumpkin_solver::core::termination::Indefinite;

    use super::*;
    use crate::layouts::integrated::exact::search_statistics::{
        MeteredBrancher, SearchEventCounters, capture_search_statistics,
    };

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
        let _ = solver.add_propagator(LayerGridOpportunityAnalyzerArgs(LayerGridRuleArgs {
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
            constraint_tag: tag,
        }));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert!(statistics.unique_support_steps >= 2);
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
        let _ = solver.add_propagator(TerminalSupportGridPropagatorArgs(LayerGridRuleArgs {
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
            constraint_tag: tag,
        }));

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
        let _ = solver.add_propagator(TerminalSupportGridPropagatorArgs(LayerGridRuleArgs {
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
            constraint_tag: tag,
        }));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&reachable_support, 0));
        assert!(solver.contains(&unreachable_support, 0));
        assert_eq!(counters.snapshot().forced_predicate_attempts, 0);
    }

    #[test]
    fn forces_a_complete_unique_support_chain() {
        let mut solver = Solver::default();
        let upstream = solver.new_bounded_integer(0, 1);
        let terminal = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(UniqueSupportChainGridPropagatorArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-unique-support-chain".to_string(),
                cell_count: 3,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 1,
                        selected: upstream,
                        from_item: item,
                        to_item: item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 2,
                        selected: terminal,
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
                constraint_tag: tag,
            },
            wake_mode: UniqueSupportChainWakeMode::AnyDomainEvent,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&upstream), 1);
        assert_eq!(solver.lower_bound(&terminal), 1);
        let statistics = counters.snapshot();
        assert_eq!(statistics.maximum_unique_support_chain, 2);
        assert!(statistics.forced_predicate_attempts >= 2);
    }

    #[test]
    fn stops_a_unique_support_chain_at_an_interior_branch() {
        let mut solver = Solver::default();
        let first_upstream = solver.new_bounded_integer(0, 1);
        let second_upstream = solver.new_bounded_integer(0, 1);
        let terminal = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(UniqueSupportChainGridPropagatorArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-unique-support-branch".to_string(),
                cell_count: 4,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 2,
                        selected: first_upstream,
                        from_item: item,
                        to_item: item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 2,
                        selected: second_upstream,
                        from_item: item,
                        to_item: item,
                    },
                    PossibleRouteArc {
                        from: 2,
                        to: 3,
                        selected: terminal,
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
                        cell: 3,
                        selected: demand,
                    }],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            wake_mode: UniqueSupportChainWakeMode::AnyDomainEvent,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&terminal), 1);
        assert!(solver.contains(&first_upstream, 0));
        assert!(solver.contains(&second_upstream, 0));
        assert_eq!(counters.snapshot().maximum_unique_support_chain, 1);
    }

    #[test]
    fn selective_chain_wakes_for_demand_selection_but_not_arc_selection() {
        let mut solver = Solver::default();
        let selected = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(0, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(UniqueSupportChainGridPropagatorArgs {
            rule: LayerGridRuleArgs {
                name: "controlled-selective-chain-wakeup".to_string(),
                cell_count: 2,
                arcs: vec![PossibleRouteArc {
                    from: 0,
                    to: 1,
                    selected,
                    from_item: item,
                    to_item: item,
                }],
                materials: vec![LayerGridMaterial {
                    item_code: 1,
                    supplies: vec![PossibleTerminalOption {
                        cell: 0,
                        selected: supply,
                    }],
                    demands: vec![PossibleTerminalOption {
                        cell: 1,
                        selected: demand,
                    }],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
            wake_mode: UniqueSupportChainWakeMode::SupportLossEvents,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let initial_executions = counters.snapshot().executions;
        solver.add_clause([selected.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let after_arc_selection = counters.snapshot().executions;
        #[cfg(not(feature = "pumpkin-debug-checks"))]
        assert_eq!(after_arc_selection, initial_executions);
        #[cfg(feature = "pumpkin-debug-checks")]
        assert!(after_arc_selection <= initial_executions + 1);

        solver.add_clause([demand.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(counters.snapshot().executions > after_arc_selection);
    }

    #[cfg(not(feature = "pumpkin-debug-checks"))]
    #[test]
    fn dirty_material_chain_rechecks_only_the_notified_material() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 2);
        let first_supply = solver.new_bounded_integer(1, 1);
        let second_supply = solver.new_bounded_integer(1, 1);
        let first_demand = solver.new_bounded_integer(0, 1);
        let second_demand = solver.new_bounded_integer(0, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(DirtyMaterialUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-dirty-material-chain".to_string(),
                cell_count: 3,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 2,
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
                materials: vec![
                    LayerGridMaterial {
                        item_code: 1,
                        supplies: vec![PossibleTerminalOption {
                            cell: 0,
                            selected: first_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 2,
                            selected: first_demand,
                        }],
                    },
                    LayerGridMaterial {
                        item_code: 2,
                        supplies: vec![PossibleTerminalOption {
                            cell: 1,
                            selected: second_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 2,
                            selected: second_demand,
                        }],
                    },
                ],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let initial = counters.snapshot();
        assert_eq!(initial.material_passes, 2);

        solver.add_clause([first_demand.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let after = counters.snapshot();
        assert_eq!(after.material_passes, initial.material_passes + 1);
        assert_eq!(after.executions, initial.executions + 1);
        assert!(solver.contains(&second_demand, 0));
    }

    #[test]
    fn dirty_material_chain_rechecks_cross_material_item_loss() {
        let mut solver = Solver::default();
        let shared_arc = solver.new_bounded_integer(1, 1);
        let material_two_arc = solver.new_bounded_integer(0, 1);
        let shared_item = solver.new_bounded_integer(1, 2);
        let material_two_item = solver.new_bounded_integer(2, 2);
        let first_supply = solver.new_bounded_integer(1, 1);
        let second_supply = solver.new_bounded_integer(1, 1);
        let first_demand = solver.new_bounded_integer(0, 1);
        let second_demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(DirtyMaterialUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-dirty-cross-material-chain".to_string(),
                cell_count: 3,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 2,
                        selected: shared_arc,
                        from_item: shared_item,
                        to_item: shared_item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 2,
                        selected: material_two_arc,
                        from_item: material_two_item,
                        to_item: material_two_item,
                    },
                ],
                materials: vec![
                    LayerGridMaterial {
                        item_code: 1,
                        supplies: vec![PossibleTerminalOption {
                            cell: 0,
                            selected: first_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 2,
                            selected: first_demand,
                        }],
                    },
                    LayerGridMaterial {
                        item_code: 2,
                        supplies: vec![PossibleTerminalOption {
                            cell: 1,
                            selected: second_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 2,
                            selected: second_demand,
                        }],
                    },
                ],
                counters,
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&material_two_arc, 0));

        solver.add_clause([first_demand.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&shared_item), 1);
        assert_eq!(solver.upper_bound(&shared_item), 1);
        assert_eq!(solver.lower_bound(&material_two_arc), 1);
    }

    #[test]
    fn watched_demand_chain_rechecks_when_an_incoming_branch_disappears() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let first_supply = solver.new_bounded_integer(1, 1);
        let second_supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-watched-demand-branch".to_string(),
                cell_count: 3,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 2,
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
                    supplies: vec![
                        PossibleTerminalOption {
                            cell: 0,
                            selected: first_supply,
                        },
                        PossibleTerminalOption {
                            cell: 1,
                            selected: second_supply,
                        },
                    ],
                    demands: vec![PossibleTerminalOption {
                        cell: 2,
                        selected: demand,
                    }],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&second, 0));
        let initial = counters.snapshot();
        assert!(initial.frontier_watched_cell_registrations >= 1);

        solver.add_clause([first.upper_bound_predicate(0)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&second), 1);
        let after = counters.snapshot();
        assert!(after.frontier_watcher_hits > initial.frontier_watcher_hits);
        assert!(after.frontier_demand_rechecks > initial.frontier_demand_rechecks);
    }

    #[test]
    fn watched_demand_chain_rechecks_when_a_local_supply_disappears() {
        let mut solver = Solver::default();
        let incoming = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let upstream_supply = solver.new_bounded_integer(1, 1);
        let local_supply = solver.new_bounded_integer(0, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-watched-demand-supply".to_string(),
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
                    supplies: vec![
                        PossibleTerminalOption {
                            cell: 0,
                            selected: upstream_supply,
                        },
                        PossibleTerminalOption {
                            cell: 1,
                            selected: local_supply,
                        },
                    ],
                    demands: vec![PossibleTerminalOption {
                        cell: 1,
                        selected: demand,
                    }],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&incoming, 0));
        solver.add_clause([local_supply.upper_bound_predicate(0)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&incoming), 1);
        let statistics = counters.snapshot();
        assert!(statistics.frontier_watcher_hits >= 1);
        assert!(statistics.frontier_demand_rechecks >= 2);
    }

    #[test]
    fn watched_demand_chain_rechecks_cross_material_item_loss() {
        let mut solver = Solver::default();
        let shared_arc = solver.new_bounded_integer(1, 1);
        let material_two_arc = solver.new_bounded_integer(0, 1);
        let shared_item = solver.new_bounded_integer(1, 2);
        let material_two_item = solver.new_bounded_integer(2, 2);
        let first_supply = solver.new_bounded_integer(1, 1);
        let second_supply = solver.new_bounded_integer(1, 1);
        let first_demand = solver.new_bounded_integer(0, 1);
        let second_demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-watched-cross-material-chain".to_string(),
                cell_count: 3,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 2,
                        selected: shared_arc,
                        from_item: shared_item,
                        to_item: shared_item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 2,
                        selected: material_two_arc,
                        from_item: material_two_item,
                        to_item: material_two_item,
                    },
                ],
                materials: vec![
                    LayerGridMaterial {
                        item_code: 1,
                        supplies: vec![PossibleTerminalOption {
                            cell: 0,
                            selected: first_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 2,
                            selected: first_demand,
                        }],
                    },
                    LayerGridMaterial {
                        item_code: 2,
                        supplies: vec![PossibleTerminalOption {
                            cell: 1,
                            selected: second_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 2,
                            selected: second_demand,
                        }],
                    },
                ],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert!(solver.contains(&material_two_arc, 0));
        solver.add_clause([first_demand.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&shared_item), 1);
        assert_eq!(solver.upper_bound(&shared_item), 1);
        assert_eq!(solver.lower_bound(&material_two_arc), 1);
        assert!(counters.snapshot().frontier_watcher_hits >= 1);
    }

    #[test]
    fn watched_demand_chain_rechecks_an_interior_item_value_loss() {
        let mut solver = Solver::default();
        let shared_arc = solver.new_bounded_integer(1, 1);
        let alternative_arc = solver.new_bounded_integer(0, 1);
        let shared_item = solver.new_bounded_integer(1, 3);
        let material_two_item = solver.new_bounded_integer(2, 2);
        let first_supply = solver.new_bounded_integer(1, 1);
        let second_supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-watched-interior-item-loss".to_string(),
                cell_count: 3,
                arcs: vec![
                    PossibleRouteArc {
                        from: 0,
                        to: 2,
                        selected: shared_arc,
                        from_item: shared_item,
                        to_item: shared_item,
                    },
                    PossibleRouteArc {
                        from: 1,
                        to: 2,
                        selected: alternative_arc,
                        from_item: material_two_item,
                        to_item: material_two_item,
                    },
                ],
                materials: vec![LayerGridMaterial {
                    item_code: 2,
                    supplies: vec![
                        PossibleTerminalOption {
                            cell: 0,
                            selected: first_supply,
                        },
                        PossibleTerminalOption {
                            cell: 1,
                            selected: second_supply,
                        },
                    ],
                    demands: vec![PossibleTerminalOption {
                        cell: 2,
                        selected: demand,
                    }],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&shared_item), 1);
        assert_eq!(solver.upper_bound(&shared_item), 3);
        assert!(solver.contains(&alternative_arc, 0));

        solver.add_clause([shared_item.disequality_predicate(2)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&alternative_arc), 1);
        assert!(counters.snapshot().frontier_watcher_hits >= 1);
    }

    #[test]
    fn watched_demand_chain_stops_safely_at_a_cycle() {
        let mut solver = Solver::default();
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-watched-cycle".to_string(),
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
                    demands: vec![PossibleTerminalOption {
                        cell: 0,
                        selected: demand,
                    }],
                }],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&first), 1);
        assert_eq!(solver.lower_bound(&second), 1);
        let statistics = counters.snapshot();
        assert_eq!(statistics.maximum_unique_support_chain, 2);
        assert_eq!(statistics.frontier_watched_cell_registrations, 2);
    }

    fn solve_chain_backtracking_fixture(watched: bool) -> (i32, i32, u64) {
        let mut solver = Solver::default();
        let choice = solver.new_bounded_integer(0, 1);
        let first = solver.new_bounded_integer(0, 1);
        let second = solver.new_bounded_integer(0, 1);
        let item = solver.new_bounded_integer(1, 1);
        let first_supply = solver.new_bounded_integer(1, 1);
        let second_supply = solver.new_bounded_integer(1, 1);
        let demand = solver.new_bounded_integer(1, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let args = LayerGridRuleArgs {
            name: "controlled-chain-backtracking".to_string(),
            cell_count: 3,
            arcs: vec![
                PossibleRouteArc {
                    from: 0,
                    to: 2,
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
                supplies: vec![
                    PossibleTerminalOption {
                        cell: 0,
                        selected: first_supply,
                    },
                    PossibleTerminalOption {
                        cell: 1,
                        selected: second_supply,
                    },
                ],
                demands: vec![PossibleTerminalOption {
                    cell: 2,
                    selected: demand,
                }],
            }],
            counters,
            constraint_tag: tag,
        };
        if watched {
            let _ = solver.add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(args));
        } else {
            let _ = solver.add_propagator(UniqueSupportChainGridPropagatorArgs {
                rule: args,
                wake_mode: UniqueSupportChainWakeMode::AnyDomainEvent,
            });
        }
        solver.add_clause(
            [
                choice.upper_bound_predicate(0),
                first.upper_bound_predicate(0),
            ],
            tag,
        );
        solver.add_clause(
            [
                choice.upper_bound_predicate(0),
                second.upper_bound_predicate(0),
            ],
            tag,
        );

        let search_counters = Arc::new(Mutex::new(SearchEventCounters::default()));
        let branchers: Vec<Box<dyn Brancher>> = vec![
            Box::new(WarmStart::new(&[choice], &[1])),
            Box::new(solver.default_brancher()),
        ];
        let mut brancher = MeteredBrancher::new(
            DynamicBrancher::new(branchers),
            Arc::clone(&search_counters),
        );
        let mut resolver = ResolutionResolver::default();
        match solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver) {
            SatisfactionResult::Satisfiable(result) => {
                let solution = result.solution();
                let values = (
                    solution.get_integer_value(first),
                    solution.get_integer_value(second),
                );
                let statistics = capture_search_statistics(
                    result.solver(),
                    result.brancher(),
                    result.conflict_resolver(),
                    &search_counters,
                );
                (
                    values.0,
                    values.1,
                    statistics.backtracks.unwrap_or_default(),
                )
            }
            _ => panic!("controlled chain backtracking fixture must be satisfiable"),
        }
    }

    #[test]
    fn watched_demand_chain_matches_the_broad_chain_through_search_backtracking() {
        let broad = solve_chain_backtracking_fixture(false);
        let watched = solve_chain_backtracking_fixture(true);
        assert_eq!(broad, watched);
        assert!(watched.0 + watched.1 >= 1);
        assert!(watched.2 >= 1);
    }

    #[cfg(not(feature = "pumpkin-debug-checks"))]
    #[test]
    fn watched_demand_chain_rechecks_only_the_directly_selected_demand() {
        let mut solver = Solver::default();
        let first_supply = solver.new_bounded_integer(1, 1);
        let second_supply = solver.new_bounded_integer(1, 1);
        let first_demand = solver.new_bounded_integer(0, 1);
        let second_demand = solver.new_bounded_integer(0, 1);
        let counters = Arc::new(LayerGridAnalyzerCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(
            LayerGridRuleArgs {
                name: "controlled-watched-demand-locality".to_string(),
                cell_count: 2,
                arcs: vec![],
                materials: vec![
                    LayerGridMaterial {
                        item_code: 1,
                        supplies: vec![PossibleTerminalOption {
                            cell: 0,
                            selected: first_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 0,
                            selected: first_demand,
                        }],
                    },
                    LayerGridMaterial {
                        item_code: 2,
                        supplies: vec![PossibleTerminalOption {
                            cell: 1,
                            selected: second_supply,
                        }],
                        demands: vec![PossibleTerminalOption {
                            cell: 1,
                            selected: second_demand,
                        }],
                    },
                ],
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            },
        ));

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let initial = counters.snapshot();
        solver.add_clause([first_demand.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let after = counters.snapshot();
        assert_eq!(
            after.frontier_demand_rechecks,
            initial.frontier_demand_rechecks + 1
        );
        assert_eq!(after.material_passes, initial.material_passes);
        assert!(solver.contains(&second_demand, 0));
    }
}
