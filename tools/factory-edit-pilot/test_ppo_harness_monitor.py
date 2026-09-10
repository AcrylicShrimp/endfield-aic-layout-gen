"""Shared-cohort, sparse metric and real W&B commit regression checks."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock

from ppo_harness_comparison import comparisons, tracking_jobs
from ppo_harness_protocol import validate_spec
from ppo_harness_state import HarnessError, read
from ppo_harness_tracking import Tracker, metrics


def evaluation(batch, outcomes):
    results = [dict(status='complete', identity=dict(generation_seed=seed),
                    generation=dict(request_sha256=f'request-{seed}', initial_sha256=f'initial-{seed}'),
                    row=dict(initial_metrics=dict(score=[100]), best_metrics=dict(score=[100 - reduction])))
               for seed, reduction in outcomes.items()]
    return dict(phase=dict(kind='evaluation', batch=batch), results=results,
                selected_training_actions=batch * 32, elapsed_seconds=batch * 2,
                quality=dict(accepted=len(results), attempted=2,
                    mean_area_reduction_percent=sum(outcomes.values()) / len(outcomes) if outcomes else None,
                    total_generation_seconds=1, total_episode_seconds=2),
                paired_to_initial=dict(paired_delta_pp=0, shared_seeds=list(outcomes)),
                phase_seconds=3, collection=dict(seconds=3), selected_per_second=8)


def example():
    protocol = dict(config=dict(candidates=['a', 'b'], seeds=[17], evaluation=dict(at=[0, 1], attempts=2)))
    value = dict(jobs=[dict(id=name, candidate=name, seed=17, status='complete', records=[
        evaluation(0, {1: 0, 2: 0}), evaluation(1, outcomes)]) for name, outcomes in
        [('a', {1: 20, 2: 0}), ('b', {1: 15})]])
    return value, protocol


class ComparisonTests(unittest.TestCase):
    def test_common_cohort_corrects_candidate_ranking_and_keeps_axes(self):
        value, protocol = example()
        self.assertLess(value['jobs'][0]['records'][1]['quality']['mean_area_reduction_percent'],
                        value['jobs'][1]['records'][1]['quality']['mean_area_reduction_percent'])
        points = comparisons(value, protocol)
        self.assertAlmostEqual(points['a'][1]['quality']['mean_area_reduction_percent'], 20)
        self.assertAlmostEqual(points['b'][1]['quality']['mean_area_reduction_percent'], 15)
        self.assertEqual(points['a'][1]['paired_to_initial']['shared_seeds'], [1])
        self.assertEqual(points['a'][1]['selected_training_actions'], 32)
        self.assertEqual(points['a'][1]['elapsed_seconds'], 2)
        self.assertEqual(len(tracking_jobs(value, protocol)[0]['records']), 4)
        provisional = metrics(value['jobs'][0]['records'][1])
        self.assertFalse(any(k.startswith('outcome/') for k in provisional))
        self.assertAlmostEqual(metrics(points['a'][1])['outcome/by_actions/area_reduction_percent'], 20)

    def test_pending_empty_and_changed_factory_are_explicit(self):
        value, protocol = example()
        value['jobs'][1]['status'] = 'running'
        self.assertEqual(comparisons(value, protocol), {})
        value['jobs'][1]['status'] = 'complete'
        value['jobs'][1]['records'][1]['results'][0]['generation']['initial_sha256'] = 'different'
        with self.assertRaisesRegex(HarnessError, 'comparison-reset'):
            comparisons(value, protocol)
        value['jobs'][1]['records'][1] = evaluation(1, {})
        point = comparisons(value, protocol)['a'][1]
        self.assertEqual(point['quality']['accepted'], 0)
        self.assertNotIn('outcome/by_actions/area_reduction_percent', metrics(point))

    def test_external_spec_rejects_invalid_consumed_fields(self):
        spec = read(Path(__file__).with_name('random-factory-server-pilot.json'))
        spec['policy'] = {k: spec['policy'][k] for k in ('depth', 'width')}
        validate_spec(spec)
        invalid_product = deepcopy(spec)
        invalid_product['products'][0]['duration_ms'] = [20000., 100000.]
        with self.assertRaisesRegex(HarnessError, 'config-product-duration'):
            validate_spec(invalid_product)
        for section, key, bad in [('selection', 'temperature', 0), ('selection', 'episode_seconds', -1),
                                   ('selection', 'unknown', 1), ('generation', 'setup_kinds', ['typo']),
                                   ('generation', 'setup_steps', [True, 3])]:
            other = deepcopy(spec)
            other[section][key] = bad
            with self.subTest(field=key), self.assertRaises(HarnessError):
                validate_spec(other)

    def test_terminal_training_does_not_hide_unfinished_tracking(self):
        from ppo_harness_monitor import status
        from ppo_harness_state import write, sha, SCHEMA
        import time
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(root / 'protocol.json', dict(jobs=[], config=dict(tracking=dict(backend='wandb'), limits=dict(run_seconds=100))))
            write(root / 'state.json', dict(schema=SCHEMA, protocol_sha256=sha(root / 'protocol.json'),
                  status='complete', jobs=[], cursor=0, elapsed_seconds=1))
            write(root / 'supervisor.json', dict(updated_epoch=time.time(), elapsed_seconds=1, observer_exit_code=-9,
                  observer_finalization='pending', child_exit_codes=[0,-9]))
            write(root / 'tracking.json', dict(status='running', updated_epoch=time.time()-120))
            value = status(root)
            self.assertEqual(value['status'], 'complete')
            self.assertEqual(value['tracking_health'], 'failed')
            write(root / 'tracking.json', dict(status='complete', updated_epoch=time.time()))
            self.assertEqual(status(root)['tracking_health'], 'complete')

    def test_offline_completed_export_accepts_later_shared_points(self):
        value, protocol = example()
        protocol['config']['tracking'] = dict(backend='wandb', mode='offline')
        with tempfile.TemporaryDirectory() as directory:
            tracker = Tracker(Path(directory), protocol, 'hash', 'session')
            tracker.value['jobs']['a'] = dict(complete=True, export_session='session', exported_records=2)
            # Existing export needs reopening once the shared cohort is known.
            with patch.object(tracker, 'start', side_effect=RuntimeError('restart-required')):
                with self.assertRaisesRegex(RuntimeError, 'restart-required'):
                    tracker.update(tracking_jobs(value, protocol)[:1])

    def online_tracker(self, root):
        from test_ppo_harness_state import config
        cfg = config()
        cfg['tracking'] = dict(backend='wandb', mode='online', entity='team', project='project')
        protocol = dict(config=cfg, spec=dict(policy=dict(width=32, depth=2)))
        return Tracker(root, protocol, 'hash', 'session')

    def test_remote_history_bounds_a_stale_sdk_resume_cursor(self):
        with tempfile.TemporaryDirectory() as directory:
            tracker = self.online_tracker(Path(directory))
            active = Mock(step=0, url='https://example.invalid/run', summary={})
            with patch('wandb.init', return_value=active), patch('wandb.Api') as api:
                api.return_value.run.return_value.lastHistoryStep = 3
                tracker.update([dict(id='job', candidate='action-only', seed=17, status='running', records=[None]*4)])
                self.assertEqual(tracker.next_step, 4)
                active.log.assert_not_called()
                self.assertEqual(tracker.value['jobs']['job']['exported_records'], 4)
                tracker.finish(complete=False)

    def test_remote_history_ahead_of_durable_records_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            tracker = self.online_tracker(Path(directory))
            active = Mock(step=0, url='https://example.invalid/run', summary={})
            with patch('wandb.init', return_value=active), patch('wandb.Api') as api:
                api.return_value.run.return_value.lastHistoryStep = 4
                with self.assertRaisesRegex(HarnessError, 'tracking-ahead'):
                    tracker.update([dict(id='job', candidate='action-only', seed=17, status='running', records=[None]*4)])
                active.log.assert_not_called()
                tracker.finish(complete=False)

    def test_completed_online_cache_rejects_remote_ahead_including_empty_local_state(self):
        for count in (0, 4):
            with self.subTest(records=count), tempfile.TemporaryDirectory() as directory:
                tracker = self.online_tracker(Path(directory))
                tracker.value['jobs']['job'] = dict(complete=True, remote_confirmed_step=4, exported_records=5)
                with patch.object(tracker, 'start') as start:
                    with self.assertRaisesRegex(HarnessError, 'tracking-ahead'):
                        tracker.update([dict(id='job', status='complete', records=[None]*count)])
                    start.assert_not_called()

    def test_completed_online_cache_skips_only_matching_record_count(self):
        with tempfile.TemporaryDirectory() as directory:
            tracker = self.online_tracker(Path(directory))
            tracker.value['jobs']['job'] = dict(complete=True, remote_confirmed_step=3, exported_records=4)
            with patch.object(tracker, 'start', side_effect=RuntimeError('restart-required')) as start:
                tracker.update([dict(id='job', status='complete', records=[None]*4)])
                start.assert_not_called()
                with self.assertRaisesRegex(RuntimeError, 'restart-required'):
                    tracker.update([dict(id='job', status='complete', records=[None]*5)])

    def test_failed_remote_cursor_lookup_closes_the_opened_sdk_run(self):
        with tempfile.TemporaryDirectory() as directory:
            tracker = self.online_tracker(Path(directory))
            active = Mock(step=0, url='https://example.invalid/run', summary={})
            with patch('wandb.init', return_value=active), patch('wandb.Api') as api:
                api.return_value.run.side_effect = RuntimeError('service unavailable')
                with self.assertRaisesRegex(RuntimeError, 'service unavailable'):
                    tracker.start(dict(id='job', candidate='action-only', seed=17, status='running'))
            self.assertIsNone(tracker.active)
            self.assertIsNone(tracker.active_job)
            active.finish.assert_called_once_with(exit_code=1)

    def test_duration_completion_is_successful_without_claiming_schedule_completion(self):
        for failed, reason in [(False, 'duration-reached'), (True, 'error')]:
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as directory:
                tracker = self.online_tracker(Path(directory))
                active = Mock(step=0, url='https://example.invalid/run', summary={})
                with patch('wandb.init', return_value=active), patch('wandb.Api') as api:
                    api.return_value.run.return_value.lastHistoryStep = -1
                    tracker.start(dict(id='job', candidate='action-only', seed=17, status='running'))
                    tracker.finish(complete=False, failed=failed, reason=reason)
                active.finish.assert_called_once_with(exit_code=1 if failed else 0)
                self.assertFalse(active.summary['training_complete'])
                self.assertEqual(active.summary['termination_reason'], reason)
                self.assertEqual(tracker.value['jobs']['job']['finished_successfully'], not failed)

    def test_real_sdk_commits_latest_point_and_specific_axes(self):
        import wandb
        with tempfile.TemporaryDirectory() as directory:
            run = wandb.init(mode='offline', dir=directory, settings=wandb.Settings(console='off', silent=True))
            try:
                run.log({'evaluation/all_admitted/by_batches/area_reduction_percent': 5}, step=0)
                self.assertEqual(run.step, 0)  # SDK default leaves this point pending.
                run.log({'train/batches': 0}, step=0, commit=True)
                self.assertEqual(run.step, 1)
                run.define_metric('evaluation/*', step_metric='train/actions')
                run.define_metric('evaluation/all_admitted/by_batches/*', step_metric='train/batches')
                run.log({'train/batches': 1, 'evaluation/all_admitted/by_batches/area_reduction_percent': 8}, step=1, commit=True)
                self.assertEqual(run.step, 2)
            finally:
                run.finish()


if __name__ == '__main__':
    unittest.main()
