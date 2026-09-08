"""Paired Adam initialization and declared stability/quality tradeoffs."""

from copy import deepcopy
import json
from pathlib import Path
import unittest

import torch

from native_training_comparison import equal
from update_size_comparison import decide, optimizer_for, require_protocol


def branches():
    result = []
    for seed in (1, 2, 3):
        for rate in (1, 0.1):
            score = [80, 10, 2]
            result.append({'seed': seed, 'multiplier': rate,
                'assessments': [{'update': i, 'successes': {'a': (8 if rate == 1 else 9)}} for i in (8, 16, 24, 32)],
                'roots': [{'case': 'a', 'initial_score': [100, 20, 4], 'median_score': score,
                           'translated_successes': 12, 'episodes': [{'seed': 10, 'score': score}]}]})
    return result


class UpdateSizeTests(unittest.TestCase):
    def test_quality_cannot_be_hidden_by_higher_reliability(self):
        self.assertTrue(decide(branches(), [1, 2, 3])['candidate_for_larger_test'])
        changed = branches()
        changed[1]['roots'][0]['median_score'] = [81, 1, 0]
        verdict = decide(changed, [1, 2, 3])
        self.assertTrue(verdict['criteria']['stability_nonworse'])
        self.assertFalse(verdict['criteria']['root_quality_nonworse'])
        self.assertFalse(verdict['candidate_for_larger_test'])
        changed = branches()
        changed[1]['roots'][0]['median_score'] = [80, 11, 0]
        self.assertFalse(decide(changed, [1, 2, 3])['candidate_for_larger_test'])
        changed[1]['roots'][0]['median_score'] = [80, 10, 3]
        self.assertFalse(decide(changed, [1, 2, 3])['candidate_for_larger_test'])

    def test_each_pair_and_two_strict_improvements_are_required(self):
        changed = branches()
        for row in changed:
            if row['multiplier'] == 0.1:
                for a in row['assessments']:
                    a['successes']['a'] = 8
        self.assertFalse(decide(changed, [1, 2, 3])['criteria']['stability_strict_pairs'])
        changed = branches()
        changed[1]['roots'][0]['translated_successes'] = 11
        self.assertFalse(decide(changed, [1, 2, 3])['criteria']['root_success_nonworse'])
        with self.assertRaisesRegex(ValueError, 'update_size.matrix'):
            decide(changed[:-1], [1, 2, 3])
        changed[1]['assessments'][0]['update'] = 4
        with self.assertRaisesRegex(ValueError, 'update_size.matrix'):
            decide(changed, [1, 2, 3])

    def test_only_learning_rate_changes_in_restored_adam(self):
        torch.manual_seed(19)
        model = torch.nn.Linear(2, 1)
        optimizer = torch.optim.Adam(model.parameters(), lr=0.0003)
        model(torch.ones(1, 2)).square().mean().backward()
        optimizer.step()
        saved = deepcopy(optimizer.state_dict())
        a, b = deepcopy(model), deepcopy(model)
        high = optimizer_for(a, saved, 0.0003)
        low = optimizer_for(b, saved, 0.00003)
        self.assertTrue(equal(high.state_dict()['state'], low.state_dict()['state']))
        h, l = deepcopy(high.state_dict()), deepcopy(low.state_dict())
        h['param_groups'][0]['lr'] = l['param_groups'][0]['lr']
        self.assertTrue(equal(h, l))
        self.assertTrue(equal(saved, optimizer.state_dict()))
        self.assertTrue(equal(a.state_dict(), b.state_dict()))

    def test_protocol_keeps_all_pairs_and_charged_budgets(self):
        spec = json.loads(Path(__file__).with_name('update-size-comparison.json').read_text())
        require_protocol(spec)
        for key, value in [('updates', 64), ('episodes', 32), ('run_order', spec['run_order'][:-1])]:
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'update_size.protocol'):
                require_protocol({**spec, key: value})


if __name__ == '__main__':
    unittest.main()
