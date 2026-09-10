"""Spatial treatments preserve full-domain forward, gradients and initialization."""
from copy import deepcopy
import os
import unittest

import numpy as np
import torch

from full_factory_on_policy import parameter_hash
from ppo_area_model import actor_distribution, models
from ppo_spatial_model import VARIANTS, specification, validate_model
from ppo_training_batch import prepare, distribution, actor_logits
from test_ppo_training_batch import observations


class SpatialTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        self.device = os.environ.get('AIC_SPATIAL_TEST_DEVICE', 'cpu')
        self.config = dict(temperature=.8, balanced=True)
        self.steps = observations()
        for i, step in enumerate(self.steps):
            obs = step['actor_observation']
            obs['current'] = deepcopy(step['critic_observation']['current'])
            obs['operands'][:, 0] = 1
            if i == 1:
                obs['history'] = np.ones((3, 53), np.float32) * .1
            if i == 2:
                obs['current']['messages'] = False

    def test_critic_initialization_is_independent_and_model_spec_strict(self):
        pairs = [models(dict(model=specification(name)), 17) for name in VARIANTS]
        self.assertEqual(len({parameter_hash(c) for _, c in pairs}), 1)
        self.assertEqual(sum(p.numel() for p in pairs[1][0].parameters()),
                         sum(p.numel() for p in pairs[2][0].parameters()))
        invalid = specification('cnn4')
        invalid['cnn_layers'] = 8
        with self.assertRaisesRegex(ValueError, 'model-spec'):
            validate_model(invalid)
        with self.assertRaises(RuntimeError):
            pairs[0][0].load_state_dict(pairs[-1][0].state_dict())

    def test_full_actor_forward_gradients_mask_isolation_and_chunks(self):
        for name in VARIANTS:
            with self.subTest(name=name):
                actor = models(dict(model=specification(name)), 17)[0].to(self.device)
                reference = deepcopy(actor)
                batch = prepare(self.steps, self.config, 32, self.device)
                logp, entropy, probabilities = distribution(actor, batch, self.config)
                expected = [actor_distribution(reference, s, self.config) for s in self.steps]
                target = torch.stack([d.log_prob(torch.tensor(0, device=self.device)) for d in expected])
                torch.testing.assert_close(logp, target, rtol=5e-5, atol=5e-6)
                for i, d in enumerate(expected):
                    torch.testing.assert_close(probabilities[i, :len(d.probs)], d.probs, rtol=5e-5, atol=5e-6)
                torch.testing.assert_close(probabilities.sum(1), torch.ones(3, device=self.device))
                self.assertEqual(float(probabilities.masked_select(~batch['allowed']).sum().detach()), 0.)
                weights = torch.tensor([.7, -.4, 1.3], device=self.device)
                (logp * weights).sum().backward()
                (target * weights).sum().backward()
                for (key, p), (_, q) in zip(actor.named_parameters(), reference.named_parameters()):
                    self.assertEqual(p.grad is None, q.grad is None, key)
                    if p.grad is not None:
                        self.assertTrue(torch.isfinite(p.grad).all(), key)
                        torch.testing.assert_close(p.grad, q.grad, rtol=2e-4, atol=1e-5, msg=key)
                with torch.no_grad():
                    split = torch.cat([distribution(actor, prepare([s], self.config, 32, self.device), self.config)[0]
                                       for s in self.steps])
                    torch.testing.assert_close(logp, split, rtol=5e-5, atol=5e-6)
                    changed = deepcopy(self.steps)
                    changed[1]['actor_observation']['current']['raster'] *= 100
                    other = distribution(actor, prepare(changed, self.config, 32, self.device), self.config)[0]
                    torch.testing.assert_close(logp[[0, 2]], other[[0, 2]], rtol=0, atol=0)
                self.assertTrue(torch.isfinite(entropy).all())

    def test_node_permutation_with_remapped_references(self):
        step = self.steps[1]
        altered = deepcopy(step)
        obs = altered['actor_observation']
        world = obs['current']
        order = np.arange(len(world['nodes']))[::-1]
        inverse = np.argsort(order)
        world['nodes'] = world['nodes'][order].copy()
        world['node_xy'] = world['node_xy'][order].copy()
        world['edges'] = inverse[world['edges']]
        obs['operands'] = np.where(obs['operands'] > 0, inverse[np.maximum(obs['operands'] - 1, 0)] + 1, 0)
        obs['member_nodes'] = inverse[obs['member_nodes']]
        for name in VARIANTS:
            actor = models(dict(model=specification(name)), 17)[0].to(self.device)
            with torch.no_grad():
                before = actor_logits(actor, prepare([step], self.config, 32, self.device))
                after = actor_logits(actor, prepare([altered], self.config, 32, self.device))
            torch.testing.assert_close(before, after, rtol=5e-5, atol=5e-6)


if __name__ == '__main__':
    unittest.main()
