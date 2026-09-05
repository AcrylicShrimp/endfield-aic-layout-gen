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
    NetworkValidation,
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
}

#[derive(Default)]
struct Collector {
    stack: Vec<(Instant, u128)>,
    operations: BTreeMap<ConstructiveProfileOperation, ConstructiveOperationTiming>,
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

/// Capture only the calling thread. Nested captures return an error without disturbing the outer
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
            scope: "calling-thread-only",
            elapsed_ns,
            operations: collector.operations,
        },
    ))
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
