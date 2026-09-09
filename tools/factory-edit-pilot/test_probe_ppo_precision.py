"""Check that timed diagnostic updates preserve the frozen PPO update math."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

import torch

from full_factory_on_policy import parameter_hash
from ppo_area_update import collect_steps, update
from probe_ppo_precision import minibatch
from random_factory_audit import optimizer_hash
import test_ppo_area as fixtures


class PrecisionProbeTests(unittest.TestCase):
    def test_probe_matches_frozen_update_with_nonempty_adam_and_scaled_gradients(self):
        fixture = fixtures.PpoTests()
        fixture.setUp()
        config = dict(fixture.config, epochs=1, minibatch_size=4)
        actor, critic = fixtures.ToyActor(), fixtures.ToyCritic()
        aopt, copt = (torch.optim.Adam(model.parameters(), lr=.0003) for model in (actor, critic))
        with tempfile.TemporaryDirectory() as directory:
            results = fixture.experience(Path(directory), actor, critic)
            update(actor, critic, aopt, copt, torch.Generator().manual_seed(1), results, config)
            results = fixture.experience(Path(directory), actor, critic)
            steps, advantages, targets, _ = collect_steps(actor, critic, results, config)
            indices = torch.randperm(len(steps), generator=torch.Generator().manual_seed(2)).tolist()
            variants = []
            for scaled in (False, True):
                a, c = deepcopy(actor), deepcopy(critic)
                ao, co = (torch.optim.Adam(model.parameters(), lr=.0003) for model in (a, c))
                ao.load_state_dict(deepcopy(aopt.state_dict()))
                co.load_state_dict(deepcopy(copt.state_dict()))
                scaler = torch.amp.GradScaler('cpu') if scaled else None
                details = minibatch((a, c, ao, co), [steps[index] for index in indices],
                                    advantages[indices], targets[indices], config, torch.device('cpu'), None, scaler, True)
                self.assertEqual(details['actual_optimizer_steps'], [1, 1])
                self.assertTrue(torch.isfinite(details['_vectors']['gradients']['actor']).all())
                variants.append((a, c, ao, co))
            update(actor, critic, aopt, copt, torch.Generator().manual_seed(2), results, config)
            expected = parameter_hash(actor), parameter_hash(critic), optimizer_hash(aopt), optimizer_hash(copt)
            for a, c, ao, co in variants:
                self.assertEqual(expected, (parameter_hash(a), parameter_hash(c), optimizer_hash(ao), optimizer_hash(co)))


if __name__ == '__main__':
    unittest.main()
