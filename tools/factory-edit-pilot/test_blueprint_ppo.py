"""Check true-terminal credit, disjoint graph batching and durable update continuation."""
from copy import deepcopy
import os
import unittest

import torch

from blueprint_edit_input import BlueprintActorCritic, prepare
from blueprint_ppo import readiness
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream, semantic
from blueprint_ppo_store import ROOT, model_checkpoint, read, restore_learners
from blueprint_ppo_update import gae, learners, update


class ContinuingCreditTests(unittest.TestCase):
    def test_bootstrap_survives_chunk_end_and_stops_at_real_terminal(self):
        advantages, targets = gae([1., 2.], [.5, .8], [False, False], 3., 1., 1.)
        torch.testing.assert_close(targets, torch.tensor([6., 5.]))
        torch.testing.assert_close(advantages, torch.tensor([5.5, 4.2]))
        _, targets = gae([1., 2.], [.5, .8], [False, True], 3., 1., 1.)
        torch.testing.assert_close(targets, torch.tensor([3., 2.]))
        _, targets = gae([1., 2., 100.], [0., 0., 0.], [False, True, False], 4., 1., 1.)
        torch.testing.assert_close(targets, torch.tensor([3., 2., 104.]))
        advantages, _ = gae([1., 2.], [.5, .8], [False, False], 3., 1., .95)
        torch.testing.assert_close(advantages, torch.tensor([5.29, 4.2]))


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires explicitly selected native library')
class PersistentPpoTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cls.config = readiness(read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo.json'))
        cls.runtime = Runtime(cls.config, os.environ['AIC_NATIVE_LIBRARY'])

    def test_packed_graphs_match_individual_outputs_and_gradients(self):
        streams = [Stream(self.runtime, i) for i in range(4)]
        inputs = [prepare(s.observe()) for s in streams]
        torch.manual_seed(17)
        single = BlueprintActorCritic()
        combined = deepcopy(single)
        logits, values = [], []
        for observation in inputs:
            action, value = single(observation)
            logits.append(action)
            values.append(value)
        (sum(v.square() for v in values) + sum(a.square().sum() for a in logits)).backward()
        batch = pack(inputs)
        packed_logits, packed_values = combined.forward_packed(batch)
        torch.testing.assert_close(packed_logits, torch.cat(logits), rtol=1e-5, atol=1e-6)
        torch.testing.assert_close(packed_values, torch.stack(values), rtol=1e-5, atol=1e-6)
        (packed_values.square().sum() + packed_logits.square().sum()).backward()
        for a, b in zip(single.parameters(), combined.parameters(), strict=True):
            torch.testing.assert_close(a.grad, b.grad, rtol=1e-4, atol=2e-5)
        logp, entropy = distribution(packed_logits, batch)
        for probabilities in logp.exp().split(batch['action_counts']):
            self.assertAlmostEqual(probabilities.sum().item(), 1., places=6)
        self.assertTrue(torch.isfinite(entropy).all().item())

    def test_second_update_replays_from_nonempty_adam_and_preserves_native_draft(self):
        streams = [Stream(self.runtime, i) for i in range(4)]
        model, actor, critic = learners(17, self.config['ppo'])
        sampler = torch.Generator().manual_seed(1000017)
        shuffle = torch.Generator().manual_seed(2000017)
        first = collect(model, streams, sampler, self.config, lambda: None)
        first_result = update(model, actor, critic, shuffle, first['samples'], first['frontiers'], self.config['ppo'])
        self.assertEqual(first_result['nonterminal_bootstraps'], 4)
        self.assertLess(first_result['actor_epochs'], first_result['critic_epochs'])
        saved = deepcopy(model_checkpoint(model, actor, critic, sampler, shuffle, 17, 1, streams, 'test-protocol'))
        self.assertTrue(saved['actor_optimizer']['state'])
        self.assertTrue(saved['critic_optimizer']['state'])
        for saved_stream, frontier in zip(saved['streams'], first['frontiers']):
            restored = Stream.restore(self.runtime, saved_stream)
            self.assertEqual(semantic(restored.observe()), frontier['semantic'])
            self.assertEqual(restored.policy_actions, 4)
        second = collect(model, streams, sampler, self.config, lambda: None)
        result = update(model, actor, critic, shuffle, second['samples'], second['frontiers'], self.config['ppo'])
        self.assertEqual(result['terminal_frontiers'], 4)
        self.assertTrue(all(s.session is None for s in streams))
        restored_model, restored_actor, restored_critic, _, restored_shuffle = restore_learners(saved, self.config['ppo'], 'test-protocol')
        replayed = update(restored_model, restored_actor, restored_critic, restored_shuffle,
                          second['samples'], second['frontiers'], self.config['ppo'])
        self.assertEqual(result, replayed)


if __name__ == '__main__':
    unittest.main()
