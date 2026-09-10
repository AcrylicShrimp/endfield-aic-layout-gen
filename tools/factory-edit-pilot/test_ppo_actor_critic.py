"""Factorial contracts, shared gradients, hidden inputs and critic-only updates."""
from copy import deepcopy
from itertools import product
import os
from pathlib import Path
import tempfile
import unittest

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')

import numpy as np
import torch

from full_factory_input_contracts import observation_hash
from full_factory_on_policy import parameter_hash, probability_hash
from ppo_actor_critic import treatment, validate, optimizers, observation, predictions as scalar_predictions
from ppo_area_model import models, SCHEMA, actor_distribution
from ppo_critic_metrics import episode, aggregate
from ppo_harness_state import save_checkpoint, restore_checkpoint
from ppo_reward import SCHEMA as REWARD_SCHEMA, DEFAULT, totals
from ppo_spatial_model import specification, validate_model
from ppo_training_batch import prepare, predictions, distribution, critic_values
from ppo_training_update import update
from test_ppo_training_batch import observations


def inputs(config):
    steps = observations()
    for i, step in enumerate(steps):
        actor = step['actor_observation']
        actor['current'] = step['critic_observation']['current']
        actor['operands'][:, 0] = 1
        actor['history'] = np.full((i * 3, 53), .07 * (i + 1), np.float32)
        step['critic_observation']['history'] = actor['history'] if config['critic_history'] else actor['history'][:0]
    return steps


def learners(config, device='cpu'):
    actor, critic = models(dict(model=dict(specification('cnn2'), actor_critic=config)), 17)
    torch.manual_seed(43)
    torch.nn.init.normal_(critic.head[-1].weight, std=.1)
    actor.to(device)
    critic.to(device)
    return actor, critic, *optimizers(actor, critic, .0003), torch.Generator().manual_seed(13)


def experiences(actor, critic, config):
    steps = inputs(critic.treatment)
    results, payloads = [], []
    device = next(actor.parameters()).device
    for i, trajectory in enumerate((steps[:2], steps[2:])):
        for index, step in enumerate(trajectory):
            reward = .1 if index == len(trajectory) - 1 else 0.
            step.update(reward=reward, area_reward=reward, failure_cost=0.,
                        status='improved' if reward else 'accepted_equal')
            with torch.no_grad():
                dist = actor_distribution(actor, step, config)
                step.update(old_log_probability=float(dist.log_prob(torch.tensor(step['index'], device=device))),
                            old_value=float(critic(step['critic_observation'], step['allowed'])),
                            probabilities_sha256=probability_hash(dist.probs),
                            observation_sha256=observation_hash(step['actor_observation']),
                            critic_observation_sha256=observation_hash(step['critic_observation']))
        payload = dict(schema=SCHEMA, reward_schema=REWARD_SCHEMA, reward_config=dict(DEFAULT),
                       policy_sha256=parameter_hash(actor), critic_sha256=parameter_hash(critic),
                       input_sha256=str(i), input_contract='current-history-16', steps=trajectory)
        payloads.append(payload)
        results.append(dict(status='complete', candidate='current-history-16',
                            policy_sha256=payload['policy_sha256'], critic_sha256=payload['critic_sha256'],
                            input_sha256=str(i), row=dict(selected=len(trajectory), **totals(trajectory, DEFAULT),
                                initial_metrics=dict(score=[100, 0, 0]), best_metrics=dict(score=[90, 0, 0]))))
    return results, payloads


class CombinationTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        if torch.cuda.is_available():
            torch.backends.cuda.matmul.fp32_precision = 'ieee'
            torch.backends.cudnn.conv.fp32_precision = 'ieee'
            torch.backends.cudnn.benchmark = False
        self.device = os.environ.get('AIC_COMBINATIONS_TEST_DEVICE', 'cpu')
        self.config = dict(temperature=.8, balanced=True, gae_lambda=.95, minibatch_size=2, epochs=3,
                           clip_epsilon=.2, entropy_coefficient=.001, gradient_clip=.5, target_kl=1e-12)

    def test_registered_factors_initialization_and_unique_optimizer_ownership(self):
        actors, critics = set(), set()
        for shared, history, tail in product((False, True), repeat=3):
            config = treatment(shared, history, tail)
            model = dict(specification('cnn2'), actor_critic=config)
            validate_model(model)
            actor, critic = models(dict(model=model), 17)
            actors.add(parameter_hash(actor))
            critics.add(parameter_hash(critic))
            self.assertEqual(actor.world is critic.world, shared)
            self.assertEqual(actor.history is critic.history, shared)
            ao, co = optimizers(actor, critic, .0003)
            a = {id(p) for g in ao.param_groups for p in g['params']}
            c = {id(p) for g in co.param_groups for p in g['params']}
            self.assertFalse(a & c)
            self.assertEqual(a | c, {id(p) for m in (actor, critic) for p in m.parameters()})
        self.assertEqual(len(actors), 1)
        self.assertEqual(len(critics), 1)
        with self.assertRaisesRegex(ValueError, 'combinations'):
            validate(dict(treatment(False, False, False), critic_history=1))
        with self.assertRaisesRegex(ValueError, 'fixed cnn2'):
            validate_model(dict(specification('cnn4'), actor_critic=treatment(False, False, False)))

    def test_shared_scalar_and_padded_predictions_and_gradients(self):
        for shared, history in product((False, True), repeat=2):
            config = treatment(shared, history, False)
            actor, critic, *_ = learners(config, self.device)
            ref_actor, ref_critic = deepcopy((actor, critic))
            steps = inputs(config)
            batch = prepare(steps, dict(self.config, actor_critic=config), 32, self.device)
            logp, entropy, _, values = predictions(actor, critic, batch, self.config)
            expected_logp, expected_values = [], []
            for step in steps:
                expected_logp.append(actor_distribution(ref_actor, step, self.config).log_prob(
                    torch.tensor(step['index'], device=self.device)))
                expected_values.append(ref_critic(step['critic_observation'], step['allowed']))
                scalar_logits, scalar_value = scalar_predictions(ref_actor, ref_critic,
                    step['actor_observation'], step['critic_observation'], step['allowed'])
                torch.testing.assert_close(scalar_logits, ref_actor(step['actor_observation']))
                torch.testing.assert_close(scalar_value, expected_values[-1])
            expected_logp, expected_values = torch.stack(expected_logp), torch.stack(expected_values)
            torch.testing.assert_close(logp, expected_logp, rtol=5e-5, atol=5e-6)
            torch.testing.assert_close(values, expected_values, rtol=5e-5, atol=5e-6)
            weights = torch.tensor([.7, -.4, 1.3], device=self.device)
            ((logp * weights).sum() + values.square().sum()).backward()
            ((expected_logp * weights).sum() + expected_values.square().sum()).backward()
            for actual, expected in ((actor, ref_actor), (critic, ref_critic)):
                for (name, p), (_, q) in zip(actual.named_parameters(), expected.named_parameters()):
                    self.assertEqual(p.grad is None, q.grad is None, name)
                    if p.grad is not None:
                        torch.testing.assert_close(p.grad, q.grad, rtol=3e-4, atol=2e-5, msg=name)
            self.assertTrue(torch.isfinite(entropy).all())

    def test_best_world_stays_hidden_and_history_toggle_is_effective(self):
        for shared, history in product((False, True), repeat=2):
            config = treatment(shared, history, False)
            actor, critic, *_ = learners(config, self.device)
            steps = inputs(config)
            def predict(items):
                with torch.no_grad():
                    return predictions(actor, critic, prepare(items, dict(self.config, actor_critic=config), 32, self.device), self.config)
            before = predict(steps)
            changed = deepcopy(steps)
            changed[1]['critic_observation']['best']['raster'] *= 10
            after = predict(changed)
            torch.testing.assert_close(before[0], after[0], rtol=0, atol=0)
            self.assertNotEqual(float(before[3][1]), float(after[3][1]))
            changed = deepcopy(steps)
            changed[1]['actor_observation']['history'] *= -5
            changed[1]['critic_observation'] = observation(critic, changed[1]['actor_observation'], changed[1]['critic_observation'])
            after = predict(changed)
            if history:
                self.assertNotEqual(float(before[3][1]), float(after[3][1]))
            else:
                torch.testing.assert_close(before[3], after[3], rtol=0, atol=0)
            malformed = deepcopy(steps)
            malformed[1]['critic_observation']['history'] = np.zeros((1, 53), np.float32)
            with self.assertRaisesRegex(ValueError, 'history must match'):
                predict(malformed)

    def test_tail_preserves_policy_adam_and_joint_shuffle_for_both_backbones(self):
        for shared, history in product((False, True), repeat=2):
            stop = learners(treatment(shared, history, False), self.device)
            tail = learners(treatment(shared, history, True), self.device)
            results, payloads = experiences(*stop[:2], self.config)
            before = parameter_hash(stop[1])
            a = update(*stop, results, self.config, microbatch=2, experiences=payloads)
            b = update(*tail, results, self.config, microbatch=2, experiences=payloads)
            self.assertEqual(a['actor_epochs'], 1)
            self.assertEqual(b['critic_epochs'], 3)
            self.assertEqual(b['critic_optimizer_steps'], 3 * b['actor_optimizer_steps'])
            self.assertEqual(a['epochs'], b['epochs'])
            for key in ('policy_after_sha256', 'actor_optimizer_sha256', 'shuffle_rng_sha256'):
                self.assertEqual(a[key], b[key], key)
            self.assertNotEqual(a['critic_after_sha256'], b['critic_after_sha256'])
            self.assertNotEqual(before, b['critic_after_sha256'])
            self.assertTrue(b['critic_tail']['actor_parameters_and_optimizer_unchanged'])
            for row in b['critic_tail']['epochs']:
                self.assertAlmostEqual(row['approximate_kl'], b['epochs'][-1]['approximate_kl'], places=12)

    def test_checkpoint_continuation_preserves_aliases_adam_and_updates(self):
        for shared in (False, True):
            config = treatment(shared, True, True)
            first = learners(config, self.device)
            results, payloads = experiences(*first[:2], self.config)
            update(*first, results, self.config, microbatch=2, experiences=payloads)
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                ref = save_checkpoint(root, root/'checkpoint.pt', first, 'test-protocol', 'test-job', 1)
                second = learners(config, self.device)
                restore_checkpoint(root, ref, second, 'test-protocol', 'test-job', 1)
                self.assertEqual(second[0].world is second[1].world, shared)
                self.assertTrue(second[2].state and second[3].state)
                results, payloads = experiences(*first[:2], self.config)
                a = update(*first, results, self.config, microbatch=2, experiences=payloads)
                b = update(*second, results, self.config, microbatch=2, experiences=payloads)
                for key in ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
                            'critic_optimizer_sha256', 'shuffle_rng_sha256', 'epochs', 'credit'):
                    self.assertEqual(a[key], b[key], key)

    def test_predictive_errors_use_remaining_reward_and_weighted_counts(self):
        first = episode([dict(reward=0., old_value=.4), dict(reward=.3, old_value=.2)])
        second = episode([dict(reward=-.1, old_value=.2)])
        result = aggregate([dict(critic_prediction=first), dict(critic_prediction=second)])['critic_prediction']
        self.assertEqual(result['all']['count'], 3)
        self.assertAlmostEqual(result['all']['bias'], .1)
        self.assertAlmostEqual(result['all']['mae'], .5/3)
        self.assertAlmostEqual(result['all']['rmse'], (.11/3)**.5)
        self.assertEqual(result['late']['count'], 0)
        self.assertIsNone(result['late']['explained_variance'])


if __name__ == '__main__':
    unittest.main()
