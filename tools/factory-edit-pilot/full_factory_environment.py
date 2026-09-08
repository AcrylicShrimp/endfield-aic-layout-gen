"""Selected-action accounting around the full physical factory acceptance API."""
import hashlib
import json
from pathlib import Path
import subprocess
import time

from native import load_native

ROOT = Path(__file__).resolve().parents[2]


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


class FactoryRuntime:
    def __init__(self, library, catalogs):
        self.module = load_native(Path(library))
        self.build = json.loads(self.module.Engine.build_info_json())
        for name, expected in self.build['files_sha256'].items():
            if hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != expected:
                raise ValueError(f'factory.stale-build: {name}')
        self.catalog_paths = Path(catalogs).read_text()
        self.catalogs = {k: json.loads(Path(p).read_text()) for k, p in json.loads(self.catalog_paths).items()}
        self.worker = subprocess.Popen(['node', str(Path(__file__).with_name('full_factory_executor.mjs')),
                                        '--catalogs', str(catalogs)], stdin=subprocess.PIPE,
                                       stdout=subprocess.PIPE, text=True)
        self.serial = 0

    def rpc(self, op, witness, **kwargs):
        self.serial += 1
        self.worker.stdin.write(json.dumps(dict(schema_version=2, id=self.serial, op=op, witness=witness, **kwargs)) + '\n')
        self.worker.stdin.flush()
        line = self.worker.stdout.readline()
        if not line:
            raise RuntimeError('factory.executor-exited: no response from Node')
        result = json.loads(line)
        if result.get('status') == 'error' or result.get('id') != self.serial or result.get('schema_version') != 2:
            raise RuntimeError(f'factory.executor-protocol: {result}')
        return result

    def close(self):
        # An interrupted RPC may leave a response larger than the pipe buffer.
        # Drain stdout while closing, so the owned worker can finish and see EOF.
        if self.worker.stdin is not None:
            self.worker.stdin.close()
            self.worker.stdin = None
        try:
            self.worker.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            self.worker.kill()
            self.worker.communicate()


class FactoryEpisode:
    def __init__(self, runtime, case, horizon=8, elapsed=30, work=5000):
        self.started = time.perf_counter()
        self.runtime, self.case = runtime, case
        self.horizon, self.elapsed, self.work = horizon, elapsed, work
        self.session = runtime.module.FactoryEditSession.open(str(ROOT), case['request'], case['initial'],
                       runtime.catalog_paths, json.dumps(dict(proposals=horizon, elapsed_ms=round(elapsed*1000))))
        self.state = json.loads(self.session.snapshot_json())
        self.selected = 0
        self.submitted = 0
        self.trace = []
        self.seen = {digest(self.state['current'])}

    @property
    def done(self):
        return self.selected >= self.horizon or time.perf_counter()-self.started >= self.elapsed

    def describe(self, options=None):
        return self.runtime.rpc('describe', self.state['current'], options=options or {})

    def step(self, action):
        if self.done:
            raise ValueError('factory.episode-exhausted')
        began = time.perf_counter()
        self.selected += 1
        before = digest(self.state['current'])
        result = (dict(status='candidate', witness=self.state['best'], route_expansions=0, seconds=0)
                  if action['kind'] == 'restore_best' else
                  self.runtime.rpc('execute', self.state['current'], action=action, work=self.work))
        row = dict(selected=self.selected, action=action, executor_status=result['status'],
                   route_expansions=result['route_expansions'], executor_seconds=result['seconds'],
                   diagnostics=result.get('diagnostics', []))
        if any(d.get('code') == 'factory.executor-error' for d in row['diagnostics']):
            raise RuntimeError(f'factory.executor-bug: {row}')
        if time.perf_counter()-self.started >= self.elapsed:
            row['status'] = 'late'
        elif result.get('witness') is not None:
            acceptance = json.loads(self.session.propose_json(json.dumps(result['witness'])))
            self.submitted += 1
            row['acceptance'] = acceptance
            proposed_state = json.loads(self.session.snapshot_json())
            if time.perf_counter()-self.started >= self.elapsed:
                # Discard a late native result at the outer, earlier episode deadline.
                row['status'] = 'late'
            else:
                row['status'] = acceptance['status']
                self.state = proposed_state
        else:
            row['status'] = 'executor_rejected'
        after = digest(self.state['current'])
        row.update(unchanged=before == after, revisited=before != after and after in self.seen,
                   current_score=self.state['current_metrics']['score'], best_score=self.state['best_metrics']['score'],
                   seconds=time.perf_counter()-began, elapsed_seconds=time.perf_counter()-self.started)
        self.seen.add(after)
        self.trace.append(row)
        return row

    def summary(self):
        return dict(case=self.case['id'], instances=len(self.state['best']['placements']),
                    initial_metrics=self.state['initial_metrics'], best_metrics=self.state['best_metrics'],
                    selected=self.selected, native_proposals=self.submitted,
                    elapsed_seconds=time.perf_counter()-self.started,
                    trace=self.trace, best_sha256=digest(self.state['best']))
