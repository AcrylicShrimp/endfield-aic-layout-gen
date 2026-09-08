"""Diagnostic distances and noninterference at the calibration boundary."""

from copy import deepcopy
import json
import math
from pathlib import Path
import unittest

import numpy as np
import torch

from native_training_comparison import equal
from policy import Policy, update
from reference.acceptance import Catalog
from reference.environment import Environment
from update_stability import categorical_distance, distance, measured_update

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class StabilityTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.manual_seed(43)
        self.config = json.loads((DIRECTORY/'run.json').read_text())
        self.config.update(device='cpu', horizon=4, epochs=2, minibatch=4)
        self.env = Environment(Catalog(ROOT, DIRECTORY/'case.json'), self.config)
        self.observation = self.env.observe(self.env.candidates())

    def test_exact_categorical_distance_and_padding_exclusion(self):
        a, b = torch.tensor([0.25, 0.75]).log(), torch.tensor([0.5, 0.5]).log()
        kl, tv = categorical_distance(a, b)
        self.assertAlmostEqual(kl, 0.25*math.log(0.5)+0.75*math.log(1.5), places=7)
        self.assertAlmostEqual(tv, 0.25, places=7)
        self.assertEqual(categorical_distance(a, a), (0.0, 0.0))

        class Fixed(torch.nn.Module):
            def __init__(self, slope):
                super().__init__()
                self.slope = torch.nn.Parameter(torch.tensor(slope))
            def forward(self, data):
                logits = torch.arange(data['mask'].shape[1])*self.slope
                logits = logits.expand(len(data['mask']), -1).masked_fill(~data['mask'], -torch.inf)
                return logits, torch.zeros(len(data['mask']))

        observations = []
        for count in (2, 3):
            observation = deepcopy(self.observation)
            for key in ('candidates', 'operands', 'operand_entities', 'operand_mask', 'points'):
                observation[key] = observation[key][:count]
            observations.append(observation)
        rollout = {'observations': observations, 'actions': [0, 1]}
        reference, candidate = Fixed(0.0), Fixed(0.3)
        alone = distance(reference, candidate, rollout, 1)
        together = distance(reference, candidate, rollout, 2)
        for key in alone:
            self.assertAlmostEqual(alone[key], together[key], places=7)
        self.assertGreater(together['exact_kl_mean'], 0)

    def test_measurement_preserves_update_model_optimizer_and_rng(self):
        model = Policy(self.observation, normalization='batch')
        rollout = {key: [] for key in ('observations', 'actions', 'log_probs', 'values', 'rewards', 'dones')}
        generator = torch.Generator().manual_seed(17)
        for _ in range(8):
            domain = self.env.candidates()
            observation = self.env.observe(domain)
            action, log_prob, value = model.act(observation, generator)
            reward, done, _ = self.env.step(domain.to_dicts()[action])
            for key, item in zip(rollout, (observation, action, log_prob, value, reward, done)):
                rollout[key].append(item)
            if done:
                self.env.reset()
        for rate in (0.0, 0.0003):
            with self.subTest(rate=rate):
                a, b = deepcopy(model), deepcopy(model)
                oa, ob = [torch.optim.Adam(actor.parameters(), lr=rate) for actor in (a, b)]
                ra, rb = np.random.default_rng(19), np.random.default_rng(19)
                before_rng = torch.get_rng_state().clone()
                ordinary = update(a, oa, rollout, self.config, ra)
                observed = measured_update(b, ob, rollout, self.config, rb)
                self.assertTrue(equal(a.state_dict(), b.state_dict()))
                self.assertTrue(equal(oa.state_dict(), ob.state_dict()))
                self.assertTrue(equal(ra.bit_generator.state, rb.bit_generator.state))
                self.assertTrue(torch.equal(before_rng, torch.get_rng_state()))
                self.assertNotIn('calibrate_normalization', b.__dict__)
                self.assertEqual(ordinary['approx_kl'], observed['approx_kl'])
                metrics = observed['deployment_distances']
                if rate == 0:
                    self.assertEqual(metrics['anchor_to_post_optimizer']['exact_kl_mean'], 0)
                    self.assertGreater(metrics['post_optimizer_to_post_calibration']['value_rms'], 0)
                for row in metrics.values():
                    self.assertTrue(all(np.isfinite(value) and value >= 0 for value in row.values()))


if __name__ == '__main__':
    unittest.main()
