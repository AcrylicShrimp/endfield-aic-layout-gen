"""Registered lower-rate controls and strict model/workload isolation."""
from copy import deepcopy
import json
from pathlib import Path
import unittest

from ppo_harness_state import HarnessError
from run_ppo_combinations import configurations, LOWER_RATES


class LearningRateTests(unittest.TestCase):
    def test_only_learning_rate_changes_and_readiness_preserves_both_rates(self):
        base = json.loads(Path(__file__).with_name('ppo-learning-rate-screen.json').read_text())
        original = deepcopy(base)
        cells = configurations(base, False, learning_rates=True)
        self.assertEqual({c['name']: c['learning_rate'] for c in cells}, LOWER_RATES)
        for cell in cells:
            config = deepcopy(cell['config'])
            self.assertEqual(config['ppo']['learning_rate'], cell['learning_rate'])
            config['ppo']['learning_rate'] = base['ppo']['learning_rate']
            self.assertEqual(config, base)
        self.assertEqual(base, original)
        for cell in configurations(base, True, learning_rates=True):
            config = cell['config']
            self.assertEqual(config['ppo']['learning_rate'], LOWER_RATES[cell['name']])
            self.assertEqual(config['training']['batches'], 2)
            self.assertEqual(config['ppo']['target_kl'], 1e-8)
            self.assertEqual(config['retention']['training_batches'], [1, 2])
        for key, value in (('critic_after_kl', False), ('critic_history', False), ('shared_backbone', False)):
            changed = deepcopy(base)
            changed['model']['actor_critic'][key] = value
            with self.subTest(key=key), self.assertRaises(HarnessError):
                configurations(changed, False, learning_rates=True)
        with self.assertRaises(HarnessError):
            configurations(base, False, shared_capacity=True, learning_rates=True)


if __name__ == '__main__':
    unittest.main()
