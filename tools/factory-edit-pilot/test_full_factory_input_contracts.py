"""Input isolation, causal history, physical masks and exact group objective tests."""
from copy import deepcopy
import unittest

import numpy as np
import torch

import test_full_factory_policy as physical_fixture
from full_factory_input_contracts import (
    CONTRACTS, InputContractPolicy, history_array, history_record, observation,
    observation_hash, selected_action_token, selection_mask, world_input,
)
from full_factory_input_learning import empty_gradients, accumulate_gradients, group_gradients
from full_factory_on_policy import relative_returns
from full_factory_policy import observe
from full_factory_selection import SelectionMemory, distribution, physical_key


class GradientTests(unittest.TestCase):
    def test_episode_accumulation_matches_direct_group_objective(self):
        torch.manual_seed(7)
        model = torch.nn.Linear(3, 4).double()
        inputs = torch.randn(4, 3, 3, dtype=torch.float64)
        selected = [[0, 2, 1], [3, 1, 0], [1, 1, 2], [2, 0, 3]]
        scores = [[9, 100, 10], [10, 1, 1], [10, 1, 1], [11, 0, 0]]
        coefficient = .001
        episodes = []
        for episode in range(4):
            gradients = dict(log_probability_gradients=empty_gradients(model), entropy_gradients=empty_gradients(model),
                             mean_log_probability=0.0, mean_entropy=0.0)
            for step in range(3):
                dist = torch.distributions.Categorical(logits=model(inputs[episode, step]))
                logp = dist.log_prob(torch.tensor(selected[episode][step]))
                entropy = dist.entropy()
                accumulate_gradients(gradients['log_probability_gradients'], torch.autograd.grad(logp, model.parameters(), retain_graph=True), 1 / 3)
                accumulate_gradients(gradients['entropy_gradients'], torch.autograd.grad(entropy, model.parameters()), 1 / 3)
                gradients['mean_log_probability'] += float(logp.detach()) / 3
                gradients['mean_entropy'] += float(entropy.detach()) / 3
            episodes.append(gradients)
        accounting = group_gradients(model, episodes, scores, coefficient)
        accumulated = [p.grad.clone() for p in model.parameters()]
        advantages = relative_returns(scores)
        loss = torch.tensor(0.0, dtype=torch.float64)
        for episode in range(4):
            for step in range(3):
                dist = torch.distributions.Categorical(logits=model(inputs[episode, step]))
                loss = loss + (-advantages[episode] * dist.log_prob(torch.tensor(selected[episode][step]))
                               - coefficient * dist.entropy()) / 12
        direct = torch.autograd.grad(loss, model.parameters())
        for actual, expected in zip(accumulated, direct):
            torch.testing.assert_close(actual, expected, rtol=1e-12, atol=1e-12)
        self.assertAlmostEqual(accounting['reward_loss'] + accounting['entropy_loss'], float(loss.detach()), places=12)
        flat = group_gradients(model, episodes, [[9, 2, 1]] * 4, coefficient)
        self.assertEqual(flat['reward_gradient_norm'], 0)
        self.assertGreater(flat['entropy_gradient_norm'], 0)


class InputContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        physical_fixture.FactoryPolicyTests.setUpClass()
        cls.fixture = physical_fixture.FactoryPolicyTests()
        cls.runtime = physical_fixture.FactoryPolicyTests.runtime
        torch.set_num_threads(1)

    @classmethod
    def tearDownClass(cls):
        physical_fixture.FactoryPolicyTests.tearDownClass()

    def test_all_contracts_have_finite_distributions_and_identical_parameter_allocation(self):
        env = self.fixture.episode()
        description = env.describe()
        torch.manual_seed(17)
        model = InputContractPolicy()
        hashes = []
        for name in CONTRACTS:
            obs = observation(env.state, description, 1, [], name, self.runtime)
            _, allowed, _ = selection_mask(env.state, description, SelectionMemory(env.state['current']), name)
            dist = distribution(model(obs), description['actions'], allowed, True)
            self.assertTrue(torch.isfinite(dist.probs).all(), name)
            self.assertAlmostEqual(float(dist.probs.sum().detach()), 1, places=6)
            self.assertEqual(len(obs['history']), 0)
            restore = next(i for i, a in enumerate(description['actions']) if a['kind'] == 'restore_best')
            if not CONTRACTS[name]['restore']:
                self.assertEqual(float(dist.probs[restore].detach()), 0)
            hashes.append(observation_hash(obs))
        self.assertEqual(hashes[2], hashes[3])  # Empty history supplies no extra reset information.

    def test_hidden_best_cannot_leak_into_current_only_or_action_only_input(self):
        env = self.fixture.episode()
        env.step(next(a for a in env.describe()['actions'] if a['kind'] == 'insert'))
        description = env.describe()
        changed = deepcopy(env.state)
        changed['best'] = changed['current']
        changed['best_metrics'] = changed['current_metrics']
        for name in ('action-only', 'geometry-current', 'full-current', 'current-history-16', 'current-no-restore'):
            original = observation(env.state, description, .5, [], name, self.runtime)
            alternate = observation(changed, description, .5, [], name, self.runtime)
            self.assertEqual(observation_hash(original), observation_hash(alternate), name)
        self.assertNotEqual(observation_hash(observation(env.state, description, .5, [], 'current-best-layout', self.runtime)),
                            observation_hash(observation(changed, description, .5, [], 'current-best-layout', self.runtime)))

    def test_best_two_omits_turns_but_best_three_exposes_them(self):
        env = self.fixture.episode()
        description = env.describe()
        changed = deepcopy(env.state)
        changed['best_metrics']['score'][2] += 1
        for name, equal in [('current-best-2', True), ('current-best-3', False)]:
            a = observation(env.state, description, 1, [], name, self.runtime)
            b = observation(changed, description, 1, [], name, self.runtime)
            self.assertEqual(observation_hash(a) == observation_hash(b), equal)

    def test_minimal_input_has_no_world_or_node_address_information(self):
        env = self.fixture.episode()
        obs = observation(env.state, env.describe(), 1, [], 'action-only', self.runtime)
        self.assertIsNone(obs['current'])
        self.assertIsNone(obs['best'])
        for key in ('operands', 'member_nodes', 'action_xy', 'member_xy'):
            self.assertFalse(obs[key].any(), key)
        self.assertFalse(obs['context'][4:].any())

    def test_geometry_world_removes_flow_and_graph_messages(self):
        env = self.fixture.episode()
        base = observe(env.state['current'], env.describe(), env.state, 1)
        changed = deepcopy(base)
        changed['nodes'][:, 17:24] += 100
        changed['nodes'][:, 31] = 1 - changed['nodes'][:, 31]
        changed['raster'][3:19] = 1 - changed['raster'][3:19]
        changed['edge_values'] += 100
        a, b = world_input(base, True), world_input(changed, True)
        for key in ('nodes', 'raster', 'edges', 'edge_values'):
            np.testing.assert_array_equal(a[key], b[key])
        self.assertFalse(a['messages'])
        self.assertEqual(len(a['edges']), 0)

    def test_history_is_ordered_bounded_resettable_and_outcomes_are_optional(self):
        history = []
        step = dict(status='executor_rejected', unchanged=True, current_score=[10, 2, 1])
        for i in range(20):
            token = np.full(42, i, np.float32)
            before = history_array(history, CONTRACTS['current-history-16-outcomes'])
            self.assertFalse(len(before) and before[-1, 0] == i)
            history.append(history_record(token, [10, 2, 1], step, [10, 2, 1]))
        for length, name in [(4, 'current-history-4'), (16, 'current-history-16')]:
            values = history_array(history, CONTRACTS[name])
            self.assertEqual(values[:, 0].tolist(), list(range(20 - length, 20)))
            self.assertFalse(values[:, 43:].any())
            self.assertEqual(history_array([], CONTRACTS[name]).shape, (0, 53))
        values = history_array(history, CONTRACTS['current-history-16-outcomes'])
        self.assertTrue(values[:, 43:].any())
        self.assertEqual(values[-1, 42], 1 / 16)

    def test_best_layout_reuses_one_encoder_and_encodes_distinct_incumbents(self):
        env = self.fixture.episode()
        model = InputContractPolicy()
        calls = []
        handle = model.world.register_forward_hook(lambda *args: calls.append(1))
        try:
            obs = observation(env.state, env.describe(), 1, [], 'current-best-layout', self.runtime)
            self.assertTrue(obs['best_is_current'])
            model(obs)
            self.assertEqual(len(calls), 1)
            env.step(next(a for a in env.describe()['actions'] if a['kind'] == 'insert'))
            obs = observation(env.state, env.describe(), .5, [], 'current-best-layout', self.runtime)
            self.assertFalse(obs['best_is_current'])
            model(obs)
            self.assertEqual(len(calls), 3)
        finally:
            handle.remove()

    def test_full_history_window_receives_policy_gradients_without_mutating_records(self):
        env = self.fixture.episode()
        description = env.describe()
        base = observation(env.state, description, 1, [], 'full-current', self.runtime)
        token = selected_action_token(base, 0)
        history = [history_record(token, [10, 2, 1],
                   dict(status='executor_rejected', unchanged=True, current_score=[10, 2, 1]), [10, 2, 1])
                   for _ in range(20)]
        original = deepcopy(history)
        for name in ('current-history-4', 'current-history-16', 'current-history-16-outcomes', 'current-best-layout-history'):
            torch.manual_seed(17)
            model = InputContractPolicy()
            obs = observation(env.state, description, .25, history, name, self.runtime)
            self.assertEqual(len(obs['history']), CONTRACTS[name]['history'])
            loss = -torch.log_softmax(model(obs), dim=0)[0]
            loss.backward()
            self.assertGreater(float(model.history.weight_ih_l0.grad.abs().sum()), 0, name)
            self.assertTrue(torch.isfinite(model.history_input[0].weight.grad).all(), name)
        for a, b in zip(history, original):
            np.testing.assert_array_equal(a['action'], b['action'])
            np.testing.assert_array_equal(a['outcome'], b['outcome'])

    def test_no_restore_exhaustion_never_reenables_restore(self):
        env = self.fixture.episode()
        description = env.describe()
        memory = SelectionMemory(env.state['current'])
        keys, _, _ = selection_mask(env.state, description, memory, 'current-no-restore')
        memory.failed[physical_key(env.state['current'])] = set(keys)
        with self.assertRaisesRegex(ValueError, 'action-exhausted'):
            selection_mask(env.state, description, memory, 'current-no-restore')


if __name__ == '__main__':
    unittest.main()
