"""Isolate advantage scaling without changing value targets, inputs or replay state."""
from copy import deepcopy
import os
from pathlib import Path
import unittest

import torch

from blueprint_advantage import child_config
from blueprint_ppo import readiness
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_store import ROOT, model_checkpoint, read, restore_learners
from blueprint_ppo_update import actor_advantages, credit, digest, learners, update
from blueprint_success_collect import collect_scripted
from blueprint_success_buffer import Evidence, SuccessBuffer
from blueprint_success_update import update_success


class AdvantageTests(unittest.TestCase):
    def test_centering_changes_actor_scale_only_and_handles_zero_variance(self):
        rows = [dict(stream=0, reward=0., old_value=v, terminal=i == 2) for i, v in enumerate([.001, .004, .002])]
        frontier = [dict(stream=0, terminal=True, bootstrap=0.)]
        config = dict(gamma=1., gae_lambda=.95)
        raw, standard, targets = credit(rows, frontier, config)
        other_raw, centered, other_targets = credit(rows, frontier, dict(config, advantage_normalization='center'))
        torch.testing.assert_close(raw, other_raw, rtol=0, atol=0)
        torch.testing.assert_close(targets, other_targets, rtol=0, atol=0)
        torch.testing.assert_close(centered, raw - raw.mean(), rtol=0, atol=0)
        self.assertAlmostEqual(float(standard.std(unbiased=False)), 1., places=6)
        self.assertLess(float(centered.std(unbiased=False)), .01)
        for mode in ('standardize', 'center'):
            self.assertEqual(actor_advantages(torch.ones(8), mode).tolist(), [0.] * 8)
        with self.assertRaisesRegex(ValueError, 'registered advantage normalization'):
            actor_advantages(raw, 'off')

    def test_arms_share_actual_generation_contract_and_change_only_the_factor(self):
        base = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-64-12h.json')
        spec = read(ROOT / 'tools/factory-edit-pilot/blueprint-advantage-screen.json')
        configs = [child_config(base, spec, variant, 17, 'test-matrix', Path('/paired/inputs'))
                   for variant in spec['variants']]
        for config in configs:
            self.assertEqual(config['ppo']['width'], 40)
            self.assertEqual(config['training']['episode_actions'], 64)
            self.assertEqual(config['training_input']['directory'], '/paired/inputs')
            for key in ('purpose', 'experiment'):
                config.pop(key)
            config['ppo'].pop('advantage_normalization')
        self.assertEqual(configs[0], configs[1])


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires explicitly selected native library')
class NativeAdvantageTests(unittest.TestCase):
    def test_centered_credit_with_nonempty_self_imitation_replays_both_optimizers(self):
        class Memory(Evidence):
            def __init__(self):
                self.chunks = {}
            def chunk(self, ordinal):
                return self.chunks[ordinal]
        torch.set_num_threads(1)
        config = readiness(read(ROOT / 'tools/factory-edit-pilot/blueprint-success-replay.json'))
        config['purpose'] = 'success-mechanical-readiness'
        config['starts'] = [config['starts'][0]]
        config['training'].update(streams=1, chunk_actions=2, episode_actions=4)
        config['ppo']['advantage_normalization'] = 'center'
        config['success']['coefficient'] = .05
        streams = [Stream(Runtime(config, os.environ['AIC_NATIVE_LIBRARY']), 0)]
        model, actor, critic = learners(17, config['ppo'])
        sampler, shuffle = torch.Generator().manual_seed(1000017), torch.Generator().manual_seed(2000017)
        evidence, buffer = Memory(), SuccessBuffer()
        for ordinal in (1, 2):
            chunk = collect_scripted(model, streams, sampler, config, lambda: None)
            evidence.chunks[ordinal] = chunk
            update_success(model, actor, critic, shuffle, chunk['samples'], chunk['frontiers'],
                           config['ppo'], config['success'], [], 17, ordinal)
            buffer.ingest(chunk, ordinal, evidence, config['success'], config['ppo']['gamma'])
        sequences = buffer.sequences(evidence, 3)
        self.assertTrue(sequences)
        third = collect_scripted(model, streams, sampler, config, lambda: None)
        saved = deepcopy(model_checkpoint(model, actor, critic, sampler, shuffle, 17, 2, streams, 'test'))
        expected = update_success(model, actor, critic, shuffle, third['samples'], third['frontiers'],
                                  config['ppo'], config['success'], sequences, 17, 3)
        self.assertEqual(expected['advantage_scale'], 1.)
        self.assertEqual(expected['advantages_sha256'], digest(credit(third['samples'], third['frontiers'], config['ppo'])[1]))
        self.assertTrue(expected['success']['selected_ids'])
        m, a, c, _, rng = restore_learners(saved, config['ppo'], 'test')
        actual = update_success(m, a, c, rng, third['samples'], third['frontiers'],
                                config['ppo'], config['success'], sequences, 17, 3)
        self.assertEqual(expected, actual)

    def test_centered_update_restores_nonempty_adam_and_replays_exactly(self):
        torch.set_num_threads(1)
        config = readiness(read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo.json'))
        config['ppo']['advantage_normalization'] = 'center'
        runtime = Runtime(config, os.environ['AIC_NATIVE_LIBRARY'])
        streams = [Stream(runtime, i) for i in range(config['training']['streams'])]
        model, actor, critic = learners(17, config['ppo'])
        sampler, shuffle = torch.Generator().manual_seed(1000017), torch.Generator().manual_seed(2000017)
        first = collect(model, streams, sampler, config, lambda: None)
        first_result = update(model, actor, critic, shuffle, first['samples'], first['frontiers'], config['ppo'])
        self.assertEqual(first_result['advantage_scale'], 1.)
        saved = deepcopy(model_checkpoint(model, actor, critic, sampler, shuffle, 17, 1, streams, 'test'))
        second = collect(model, streams, sampler, config, lambda: None)
        expected = update(model, actor, critic, shuffle, second['samples'], second['frontiers'], config['ppo'])
        m, a, c, _, rng = restore_learners(saved, config['ppo'], 'test')
        self.assertTrue(a.state and c.state)
        actual = update(m, a, c, rng, second['samples'], second['frontiers'], config['ppo'])
        self.assertEqual(expected, actual)
        self.assertEqual(digest(m.state_dict()), digest(model.state_dict()))


if __name__ == '__main__':
    unittest.main()
