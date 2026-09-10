"""Registered matrix controls and paired factorial arithmetic."""
from itertools import product
from copy import deepcopy
import json
from pathlib import Path
import unittest

from compare_ppo_combinations import effects, schedule_prefixes
from ppo_actor_critic import FIELDS, treatment, label
from run_ppo_combinations import configurations


class AnalysisTests(unittest.TestCase):
    def test_matrix_changes_only_registered_factors_and_readiness_is_bounded(self):
        base = json.loads(Path(__file__).with_name('ppo-actor-critic-screen.json').read_text())
        cells = configurations(base, False)
        self.assertEqual(len({c['name'] for c in cells}), 8)
        for cell in cells:
            config = cell['config']
            self.assertEqual(config['model'].pop('actor_critic'), cell['factors'])
            self.assertEqual(config, base)
        readiness = configurations(base, True)
        self.assertEqual(len(readiness), 4)
        for cell in readiness:
            self.assertTrue(cell['factors']['critic_after_kl'])
            self.assertEqual(cell['config']['training']['batches'], 2)
            self.assertEqual(cell['config']['retention']['training_batches'], [1, 2])

    def test_main_effects_and_interactions_average_within_seed(self):
        cells = []
        metrics = ('area_reduction_pp', 'later_reduction_pp', 'failure_percent', 'critic_mae_pp',
                   'critic_late_mae_pp', 'critic_late_bias_pp', 'training_seconds', 'update_seconds')
        for s, h, t in product((False, True), repeat=3):
            factors = treatment(s, h, t)
            jobs = {}
            for seed in ('17', '29'):
                value = 2*s+3*h+5*t+7*s*h+11*h*t+13*s*t+17*s*h*t + (4*s if seed == '29' else 0)
                jobs[seed] = dict(summary=dict.fromkeys(metrics, value))
            cells.append(dict(name=label(factors), factors=factors, jobs=jobs))
        result = {tuple(e['factors']): e for e in effects(cells)}
        main = result[(FIELDS[0],)]
        self.assertAlmostEqual(main['by_seed']['17']['area_reduction_pp'], 16.25)
        self.assertAlmostEqual(main['by_seed']['29']['area_reduction_pp'], 20.25)
        self.assertAlmostEqual(main['mean']['area_reduction_pp'], 18.25)
        self.assertAlmostEqual(result[FIELDS[:2]]['mean']['area_reduction_pp'], 15.5)
        self.assertAlmostEqual(result[FIELDS]['mean']['area_reduction_pp'], 17.)

    def test_schedule_divergence_requires_the_first_critic_tail(self):
        hashes = ('policy_before_sha256', 'critic_before_sha256', 'policy_after_sha256',
                  'actor_optimizer_sha256', 'shuffle_rng_sha256', 'critic_after_sha256', 'critic_optimizer_sha256')
        prefix = [dict(dict.fromkeys(hashes, str(i)), selected=64, kl_stopped=i == 2,
                       epochs=[dict(epoch=1, value_loss=.1)]) for i in (1, 2, 3)]
        updates = {bits: {'17': deepcopy(prefix)} for bits in product((False, True), repeat=3)}
        # Critic fitting may diverge at the tail, and subsequent actors may diverge.
        for shared, history in product((False, True), repeat=2):
            tail = updates[(shared, history, True)]['17']
            tail[1]['critic_after_sha256'] = 'after-extra-fit'
            tail[2]['policy_after_sha256'] = 'subsequent-policy'
        summaries = schedule_prefixes(updates)
        self.assertEqual([r['first_tail_batch'] for r in summaries], [2]*4)
        corrupt = deepcopy(updates)
        corrupt[(False, False, True)]['17'][0]['critic_after_sha256'] = 'premature-divergence'
        with self.assertRaisesRegex(ValueError, 'critic differs before first tail'):
            schedule_prefixes(corrupt)
        corrupt = deepcopy(updates)
        corrupt[(True, True, True)]['17'][1]['policy_after_sha256'] = 'changed-actor-at-first-tail'
        with self.assertRaisesRegex(ValueError, 'joint prefix differs'):
            schedule_prefixes(corrupt)


if __name__ == '__main__':
    unittest.main()
