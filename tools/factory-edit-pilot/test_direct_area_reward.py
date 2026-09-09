"""Mathematical, adversarial payload and matched-evaluation checks for direct rewards."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

import torch

from direct_area_reward import PrefixCredit, SCHEMA, payload_hash, reward_sequence, update
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from plot_direct_area_study import paired


class DirectRewardTests(unittest.TestCase):
    def test_rewards_telescope_and_restore_does_not_repay_old_record(self):
        # Current can worsen/restore during the three unchanged incumbent entries.
        rewards = reward_sequence(1000, [1000, 900, 900, 900, 880])
        self.assertEqual(rewards, [.1, 0, 0, .02])
        self.assertAlmostEqual(sum(rewards), .12)
        self.assertEqual(reward_sequence(1000, [900, 880])[0], 2 * reward_sequence(1000, [900, 890])[0])
        for initial, values in ((0, [0, 0]), (100, [90, 91]), (100, [101, 90]), (100, [100., 90])):
            with self.assertRaises(ValueError):
                reward_sequence(initial, values)

    def test_prefix_gradient_matches_direct_reward_to_go_objective(self):
        torch.manual_seed(17)
        model = torch.nn.Linear(3, 4).double()
        inputs = torch.randn(5, 3, dtype=torch.float64)
        actions, rewards = [0, 1, 3, 2, 0], [0, .02, 0, 0, .17]
        credit = PrefixCredit(model, 5)
        logps = []
        for x, action, reward in zip(inputs, actions, rewards):
            logp = model(x).log_softmax(0)[action]
            logps.append(logp)
            credit.select(torch.autograd.grad(logp, tuple(model.parameters()), retain_graph=True))
            credit.reward(reward)
        objective = sum(logp * sum(rewards[t:]) for t, logp in enumerate(logps)) / 5
        expected = torch.autograd.grad(objective, tuple(model.parameters()))
        for actual, reference in zip(credit.payload()['reward_gradients'], expected):
            torch.testing.assert_close(actual, reference, rtol=1e-12, atol=1e-12)

    def test_future_actions_cannot_receive_earlier_rewards(self):
        model = torch.nn.Linear(1, 1, bias=False).double()
        credit = PrefixCredit(model, 3)
        for derivative, reward in ((2., .3), (7., 0), (19., 0)):
            credit.select([torch.tensor([[derivative]], dtype=torch.float64)])
            credit.reward(reward)
        self.assertAlmostEqual(credit.payload()['reward_gradients'][0].item(), .2)

    def test_incomplete_or_unpaired_credit_is_rejected(self):
        model = torch.nn.Linear(1, 1)
        credit = PrefixCredit(model, 2)
        with self.assertRaises(ValueError):
            credit.reward(.1)
        credit.select([torch.zeros_like(p) for p in model.parameters()])
        with self.assertRaises(ValueError):
            credit.payload()
        with self.assertRaises(ValueError):
            credit.select([torch.zeros_like(p) for p in model.parameters()])

    def test_bad_second_payload_cannot_mutate_optimizer_or_model(self):
        model = torch.nn.Linear(1, 1)
        optimizer = torch.optim.Adam(model.parameters(), lr=.0003)
        before = parameter_hash(model)
        with tempfile.TemporaryDirectory() as temporary:
            results = []
            for index in range(2):
                credit = PrefixCredit(model, 1)
                credit.select([torch.ones_like(p) for p in model.parameters()])
                credit.reward(.1)
                payload = credit.payload()
                if index:
                    payload['reward_gradients'][0] = torch.zeros(7)
                path = Path(temporary) / f'{index}.pt'
                torch.save(dict(policy_sha256=before, input_sha256=str(index), gradients=payload), path)
                results.append(dict(status='complete', policy_sha256=before, input_sha256=str(index),
                    gradients=str(path), gradients_sha256=sha(path), gradient_values_sha256=payload_hash(payload),
                    row=dict(selected=1, reward_sum=.1, initial_metrics=dict(score=[100, 1, 0]), best_metrics=dict(score=[90, 1, 0]))))
            with self.assertRaises(ValueError):
                update(model, optimizer, results, dict(entropy_coefficient=.001, gradient_clip=.5))
        self.assertEqual(parameter_hash(model), before)
        self.assertEqual(len(optimizer.state), 0)
        self.assertTrue(all(p.grad is None for p in model.parameters()))

    def test_paired_curve_preserves_magnitude_and_rejects_changed_bank(self):
        result = dict(identity=dict(index=0), status='complete', generation=dict(initial_sha256='a', request_sha256='b', draw=dict(product='example')),
                      row=dict(initial_metrics=dict(score=[100, 10, 3]), best_metrics=dict(score=[90, 8, 2])))
        baseline = dict(results=[result])
        evaluation = deepcopy(baseline)
        evaluation['results'][0]['row']['best_metrics']['score'] = [88, 9, 3]
        summary = paired(evaluation, baseline)
        self.assertEqual(summary['products']['all']['delta_pp'], 2)
        self.assertEqual(summary['products']['all']['wins'], 1)
        evaluation['results'][0]['generation']['initial_sha256'] = 'changed'
        with self.assertRaises(ValueError):
            paired(evaluation, baseline)
        with self.assertRaises(ValueError):
            paired(dict(results=[result, result]), baseline)


if __name__ == '__main__':
    unittest.main()
