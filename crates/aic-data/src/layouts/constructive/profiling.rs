//! Opt-in, calling-thread wall-time accounting; independent of planner decisions.
use serde::Serialize;
use std::{cell::RefCell, collections::BTreeMap, marker::PhantomData, rc::Rc, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstructiveProfileOperation {
    InputLoading,
    Construction,
    ReportFile,
    CompletionSummary,
    HtmlRendering,
    HtmlWriting,
    GroupGrowth,
    FacilityAddition,
    InternalClosure,
    BoundaryRefresh,
    AttachmentEnumeration,
    SharedPath,
    PathIndexBuild,
    CrossingOptions,
    Astar,
    BoundaryEscape,
    BoundaryWorkspace,
    BoundaryFlood,
    BoundaryDispatch,
    NetworkValidation,
    ValidationSetup,
    ValidationCells,
    ValidationSegments,
    ValidationTerminals,
    ValidationTopology,
    ValidationReachability,
    ValidationIndexBuild,
    CoverageAudit,
    PortDemand,
    CapacityReport,
    NodeClone,
    NodeMerge,
    NodeRotation,
    NodeTranslation,
    NodeCanonicalization,
    HtmlCapacityInspection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ConstructiveOperationTiming {
    pub calls: u64,
    pub inclusive_ns: u128,
    pub exclusive_ns: u128,
    pub max_ns: u128,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub work: BTreeMap<&'static str, u64>,
}

impl ConstructiveOperationTiming {
    fn record(&mut self, inclusive: u128, children: u128) {
        self.calls += 1;
        self.inclusive_ns += inclusive;
        self.exclusive_ns += inclusive - children;
        self.max_ns = self.max_ns.max(inclusive);
    }
}

#[derive(Debug, Serialize)]
pub struct ConstructiveRuntimeProfile {
    pub schema_version: u32,
    pub clock: &'static str,
    pub scope: &'static str,
    pub elapsed_ns: u128,
    pub operations: BTreeMap<ConstructiveProfileOperation, ConstructiveOperationTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workers: Option<ConstructiveWorkerProfile>,
}

#[derive(Debug, Default, Serialize)]
pub struct ConstructiveWorkerProfile {
    pub captures: u64,
    pub busy_captures: u64,
    pub jobs: u64,
    pub wall_ns_sum: u128,
    pub max_parallelism: usize,
    pub slots: BTreeMap<usize, ConstructiveWorkerLoad>,
    pub operations: BTreeMap<ConstructiveProfileOperation, ConstructiveOperationTiming>,
}

#[derive(Debug, Default, Serialize)]
pub struct ConstructiveWorkerLoad {
    pub captures: u64,
    pub jobs: u64,
    pub wall_ns_sum: u128,
}

#[derive(Default)]
struct Collector {
    stack: Vec<(Instant, u128)>,
    operations: BTreeMap<ConstructiveProfileOperation, ConstructiveOperationTiming>,
    workers: ConstructiveWorkerProfile,
}

thread_local! {
    static ACTIVE: RefCell<Option<Collector>> = const { RefCell::new(None) };
}

struct Capture;

impl Drop for Capture {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = None);
    }
}

/// Capture the calling thread and explicitly merged worker captures. Nested captures on the same
/// thread return an error without disturbing the outer
/// capture. A panic unwinds the scopes and disables collection before propagating to the caller.
pub fn collect_constructive_profile<T>(
    action: impl FnOnce() -> T,
) -> Result<(T, ConstructiveRuntimeProfile), std::io::Error> {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if active.is_some() {
            return Err(std::io::Error::other(
                "constructive-profile-nested-capture: a capture is already active on this thread",
            ));
        }
        *active = Some(Collector::default());
        Ok(())
    })?;
    let _capture = Capture;
    let started = Instant::now();
    let result = action();
    let elapsed_ns = started.elapsed().as_nanos();
    let collector = ACTIVE.with(|active| active.borrow_mut().take().unwrap());
    debug_assert!(collector.stack.is_empty());
    Ok((
        result,
        ConstructiveRuntimeProfile {
            schema_version: 1,
            clock: "monotonic-wall-time",
            scope: if collector.workers.captures == 0 {
                "calling-thread-only"
            } else {
                "calling-thread-with-separate-worker-aggregates"
            },
            elapsed_ns,
            operations: collector.operations,
            workers: (collector.workers.captures > 0).then_some(collector.workers),
        },
    ))
}

