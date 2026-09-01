use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Instant;

use pumpkin_solver::core::branching::{Brancher, BrancherEvent, SelectionContext};
use pumpkin_solver::core::predicates::Predicate;
use pumpkin_solver::core::results::SolutionReference;
use pumpkin_solver::core::statistics::StatisticLogger;
use pumpkin_solver::core::variables::DomainId;

use super::PortModel;
use crate::layouts::integrated::exact::ladder::{
    BOTTOM_UP_SEARCH_PROVENANCE_SCHEMA_VERSION, BottomUpProvenanceDecision,
    BottomUpProvenanceFamilyCheckpoint, BottomUpProvenanceFamilyDomainState,
    BottomUpProvenanceTargetState, BottomUpProvenanceTargetTransition,
    BottomUpSearchProvenanceTrace,
};
use crate::layouts::integrated::exact::recorder::RecordedVariableDescriptor;

const MAX_TARGET_TRANSITIONS: usize = 1_024;

#[derive(Debug, Clone)]
struct EndpointProbe {
    port_choice: DomainId,
    local_key: DomainId,
    connection_x: DomainId,
    connection_y: DomainId,
}

#[derive(Debug, Clone)]
pub(super) struct SearchProvenanceProbe {
    x: DomainId,
    y: DomainId,
    rotation: DomainId,
    rotation_values: Vec<i32>,
    endpoints: Vec<EndpointProbe>,
    variable_catalog: Vec<RecordedVariableDescriptor>,
    variable_by_domain: BTreeMap<DomainId, RecordedVariableDescriptor>,
}

impl SearchProvenanceProbe {
    pub(super) fn new(model: &PortModel, target_instance: &str) -> Self {
        let facility = model
            .placement
            .instances
            .iter()
            .find(|facility| facility.id == target_instance)
            .expect("validated provenance target exists in the modeled phase");
        let mut rotation_values = facility
            .orientations
            .iter()
            .flat_map(|orientation| orientation.equivalent_rotations.iter().copied())
            .map(|rotation| i32::try_from(rotation).expect("validated rotation fits i32"))
            .collect::<Vec<_>>();
        rotation_values.sort_unstable();
        rotation_values.dedup();
        let endpoints = model
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.instance == target_instance)
            .map(|endpoint| EndpointProbe {
                port_choice: endpoint.port_choice,
                local_key: endpoint.local_key,
                connection_x: endpoint.connection_x,
                connection_y: endpoint.connection_y,
            })
            .collect();
        let variable_catalog = model.placement.model.variable_catalog();
        let variable_by_domain = variable_catalog
            .iter()
            .cloned()
            .map(|descriptor| (descriptor.domain, descriptor))
            .collect();
        Self {
            x: facility.x,
            y: facility.y,
            rotation: model.rotations[target_instance],
            rotation_values,
            endpoints,
            variable_catalog,
            variable_by_domain,
        }
    }

    fn rotation_values(&self, context: &SelectionContext, contains_checks: &mut u64) -> Vec<i32> {
        self.rotation_values
            .iter()
            .copied()
            .filter(|value| {
                *contains_checks += 1;
                context.contains(self.rotation, *value)
            })
            .collect()
    }

    fn target_state(
        &self,
        context: &SelectionContext,
        rotation_values: Vec<i32>,
        contains_checks: &mut u64,
    ) -> BottomUpProvenanceTargetState {
        BottomUpProvenanceTargetState {
            rotation_values,
            x_cardinality: exact_cardinality(context, self.x, None, contains_checks),
            y_cardinality: exact_cardinality(context, self.y, None, contains_checks),
            endpoint_count: self.endpoints.len(),
            port_cardinality_sum: self
                .endpoints
                .iter()
                .map(|endpoint| {
                    exact_cardinality(context, endpoint.port_choice, None, contains_checks)
                })
                .sum(),
            local_key_cardinality_sum: self
                .endpoints
                .iter()
                .map(|endpoint| {
                    exact_cardinality(context, endpoint.local_key, None, contains_checks)
                })
                .sum(),
            connection_x_cardinality_sum: self
                .endpoints
                .iter()
                .map(|endpoint| {
                    exact_cardinality(context, endpoint.connection_x, None, contains_checks)
                })
                .sum(),
            connection_y_cardinality_sum: self
                .endpoints
                .iter()
                .map(|endpoint| {
                    exact_cardinality(context, endpoint.connection_y, None, contains_checks)
                })
                .sum(),
        }
    }

    fn family_state(
        &self,
        context: &SelectionContext,
        contains_checks: &mut u64,
    ) -> Vec<BottomUpProvenanceFamilyDomainState> {
        let mut families = BTreeMap::<String, BottomUpProvenanceFamilyDomainState>::new();
        for descriptor in &self.variable_catalog {
            let cardinality = exact_cardinality(
                context,
                descriptor.domain,
                Some((
                    descriptor.declared_lower_bound,
                    descriptor.declared_upper_bound,
                )),
                contains_checks,
            );
            let family = descriptor.family.name().to_string();
            let state = families.entry(family.clone()).or_insert_with(|| {
                BottomUpProvenanceFamilyDomainState {
                    family,
                    total: 0,
                    fixed: 0,
                    unresolved: 0,
                    current_cardinality_sum: 0,
                    declared_cardinality_sum: 0,
                }
            });
            state.total += 1;
            if cardinality == 1 {
                state.fixed += 1;
            } else {
                state.unresolved += 1;
            }
            state.current_cardinality_sum += cardinality as u64;
            state.declared_cardinality_sum += descriptor.declared_cardinality;
        }
        families.into_values().collect()
    }
}

