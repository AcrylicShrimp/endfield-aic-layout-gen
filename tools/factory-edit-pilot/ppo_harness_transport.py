"""Trusted local socket messages with deduplicated, anonymous shared array buffers."""
import array
from collections import deque
import ctypes
import hashlib
import mmap
import os
import pickle
import socket
import struct
import sys
import time

import numpy as np

from ppo_harness_state import require

HEADER = struct.Struct('!4sQ')
MAGIC = b'AIC4'
_channel = None


def shared_descriptor():
    require(sys.platform == 'linux', 'shared-memory-platform',
            'native-v4 array delivery requires Linux memfd_create', 'collection')
    if hasattr(os, 'memfd_create'):
        return os.memfd_create('aic-ppo-experience', os.MFD_CLOEXEC)
    # Some portable CPython builds omit the os wrapper although the host libc
    # supports memfd. Use the same public Linux API without a disk fallback.
    create = ctypes.CDLL(None, use_errno=True).memfd_create
    create.argtypes = [ctypes.c_char_p, ctypes.c_uint]
    create.restype = ctypes.c_int
    fd = create(b'aic-ppo-experience', 1)  # MFD_CLOEXEC from linux/memfd.h.
    if fd < 0:
        number = ctypes.get_errno()
        raise OSError(number, os.strerror(number))
    return fd


def channel():
    global _channel
    if _channel is None:
        _channel = socket.socket(fileno=int(os.environ['AIC_PPO_TRANSPORT_FD']))
    return _channel


def encode(message):
    """Return a metadata frame and an owned fd; callers must close the latter."""
    buffers = []
    metadata = pickle.dumps(message, protocol=5, buffer_callback=buffers.append)
    unique, keys, references, offsets = [], {}, [], []
    total = logical = 0
    for buffer in buffers:
        raw = buffer.raw()
        logical += len(raw)
        key = (len(raw), hashlib.sha256(raw).digest())
        match = next((i for i in keys.get(key, [])
                      if np.array_equal(np.frombuffer(raw, dtype=np.uint8),
                                        np.frombuffer(unique[i], dtype=np.uint8))), None)
        if match is None:
            match = len(unique)
            keys.setdefault(key, []).append(match)
            total = (total + 63) // 64 * 64
            offsets.append((total, len(raw)))
            unique.append(raw)
            total += len(raw)
        references.append(match)
    fd = None
    try:
        if total:
            fd = shared_descriptor()
            os.ftruncate(fd, total)
            with mmap.mmap(fd, total, access=mmap.ACCESS_WRITE) as mapping:
                for (offset, size), raw in zip(offsets, unique):
                    mapping[offset:offset + size] = raw
        packed = pickle.dumps(dict(metadata=metadata, offsets=offsets, references=references,
                                   shared_bytes=total, logical_bytes=logical), protocol=5)
        return HEADER.pack(MAGIC, len(packed)) + packed, fd
    except BaseException:
        if fd is not None:
            os.close(fd)
        raise


def send(message, stream=None):
    stream = channel() if stream is None else stream
    packed, fd = encode(message)
    try:
        ancillary = [] if fd is None else [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array('i', [fd]))]
        sent = stream.sendmsg([packed], ancillary)
        stream.sendall(packed[sent:])
    finally:
        if fd is not None:
            os.close(fd)


def receive(stream):
    data, ancillary, flags, _ = stream.recvmsg(65536, socket.CMSG_SPACE(16 * array.array('i').itemsize))
    descriptors = []
    try:
        for level, kind, raw in ancillary:
            if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                values = array.array('i')
                values.frombytes(raw[:len(raw) - len(raw) % values.itemsize])
                descriptors.extend(values)
        for fd in descriptors:
            os.set_inheritable(fd, False)
        require(not flags & socket.MSG_CTRUNC, 'worker-message-descriptors',
                'truncated shared-memory descriptors', 'collection')
        return data, descriptors
    except BaseException:
        for fd in descriptors:
            os.close(fd)
        raise


class Reader:
    def __init__(self, limit):
        self.limit = limit
        self.buffer = bytearray()
        self.descriptors = deque()
        self.bytes = self.shared_bytes = self.logical_bytes = 0
        self.decode_seconds = 0.0

    def close(self):
        while self.descriptors:
            os.close(self.descriptors.popleft())
        self.buffer.clear()

    def decode(self, wire):
        require(isinstance(wire, dict) and set(wire) ==
                {'metadata', 'offsets', 'references', 'shared_bytes', 'logical_bytes'},
                'worker-message-format', 'invalid shared-memory envelope', 'collection')
        size, offsets, references = wire['shared_bytes'], wire['offsets'], wire['references']
        require(type(size) is int and 0 <= size <= self.limit and isinstance(offsets, list) and
                isinstance(references, list) and isinstance(wire['metadata'], bytes),
                'worker-message-size', 'invalid shared-memory bounds', 'collection')
        end = 0
        for item in offsets:
            require(isinstance(item, tuple) and len(item) == 2 and all(type(n) is int for n in item),
                    'worker-message-size', 'invalid buffer range', 'collection')
            offset, length = item
            require(offset == (end + 63) // 64 * 64 and 0 <= length <= size - offset,
                    'worker-message-size', 'buffer outside shared-memory bounds', 'collection')
            end = offset + length
        require(end == size and all(type(i) is int and 0 <= i < len(offsets) for i in references),
                'worker-message-size', 'invalid shared buffer references', 'collection')
        logical = sum(offsets[i][1] for i in references)
        require(logical == wire['logical_bytes'] and logical <= self.limit,
                'worker-message-size', 'logical array bytes exceed memory bound', 'collection')
        mapping = None
        if size:
            require(bool(self.descriptors), 'worker-message-descriptors', 'missing shared-memory descriptor', 'collection')
            fd = self.descriptors.popleft()
            try:
                require(os.fstat(fd).st_size == size, 'worker-message-size', 'shared-memory file size mismatch', 'collection')
                # Private mappings remain writable for torch.as_tensor; ndarray
                # bases own the mapping until the transient batch is released.
                mapping = mmap.mmap(fd, size, access=mmap.ACCESS_COPY)
            finally:
                os.close(fd)
        views = [memoryview(mapping)[offset:offset + length] if mapping is not None else memoryview(bytearray())
                 for offset, length in offsets]
        message = pickle.loads(wire['metadata'], buffers=[views[i] for i in references])
        require(isinstance(message, dict), 'worker-message-format', 'worker message must be a mapping', 'collection')
        self.shared_bytes += size
        self.logical_bytes += logical
        return message

    def feed(self, data, descriptors=()):
        self.descriptors.extend(descriptors)
        self.bytes += len(data)
        self.buffer.extend(data)
        messages = []
        try:
            while len(self.buffer) >= HEADER.size:
                magic, size = HEADER.unpack_from(self.buffer)
                require(magic == MAGIC, 'worker-message-format', 'expected native-v4 framed message', 'collection')
                require(0 < size <= self.limit, 'worker-message-size',
                        f'worker message exceeds process-tree memory bound: {size} bytes', 'collection')
                end = HEADER.size + size
                if len(self.buffer) < end:
                    break
                began = time.perf_counter()
                with memoryview(self.buffer)[HEADER.size:end] as packed:
                    wire = pickle.loads(packed)
                messages.append(self.decode(wire))
                self.decode_seconds += time.perf_counter() - began
                del self.buffer[:end]
        except BaseException:
            self.close()
            raise
        return messages
