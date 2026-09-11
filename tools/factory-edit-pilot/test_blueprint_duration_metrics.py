"""Check shared metric definitions, paired initial scores and true episode means."""
from copy import deepcopy
import unittest
from unittest.mock import Mock

from blueprint_duration_metrics import define_metrics, evaluation_metrics, metrics, training_quality


def cases():
    return [dict(start=f'factory-{i}', sample=0, total_area_reduction_percent=value,
                 start_score=[100, 5, 1], best_score=[100 - value, 5, 1],
                 policy_actions=64, counts={'status/rejected': i}, generation_seed=i)
            for i, value in enumerate((10., 30.))]


class MetricTests(unittest.TestCase):
    def test_iteration_zero_is_a_real_score_with_zero_paired_delta(self):
        rows = cases()
        result = evaluation_metrics(rows, rows)
        for axis in ('actions', 'batches', 'seconds'):
            self.assertEqual(result[f'evaluation/all_admitted/by_{axis}/area_reduction_percent'], 20.)
            self.assertEqual(result[f'evaluation/all_admitted/by_{axis}/delta_from_initial_pp'], 0.)
        self.assertEqual(result['evaluation/accepted'], 2)
        self.assertEqual(result['evaluation/coverage_fraction'], 1.)
        self.assertEqual(result['evaluation/failed_actions'], 1)
        self.assertEqual(result['evaluation/failure_fraction'], 1 / 128)
        result = metrics(dict(update=0, rows=rows), 0, 256, seconds=3., initial=rows)
        self.assertEqual(result['train/batches'], 0)
        self.assertEqual(result['train/actions'], 0)
        self.assertEqual(result['time/elapsed_seconds'], 3.)

    def test_paired_difference_uses_case_identity_and_rejects_missing_or_drifted_cases(self):
        initial = cases()
        current = deepcopy(initial[::-1])
        current[0]['total_area_reduction_percent'] += 8
        self.assertEqual(evaluation_metrics(current, initial)['evaluation/all_admitted/by_seconds/delta_from_initial_pp'], 4.)
        for altered in (current[:1], [current[0], current[0]]):
            with self.assertRaises(ValueError):
                evaluation_metrics(altered, initial)
        current[0]['start_score'][0] += 1
        with self.assertRaisesRegex(ValueError, 'reset drift'):
            evaluation_metrics(current, initial)

    def test_training_area_quality_requires_completed_episodes_not_reward_per_action(self):
        payload = dict(samples=[dict(outcome=dict(status=s)) for s in ('rejected', 'draft_accepted')], completed=[])
        quality = training_quality(payload)
        self.assertNotIn('mean_area_reduction_percent', quality)
        self.assertNotIn('mean_area_reward', quality)
        self.assertEqual(quality['failure_fraction'], .5)
        payload['completed'] = [dict(initial_score=[100, 5, 1], counts=dict(area_reward=.2),
            result=dict(factory_validated=True, best_metrics=dict(score=[80, 5, 1])))]
        quality = training_quality(payload)
        self.assertAlmostEqual(quality['mean_area_reduction_percent'], 20.)
        self.assertEqual(quality['mean_area_reward'], .2)
        self.assertEqual(quality['accepted'], 1)

    def test_existing_chart_keys_have_their_original_step_axes(self):
        run = Mock()
        define_metrics(run)
        for axis, metric in [('actions', 'train/actions'), ('batches', 'train/batches'), ('seconds', 'time/elapsed_seconds')]:
            run.define_metric.assert_any_call(f'evaluation/all_admitted/by_{axis}/*', step_metric=metric)


if __name__ == '__main__':
    unittest.main()
