"""Bounded regression and checkpoint checks, outside the normal training loop."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import torch

from full_factory_input_contracts import observation_hash
from full_factory_on_policy import parameter_hash, probability_hash
from full_factory_screen import sha
from ppo_area_model import SCHEMA, actor_distribution, models
from ppo_area_update import save_experience, update as reference_update
from ppo_training_update import update, collect_steps
from test_ppo_training_batch import observations


def experience(root, actor, critic, config, corrupt=False):
    results = []
    for i, trajectory in enumerate([observations()[:2], observations()[2:]]):
        for index, step in enumerate(trajectory):
            step['reward'] = .1 if index == len(trajectory) - 1 else 0.
            with torch.no_grad():
                dist = actor_distribution(actor, step, config)
                step.update(old_log_probability=float(dist.log_prob(torch.tensor(step['index']))),
                            old_value=float(critic(step['critic_observation'], step['allowed'])),
                            probabilities_sha256=probability_hash(dist.probs),
                            observation_sha256=observation_hash(step['actor_observation']),
                            critic_observation_sha256=observation_hash(step['critic_observation']))
        if corrupt and i == 1:
            trajectory[0]['old_value'] = float('nan')
        payload = dict(schema=SCHEMA, policy_sha256=parameter_hash(actor), critic_sha256=parameter_hash(critic),
                       input_sha256=str(i), steps=trajectory)
        path = root / f'experience-{i}.pt.gz'
        save_experience(path, payload)
        results.append(dict(status='complete', policy_sha256=payload['policy_sha256'], critic_sha256=payload['critic_sha256'],
                            input_sha256=str(i), experience=str(path), experience_sha256=sha(path),
                            row=dict(selected=len(trajectory), initial_metrics=dict(score=[100, 0, 0]),
                                     best_metrics=dict(score=[90, 0, 0]))))
    return results


class UpdateTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        self.spec = dict(policy=dict(depth=2, width=8))
        self.config = dict(temperature=.8, balanced=True, gae_lambda=.95, minibatch_size=2, epochs=2,
                           clip_epsilon=.2, entropy_coefficient=.001, gradient_clip=.5, target_kl=1.)

    def learners(self):
        actor, critic = models(self.spec, 17)
        torch.nn.init.normal_(critic.head[-1].weight, std=.1)
        opts = [torch.optim.Adam(model.parameters(), lr=.0003) for model in (actor, critic)]
        return actor, critic, *opts, torch.Generator().manual_seed(13)

    def test_full_update_matches_frozen_credit_and_reference_behavior(self):
        first, second = self.learners(), self.learners()
        with tempfile.TemporaryDirectory() as temporary:
            results = experience(Path(temporary), *first[:2], self.config)
            reference = reference_update(*first, results, self.config)
            actual = update(*second, results, self.config, microbatch=2)
            self.assertEqual(actual['credit'], reference['credit'])
            self.assertEqual(actual['optimizer_steps'], reference['optimizer_steps'])
            self.assertEqual(actual['shuffle_rng_sha256'], reference['shuffle_rng_sha256'])
            self.assertEqual(actual['execution']['full_update_executions'], 1)
            self.assertEqual(actual['behavior_source'], 'recorded-immutable-trajectory')
            for original, optimized in zip(first[:2], second[:2]):
                for (name, a), (_, b) in zip(original.named_parameters(), optimized.named_parameters()):
                    # A shared scalar logit bias cancels exactly from the policy.
                    # Roundoff in its zero gradient is amplified by Adam; compare
                    # actual probabilities below rather than this arbitrary offset.
                    if original is first[0] and name == 'score.2.bias':
                        continue
                    torch.testing.assert_close(a, b, rtol=5e-4, atol=5e-6, msg=name)
            for step in observations():
                with torch.no_grad():
                    torch.testing.assert_close(actor_distribution(first[0], step, self.config).probs,
                                               actor_distribution(second[0], step, self.config).probs,
                                               rtol=5e-5, atol=5e-6)
                    torch.testing.assert_close(first[1](step['critic_observation'], step['allowed']),
                                               second[1](step['critic_observation'], step['allowed']),
                                               rtol=5e-5, atol=5e-6)

    def test_nonempty_adam_and_shuffle_resume_is_exact(self):
        uninterrupted = self.learners()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            results = experience(root, *uninterrupted[:2], self.config)
            update(*uninterrupted, results, self.config, microbatch=2)
            checkpoint = root / 'checkpoint.pt'
            torch.save(dict(actor=uninterrupted[0].state_dict(), critic=uninterrupted[1].state_dict(),
                            actor_optimizer=uninterrupted[2].state_dict(), critic_optimizer=uninterrupted[3].state_dict(),
                            shuffle=uninterrupted[4].get_state()), checkpoint)
            state = torch.load(checkpoint, weights_only=True)
            resumed = self.learners()
            for model, key in zip(resumed[:4], ('actor', 'critic', 'actor_optimizer', 'critic_optimizer')):
                model.load_state_dict(state[key])
            resumed[4].set_state(state['shuffle'])
            self.assertTrue(resumed[2].state and resumed[3].state)
            results = experience(root, *uninterrupted[:2], self.config)
            expected = update(*uninterrupted, results, self.config, microbatch=2)
            actual = update(*resumed, results, self.config, microbatch=2)
            for key in ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
                        'critic_optimizer_sha256', 'shuffle_rng_sha256', 'credit', 'epochs'):
                self.assertEqual(actual[key], expected[key], key)

    def test_credit_loading_does_not_recompute_models_or_observation_hashes(self):
        actor, critic, *_ = self.learners()
        with tempfile.TemporaryDirectory() as temporary:
            results = experience(Path(temporary), actor, critic, self.config)
            with patch.object(actor, 'forward', side_effect=AssertionError('unexpected actor replay')), \
                 patch.object(critic, 'forward', side_effect=AssertionError('unexpected critic replay')), \
                 patch('full_factory_input_contracts.observation_hash', side_effect=AssertionError('unexpected observation audit')):
                steps, advantages, targets, _ = collect_steps(actor, critic, results, self.config)
            self.assertEqual(len(steps), 3)
            self.assertEqual(len(advantages), len(targets))


if __name__ == '__main__':
    unittest.main()
