use std::collections::{BTreeMap, BTreeSet, HashMap};
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
    row_selector_domains: BTreeSet<DomainId>,
    row_selector_root_recorded: bool,
    row_selector_root_fixed_true: u64,
    row_selector_root_fixed_false: u64,
    row_selector_root_unfixed: u64,
    row_selector_decisions: u64,
    non_row_selector_decisions: u64,
    row_selector_true_decisions: u64,
    row_selector_false_decisions: u64,
    row_selector_unclassified_decisions: u64,
    consecutive_row_selector_decisions: u64,
    maximum_consecutive_row_selector_decisions: u64,
    row_selector_conflict_appearances: u64,
}

impl SearchEventCounters {
    pub(super) fn with_row_selectors(
        domains: impl IntoIterator<Item = DomainId>,
    ) -> SearchEventCounters {
        SearchEventCounters {
            row_selector_domains: domains.into_iter().collect(),
            ..SearchEventCounters::default()
        }
    }

    fn record_root_row_selector_state(&mut self, context: &SelectionContext) {
        if self.row_selector_root_recorded || self.row_selector_domains.is_empty() {
            return;
        }
        for domain in &self.row_selector_domains {
            if context.lower_bound(*domain) == 1 {
                self.row_selector_root_fixed_true += 1;
            } else if context.upper_bound(*domain) == 0 {
                self.row_selector_root_fixed_false += 1;
            } else {
                self.row_selector_root_unfixed += 1;
            }
        }
        self.row_selector_root_recorded = true;
    }

    fn record_decision(&mut self, decision: Predicate) {
        self.branch_decisions += 1;
        if self.row_selector_domains.contains(&decision.get_domain()) {
            self.row_selector_decisions += 1;
            self.consecutive_row_selector_decisions += 1;
            self.maximum_consecutive_row_selector_decisions = self
                .maximum_consecutive_row_selector_decisions
                .max(self.consecutive_row_selector_decisions);
            match boolean_decision_polarity(decision) {
                Some(true) => self.row_selector_true_decisions += 1,
                Some(false) => self.row_selector_false_decisions += 1,
                None => self.row_selector_unclassified_decisions += 1,
            }
        } else {
            self.non_row_selector_decisions += 1;
            self.consecutive_row_selector_decisions = 0;
        }
    }

    pub(super) fn branch_decisions(&self) -> u64 {
        self.branch_decisions
    }
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
        self.counters
            .lock()
            .expect("search event counters are not poisoned")
            .record_root_row_selector_state(context);
        let decision = self.inner.next_decision(context);
        if let Some(decision) = decision {
            self.counters
                .lock()
                .expect("search event counters are not poisoned")
                .record_decision(decision);
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
        let mut counters = self
            .counters
            .lock()
            .expect("search event counters are not poisoned");
        counters.row_selector_conflict_appearances += counters
            .row_selector_domains
            .contains(&predicate.get_domain())
            as u64;
        drop(counters);
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
        if !self
            .counters
            .lock()
            .expect("search event counters are not poisoned")
            .row_selector_domains
            .is_empty()
            && !events.contains(&BrancherEvent::AppearanceInConflictPredicate)
        {
            events.push(BrancherEvent::AppearanceInConflictPredicate);
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
        row_selector_total: (!event_counts.row_selector_domains.is_empty())
            .then_some(event_counts.row_selector_domains.len() as u64),
        row_selector_root_fixed_true: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_root_fixed_true),
        row_selector_root_fixed_false: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_root_fixed_false),
        row_selector_root_unfixed: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_root_unfixed),
        row_selector_decisions: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_decisions),
        non_row_selector_decisions: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.non_row_selector_decisions),
        row_selector_true_decisions: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_true_decisions),
        row_selector_false_decisions: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_false_decisions),
        row_selector_unclassified_decisions: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_unclassified_decisions),
        maximum_consecutive_row_selector_decisions: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.maximum_consecutive_row_selector_decisions),
        row_selector_conflict_appearances: event_counts
            .row_selector_root_recorded
            .then_some(event_counts.row_selector_conflict_appearances),
    }
}

fn boolean_decision_polarity(decision: Predicate) -> Option<bool> {
    let rhs = decision.get_right_hand_side();
    if decision.is_lower_bound_predicate() {
        (rhs == 1).then_some(true)
    } else if decision.is_upper_bound_predicate() {
        (rhs == 0).then_some(false)
    } else if decision.is_equality_predicate() {
        match rhs {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    } else if decision.is_not_equal_predicate() {
        match rhs {
            0 => Some(true),
            1 => Some(false),
            _ => None,
        }
    } else {
        None
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
    use pumpkin_solver::core::predicates::PredicateConstructor;

    #[test]
    fn parses_requested_native_statistics() {
        let parsed = parse_statistics(
            "aic-pumpkin nodes=12\naic-pumpkin failures=3\naic-pumpkin propagations=99\n",
        );

        assert_eq!(parsed.get("nodes"), Some(&12));
        assert_eq!(parsed.get("failures"), Some(&3));
        assert_eq!(parsed.get("propagations"), Some(&99));
    }

    #[test]
    fn classifies_boolean_decision_polarity() {
        let row = DomainId::new(7);
        assert_eq!(
            boolean_decision_polarity(row.lower_bound_predicate(1)),
            Some(true)
        );
        assert_eq!(
            boolean_decision_polarity(row.upper_bound_predicate(0)),
            Some(false)
        );
        assert_eq!(
            boolean_decision_polarity(row.equality_predicate(1)),
            Some(true)
        );
        assert_eq!(
            boolean_decision_polarity(row.equality_predicate(0)),
            Some(false)
        );
        assert_eq!(
            boolean_decision_polarity(row.disequality_predicate(0)),
            Some(true)
        );
        assert_eq!(
            boolean_decision_polarity(row.disequality_predicate(1)),
            Some(false)
        );
    }
}
