"""Duration boundaries and lossless cache/storage semantics."""
from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest

from blueprint_duration_store import SequenceCache, boundary_stop, exclusive_run, retained_updates, save_compact
from blueprint_ppo_store import load
from blueprint_ppo_update import digest
from blueprint_success_buffer import SuccessBuffer


class DurationTests(unittest.TestCase):
    def test_only_one_trainer_can_own_an_output_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            with exclusive_run(output):
                with self.assertRaisesRegex(ValueError, 'another trainer owns'):
                    with exclusive_run(output):
                        self.fail('concurrent trainer acquired the lock')
            with exclusive_run(output):
                pass

    def test_deadline_finishes_live_episode_and_distinguishes_stop_reasons(self):
        live = [SimpleNamespace(session=object())]
        idle = [SimpleNamespace(session=None)]
        self.assertIsNone(boundary_stop(12, 10, live, 1, 100))
        self.assertIsNone(boundary_stop(9, 10, idle, 2, 100))
        self.assertEqual(boundary_stop(10, 10, idle, 2, 100), 'duration')
        self.assertEqual(boundary_stop(9, 10, idle, 2, 100, True), 'requested_stop')
        self.assertEqual(boundary_stop(9, 10, idle, 100, 100), 'update_limit')

    def test_cached_sequences_equal_original_and_rebuild_from_durable_buffer(self):
        class Source:
            def __init__(self):
                self.calls = 0
            def sample(self, ref):
                self.calls += 1
                return dict(input=dict(reference=ref, feature=[1., 2.]), index=0)
        buffer = SuccessBuffer()
        for identity, source, admission in [('z', (1, 0), 2), ('a', (3, 0), 4)]:
            buffer.state['entries'][identity] = dict(id=identity, admitted_update=admission,
                labels=[dict(reference=source, stage='pose', return_to_terminal=.1)])
        evidence, cache = Source(), SequenceCache()
        expected = buffer.sequences(evidence, 5)
        self.assertEqual(digest(expected), digest(cache.resolve(buffer, evidence, 5)))
        calls = evidence.calls
        self.assertEqual(digest(expected), digest(cache.resolve(buffer, evidence, 6)))
        self.assertEqual(evidence.calls, calls)
        restored = SuccessBuffer(deepcopy(buffer.snapshot()))
        self.assertEqual(digest(expected), digest(SequenceCache().resolve(restored, evidence, 6)))
        with self.assertRaisesRegex(ValueError, 'prior completed episode'):
            cache.resolve(buffer, evidence, 4)

    def test_checkpoint_retention_and_compression_preserve_state(self):
        job = dict(update=12, evaluations=[dict(update=0), dict(update=6)], first_admission_update=4)
        self.assertEqual(retained_updates(job), {0, 1, 2, 3, 4, 5, 6, 9, 10, 11, 12})
        payload = dict(buffer=SuccessBuffer().snapshot(), refs=[(1, 2)], weights=[.1, -.2])
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'checkpoint.pt.gz'
            save_compact(path, payload, 6)
            self.assertEqual(digest(payload), digest(load(path)))
            self.assertFalse(path.with_name(path.name + '.tmp').exists())


if __name__ == '__main__':
    unittest.main()
