"""Sampling uncertainty preserves paired seed clusters across fixed cases."""

from copy import deepcopy
import unittest

from endpoint_precision import bootstrap, summarize


class PrecisionTests(unittest.TestCase):
    def test_opposed_case_effects_cancel_in_every_seed_cluster(self):
        result = bootstrap([[1, -1], [-1, 1], [1, -1], [-1, 1]], 17, 1000)
        self.assertEqual(result['mean_difference'], 0)
        self.assertEqual(result['mean_interval'], [0, 0])
        self.assertLess(result['case_intervals'][0][0], 0)
        self.assertGreater(result['case_intervals'][0][1], 0)
        fixed = bootstrap([[1, 1], [1, 1]], 17, 101)
        self.assertEqual(fixed['mean_interval'], [1, 1])

    def test_paired_scores_and_seed_alignment(self):
        def cell(case, scores):
            return {'case': case, 'initial_score': [100, 20, 4], 'median_score': scores[-1],
                    'episodes': [{'seed': i, 'score': score, 'translated_facilities': ['F']}
                                 for i, score in enumerate(scores)]}
        high = [cell('a', [[100, 20, 4], [80, 10, 2]]), cell('b', [[80, 10, 2], [100, 20, 4]])]
        low = [cell('a', [[80, 10, 2], [100, 20, 4]]), cell('b', [[100, 20, 4], [80, 10, 2]])]
        result = summarize(high, low, 17, 1000)
        self.assertEqual(result['bootstrap']['mean_interval'], [0, 0])
        self.assertEqual(result['pooled_low_vs_high'], {'wins': 2, 'ties': 0, 'losses': 2})
        bad = deepcopy(low)
        bad[1]['episodes'][0]['seed'] = 10
        with self.assertRaisesRegex(ValueError, 'endpoint_precision.pairs'):
            summarize(high, bad, 17, 1000)


if __name__ == '__main__':
    unittest.main()
