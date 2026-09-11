"""Architecture defaults, packed graph semantics and non-default-depth continuation."""
from copy import deepcopy
import os
import unittest

import torch

from blueprint_capacity import child_config
from blueprint_edit_input import BlueprintActorCritic, prepare
from blueprint_ppo import readiness
from blueprint_ppo_batch import pack
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_store import ROOT, model_checkpoint, read, restore_learners
from blueprint_ppo_update import digest, learners, update


class CapacityConfigTests(unittest.TestCase):
    def test_explicit_baseline_is_identical_and_invalid_architecture_is_rejected(self):
        config = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-64-12h.json')['ppo']
        default, _, _ = learners(17, config)
        explicit, _, _ = learners(17, dict(config, message_rounds=2))
        self.assertEqual(digest(default.state_dict()), digest(explicit.state_dict()))
        for width, rounds in [(0, 2), (40, 0), (40, 1.5), (True, 2), (40, True)]:
            with self.assertRaisesRegex(ValueError, 'positive integers'):
                BlueprintActorCritic(width, rounds)

    def test_child_configuration_preserves_training_and_reward_contract(self):
        base = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-64-12h.json')
        spec = read(ROOT / 'tools/factory-edit-pilot/blueprint-capacity-screen.json')
        original = deepcopy(base)
        for model in spec['models']:
            child = child_config(base, spec, model, 29, 'matrix')
            self.assertEqual(child['evaluation'], base['evaluation'])
            self.assertEqual(child['success'], base['success'])
            self.assertEqual(child['starts'], base['starts'])
            self.assertEqual(child['training'], dict(base['training'], updates=512))
            self.assertEqual(child['ppo'], dict(base['ppo'], width=model['width'], message_rounds=model['message_rounds']))
        self.assertEqual(base, original)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires explicitly selected native library')
class CapacityNativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cls.config = readiness(read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo.json'))
        cls.config['ppo'].update(width=80, message_rounds=4)
        cls.runtime = Runtime(cls.config, os.environ['AIC_NATIVE_LIBRARY'])

    def test_larger_deeper_packed_outputs_and_every_round_gradient(self):
        inputs = [prepare(Stream(self.runtime, i).observe()) for i in range(4)]
        model, _, _ = learners(17, self.config['ppo'])
        combined = deepcopy(model)
        individual = [model(row) for row in inputs]
        logits, values = combined.forward_packed(pack(inputs))
        torch.testing.assert_close(logits, torch.cat([p[0] for p in individual]), rtol=1e-5, atol=1e-6)
        torch.testing.assert_close(values, torch.stack([p[1] for p in individual]), rtol=1e-5, atol=1e-6)
        sum(a.square().sum() + v.square() for a, v in individual).backward()
        (logits.square().sum() + values.square().sum()).backward()
        for (name, first), second in zip(model.named_parameters(), combined.parameters(), strict=True):
            torch.testing.assert_close(first.grad, second.grad, rtol=1e-4, atol=2e-5)
            if name.startswith(('messages.', 'updates.')):
                self.assertGreater(float(first.grad.norm()), 0., name)

    def test_deeper_nonempty_adam_checkpoint_replays_exactly(self):
        streams = [Stream(self.runtime, i) for i in range(4)]
        model, actor, critic = learners(17, self.config['ppo'])
        sampler, shuffle = torch.Generator().manual_seed(1000017), torch.Generator().manual_seed(2000017)
        first = collect(model, streams, sampler, self.config, lambda: None)
        update(model, actor, critic, shuffle, first['samples'], first['frontiers'], self.config['ppo'])
        saved = deepcopy(model_checkpoint(model, actor, critic, sampler, shuffle, 17, 1, streams, 'capacity'))
        self.assertTrue(saved['actor_optimizer']['state'])
        second = collect(model, streams, sampler, self.config, lambda: None)
        actual = update(model, actor, critic, shuffle, second['samples'], second['frontiers'], self.config['ppo'])
        m, a, c, _, rng = restore_learners(saved, self.config['ppo'], 'capacity')
        replayed = update(m, a, c, rng, second['samples'], second['frontiers'], self.config['ppo'])
        self.assertEqual(actual, replayed)


if __name__ == '__main__':
    unittest.main()
