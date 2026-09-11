"""Check BC gradient ownership, baseline parity, KL freeze and durable replay."""
from copy import deepcopy
import os
import unittest

import torch

from blueprint_edit_input import prepare
from blueprint_ppo import readiness
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_store import ROOT, model_checkpoint, read, restore_learners
from blueprint_ppo_update import digest, learners, update
from blueprint_bc_update import imitation_loss, update_bc


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires explicitly selected native library')
class BlueprintBcTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cls.config = readiness(read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo-bc.json'))
        cls.runtime = Runtime(cls.config, os.environ['AIC_NATIVE_LIBRARY'])

    def setup_chunk(self):
        model, actor, critic = learners(17, self.config['ppo'])
        sampler = torch.Generator().manual_seed(1000017)
        shuffle = torch.Generator().manual_seed(2000017)
        streams = [Stream(self.runtime, i) for i in range(4)]
        payload = collect(model, streams, sampler, self.config, lambda: None)
        observation = payload['samples'][0]['observation']
        prepared = prepare(observation)
        # The unit test needs real candidate graphs; dataset validation tests
        # independently cover the native execution of the three-stage sequence.
        rows = []
        for kind in ('pose', 'repair_all', 'commit'):
            index = next(i for i, a in enumerate(prepared['actions']) if a['kind'] == kind)
            rows.append(dict(input=prepared, index=index, action=prepared['actions'][index]))
        demos = [dict(id='unit-test', split='train', after=dict(rows=rows))]
        return model, actor, critic, sampler, shuffle, streams, payload, demos

    def test_bc_updates_actor_encoder_but_has_no_direct_critic_gradient(self):
        model, _, _, _, _, _, _, demos = self.setup_chunk()
        imitation_loss(model, demos).backward()
        self.assertTrue(all(p.grad is None for p in model.critic.parameters()))
        gradients = {name: float(p.grad.abs().sum()) for name, p in model.named_parameters() if p.grad is not None}
        self.assertGreater(sum(value for name, value in gradients.items() if name.startswith('actor.')), 0)
        self.assertGreater(sum(value for name, value in gradients.items() if not name.startswith('actor.')), 0)

    def test_zero_coefficient_matches_original_ppo_exactly(self):
        model, actor, critic, sampler, shuffle, streams, payload, demos = self.setup_chunk()
        saved = deepcopy(model_checkpoint(model, actor, critic, sampler, shuffle, 17, 0, streams, 'unit'))
        expected = update(model, actor, critic, shuffle, payload['samples'], payload['frontiers'], self.config['ppo'])
        m, a, c, _, rng = restore_learners(saved, self.config['ppo'], 'unit')
        actual = update_bc(m, a, c, rng, payload['samples'], payload['frontiers'], self.config['ppo'],
                           dict(coefficient=0, demonstrations_per_minibatch=8), demos, 17, 1)
        self.assertEqual(actual.pop('bc')['label_count'], 0)
        self.assertEqual(actual, expected)

    def test_nonempty_adam_replay_and_bc_stops_with_actor(self):
        model, actor, critic, sampler, shuffle, streams, payload, demos = self.setup_chunk()
        bc = dict(coefficient=.05, demonstrations_per_minibatch=8)
        first = update_bc(model, actor, critic, shuffle, payload['samples'], payload['frontiers'], self.config['ppo'], bc, demos, 17, 1)
        self.assertEqual(first['actor_epochs'], 1)
        self.assertEqual(first['critic_epochs'], 4)
        self.assertEqual(len(first['bc']['selected_ids']), 1)
        self.assertTrue(first['critic_tail_preserves_actor'])
        saved = deepcopy(model_checkpoint(model, actor, critic, sampler, shuffle, 17, 1, streams, 'unit'))
        second = collect(model, streams, sampler, self.config, lambda: None)
        expected = update_bc(model, actor, critic, shuffle, second['samples'], second['frontiers'], self.config['ppo'], bc, demos, 17, 2)
        m, a, c, _, rng = restore_learners(saved, self.config['ppo'], 'unit')
        actual = update_bc(m, a, c, rng, second['samples'], second['frontiers'], self.config['ppo'], bc, demos, 17, 2)
        self.assertEqual(actual, expected)
        bad = deepcopy(demos)
        bad[0]['split'] = 'holdout'
        with self.assertRaisesRegex(ValueError, 'no holdout'):
            update_bc(m, a, c, rng, second['samples'], second['frontiers'], self.config['ppo'], bc, bad, 17, 2)


if __name__ == '__main__':
    unittest.main()