pub(super) type SearchProvenanceCollector = Rc<RefCell<BottomUpSearchProvenanceTrace>>;

pub(super) fn collector(
    target_instance: &str,
    maximum_detailed_decisions: usize,
) -> SearchProvenanceCollector {
    Rc::new(RefCell::new(BottomUpSearchProvenanceTrace {
        schema_version: BOTTOM_UP_SEARCH_PROVENANCE_SCHEMA_VERSION,
        target_instance: target_instance.to_string(),
        maximum_detailed_decisions,
        decision_requests: 0,
        decisions: 0,
        conflict_callbacks: 0,
        backtrack_callbacks: 0,
        observed_restart_callbacks: 0,
        observer_contains_checks: 0,
        target_rotation_decisions: 0,
        unrecorded_decisions: 0,
        non_singleton_rotation_requests: 0,
        singleton_rotation_requests: BTreeMap::new(),
        singleton_rotation_entries: BTreeMap::new(),
        first_singleton_decision: BTreeMap::new(),
        rotation_widening_transitions: 0,
        detailed_decisions_truncated: false,
        target_transitions_seen: 0,
        target_transitions_dropped: 0,
        decision_histogram_matches_total: true,
        decision_catalog_covers_all: true,
        decision_family_counts: BTreeMap::new(),
        detailed_decisions: Vec::new(),
        target_transitions: Vec::new(),
        family_checkpoints: Vec::new(),
    }))
}

pub(super) fn finish(collector: &SearchProvenanceCollector) -> BottomUpSearchProvenanceTrace {
    let mut trace = collector.borrow().clone();
    trace.decision_histogram_matches_total =
        trace.decision_family_counts.values().sum::<u64>() == trace.decisions;
    trace.decision_catalog_covers_all = trace.unrecorded_decisions == 0;
    trace
}

#[derive(Debug)]
pub(super) struct SearchProvenanceBrancher<B> {
    inner: B,
    inner_events: Vec<BrancherEvent>,
    probe: SearchProvenanceProbe,
    collector: SearchProvenanceCollector,
    started: Instant,
    previous_rotation_values: Option<Vec<i32>>,
    last_decision_domain: Option<DomainId>,
    last_observed_backtracks: u64,
    first_singleton_checkpoints: BTreeSet<i32>,
    checkpoint_ordinals: BTreeSet<u64>,
}

impl<B: Brancher> SearchProvenanceBrancher<B> {
    pub(super) fn new(
        inner: B,
        probe: SearchProvenanceProbe,
        collector: SearchProvenanceCollector,
    ) -> Self {
        let inner_events = inner.subscribe_to_events();
        Self {
            inner,
            inner_events,
            probe,
            collector,
            started: Instant::now(),
            previous_rotation_values: None,
            last_decision_domain: None,
            last_observed_backtracks: 0,
            first_singleton_checkpoints: BTreeSet::new(),
            checkpoint_ordinals: BTreeSet::new(),
        }
    }

    fn elapsed_us(&self) -> u64 {
        self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
    }

