"""Check actual successful segments, full-episode credit and replay ownership."""
from copy import deepcopy
import os
import unittest

import torch

from blueprint_ppo import readiness
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_store import ROOT, read, restore_learners
from blueprint_ppo_update import digest, learners, predictions, update
from blueprint_success import restore_buffer, snapshot
from blueprint_success_buffer import Evidence, SuccessBuffer
from blueprint_success_collect import collect_scripted
from blueprint_success_update import imitation_loss, update_success


class MemoryEvidence(Evidence):
    def __init__(self):
        self.chunks = {}

    def chunk(self, ordinal):
        return self.chunks[ordinal]


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires explicitly selected native library')
class SuccessReplayTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cls.base = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-replay.json')

    def setup_script(self, actions, chunk_actions):
        config = readiness(self.base)
        config['purpose'] = 'success-mechanical-readiness'
        config['starts'] = [config['starts'][0]]
        config['training'].update(streams=1, chunk_actions=chunk_actions, episode_actions=len(actions))
        config['success'].update(coefficient=.05, mechanical_actions=actions)
        runtime = Runtime(config, os.environ['AIC_NATIVE_LIBRARY'])
        model, actor, critic = learners(17, config['ppo'])
        sampler = torch.Generator().manual_seed(1000017)
        shuffle = torch.Generator().manual_seed(2000017)
        return config, [Stream(runtime, 0)], model, actor, critic, sampler, shuffle

    def two_success_script(self):
        first = deepcopy(self.base['success']['mechanical_actions'])
        return first + [dict(first[1], y=first[1]['y'] - 1), first[2], first[3], first[3]]

    def test_full_episode_return_and_cross_chunk_archive_dedup_and_guards(self):
        config, streams, model, _, _, sampler, _ = self.setup_script(self.two_success_script(), 4)
        evidence, buffer = MemoryEvidence(), SuccessBuffer()
        for ordinal in range(1, 5):
            chunk = collect_scripted(model, streams, sampler, config, lambda: None)
            evidence.chunks[ordinal] = chunk
            events = buffer.ingest(chunk, ordinal, evidence, config['success'], 1.)
            if ordinal == 1:
                self.assertEqual(len(buffer.state['entries']), 0)
                self.assertEqual(sum(e['kind'] == 'archived' for e in events), 1)
                saved_open = deepcopy(buffer.snapshot())
                buffer = SuccessBuffer(saved_open)
            if ordinal == 2:
                self.assertEqual(len(buffer.state['entries']), 2)
                entries = list(buffer.state['entries'].values())
                self.assertTrue(all(abs(l['return_to_terminal'] - .1) < 1e-12 for l in entries[0]['labels']))
                self.assertTrue(all(abs(l['return_to_terminal'] - .05) < 1e-12 for l in entries[1]['labels']))
                with self.assertRaisesRegex(ValueError, 'previous updates'):
                    buffer.sequences(evidence, ordinal)
                self.assertEqual(len(buffer.sequences(evidence, ordinal + 1)), 2)
                limited = SuccessBuffer(saved_open)
                with self.assertRaisesRegex(ValueError, 'unique sequence guard'):
                    limited.ingest(chunk, ordinal, evidence, dict(config['success'], max_sequences=0), 1.)
        self.assertEqual(buffer.state['positive_commits'], 4)
        self.assertEqual(len(buffer.state['entries']), 2)
        self.assertEqual(buffer.state['duplicates'], 2)
        self.assertTrue(all(e['occurrences'] == 2 for e in buffer.state['entries'].values()))
        self.assertEqual(buffer.state['active'], {})
        with self.assertRaisesRegex(ValueError, 'episode must begin'):
            SuccessBuffer().ingest(evidence.chunks[2], 2, evidence, config['success'], 1.)

    def test_restore_boundary_and_rejected_or_noop_actions_do_not_become_labels(self):
        restore, pose, repair, commit = self.base['success']['mechanical_actions']
        actions = [pose, restore, pose, commit, repair, repair, commit, commit]
        config, streams, model, _, _, sampler, _ = self.setup_script(actions, 4)
        evidence, buffer = MemoryEvidence(), SuccessBuffer()
        for ordinal in (1, 2):
            chunk = collect_scripted(model, streams, sampler, config, lambda: None)
            evidence.chunks[ordinal] = chunk
            buffer.ingest(chunk, ordinal, evidence, config['success'], 1.)
        entry = next(iter(buffer.state['entries'].values()))
        actions = [evidence.sample(l['reference'])['policy_action'] for l in entry['labels']]
        self.assertEqual(actions, [3, 5, 7])
        self.assertEqual([evidence.sample(r)['policy_action'] for r in entry['segment']['references']], [3, 4, 5, 6, 7])

    def test_success_without_a_policy_pose_is_also_preserved(self):
        actions = self.base['success']['mechanical_actions'][2:]
        config, _, model, _, _, sampler, _ = self.setup_script(actions, 1)
        fixture = read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo.json')
        config['starts'] = [next(s for s in fixture['starts'] if s['name'] == 'repair_ready')]
        streams = [Stream(Runtime(config, os.environ['AIC_NATIVE_LIBRARY']), 0)]
        evidence, buffer = MemoryEvidence(), SuccessBuffer()
        for ordinal in (1, 2):
            evidence.chunks[ordinal] = collect_scripted(model, streams, sampler, config, lambda: None)
            buffer.ingest(evidence.chunks[ordinal], ordinal, evidence, config['success'], 1.)
        self.assertEqual(buffer.state['positive_commits'], 1)
        entry = next(iter(buffer.state['entries'].values()))
        self.assertEqual([l['stage'] for l in entry['labels']], ['repair', 'commit'])

    def test_advantage_gate_has_no_direct_critic_gradient_and_zero_gate_is_zero(self):
        config, streams, model, _, _, sampler, _ = self.setup_script(self.two_success_script(), 4)
        evidence, buffer = MemoryEvidence(), SuccessBuffer()
        for ordinal in (1, 2):
            evidence.chunks[ordinal] = collect_scripted(model, streams, sampler, config, lambda: None)
            buffer.ingest(evidence.chunks[ordinal], ordinal, evidence, config['success'], 1.)
        sequences = buffer.sequences(evidence, 3)
        rows = [r for s in sequences for r in s['rows']]
        with torch.no_grad():
            _, _, values = predictions(model, rows)
        for row, value in zip(rows, values):
            row['return_to_terminal'] = float(value) + .1
        loss, diagnostics = imitation_loss(model, sequences)
        loss.backward()
        self.assertEqual(diagnostics['positive'], len(rows))
        self.assertTrue(all(p.grad is None for p in model.critic.parameters()))
        self.assertGreater(sum(float(p.grad.abs().sum()) for n, p in model.named_parameters() if n.startswith('actor.') and p.grad is not None), 0.)
        for row, value in zip(rows, values):
            row['return_to_terminal'] = float(value) - .1
        loss, diagnostics = imitation_loss(model, sequences)
        self.assertEqual(float(loss.detach()), 0.)
        self.assertEqual(diagnostics['positive'], 0)

    def test_empty_and_zero_parity_then_nonempty_adam_buffer_replay_with_kl_tail(self):
        config, streams, model, actor, critic, sampler, shuffle = self.setup_script(self.base['success']['mechanical_actions'], 2)
        evidence, buffer = MemoryEvidence(), SuccessBuffer()
        for ordinal in range(1, 5):
            chunk = collect_scripted(model, streams, sampler, config, lambda: None)
            evidence.chunks[ordinal] = chunk
            sequences = buffer.sequences(evidence, ordinal)
            saved = deepcopy(snapshot(model, actor, critic, sampler, shuffle, 17, ordinal - 1, streams, 'unit', buffer))
            actual = update_success(model, actor, critic, shuffle, chunk['samples'], chunk['frontiers'], config['ppo'], config['success'], sequences, 17, ordinal)
            learner, restored_buffer = restore_buffer(saved)
            m, a, c, _, rng = restore_learners(deepcopy(learner), config['ppo'], 'unit')
            replayed = update_success(m, a, c, rng, chunk['samples'], chunk['frontiers'], config['ppo'], config['success'], restored_buffer.sequences(evidence, ordinal), 17, ordinal)
            self.assertEqual(actual, replayed)
            self.assertEqual(actual['actor_epochs'], 1)
            self.assertEqual(actual['critic_epochs'], 4)
            self.assertTrue(actual['critic_tail_preserves_actor'])
            if ordinal == 1:
                m, a, c, _, rng = restore_learners(deepcopy(learner), config['ppo'], 'unit')
                baseline = update(m, a, c, rng, chunk['samples'], chunk['frontiers'], config['ppo'])
                self.assertEqual({k: v for k, v in actual.items() if k != 'success'}, baseline)
            if ordinal == 3:
                self.assertTrue(learner['actor_optimizer']['state'])
                self.assertTrue(restored_buffer.state['entries'])
                m, a, c, _, rng = restore_learners(deepcopy(learner), config['ppo'], 'unit')
                zero = update_success(m, a, c, rng, chunk['samples'], chunk['frontiers'], config['ppo'], dict(config['success'], coefficient=0), sequences, 17, ordinal)
                m, a, c, _, rng = restore_learners(deepcopy(learner), config['ppo'], 'unit')
                baseline = update(m, a, c, rng, chunk['samples'], chunk['frontiers'], config['ppo'])
                self.assertEqual({k: v for k, v in zero.items() if k != 'success'}, baseline)
            buffer.mark_used(actual['success']['selected_ids'])
            buffer.ingest(chunk, ordinal, evidence, config['success'], 1.)
        self.assertGreater(actual['success']['positive_labels'], 0)


if __name__ == '__main__':
    unittest.main()
