"""Snapshot rates use action counts; area contribution uses episode means."""
import unittest

from profile_ppo_progress import summarize


def result(selected, area, failed):
    return dict(status='complete', row=dict(selected=selected,
        statuses=dict(accepted_exploration=selected - failed, executor_rejected=failed),
        kinds=dict(pose=selected), failed_actions=failed,
        routing_failure_codes={'local-no-path': failed, 'region-fixed-boundary': failed},
        area_reward_by_kind=dict(pose=area), area_reward_sum=area,
        routing_budget_failed_actions=0, first_eight_area_reward_sum=area,
        later_area_reward_sum=0, failure_cost_sum=.001 * failed,
        accepted_physical_unchanged_actions=0, area_improving_actions=int(area > 0),
        routing_first_attempt_budget_failures=0))


class ProgressTests(unittest.TestCase):
    def test_action_weighted_failures_and_episode_weighted_gain(self):
        rows = [result(2, .2, 1), result(6, .4, 1), dict(status='generation_rejected')]
        actual = summarize(rows)
        self.assertEqual(actual['attempts'], 3)
        self.assertEqual(actual['episodes'], 2)
        self.assertEqual(actual['failure_percent'], 25)
        self.assertAlmostEqual(actual['mean_area_reduction_percent'], 30)
        self.assertAlmostEqual(actual['contribution_pp']['pose'], 30)
        self.assertAlmostEqual(actual['mean_failure_cost_pp'], .1)
        # The same failure has two diagnostic tiers, but counts once as rejected.
        self.assertEqual(sum(actual['failure_codes'].values()), 4)
        self.assertEqual(actual['failed'], 2)

    def test_inconsistent_failure_counter_is_rejected(self):
        row = result(2, .2, 1)
        row['row']['failed_actions'] = 0
        with self.assertRaisesRegex(ValueError, 'failure counters differ'):
            summarize([row])


if __name__ == '__main__':
    unittest.main()
