"""Batched PPO must preserve per-factory normalization, graphs and gradients."""
from copy import deepcopy
import unittest

import numpy as np
import torch

from full_factory_input_contracts import SCHEMA
from ppo_area_model import actor_distribution, models
from ppo_training_batch import prepare, compile_steps, distribution, critic_values


def observations():
    rng = np.random.default_rng(41)
    def values(*shape):
        return rng.normal(0, .2, shape).astype(np.float32)
    def world(height, width, nodes, connected):
        edges = np.array([[i, (i + 1) % nodes] for i in range(nodes)] if connected else [], np.int64).reshape(-1, 2)
        return dict(raster=values(28, height, width), nodes=values(nodes, 32),
                    node_xy=np.array([[i % width, i % height] for i in range(nodes)], np.int64),
                    edges=edges, edge_values=values(len(edges), 16), messages=True)
    steps = []
    for i, count in enumerate([3, 5, 2]):
        current = world(3 + i % 2, 4, i + 1, i != 0)
        best = current if i != 1 else world(2, 5, 2, False)
        actions = values(count, 32)
        members = np.array([0, 0, count - 1] if i != 1 else [], np.int64)
        actor = dict(schema=SCHEMA, current=None, best=None, best_is_current=False,
                     actions=actions, operands=np.zeros((count, 2), np.int64),
                     action_xy=np.zeros((count, 2), np.int64), member_nodes=np.zeros(len(members), np.int64),
                     member_actions=members, member_xy=np.zeros((len(members), 2), np.int64),
                     member_pose=values(len(members), 10), context=np.concatenate((values(4), np.zeros(5, np.float32))),
                     history=np.zeros((0, 53), np.float32))
        critic = dict(current=current, best=best, best_is_current=best is current,
                      actions=actions.copy(), context=values(9))
        steps.append(dict(actor_observation=actor, critic_observation=critic,
                          allowed=[True] * (count - 1) + [i == 2], kinds=['pose'] * (count - 1) + ['contract'],
                          index=0, old_log_probability=-1., old_value=.1))
    return steps


class BatchedTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        self.config = dict(temperature=.8, balanced=True)
        self.actor, self.critic = models(dict(policy=dict(depth=2, width=8)), 17)
        # A nonzero output head exercises every world-encoder gradient.
        torch.nn.init.normal_(self.critic.head[-1].weight, std=.1)
        self.steps = observations()

    def test_ragged_forward_and_gradients_match_individual_reference(self):
        actor, critic = deepcopy(self.actor), deepcopy(self.critic)
        batch = prepare(self.steps, self.config, 8, 'cpu')
        logp, entropy, probabilities = distribution(actor, batch, self.config)
        values = critic_values(critic, batch)
        expected_dist = [actor_distribution(self.actor, step, self.config) for step in self.steps]
        expected_logp = torch.stack([d.log_prob(torch.tensor(s['index'])) for d, s in zip(expected_dist, self.steps)])
        expected_entropy = torch.stack([d.entropy() for d in expected_dist])
        expected_values = torch.stack([self.critic(s['critic_observation'], s['allowed']) for s in self.steps])
        for actual, expected in [(logp, expected_logp), (entropy, expected_entropy), (values, expected_values)]:
            torch.testing.assert_close(actual, expected, rtol=2e-5, atol=2e-6)
        for i, expected in enumerate(expected_dist):
            torch.testing.assert_close(probabilities[i, :len(expected.probs)], expected.probs, rtol=2e-5, atol=2e-6)
            self.assertEqual(float(probabilities[i, len(expected.probs):].detach().sum()), 0.)
        weights = torch.tensor([.7, -.4, 1.3])
        ((logp * weights).sum() - entropy.mean() * .001 + values.square().sum()).backward()
        ((expected_logp * weights).sum() - expected_entropy.mean() * .001 + expected_values.square().sum()).backward()
        for actual_model, expected_model in [(actor, self.actor), (critic, self.critic)]:
            for (name, actual), (_, expected) in zip(actual_model.named_parameters(), expected_model.named_parameters()):
                self.assertEqual(actual.grad is None, expected.grad is None, name)
                if actual.grad is not None:
                    torch.testing.assert_close(actual.grad, expected.grad, rtol=3e-5, atol=3e-6, msg=name)

    def test_other_factory_does_not_change_prediction_or_graph_normalization(self):
        before = critic_values(self.critic, prepare(self.steps, self.config, 8, 'cpu')).detach()
        changed = deepcopy(self.steps)
        changed[1]['critic_observation']['current']['raster'] *= 10
        changed[1]['critic_observation']['current']['nodes'] *= 10
        after = critic_values(self.critic, prepare(changed, self.config, 8, 'cpu')).detach()
        torch.testing.assert_close(before[[0, 2]], after[[0, 2]], rtol=0, atol=0)
        self.assertNotEqual(float(before[1]), float(after[1]))

    def test_scaled_masked_entropy_has_finite_gradients(self):
        batch = prepare(self.steps, self.config, 8, 'cpu')
        logp, entropy, _ = distribution(self.actor, batch, self.config)
        values = critic_values(self.critic, batch)
        scaler = torch.amp.GradScaler('cpu')
        scaler.scale(-logp.mean() - .001 * entropy.mean() + values.square().mean()).backward()
        for model in (self.actor, self.critic):
            self.assertTrue(all(torch.isfinite(p.grad).all() for p in model.parameters() if p.grad is not None))

    def test_static_inputs_reuse_across_shuffles_without_mutating_observations(self):
        compiled = compile_steps(self.steps, self.config)
        self.assertTrue(all('_static_inputs' not in step for step in self.steps))
        for indices in ([2, 0, 1], [1], [0, 2]):
            original = prepare([self.steps[i] for i in indices], self.config, 8, 'cpu')
            cached = prepare([compiled[i] for i in indices], self.config, 8, 'cpu')
            for key in ('allowed', 'adjustments', 'summaries', 'old_logp', 'old_values'):
                torch.testing.assert_close(cached[key], original[key], rtol=0, atol=0)
            torch.testing.assert_close(distribution(self.actor, cached, self.config)[0],
                                       distribution(self.actor, original, self.config)[0], rtol=0, atol=0)
            torch.testing.assert_close(critic_values(self.critic, cached), critic_values(self.critic, original), rtol=0, atol=0)
        with self.assertRaisesRegex(ValueError, 'balancing configuration'):
            prepare(compiled, dict(self.config, balanced=False), 8, 'cpu')



if __name__ == '__main__':
    unittest.main()
