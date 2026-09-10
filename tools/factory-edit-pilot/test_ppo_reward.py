"""Reward conservation, failed-action costs and negative PPO credit."""
from copy import deepcopy
import math
import unittest

from ppo_reward import apply, totals, validate, validate_episode
from ppo_training_update import calculate_gae


class RewardTests(unittest.TestCase):
    def test_only_failed_edits_receive_weak_cost_and_area_still_telescopes(self):
        config = dict(failure_penalty=.001)
        trace = [dict(status=status, reward=reward) for status, reward in [
            ('improved', .1), ('executor_rejected', 0.), ('accepted_exploration', 0.),
            ('rejected', 0.), ('accepted_equal', 0.), ('improved', .15)]]
        for step in trace:
            apply(step, config)
        self.assertEqual([s['reward'] for s in trace], [.1, -.001, 0., -.001, 0., .15])
        row = dict(**totals(trace, config), initial_metrics=dict(score=[100]), best_metrics=dict(score=[75]))
        validate_episode(trace, row, config)
        self.assertAlmostEqual(row['reward_sum'], .248)
        self.assertAlmostEqual(row['area_reward_sum'], .25)
        self.assertAlmostEqual(row['failure_cost_sum'], .002)
        self.assertEqual(row['failed_actions'], 2)
        for key in ('reward', 'area_reward', 'failure_cost', 'status'):
            changed = deepcopy(trace)
            changed[1][key] = 'accepted_equal' if key == 'status' else 0.1
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_episode(changed, row, config)
        with self.assertRaises(ValueError):
            validate_episode(trace, row, dict(failure_penalty=.002))

    def test_failure_only_episode_has_small_negative_return_and_gae(self):
        config = dict(failure_penalty=.001)
        trace = [dict(status='executor_rejected', reward=0.) for _ in range(32)]
        for step in trace:
            apply(step, config)
        row = dict(**totals(trace, config), initial_metrics=dict(score=[3366]), best_metrics=dict(score=[3366]))
        validate_episode(trace, row, config)
        self.assertAlmostEqual(row['reward_sum'], -.032)
        advantage, target = calculate_gae([s['reward'] for s in trace], [0.] * 32, .95)
        self.assertTrue((advantage < 0).all() and (target < 0).all())
        self.assertTrue(math.isfinite(float(advantage.sum())))

    def test_invalid_coefficients_and_improving_failures_are_rejected(self):
        for value in (-.001, .011, float('nan'), float('inf'), True):
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate(dict(failure_penalty=value))
        for penalty in (0., .001):
            with self.assertRaises(ValueError):
                apply(dict(status='rejected', reward=.01), dict(failure_penalty=penalty))


if __name__ == '__main__':
    unittest.main()
