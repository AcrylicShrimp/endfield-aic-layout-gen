"""Wall comparisons exclude non-time guard stops and retain selected-case scope."""

import unittest

from tree_search import METHODS
from tree_search_wall import summarize


class WallSummaryTests(unittest.TestCase):
    def cells(self):
        return [{'case': 'a', 'method': method, 'seed': seed, 'best_score': [98 if method == 'puct' else 119, 9, 1],
                 'initial_score': [133, 14, 1], 'termination': 'wall_budget', 'simulated_commands': 100,
                 'cell_seconds': 6.01, 'total_cell_overrun_seconds': .01,
                 'independent_validation': {'status': 'passed', 'translated_facilities': ['f']}}
                for method in METHODS for seed in (17, 18)]

    def spec(self):
        return {'case': 'a', 'methods': list(METHODS), 'sampling_seed_start': 17, 'sampling_seeds': 2,
                'additional_compaction_area_below': 119}

    def test_reports_best_median_compaction_and_paired_outcomes(self):
        result = summarize(self.cells(), self.spec())
        self.assertEqual(result['methods']['puct']['median_score'], [98, 9, 1])
        self.assertEqual(result['methods']['puct']['additional_compactions'], 2)
        self.assertEqual(result['paired_wall_allowances'][1]['outcomes'], {'win': 2})
        self.assertIn('post-selected', result['scope'])

    def test_node_guard_is_not_a_completed_wall_comparison(self):
        cells = self.cells()
        next(row for row in cells if row['method'] == 'puct' and row['seed'] == 17)['termination'] = 'node_budget'
        result = summarize(cells, self.spec())
        self.assertEqual(result['paired_wall_allowances'][1]['outcomes'], {'unmatched_wall_allowance': 1, 'win': 1})

    def test_rejects_incomplete_or_duplicate_cells(self):
        for cells in (self.cells()[:-1], self.cells()[:-1]+[self.cells()[0]]):
            with self.assertRaisesRegex(ValueError, 'require all declared'):
                summarize(cells, self.spec())


if __name__ == '__main__':
    unittest.main()
