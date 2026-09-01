use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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

use super::ladder::{
    EndpointClearanceBatchingStatistics, EndpointClearanceGroupStatistics,
    EndpointClearancePropagationStatistics, EndpointClearanceRelationHotsetStatistics,
    EndpointClearanceRelationStatistics,
};

declare_inference_label!(EndpointRectangleClearance);

#[derive(Debug)]
pub(in crate::layouts::integrated) struct EndpointClearancePropagationCounters {
    enabled: bool,
    relations: AtomicU64,
    executions: AtomicU64,
    notifications: AtomicU64,
    coordinate_notifications: AtomicU64,
    connection_x_notifications: AtomicU64,
    connection_y_notifications: AtomicU64,
    facility_x_notifications: AtomicU64,
    facility_y_notifications: AtomicU64,
    orientation_notifications: AtomicU64,
    skipped_false_orientation_notifications: AtomicU64,
    enqueued_notifications: AtomicU64,
    orientation_checks: AtomicU64,
    rejected_orientations: AtomicU64,
    forced_separation_detections: AtomicU64,
    bound_updates: AtomicU64,
    conflicts: AtomicU64,
    maximum_reason_predicates: AtomicU64,
    scratch_executions: AtomicU64,
    coordinate_only_executions: AtomicU64,
    orientation_only_executions: AtomicU64,
    mixed_event_executions: AtomicU64,
    unclassified_executions: AtomicU64,
    executions_with_rejection: AtomicU64,
    executions_with_forced_separation: AtomicU64,
    executions_with_bound_update: AtomicU64,
    executions_with_conflict: AtomicU64,
    executions_without_domain_effect: AtomicU64,
    scratch_executions_without_domain_effect: AtomicU64,
    coordinate_only_executions_without_domain_effect: AtomicU64,
    orientation_only_executions_without_domain_effect: AtomicU64,
    mixed_event_executions_without_domain_effect: AtomicU64,
    unclassified_executions_without_domain_effect: AtomicU64,
    universally_entailed_executions: AtomicU64,
    entailment_episodes: AtomicU64,
    notifications_while_entailed: AtomicU64,
    shards: AtomicU64,
    shard_facility_coordinate_watchers: AtomicU64,
    shard_orientation_watchers: AtomicU64,
    shard_endpoint_coordinate_watchers: AtomicU64,
    shard_notification_callbacks: AtomicU64,
    shard_enqueue_requests: AtomicU64,
    shard_executions: AtomicU64,
    shard_scratch_executions: AtomicU64,
    shard_full_batches: AtomicU64,
    shard_endpoint_only_batches: AtomicU64,
    shard_dirty_relation_checks: AtomicU64,
    shard_total_dirty_batch_size: AtomicU64,
    shard_maximum_dirty_batch_size: AtomicU64,
    relation_details: Mutex<Vec<Arc<EndpointClearanceRelationCounters>>>,
}

impl Default for EndpointClearancePropagationCounters {
    fn default() -> Self {
        Self::new(true)
    }
}

