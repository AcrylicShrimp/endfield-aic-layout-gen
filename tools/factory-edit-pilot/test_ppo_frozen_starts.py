"""Frozen cohorts must preserve input bytes, schedule coverage and owned copies."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

from ppo_frozen_starts import load, materialize
from ppo_harness_state import HarnessError, sha, write, read


class FrozenStartsTests(unittest.TestCase):
    def fixture(self, root):
        config = dict(seeds=[0], training=dict(batches=1, attempts=1, generation_seed_base=10),
                      evaluation=dict(attempts=1, generation_seed_base=20))
        files = {}
        for key in ('request', 'initial', 'original'):
            path = root / f'{key}.json'
            write(path, dict(kind=key))
            files[key] = dict(path=str(path), sha256=sha(path))
        accepted = dict(generation_seed=10, status='accepted', files=files, baseline_result=dict(path='baseline', sha256='source'),
                        generation=dict(generation_seed=10, status='accepted', seconds=1., admission_seconds=.5, setup_seconds=.5,
                            request_sha256=files['request']['sha256'], initial_sha256=files['initial']['sha256'],
                            initial_metrics=dict(score=[100, 20, 3]), original_attempt=dict(directory='baseline'),
                            case=dict(dialect='closure-factory', instances=1)))
        rejected = dict(generation_seed=20, status='rejected', files={}, baseline_result=dict(path='baseline', sha256='source'),
                        generation=dict(generation_seed=20, status='rejected', seconds=.2, admission_status='rejected',
                                        original_attempt=dict(directory='baseline')))
        manifest = root / 'manifest.json'
        write(manifest, dict(schema='ppo-frozen-starts-v1', entries={'10': accepted, '20': rejected}))
        frozen, paths = load(manifest, config)
        spec = dict(frozen_starts=frozen, generation=dict(setup_seconds=120), selection=dict(route_work=5000))
        return config, manifest, spec, paths

    def test_owned_materialization_preserves_baseline_bytes_and_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            _, _, spec, paths = self.fixture(root)
            runtime = Mock(metadata=dict(backend='retained-full-factory-native', provenance='native'))
            runtime.open_episode.return_value.summary.return_value = dict(initial_metrics=dict(score=[100, 20, 3]))
            case, generation = materialize(runtime, spec, 10, root / 'accepted', lambda: None)
            for key in ('request', 'initial', 'original'):
                self.assertEqual(Path(case[key]).parent, root / 'accepted')
                self.assertEqual(sha(case[key]), sha(root / f'{key}.json'))
            self.assertTrue(read(root / 'accepted/reference.json')['factory_validated'])
            self.assertEqual(generation['initial_sha256'], sha(case['initial']))
            self.assertEqual(Path(generation['original_attempt']['directory']).parent, root / 'accepted')
            self.assertEqual(len(paths), 4)
            other, rejected = materialize(runtime, spec, 20, root / 'rejected', lambda: None)
            self.assertIsNone(other)
            self.assertEqual(rejected['status'], 'rejected')
            runtime.open_episode.assert_called_once()

    def test_missing_seed_and_baseline_hash_mismatch_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            config, manifest, _, _ = self.fixture(root)
            config['evaluation']['attempts'] = 2
            with self.assertRaisesRegex(HarnessError, 'frozen-start-missing'):
                load(manifest, config)
            config['evaluation']['attempts'] = 1
            value = read(manifest)
            value['entries']['10']['generation']['initial_sha256'] = 'different'
            write(manifest, value)
            with self.assertRaisesRegex(HarnessError, 'frozen-start-baseline-hash'):
                load(manifest, config)

    def test_source_drift_after_preparation_cannot_enter_training(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            _, _, spec, _ = self.fixture(root)
            write(root / 'initial.json', dict(changed=True))
            runtime = Mock()
            with self.assertRaisesRegex(HarnessError, 'frozen-start-hash'):
                materialize(runtime, spec, 10, root / 'attempt', lambda: None)
            runtime.open_episode.assert_not_called()


if __name__ == '__main__':
    unittest.main()
