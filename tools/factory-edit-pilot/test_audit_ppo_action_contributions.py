"""Check area accounting and restore boundaries in observational follow-ups."""
from copy import deepcopy
import unittest

from audit_ppo_action_contributions import add_episode, bucket


class ContributionTests(unittest.TestCase):
    def example(self):
        kinds = ['port', 'restore_best', 'pose', 'reroute', 'reroute']
        areas = [100, 100, 80, 80, 80]
        previous, trace = 100, []
        for i, (kind, area) in enumerate(zip(kinds, areas)):
            # The port operation improves a lower-priority score only.
            trace.append(dict(action={'kind': kind}, best_score=[area, 1, 1],
                              best_area_before=previous, reward=(previous - area) / 100,
                              physical_before=str(i), physical_after=str(i + 1),
                              physical_unchanged=False,
                              status='improved' if i in (0, 2) else 'accepted_equal'))
            previous = area
        row = dict(initial_metrics={'score': [100, 2, 2]}, best_metrics={'score': [80, 1, 1]},
                   selected=5, kinds={'port': 1, 'restore_best': 1, 'pose': 1, 'reroute': 2},
                   statuses={'improved': 2, 'accepted_equal': 3})
        return row, trace

    def test_secondary_score_is_not_area_and_restore_censors_follow_up(self):
        row, trace = self.example()
        result = bucket()
        add_episode(result, row, trace)
        port = result['kinds']['port']
        self.assertEqual(port['score_improved'], 1)
        self.assertEqual(port['area_improved'], 0)
        self.assertEqual(port['area_pp'], 0)
        self.assertEqual(port['follow4_any_gain'], 1)
        self.assertEqual(port['follow4_gain_before_restore'], 0)
        self.assertEqual(result['kinds']['pose']['area_pp'], 20)
        self.assertEqual(result['area_pp'], 20)

    def test_rejects_reward_or_physical_identity_mismatch(self):
        row, trace = self.example()
        damaged = deepcopy(trace)
        damaged[2]['reward'] = 0
        with self.assertRaisesRegex(ValueError, 'area reward'):
            add_episode(bucket(), row, damaged)
        trace[0]['physical_unchanged'] = True
        with self.assertRaisesRegex(ValueError, 'physical unchanged'):
            add_episode(bucket(), row, trace)

    def test_failure_cost_is_separate_from_area_contribution(self):
        row, trace = self.example()
        row['reward_config'] = {'failure_penalty': .001}
        row['statuses']['accepted_equal'] -= 1
        row['statuses']['executor_rejected'] = 1
        trace[-1].update(status='executor_rejected', area_reward=0, reward=-.001,
                         failure_cost=.001)
        result = bucket()
        add_episode(result, row, trace)
        self.assertEqual(result['area_pp'], 20)
        self.assertEqual(result['kinds']['reroute']['area_pp'], 0)
        self.assertEqual(result['positions']['01-08']['failure_cost_pp'], .1)


if __name__ == '__main__':
    unittest.main()
