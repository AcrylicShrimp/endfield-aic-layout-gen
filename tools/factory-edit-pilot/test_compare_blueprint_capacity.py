"""Reject invalid pairing and preserve non-architecture experimental factors."""
from copy import deepcopy
import unittest

from blueprint_capacity import child_config
from blueprint_ppo_store import ROOT, read
from compare_blueprint_capacity import common_config, compare_rows


class CapacityComparisonTests(unittest.TestCase):
    def test_case_identity_is_paired_independently_of_order(self):
        baseline = [dict(generation_seed=i, sample=0, start_score=[100, 20, 3],
                         request_sha256=str(i), initial_sha256=str(i), action_seed=i + 10,
                         total_area_reduction_percent=10.) for i in range(3)]
        candidate = deepcopy(baseline)
        candidate[0]['total_area_reduction_percent'] = 11.
        candidate[1]['total_area_reduction_percent'] = 9.
        self.assertEqual(compare_rows(list(reversed(candidate)), baseline), dict(win=1, tie=1, loss=1))
        candidate[0]['action_seed'] += 1
        with self.assertRaisesRegex(ValueError, 'paired capacity reset'):
            compare_rows(candidate, baseline)
        with self.assertRaisesRegex(ValueError, 'paired capacity cohort'):
            compare_rows(baseline[:2], baseline)

    def test_only_registered_factors_are_removed(self):
        base = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-64-12h.json')
        spec = read(ROOT / 'tools/factory-edit-pilot/blueprint-capacity-screen.json')
        configs = [child_config(base, spec, m, seed, 'matrix') for m in spec['models'] for seed in spec['seeds']]
        self.assertTrue(all(common_config(c) == common_config(configs[0]) for c in configs))
        altered = deepcopy(configs[-1])
        altered['ppo']['learning_rate'] *= 2
        self.assertNotEqual(common_config(altered), common_config(configs[0]))
        altered = deepcopy(configs[-1])
        altered['evaluation']['max_actions'] += 1
        self.assertNotEqual(common_config(altered), common_config(configs[0]))


if __name__ == '__main__':
    unittest.main()
