use std::time::{Duration, Instant};

use super::IterativeOptimizationConfig;

pub(super) struct StrategyBudget {
    strategy_deadline: Instant,
    ordinary_pool: Duration,
    reserve_remaining: Duration,
    minimum_attempt: Duration,
}

pub(super) struct GrowthPhaseBudget {
    strategy_deadline: Instant,
    phase_started: Instant,
    ordinary_grant: Duration,
    deadline: Instant,
    reserve_borrowed: Duration,
    borrow_used: bool,
}

impl StrategyBudget {
    pub(super) fn new(config: &IterativeOptimizationConfig, now: Instant) -> Self {
        let total = Duration::from_millis(config.total_time_limit_ms);
        let reserve_millis = config
            .total_time_limit_ms
            .saturating_mul(u64::from(config.final_refinement_reserve_percent))
            / 100;
        let reserve = Duration::from_millis(reserve_millis);
        Self {
            strategy_deadline: now.checked_add(total).unwrap_or(now),
            ordinary_pool: total.saturating_sub(reserve),
            reserve_remaining: reserve,
            minimum_attempt: Duration::from_millis(config.minimum_phase_attempt_ms),
        }
    }

    pub(super) fn strategy_deadline(&self) -> Instant {
        self.strategy_deadline
    }

    pub(super) fn begin_growth_phase(
        &mut self,
        remaining_growth_phases: usize,
        now: Instant,
    ) -> GrowthPhaseBudget {
        let divisor = u32::try_from(remaining_growth_phases)
            .unwrap_or(u32::MAX)
            .max(1);
        let ordinary_grant = self.ordinary_pool / divisor;
        self.ordinary_pool = self.ordinary_pool.saturating_sub(ordinary_grant);
        GrowthPhaseBudget {
            strategy_deadline: self.strategy_deadline,
            phase_started: now,
            ordinary_grant,
            deadline: min_instant(
                self.strategy_deadline,
                now.checked_add(ordinary_grant).unwrap_or(now),
            ),
            reserve_borrowed: Duration::ZERO,
            borrow_used: false,
        }
    }

    pub(super) fn borrow_for_missing_incumbent(
        &mut self,
        phase: &mut GrowthPhaseBudget,
    ) -> Duration {
        if phase.borrow_used || self.reserve_remaining.is_zero() {
            return Duration::ZERO;
        }
        let borrowed = self.reserve_remaining.min(self.minimum_attempt);
        self.reserve_remaining = self.reserve_remaining.saturating_sub(borrowed);
        phase.borrow_used = true;
        phase.reserve_borrowed = borrowed;
        phase.deadline = min_instant(
            phase.strategy_deadline,
            phase
                .deadline
                .checked_add(borrowed)
                .unwrap_or(phase.deadline),
        );
        borrowed
    }

    pub(super) fn finish_growth_phase(&mut self, phase: &GrowthPhaseBudget, now: Instant) {
        let elapsed = now.saturating_duration_since(phase.phase_started);
        self.ordinary_pool += phase.ordinary_grant.saturating_sub(elapsed);
    }

    pub(super) fn final_refinement_grant(&self, now: Instant) -> Duration {
        let available = self.ordinary_pool + self.reserve_remaining;
        available.min(self.strategy_deadline.saturating_duration_since(now))
    }

    pub(super) fn begin_final_refinement(&mut self, now: Instant) -> GrowthPhaseBudget {
        let grant = self.final_refinement_grant(now);
        self.ordinary_pool = Duration::ZERO;
        self.reserve_remaining = Duration::ZERO;
        GrowthPhaseBudget {
            strategy_deadline: self.strategy_deadline,
            phase_started: now,
            ordinary_grant: grant,
            deadline: min_instant(
                self.strategy_deadline,
                now.checked_add(grant).unwrap_or(now),
            ),
            reserve_borrowed: Duration::ZERO,
            borrow_used: true,
        }
    }

    #[cfg(test)]
    fn ordinary_pool(&self) -> Duration {
        self.ordinary_pool
    }

    #[cfg(test)]
    fn reserve_remaining(&self) -> Duration {
        self.reserve_remaining
    }
}

impl GrowthPhaseBudget {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn remaining(&self, now: Instant) -> Duration {
        self.deadline.saturating_duration_since(now)
    }

    pub(super) fn reserve_borrowed(&self) -> Duration {
        self.reserve_borrowed
    }
}

fn min_instant(left: Instant, right: Instant) -> Instant {
    if left <= right { left } else { right }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> IterativeOptimizationConfig {
        IterativeOptimizationConfig {
            total_time_limit_ms: 1_000,
            final_refinement_reserve_percent: 20,
            minimum_phase_attempt_ms: 100,
            ..IterativeOptimizationConfig::default()
        }
    }

    #[test]
    fn divides_the_ordinary_pool_and_returns_unused_time() {
        let now = Instant::now();
        let mut budget = StrategyBudget::new(&config(), now);
        let phase = budget.begin_growth_phase(2, now);
        assert_eq!(phase.remaining(now), Duration::from_millis(400));

        budget.finish_growth_phase(&phase, now + Duration::from_millis(150));
        assert_eq!(budget.ordinary_pool(), Duration::from_millis(650));
        assert_eq!(budget.reserve_remaining(), Duration::from_millis(200));
    }

    #[test]
    fn permits_exactly_one_minimum_attempt_reserve_borrow() {
        let now = Instant::now();
        let mut budget = StrategyBudget::new(&config(), now);
        let mut phase = budget.begin_growth_phase(1, now);

        assert_eq!(
            budget.borrow_for_missing_incumbent(&mut phase),
            Duration::from_millis(100)
        );
        assert_eq!(
            budget.borrow_for_missing_incumbent(&mut phase),
            Duration::ZERO
        );
        assert_eq!(phase.reserve_borrowed(), Duration::from_millis(100));
        assert_eq!(budget.reserve_remaining(), Duration::from_millis(100));
    }

    #[test]
    fn final_grant_contains_returned_ordinary_time_and_reserve_remainder() {
        let now = Instant::now();
        let mut budget = StrategyBudget::new(&config(), now);
        let phase = budget.begin_growth_phase(1, now);
        budget.finish_growth_phase(&phase, now + Duration::from_millis(300));

        assert_eq!(
            budget.final_refinement_grant(now + Duration::from_millis(300)),
            Duration::from_millis(700)
        );
    }

    #[test]
    fn final_refinement_consumes_the_remaining_pools_once() {
        let now = Instant::now();
        let mut budget = StrategyBudget::new(&config(), now);
        let final_budget = budget.begin_final_refinement(now);

        assert_eq!(final_budget.remaining(now), Duration::from_secs(1));
        assert_eq!(budget.final_refinement_grant(now), Duration::ZERO);
    }

    #[test]
    fn insufficient_ordinary_pool_does_not_invent_a_minimum_attempt_grant() {
        let now = Instant::now();
        let config = IterativeOptimizationConfig {
            total_time_limit_ms: 100,
            final_refinement_reserve_percent: 0,
            minimum_phase_attempt_ms: 50,
            ..IterativeOptimizationConfig::default()
        };
        let mut budget = StrategyBudget::new(&config, now);
        let mut phase = budget.begin_growth_phase(10, now);

        assert_eq!(phase.remaining(now), Duration::from_millis(10));
        assert_eq!(
            budget.borrow_for_missing_incumbent(&mut phase),
            Duration::ZERO
        );
        assert_eq!(phase.remaining(now), Duration::from_millis(10));
    }
}
