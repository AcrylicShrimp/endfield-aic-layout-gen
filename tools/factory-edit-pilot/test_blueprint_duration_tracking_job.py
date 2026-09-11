"""The normal job cannot silently train after its required observer has failed."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

from blueprint_duration_tracking_job import TrackingProcess
from ppo_harness_state import write, sha
from blueprint_duration_job import execute


class TrackingJobTests(unittest.TestCase):
    def test_only_declared_readiness_can_disable_tracking(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(root / 'protocol.json', dict(config=dict(purpose='main', tracking=dict(backend='disabled'))))
            with self.assertRaisesRegex(ValueError, 'only declared readiness'):
                TrackingProcess(root, None)
            write(root / 'protocol.json', dict(config=dict(purpose='blueprint-success-duration-readiness', tracking=dict(backend='disabled'))))
            tracker = TrackingProcess(root, None)
            tracker.health(initial=True)
            tracker.finish()
            tracker.close()

    def test_preflight_preserves_historical_and_complete_job_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sentinel = b'preserved historical completion\n'
            (root / 'completion.json').write_bytes(sentinel)
            for schema, status, message in [('blueprint-success-duration-v1', 'paused', 'v2 required'),
                                             ('blueprint-success-duration-v2', 'complete', 'already complete')]:
                write(root / 'protocol.json', dict(schema=schema))
                write(root / 'state.json', dict(schema=schema, status=status,
                    protocol_sha256=sha(root / 'protocol.json')))
                with self.assertRaisesRegex(ValueError, message):
                    execute(root)
                self.assertEqual((root / 'completion.json').read_bytes(), sentinel)

    def test_dead_observer_blocks_learning_and_iteration_zero_waits_for_export(self):
        tracker = TrackingProcess.__new__(TrackingProcess)
        tracker.output = Path('/example/run')
        tracker.process = Mock()
        tracker.process.poll.return_value = 1
        with self.assertRaisesRegex(RuntimeError, 'observer exited'):
            tracker.health()
        tracker.process.poll.return_value = None
        tracker.wait_for = Mock()
        tracker.health(initial=True)
        predicate, description = tracker.wait_for.call_args.args
        self.assertFalse(predicate(dict(exported_events=0)))
        self.assertTrue(predicate(dict(exported_events=1)))
        self.assertEqual(description, 'iteration-zero export')


if __name__ == '__main__':
    unittest.main()
