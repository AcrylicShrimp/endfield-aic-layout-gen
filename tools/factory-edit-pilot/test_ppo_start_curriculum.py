"""Curriculum quota, ancestry, retention and fresh-trajectory input tests."""
from copy import deepcopy
import gzip
import json
from pathlib import Path
import random
import tempfile
import unittest
from unittest.mock import Mock

from full_factory_environment import digest
from ppo_episode_metrics import episode_metrics, curriculum_metrics
from ppo_harness_schedule import cleanup_phase, phase_tasks
from ppo_harness_state import HarnessError, read, write, sha, manifest, checked_path
from ppo_start_curriculum import SCHEMA, choose, quotas, save_next_plan, materialize, validate


CONFIG = dict(schema=SCHEMA, percentages=[80, 10, 10], selection_seed=12345)


def donor(batch, index, depth):
    return dict(identity=dict(job='job', phase='training', batch=batch, index=index), status='complete',
                generation=dict(start_curriculum=dict(depth=depth)), row=dict(budget_complete=True))


class CurriculumTests(unittest.TestCase):
    def test_mature_quotas_and_two_pass_lag(self):
        for n in (32, 64, 128):
            for first in range(3, 30):
                total = {depth: sum(quotas(n, b)[depth] for b in range(first, first + 5)) for depth in range(3)}
                self.assertEqual(total, {0: 4 * n, 1: n // 2, 2: n // 2})
                self.assertEqual(quotas(n, first)[2], quotas(n, first - 1)[1])

    def test_bootstrap_and_shortage_are_explicit_and_never_reuse_twice_optimized_parents(self):
        previous = []
        for b in range(1, 9):
            plan = choose(CONFIG, 'job', b, 64, previous)
            depths = [s['depth'] for s in plan['slots']]
            if b == 1:
                self.assertEqual(set(depths), {0})
            elif b == 2:
                self.assertEqual((depths.count(0), depths.count(1), depths.count(2)), (58, 6, 0))
            else:
                self.assertEqual({d: depths.count(d) for d in range(3)}, quotas(64, b))
            for slot in plan['slots']:
                if slot['depth']:
                    self.assertEqual(slot['donor']['generation']['start_curriculum']['depth'], slot['depth'] - 1)
            previous = [donor(b, i, d) for i, d in enumerate(depths)]
        plan = choose(CONFIG, 'job', 9, 64, [donor(8, 0, 2)])
        self.assertEqual(plan['actual_counts'], {'0': 64})
        self.assertTrue(all(d['code'] == 'ppo-harness.curriculum-donor-shortage' for d in plan['diagnostics']))

    def test_selection_ignores_scores_arrival_order_and_global_rng(self):
        rows = [donor(5, i, 0 if i < 52 else 1) for i in range(64)]
        before = random.getstate()
        first = choose(CONFIG, 'job', 6, 64, rows)
        self.assertEqual(random.getstate(), before)
        changed = deepcopy(rows[::-1])
        for i, row in enumerate(changed):
            row['row']['best_metrics'] = dict(score=[i, 0, 0])
        second = choose(CONFIG, 'job', 6, 64, changed)
        signature = lambda plan: [(s['depth'], s.get('donor', {}).get('identity')) for s in plan['slots']]
        self.assertEqual(signature(first), signature(second))
        parents = [s['donor']['identity']['index'] for s in first['slots'] if s['depth']]
        self.assertEqual(len(parents), len(set(parents)))
        for field, value in [('job', 'other'), ('phase', 'evaluation'), ('batch', 4)]:
            invalid = deepcopy(rows)
            invalid[0]['identity'][field] = value
            with self.assertRaisesRegex(HarnessError, 'curriculum-parent'):
                choose(CONFIG, 'job', 6, 64, invalid)

    def fixture(self, root, batch=1):
        rows = []
        phase = root / 'job' / f'training-{batch:04d}'
        for index in range(10):
            attempt = phase / f'episode-{index:04d}' / 'attempt-0000'
            generated, episode = attempt / 'generated', attempt / 'episode'
            generated.mkdir(parents=True)
            (generated / 'admission').mkdir()
            episode.mkdir()
            write(generated / 'request.json', {'instances': [{'instance': str(index)}]})
            write(generated / 'original.json', {'obligation': index})
            best = dict(factory=index, area=80)
            metrics = dict(score=[80, 0, 0])
            (episode / 'best.json.gz').write_bytes(gzip.compress(json.dumps(best).encode()))
            write(attempt / 'task.json', dict(training=True))
            row = donor(batch, index, 0)
            row['generation'].update(case=dict(request=str(generated / 'request.json'), original=str(generated / 'original.json'),
                files_sha256={name: sha(generated / name) for name in ('request.json', 'original.json')}),
                request_sha256=sha(generated / 'request.json'), draw=dict(product='synthetic'), constructor_geometry_sha256='constructor',
                original_attempt=dict(directory=str(generated / 'admission')))
            row['generation']['start_curriculum']['root'] = dict(batch=batch, generation_seed=index, initial_area=100,
                initial_sha256='root-input', request_sha256=row['generation']['request_sha256'])
            row['row'].update(directory=str(episode), best_metrics=metrics, best_sha256=digest(best))
            row['policy_sha256'] = 'old-policy'
            write(attempt / 'result.json', row)
            row.update(result_path=str(attempt / 'result.json'), result_sha256=sha(attempt / 'result.json'))
            rows.append(row)
        config = dict(start_curriculum=CONFIG, retention=dict(training_batches=[]), training=dict(attempts=10, horizon=32,
                      generation_seed_base=100, action_seed_base=200))
        reference = save_next_plan(root, phase / 'starts-session', config, 'job', batch, rows)
        record = phase / 'record.json'
        write(record, dict(next_training_starts=reference))
        job = dict(id='job', seed=17, candidate='current-history-16', records=[dict(**manifest(root, record),
                   phase=dict(kind='training', batch=batch))], checkpoint=dict(path='current-policy.pt', sha256='checkpoint',
                   policy_sha256='current-actor', critic_sha256='current-critic'))
        return phase, config, reference, job, rows

    def test_cleanup_preserves_donors_and_resume_reconstructs_current_policy_tasks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            phase, config, reference, job, rows = self.fixture(root)
            cleanup_phase(root, phase, rows, retain_training=False)
            self.assertFalse(Path(rows[0]['row']['directory']).exists())
            plan = read(checked_path(root, reference))
            inherited = next(s for s in plan['slots'] if s['depth'])
            for ref in inherited['files'].values():
                self.assertTrue(checked_path(root, ref).is_file())
            _, tasks, _ = phase_tasks(root, dict(config=config), 'protocol', job, dict(kind='training', batch=2))
            before = [read(path) for _, path in tasks]
            _, retried, _ = phase_tasks(root, dict(config=config), 'protocol', job, dict(kind='training', batch=2))
            self.assertEqual(before, [read(path) for _, path in retried])
            for task in before:
                self.assertEqual(task['policy_sha256'], 'current-actor')
                self.assertEqual(task['critic_sha256'], 'current-critic')
            path = checked_path(root, reference)
            path.write_text('{}')
            with self.assertRaisesRegex(HarnessError, 'artifact-identity'):
                phase_tasks(root, dict(config=config), 'protocol', job, dict(kind='training', batch=2))

    def test_materialization_readmits_parent_best_and_rejects_corruption(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            _, config, reference, job, _ = self.fixture(root)
            _, tasks, _ = phase_tasks(root, dict(config=config), 'protocol', job, dict(kind='training', batch=2))
            task = next(read(path) for _, path in tasks if read(path)['initial_start']['depth'])
            start = task['initial_start']
            best_path = checked_path(root, start['files']['best'])
            witness = json.loads(gzip.decompress(best_path.read_bytes()))
            runtime = Mock(metadata=dict(backend='retained-full-factory-native'))
            runtime.open_episode.return_value.summary.return_value = dict(initial_metrics=start['expected_metrics'])
            runtime.open_episode.return_value.export_layout.return_value = json.dumps(witness)
            spec = dict(generation=dict(setup_seconds=120), selection=dict(route_work=50000))
            case, generation = materialize(root, runtime, spec, task, root / 'materialized', lambda: None)
            self.assertEqual(read(case['initial']), witness)
            self.assertEqual(generation['initial_metrics']['score'][0], 80)
            self.assertEqual(generation['start_curriculum']['root']['initial_area'], 100)
            self.assertEqual(generation['generation_seed'], task['generation_seed'])
            runtime.open_episode.return_value.summary.return_value = dict(initial_metrics=dict(score=[81, 0, 0]))
            with self.assertRaisesRegex(HarnessError, 'curriculum-admission'):
                materialize(root, runtime, spec, task, root / 'bad-metrics', lambda: None)
            best_path.write_bytes(b'corrupt')
            with self.assertRaisesRegex(HarnessError, 'artifact-identity'):
                materialize(root, runtime, spec, task, root / 'bad-hash', lambda: None)
            best_path.unlink()
            with self.assertRaisesRegex(HarnessError, 'curriculum-missing'):
                materialize(root, runtime, spec, task, root / 'missing-input', lambda: None)

    def test_configuration_rejects_implicit_or_unsupported_mixtures(self):
        validate(CONFIG)
        for value in [{}, dict(CONFIG, percentages=[80, 20, 0]), dict(CONFIG, selection_seed=True), dict(CONFIG, surprise=1)]:
            with self.assertRaisesRegex(HarnessError, 'curriculum-config'):
                validate(value)

    def test_group_metrics_use_new_start_for_reward_and_root_for_cumulative_progress(self):
        trace = [dict(action=dict(kind='reroute'), area_reward=0., status='accepted_equal', physical_unchanged=True)
                 for _ in range(10)]
        trace[0].update(action=dict(kind='pose'), area_reward=.1, status='improved', physical_unchanged=False)
        trace[9].update(action=dict(kind='pose'), area_reward=.1, status='improved', physical_unchanged=False)
        row = dict(selected=10, area_reward_sum=.2, failure_cost_sum=0., failed_actions=0,
                   initial_metrics=dict(score=[50, 0, 0]), best_metrics=dict(score=[40, 0, 0]), **episode_metrics(trace))
        results = [dict(status='complete', row=row,
                       generation=dict(start_curriculum=dict(depth=2, requested_depth=2, root=dict(initial_area=100))))]
        group = curriculum_metrics(results)['start_depths']['2']
        self.assertAlmostEqual(group['mean_area_reduction_percent'], 20)
        self.assertAlmostEqual(group['mean_cumulative_area_reduction_percent'], 60)
        self.assertAlmostEqual(group['mean_first_eight_area_reduction_percent'], 10)
        self.assertAlmostEqual(group['mean_later_area_reduction_percent'], 10)
        self.assertAlmostEqual(group['accepted_physical_unchanged_fraction'], .8)


if __name__ == '__main__':
    unittest.main()
