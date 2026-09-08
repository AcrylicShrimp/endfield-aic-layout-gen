"""Terminal boundaries and first-improvement signal classification."""

import json
import unittest

import numpy as np

from policy import advantages
from retained_signals import phases, reference_signals, summarize


class SignalTests(unittest.TestCase):
    def test_terminal_boundaries_and_lambda_one_identity(self):
        rewards, values, dones = [1., 0., 3., 2.], [.3, .1, .5, .2], [False, True, False, True]
        returns, expected = reference_signals(rewards, values, dones, .5, 1.)
        self.assertTrue(np.allclose(returns, [1., 0., 4., 2.]))
        self.assertTrue(np.allclose(expected, returns-values))
        actual, targets = advantages(rewards, values, dones, 0., .5, 1.)
        self.assertTrue(np.allclose(actual, expected))
        self.assertTrue(np.allclose(targets, returns))
        _, shorter = reference_signals(rewards, values, dones, .5, .3)
        actual, _ = advantages(rewards, values, dones, 0., .5, .3)
        self.assertTrue(np.allclose(actual, shorter))
        with self.assertRaisesRegex(ValueError, 'retained_signals.shape'):
            reference_signals([1.], [0.], [False], 1., 1.)

    def test_later_positive_rewards_remain_after_first(self):
        self.assertEqual(phases([0., 2., 0., 1.]),
                         ['before_first_reward', 'first_reward', 'after_first_reward', 'after_first_reward'])
        self.assertEqual(phases([0., 0.]), ['no_positive_episode', 'no_positive_episode'])
        self.assertEqual(phases([1.]), ['first_reward'])

    def test_summary_is_json_serializable_and_preserves_all_observations(self):
        rewards = [0., 1.]+[0.]*14
        record = {'samples': {'rewards': rewards, 'values': [.1]*16, 'dones': [False]*15+[True]},
                  'summary': {'case': 'fixture', 'start_reason': 'current', 'return': 1.},
                  'trace': [{'reward': r} for r in rewards]}
        result = json.loads(json.dumps(summarize([record], {'gamma': 1., 'gae_lambda': .95})))
        self.assertEqual(sum(g['observations'] for g in result['groups']), 16)
        after = next(g for g in result['groups'] if g['phase'] == 'after_first_reward')
        self.assertEqual(after['observations'], 14)
        self.assertEqual(after['sampled_return_sum'], 0.)
        self.assertGreater(after['value_minus_sampled_return_sum'], 0.)


if __name__ == '__main__':
    unittest.main()