impl EndpointClearancePropagationCounters {
    pub(in crate::layouts::integrated) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            relations: AtomicU64::default(),
            executions: AtomicU64::default(),
            notifications: AtomicU64::default(),
            coordinate_notifications: AtomicU64::default(),
            connection_x_notifications: AtomicU64::default(),
            connection_y_notifications: AtomicU64::default(),
            facility_x_notifications: AtomicU64::default(),
            facility_y_notifications: AtomicU64::default(),
            orientation_notifications: AtomicU64::default(),
            skipped_false_orientation_notifications: AtomicU64::default(),
            enqueued_notifications: AtomicU64::default(),
            orientation_checks: AtomicU64::default(),
            rejected_orientations: AtomicU64::default(),
            forced_separation_detections: AtomicU64::default(),
            bound_updates: AtomicU64::default(),
            conflicts: AtomicU64::default(),
            maximum_reason_predicates: AtomicU64::default(),
            scratch_executions: AtomicU64::default(),
            coordinate_only_executions: AtomicU64::default(),
            orientation_only_executions: AtomicU64::default(),
            mixed_event_executions: AtomicU64::default(),
            unclassified_executions: AtomicU64::default(),
            executions_with_rejection: AtomicU64::default(),
            executions_with_forced_separation: AtomicU64::default(),
            executions_with_bound_update: AtomicU64::default(),
            executions_with_conflict: AtomicU64::default(),
            executions_without_domain_effect: AtomicU64::default(),
            scratch_executions_without_domain_effect: AtomicU64::default(),
            coordinate_only_executions_without_domain_effect: AtomicU64::default(),
            orientation_only_executions_without_domain_effect: AtomicU64::default(),
            mixed_event_executions_without_domain_effect: AtomicU64::default(),
            unclassified_executions_without_domain_effect: AtomicU64::default(),
            universally_entailed_executions: AtomicU64::default(),
            entailment_episodes: AtomicU64::default(),
            notifications_while_entailed: AtomicU64::default(),
            shards: AtomicU64::default(),
            shard_facility_coordinate_watchers: AtomicU64::default(),
            shard_orientation_watchers: AtomicU64::default(),
            shard_endpoint_coordinate_watchers: AtomicU64::default(),
            shard_notification_callbacks: AtomicU64::default(),
            shard_enqueue_requests: AtomicU64::default(),
            shard_executions: AtomicU64::default(),
            shard_scratch_executions: AtomicU64::default(),
            shard_full_batches: AtomicU64::default(),
            shard_endpoint_only_batches: AtomicU64::default(),
            shard_dirty_relation_checks: AtomicU64::default(),
            shard_total_dirty_batch_size: AtomicU64::default(),
            shard_maximum_dirty_batch_size: AtomicU64::default(),
            relation_details: Mutex::new(Vec::new()),
        }
    }

    fn increment(&self, counter: &AtomicU64) {
        if self.enabled {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn increment_by(&self, counter: &AtomicU64, amount: usize) {
        if self.enabled {
            counter.fetch_add(amount.try_into().unwrap_or(u64::MAX), Ordering::Relaxed);
        }
    }

    fn note_maximum_reason_predicates(&self, predicates: u64) {
        if self.enabled {
            self.maximum_reason_predicates
                .fetch_max(predicates, Ordering::Relaxed);
        }
    }

    pub(in crate::layouts::integrated) fn snapshot(
        &self,
    ) -> EndpointClearancePropagationStatistics {
        EndpointClearancePropagationStatistics {
            relations: self.relations.load(Ordering::Relaxed),
            executions: self.executions.load(Ordering::Relaxed),
            notifications: self.notifications.load(Ordering::Relaxed),
            coordinate_notifications: self.coordinate_notifications.load(Ordering::Relaxed),
            connection_x_notifications: self.connection_x_notifications.load(Ordering::Relaxed),
            connection_y_notifications: self.connection_y_notifications.load(Ordering::Relaxed),
            facility_x_notifications: self.facility_x_notifications.load(Ordering::Relaxed),
            facility_y_notifications: self.facility_y_notifications.load(Ordering::Relaxed),
            orientation_notifications: self.orientation_notifications.load(Ordering::Relaxed),
            skipped_false_orientation_notifications: self
                .skipped_false_orientation_notifications
                .load(Ordering::Relaxed),
            enqueued_notifications: self.enqueued_notifications.load(Ordering::Relaxed),
            orientation_checks: self.orientation_checks.load(Ordering::Relaxed),
            rejected_orientations: self.rejected_orientations.load(Ordering::Relaxed),
            forced_separation_detections: self.forced_separation_detections.load(Ordering::Relaxed),
            bound_updates: self.bound_updates.load(Ordering::Relaxed),
            conflicts: self.conflicts.load(Ordering::Relaxed),
            maximum_reason_predicates: self.maximum_reason_predicates.load(Ordering::Relaxed),
            scratch_executions: self.scratch_executions.load(Ordering::Relaxed),
            coordinate_only_executions: self.coordinate_only_executions.load(Ordering::Relaxed),
            orientation_only_executions: self.orientation_only_executions.load(Ordering::Relaxed),
            mixed_event_executions: self.mixed_event_executions.load(Ordering::Relaxed),
            unclassified_executions: self.unclassified_executions.load(Ordering::Relaxed),
            executions_with_rejection: self.executions_with_rejection.load(Ordering::Relaxed),
            executions_with_forced_separation: self
                .executions_with_forced_separation
                .load(Ordering::Relaxed),
            executions_with_bound_update: self.executions_with_bound_update.load(Ordering::Relaxed),
            executions_with_conflict: self.executions_with_conflict.load(Ordering::Relaxed),
            executions_without_domain_effect: self
                .executions_without_domain_effect
                .load(Ordering::Relaxed),
            scratch_executions_without_domain_effect: self
                .scratch_executions_without_domain_effect
                .load(Ordering::Relaxed),
            coordinate_only_executions_without_domain_effect: self
                .coordinate_only_executions_without_domain_effect
                .load(Ordering::Relaxed),
            orientation_only_executions_without_domain_effect: self
                .orientation_only_executions_without_domain_effect
                .load(Ordering::Relaxed),
            mixed_event_executions_without_domain_effect: self
                .mixed_event_executions_without_domain_effect
                .load(Ordering::Relaxed),
            unclassified_executions_without_domain_effect: self
                .unclassified_executions_without_domain_effect
                .load(Ordering::Relaxed),
            universally_entailed_executions: self
                .universally_entailed_executions
                .load(Ordering::Relaxed),
            entailment_episodes: self.entailment_episodes.load(Ordering::Relaxed),
            notifications_while_entailed: self.notifications_while_entailed.load(Ordering::Relaxed),
            relation_hotset: self.relation_hotset(),
            batching: EndpointClearanceBatchingStatistics {
                shards: self.shards.load(Ordering::Relaxed),
                facility_coordinate_watchers: self
                    .shard_facility_coordinate_watchers
                    .load(Ordering::Relaxed),
                orientation_watchers: self.shard_orientation_watchers.load(Ordering::Relaxed),
                endpoint_coordinate_watchers: self
                    .shard_endpoint_coordinate_watchers
                    .load(Ordering::Relaxed),
                notification_callbacks: self.shard_notification_callbacks.load(Ordering::Relaxed),
                enqueue_requests: self.shard_enqueue_requests.load(Ordering::Relaxed),
                shard_executions: self.shard_executions.load(Ordering::Relaxed),
                scratch_executions: self.shard_scratch_executions.load(Ordering::Relaxed),
                full_shard_batches: self.shard_full_batches.load(Ordering::Relaxed),
                endpoint_only_batches: self.shard_endpoint_only_batches.load(Ordering::Relaxed),
                dirty_relation_checks: self.shard_dirty_relation_checks.load(Ordering::Relaxed),
                total_dirty_batch_size: self.shard_total_dirty_batch_size.load(Ordering::Relaxed),
                maximum_dirty_batch_size: self
                    .shard_maximum_dirty_batch_size
                    .load(Ordering::Relaxed),
            },
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn note_shard_registration(
        &self,
        orientation_watchers: usize,
        relation_count: usize,
    ) {
        if !self.enabled {
            return;
        }
        self.shards.fetch_add(1, Ordering::Relaxed);
        self.shard_facility_coordinate_watchers
            .fetch_add(2, Ordering::Relaxed);
        self.shard_orientation_watchers.fetch_add(
            orientation_watchers.try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        self.shard_endpoint_coordinate_watchers.fetch_add(
            relation_count
                .saturating_mul(2)
                .try_into()
                .unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    pub(super) fn note_shard_notification(&self, enqueue: bool) {
        self.increment(&self.shard_notification_callbacks);
        if enqueue {
            self.increment(&self.shard_enqueue_requests);
        }
    }

    pub(super) fn note_notification_axis(
        &self,
        axis: EndpointClearanceNotificationAxis,
        logical_relations: usize,
    ) {
        self.increment_by(&self.notifications, logical_relations);
        match axis {
            EndpointClearanceNotificationAxis::ConnectionX => {
                self.increment_by(&self.coordinate_notifications, logical_relations);
                self.increment_by(&self.connection_x_notifications, logical_relations);
            }
            EndpointClearanceNotificationAxis::ConnectionY => {
                self.increment_by(&self.coordinate_notifications, logical_relations);
                self.increment_by(&self.connection_y_notifications, logical_relations);
            }
            EndpointClearanceNotificationAxis::FacilityX => {
                self.increment_by(&self.coordinate_notifications, logical_relations);
                self.increment_by(&self.facility_x_notifications, logical_relations);
            }
            EndpointClearanceNotificationAxis::FacilityY => {
                self.increment_by(&self.coordinate_notifications, logical_relations);
                self.increment_by(&self.facility_y_notifications, logical_relations);
            }
            EndpointClearanceNotificationAxis::Orientation => {
                self.increment_by(&self.orientation_notifications, logical_relations);
            }
        }
    }

    pub(super) fn note_enqueued_notification(&self, logical_relations: usize) {
        self.increment_by(&self.enqueued_notifications, logical_relations);
    }

    pub(super) fn note_skipped_false_orientation_notification(&self, logical_relations: usize) {
        self.increment_by(
            &self.skipped_false_orientation_notifications,
            logical_relations,
        );
    }

    pub(super) fn note_shard_batch(&self, dirty_relations: usize, full: bool, scratch: bool) {
        if !self.enabled {
            return;
        }
        self.shard_executions.fetch_add(1, Ordering::Relaxed);
        if scratch {
            self.shard_scratch_executions
                .fetch_add(1, Ordering::Relaxed);
            return;
        } else if full {
            self.shard_full_batches.fetch_add(1, Ordering::Relaxed);
        } else {
            self.shard_endpoint_only_batches
                .fetch_add(1, Ordering::Relaxed);
        }
        let dirty_relations = dirty_relations.try_into().unwrap_or(u64::MAX);
        self.shard_total_dirty_batch_size
            .fetch_add(dirty_relations, Ordering::Relaxed);
        self.shard_maximum_dirty_batch_size
            .fetch_max(dirty_relations, Ordering::Relaxed);
    }

    pub(super) fn note_shard_relation_check(&self) {
        self.increment(&self.shard_dirty_relation_checks);
    }

    pub(in crate::layouts::integrated) fn register_relation(
        &self,
        terminal: &str,
        target_facility: &str,
    ) -> Option<Arc<EndpointClearanceRelationCounters>> {
        if !self.enabled {
            return None;
        }
        let relation = Arc::new(EndpointClearanceRelationCounters {
            terminal: terminal.to_string(),
            target_facility: target_facility.to_string(),
            executions: AtomicU64::default(),
            notifications: AtomicU64::default(),
            executions_without_domain_effect: AtomicU64::default(),
            universally_entailed_executions: AtomicU64::default(),
            executions_with_domain_effect: AtomicU64::default(),
        });
        self.relation_details
            .lock()
            .expect("endpoint-clearance relation registry is not poisoned")
            .push(Arc::clone(&relation));
        Some(relation)
    }

    fn relation_hotset(&self) -> EndpointClearanceRelationHotsetStatistics {
        let details = self
            .relation_details
            .lock()
            .expect("endpoint-clearance relation registry is not poisoned");
        if details.is_empty() {
            return EndpointClearanceRelationHotsetStatistics::default();
        }
        let mut relations = details
            .iter()
            .map(|relation| relation.snapshot())
            .collect::<Vec<_>>();
        relations.sort_by(|left, right| {
            right
                .executions
                .cmp(&left.executions)
                .then_with(|| left.terminal.cmp(&right.terminal))
                .then_with(|| left.target_facility.cmp(&right.target_facility))
        });
        let mut executions = relations
            .iter()
            .map(|relation| relation.executions)
            .collect::<Vec<_>>();
        executions.sort_unstable();
        let percentile = |percent: usize| executions[(executions.len() - 1) * percent / 100];
        let total = executions.iter().copied().map(u128::from).sum::<u128>();
        let share_ppm = |count: usize| {
            if total == 0 {
                return 0;
            }
            let top = relations
                .iter()
                .take(count)
                .map(|relation| u128::from(relation.executions))
                .sum::<u128>();
            u64::try_from(top * 1_000_000 / total).unwrap_or(u64::MAX)
        };
        let aggregate_groups = |select: fn(&EndpointClearanceRelationStatistics) -> &str| {
            let mut groups = BTreeMap::<String, u64>::new();
            for relation in &relations {
                *groups.entry(select(relation).to_string()).or_default() += relation.executions;
            }
            let mut groups = groups
                .into_iter()
                .map(|(entity, executions)| EndpointClearanceGroupStatistics {
                    entity,
                    executions,
                    execution_share_ppm: if total == 0 {
                        0
                    } else {
                        u64::try_from(u128::from(executions) * 1_000_000 / total)
                            .unwrap_or(u64::MAX)
                    },
                })
                .collect::<Vec<_>>();
            groups.sort_by(|left, right| {
                right
                    .executions
                    .cmp(&left.executions)
                    .then_with(|| left.entity.cmp(&right.entity))
            });
            groups.truncate(20);
            groups
        };
        let top_terminals_by_execution = aggregate_groups(|relation| &relation.terminal);
        let top_target_facilities_by_execution =
            aggregate_groups(|relation| &relation.target_facility);
        EndpointClearanceRelationHotsetStatistics {
            collected_relations: relations.len().try_into().unwrap_or(u64::MAX),
            zero_execution_relations: executions
                .iter()
                .filter(|executions| **executions == 0)
                .count()
                .try_into()
                .unwrap_or(u64::MAX),
            execution_p50: percentile(50),
            execution_p95: percentile(95),
            maximum_executions: *executions.last().expect("relation list is non-empty"),
            top_1_execution_share_ppm: share_ppm(1),
            top_10_execution_share_ppm: share_ppm(10),
            top_100_execution_share_ppm: share_ppm(100),
            top_relations_by_execution: relations.into_iter().take(20).collect(),
            top_terminals_by_execution,
            top_target_facilities_by_execution,
        }
    }

    fn note_execution(&self, trigger: ExecutionTrigger, effects: ExecutionEffects) {
        match trigger {
            ExecutionTrigger::Scratch => self.increment(&self.scratch_executions),
            ExecutionTrigger::CoordinateOnly => self.increment(&self.coordinate_only_executions),
            ExecutionTrigger::OrientationOnly => self.increment(&self.orientation_only_executions),
            ExecutionTrigger::Mixed => self.increment(&self.mixed_event_executions),
            ExecutionTrigger::Unclassified => self.increment(&self.unclassified_executions),
        }
        if effects.rejection {
            self.increment(&self.executions_with_rejection);
        }
        if effects.forced_separation {
            self.increment(&self.executions_with_forced_separation);
        }
        if effects.bound_update {
            self.increment(&self.executions_with_bound_update);
        }
        if effects.conflict {
            self.increment(&self.executions_with_conflict);
        }
        if !effects.has_domain_effect() {
            self.increment(&self.executions_without_domain_effect);
            match trigger {
                ExecutionTrigger::Scratch => {
                    self.increment(&self.scratch_executions_without_domain_effect)
                }
                ExecutionTrigger::CoordinateOnly => {
                    self.increment(&self.coordinate_only_executions_without_domain_effect)
                }
                ExecutionTrigger::OrientationOnly => {
                    self.increment(&self.orientation_only_executions_without_domain_effect)
                }
                ExecutionTrigger::Mixed => {
                    self.increment(&self.mixed_event_executions_without_domain_effect)
                }
                ExecutionTrigger::Unclassified => {
                    self.increment(&self.unclassified_executions_without_domain_effect)
                }
            }
        }
    }
}

#[derive(Debug)]
pub(in crate::layouts::integrated) struct EndpointClearanceRelationCounters {
    terminal: String,
    target_facility: String,
    executions: AtomicU64,
    notifications: AtomicU64,
    executions_without_domain_effect: AtomicU64,
    universally_entailed_executions: AtomicU64,
    executions_with_domain_effect: AtomicU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EndpointClearanceNotificationAxis {
    ConnectionX,
    ConnectionY,
    FacilityX,
    FacilityY,
    Orientation,
}

impl EndpointClearanceRelationCounters {
    fn snapshot(&self) -> EndpointClearanceRelationStatistics {
        EndpointClearanceRelationStatistics {
            terminal: self.terminal.clone(),
            target_facility: self.target_facility.clone(),
            executions: self.executions.load(Ordering::Relaxed),
            notifications: self.notifications.load(Ordering::Relaxed),
            executions_without_domain_effect: self
                .executions_without_domain_effect
                .load(Ordering::Relaxed),
            universally_entailed_executions: self
                .universally_entailed_executions
                .load(Ordering::Relaxed),
            executions_with_domain_effect: self
                .executions_with_domain_effect
                .load(Ordering::Relaxed),
        }
    }

    fn note_execution(&self, effects: ExecutionEffects) {
        self.executions.fetch_add(1, Ordering::Relaxed);
        if effects.has_domain_effect() {
            self.executions_with_domain_effect
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.executions_without_domain_effect
                .fetch_add(1, Ordering::Relaxed);
        }
        if effects.universally_entailed {
            self.universally_entailed_executions
                .fetch_add(1, Ordering::Relaxed);
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
    pub priority: Priority,
    pub counters: Arc<EndpointClearancePropagationCounters>,
    pub false_event_filter_enabled: bool,
    pub relation_counters: Option<Arc<EndpointClearanceRelationCounters>>,
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
        PropagatorSpec {
            registration: registration.build(),
            checkers: RuntimeCheckers::empty(),
            propagator: EndpointRectangleClearancePropagator::new_relation_kernel(
                self.name,
                self.connection_x,
                self.connection_y,
                self.facility_x,
                self.facility_y,
                self.orientations,
                self.priority,
                self.counters,
                self.false_event_filter_enabled,
                self.relation_counters,
                self.constraint_tag,
            ),
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
    priority: Priority,
    counters: Arc<EndpointClearancePropagationCounters>,
    false_event_filter_enabled: bool,
    relation_counters: Option<Arc<EndpointClearanceRelationCounters>>,
    pending_trigger_mask: u8,
    entailed_observed: bool,
    inference_code: InferenceCode,
}

const COORDINATE_TRIGGER: u8 = 1 << 0;
const ORIENTATION_TRIGGER: u8 = 1 << 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExecutionTrigger {
    Scratch,
    CoordinateOnly,
    OrientationOnly,
    Mixed,
    Unclassified,
}

impl ExecutionTrigger {
    pub(super) fn from_mask(mask: u8) -> Self {
        match mask {
            COORDINATE_TRIGGER => Self::CoordinateOnly,
            ORIENTATION_TRIGGER => Self::OrientationOnly,
            mask if mask == COORDINATE_TRIGGER | ORIENTATION_TRIGGER => Self::Mixed,
            _ => Self::Unclassified,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ExecutionEffects {
    rejection: bool,
    forced_separation: bool,
    bound_update: bool,
    conflict: bool,
    universally_entailed: bool,
}

impl ExecutionEffects {
    fn has_domain_effect(self) -> bool {
        self.rejection || self.bound_update || self.conflict
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Separation {
    Left,
    Right,
    Above,
    Below,
}

impl Separation {
    const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Above, Self::Below];

    const fn bit(self) -> u8 {
        match self {
            Self::Left => 1 << 0,
            Self::Right => 1 << 1,
            Self::Above => 1 << 2,
            Self::Below => 1 << 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SeparationMask(u8);

impl SeparationMask {
    fn insert(&mut self, separation: Separation) {
        self.0 |= separation.bit();
    }

    const fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn unique(self) -> Option<Separation> {
        if self.0.count_ones() != 1 {
            return None;
        }
        Separation::ALL
            .into_iter()
            .find(|separation| self.0 == separation.bit())
    }
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
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_relation_kernel(
        name: String,
        connection_x: DomainId,
        connection_y: DomainId,
        facility_x: DomainId,
        facility_y: DomainId,
        orientations: Vec<EndpointClearanceOrientation>,
        priority: Priority,
        counters: Arc<EndpointClearancePropagationCounters>,
        false_event_filter_enabled: bool,
        relation_counters: Option<Arc<EndpointClearanceRelationCounters>>,
        constraint_tag: ConstraintTag,
    ) -> Self {
        counters.increment(&counters.relations);
        Self {
            name,
            connection_x,
            connection_y,
            facility_x,
            facility_y,
            orientations,
            priority,
            counters,
            false_event_filter_enabled,
            relation_counters,
            pending_trigger_mask: 0,
            entailed_observed: false,
            inference_code: InferenceCode::new(constraint_tag, EndpointRectangleClearance),
        }
    }

    pub(super) fn note_logical_notification(&self) {
        if let Some(relation_counters) = &self.relation_counters {
            relation_counters
                .notifications
                .fetch_add(1, Ordering::Relaxed);
        }
        if self.entailed_observed {
            self.counters
                .increment(&self.counters.notifications_while_entailed);
        }
    }

    pub(super) fn reset_transient_state(&mut self) {
        self.pending_trigger_mask = 0;
        self.entailed_observed = false;
    }

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
    ) -> SeparationMask {
        let mut possible = SeparationMask::default();
        if bounds.connection_x_lower < bounds.facility_x_upper {
            possible.insert(Separation::Left);
        }
        if bounds.connection_x_upper >= bounds.facility_x_lower + orientation.width {
            possible.insert(Separation::Right);
        }
        if bounds.connection_y_lower < bounds.facility_y_upper {
            possible.insert(Separation::Above);
        }
        if bounds.connection_y_upper >= bounds.facility_y_lower + orientation.height {
            possible.insert(Separation::Below);
        }
        possible
    }

    fn guaranteed_outside(bounds: Bounds, orientation: EndpointClearanceOrientation) -> bool {
        bounds.connection_x_upper < bounds.facility_x_lower
            || bounds.connection_x_lower >= bounds.facility_x_upper + orientation.width
            || bounds.connection_y_upper < bounds.facility_y_lower
            || bounds.connection_y_lower >= bounds.facility_y_upper + orientation.height
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
        let predicates = Separation::ALL
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
        for separation in Separation::ALL {
            if separation != forced {
                predicates.extend(self.impossible_reason(bounds, separation));
            }
        }
        predicates.sort_unstable();
        predicates.dedup();
        PropositionalConjunction::new(predicates)
    }

    fn note_reason(&self, reason: &PropositionalConjunction) {
        self.counters
            .note_maximum_reason_predicates(reason.len().try_into().unwrap_or(u64::MAX));
    }

    fn post(
        &self,
        context: &mut PropagationContext,
        conclusion: Predicate,
        reason: PropositionalConjunction,
        effects: &mut ExecutionEffects,
    ) -> PropagationStatusCP {
        self.note_reason(&reason);
        self.counters.increment(&self.counters.bound_updates);
        effects.bound_update = true;
        if let Err(conflict) = context.post(conclusion, (reason, &self.inference_code)) {
            self.counters.increment(&self.counters.conflicts);
            effects.conflict = true;
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
        effects: &mut ExecutionEffects,
    ) -> PropagationStatusCP {
        self.counters
            .increment(&self.counters.forced_separation_detections);
        effects.forced_separation = true;
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
                        effects,
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
                        effects,
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
                        effects,
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
                        effects,
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
                        effects,
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
                        effects,
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
                        effects,
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
                        effects,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn propagate_all(
        &mut self,
        context: &mut PropagationContext,
        trigger: ExecutionTrigger,
    ) -> PropagationStatusCP {
        self.counters.increment(&self.counters.executions);
        let mut effects = ExecutionEffects::default();
        let result = (|| {
            let bounds = self.bounds(context);
            let mut any_surviving_orientation = false;
            let mut all_surviving_orientations_are_entailed = true;
            for orientation in &self.orientations {
                if context.evaluate_predicate(orientation.selected.get_false_predicate())
                    == Some(true)
                {
                    continue;
                }
                any_surviving_orientation = true;
                all_surviving_orientations_are_entailed &=
                    Self::guaranteed_outside(bounds, *orientation);
                self.counters.increment(&self.counters.orientation_checks);
                let possible = Self::possible_separations(bounds, *orientation);
                if possible.is_empty() {
                    let reason = self.all_impossible_reason(bounds);
                    self.note_reason(&reason);
                    self.counters
                        .increment(&self.counters.rejected_orientations);
                    effects.rejection = true;
                    if let Err(conflict) = context.post(
                        orientation.selected.get_false_predicate(),
                        (reason, &self.inference_code),
                    ) {
                        self.counters.increment(&self.counters.conflicts);
                        effects.conflict = true;
                        return Err(conflict.into());
                    }
                } else if context.evaluate_predicate(orientation.selected.get_true_predicate())
                    == Some(true)
                    && let Some(forced) = possible.unique()
                {
                    self.force_separation(context, bounds, *orientation, forced, &mut effects)?;
                }
            }
            if any_surviving_orientation && all_surviving_orientations_are_entailed {
                effects.universally_entailed = true;
                self.counters
                    .increment(&self.counters.universally_entailed_executions);
                if !self.entailed_observed {
                    self.entailed_observed = true;
                    self.counters.increment(&self.counters.entailment_episodes);
                }
            }
            Ok(())
        })();
        self.counters.note_execution(trigger, effects);
        if let Some(relation_counters) = &self.relation_counters {
            relation_counters.note_execution(effects);
        }
        result
    }
}

impl Propagator for EndpointRectangleClearancePropagator {
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
        self.counters.increment(&self.counters.notifications);
        if let Some(relation_counters) = &self.relation_counters {
            relation_counters
                .notifications
                .fetch_add(1, Ordering::Relaxed);
        }
        if self.entailed_observed {
            self.counters
                .increment(&self.counters.notifications_while_entailed);
        }
        let local_index = local_id.unpack() as usize;
        if local_index < 4 {
            self.counters
                .increment(&self.counters.coordinate_notifications);
            match local_index {
                0 => self
                    .counters
                    .increment(&self.counters.connection_x_notifications),
                1 => self
                    .counters
                    .increment(&self.counters.connection_y_notifications),
                2 => self
                    .counters
                    .increment(&self.counters.facility_x_notifications),
                3 => self
                    .counters
                    .increment(&self.counters.facility_y_notifications),
                _ => unreachable!("coordinate notification local id is in 0..4"),
            }
            self.counters
                .increment(&self.counters.enqueued_notifications);
            self.pending_trigger_mask |= COORDINATE_TRIGGER;
            return EnqueueDecision::Enqueue;
        }

        self.counters
            .increment(&self.counters.orientation_notifications);
        let orientation = self
            .orientations
            .get(local_index - 4)
            .expect("registered orientation local id is valid");
        if self.false_event_filter_enabled
            && context.evaluate_predicate(orientation.selected.get_false_predicate()) == Some(true)
        {
            self.counters
                .increment(&self.counters.skipped_false_orientation_notifications);
            return EnqueueDecision::Skip;
        }

        self.counters
            .increment(&self.counters.enqueued_notifications);
        self.pending_trigger_mask |= ORIENTATION_TRIGGER;
        EnqueueDecision::Enqueue
    }

    fn propagate(&mut self, mut context: PropagationContext) -> PropagationStatusCP {
        let trigger = ExecutionTrigger::from_mask(std::mem::take(&mut self.pending_trigger_mask));
        self.propagate_all(&mut context, trigger)
    }

    fn propagate_from_scratch(&self, mut context: PropagationContext) -> PropagationStatusCP {
        let mut scratch = self.clone();
        scratch.propagate_all(&mut context, ExecutionTrigger::Scratch)
    }

    fn synchronise(&mut self, _context: NotificationContext<'_>) {
        self.pending_trigger_mask = 0;
        self.entailed_observed = false;
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
            priority: Priority::High,
            counters: Arc::default(),
            false_event_filter_enabled: false,
            relation_counters: None,
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
        false_event_filter_enabled: bool,
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
                priority: Priority::High,
                counters: Arc::default(),
                false_event_filter_enabled,
                relation_counters: None,
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
                            let reified = fixed_multiple_orientation_case(
                                false,
                                false,
                                connection,
                                facility,
                                selected_index,
                            );
                            for false_event_filter_enabled in [false, true] {
                                assert_eq!(
                                    fixed_multiple_orientation_case(
                                        true,
                                        false_event_filter_enabled,
                                        connection,
                                        facility,
                                        selected_index,
                                    ),
                                    reified,
                                    "connection={connection:?} facility={facility:?} selected={selected_index} filter={false_event_filter_enabled}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn guaranteed_outside_matches_every_assignment_in_small_bound_domains() {
        let mut solver = Solver::default();
        let selected = solver.new_named_literal("entailed-orientation");
        let orientation = EndpointClearanceOrientation {
            selected,
            selected_parent: *selected.get_integer_variable().inner(),
            width: 2,
            height: 2,
        };
        let ranges = (0..=2)
            .flat_map(|lower| (lower..=2).map(move |upper| (lower, upper)))
            .collect::<Vec<_>>();
        for connection_x in &ranges {
            for connection_y in &ranges {
                for facility_x in &ranges {
                    for facility_y in &ranges {
                        let bounds = Bounds {
                            connection_x_lower: connection_x.0,
                            connection_x_upper: connection_x.1,
                            connection_y_lower: connection_y.0,
                            connection_y_upper: connection_y.1,
                            facility_x_lower: facility_x.0,
                            facility_x_upper: facility_x.1,
                            facility_y_lower: facility_y.0,
                            facility_y_upper: facility_y.1,
                        };
                        let expected = (connection_x.0..=connection_x.1).all(|cx| {
                            (connection_y.0..=connection_y.1).all(|cy| {
                                (facility_x.0..=facility_x.1).all(|fx| {
                                    (facility_y.0..=facility_y.1).all(|fy| {
                                        cx < fx
                                            || cx >= fx + orientation.width
                                            || cy < fy
                                            || cy >= fy + orientation.height
                                    })
                                })
                            })
                        });
                        assert_eq!(
                            EndpointRectangleClearancePropagator::guaranteed_outside(
                                bounds,
                                orientation,
                            ),
                            expected,
                            "connection_x={connection_x:?} connection_y={connection_y:?} facility_x={facility_x:?} facility_y={facility_y:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn relation_hotset_is_sorted_and_reports_execution_concentration() {
        let counters = EndpointClearancePropagationCounters::default();
        let hot = counters
            .register_relation("terminal-hot", "facility-a")
            .expect("enabled counters retain relation details");
        let same_terminal = counters
            .register_relation("terminal-hot", "facility-b")
            .expect("enabled counters retain relation details");
        let same_facility = counters
            .register_relation("terminal-cold", "facility-a")
            .expect("enabled counters retain relation details");
        let cold = counters
            .register_relation("terminal-cold", "facility-b")
            .expect("enabled counters retain relation details");
        for _ in 0..3 {
            hot.note_execution(ExecutionEffects::default());
        }
        for _ in 0..2 {
            same_terminal.note_execution(ExecutionEffects::default());
        }
        same_facility.note_execution(ExecutionEffects::default());
        cold.note_execution(ExecutionEffects {
            universally_entailed: true,
            ..ExecutionEffects::default()
        });

        let hotset = counters.snapshot().relation_hotset;
        assert_eq!(hotset.collected_relations, 4);
        assert_eq!(hotset.zero_execution_relations, 0);
        assert_eq!(hotset.execution_p50, 1);
        assert_eq!(hotset.execution_p95, 2);
        assert_eq!(hotset.maximum_executions, 3);
        assert_eq!(hotset.top_1_execution_share_ppm, 428_571);
        assert_eq!(
            hotset.top_relations_by_execution[0].terminal,
            "terminal-hot"
        );
        assert_eq!(hotset.top_terminals_by_execution[0].entity, "terminal-hot");
        assert_eq!(hotset.top_terminals_by_execution[0].executions, 5);
        assert_eq!(
            hotset.top_target_facilities_by_execution[0].entity,
            "facility-a"
        );
        assert_eq!(hotset.top_target_facilities_by_execution[0].executions, 4);
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
            priority: Priority::High,
            counters: Arc::default(),
            false_event_filter_enabled: false,
            relation_counters: None,
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
            priority: Priority::High,
            counters: Arc::default(),
            false_event_filter_enabled: false,
            relation_counters: None,
            constraint_tag: tag,
        });
        solver.add_clause([selected.get_false_predicate()], tag);
        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
    }

    #[test]
    fn false_orientation_events_can_be_skipped_without_losing_the_selected_sibling() {
        let mut solver = Solver::default();
        let connection_x = solver.new_named_bounded_integer(1, 1, "connection-x");
        let connection_y = solver.new_named_bounded_integer(1, 1, "connection-y");
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let blocked = solver.new_named_literal("blocked-orientation");
        let supported = solver.new_named_literal("supported-orientation");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
            name: "filtered-orientation-events".to_string(),
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
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: true,
            relation_counters: None,
            constraint_tag: tag,
        });
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

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        assert_eq!(
            solver.upper_bound(blocked.get_integer_variable().inner()),
            0
        );
        assert_eq!(
            solver.lower_bound(supported.get_integer_variable().inner()),
            1
        );
        let statistics = counters.snapshot();
        assert!(statistics.skipped_false_orientation_notifications > 0);
        assert!(statistics.enqueued_notifications > 0);
        assert_eq!(
            statistics.notifications,
            statistics.skipped_false_orientation_notifications + statistics.enqueued_notifications
        );
        assert_eq!(
            statistics.coordinate_notifications,
            statistics.connection_x_notifications
                + statistics.connection_y_notifications
                + statistics.facility_x_notifications
                + statistics.facility_y_notifications
        );
        assert_eq!(
            statistics.executions,
            statistics.scratch_executions
                + statistics.coordinate_only_executions
                + statistics.orientation_only_executions
                + statistics.mixed_event_executions
                + statistics.unclassified_executions
        );
        assert!(statistics.executions_without_domain_effect <= statistics.executions);
    }

    #[test]
    fn disabled_false_event_filter_keeps_the_baseline_schedule() {
        let mut solver = Solver::default();
        let connection_x = solver.new_named_bounded_integer(1, 1, "connection-x");
        let connection_y = solver.new_named_bounded_integer(1, 1, "connection-y");
        let facility_x = solver.new_named_bounded_integer(0, 0, "facility-x");
        let facility_y = solver.new_named_bounded_integer(0, 0, "facility-y");
        let selected = solver.new_named_literal("unselected-orientation");
        let counters = Arc::new(EndpointClearancePropagationCounters::default());
        let tag = solver.new_constraint_tag();
        let _ = solver.add_propagator(EndpointRectangleClearancePropagatorArgs {
            name: "unfiltered-orientation-events".to_string(),
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
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: false,
            relation_counters: None,
            constraint_tag: tag,
        });
        solver.add_clause([selected.get_false_predicate()], tag);

        assert_eq!(
            solver.propagate_to_fixpoint(),
            CSPSolverExecutionFlag::Feasible
        );
        let statistics = counters.snapshot();
        assert!(statistics.orientation_notifications > 0);
        assert_eq!(statistics.skipped_false_orientation_notifications, 0);
        assert_eq!(statistics.notifications, statistics.enqueued_notifications);
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
            priority: Priority::High,
            counters: Arc::clone(&counters),
            false_event_filter_enabled: true,
            relation_counters: None,
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
        assert!(
            counters
                .skipped_false_orientation_notifications
                .load(Ordering::Relaxed)
                > 0
        );
    }
}
