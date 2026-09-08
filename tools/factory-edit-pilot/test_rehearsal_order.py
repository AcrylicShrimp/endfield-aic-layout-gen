"""Exact curriculum exposure preservation and complete scheduling-study criteria."""

from collections import Counter
from copy import deepcopy
import json
from pathlib import Path
import unittest

from case_curriculum import CaseCurriculum
from rehearsal_order import decide, orders_for, require_protocol
from update_size_comparison import rehearsal_levels, validate_rehearsal_orders


def fixture():
    def cell(area, successes):
        score = [area, 10, 2]
        return {'case': 'a', 'initial_score': [100, 20, 4], 'median_score': score,
                'translated_successes': successes,
                'episodes': [{'seed': i, 'score': score} for i in range(64)]}
    branches = [{'seed': seed, 'ordering': kind,
                 'assessments': [{'update': i, 'successes': {'a': 50 if kind == 'synchronized' else 55}}
                                 for i in (8, 16, 24, 32)],
                 'roots': [cell(80, 58 if kind == 'synchronized' else 60)]}
                for seed in (1, 2, 3) for kind in ('synchronized', 'staggered')]
    return branches, [cell(90, 62)]


class RehearsalOrderTests(unittest.TestCase):
    def setUp(self):
        self.keys = [f'case{i}' for i in range(16)]
        starts = {key: {level: [] for level in range(1, 10)} for key in self.keys}
        state = {key: {'level': 9 if i < 14 else 8, 'streak': 0} for i, key in enumerate(self.keys)}
        self.controller = CaseCurriculum(starts, state)

    def test_visit_multisets_and_current_starts_are_preserved(self):
        before = deepcopy(self.controller.state)
        orders = orders_for(self.controller, self.keys, 73728, 4)
        self.assertEqual(self.controller.state, before)
        for key in self.keys:
            self.assertEqual(Counter(orders['synchronized'][key]), Counter(orders['staggered'][key]))
            self.assertEqual(len(orders['staggered'][key]), 32)
        sync_peak = max(max(h.values()) for h in orders['histograms']['synchronized'])
        staggered_peak = max(max(h.values()) for h in orders['histograms']['staggered'])
        self.assertGreaterEqual(sync_peak, 14)
        self.assertLess(staggered_peak, sync_peak)
        levels = [self.controller.state[key]['level'] for key in self.keys]
        self.assertEqual(rehearsal_levels(self.keys, levels, ['current']*16, orders['staggered'], 9), levels)
        actual = rehearsal_levels(self.keys, levels, ['rehearsal']*16, orders['staggered'], 9)
        self.assertEqual(actual, [orders['staggered'][key][8] for key in self.keys])
        self.assertEqual(rehearsal_levels(self.keys, levels, ['current']*16, None, 1), levels)

    def test_invalid_orders_and_protocol_are_rejected(self):
        orders = orders_for(self.controller, self.keys, 73728, 4)['staggered']
        bad = deepcopy(orders)
        bad['case0'][0] = 9
        with self.assertRaisesRegex(ValueError, 'update_size.rehearsal_orders'):
            validate_rehearsal_orders(bad, self.controller)
        bad = deepcopy(orders)
        bad['case0'].pop()
        with self.assertRaisesRegex(ValueError, 'update_size.rehearsal_orders'):
            validate_rehearsal_orders(bad, self.controller)
        spec = json.loads(Path(__file__).with_name('rehearsal-order-comparison.json').read_text())
        require_protocol(spec)
        for key, value in (('episodes', 16), ('learning_rate_multiplier', 1), ('updates', 64)):
            with self.assertRaisesRegex(ValueError, 'rehearsal_order.protocol'):
                require_protocol({**spec, key: value})

    def test_all_pairs_and_case_qualification_are_required(self):
        branches, anchor = fixture()
        self.assertTrue(decide(branches, [1, 2, 3], anchor, 52)['candidate_for_replication'])
        branches[0]['roots'][0]['translated_successes'] = 40
        branches[1]['roots'][0]['translated_successes'] = 51
        verdict = decide(branches, [1, 2, 3], anchor, 52)
        self.assertTrue(verdict['criteria']['root_success_nonworse'])
        self.assertFalse(verdict['criteria']['every_staggered_case_qualifies'])
        self.assertFalse(verdict['candidate_for_replication'])
        branches, anchor = fixture()
        branches[1]['roots'][0]['translated_successes'] = 57
        self.assertFalse(decide(branches, [1, 2, 3], anchor, 52)['criteria']['root_success_nonworse'])
        branches, anchor = fixture()
        branches[1]['assessments'][0]['successes']['a'] = 49
        self.assertFalse(decide(branches, [1, 2, 3], anchor, 52)['criteria']['worst_assessment_nonworse'])
        branches, anchor = fixture()
        for b in branches:
            if b['ordering'] == 'staggered' and b['seed'] != 1:
                for gate in b['assessments']:
                    gate['successes']['a'] = 50
        self.assertFalse(decide(branches, [1, 2, 3], anchor, 52)['criteria']['strict_assessment_pairs'])
        with self.assertRaisesRegex(ValueError, 'rehearsal_order.matrix'):
            decide(branches[:-1], [1, 2, 3], anchor, 52)

    def test_quality_priority_and_anchor_retention_remain_visible(self):
        for score in ([81, 1, 0], [80, 11, 0], [80, 10, 3]):
            branches, anchor = fixture()
            branches[1]['roots'][0]['median_score'] = score
            verdict = decide(branches, [1, 2, 3], anchor, 52)
            self.assertFalse(verdict['criteria']['pooled_quality_nonworse'])
            self.assertFalse(verdict['candidate_for_replication'])
        branches, anchor = fixture()
        anchor[0]['median_score'] = [70, 10, 2]
        verdict = decide(branches, [1, 2, 3], anchor, 52)
        self.assertTrue(verdict['criteria']['pooled_quality_nonworse'])
        self.assertFalse(verdict['criteria']['anchor_quality_retained'])
        self.assertFalse(verdict['candidate_for_replication'])


if __name__ == '__main__':
    unittest.main()
