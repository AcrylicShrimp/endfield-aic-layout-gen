//! Bounded immutable-candidate jobs; all workers join before any result is returned.
use super::*;
use crate::layouts::constructive::profiling;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn join_workers<'a, T>(
    handles: Vec<std::thread::ScopedJoinHandle<'a, T>>,
    cancelled: &AtomicBool,
) -> (Vec<T>, Option<ScreenError>) {
    let mut results = Vec::new();
    let mut error = None;
    for handle in handles {
        match handle.join() {
            Ok(result) => results.push(result),
            Err(_) => {
                cancelled.store(true, Ordering::Relaxed);
                error.get_or_insert(ScreenError::WorkerPanic);
            }
        }
    }
    (results, error)
}

pub(super) fn worker_count(requested: usize, jobs: usize, bytes: usize, host: usize) -> usize {
    let memory_workers = (WORKSPACE_BYTES_LIMIT / bytes).saturating_sub(1);
    requested
        .min(16)
        .min(host)
        .min(jobs)
        .min(memory_workers)
        .max(1)
}

pub(super) fn screen(
    base: &Workspace,
    jobs: &[(TransportKind, String)],
    node: &ConstructiveNode,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    workers: usize,
) -> Result<Vec<Outcome>, ScreenError> {
    // Reserve every workspace before starting threads. Partial allocation cannot start analysis.
    let workspaces = (0..workers)
        .map(|_| base.fork())
        .collect::<Result<Vec<_>, _>>()?;
    let next = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let capture = profiling::enabled();
    let (mut results, profiles, error) = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        let mut error = None;
        for mut workspace in workspaces {
            let next = &next;
            let cancelled = &cancelled;
            let active = &active;
            let peak = &peak;
            let worker = move || {
                let count = active.fetch_add(1, Ordering::Relaxed) + 1;
                peak.fetch_max(count, Ordering::Relaxed);
                let _active = Active(active);
                let action = || {
                    let mut results = Vec::new();
                    while !cancelled.load(Ordering::Relaxed) {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some((transport, item)) = jobs.get(index) else {
                            break;
                        };
                        let outcome = screen_commodity(
                            &mut workspace,
                            node,
                            transports,
                            components,
                            *transport,
                            item,
                        );
                        results.push((index, outcome));
                    }
                    results
                };
                if capture {
                    let (results, profile) = collect_constructive_profile(action)
                        .expect("fresh worker has no nested profile capture");
                    (results, Some(profile))
                } else {
                    let mut action = action;
                    (action(), None)
                }
            };
            match std::thread::Builder::new().spawn_scoped(scope, worker) {
                Ok(handle) => handles.push(handle),
                Err(_) => {
                    cancelled.store(true, Ordering::Relaxed);
                    error = Some(ScreenError::WorkerSpawn);
                    break;
                }
            }
        }
        let mut all = Vec::new();
        let mut profiles = Vec::new();
        // Join in worker creation order, never race-select a result or leave a detached worker.
        let (joined, join_error) = join_workers(handles, &cancelled);
        error = error.or(join_error);
        for (results, profile) in joined {
            if let Some(profile) = profile {
                profiles.push((results.len(), profile));
            }
            all.extend(results);
        }
        (all, profiles, error)
    });
    profiling::merge_workers(profiles, peak.load(Ordering::Relaxed));
    if let Some(error) = error {
        return Err(error);
    }
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, outcome)| outcome).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workers_are_bounded_by_jobs_host_and_aggregate_memory() {
        assert_eq!(worker_count(8, 3, 100, 10), 3);
        assert_eq!(worker_count(8, 20, 100, 2), 2);
        assert_eq!(worker_count(8, 0, 100, 10), 1);
        assert_eq!(worker_count(1, 20, 100, 10), 1);
        assert_eq!(worker_count(8, 20, WORKSPACE_BYTES_LIMIT / 3, 10), 2);
        assert_eq!(worker_count(8, 20, WORKSPACE_BYTES_LIMIT, 10), 1);
    }

    #[test]
    fn panic_cancels_claims_and_joins_other_workers_before_returning() {
        let cancelled = AtomicBool::new(false);
        let finished = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let first = scope.spawn(|| -> () {
                panic!("controlled worker panic");
            });
            let second = scope.spawn(|| {
                while !cancelled.load(Ordering::Relaxed) {
                    std::thread::yield_now();
                }
                finished.store(true, Ordering::Relaxed);
            });
            let (_, error) = join_workers(vec![first, second], &cancelled);
            assert_eq!(error, Some(ScreenError::WorkerPanic));
            assert!(finished.load(Ordering::Relaxed));
        });
    }
}
