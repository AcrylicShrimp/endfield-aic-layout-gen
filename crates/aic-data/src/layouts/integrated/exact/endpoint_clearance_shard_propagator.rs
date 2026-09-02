use std::sync::Arc;

use pumpkin_solver::core::proof::ConstraintTag;
use pumpkin_solver::core::propagation::{
    DomainEvents, EnqueueDecision, EventsToRegister, LocalId, NotificationContext,
    OpaqueDomainEvent, Priority, PropagationContext, Propagator, PropagatorConstructor,
    PropagatorConstructorContext, PropagatorSpec, ReadDomains, RuntimeCheckers,
};
use pumpkin_solver::core::state::PropagationStatusCP;
use pumpkin_solver::core::variables::DomainId;

use super::endpoint_clearance_propagator::{
    EndpointClearanceFullShardCauses, EndpointClearanceNotificationAxis,
    EndpointClearanceOrientation, EndpointClearancePropagationCounters,
    EndpointClearanceRelationCounters, EndpointRectangleClearancePropagator, ExecutionTrigger,
};

const COORDINATE_TRIGGER: u8 = 1 << 0;
const ORIENTATION_TRIGGER: u8 = 1 << 1;
const FACILITY_X_CAUSE: u8 = 1 << 0;
const FACILITY_Y_CAUSE: u8 = 1 << 1;
const ORIENTATION_CAUSE: u8 = 1 << 2;

