"""Independent formula and transactional checks for PPO, critic and GAE."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

import numpy as np
import torch

from full_factory_input_contracts import observation_hash
from full_factory_on_policy import parameter_hash, probability_hash
from full_factory_screen import sha
from ppo_area_model import AreaCritic, SCHEMA, actor_distribution, clipped_surrogate, gae, models
from ppo_area_update import save_experience, update
from random_factory_pilot import new_model


class ToyActor(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.layer = torch.nn.Linear(2, 1)

    def forward(self, obs):
        return self.layer(torch.as_tensor(obs['actions'])).squeeze(1)


class ToyCritic(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.layer = torch.nn.Linear(2, 1)

    def forward(self, obs, allowed):
        return self.layer(torch.as_tensor(obs['state'])).squeeze()


class PpoTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.manual_seed(19)
        self.config = dict(temperature=1., balanced=True, gae_lambda=.95, minibatch_size=2,
            epochs=2, clip_epsilon=.2, entropy_coefficient=.001, gradient_clip=.5, target_kl=1)

    def test_clipped_surrogate_handles_both_advantage_signs(self):
        logp = torch.tensor([1.5, .5, 1.5, .5], dtype=torch.float64).log().requires_grad_()
        advantages = torch.tensor([1., 1., -1., -1.], dtype=torch.float64)
        result, ratio = clipped_surrogate(logp, 0, advantages, .2)
        torch.testing.assert_close(result, torch.tensor([1.2, .5, -1.5, -.8], dtype=torch.float64))
        result.sum().backward()
        torch.testing.assert_close(logp.grad, torch.tensor([0, .5, -1.5, 0], dtype=torch.float64))

    def test_gae_delayed_reward_and_terminal_boundary(self):
        advantage, target = gae([0, 0, .2], [0, 0, 0], .95)
        torch.testing.assert_close(advantage, torch.tensor([.1805, .19, .2]))
        torch.testing.assert_close(target, advantage)
        advantage, target = gae([0, 0, .2], [.1, .15, .2], 1)
        torch.testing.assert_close(advantage, torch.tensor([.1, .05, 0]))
        torch.testing.assert_close(target, torch.tensor([.2, .2, .2]))
        self.assertEqual(float(gae([0], [0])[0][0]), 0)

    def test_masked_kind_balanced_behavior_ratio_is_one(self):
        actor = ToyActor()
        step = dict(actor_observation=dict(actions=np.array([[1, 0], [0, 1], [1, 1]], np.float32)),
                    kinds=['a', 'a', 'b'], allowed=[True, False, True], index=2)
        before = actor_distribution(actor, step, self.config)
        old = before.log_prob(torch.tensor(2)).detach()
        after = actor_distribution(actor, deepcopy(step), self.config)
        self.assertEqual(float((after.log_prob(torch.tensor(2)) - old).exp().detach()), 1)
        self.assertEqual(float(before.probs[1].detach()), 0)

    def test_real_critic_has_separate_parameters_and_does_not_change_actor_initialization(self):
        spec = dict(policy=dict(depth=1, width=8))
        actor, critic = models(spec, 17)
        self.assertEqual(parameter_hash(actor), parameter_hash(new_model(spec, 17)))
        self.assertFalse({p.data_ptr() for p in actor.parameters()} & {p.data_ptr() for p in critic.parameters()})
        world = dict(raster=np.zeros((28, 2, 2), np.float32), nodes=np.zeros((2, 32), np.float32),
            node_xy=np.array([[0, 0], [1, 1]]), edges=np.array([[0, 1], [1, 0]]),
            edge_values=np.zeros((2, 16), np.float32), messages=True)
        obs = dict(current=world, best=world, best_is_current=True,
                   actions=np.zeros((2, 32), np.float32), context=np.zeros(9, np.float32))
        before = parameter_hash(actor)
        value = critic(obs, [True, True])
        self.assertEqual(float(value.detach()), 0)
        optimizer = torch.optim.Adam(critic.parameters(), lr=.01)
        (.5 * (value - .2).square()).backward()
        self.assertTrue(any(p.grad is not None and p.grad.abs().sum() > 0 for p in critic.parameters()))
        optimizer.step()
        self.assertGreater(float(critic(obs, [True, True]).detach()), 0)
        self.assertEqual(parameter_hash(actor), before)

    def experience(self, root, actor, critic, corrupt=False):
        results = []
        for episode in range(2):
            steps = []
            for time in range(2):
                step = dict(actor_observation=dict(actions=np.array([[1 + episode, time], [time, 1], [1, 1]], np.float32)),
                    critic_observation=dict(state=np.array([episode, time], np.float32)),
                    allowed=[True, True, False], kinds=['a', 'b', 'b'], index=episode,
                    reward=.1 * (episode + 1) if time else 0)
                with torch.no_grad():
                    dist = actor_distribution(actor, step, self.config)
                    step.update(old_log_probability=float(dist.log_prob(torch.tensor(step['index']))),
                        old_value=float(critic(step['critic_observation'], step['allowed'])),
                        probabilities_sha256=probability_hash(dist.probs),
                        observation_sha256=observation_hash(step['actor_observation']),
                        critic_observation_sha256=observation_hash(step['critic_observation']))
                steps.append(step)
            if corrupt and episode == 1:
                steps[1]['old_log_probability'] += .1
            payload = dict(schema=SCHEMA, policy_sha256=parameter_hash(actor), critic_sha256=parameter_hash(critic),
                           input_sha256=str(episode), steps=steps)
            path = root / f'{episode}.pt.gz'
            save_experience(path, payload)
            results.append(dict(status='complete', policy_sha256=payload['policy_sha256'], critic_sha256=payload['critic_sha256'],
                input_sha256=str(episode), experience=str(path), experience_sha256=sha(path),
                row=dict(selected=2, initial_metrics=dict(score=[100, 0, 0]), best_metrics=dict(score=[90 - episode * 10, 0, 0]))))
        return results

    def test_update_replays_exactly_with_optimizer_and_shuffle_state(self):
        actor, critic = ToyActor(), ToyCritic()
        a_opt, c_opt = (torch.optim.Adam(m.parameters(), lr=.0003) for m in (actor, critic))
        rng = torch.Generator().manual_seed(23)
        with tempfile.TemporaryDirectory() as temporary:
            results = self.experience(Path(temporary), actor, critic)
            # Prime both optimizers, then verify continuation with nonempty Adam state.
            update(actor, critic, a_opt, c_opt, rng, results, self.config)
            other_a, other_c = deepcopy(actor), deepcopy(critic)
            other_ao, other_co = (torch.optim.Adam(m.parameters(), lr=.0003) for m in (other_a, other_c))
            other_ao.load_state_dict(deepcopy(a_opt.state_dict()))
            other_co.load_state_dict(deepcopy(c_opt.state_dict()))
            other_rng = torch.Generator().set_state(rng.get_state())
            results = self.experience(Path(temporary), actor, critic)
            expected = update(actor, critic, a_opt, c_opt, rng, results, self.config)
            actual = update(other_a, other_c, other_ao, other_co, other_rng, results, self.config)
            self.assertEqual(actual, expected)
            self.assertEqual(expected['optimizer_steps'], 4)
            self.assertEqual(len(expected['epochs']), 2)
            self.assertNotEqual(expected['critic_before_sha256'], expected['critic_after_sha256'])

    def test_bad_behavior_data_is_rejected_before_either_optimizer_mutates(self):
        actor, critic = ToyActor(), ToyCritic()
        a_opt, c_opt = (torch.optim.Adam(m.parameters(), lr=.0003) for m in (actor, critic))
        before = parameter_hash(actor), parameter_hash(critic)
        with tempfile.TemporaryDirectory() as temporary:
            results = self.experience(Path(temporary), actor, critic, corrupt=True)
            with self.assertRaises(ValueError):
                update(actor, critic, a_opt, c_opt, torch.Generator().manual_seed(0), results, self.config)
        self.assertEqual(before, (parameter_hash(actor), parameter_hash(critic)))
        self.assertFalse(a_opt.state or c_opt.state)


if __name__ == '__main__':
    unittest.main()