pub(super) fn enabled() -> bool {
    ACTIVE.with(|active| active.borrow().is_some())
}

/// Worker durations overlap each other and the caller. Never add them to the caller's span stack.
pub(super) fn merge_workers(
    profiles: Vec<(usize, ConstructiveRuntimeProfile)>,
    peak_active: usize,
) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let Some(collector) = active.as_mut() else {
            return;
        };
        let workers = &mut collector.workers;
        workers.max_parallelism = workers.max_parallelism.max(peak_active);
        for (slot, (jobs, profile)) in profiles.into_iter().enumerate() {
            workers.captures += 1;
            workers.busy_captures += u64::from(jobs > 0);
            workers.jobs += jobs as u64;
            workers.wall_ns_sum += profile.elapsed_ns;
            let load = workers.slots.entry(slot).or_default();
            load.captures += 1;
            load.jobs += jobs as u64;
            load.wall_ns_sum += profile.elapsed_ns;
            for (operation, row) in profile.operations {
                let total = workers.operations.entry(operation).or_default();
                total.calls += row.calls;
                total.inclusive_ns += row.inclusive_ns;
                total.exclusive_ns += row.exclusive_ns;
                total.max_ns = total.max_ns.max(row.max_ns);
                for (key, value) in row.work {
                    let current = total.work.entry(key).or_default();
                    if key.starts_with("peak-") {
                        *current = (*current).max(value);
                    } else {
                        *current += value;
                    }
                }
            }
        }
    });
}

/// Measure a pipeline operation without exposing a guard that could escape its capture.
pub fn measure_constructive_operation<T>(
    operation: ConstructiveProfileOperation,
    action: impl FnOnce() -> T,
) -> T {
    let _span = span(operation);
    action()
}

pub(super) struct Span {
    operation: ConstructiveProfileOperation,
    enabled: bool,
    // Guards must be dropped on the thread whose stack they entered.
    _thread: PhantomData<Rc<()>>,
}

impl Span {
    pub(super) fn max_work(&self, key: &'static str, amount: u64) {
        if self.enabled {
            ACTIVE.with(|active| {
                let mut active = active.borrow_mut();
                let current = active
                    .as_mut()
                    .unwrap()
                    .operations
                    .entry(self.operation)
                    .or_default()
                    .work
                    .entry(key)
                    .or_default();
                *current = (*current).max(amount);
            });
        }
    }

    /// Aggregate work once per invocation, never borrow the collector in a traversal's inner loop.
    pub(super) fn add_work(&self, key: &'static str, amount: u64) {
        if self.enabled {
            ACTIVE.with(|active| {
                let mut active = active.borrow_mut();
                let collector = active.as_mut().unwrap();
                *collector
                    .operations
                    .entry(self.operation)
                    .or_default()
                    .work
                    .entry(key)
                    .or_default() += amount;
            });
        }
    }
}

