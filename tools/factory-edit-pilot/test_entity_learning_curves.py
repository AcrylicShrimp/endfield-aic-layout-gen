"""Retention regressions: case weighting, strict reliability and observed time budgets."""

import unittest

from entity_learning_curves import checkpoint_metrics, matched_times, select_checkpoint


def episode(area, translated=True, tiles=1, turns=0):
    return {'score': [area, tiles, turns], 'translated_facilities': ['facility'] if translated else []}


class EntityCurveTests(unittest.TestCase):
    def test_equal_case_weighting_and_actual_upper_median(self):
        value = checkpoint_metrics({'small': [episode(4), episode(6)], 'large': [episode(70), episode(90)]},
                                   {'small': [10, 2, 0], 'large': [100, 2, 0]}, 2)
        self.assertEqual(value['quality_exact'][0], '3/4')
        self.assertEqual(value['cells']['small']['median_score'][0], 6)
        self.assertTrue(value['eligible'])

    def test_every_case_requires_translated_area_improvement(self):
        value = checkpoint_metrics({'a': [episode(8)]*4,
                                    'b': [episode(8), episode(8, False), episode(10), episode(10)]},
                                   {'a': [10, 3, 0], 'b': [10, 3, 0]}, 2)
        self.assertFalse(value['eligible'])
        self.assertEqual(value['cells']['b']['translated_successes'], 1)

    def test_selection_uses_quality_then_earlier_checkpoint(self):
        def point(count, quality, eligible=True):
            return {'commands': count, 'quality_exact': quality, 'eligible': eligible}
        chosen = select_checkpoint([point(100, ['1/2', '3', '2']), point(200, ['1/2', '3', '1']),
                                    point(300, ['1/2', '3', '1']), point(400, ['1/3', '1', '0'], False)])
        self.assertEqual(chosen['commands'], 200)
        self.assertIsNone(select_checkpoint([point(10, ['1', '1', '0'], False)]))

    def test_time_comparison_never_interpolates_or_uses_future_policy(self):
        runs = [{'width': width, 'seed': 17, 'training_seconds': total,
                 'curve': [{'commands': count, 'training_seconds': seconds}
                           for count, seconds in points]}
                for width, total, points in [(1, 100, [(0, 0), (100, 30), (200, 100)]),
                                            (2, 200, [(0, 0), (100, 60), (200, 200)])]]
        comparisons = matched_times(runs)
        self.assertEqual(comparisons[1]['budget_seconds'], 50)
        self.assertEqual([row['point']['commands'] for row in comparisons[1]['observations']], [100, 0])
        self.assertEqual([row['point']['commands'] for row in comparisons[-1]['observations']], [200, 100])


if __name__ == '__main__':
    unittest.main()
