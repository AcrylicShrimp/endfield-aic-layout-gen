"""Transient IPC and exact prepared-input transport contract checks."""
import os
import socket
import sys
import pickle
from concurrent.futures import ThreadPoolExecutor
from threading import Event
import unittest
from unittest.mock import patch

import numpy as np
import torch

from ppo_harness_state import HarnessError
from ppo_harness_transport import Reader, send, receive, encode, HEADER, MAGIC
from ppo_training_transfer import move_prepared
from ppo_training_supply import prepared_batches


class SupplyTests(unittest.TestCase):
    def equal_tree(self, actual, expected):
        if isinstance(expected, torch.Tensor):
            torch.testing.assert_close(actual, expected, rtol=0, atol=0)
        elif isinstance(expected, dict):
            self.assertEqual(actual.keys(), expected.keys())
            for key in expected:
                self.equal_tree(actual[key], expected[key])
        elif isinstance(expected, (list, tuple)):
            self.assertIs(type(actual), type(expected))
            self.assertEqual(len(actual), len(expected))
            for a, b in zip(actual, expected):
                self.equal_tree(a, b)
        else:
            self.assertEqual(actual, expected)

    @unittest.skipUnless(sys.platform == 'linux', 'Linux shared array delivery')
    def test_fragmented_and_coalesced_frames_preserve_arrays_and_message_order(self):
        shared = np.arange(2400, dtype=np.float32).reshape(40, 60)
        message = dict(kind='result', experience_payload=dict(a=shared, b=shared, duplicate=shared.copy(),
                       view=shared[:, ::2], empty=np.empty((0, 2), dtype=np.int64),
                       mask=np.array([True, False]), scalar=np.array(1.25), fortran=np.asfortranarray(shared)))
        packed, fd = encode(message)
        other, no_fd = encode(dict(kind='ready'))
        self.assertIsNone(no_fd)
        reader = Reader(1024 * 1024)
        actual = reader.feed(packed[:7], [fd])
        packed += other
        for offset in range(7, len(packed), 7):
            actual.extend(reader.feed(packed[offset:offset+7]))
        self.assertEqual([m['kind'] for m in actual], ['result', 'ready'])
        payload = actual[0]['experience_payload']
        for key, value in message['experience_payload'].items():
            np.testing.assert_array_equal(payload[key], value, strict=True)
        self.assertIs(payload['a'], payload['b'])
        self.assertTrue(np.shares_memory(payload['a'], payload['duplicate']))
        self.assertLess(reader.shared_bytes, reader.logical_bytes)
        self.assertEqual(reader.buffer, b'')
        self.assertEqual(reader.bytes, len(packed))
        reader.close()
        # Mapping ownership follows array lifetime, independently of the reader/fd.
        np.testing.assert_array_equal(payload['a'], shared)
        torch.testing.assert_close(torch.as_tensor(payload['a']), torch.as_tensor(shared), rtol=0, atol=0)

    @unittest.skipUnless(sys.platform == 'linux', 'Linux shared array delivery')
    def test_socket_descriptor_delivery_and_cleanup_on_invalid_envelope(self):
        sender, receiver = socket.socketpair()
        try:
            send(dict(kind='result', value=np.arange(1000)), sender)
            data, fds = receive(receiver)
            self.assertEqual(len(fds), 1)
            fd = fds[0]
            reader = Reader(65536)
            result = reader.feed(data, fds)[0]
            np.testing.assert_array_equal(result['value'], np.arange(1000))
            with self.assertRaises(OSError):
                os.fstat(fd)
            packed, fd = encode(dict(value=np.arange(10)))
            with self.assertRaisesRegex(HarnessError, 'worker-message-descriptors'):
                Reader(65536).feed(packed)
            with self.assertRaisesRegex(HarnessError, 'worker-message-format'):
                Reader(65536).feed(HEADER.pack(b'BAD!', 3), [fd])
            with self.assertRaises(OSError):
                os.fstat(fd)
        finally:
            sender.close()
            receiver.close()

    def test_invalid_frame_bounds_and_formats_fail_before_deserialization(self):
        for header, code in [(HEADER.pack(MAGIC, 0), 'worker-message-size'),
                             (HEADER.pack(MAGIC, 65), 'worker-message-size'),
                             (HEADER.pack(b'BAD!', 3), 'worker-message-format')]:
            with self.subTest(code=code), self.assertRaisesRegex(HarnessError, code):
                Reader(64).feed(header)
        body = pickle.dumps(['unexpected list'])
        with self.assertRaisesRegex(HarnessError, 'worker-message-format'):
            Reader(1024).feed(HEADER.pack(MAGIC, len(body)) + body)

    def test_preparation_stays_one_ahead_and_preserves_requested_order(self):
        second_started, release = Event(), Event()
        calls = []

        def prepare(steps, *unused):
            calls.append(steps)
            if len(calls) == 2:
                second_started.set()
                if not release.wait(2):
                    raise TimeoutError('test preparation release')
            return steps

        with patch('ppo_training_supply.prepare_cpu', side_effect=prepare), ThreadPoolExecutor(1) as executor:
            stream = prepared_batches(executor, list(range(5)), torch.zeros(5), torch.ones(5), {}, 8, 2,
                                      [3, 1, 4, 0, 2], 2, lambda: None)
            try:
                first = next(stream)
                self.assertTrue(second_started.wait(2))
                self.assertEqual(first[:2], ([3, 1], [3, 1]))
                self.assertEqual(calls, [[3, 1], [4, 0]])
                release.set()
                rest = list(stream)
                self.assertEqual([value[0] for value in rest], [[4, 0], [2]])
            finally:
                release.set()
                stream.close()

    def test_preparation_exception_reaches_learner(self):
        with patch('ppo_training_supply.prepare_cpu', side_effect=ValueError('broken preparation')), \
             ThreadPoolExecutor(1) as executor:
            stream = prepared_batches(executor, [0], torch.zeros(1), torch.ones(1), {}, 8, 1, [0], 1, lambda: None)
            with self.assertRaisesRegex(ValueError, 'broken preparation'):
                next(stream)

    def test_cpu_tree_structure_and_noncontiguous_values_survive(self):
        tree = dict(a=torch.arange(12).reshape(3, 4).T, b=[torch.tensor(True), None, 'metadata'],
                    c=(torch.empty(0, 3), torch.tensor([1.25], dtype=torch.float64)))
        actual = move_prepared(tree, 'cpu')
        self.equal_tree(actual, tree)
        self.assertIsInstance(actual['c'], tuple)

    @unittest.skipUnless(torch.cuda.is_available(), 'CUDA transfer requires a CUDA device')
    def test_cuda_transfer_preserves_values_and_stream_lifetime(self):
        tree = dict(a=torch.arange(12).reshape(3, 4).T, b=[torch.tensor(True), None, 'metadata'],
                    c=(torch.empty(0, 3), torch.tensor([1.25], dtype=torch.float64)))
        actual = move_prepared(tree, 'cuda')
        # Immediate allocator reuse must not overwrite a still-pending H2D copy.
        another = move_prepared(dict(a=torch.full((1024,), -1.0)), 'cuda')
        self.equal_tree(move_prepared(actual, 'cpu'), tree)
        self.assertEqual(float(another['a'].sum()), -1024)


if __name__ == '__main__':
    unittest.main()
