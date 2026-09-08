"""Historical replay admission and field-level evidence comparisons."""

from copy import deepcopy
import json
from pathlib import Path
import unittest

import numpy as np
import torch

from checkpoint_update_replay import compare_record, cpu_state, require_protocol
from native_training_comparison import equal


class CheckpointReplayTests(unittest.TestCase):
    def test_fixed_interval_rejects_changed_exposure_or_sampler_contract(self):
        spec = json.loads((Path(__file__).with_name('checkpoint-update-replay.json')).read_text())
        require_protocol(spec)
        for key, value in [('updates', 31), ('assessment_stride', 8), ('end_commands', 1211392),
                           ('device', 'cpu'), ('historical_sources', ['policy.py'])]:
            with self.subTest(key=key):
                changed = {**spec, key: value}
                with self.assertRaisesRegex(ValueError, 'checkpoint_replay.protocol'):
                    require_protocol(changed)

    def test_state_comparison_preserves_nested_rng_and_finds_actual_changed_field(self):
        reference = {'policy': {'x': torch.tensor([1., 2.], requires_grad=True)},
                     'rng': np.random.default_rng(17).bit_generator.state,
                     'states': [torch.arange(3), (torch.ones(2), None)]}
        actual = cpu_state(reference)
        self.assertTrue(equal(actual, reference))
        self.assertFalse(actual['policy']['x'].requires_grad)
        altered = deepcopy(actual)
        altered['policy']['x'][1] = 3
        differences = compare_record(altered, actual)
        self.assertEqual(set(differences), {'policy'})
        self.assertTrue(equal(differences['policy']['expected'], actual['policy']))
        expected_episode = {'commands': 16, 'return': 0.0, 'score': [133, 14, 1], 'counts': {'commands': 16}}
        changed = {**expected_episode, 'score': [119, 12, 1]}
        self.assertEqual(set(compare_record(changed, expected_episode)), {'score'})
        self.assertEqual(compare_record(expected_episode, deepcopy(expected_episode)), {})


if __name__ == '__main__':
    unittest.main()
