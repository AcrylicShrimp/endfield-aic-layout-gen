"""Execution cutover preserves replay draws, stage weighting, Adam and critic tails."""
from copy import deepcopy
import os
import unittest

import torch

from blueprint_execution_update import draw_sequences, imitation, prepare_minibatch, update_packed
from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest, learners
from blueprint_success_buffer import SuccessBuffer
from blueprint_success_collect import collect_scripted
from blueprint_success_update import imitation_loss, update_success


class ReplayDrawTests(unittest.TestCase):
    def test_balanced_draws_have_four_per_stratum_and_separate_order_rng(self):
        sequences = [dict(id=str(i), stratum='large' if i < 2 else 'small') for i in range(9)]
        draw = torch.Generator().manual_seed(3)
        order = torch.Generator().manual_seed(4)
        shuffle = torch.Generator().manual_seed(5)
        before = digest(shuffle.get_state())
        for _ in range(30):
            selected = draw_sequences(sequences, 'balanced', draw, order)
            self.assertEqual(sum(s['stratum'] == 'large' for s in selected), 4)
            self.assertEqual(sum(s['stratum'] == 'small' for s in selected), 4)
        self.assertEqual(before, digest(shuffle.get_state()))
        with self.assertRaisesRegex(ValueError, 'stratum-empty'):
            draw_sequences(sequences[2:], 'balanced', draw, order)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires selected native library')
class UpdateTests(unittest.TestCase):
    def fixture(self):
        from test_blueprint_success import MemoryEvidence, SuccessReplayTests
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        helper = SuccessReplayTests()
        helper.base = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-replay.json')
        config, streams, model, actor, critic, sampler, shuffle = helper.setup_script(helper.two_success_script(), 4)
        config['ppo']['advantage_normalization'] = 'center'
        evidence, buffer = MemoryEvidence(), SuccessBuffer()
        for ordinal in (1, 2):
            chunk = collect_scripted(model, streams, sampler, config, lambda: None)
            evidence.chunks[ordinal] = chunk
            update_success(model, actor, critic, shuffle, chunk['samples'], chunk['frontiers'], config['ppo'],
                           config['success'], buffer.sequences(evidence, ordinal), 17, ordinal)
            buffer.ingest(chunk, ordinal, evidence, config['success'], 1.)
        # Mechanical execution test only: ensure the real success labels exercise
        # a positive SIL gate while retaining nonempty optimizer moments.
        with torch.no_grad():
            model.critic[-1].bias.sub_(1.)
        chunk = collect_scripted(model, streams, sampler, config, lambda: None)
        sequences = buffer.sequences(evidence, 3)
        for i, sequence in enumerate(sequences):
            sequence.update(child_admitted_update=0, origin='imported', stratum='large' if i == 0 else 'small')
        return config, model, actor, critic, shuffle, chunk, sequences

    def test_cpu_cutover_keeps_uniform_draws_and_optimizer_numerics(self):
        config, model, actor, critic, shuffle, chunk, sequences = self.fixture()
        replacement, new_actor, new_critic = learners(17, config['ppo'])
        replacement.load_state_dict(deepcopy(model.state_dict()))
        new_actor.load_state_dict(deepcopy(actor.state_dict()))
        new_critic.load_state_dict(deepcopy(critic.state_dict()))
        new_shuffle = torch.Generator().set_state(shuffle.get_state())
        old = update_success(model, actor, critic, shuffle, chunk['samples'], chunk['frontiers'], config['ppo'],
                             config['success'], sequences, 17, 513)
        new = update_packed(replacement, new_actor, new_critic, new_shuffle, chunk['samples'], chunk['frontiers'],
                            config['ppo'], sequences, .05, 'uniform', 17, 513, 1, 'cpu')
        self.assertEqual(old['success']['selected_ids'], new['success']['selected_ids'])
        self.assertEqual(old['success']['rng_sha256'], new['success']['rng_sha256'])
        self.assertEqual(old['shuffle_rng'], new['shuffle_rng'])
        self.assertEqual(old['actor_epochs'], new['actor_epochs'])
        self.assertEqual(old['success']['positive_labels'], new['success']['positive_labels'])
        for a, b in zip(model.parameters(), replacement.parameters(), strict=True):
            torch.testing.assert_close(a, b, rtol=1e-4, atol=2e-6)
        for a, b in ((actor, new_actor), (critic, new_critic)):
            for key, state in a.state_dict()['state'].items():
                for field, tensor in state.items():
                    torch.testing.assert_close(tensor, b.state_dict()['state'][key][field], rtol=1e-4, atol=2e-6)

    def test_balanced_nonempty_adam_exact_replay_and_kl_freeze(self):
        self.replay('cpu')

    @unittest.skipUnless(torch.cuda.is_available(), 'requires CUDA')
    def test_cuda_nonempty_adam_exact_replay_and_kl_freeze(self):
        self.replay('cuda:0')

    def replay(self, device):
        config, model, actor, critic, shuffle, chunk, sequences = self.fixture()
        config['ppo']['target_kl'] = 0.
        saved = deepcopy(dict(model=model.state_dict(), actor=actor.state_dict(), critic=critic.state_dict(), shuffle=shuffle.get_state()))
        results = []
        for _ in range(2):
            learner, a, c = learners(17, config['ppo'])
            learner.to(device)
            learner.load_state_dict(saved['model'])
            a.load_state_dict(deepcopy(saved['actor']))
            c.load_state_dict(deepcopy(saved['critic']))
            rng = torch.Generator().set_state(saved['shuffle'])
            result = update_packed(learner, a, c, rng, chunk['samples'], chunk['frontiers'], config['ppo'],
                                   sequences, .5, 'balanced', 17, 513, 1, device)
            result.pop('execution')
            self.assertEqual(result['actor_epochs'], 1)
            self.assertEqual(result['critic_epochs'], 4)
            self.assertTrue(result['critic_tail_preserves_actor'])
            self.assertGreater(result['success']['positive_labels'], 0)
            results.append(result)
        self.assertEqual(*results)

    def test_stage_mean_and_positive_gate_match_reference_without_critic_gradient(self):
        _, model, _, _, _, chunk, sequences = self.fixture()
        model.zero_grad(set_to_none=True)
        sequences[0]['rows'].append(deepcopy(sequences[0]['rows'][0]))
        for i, row in enumerate(sequences[0]['rows']):
            row['return_to_terminal'] = -100. if i == 0 else 100.
        reference, _ = imitation_loss(model, sequences)
        other = deepcopy(model)
        prepared = prepare_minibatch(chunk['samples'], [0], torch.zeros(len(chunk['samples'])),
                                     torch.zeros(len(chunk['samples'])), sequences)
        actual, diagnostics = imitation(other, prepared['sil'], 'cpu')
        torch.testing.assert_close(actual, reference, rtol=1e-5, atol=1e-6)
        reference.backward()
        actual.backward()
        self.assertTrue(all(p.grad is None for p in other.critic.parameters()))
        self.assertEqual(float(diagnostics[0, 0]), 0.)
        for a, b in zip(model.parameters(), other.parameters(), strict=True):
            if a.grad is not None:
                torch.testing.assert_close(a.grad, b.grad, rtol=1e-4, atol=2e-5)


if __name__ == '__main__':
    unittest.main()
