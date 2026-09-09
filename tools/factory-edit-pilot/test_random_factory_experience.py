"""Fresh draws, scale-aware ranks and the actual score-function objective."""
from copy import deepcopy
from fractions import Fraction
from pathlib import Path
import unittest

import torch

from random_factory_generation import draw, resolve_spec, geometry_identity
from random_factory_collection import fractional_score, fresh_group_gradients, update


class FreshExperienceTests(unittest.TestCase):
    def test_fresh_seeded_draws_cover_products_rates_and_setup(self):
        spec = resolve_spec(Path(__file__).with_name('random-factory-server-pilot.json'))
        draws = [draw(spec, seed) for seed in range(5120000, 5120256)]
        self.assertEqual(draws, [draw(spec, seed) for seed in range(5120000, 5120256)])
        self.assertEqual({d['product'] for d in draws}, {p['id'] for p in spec['products']})
        self.assertGreater(len({(d['product'], d['rate']['duration_ms']) for d in draws}), 80)
        self.assertEqual(len({d['setup_seed'] for d in draws}), 256)
        self.assertTrue(all(d['rate']['duration_ms'] % 1000 == 0 for d in draws))

    def test_geometry_identity_excludes_rate_only_changes(self):
        a = dict(placements=[dict(instance='a', x=0, y=0)], utilizations={'a': '1/2'},
                 transport_networks=[dict(cells=[dict(x=1, y=0)], rate={'numerator': 1, 'denominator': 2})])
        b = deepcopy(a)
        b['utilizations']['a'] = '1/3'
        b['transport_networks'][0]['rate']['denominator'] = 3
        self.assertEqual(geometry_identity(a), geometry_identity(b))
        b['placements'][0]['x'] = 1
        self.assertNotEqual(geometry_identity(a), geometry_identity(b))

    def test_fractional_score_uses_own_reset_and_preserves_priority(self):
        def score(before, after):
            return fractional_score(dict(initial_metrics=dict(score=before), best_metrics=dict(score=after)))
        self.assertEqual(score([1000, 20, 4], [900, 25, 6])[0], Fraction(-1, 10))
        self.assertLess(score([1000, 20, 4], [900, 25, 6]), score([10, 4, 1], [10, 1, 0]))
        self.assertEqual(score([1000, 20, 4], [900, 20, 4]), score([10, 4, 1], [9, 4, 1]))
        self.assertLess(score([100, 20, 4], [100, 19, 8]), score([100, 20, 4], [100, 20, 0]))

    def test_rank_weighted_gradient_matches_direct_distinct_episode_objective(self):
        torch.manual_seed(3)
        model = torch.nn.Linear(3, 2).double()
        inputs = torch.randn(4, 3, 3, dtype=torch.float64)
        scores = [[Fraction(-1, 10), Fraction(0), Fraction(0)],
                  [Fraction(0), Fraction(0), Fraction(0)],
                  [Fraction(-1, 20), Fraction(0), Fraction(0)],
                  [Fraction(0), Fraction(-1, 3), Fraction(0)]]
        ranks = [1, -1, 1 / 3, -1 / 3]
        records, direct = [], 0
        for index, x in enumerate(inputs):
            dist = torch.distributions.Categorical(logits=model(x))
            logp = dist.log_prob(torch.tensor([index % 2, 1, 0])).mean()
            entropy = dist.entropy().mean()
            records.append(dict(log_probability_gradients=torch.autograd.grad(logp, tuple(model.parameters()), retain_graph=True),
                entropy_gradients=torch.autograd.grad(entropy, tuple(model.parameters()), retain_graph=True),
                mean_log_probability=float(logp.detach()), mean_entropy=float(entropy.detach())))
            direct = direct + (-ranks[index] * logp - .001 * entropy) / 4
        expected = torch.autograd.grad(direct, tuple(model.parameters()))
        result = fresh_group_gradients(model, records, scores, .001)
        self.assertEqual(result['returns'], ranks)
        for parameter, gradient in zip(model.parameters(), expected):
            torch.testing.assert_close(parameter.grad, gradient, atol=1e-12, rtol=1e-12)

    def test_no_update_from_rejected_generations(self):
        model = torch.nn.Linear(1, 1)
        optimizer = torch.optim.Adam(model.parameters())
        before = deepcopy(model.state_dict())
        with self.assertRaisesRegex(ValueError, 'insufficient-valid'):
            update(model, optimizer, [dict(status='generation-rejected')], {})
        for name, value in model.state_dict().items():
            torch.testing.assert_close(value, before[name], rtol=0, atol=0)
        self.assertEqual(optimizer.state_dict()['state'], {})


if __name__ == '__main__':
    unittest.main()