    fn observe_fixpoint(&mut self, context: &SelectionContext) {
        let mut contains_checks = 0;
        let rotation_values = self.probe.rotation_values(context, &mut contains_checks);
        let changed = self
            .previous_rotation_values
            .as_ref()
            .is_none_or(|previous| *previous != rotation_values);
        let decisions_before = self.collector.borrow().decisions;

        {
            let mut trace = self.collector.borrow_mut();
            trace.decision_requests += 1;
            if let [rotation] = rotation_values.as_slice() {
                *trace
                    .singleton_rotation_requests
                    .entry(*rotation)
                    .or_default() += 1;
            } else {
                trace.non_singleton_rotation_requests += 1;
            }
        }

        let mut checkpoint_trigger = None;
        if decisions_before == 0 || decisions_before.is_power_of_two() {
            checkpoint_trigger = Some(if decisions_before == 0 {
                "pre-first-branch-fixpoint".to_string()
            } else {
                "geometric-decision-sample".to_string()
            });
        }

        if changed {
            let previous_len = self.previous_rotation_values.as_ref().map_or(0, Vec::len);
            let origin = if self.previous_rotation_values.is_none() {
                "pre-first-branch-fixpoint"
            } else if self.collector.borrow().backtrack_callbacks > self.last_observed_backtracks {
                "post-conflict-or-backtrack-fixpoint"
            } else if self.last_decision_domain == Some(self.probe.rotation) {
                "direct-rotation-decision"
            } else {
                "propagated-from-other-decision"
            };
            let state =
                self.probe
                    .target_state(context, rotation_values.clone(), &mut contains_checks);
            let elapsed_us = self.elapsed_us();
            let mut trace = self.collector.borrow_mut();
            trace.target_transitions_seen += 1;
            if previous_len == 1 && rotation_values.len() > 1 {
                trace.rotation_widening_transitions += 1;
            }
            if let [rotation] = rotation_values.as_slice() {
                *trace
                    .singleton_rotation_entries
                    .entry(*rotation)
                    .or_default() += 1;
                trace
                    .first_singleton_decision
                    .entry(*rotation)
                    .or_insert(decisions_before);
                if self.first_singleton_checkpoints.insert(*rotation)
                    && checkpoint_trigger.is_none()
                {
                    checkpoint_trigger = Some(format!("first-singleton-{rotation}"));
                }
            }
            if trace.target_transitions.len() < MAX_TARGET_TRANSITIONS {
                let observation_index = trace.target_transitions_seen - 1;
                let conflict_callbacks = trace.conflict_callbacks;
                let backtrack_callbacks = trace.backtrack_callbacks;
                let observed_restart_callbacks = trace.observed_restart_callbacks;
                trace
                    .target_transitions
                    .push(BottomUpProvenanceTargetTransition {
                        observation_index,
                        decisions_before,
                        conflict_callbacks,
                        backtrack_callbacks,
                        observed_restart_callbacks,
                        elapsed_us,
                        origin: origin.to_string(),
                        state,
                    });
            } else {
                trace.target_transitions_dropped += 1;
            }
        }

        if let Some(trigger) = checkpoint_trigger
            && self.checkpoint_ordinals.insert(decisions_before)
        {
            let target =
                self.probe
                    .target_state(context, rotation_values.clone(), &mut contains_checks);
            let variable_families = self.probe.family_state(context, &mut contains_checks);
            let elapsed_us = self.elapsed_us();
            let trace = &mut *self.collector.borrow_mut();
            trace
                .family_checkpoints
                .push(BottomUpProvenanceFamilyCheckpoint {
                    trigger,
                    decisions_before,
                    conflict_callbacks: trace.conflict_callbacks,
                    backtrack_callbacks: trace.backtrack_callbacks,
                    observed_restart_callbacks: trace.observed_restart_callbacks,
                    elapsed_us,
                    target,
                    variable_families,
                });
        }

        self.collector.borrow_mut().observer_contains_checks += contains_checks;
        self.previous_rotation_values = Some(rotation_values);
        self.last_observed_backtracks = self.collector.borrow().backtrack_callbacks;
    }

