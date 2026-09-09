"""Native observation and gradient equivalence across the twelve PPO inputs."""
from copy import deepcopy
import unittest

import torch

import test_full_factory_policy as fixture
from full_factory_input_contracts import CONTRACTS, observation, observation_hash, selected_action_token, history_record, selection_mask
from full_factory_selection import SelectionMemory
from ppo_area_model import models, actor_distribution
from ppo_training_batch import prepare, distribution
from ppo_training_observation import observation_pair, WitnessIdentities


class InputBatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixture.FactoryPolicyTests.setUpClass()
        cls.fixture = fixture.FactoryPolicyTests()
        cls.runtime = fixture.FactoryPolicyTests.runtime
        torch.set_num_threads(1)

    @classmethod
    def tearDownClass(cls):
        fixture.FactoryPolicyTests.tearDownClass()

    def steps(self, candidate):
        env = self.fixture.episode()
        env.step(next(a for a in env.describe()['actions'] if a['kind'] == 'insert'))
        description = env.describe()
        base = observation(env.state, description, .5, [], 'full-current', self.runtime)
        token = selected_action_token(base, 0)
        history = [history_record(token, [10, 2, 1],
                                  dict(status='executor_rejected', unchanged=True, current_score=[10, 2, 1]), [10, 2, 1])
                   for _ in range(20)]
        steps = []
        for length in [0, 1, 20]:
            prior = history[:length]
            actor, critic = observation_pair(env.state, description, .5, prior, candidate, self.runtime, True)
            expected = observation(env.state, description, .5, prior, candidate, self.runtime)
            self.assertEqual(observation_hash(actor), observation_hash(expected), candidate)
            self.assertEqual(observation_hash(critic), observation_hash(observation(
                env.state, description, .5, [], 'current-best-layout', self.runtime)))
            memory = SelectionMemory(env.state['current'])
            actual = WitnessIdentities().mask(env.state, description, memory, True, candidate)
            self.assertEqual(actual, selection_mask(env.state, description, memory, candidate))
            allowed = actual[1]
            steps.append(dict(actor_observation=actor, critic_observation=critic, allowed=allowed,
                              kinds=[a['kind'] for a in description['actions']],
                              index=next(i for i, ok in enumerate(allowed) if ok), old_log_probability=0., old_value=0.))
        return steps

    def compare(self, steps):
        config = dict(temperature=.8, balanced=True)
        actor, _ = models(dict(policy=dict(depth=2, width=8)), 17)
        other = deepcopy(actor)
        expected = torch.stack([actor_distribution(actor, s, config).log_prob(torch.tensor(s['index'])) for s in steps])
        actual = distribution(other, prepare(steps, config, 8, 'cpu'), config)[0]
        torch.testing.assert_close(actual, expected, rtol=2e-5, atol=3e-6)
        weights = torch.arange(1, len(steps) + 1, dtype=torch.float32)
        (expected * weights).sum().backward()
        (actual * weights).sum().backward()
        for (name, a), (_, b) in zip(actor.named_parameters(), other.named_parameters()):
            self.assertEqual(a.grad is None, b.grad is None, name)
            if a.grad is not None:
                torch.testing.assert_close(a.grad, b.grad, rtol=5e-5, atol=5e-6, msg=name)
        return other

    def test_all_twelve_observations_masks_and_actor_gradients_match(self):
        for candidate, contract in CONTRACTS.items():
            with self.subTest(candidate=candidate):
                actor = self.compare(self.steps(candidate))
                if contract['history']:
                    self.assertGreater(float(actor.history.weight_ih_l0.grad.abs().sum()), 0)
                if contract['current'] == 'geometry':
                    self.assertIsNone(actor.world.messages[0].weight.grad)

    def test_mixed_world_information_and_history_endpoints_stay_independent(self):
        steps = [self.steps(name)[i] for i, name in enumerate(
            ['geometry-current', 'current-best-layout-history', 'action-only'])]
        self.compare(steps)

    def test_no_restore_exhaustion_remains_an_explicit_failure(self):
        env = self.fixture.episode()
        description = env.describe()
        identity = WitnessIdentities()
        memory = SelectionMemory(env.state['current'])
        keys, _, _ = identity.mask(env.state, description, memory, True, 'current-no-restore')
        memory.failed[identity.get(env.state['current'])[0]] = set(keys)
        with self.assertRaisesRegex(ValueError, 'action-exhausted'):
            identity.mask(env.state, description, memory, True, 'current-no-restore')


if __name__ == '__main__':
    unittest.main()