pub(super) fn span(operation: ConstructiveProfileOperation) -> Span {
    let enabled = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if let Some(collector) = active.as_mut() {
            collector.stack.push((Instant::now(), 0));
            true
        } else {
            false
        }
    });
    Span {
        operation,
        enabled,
        _thread: PhantomData,
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            let collector = active.as_mut().unwrap();
            let (started, children) = collector.stack.pop().unwrap();
            let inclusive = started.elapsed().as_nanos();
            collector
                .operations
                .entry(self.operation)
                .or_default()
                .record(inclusive, children);
            if let Some((_, parent_children)) = collector.stack.last_mut() {
                *parent_children += inclusive;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ConstructiveProfileOperation::{Construction, SharedPath};

    #[test]
    fn worker_durations_and_operations_are_separate_from_the_calling_thread_stack() {
        let (_, profile) = collect_constructive_profile(|| {
            measure_constructive_operation(Construction, || {
                let profiles = std::thread::scope(|scope| {
                    (0..2)
                        .map(|_| {
                            scope.spawn(|| {
                                collect_constructive_profile(|| {
                                    measure_constructive_operation(SharedPath, || 7)
                                })
                                .unwrap()
                                .1
                            })
                        })
                        .collect::<Vec<_>>()
                        .into_iter()
                        .map(|h| (1, h.join().unwrap()))
                        .collect()
                });
                merge_workers(profiles, 2);
            });
        })
        .unwrap();
        assert_eq!(profile.operations.len(), 1);
        assert_eq!(
            profile.operations[&Construction].exclusive_ns,
            profile.operations[&Construction].inclusive_ns
        );
        let workers = profile.workers.unwrap();
        assert_eq!(workers.jobs, 2);
        assert_eq!(workers.captures, 2);
        assert_eq!(workers.operations[&SharedPath].calls, 2);
        assert_eq!(workers.slots.values().map(|s| s.jobs).sum::<u64>(), 2);
        assert!(workers.wall_ns_sum >= workers.operations[&SharedPath].exclusive_ns);
    }

    #[test]
    fn accounting_subtracts_children_and_keeps_maximum() {
        let mut row = ConstructiveOperationTiming::default();
        row.record(100, 40);
        row.record(20, 5);
        assert_eq!(row.calls, 2);
        assert_eq!(row.inclusive_ns, 120);
        assert_eq!(row.exclusive_ns, 75);
        assert_eq!(row.max_ns, 100);
    }

    #[test]
    fn nested_and_recursive_scopes_do_not_double_count_exclusive_time() {
        let (_, profile) = collect_constructive_profile(|| {
            measure_constructive_operation(Construction, || {
                measure_constructive_operation(SharedPath, || {
                    measure_constructive_operation(SharedPath, || std::hint::black_box(42));
                });
            });
        })
        .unwrap();
        let parent = &profile.operations[&Construction];
        let child = &profile.operations[&SharedPath];
        assert_eq!(parent.calls, 1);
        assert_eq!(child.calls, 2);
        assert_eq!(
            parent.exclusive_ns + child.exclusive_ns,
            parent.inclusive_ns
        );
        assert!(parent.inclusive_ns <= profile.elapsed_ns);
        assert!(child.inclusive_ns >= child.exclusive_ns);
    }

    #[test]
    fn disabled_capture_and_other_threads_are_not_collected() {
        measure_constructive_operation(SharedPath, || ());
        let (_, profile) = collect_constructive_profile(|| {
            std::thread::spawn(|| measure_constructive_operation(SharedPath, || ()))
                .join()
                .unwrap();
        })
        .unwrap();
        assert!(profile.operations.is_empty());
    }

    #[test]
    fn nested_capture_is_rejected_without_losing_outer_capture() {
        let (_, profile) = collect_constructive_profile(|| {
            measure_constructive_operation(Construction, || {
                assert!(collect_constructive_profile(|| ()).is_err());
            });
        })
        .unwrap();
        assert_eq!(profile.operations[&Construction].calls, 1);
    }

    #[test]
    fn panic_restores_disabled_collection_and_next_capture_is_empty() {
        assert!(
            std::panic::catch_unwind(|| {
                let _ = collect_constructive_profile(|| {
                    measure_constructive_operation(Construction, || panic!("test unwind"));
                });
            })
            .is_err()
        );
        measure_constructive_operation(SharedPath, || ());
        let (_, profile) = collect_constructive_profile(|| ()).unwrap();
        assert!(profile.operations.is_empty());
    }
}