fn decode_full_shard_causes(mask: u8) -> EndpointClearanceFullShardCauses {
    EndpointClearanceFullShardCauses {
        initial_execution: false,
        facility_x: mask & FACILITY_X_CAUSE != 0,
        facility_y: mask & FACILITY_Y_CAUSE != 0,
        orientation: mask & ORIENTATION_CAUSE != 0,
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated) struct EndpointClearanceShardRelationArgs {
    pub terminal: String,
    pub connection_x: DomainId,
    pub connection_y: DomainId,
    pub relation_counters: Option<Arc<EndpointClearanceRelationCounters>>,
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated) struct TargetFacilityEndpointClearanceShardPropagatorArgs {
    pub name: String,
    pub target_facility: String,
    pub facility_x: DomainId,
    pub facility_y: DomainId,
    pub orientations: Vec<EndpointClearanceOrientation>,
    pub relations: Vec<EndpointClearanceShardRelationArgs>,
    pub priority: Priority,
    pub counters: Arc<EndpointClearancePropagationCounters>,
    pub false_event_filter_enabled: bool,
    pub constraint_tag: ConstraintTag,
}

#[derive(Clone, Copy, Debug)]
enum ShardEvent {
    FacilityX,
    FacilityY,
    Orientation(usize),
    ConnectionX(usize),
    ConnectionY(usize),
}

impl PropagatorConstructor for TargetFacilityEndpointClearanceShardPropagatorArgs {
    type PropagatorImpl = TargetFacilityEndpointClearanceShardPropagator;

    fn create(
        self,
        _context: PropagatorConstructorContext,
    ) -> PropagatorSpec<Self::PropagatorImpl> {
        assert!(!self.orientations.is_empty());
        assert!(!self.relations.is_empty());
        for orientation in &self.orientations {
            assert!(orientation.width > 0);
            assert!(orientation.height > 0);
        }

        let mut registration = EventsToRegister::builder()
            .add(&self.facility_x, DomainEvents::BOUNDS, LocalId::from(0))
            .add(&self.facility_y, DomainEvents::BOUNDS, LocalId::from(1));
        let mut events = vec![ShardEvent::FacilityX, ShardEvent::FacilityY];
        for (orientation_index, orientation) in self.orientations.iter().enumerate() {
            let local_index = events.len();
            registration = registration.add(
                &orientation.selected_parent,
                DomainEvents::ANY_INT,
                LocalId::from(u32::try_from(local_index).expect("shard watcher count fits u32")),
            );
            events.push(ShardEvent::Orientation(orientation_index));
        }
        for (relation_index, relation) in self.relations.iter().enumerate() {
            for (variable, event) in [
                (
                    relation.connection_x,
                    ShardEvent::ConnectionX(relation_index),
                ),
                (
                    relation.connection_y,
                    ShardEvent::ConnectionY(relation_index),
                ),
            ] {
                let local_index = events.len();
                registration = registration.add(
                    &variable,
                    DomainEvents::BOUNDS,
                    LocalId::from(
                        u32::try_from(local_index).expect("shard watcher count fits u32"),
                    ),
                );
                events.push(event);
            }
        }

        let relation_count = self.relations.len();
        let relation_kernels = self
            .relations
            .into_iter()
            .map(|relation| {
                EndpointRectangleClearancePropagator::new_relation_kernel(
                    format!(
                        "terminal:{}:outside-facility:{}:sharded-point-rectangle-clearance",
                        relation.terminal, self.target_facility
                    ),
                    relation.connection_x,
                    relation.connection_y,
                    self.facility_x,
                    self.facility_y,
                    self.orientations.clone(),
                    self.priority,
                    Arc::clone(&self.counters),
                    self.false_event_filter_enabled,
                    relation.relation_counters,
                    self.constraint_tag,
                )
            })
            .collect::<Vec<_>>();
        self.counters
            .note_shard_registration(self.orientations.len(), relation_count);

        PropagatorSpec {
            registration: registration.build(),
            checkers: RuntimeCheckers::empty(),
            propagator: TargetFacilityEndpointClearanceShardPropagator {
                name: self.name,
                orientations: self.orientations,
                relation_kernels,
                priority: self.priority,
                counters: self.counters,
                false_event_filter_enabled: self.false_event_filter_enabled,
                events,
                pending_full: true,
                pending_initial_execution: true,
                pending_full_trigger_mask: 0,
                pending_full_cause_mask: 0,
                dirty_relations: Vec::new(),
                batch_buffer: Vec::with_capacity(relation_count),
                relation_is_dirty: vec![false; relation_count],
                relation_trigger_masks: vec![0; relation_count],
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::layouts::integrated) struct TargetFacilityEndpointClearanceShardPropagator {
    name: String,
    orientations: Vec<EndpointClearanceOrientation>,
    relation_kernels: Vec<EndpointRectangleClearancePropagator>,
    priority: Priority,
    counters: Arc<EndpointClearancePropagationCounters>,
    false_event_filter_enabled: bool,
    events: Vec<ShardEvent>,
    pending_full: bool,
    pending_initial_execution: bool,
    pending_full_trigger_mask: u8,
    pending_full_cause_mask: u8,
    dirty_relations: Vec<usize>,
    batch_buffer: Vec<(usize, u8)>,
    relation_is_dirty: Vec<bool>,
    relation_trigger_masks: Vec<u8>,
}

impl TargetFacilityEndpointClearanceShardPropagator {
    fn note_all_logical_notifications(&self) {
        if !self.counters.enabled() {
            return;
        }
        for relation in &self.relation_kernels {
            relation.note_logical_notification();
        }
    }

    fn note_logical_notification(&self, relation_index: usize) {
        if self.counters.enabled() {
            self.relation_kernels[relation_index].note_logical_notification();
        }
    }

    fn dirty_all(&mut self, trigger_mask: u8, cause_mask: u8) {
        self.pending_full = true;
        self.pending_full_trigger_mask |= trigger_mask;
        self.pending_full_cause_mask |= cause_mask;
    }

    fn dirty_relation(&mut self, relation_index: usize, trigger_mask: u8) {
        self.relation_trigger_masks[relation_index] |= trigger_mask;
        if self.pending_full || self.relation_is_dirty[relation_index] {
            return;
        }
        self.relation_is_dirty[relation_index] = true;
        self.dirty_relations.push(relation_index);
    }

    fn take_batch(&mut self) -> (bool, EndpointClearanceFullShardCauses, Vec<(usize, u8)>) {
        let mut batch = std::mem::take(&mut self.batch_buffer);
        batch.clear();
        if self.pending_full {
            self.pending_full = false;
            let full_trigger_mask = std::mem::take(&mut self.pending_full_trigger_mask);
            let mut causes =
                decode_full_shard_causes(std::mem::take(&mut self.pending_full_cause_mask));
            causes.initial_execution = std::mem::take(&mut self.pending_initial_execution);
            self.dirty_relations.clear();
            for relation_index in 0..self.relation_kernels.len() {
                self.relation_is_dirty[relation_index] = false;
                let trigger_mask = full_trigger_mask
                    | std::mem::take(&mut self.relation_trigger_masks[relation_index]);
                batch.push((relation_index, trigger_mask));
            }
            return (true, causes, batch);
        }

        for relation_index in self.dirty_relations.drain(..) {
            self.relation_is_dirty[relation_index] = false;
            batch.push((
                relation_index,
                std::mem::take(&mut self.relation_trigger_masks[relation_index]),
            ));
        }
        (false, EndpointClearanceFullShardCauses::default(), batch)
    }

    fn recycle_batch(&mut self, mut batch: Vec<(usize, u8)>) {
        batch.clear();
        self.batch_buffer = batch;
    }

    fn restore_unprocessed(&mut self, batch: &[(usize, u8)]) {
        for (relation_index, trigger_mask) in batch {
            self.dirty_relation(*relation_index, *trigger_mask);
        }
    }

    fn clear_transient_state(&mut self) {
        self.pending_full = false;
        self.pending_initial_execution = false;
        self.pending_full_trigger_mask = 0;
        self.pending_full_cause_mask = 0;
        self.dirty_relations.clear();
        self.relation_is_dirty.fill(false);
        self.relation_trigger_masks.fill(0);
        for relation in &mut self.relation_kernels {
            relation.reset_transient_state();
        }
    }
}

impl Propagator for TargetFacilityEndpointClearanceShardPropagator {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        self.priority
    }

    fn notify(
        &mut self,
        context: NotificationContext,
        local_id: LocalId,
        _event: OpaqueDomainEvent,
    ) -> EnqueueDecision {
        let event = self.events[local_id.unpack() as usize];
        match event {
            ShardEvent::FacilityX => {
                let logical_relations = self.relation_kernels.len();
                self.counters.note_notification_axis(
                    EndpointClearanceNotificationAxis::FacilityX,
                    logical_relations,
                );
                self.note_all_logical_notifications();
                self.dirty_all(COORDINATE_TRIGGER, FACILITY_X_CAUSE);
            }
            ShardEvent::FacilityY => {
                let logical_relations = self.relation_kernels.len();
                self.counters.note_notification_axis(
                    EndpointClearanceNotificationAxis::FacilityY,
                    logical_relations,
                );
                self.note_all_logical_notifications();
                self.dirty_all(COORDINATE_TRIGGER, FACILITY_Y_CAUSE);
            }
            ShardEvent::Orientation(orientation_index) => {
                let logical_relations = self.relation_kernels.len();
                self.counters.note_notification_axis(
                    EndpointClearanceNotificationAxis::Orientation,
                    logical_relations,
                );
                self.note_all_logical_notifications();
                let orientation = self.orientations[orientation_index];
                if self.false_event_filter_enabled
                    && context.evaluate_predicate(orientation.selected.get_false_predicate())
                        == Some(true)
                {
                    self.counters
                        .note_skipped_false_orientation_notification(logical_relations);
                    self.counters.note_shard_notification(false);
                    return EnqueueDecision::Skip;
                }
                self.dirty_all(ORIENTATION_TRIGGER, ORIENTATION_CAUSE);
            }
            ShardEvent::ConnectionX(relation_index) => {
                self.counters
                    .note_notification_axis(EndpointClearanceNotificationAxis::ConnectionX, 1);
                self.note_logical_notification(relation_index);
                self.dirty_relation(relation_index, COORDINATE_TRIGGER);
            }
            ShardEvent::ConnectionY(relation_index) => {
                self.counters
                    .note_notification_axis(EndpointClearanceNotificationAxis::ConnectionY, 1);
                self.note_logical_notification(relation_index);
                self.dirty_relation(relation_index, COORDINATE_TRIGGER);
            }
        }

        let logical_relations = match event {
            ShardEvent::FacilityX | ShardEvent::FacilityY | ShardEvent::Orientation(_) => {
                self.relation_kernels.len()
            }
            ShardEvent::ConnectionX(_) | ShardEvent::ConnectionY(_) => 1,
        };
        self.counters.note_enqueued_notification(logical_relations);
        self.counters.note_shard_notification(true);
        EnqueueDecision::Enqueue
    }

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        let (full, causes, batch) = self.take_batch();
        self.counters
            .note_shard_batch(batch.len(), full, false, causes);
        for (batch_index, (relation_index, trigger_mask)) in batch.iter().copied().enumerate() {
            let exact_unaffected_axis_opportunity = if full && causes.is_facility_x_only() {
                self.relation_kernels[relation_index].universally_separated_on_y(&context)
            } else if full && causes.is_facility_y_only() {
                self.relation_kernels[relation_index].universally_separated_on_x(&context)
            } else {
                false
            };
            let (result, effects) = self.relation_kernels[relation_index]
                .propagate_all_with_effects(
                    &mut context,
                    ExecutionTrigger::from_mask(trigger_mask),
                );
            self.counters.note_shard_relation_check(
                full,
                causes,
                effects,
                exact_unaffected_axis_opportunity,
            );
            if let Err(conflict) = result {
                self.restore_unprocessed(&batch[batch_index + 1..]);
                self.recycle_batch(batch);
                return Err(conflict);
            }
        }
        self.recycle_batch(batch);
        Ok(())
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        self.counters.note_shard_batch(
            self.relation_kernels.len(),
            true,
            true,
            EndpointClearanceFullShardCauses::default(),
        );
        let mut scratch = self.clone();
        for relation in &mut scratch.relation_kernels {
            relation.propagate_all(&mut context, ExecutionTrigger::Scratch)?;
        }
        Ok(())
    }

    fn synchronise(&mut self, _context: NotificationContext<'_>) {
        self.clear_transient_state();
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
    use pumpkin_solver::core::variables::TransformableVariable;

    use super::*;
    use crate::layouts::integrated::exact::endpoint_clearance_propagator::EndpointRectangleClearancePropagatorArgs;
    use crate::layouts::integrated::exact::ladder::{
        EndpointClearanceBatchClassStatistics, EndpointClearanceFullShardCauseBucketStatistics,
        EndpointClearancePropagationStatistics,
    };

    #[derive(Debug, PartialEq, Eq)]
    struct RootSnapshot {
        status: CSPSolverExecutionFlag,
        domains: Vec<Vec<i32>>,
    }

    fn cause_bucket(
        statistics: &EndpointClearancePropagationStatistics,
        causes: EndpointClearanceFullShardCauses,
    ) -> &EndpointClearanceFullShardCauseBucketStatistics {
        &statistics.batching.full_shard_cause_buckets[usize::from(causes.mask())]
    }

    fn assert_batch_accounting(batch: &EndpointClearanceBatchClassStatistics) {
        assert_eq!(
            batch.actual_relation_checks + batch.conflict_abandoned_relation_occurrences,
            batch.scheduled_relation_checks
        );
        assert_eq!(
            batch.effectful_relation_checks + batch.no_effect_relation_checks,
            batch.actual_relation_checks
        );
        assert!(batch.conflict_relation_checks <= batch.effectful_relation_checks);
        assert!(batch.universally_entailed_relation_checks <= batch.no_effect_relation_checks);
    }

    fn retained_values(solver: &Solver, variable: DomainId, lower: i32, upper: i32) -> Vec<i32> {
        (lower..=upper)
            .filter(|value| solver.contains(&variable, *value))
            .collect()
    }

    fn partial_domain_case(sharded: bool, false_event_filter_enabled: bool) -> RootSnapshot {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 3, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 2, "facility-y");
        let first_x = solver.new_named_bounded_integer(0, 3, "first-x");
        let first_y = solver.new_named_bounded_integer(0, 2, "first-y");
        let second_x = solver.new_named_bounded_integer(1, 4, "second-x");
        let second_y = solver.new_named_bounded_integer(0, 3, "second-y");
        let horizontal = solver.new_named_literal("horizontal");
        let vertical = solver.new_named_literal("vertical");
        solver.add_clause(
            [
                horizontal.get_true_predicate(),
                vertical.get_true_predicate(),
            ],
            tag,
        );
        solver.add_clause(
            [
                horizontal.get_false_predicate(),
                vertical.get_false_predicate(),
            ],
            tag,
        );
        let orientations = vec![
            EndpointClearanceOrientation {
                selected: horizontal,
                selected_parent: *horizontal.get_integer_variable().inner(),
                width: 2,
                height: 1,
            },
            EndpointClearanceOrientation {
                selected: vertical,
                selected_parent: *vertical.get_integer_variable().inner(),
                width: 1,
                height: 2,
            },
        ];
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let relations = [("first", first_x, first_y), ("second", second_x, second_y)];
        if sharded {
            let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
                name: "partial-domain-shard".to_string(),
                target_facility: "target".to_string(),
                facility_x,
                facility_y,
                orientations,
                relations: relations
                    .into_iter()
                    .map(|(terminal, connection_x, connection_y)| {
                        EndpointClearanceShardRelationArgs {
                            terminal: terminal.to_string(),
                            connection_x,
                            connection_y,
                            relation_counters: counters.register_relation(terminal, "target"),
                        }
                    })
                    .collect(),
                priority: Priority::High,
                counters,
                false_event_filter_enabled,
                constraint_tag: tag,
            });
        } else {
            for (terminal, connection_x, connection_y) in relations {
                let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
                    name: format!("partial-domain-{terminal}"),
                    connection_x,
                    connection_y,
                    facility_x,
                    facility_y,
                    orientations: orientations.clone(),
                    priority: Priority::High,
                    counters: Arc::clone(&counters),
                    false_event_filter_enabled,
                    relation_counters: counters.register_relation(terminal, "target"),
                    constraint_tag: tag,
                });
            }
        }

        let status = solver.propagate_to_fixpoint();
        RootSnapshot {
            status,
            domains: [
                (facility_x, 0, 3),
                (facility_y, 0, 2),
                (first_x, 0, 3),
                (first_y, 0, 2),
                (second_x, 1, 4),
                (second_y, 0, 3),
                (*horizontal.get_integer_variable().inner(), 0, 1),
                (*vertical.get_integer_variable().inner(), 0, 1),
            ]
            .into_iter()
            .map(|(variable, lower, upper)| retained_values(&solver, variable, lower, upper))
            .collect(),
        }
    }

    fn fixed_case(
        sharded: bool,
        facility: (i32, i32),
        first: (i32, i32),
        second: (i32, i32),
    ) -> CSPSolverExecutionFlag {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(facility.0, facility.0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(facility.1, facility.1, "facility-y");
        let first_x = solver.new_named_bounded_integer(first.0, first.0, "first-x");
        let first_y = solver.new_named_bounded_integer(first.1, first.1, "first-y");
        let second_x = solver.new_named_bounded_integer(second.0, second.0, "second-x");
        let second_y = solver.new_named_bounded_integer(second.1, second.1, "second-y");
        let selected = solver.new_named_literal("selected-orientation");
        solver.add_clause([selected.get_true_predicate()], tag);
        let orientation = EndpointClearanceOrientation {
            selected,
            selected_parent: *selected.get_integer_variable().inner(),
            width: 2,
            height: 2,
        };
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let relations = [("first", first_x, first_y), ("second", second_x, second_y)];
        if sharded {
            let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
                name: "controlled-shard".to_string(),
                target_facility: "target".to_string(),
                facility_x,
                facility_y,
                orientations: vec![orientation],
                relations: relations
                    .into_iter()
                    .map(|(terminal, connection_x, connection_y)| {
                        EndpointClearanceShardRelationArgs {
                            terminal: terminal.to_string(),
                            connection_x,
                            connection_y,
                            relation_counters: counters.register_relation(terminal, "target"),
                        }
                    })
                    .collect(),
                priority: Priority::High,
                counters,
                false_event_filter_enabled: false,
                constraint_tag: tag,
            });
        } else {
            for (terminal, connection_x, connection_y) in relations {
                let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
                    name: format!("controlled-{terminal}"),
                    connection_x,
                    connection_y,
                    facility_x,
                    facility_y,
                    orientations: vec![orientation],
                    priority: Priority::High,
                    counters: Arc::clone(&counters),
                    false_event_filter_enabled: false,
                    relation_counters: counters.register_relation(terminal, "target"),
                    constraint_tag: tag,
                });
            }
        }
        solver.propagate_to_fixpoint()
    }

    #[test]
    fn complete_assignments_match_pairwise_relations_exhaustively() {
        for facility_x in 0..4 {
            for facility_y in 0..4 {
                for first_x in 0..4 {
                    for first_y in 0..4 {
                        for second_x in 0..4 {
                            for second_y in 0..4 {
                                let facility = (facility_x, facility_y);
                                let first = (first_x, first_y);
                                let second = (second_x, second_y);
                                assert_eq!(
                                    fixed_case(false, facility, first, second),
                                    fixed_case(true, facility, first, second),
                                    "facility={facility:?} first={first:?} second={second:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn initial_execution_checks_every_relation_without_a_watched_event() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let connection_x = solver.new_named_bounded_integer(1, 1, "connection-x");
        let connection_y = solver.new_named_bounded_integer(1, 1, "connection-y");
        let selected = solver.new_named_literal("selected-orientation");
        solver.add_clause([selected.get_true_predicate()], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );

        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "late-controlled-shard".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 2,
                height: 2,
            }],
            relations: vec![EndpointClearanceShardRelationArgs {
                terminal: "inside".to_string(),
                connection_x,
                connection_y,
                relation_counters: counters.register_relation("inside", "target"),
            }],
            priority: Priority::High,
            counters,
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
    }

    #[test]
    fn partial_domains_and_multiple_orientations_match_pairwise_fixpoint() {
        for false_event_filter_enabled in [false, true] {
            let pairwise = partial_domain_case(false, false_event_filter_enabled);
            let sharded = partial_domain_case(true, false_event_filter_enabled);
            assert_eq!(sharded, pairwise);
            assert_eq!(sharded.status, CSPSolverExecutionFlag::Feasible);
            assert_eq!(sharded.domains[6], vec![0, 1]);
            assert_eq!(sharded.domains[7], vec![0, 1]);
        }
        assert_eq!(
            partial_domain_case(true, false),
            partial_domain_case(true, true)
        );
    }

    #[test]
    fn later_shared_facility_update_redirties_an_earlier_relation() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 2, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let selected = solver.new_named_literal("selected-orientation");
        solver.add_clause([selected.get_true_predicate()], tag);
        let first_x = solver.new_named_bounded_integer(1, 1, "first-x");
        let first_y = solver.new_named_bounded_integer(0, 0, "first-y");
        let second_x = solver.new_named_bounded_integer(0, 0, "second-x");
        let second_y = solver.new_named_bounded_integer(0, 0, "second-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "redirty-shard".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 1,
                height: 1,
            }],
            relations: [("earlier", first_x, first_y), ("later", second_x, second_y)]
                .into_iter()
                .map(
                    |(terminal, connection_x, connection_y)| EndpointClearanceShardRelationArgs {
                        terminal: terminal.to_string(),
                        connection_x,
                        connection_y,
                        relation_counters: counters.register_relation(terminal, "target"),
                    },
                )
                .collect(),
            priority: Priority::High,
            counters,
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(solver.lower_bound(&facility_x), 2);
    }

    #[test]
    fn endpoint_and_facility_events_schedule_relation_and_full_batches() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 3, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let selected = solver.new_named_literal("selected-orientation");
        solver.add_clause([selected.get_true_predicate()], tag);
        let first_x = solver.new_named_bounded_integer(0, 3, "first-x");
        let first_y = solver.new_named_bounded_integer(10, 10, "first-y");
        let second_x = solver.new_named_bounded_integer(0, 3, "second-x");
        let second_y = solver.new_named_bounded_integer(10, 10, "second-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "batch-scope-shard".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 1,
                height: 1,
            }],
            relations: [("first", first_x, first_y), ("second", second_x, second_y)]
                .into_iter()
                .map(
                    |(terminal, connection_x, connection_y)| EndpointClearanceShardRelationArgs {
                        terminal: terminal.to_string(),
                        connection_x,
                        connection_y,
                        relation_counters: counters.register_relation(terminal, "target"),
                    },
                )
                .collect(),
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );

        let initial = counters.snapshot();
        assert_eq!(initial.batching.full_shard.batches, 1);
        assert_eq!(initial.batching.full_shard.scheduled_relation_checks, 2);
        assert_eq!(initial.batching.full_shard.actual_relation_checks, 2);
        assert_eq!(
            initial
                .batching
                .full_shard_cause_buckets
                .iter()
                .filter(|bucket| bucket.initial_execution)
                .map(|bucket| bucket.batch.batches)
                .sum::<u64>(),
            1
        );
        assert_eq!(
            initial
                .batching
                .full_shard_cause_buckets
                .iter()
                .filter(|bucket| bucket.initial_execution)
                .map(|bucket| bucket.batch.scheduled_relation_checks)
                .sum::<u64>(),
            2
        );
        solver.add_clause([first_x.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let endpoint = counters.snapshot();
        assert_eq!(
            endpoint.batching.relation_subset.scheduled_relation_checks
                - initial.batching.relation_subset.scheduled_relation_checks,
            1
        );
        assert_eq!(
            endpoint.batching.relation_subset.actual_relation_checks
                - initial.batching.relation_subset.actual_relation_checks,
            1
        );
        assert_batch_accounting(&endpoint.batching.relation_subset);
        assert_eq!(endpoint.notifications - initial.notifications, 1);
        assert_eq!(
            endpoint.enqueued_notifications - initial.enqueued_notifications,
            1
        );
        assert_eq!(
            endpoint.batching.notification_callbacks - initial.batching.notification_callbacks,
            1
        );
        assert_eq!(
            endpoint.batching.enqueue_requests - initial.batching.enqueue_requests,
            1
        );

        solver.add_clause([facility_x.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let facility = counters.snapshot();
        assert_eq!(
            facility.batching.full_shard.scheduled_relation_checks
                - endpoint.batching.full_shard.scheduled_relation_checks,
            2
        );
        assert_eq!(
            facility.batching.full_shard.actual_relation_checks
                - endpoint.batching.full_shard.actual_relation_checks,
            2
        );
        assert_eq!(
            cause_bucket(
                &facility,
                EndpointClearanceFullShardCauses {
                    facility_x: true,
                    ..EndpointClearanceFullShardCauses::default()
                }
            )
            .batch
            .batches
                - cause_bucket(
                    &endpoint,
                    EndpointClearanceFullShardCauses {
                        facility_x: true,
                        ..EndpointClearanceFullShardCauses::default()
                    }
                )
                .batch
                .batches,
            1
        );
        assert_eq!(
            cause_bucket(
                &facility,
                EndpointClearanceFullShardCauses {
                    facility_x: true,
                    ..EndpointClearanceFullShardCauses::default()
                }
            )
            .batch
            .scheduled_relation_checks
                - cause_bucket(
                    &endpoint,
                    EndpointClearanceFullShardCauses {
                        facility_x: true,
                        ..EndpointClearanceFullShardCauses::default()
                    }
                )
                .batch
                .scheduled_relation_checks,
            2
        );
        assert_eq!(
            cause_bucket(
                &facility,
                EndpointClearanceFullShardCauses {
                    facility_x: true,
                    ..EndpointClearanceFullShardCauses::default()
                }
            )
            .exact_unaffected_axis_opportunity_relation_checks
                - cause_bucket(
                    &endpoint,
                    EndpointClearanceFullShardCauses {
                        facility_x: true,
                        ..EndpointClearanceFullShardCauses::default()
                    }
                )
                .exact_unaffected_axis_opportunity_relation_checks,
            2
        );
        assert_batch_accounting(&facility.batching.full_shard);
        assert_eq!(facility.notifications - endpoint.notifications, 2);
        assert_eq!(
            facility.enqueued_notifications - endpoint.enqueued_notifications,
            2
        );
        assert_eq!(
            facility.batching.notification_callbacks - endpoint.batching.notification_callbacks,
            1
        );
        assert_eq!(
            facility.batching.enqueue_requests - endpoint.batching.enqueue_requests,
            1
        );
    }

    #[test]
    fn full_batch_causes_distinguish_facility_y_and_orientation_events() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 1, "facility-y");
        let selected = solver.new_named_literal("selected-orientation");
        let connection_x = solver.new_named_bounded_integer(10, 10, "connection-x");
        let connection_y = solver.new_named_bounded_integer(10, 10, "connection-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "full-batch-causes".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 1,
                height: 1,
            }],
            relations: vec![EndpointClearanceShardRelationArgs {
                terminal: "terminal".to_string(),
                connection_x,
                connection_y,
                relation_counters: counters.register_relation("terminal", "target"),
            }],
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let initial = counters.snapshot();

        solver.add_clause([facility_y.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let facility_y_event = counters.snapshot();
        let facility_y_causes = EndpointClearanceFullShardCauses {
            facility_y: true,
            ..EndpointClearanceFullShardCauses::default()
        };
        assert_eq!(
            cause_bucket(&facility_y_event, facility_y_causes)
                .batch
                .batches
                - cause_bucket(&initial, facility_y_causes).batch.batches,
            1
        );
        assert_eq!(
            cause_bucket(&facility_y_event, facility_y_causes)
                .exact_unaffected_axis_opportunity_relation_checks
                - cause_bucket(&initial, facility_y_causes)
                    .exact_unaffected_axis_opportunity_relation_checks,
            1
        );

        solver.add_clause([selected.get_true_predicate()], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let orientation_event = counters.snapshot();
        let orientation_causes = EndpointClearanceFullShardCauses {
            orientation: true,
            ..EndpointClearanceFullShardCauses::default()
        };
        assert_eq!(
            cause_bucket(&orientation_event, orientation_causes)
                .batch
                .batches
                - cause_bucket(&facility_y_event, orientation_causes)
                    .batch
                    .batches,
            1
        );
    }

    #[test]
    fn coalesced_full_batch_uses_one_disjoint_cause_bucket() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 1, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 1, "facility-y");
        let selected = solver.new_named_literal("selected-orientation");
        let connection_x = solver.new_named_bounded_integer(10, 10, "connection-x");
        let connection_y = solver.new_named_bounded_integer(10, 10, "connection-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "coalesced-causes".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 1,
                height: 1,
            }],
            relations: vec![EndpointClearanceShardRelationArgs {
                terminal: "terminal".to_string(),
                connection_x,
                connection_y,
                relation_counters: counters.register_relation("terminal", "target"),
            }],
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let initial = counters.snapshot();

        solver.add_clause([facility_x.lower_bound_predicate(1)], tag);
        solver.add_clause([facility_y.lower_bound_predicate(1)], tag);
        solver.add_clause([selected.get_true_predicate()], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let coalesced = counters.snapshot();
        let causes = EndpointClearanceFullShardCauses {
            facility_x: true,
            facility_y: true,
            orientation: true,
            ..EndpointClearanceFullShardCauses::default()
        };
        let bucket = cause_bucket(&coalesced, causes);
        let initial_bucket = cause_bucket(&initial, causes);
        assert_eq!(bucket.batch.batches - initial_bucket.batch.batches, 1);
        assert_eq!(
            bucket.batch.scheduled_relation_checks - initial_bucket.batch.scheduled_relation_checks,
            1
        );
        assert_eq!(
            bucket.batch.actual_relation_checks - initial_bucket.batch.actual_relation_checks,
            1
        );
        assert_eq!(
            bucket.exact_unaffected_axis_opportunity_relation_checks
                - initial_bucket.exact_unaffected_axis_opportunity_relation_checks,
            0
        );
        assert_eq!(
            coalesced
                .batching
                .full_shard_cause_buckets
                .iter()
                .zip(&initial.batching.full_shard_cause_buckets)
                .map(|(after, before)| after.batch.batches - before.batch.batches)
                .sum::<u64>(),
            1
        );
        for bucket in &coalesced.batching.full_shard_cause_buckets {
            assert_batch_accounting(&bucket.batch);
        }
    }

    #[test]
    fn relation_subset_conflict_reports_its_abandoned_tail_occurrence() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let selected = solver.new_named_literal("selected-orientation");
        solver.add_clause([selected.get_true_predicate()], tag);
        let first_x = solver.new_named_bounded_integer(-1, 1, "first-x");
        let first_y = solver.new_named_bounded_integer(-1, 1, "first-y");
        let second_x = solver.new_named_bounded_integer(-1, 1, "second-x");
        let second_y = solver.new_named_bounded_integer(-1, 1, "second-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "relation-subset-conflict".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 1,
                height: 1,
            }],
            relations: [("first", first_x, first_y), ("second", second_x, second_y)]
                .into_iter()
                .map(
                    |(terminal, connection_x, connection_y)| EndpointClearanceShardRelationArgs {
                        terminal: terminal.to_string(),
                        connection_x,
                        connection_y,
                        relation_counters: counters.register_relation(terminal, "target"),
                    },
                )
                .collect(),
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let initial = counters.snapshot();

        for variable in [first_x, first_y, second_x, second_y] {
            solver.add_clause([variable.equality_predicate(0)], tag);
        }
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
        let conflict = counters.snapshot();
        assert_eq!(
            conflict.batching.relation_subset.scheduled_relation_checks
                - initial.batching.relation_subset.scheduled_relation_checks,
            2
        );
        assert_eq!(
            conflict.batching.relation_subset.actual_relation_checks
                - initial.batching.relation_subset.actual_relation_checks,
            1
        );
        assert_eq!(
            conflict
                .batching
                .relation_subset
                .conflict_abandoned_relation_occurrences
                - initial
                    .batching
                    .relation_subset
                    .conflict_abandoned_relation_occurrences,
            1
        );
        assert_batch_accounting(&conflict.batching.relation_subset);
    }

    #[test]
    fn disabled_counters_keep_every_batch_metric_zero() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 1, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 1, "facility-y");
        let selected = solver.new_named_literal("selected-orientation");
        let connection_x = solver.new_named_bounded_integer(10, 10, "connection-x");
        let connection_y = solver.new_named_bounded_integer(10, 10, "connection-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::new(false));
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "disabled-counters".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 1,
                height: 1,
            }],
            relations: vec![EndpointClearanceShardRelationArgs {
                terminal: "terminal".to_string(),
                connection_x,
                connection_y,
                relation_counters: counters.register_relation("terminal", "target"),
            }],
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        solver.add_clause([facility_x.lower_bound_predicate(1)], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert_eq!(statistics.relations, 0);
        assert_eq!(statistics.batching.shards, 0);
        assert_eq!(statistics.batching.shard_executions, 0);
        assert_eq!(
            statistics.batching.full_shard,
            EndpointClearanceBatchClassStatistics::default()
        );
        assert_eq!(
            statistics.batching.relation_subset,
            EndpointClearanceBatchClassStatistics::default()
        );
        assert!(statistics.batching.full_shard_cause_buckets.is_empty());
    }

    #[test]
    fn conflict_reports_the_abandoned_full_batch_tail_occurrence() {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let selected = solver.new_named_literal("selected-orientation");
        solver.add_clause([selected.get_true_predicate()], tag);
        let blocked_x = solver.new_named_bounded_integer(0, 0, "blocked-x");
        let blocked_y = solver.new_named_bounded_integer(0, 0, "blocked-y");
        let clear_x = solver.new_named_bounded_integer(10, 10, "clear-x");
        let clear_y = solver.new_named_bounded_integer(10, 10, "clear-y");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
            name: "conflicting-full-batch".to_string(),
            target_facility: "target".to_string(),
            facility_x,
            facility_y,
            orientations: vec![EndpointClearanceOrientation {
                selected,
                selected_parent: *selected.get_integer_variable().inner(),
                width: 1,
                height: 1,
            }],
            relations: [
                ("blocked", blocked_x, blocked_y),
                ("clear", clear_x, clear_y),
            ]
            .into_iter()
            .map(
                |(terminal, connection_x, connection_y)| EndpointClearanceShardRelationArgs {
                    terminal: terminal.to_string(),
                    connection_x,
                    connection_y,
                    relation_counters: counters.register_relation(terminal, "target"),
                },
            )
            .collect(),
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: false,
            constraint_tag: tag,
        });
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Infeasible
        );
        let statistics = counters.snapshot();
        assert_eq!(statistics.batching.full_shard.scheduled_relation_checks, 2);
        assert_eq!(statistics.batching.full_shard.actual_relation_checks, 1);
        assert_eq!(
            statistics
                .batching
                .full_shard
                .conflict_abandoned_relation_occurrences,
            1
        );
        assert_eq!(statistics.batching.full_shard.conflict_relation_checks, 1);
        assert_eq!(statistics.batching.full_shard.effectful_relation_checks, 1);
        assert_batch_accounting(&statistics.batching.full_shard);
        for bucket in &statistics.batching.full_shard_cause_buckets {
            assert_batch_accounting(&bucket.batch);
        }
    }

    fn backtracking_case(sharded: bool) -> (bool, bool, bool, u64) {
        let mut solver = Solver::default();
        let tag = solver.new_constraint_tag();
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let first_x = solver.new_named_bounded_integer(1, 3, "first-x");
        let first_y = solver.new_named_bounded_integer(1, 1, "first-y");
        let second_x = solver.new_named_bounded_integer(1, 3, "second-x");
        let second_y = solver.new_named_bounded_integer(1, 1, "second-y");
        let choice = solver.new_named_literal("try-blocked-orientation-first");
        let blocked = solver.new_named_literal("blocked-orientation");
        let supported = solver.new_named_literal("supported-orientation");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let orientations = vec![
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
        ];
        let relations = [("first", first_x, first_y), ("second", second_x, second_y)];
        if sharded {
            let _ = solver.add_propagator(TargetFacilityEndpointClearanceShardPropagatorArgs {
                name: "backtracking-shard".to_string(),
                target_facility: "target".to_string(),
                facility_x,
                facility_y,
                orientations,
                relations: relations
                    .into_iter()
                    .map(|(terminal, connection_x, connection_y)| {
                        EndpointClearanceShardRelationArgs {
                            terminal: terminal.to_string(),
                            connection_x,
                            connection_y,
                            relation_counters: counters.register_relation(terminal, "target"),
                        }
                    })
                    .collect(),
                priority: Priority::High,
                counters: Arc::clone(&counters),
                false_event_filter_enabled: true,
                constraint_tag: tag,
            });
        } else {
            for (terminal, connection_x, connection_y) in relations {
                let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
                    name: format!("backtracking-{terminal}"),
                    connection_x,
                    connection_y,
                    facility_x,
                    facility_y,
                    orientations: orientations.clone(),
                    priority: Priority::High,
                    counters: Arc::clone(&counters),
                    false_event_filter_enabled: true,
                    relation_counters: counters.register_relation(terminal, "target"),
                    constraint_tag: tag,
                });
            }
        }

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
            .add_constraint(pumpkin_solver::equals(vec![first_x.scaled(1)], 1, tag))
            .implied_by(choice);

        let choice_parent = *choice.get_integer_variable().inner();
        let branchers: Vec<Box<dyn Brancher>> = vec![
            Box::new(WarmStart::new(&[choice_parent], &[1])),
            Box::new(solver.default_brancher()),
        ];
        let mut brancher = DynamicBrancher::new(branchers);
        let mut resolver = ResolutionResolver::default();
        let (satisfiable, choice_value, supported_value) =
            match solver.satisfy(&mut brancher, &mut Indefinite, &mut resolver) {
                SatisfactionResult::Satisfiable(result) => {
                    let solution = result.solution();
                    (
                        true,
                        solution.get_literal_value(choice),
                        solution.get_literal_value(supported),
                    )
                }
                _ => (false, false, false),
            };
        let second = counters
            .snapshot()
            .relation_hotset
            .top_relations_by_execution
            .into_iter()
            .find(|relation| relation.terminal == "second")
            .expect("second relation is retained in the controlled hotset");
        (
            satisfiable,
            choice_value,
            supported_value,
            second.executions,
        )
    }

    #[test]
    fn conflicting_early_relation_does_not_lose_a_sibling_after_backtracking() {
        let pairwise = backtracking_case(false);
        let sharded = backtracking_case(true);
        assert_eq!(pairwise, sharded);
        assert!(sharded.0);
        assert!(!sharded.1);
        assert!(sharded.2);
        assert!(sharded.3 >= 2);
    }
}
