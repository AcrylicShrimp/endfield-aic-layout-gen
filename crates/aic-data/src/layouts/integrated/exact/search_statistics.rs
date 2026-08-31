use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, ThreadId};

use pumpkin_solver::Solver;
use pumpkin_solver::core::branching::{Brancher, BrancherEvent, SelectionContext};
use pumpkin_solver::core::conflict_resolving::ConflictResolver;
use pumpkin_solver::core::predicates::Predicate;
use pumpkin_solver::core::results::SolutionReference;
use pumpkin_solver::core::statistics::configure_statistic_logging;
use pumpkin_solver::core::variables::DomainId;

use crate::layouts::integrated::ExactSearchStatistics;

#[derive(Debug, Default)]
pub(super) struct SearchEventCounters {
    branch_decisions: u64,
    backtracks: u64,
}

#[derive(Debug)]
pub(super) struct MeteredBrancher<B> {
    inner: B,
    counters: Arc<Mutex<SearchEventCounters>>,
}

impl<B> MeteredBrancher<B> {
    pub(super) fn new(inner: B, counters: Arc<Mutex<SearchEventCounters>>) -> Self {
        Self { inner, counters }
    }
}

impl<B: Brancher> Brancher for MeteredBrancher<B> {
    fn log_statistics(&self, statistic_logger: pumpkin_solver::core::statistics::StatisticLogger) {
        self.inner.log_statistics(statistic_logger);
    }

    fn next_decision(&mut self, context: &mut SelectionContext) -> Option<Predicate> {
        let decision = self.inner.next_decision(context);
        if decision.is_some() {
            self.counters
                .lock()
                .expect("search event counters are not poisoned")
                .branch_decisions += 1;
        }
        decision
    }

    fn on_conflict(&mut self) {
        self.inner.on_conflict();
    }

    fn on_backtrack(&mut self) {
        self.counters
            .lock()
            .expect("search event counters are not poisoned")
            .backtracks += 1;
        self.inner.on_backtrack();
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
        self.inner.on_restart();
    }

    fn synchronise(&mut self, context: &mut SelectionContext) {
        self.inner.synchronise(context);
    }

    fn is_restart_pointless(&mut self) -> bool {
        self.inner.is_restart_pointless()
    }

    fn subscribe_to_events(&self) -> Vec<BrancherEvent> {
        let mut events = self.inner.subscribe_to_events();
        if !events.contains(&BrancherEvent::Backtrack) {
            events.push(BrancherEvent::Backtrack);
        }
        events
    }
}

#[derive(Debug, Default)]
struct ThreadStatisticBuffers {
    by_thread: Mutex<HashMap<ThreadId, Vec<u8>>>,
}

#[derive(Debug, Clone)]
struct ThreadStatisticWriter {
    buffers: Arc<ThreadStatisticBuffers>,
}

impl Write for ThreadStatisticWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffers
            .by_thread
            .lock()
            .map_err(|_| io::Error::other("Pumpkin statistic buffer is poisoned"))?
            .entry(thread::current().id())
            .or_default()
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

static STATISTIC_BUFFERS: OnceLock<Arc<ThreadStatisticBuffers>> = OnceLock::new();

fn statistic_buffers() -> Arc<ThreadStatisticBuffers> {
    Arc::clone(STATISTIC_BUFFERS.get_or_init(|| {
        let buffers = Arc::new(ThreadStatisticBuffers::default());
        configure_statistic_logging(
            "aic-pumpkin",
            None,
            None,
            Some(Box::new(ThreadStatisticWriter {
                buffers: Arc::clone(&buffers),
            })),
        );
        buffers
    }))
}

pub(super) fn capture_search_statistics(
    solver: &Solver,
    brancher: &impl Brancher,
    resolver: &impl ConflictResolver,
    counters: &Arc<Mutex<SearchEventCounters>>,
) -> ExactSearchStatistics {
    let buffers = statistic_buffers();
    let thread_id = thread::current().id();
    buffers
        .by_thread
        .lock()
        .expect("Pumpkin statistic buffers are not poisoned")
        .remove(&thread_id);
    solver.log_statistics(brancher, resolver, true);
    let bytes = buffers
        .by_thread
        .lock()
        .expect("Pumpkin statistic buffers are not poisoned")
        .remove(&thread_id)
        .unwrap_or_default();
    let values = parse_statistics(&String::from_utf8_lossy(&bytes));
    let event_counts = counters
        .lock()
        .expect("search event counters are not poisoned");

    ExactSearchStatistics {
        branch_decisions: values
            .get("nodes")
            .copied()
            .or(Some(event_counts.branch_decisions)),
        backtracks: Some(event_counts.backtracks),
        conflicts: values.get("failures").copied(),
        learned_clauses: values.get("nogoods").copied(),
        solver_propagations: values.get("propagations").copied(),
        // Pumpkin 0.5 logs this field but never increments its backing counter.
        atomic_propagations: None,
        restarts: values.get("restarts").copied(),
    }
}

fn parse_statistics(output: &str) -> BTreeMap<String, u64> {
    output
        .lines()
        .filter_map(|line| {
            let (name, value) = line.strip_prefix("aic-pumpkin ")?.split_once('=')?;
            value
                .parse::<u64>()
                .ok()
                .map(|value| (name.to_string(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requested_native_statistics() {
        let parsed = parse_statistics(
            "aic-pumpkin nodes=12\naic-pumpkin failures=3\naic-pumpkin propagations=99\n",
        );

        assert_eq!(parsed.get("nodes"), Some(&12));
        assert_eq!(parsed.get("failures"), Some(&3));
        assert_eq!(parsed.get("propagations"), Some(&99));
    }
}
