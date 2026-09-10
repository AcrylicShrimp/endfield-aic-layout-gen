"""Verify skipped, ordinary and aborted updates on complete native factories."""
from copy import deepcopy
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import torch

from full_factory_counterfactual import identical
from full_factory_environment import ROOT
from full_factory_large_learning import read
from full_factory_on_policy import on_policy_rollout, parameter_hash
from full_factory_policy import FullFactoryPolicy
from full_factory_stability_update import CONTRACT, checkpoint_value, update_group
import test_full_factory_policy as fixtures
from test_full_factory_on_policy import KindPolicy


def snapshot(model, optimizer):
    return deepcopy(dict(policy=model.state_dict(), optimizer=optimizer.state_dict(), rng=torch.get_rng_state(),
                         gradients=[v.grad for v in model.parameters()]))


class CheckpointTests(unittest.TestCase):
    def test_collection_and_optimizer_counters_are_distinct_and_snapshot_is_owned(self):
        model = FullFactoryPolicy(1, 8)
        optimizer = torch.optim.Adam(model.parameters(), lr=.0003)
        arm = dict(name='skip-flat', learning_rate=.0003, skip_flat=True)
        protocol = dict(depth=1, width=8)
        initial = checkpoint_value(protocol, 'a'*64, arm, model, optimizer, 2, 0)
        self.assertEqual(initial['groups_collected'], 2)
        self.assertEqual(initial['optimizer_updates'], 0)
        self.assertEqual(initial['optimizer']['state'], {})
        for value in model.parameters():
            value.grad = torch.ones_like(value)
        optimizer.step()
        saved = checkpoint_value(protocol, 'a'*64, arm, model, optimizer, 3, 1)
        frozen = deepcopy(saved)
        self.assertEqual(saved['learning_contract'], CONTRACT)
        with self.assertRaisesRegex(ValueError, 'state count'):
            checkpoint_value(protocol, 'a'*64, arm, model, optimizer, 3, 0)
        with self.assertRaisesRegex(ValueError, 'step disagrees'):
            checkpoint_value(protocol, 'a'*64, arm, model, optimizer, 3, 2)
        optimizer.step()
        self.assertTrue(identical(saved, frozen))
        self.assertNotEqual(parameter_hash(model), saved['policy_sha256'])

    def test_incomplete_group_cannot_mutate_optimizer_or_policy(self):
        model = FullFactoryPolicy(1, 8)
        optimizer = torch.optim.Adam(model.parameters(), lr=.0003)
        before = snapshot(model, optimizer)
        with self.assertRaisesRegex(ValueError, 'incomplete episode group'):
            update_group(dict(config=dict(group_episodes=4)), dict(id='case'),
                         dict(learning_rate=.0003, skip_flat=True), model, optimizer, [])
        self.assertTrue(identical(before, snapshot(model, optimizer)))


class NativeStabilityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixtures.FactoryPolicyTests.setUpClass.__func__(cls)

    @classmethod
    def tearDownClass(cls):
        fixtures.FactoryPolicyTests.tearDownClass.__func__(cls)

    def context(self, horizon):
        config = read(Path(__file__).with_name('full-factory-on-policy-study.json'))
        config.update(group_episodes=4, horizon=horizon, episode_seconds=30)
        protocol = dict(config=config, catalogs=str(Path(self.temp.name)/'catalogs.json'),
                        native_library=os.environ.get('AIC_NATIVE_LIBRARY', str(ROOT/'target/release/libaic_pilot_native.dylib')))
        case = dict(self.case, instances=len(self.witness['placements']), family='test', split='train', transfer_group='stability-test')
        return protocol, case

    def collect(self, protocol, case, model, directory):
        rows = [on_policy_rollout(protocol, case, model, seed, Path(directory)/str(seed))[0] for seed in (31, 47, 61, 83)]
        self.assertTrue(all(r['budget_complete'] for r in rows))
        return rows

    def flat_source(self):
        model = KindPolicy()
        with torch.no_grad():
            model.preferences.fill_(-1000)
            model.preferences[5] = 0  # Insert and restore cannot improve the initial best in one action.
            model.preferences[7] = 1
        optimizer = torch.optim.Adam(model.parameters(), lr=.0003)
        model.preferences.grad = torch.ones_like(model.preferences)
        optimizer.step()
        return model, optimizer

    def test_native_flat_skip_preserves_inherited_moments_ordinary_still_updates(self):
        protocol, case = self.context(1)
        model, optimizer = self.flat_source()
        before = snapshot(model, optimizer)
        with tempfile.TemporaryDirectory() as temporary:
            rows = self.collect(protocol, case, model, temporary)
            self.assertEqual(len({tuple(r['best_metrics']['score']) for r in rows}), 1)
            with patch('full_factory_stability_update.replay_gradients', side_effect=AssertionError('skip must not replay')):
                skipped = update_group(protocol, case, dict(learning_rate=.0003, skip_flat=True), model, optimizer, rows)
            self.assertTrue(skipped['flat_returns'])
            self.assertFalse(skipped['optimizer_step_applied'])
            self.assertIsNone(skipped['entropy_gradient_norm'])
            self.assertEqual(skipped['replayed_actions'], 0)
            self.assertTrue(identical(before, snapshot(model, optimizer)))
            ordinary = update_group(protocol, case, dict(learning_rate=.0003, skip_flat=False), model, optimizer, rows)
            self.assertTrue(ordinary['flat_returns'])
            self.assertTrue(ordinary['optimizer_step_applied'])
            self.assertEqual(ordinary['reward_gradient_norm'], 0)
            self.assertGreater(ordinary['entropy_gradient_norm'], 0)
            self.assertEqual(float(optimizer.state[model.preferences]['step']), 2)
            self.assertNotEqual(ordinary['policy_before_sha256'], ordinary['policy_after_sha256'])

    def test_nonflat_group_uses_same_update_with_or_without_skip_option(self):
        protocol, case = self.context(4)
        behavior = KindPolicy()
        with tempfile.TemporaryDirectory() as temporary:
            rows = self.collect(protocol, case, behavior, temporary)
            self.assertGreater(len({tuple(r['best_metrics']['score']) for r in rows}), 1)
            results = []
            for skip in (False, True):
                model = KindPolicy()
                model.load_state_dict(behavior.state_dict())
                optimizer = torch.optim.Adam(model.parameters(), lr=.0003)
                result = update_group(protocol, case, dict(learning_rate=.0003, skip_flat=skip), model, optimizer, rows)
                self.assertFalse(result['flat_returns'])
                self.assertTrue(result['optimizer_step_applied'])
                self.assertGreater(result['reward_gradient_norm'], 0)
                results.append((deepcopy(model.state_dict()), deepcopy(optimizer.state_dict()), result))
            self.assertTrue(identical(results[0], results[1]))

    def test_deadline_after_adam_step_rolls_back_model_optimizer_rng_and_gradients(self):
        protocol, case = self.context(1)
        model, optimizer = self.flat_source()
        with tempfile.TemporaryDirectory() as temporary:
            rows = self.collect(protocol, case, model, temporary)
            before = snapshot(model, optimizer)
            original = optimizer.step
            stepped = False

            def step(*args, **kwargs):
                nonlocal stepped
                result = original(*args, **kwargs)
                torch.rand(1)
                stepped = True
                return result

            def guard():
                if stepped:
                    raise TimeoutError('test deadline after Adam')

            with patch.object(optimizer, 'step', side_effect=step):
                with self.assertRaisesRegex(TimeoutError, 'after Adam'):
                    update_group(protocol, case, dict(learning_rate=.0003, skip_flat=False), model, optimizer, rows, guard)
            self.assertTrue(stepped)
            self.assertTrue(identical(before, snapshot(model, optimizer)))

    def test_flat_skip_rejects_changed_behavior_policy_before_any_mutation(self):
        protocol, case = self.context(1)
        model, optimizer = self.flat_source()
        with tempfile.TemporaryDirectory() as temporary:
            rows = self.collect(protocol, case, model, temporary)
            with torch.no_grad():
                model.preferences[5] += .1
            before = snapshot(model, optimizer)
            with self.assertRaisesRegex(ValueError, 'behavior policy'):
                update_group(protocol, case, dict(learning_rate=.0003, skip_flat=True), model, optimizer, rows)
            self.assertTrue(identical(before, snapshot(model, optimizer)))


if __name__ == '__main__':
    unittest.main()
