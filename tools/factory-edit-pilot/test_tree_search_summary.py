"""Tree comparisons preserve lexicographic scores and exclude exhausted paired budgets."""

import unittest

from tree_search_summary import method_metrics, paired


def cell(method, area, tiles=2, completed=True, case='a', initial=10, translated=True):
    return {'case': case, 'seed': 17, 'method': method, 'best_score': [area, tiles, 0],
            'initial_score': [initial, 4, 0], 'transition_allowance_completed': completed,
            'cell_seconds': 1, 'simulated_commands': 128 if completed else 64,
            'termination': 'transition_budget' if completed else 'wall_budget',
            'independent_validation': {'translated_facilities': ['f'] if translated else []}}


class TreeSummaryTests(unittest.TestCase):
    def test_area_ratio_gives_each_case_equal_weight(self):
        result = method_metrics([cell('uct', 6), cell('uct', 90, case='b', initial=100)])
        self.assertEqual(result['mean_median_area_ratio_exact'], '3/4')

    def test_area_gain_requires_translation(self):
        result = method_metrics([cell('uct', 8, translated=False)])
        self.assertEqual(result['translated_successes'], 0)

    def test_paired_scores_prioritize_area_then_tiles(self):
        result = paired([cell('uct', 8, tiles=5), cell('random_rollouts', 9, tiles=1)], 'uct', 'random_rollouts')
        self.assertEqual(result['outcomes'], {'win': 1})
        result = paired([cell('uct', 8, tiles=5), cell('random_rollouts', 8, tiles=1)], 'uct', 'random_rollouts')
        self.assertEqual(result['outcomes'], {'loss': 1})

    def test_early_wall_stop_is_not_a_matched_transition_pair(self):
        result = paired([cell('uct', 8, completed=False), cell('random_rollouts', 9)], 'uct', 'random_rollouts')
        self.assertEqual(result['outcomes'], {'unmatched_allowance': 1})
        self.assertFalse(result['pairs'])
        self.assertFalse(result['wall_time_matched'])


if __name__ == '__main__':
    unittest.main()
