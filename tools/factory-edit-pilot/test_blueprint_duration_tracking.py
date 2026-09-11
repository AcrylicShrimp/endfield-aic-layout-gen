"""Verify observer ordering, authenticated backfill, and lifecycle without training."""
import os
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from blueprint_duration_tracking import events, export_pending, metrics, observe, trainer_alive
from ppo_harness_state import read, sha, write


class TrackingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def ref(self, name, value, **extra):
        path = self.root / name
        write(path, value)
        return dict(path=name, sha256=sha(path), **extra)

    def training(self, update):
        return self.ref(f'{update}.json', dict(update=update, actions=256, area_reward=.5, elapsed_seconds=update * 2.,
            collection_seconds=.2, buffer=dict(entries=4, positive_commits=9),
            ppo=dict(actor_epochs=2, success=dict(positive_labels=3, selected_ids=['private']),
                     epochs=[dict(value_loss=.125)])))

    def evaluation(self, update):
        return self.ref(f'eval-{update}.json', dict(update=update, rows=[
            dict(start='original', sample=0, total_area_reduction_percent=0., start_score=[700, 1, 1], best_score=[700, 1, 1], policy_actions=64, counts={}),
            dict(start='overlap', sample=0, total_area_reduction_percent=10., start_score=[700, 1, 1], best_score=[630, 1, 1], policy_actions=64, counts={'status/rejected': 2})]),
            update=update, seconds=12.)

    def state(self, update=2, status='running'):
        return dict(status=status, jobs={'17': dict(update=update,
            records=[self.training(i) for i in range(1, update + 1)], evaluations=[self.evaluation(0)])})

    def test_late_evaluation_preserves_exported_prefix_and_final_flush(self):
        state = self.state()
        first = events(state)
        self.assertEqual([(u, e) for u, e, _ in first], [(0, 1), (1, 0)])
        state['jobs']['17']['evaluations'].append(self.evaluation(2))
        self.assertEqual(events(state), first)
        state['jobs']['17']['records'].append(self.training(3))
        state['jobs']['17']['update'] = 3
        self.assertEqual(events(state)[:len(first)], first)
        self.assertEqual([(u, e) for u, e, _ in events(state)], [(0, 1), (1, 0), (2, 0), (2, 1)])
        state['status'] = 'complete'
        self.assertEqual(events(state)[-1][:2], (3, 0))

    def test_authenticated_metrics_and_resume_cursor(self):
        state, run = self.state(status='complete'), Mock()
        cursor = export_pending(run, self.root, state, 1, 256)
        self.assertEqual(cursor, 3)
        self.assertEqual([c.kwargs['step'] for c in run.log.call_args_list], [1, 2])
        self.assertEqual(run.log.call_args_list[-1].args[0]['train/actions'], 512)
        self.assertEqual(run.log.call_args_list[-1].args[0]['time/elapsed_seconds'], 4.)
        self.assertEqual(run.log.call_args_list[-1].args[0]['train/batches'], 2)
        self.assertEqual(run.log.call_args_list[-1].args[0]['ppo/value_loss'], .125)
        self.assertNotIn('training/mean_area_reward', run.log.call_args_list[-1].args[0])
        self.assertNotIn('success/selected_ids', run.log.call_args_list[-1].args[0])
        baseline = read(self.root / 'eval-0.json')['rows']
        evaluation = metrics(self.root, events(state)[0], 256, baseline)
        self.assertEqual(evaluation['evaluation/all_admitted/by_seconds/area_reduction_percent'], 5.)
        self.assertEqual(evaluation['evaluation/overlap/mean_best_area'], 630)
        with self.assertRaisesRegex(ValueError, 'ahead'):
            export_pending(run, self.root, state, 4, 256)
        state['status'] = 'running'
        self.assertEqual(export_pending(run, self.root, state, 3, 256), 3)
        with self.assertRaisesRegex(ValueError, 'ahead'):
            export_pending(run, self.root, state, 4, 256)
        (self.root / '1.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'evidence identity'):
            export_pending(run, self.root, state, 1, 256)

    def setup_observer(self, status):
        config = dict(seeds=[17], training=dict(streams=8, chunk_actions=32), ppo={},
                      success=dict(coefficient=.05), duration={}, evaluation={}, purpose='test')
        write(self.root / 'protocol.json', dict(schema='blueprint-success-duration-v2', config=config))
        state = self.state(status=status)
        state.update(protocol_sha256=sha(self.root / 'protocol.json'), stop_reason='duration' if status == 'complete' else None,
                     started_at='2026-09-11T08:41:52+00:00', seconds=12.)
        write(self.root / 'state.json', state)
        run = Mock(step=0, url='https://example.test/run', summary={})
        remote = Mock(lastHistoryStep=-1)
        api = Mock()
        api.run.side_effect = [remote, Mock(lastHistoryStep=2)]
        sdk = SimpleNamespace(init=Mock(return_value=run), Api=Mock(return_value=api), Settings=Mock())
        return run, sdk

    def test_complete_finishes_only_after_export_and_confirms_remote(self):
        run, sdk = self.setup_observer('complete')
        observe(self.root, dict(entity='test', project='test'), .01, threading.Event(), sdk)
        run.finish.assert_called_once_with(exit_code=0)
        result = read(self.root / 'wandb-observer.json')
        self.assertEqual(result['remote_confirmed_step'], 2)
        self.assertTrue(run.summary['training_complete'])
        self.assertEqual(result['status'], 'finished')

    def test_dead_trainer_fails_and_redacts_diagnostic(self):
        run, sdk = self.setup_observer('running')
        with patch('blueprint_duration_tracking.trainer_alive', return_value=False):
            with self.assertRaisesRegex(RuntimeError, 'trainer process is absent'):
                observe(self.root, dict(entity='test', project='test'), .01, threading.Event(), sdk)
        run.finish.assert_called_once_with(exit_code=1)
        self.assertEqual(read(self.root / 'wandb-observer.json')['status'], 'failed')
        run, sdk = self.setup_observer('running')
        sdk.init.side_effect = RuntimeError('test-secret')
        with patch.dict(os.environ, WANDB_API_KEY='test-secret'):
            with self.assertRaisesRegex(RuntimeError, 'redacted'):
                observe(self.root, dict(entity='test', project='test'), .01, threading.Event(), sdk)
        self.assertNotIn('test-secret', (self.root / 'wandb-observer.json').read_text())

    def test_live_pid_and_stopped_completion(self):
        write(self.root / 'completion.json', dict(status='running', pid=os.getpid()))
        self.assertTrue(trainer_alive(self.root))
        with patch('blueprint_duration_tracking.os.kill', side_effect=ProcessLookupError):
            self.assertFalse(trainer_alive(self.root))
        write(self.root / 'completion.json', dict(status='failed', pid=os.getpid()))
        self.assertFalse(trainer_alive(self.root))


if __name__ == '__main__':
    unittest.main()
