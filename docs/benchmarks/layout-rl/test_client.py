import io
import unittest
from unittest.mock import Mock, patch

from client import Worker


class ClientTests(unittest.TestCase):
    def worker(self, returncode=0):
        worker = Worker.__new__(Worker)
        worker.timeout = 1
        worker.ordinal = 0
        worker.closed = False
        worker.output = "test-owned-worker"
        worker.buffer = b""
        worker.process = Mock()
        worker.process.poll.return_value = returncode
        worker.process.returncode = returncode
        worker.process.stdin = io.BytesIO()
        worker.process.stdout = Mock()
        worker.selector = Mock()
        worker.selector.select.return_value = [True]
        worker.log = Mock()
        worker.log.name = "test-owned-worker.log"
        return worker

    def test_nonzero_worker_exit_is_not_silently_accepted(self):
        worker = self.worker(1)
        with self.assertRaisesRegex(RuntimeError, "exited with 1"):
            worker.close()
        self.assertTrue(worker.closed)
        worker.close()

    def test_response_identity_and_structured_errors_are_checked(self):
        for response, message in [(b'{"schema_version":1,"id":9,"result":{}}\n', "identity"),
                                  (b'{"schema_version":1,"id":0,"error":{"code":"rl-input-invalid"}}\n', "rl-input-invalid")]:
            worker = self.worker()
            with patch("client.os.read", return_value=response):
                with self.assertRaisesRegex(RuntimeError, message):
                    worker.call("hello")
            worker.close()

    def test_force_cleanup_targets_only_the_owned_worker_group(self):
        worker = self.worker(None)
        worker.process.pid = 987654
        with patch("client.os.killpg") as kill:
            worker.close(force=True)
            self.assertEqual(kill.call_args.args[0], 987654)
        self.assertTrue(worker.closed)


if __name__ == "__main__":
    unittest.main()
