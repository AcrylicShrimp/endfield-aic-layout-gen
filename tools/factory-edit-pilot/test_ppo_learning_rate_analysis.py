"""Reject rate-comparison confounds and altered checkpoint learning rates."""
from copy import deepcopy
import io
import unittest

import torch

from compare_ppo_combinations import check_checkpoint_rate
from compare_ppo_learning_rates import workload_controls
from ppo_spatial_model import shared_capacity_specification
from test_ppo_capacity_analysis import protocol


class LearningRateAnalysisTests(unittest.TestCase):
    def test_configuration_allows_only_registered_rate_and_tracking_changes(self):
        old = protocol('/old', shared_capacity_specification(True, True))
        old['config']['ppo']['learning_rate'] = 0.0003
        for rate in (0.0001, 0.00003):
            new = protocol('/new', shared_capacity_specification(True, True))
            new['config']['ppo']['learning_rate'] = rate
            new['sources']['/new/tools/factory-edit-pilot/ppo_harness_tracking.py'] = 'updated-display'
            self.assertTrue(workload_controls(old, new, rate)['only_learning_rate_configuration_differs'])
            for field in ('rate', 'epochs', 'model', 'catalog', 'code'):
                changed = deepcopy(new)
                if field == 'rate':
                    changed['config']['ppo']['learning_rate'] = 0.0003
                elif field == 'epochs':
                    changed['config']['ppo']['epochs'] = 8
                elif field == 'model':
                    changed['config']['model']['actor_critic']['critic_history'] = False
                elif field == 'catalog':
                    changed['sources']['/new/data/items.json'] = 'changed-catalog'
                else:
                    changed['sources']['/new/tools/factory-edit-pilot/ppo_spatial_model.py'] = 'changed-model-code'
                with self.subTest(rate=rate, field=field), self.assertRaises(ValueError):
                    workload_controls(old, changed, rate)

    def test_restored_checkpoint_cannot_override_declared_rate(self):
        saved = dict(protocol_sha256='protocol', job='job', batch=2,
                     actor_optimizer=dict(param_groups=[dict(lr=0.0001)]),
                     critic_optimizer=dict(param_groups=[dict(lr=0.0001)]))
        def payload(value):
            buffer = io.BytesIO()
            torch.save(value, buffer)
            return buffer.getvalue()
        check_checkpoint_rate(payload(saved), 0.0001, 'protocol', 'job', 2)
        for key in ('actor_optimizer', 'critic_optimizer'):
            changed = deepcopy(saved)
            changed[key]['param_groups'][0]['lr'] = 0.0003
            with self.subTest(optimizer=key), self.assertRaisesRegex(ValueError, 'learning rate differs'):
                check_checkpoint_rate(payload(changed), 0.0001, 'protocol', 'job', 2)
        with self.assertRaisesRegex(ValueError, 'checkpoint identity differs'):
            check_checkpoint_rate(payload(saved), 0.0001, 'other-protocol', 'job', 2)


if __name__ == '__main__':
    unittest.main()
