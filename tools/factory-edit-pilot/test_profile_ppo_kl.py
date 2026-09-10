"""Positive immediate rewards need not imply positive advantage."""
import unittest

import numpy as np

from profile_ppo_kl import advantages


class AdvantageTests(unittest.TestCase):
    def test_positive_rewards_below_prediction_have_negative_advantage(self):
        actual = advantages([dict(reward=.05, old_value=.2), dict(reward=.05, old_value=.1)], .95)
        np.testing.assert_allclose(actual, [-.0975, -.05], rtol=1e-6)

    def test_future_gain_assigns_credit_to_zero_reward_action(self):
        actual = advantages([dict(reward=0., old_value=0.), dict(reward=.5, old_value=0.)], .95)
        np.testing.assert_allclose(actual, [.475, .5], rtol=1e-6)


if __name__ == '__main__':
    unittest.main()
