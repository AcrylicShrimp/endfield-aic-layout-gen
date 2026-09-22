import hashlib
import json
import os
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from hierarchical_curriculum_tracking import committed_metrics, export_pending, observe
from ppo_harness_state import read, write


class HierarchicalCurriculumTrackingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.config = dict(schema='aic-hierarchical-curriculum-v1', generations=1,
                           training_cases=32, evaluation_interval=5, success_window_cases=128,
                           advance_completion_rate=.75, seed=1, namespace='test', max_turns=10,
                           max_extension=4, levels=[dict(min_facilities=1, max_facilities=1,
                                                       max_terminals=4, max_connections=4)])
        self.binding = dict(schema='aic-hierarchical-curriculum-binding-v1',
                            initial_outer_checkpoint_sha256='outer', stable='binding')
        binding_sha = hashlib.sha256(json.dumps(
            self.binding, separators=(',', ':')).encode()).hexdigest()
        self.state = dict(schema='aic-hierarchical-curriculum-state-v3', binding_sha256=binding_sha,
                          completed_generations=1, level=0, recent_training_success=[])
        write(self.root / 'config.json', self.config)
        write(self.root / 'binding.json', self.binding)
        write(self.root / 'state.json', self.state)
        directory = self.root / 'generations' / '000000'
        (directory / 'training').mkdir(parents=True)
        write(directory / 'summary.json', dict(
            schema='aic-hierarchical-curriculum-generation-v2', generation=0, level=0,
            advanced=False, recent_training_cases=32, recent_training_completion_rate=.5,
            training_completed=16, training_cases=32, training_completion_rate=.5,
            training_mean_footprint_area=12., training_mean_transport_cells=3., training_mean_turns=2.,
            training_mcts_samples_per_minute=600., heldout_completed=None, heldout_cases=None,
            heldout_completion_rate=None, heldout_mean_footprint_area=None,
            heldout_mean_transport_cells=None, heldout_mean_turns=None,
            heldout_mcts_samples_per_minute=None, lower_checkpoint_sha256='lower',
            outer_checkpoint_sha256='outer-next'))
        write(directory / 'training' / 'report.json', dict(
            schema='aic-hierarchical-generation-training-report-v3', status='complete',
            root_states=10, outer_action_samples=20, lower_route_samples=5,
            samples_per_minute=1000., elapsed_seconds=2., initial_outer_loss=3., final_outer_loss=1.,
            initial_lower_loss=4., final_lower_loss=2., maximum_gradient_norm=9.))

    def test_exports_only_committed_compact_metrics(self):
        row, summary = committed_metrics(self.root, 0)
        self.assertEqual(row['train/completion_rate'], .5)
        self.assertEqual(row['loss/outer_final'], 1.)
        self.assertNotIn('heldout/completion_rate', row)
        self.assertEqual(summary['outer_checkpoint_sha256'], 'outer-next')
        run = Mock()
        cursor, latest = export_pending(run, self.root, self.state, 0)
        self.assertEqual(cursor, 1)
        self.assertEqual(latest['lower_checkpoint_sha256'], 'lower')
        run.log.assert_called_once_with(row, step=0, commit=True)
        with self.assertRaisesRegex(ValueError, 'ahead'):
            export_pending(run, self.root, self.state, 2)

    def test_complete_run_resumes_and_confirms_remote(self):
        write(self.root / 'report.json', dict(status='complete'))
        run = Mock(step=0, url='https://example.test/run', summary={})
        api = Mock()
        api.run.side_effect = [Mock(lastHistoryStep=-1), Mock(lastHistoryStep=0)]
        sdk = SimpleNamespace(init=Mock(return_value=run), Api=Mock(return_value=api), Settings=Mock())
        observe(self.root, dict(entity='test', project='test'), .01, threading.Event(), sdk)
        run.finish.assert_called_once_with(exit_code=0)
        status = read(self.root / 'wandb-observer.json')
        self.assertEqual(status['status'], 'finished')
        self.assertEqual(status['remote_confirmed_step'], 0)

    def test_secret_is_redacted_on_failure(self):
        sdk = SimpleNamespace(init=Mock(side_effect=RuntimeError('test-secret')), Settings=Mock())
        with patch.dict(os.environ, WANDB_API_KEY='test-secret'):
            with self.assertRaisesRegex(RuntimeError, 'redacted'):
                observe(self.root, dict(entity='test', project='test'), .01, threading.Event(), sdk)
        self.assertNotIn('test-secret', (self.root / 'wandb-observer.json').read_text())


if __name__ == '__main__':
    unittest.main()
