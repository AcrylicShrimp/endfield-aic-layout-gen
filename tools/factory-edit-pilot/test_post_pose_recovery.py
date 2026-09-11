"""Check source-state reconstruction and explicit counterfactual budget forks."""
import json
import os
import tempfile
import unittest

from audit_blueprint_ppo import WitnessAudit
from blueprint_ppo_environment import Runtime, semantic
from blueprint_ppo_store import ROOT, read
from post_pose_recovery import advance, branch, geometry, restore_verified


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires selected native library')
class PostPoseRecoveryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.training = read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo.json')
        cls.config = read(ROOT / 'tools/factory-edit-pilot/blueprint-post-pose-recovery.json')
        cls.runtime = Runtime(cls.training, os.environ['AIC_NATIVE_LIBRARY'])

    def test_reconstruction_updates_best_only_at_improving_commit(self):
        original = next(s for s in self.training['starts'] if s['name'] == 'original')
        improved = next(s for s in self.training['starts'] if s['name'] == 'improved')
        overlap = next(s for s in self.training['starts'] if s['name'] == 'overlap')
        session, _, _ = self.runtime.open(original, 64, 60000)
        saved = json.loads(session.checkpoint_json())
        initial_best = saved['best_draft']
        for index, action in enumerate(improved['prefix'] + overlap['prefix']):
            observation = json.loads(session.observation_json())
            outcome = self.runtime.step(session, observation, action)
            after = json.loads(session.observation_json())
            saved = advance(saved, dict(outcome=outcome, after_semantic=semantic(after)), after)
            restored = restore_verified(self.runtime, saved, after)
            self.assertEqual(json.loads(restored.checkpoint_json())['best_draft'], saved['best_draft'])
            if index < 2:
                self.assertEqual(saved['best_draft'], initial_best)
        self.assertNotEqual(saved['best_draft'], initial_best)
        self.assertTrue(geometry(after)['conflicts'])
        self.assertNotEqual(saved['best_draft'], saved['draft'])

    def test_terminal_post_pose_can_be_probed_without_mutating_source(self):
        ready = next(s for s in self.training['starts'] if s['name'] == 'repair_ready')
        session, _, _ = self.runtime.open(ready, 0, 60000)
        observation = json.loads(session.observation_json())
        self.assertTrue(observation['done'])
        original_semantic = semantic(observation)
        saved = json.loads(session.checkpoint_json())
        with tempfile.TemporaryDirectory() as directory:
            checker = WitnessAudit(self.runtime, directory)
            result = branch(self.runtime, saved, observation, 'original', observation['config']['route_expansions'],
                            observation['config']['search_padding'], self.config, checker, lambda: None)
        self.assertEqual(result['status'], 'validated')
        self.assertTrue(result['improves_area'])
        self.assertLessEqual(result['expansions'], result['work_allowance'])
        self.assertEqual(semantic(json.loads(session.observation_json())), original_semantic)
        self.assertEqual(saved['attempts'], saved['config']['max_actions'])


if __name__ == '__main__':
    unittest.main()
