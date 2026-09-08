"""Tree allocation, sparse-return backup and native hypothetical accounting."""

import json
import os
from pathlib import Path
import random
import unittest

import torch

from mastery_policy_selection import read
from native import Runtime
from policy import Policy
from reference.acceptance import Catalog, check
from reference.environment import Environment, serialize
from tree_search import Tree, search
from tree_search_pilot import validate_result


class TreeTests(unittest.TestCase):
    def test_uct_covers_unvisited_then_uses_observed_returns(self):
        tree = Tree(8, 4096)
        rng = random.Random(17)
        first, _ = tree.choose(0, 2, None, 'uct', rng)
        child = tree.add(0, first)
        tree.backup([0, child], 0)
        second, _ = tree.choose(0, 2, None, 'uct', rng)
        self.assertNotEqual(first, second)
        good = tree.add(0, second)
        tree.backup([0, good], 1)
        self.assertEqual(tree.choose(0, 2, None, 'uct', rng)[0], second)
        self.assertEqual(int(tree.visits[0]), 2)

    def test_puct_considers_unexpanded_prior_and_value_of_visited_child(self):
        tree = Tree(8, 4096)
        rng = random.Random(17)
        self.assertEqual(tree.choose(0, 2, [.1, .9], 'puct', rng)[0], 1)
        child = tree.add(0, 0)
        tree.backup([0, child], 1)
        self.assertEqual(tree.choose(0, 2, [.4, .6], 'puct', rng)[0], 0)

    def test_fixed_pool_rejects_oversized_allocation(self):
        with self.assertRaisesRegex(ValueError, 'tree.memory'):
            Tree(1000, 100)
        tree = Tree(1, 4096)
        self.assertIsNone(tree.add(0, 1))
        self.assertLessEqual(tree.bytes, 4096)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'explicit native build required')
class NativeTreeTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parents[2]
        directory = self.root/'tools/factory-edit-pilot'
        self.case = directory/'case.json'
        self.config = {**read(directory/'run.json'), 'horizon': 4}
        self.runtime = Runtime(self.root, self.case, self.config, os.environ['AIC_NATIVE_LIBRARY'])

    def verify(self, result):
        catalog = Catalog(self.root, self.case)
        self.assertEqual(check(catalog, result['best'])['score'], result['best_score'])
        env = Environment(catalog, self.config)
        for row in result['best_trace']:
            env.step(row['command'])
        self.assertEqual(serialize(env.best), result['best'])
        self.assertTrue(result['original_engine_unchanged'])
        self.assertEqual(result['executed_commands'], 0)

    def test_partial_last_rollout_and_path_replays_are_charged(self):
        for method in ('random_rollouts', 'uct'):
            result = search(self.runtime, method, None, 17, 13, 30, 64, 8192)
            self.assertEqual(result['simulated_commands'], 13)
            self.assertEqual(result['counts']['restores'], 4)
            self.assertEqual(result['counts']['completed_rollouts'], 3)
            self.verify(result)

    def test_node_exhaustion_keeps_original_valid_layout(self):
        result = search(self.runtime, 'uct', None, 17, 128, 30, 1, 8192)
        self.assertEqual(result['termination'], 'node_budget')
        self.assertEqual(result['simulated_commands'], 0)
        self.verify(result)

    def test_independent_replay_rejects_invalid_trace_even_with_valid_layout(self):
        result = search(self.runtime, 'uct', None, 17, 13, 30, 64, 8192)
        self.assertEqual(validate_result(self.root, self.case, self.config, result)['status'], 'passed')
        result['best_trace'] = [{'command': {'op': 'not_a_command'}}]
        with self.assertRaisesRegex(ValueError, 'outside independent domain'):
            validate_result(self.root, self.case, self.config, result)

    def test_guided_search_preserves_bn_and_global_rng(self):
        torch.set_num_threads(1)
        example, _ = self.runtime.new_engine().observe()
        actor = Policy(example, normalization='batch').eval()
        before = torch.get_rng_state().clone()
        for method in ('policy_rollouts', 'puct'):
            result = search(self.runtime, method, actor, 17, 9, 30, 64, 8192)
            self.assertEqual(result['simulated_commands'], 9)
            self.assertEqual(result['counts']['model_calls'], 9)
            self.verify(result)
        self.assertTrue(torch.equal(before, torch.get_rng_state()))


if __name__ == '__main__':
    unittest.main()
