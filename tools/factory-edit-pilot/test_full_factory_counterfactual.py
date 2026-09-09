"""Check the distinction between momentum-only, reset and skipped updates."""
from copy import deepcopy
import unittest

import torch

from full_factory_counterfactual import branch_step, check_baseline, identical


class CounterfactualTests(unittest.TestCase):
    def setUp(self):
        self.model = torch.nn.Linear(1, 1, bias=False).double()
        with torch.no_grad():
            self.model.weight.fill_(1)
        self.optimizer = torch.optim.Adam(self.model.parameters(), lr=.03)
        self.model.weight.grad = torch.full_like(self.model.weight, 2)
        self.optimizer.step()
        self.before = deepcopy(dict(policy=self.model.state_dict(), optimizer=self.optimizer.state_dict()))

    def fresh(self):
        model = torch.nn.Linear(1, 1, bias=False).double()
        model.load_state_dict(self.before['policy'])
        optimizer = torch.optim.Adam(model.parameters())
        optimizer.load_state_dict(deepcopy(self.before['optimizer']))
        return model, optimizer

    def branch(self, **changes):
        return dict(dict(name='test', entropy_scale=1, reset_adam=False, lr_scale=1, skip=False), **changes)

    def test_zero_gradient_keeps_momentum_but_skip_preserves_entire_state(self):
        model, optimizer = self.fresh()
        gradients = [torch.ones_like(model.weight)]
        branch_step(model, optimizer, gradients, self.branch(entropy_scale=0), 1)
        # Independently compute the second Adam step with zero current gradient.
        mean = .9*(.1*2)/(1-.9**2)
        variance = .999*(.001*4)/(1-.999**2)
        expected = float(self.before['policy']['weight']) - .03*mean/(variance**.5+1e-8)
        self.assertAlmostEqual(float(model.weight.detach()), expected, places=14)
        self.assertEqual(float(optimizer.state[model.weight]['step']), 2)
        self.assertTrue(torch.equal(gradients[0], torch.ones_like(model.weight)))
        skipped, saved = self.fresh()
        branch_step(skipped, saved, gradients, self.branch(skip=True), 1)
        check_baseline(skipped, saved, self.before)
        # One branch must not contaminate subsequent branches or their source.
        check_baseline(self.model, self.optimizer, self.before)

    def test_reset_zero_changes_optimizer_but_not_policy(self):
        model, optimizer = self.fresh()
        branch_step(model, optimizer, [torch.ones_like(model.weight)], self.branch(reset_adam=True, entropy_scale=0), 1)
        self.assertTrue(identical(model.state_dict(), self.before['policy']))
        self.assertFalse(identical(optimizer.state_dict(), self.before['optimizer']))
        state = optimizer.state[model.weight]
        self.assertEqual(float(state['step']), 1)
        self.assertEqual(float(state['exp_avg']), 0)
        self.assertEqual(float(state['exp_avg_sq']), 0)

    def test_learning_rate_scales_parameter_step_with_fixed_gradient_and_moments(self):
        deltas = []
        for scale in (1, .1):
            model, optimizer = self.fresh()
            branch_step(model, optimizer, [torch.full_like(model.weight, -.5)], self.branch(lr_scale=scale), 1)
            deltas.append(float(model.weight.detach()-self.before['policy']['weight']))
        self.assertNotEqual(deltas[0], 0)
        self.assertAlmostEqual(deltas[1]/deltas[0], .1, places=12)

    def test_baseline_rejects_policy_or_optimizer_corruption(self):
        model, optimizer = self.fresh()
        check_baseline(model, optimizer, self.before)
        optimizer.state[model.weight]['exp_avg'].add_(1)
        with self.assertRaisesRegex(ValueError, 'Adam mismatch'):
            check_baseline(model, optimizer, self.before)
        model, optimizer = self.fresh()
        with torch.no_grad():
            model.weight.add_(1)
        with self.assertRaisesRegex(ValueError, 'policy mismatch'):
            check_baseline(model, optimizer, self.before)


if __name__ == '__main__':
    unittest.main()