    fn record_decision(&mut self, context: &SelectionContext, decision: Predicate) {
        let domain = decision.get_domain();
        let descriptor = self.probe.variable_by_domain.get(&domain);
        let family = descriptor
            .map(|descriptor| descriptor.family.name())
            .unwrap_or("unrecorded")
            .to_string();
        let mut contains_checks = 0;
        let record_detail = {
            let trace = self.collector.borrow();
            trace.detailed_decisions.len() < trace.maximum_detailed_decisions
        };
        let detail = record_detail.then(|| {
            let cardinality = exact_cardinality(
                context,
                domain,
                descriptor.map(|descriptor| {
                    (
                        descriptor.declared_lower_bound,
                        descriptor.declared_upper_bound,
                    )
                }),
                &mut contains_checks,
            );
            (self.elapsed_us(), cardinality)
        });
        let mut trace = self.collector.borrow_mut();
        let decision_index = trace.decisions;
        trace.decisions += 1;
        if domain == self.probe.rotation {
            trace.target_rotation_decisions += 1;
        }
        if descriptor.is_none() {
            trace.unrecorded_decisions += 1;
        }
        trace.observer_contains_checks += contains_checks;
        *trace
            .decision_family_counts
            .entry(family.clone())
            .or_default() += 1;
        if let Some((elapsed_us, cardinality)) = detail {
            trace.detailed_decisions.push(BottomUpProvenanceDecision {
                decision_index,
                elapsed_us,
                variable_family: family,
                variable_name: descriptor.map(|descriptor| descriptor.name.clone()),
                domain_id: domain.id(),
                relation: predicate_relation(decision).to_string(),
                right_hand_side: decision.get_right_hand_side(),
                domain_cardinality_before: cardinality,
                target_rotation_values_before: self
                    .previous_rotation_values
                    .clone()
                    .unwrap_or_default(),
            });
        } else {
            trace.detailed_decisions_truncated = true;
        }
        self.last_decision_domain = Some(domain);
    }

    fn inner_subscribed(&self, event: BrancherEvent) -> bool {
        self.inner_events.contains(&event)
    }
}

impl<B: Brancher> Brancher for SearchProvenanceBrancher<B> {
    fn log_statistics(&self, statistic_logger: StatisticLogger) {
        self.inner.log_statistics(statistic_logger);
    }

    fn next_decision(&mut self, context: &mut SelectionContext) -> Option<Predicate> {
        self.observe_fixpoint(context);
        let decision = self.inner.next_decision(context);
        if let Some(decision) = decision {
            self.record_decision(context, decision);
        }
        decision
    }

    fn on_conflict(&mut self) {
        self.collector.borrow_mut().conflict_callbacks += 1;
        if self.inner_subscribed(BrancherEvent::Conflict) {
            self.inner.on_conflict();
        }
    }

    fn on_backtrack(&mut self) {
        self.collector.borrow_mut().backtrack_callbacks += 1;
        if self.inner_subscribed(BrancherEvent::Backtrack) {
            self.inner.on_backtrack();
        }
    }

    fn on_solution(&mut self, solution: SolutionReference) {
        self.inner.on_solution(solution);
    }

    fn on_unassign_integer(&mut self, variable: DomainId, value: i32) {
        self.inner.on_unassign_integer(variable, value);
    }

    fn on_appearance_in_conflict_predicate(&mut self, predicate: Predicate) {
        self.inner.on_appearance_in_conflict_predicate(predicate);
    }

    fn on_restart(&mut self) {
        self.collector.borrow_mut().observed_restart_callbacks += 1;
        if self.inner_subscribed(BrancherEvent::Restart) {
            self.inner.on_restart();
        }
    }

    fn synchronise(&mut self, context: &mut SelectionContext) {
        self.inner.synchronise(context);
    }

    fn is_restart_pointless(&mut self) -> bool {
        self.inner.is_restart_pointless()
    }

    fn subscribe_to_events(&self) -> Vec<BrancherEvent> {
        let mut events = self.inner_events.clone();
        for event in [
            BrancherEvent::Conflict,
            BrancherEvent::Backtrack,
            BrancherEvent::Restart,
        ] {
            if !events.contains(&event) {
                events.push(event);
            }
        }
        events
    }
}

fn exact_cardinality(
    context: &SelectionContext,
    domain: DomainId,
    declared_bounds: Option<(i32, i32)>,
    contains_checks: &mut u64,
) -> usize {
    let (lower, upper) = declared_bounds
        .unwrap_or_else(|| (context.lower_bound(domain), context.upper_bound(domain)));
    (lower..=upper)
        .filter(|value| {
            *contains_checks += 1;
            context.contains(domain, *value)
        })
        .count()
}

fn predicate_relation(predicate: Predicate) -> &'static str {
    if predicate.is_lower_bound_predicate() {
        "greater-than-or-equal"
    } else if predicate.is_upper_bound_predicate() {
        "less-than-or-equal"
    } else if predicate.is_equality_predicate() {
        "equal"
    } else if predicate.is_not_equal_predicate() {
        "not-equal"
    } else {
        "unknown"
    }
}
