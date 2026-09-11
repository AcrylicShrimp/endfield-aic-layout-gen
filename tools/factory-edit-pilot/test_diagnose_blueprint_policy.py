"""Counterfactual arms must own their Adam tensors; zero reward is not zero GAE."""
import unittest

import torch

from blueprint_ppo_store import model_checkpoint
from blueprint_ppo_update import credit, digest, learners
from diagnose_blueprint_policy import fresh_learners


class DiagnosticTests(unittest.TestCase):
    def test_counterfactual_adam_step_cannot_mutate_frozen_input(self):
        config = dict(width=4, learning_rate=.0003)
        model, actor, critic = learners(17, config)
        sum(p.square().sum() for p in model.parameters()).backward()
        actor.step()
        critic.step()
        saved = model_checkpoint(model, actor, critic, torch.Generator(), torch.Generator(), 17, 1, [], 'protocol')
        before = digest(saved)
        restored, a, c, _, _ = fresh_learners(saved, config, 'protocol')
        sum(p.square().sum() for p in restored.parameters()).backward()
        a.step()
        c.step()
        self.assertEqual(digest(saved), before)
        _, again, _, _, _ = fresh_learners(saved, config, 'protocol')
        self.assertEqual(digest(again.state_dict()), digest(saved['actor_optimizer']))
        self.assertNotEqual(digest(a.state_dict()), digest(again.state_dict()))

    def test_zero_reward_can_have_normalized_credit_from_values(self):
        samples = [dict(stream=0, reward=0., old_value=value, terminal=i == 2)
                   for i, value in enumerate([.03, .01, .02])]
        frontiers = [dict(stream=0, terminal=True, bootstrap=0.)]
        settings = dict(gamma=1., gae_lambda=.95)
        raw, normalized, _ = credit(samples, frontiers, settings)
        self.assertTrue(bool((raw < 0).all()))
        self.assertAlmostEqual(float(normalized.std(unbiased=False)), 1., places=6)
        self.assertTrue(bool((normalized > 0).any()) and bool((normalized < 0).any()))
        for sample in samples:
            sample['old_value'] = 0.
        _, normalized, _ = credit(samples, frontiers, settings)
        self.assertTrue(bool((normalized == 0).all()))


if __name__ == '__main__':
    unittest.main()
