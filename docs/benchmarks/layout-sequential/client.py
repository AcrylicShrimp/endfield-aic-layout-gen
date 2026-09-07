"""Bounded CPU environment workers; JSON lines only, no shell commands or pickle."""

import concurrent.futures
import json
import os
from pathlib import Path
import selectors
import subprocess
import time


class Worker:
    def __init__(self, roots, output, timeout=30):
        self.timeout = timeout
        self.ordinal = 0
        self.closed = False
        self.output = Path(output)
        self.log = self.output.parent.joinpath(self.output.name + ".stderr.log").open("x")
        self.process = subprocess.Popen(
            ["node", str(Path(__file__).with_name("server.mjs")), "--roots", str(roots),
             "--output-directory", str(output)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.log, bufsize=0, start_new_session=True)
        self.buffer = b""
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)

    def call(self, op, **args):
        identity = self.ordinal
        self.ordinal += 1
        self.process.stdin.write((json.dumps(dict(schema_version=1, id=identity, op=op, **args)) + "\n").encode())
        deadline = time.monotonic() + self.timeout
        while b"\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not self.selector.select(remaining):
                self.close(force=True)
                raise TimeoutError(f"Environment request {identity} exceeded {self.timeout}s: {self.output}")
            data = os.read(self.process.stdout.fileno(), 1024 * 1024)
            if not data:
                raise RuntimeError(f"Environment exited; inspect {self.log.name}")
            self.buffer += data
            if len(self.buffer) > 64 * 1024 * 1024:
                raise RuntimeError("Environment response exceeded 64 MiB")
        line, self.buffer = self.buffer.split(b"\n", 1)
        result = json.loads(line)
        if result.get("schema_version") != 1 or result.get("id") != identity:
            raise RuntimeError("Environment response identity mismatch")
        if "error" in result:
            raise RuntimeError(json.dumps(result["error"]))
        return result["result"]

    def close(self, force=False):
        if self.closed:
            return
        if self.process.poll() is None:
            if not force:
                try:
                    self.process.stdin.close()
                    self.process.wait(timeout=2)
                except (OSError, subprocess.TimeoutExpired):
                    force = True
            if force:
                # This worker owns its new process group, including its validator child.
                import signal
                try:
                    os.killpg(self.process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                self.process.wait(timeout=2)
        self.selector.close()
        self.process.stdout.close()
        if not self.process.stdin.closed:
            self.process.stdin.close()
        self.log.close()
        self.closed = True
        if not force and self.process.returncode != 0:
            raise RuntimeError(f"Environment exited with {self.process.returncode}; inspect {self.log.name}")


class Pool:
    def __init__(self, roots, output, workers):
        if not 1 <= workers <= 16:
            raise ValueError("Expected 1..16 environment workers")
        self.executor = concurrent.futures.ThreadPoolExecutor(max_workers=workers)
        self.workers = []
        try:
            for i in range(workers):
                self.workers.append(Worker(roots, Path(output) / f"worker-{i}"))
            self.metadata = self.workers[0].call("hello")
        except BaseException:
            self.close()
            raise

    def call(self, op, arguments):
        if len(arguments) != len(self.workers):
            raise ValueError("One request per worker is required")
        futures = [self.executor.submit(w.call, op, **a) for w, a in zip(self.workers, arguments)]
        return [f.result() for f in futures]

    def close(self):
        self.executor.shutdown(wait=True, cancel_futures=True)
        errors = []
        for worker in self.workers:
            try:
                worker.close()
            except Exception as error:
                errors.append(error)
        if errors:
            raise errors[0]

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
