"""Standalone instrumentation; never imported by ordinary training."""
from collections import defaultdict
from contextlib import ExitStack, contextmanager
import functools
import json
from pathlib import Path
import pstats
import subprocess
import time
from types import SimpleNamespace
from unittest.mock import patch
import uuid


class Timings:
    def __init__(self):
        self.rows = defaultdict(lambda: dict(calls=0, inclusive_seconds=0., exclusive_seconds=0.))
        self.stack = []
        self.counters = defaultdict(float)

    @contextmanager
    def region(self, name):
        frame = [time.perf_counter(), 0.]
        self.stack.append(frame)
        try:
            yield
        finally:
            elapsed = time.perf_counter() - frame[0]
            self.stack.pop()
            row = self.rows[name]
            row['calls'] += 1
            row['inclusive_seconds'] += elapsed
            row['exclusive_seconds'] += elapsed - frame[1]
            if self.stack:
                self.stack[-1][1] += elapsed

    def wrap(self, name, function):
        @functools.wraps(function)
        def measured(*args, **kwargs):
            with self.region(name):
                return function(*args, **kwargs)
        return measured

    def report(self):
        return dict(regions=dict(sorted(self.rows.items())), counters=dict(self.counters),
                    accounting='Inclusive regions nest. Exclusive times subtract instrumented children only. Pipe reads include Node computation and must not be added to reported Node compute.')


class Stream:
    def __init__(self, stream, timings, name):
        self.stream, self.timings, self.name = stream, timings, name

    def __getattr__(self, name):
        return getattr(self.stream, name)

    def write(self, text):
        self.timings.counters['ipc.request_utf8_bytes'] += len(text.encode())
        with self.timings.region('ipc.write'):
            return self.stream.write(text)

    def flush(self):
        with self.timings.region('ipc.flush'):
            return self.stream.flush()

    def readline(self, *args):
        with self.timings.region('ipc.read_wait'):
            value = self.stream.readline(*args)
        self.timings.counters['ipc.response_utf8_bytes'] += len(value.encode())
        return value


class Session:
    def __init__(self, value, timings):
        self.value, self.timings = value, timings

    def propose_json(self, text):
        self.timings.counters['native.proposal_utf8_bytes'] += len(text.encode())
        with self.timings.region('native.propose_parse_validate_commit'):
            return self.value.propose_json(text)

    def snapshot_json(self):
        with self.timings.region('native.snapshot_serialize'):
            text = self.value.snapshot_json()
        self.timings.counters['native.snapshot_utf8_bytes'] += len(text.encode())
        return text

    def __getattr__(self, name):
        return getattr(self.value, name)


@contextmanager
def instrument(timings, node_directory, actor=None, critic=None):
    import full_factory_environment as environment
    import full_factory_input_contracts as inputs
    import ppo_training_observation as observation
    import ppo_training_rollout as collection
    import random_factory_generation as generation
    node_directory.mkdir(parents=True, exist_ok=True)
    original_popen = subprocess.Popen
    original_runtime = environment.FactoryRuntime.__init__
    original_rpc = environment.FactoryRuntime.rpc
    original_dumps, original_loads = json.dumps, json.loads

    def popen(arguments, *args, **kwargs):
        if isinstance(arguments, (list, tuple)) and Path(arguments[0]).name == 'node':
            arguments = [arguments[0], '--cpu-prof', f'--cpu-prof-dir={node_directory.resolve()}',
                         f'--cpu-prof-name={uuid.uuid4().hex}.cpuprofile', *arguments[1:]]
        return original_popen(arguments, *args, **kwargs)

    def runtime_init(self, *args, **kwargs):
        with timings.region('runtime.open'):
            original_runtime(self, *args, **kwargs)
        original = self.module.FactoryEditSession
        def open_session(*args):
            with timings.region('native.open_catalogs_request_initial'):
                value = original.open(*args)
            return Session(value, timings)
        self.module = SimpleNamespace(FactoryEditSession=SimpleNamespace(open=open_session))
        self.worker.stdin = Stream(self.worker.stdin, timings, 'stdin')
        self.worker.stdout = Stream(self.worker.stdout, timings, 'stdout')

    def rpc(self, operation, witness, **kwargs):
        with timings.region(f'rpc.{operation}'):
            result = original_rpc(self, operation, witness, **kwargs)
        timings.counters[f'node.{operation}.reported_seconds'] += result['seconds']
        if operation == 'describe':
            poses = result.get('poses', {})
            timings.counters['node.pose.reported_seconds'] += poses.get('seconds', 0.)
            timings.counters['node.description_cache_hits'] += poses.get('observation_cache') == 'hit'
            timings.counters['node.actions_returned'] += len(result['actions'])
        else:
            timings.counters['node.route_expansions'] += result.get('route_expansions', 0)
        return result

    def dumps(*args, **kwargs):
        with timings.region('python.json_encode'):
            value = original_dumps(*args, **kwargs)
        timings.counters['python.json_encoded_utf8_bytes'] += len(value.encode())
        return value

    def loads(value, *args, **kwargs):
        with timings.region('python.json_decode'):
            return original_loads(value, *args, **kwargs)

    targets = [(environment.FactoryEpisode, '__init__', 'episode.open'),
               (environment.FactoryEpisode, 'step', 'episode.step'),
               (observation.WitnessIdentities, 'mask', 'selection.mask'),
               (observation.WitnessIdentities, 'get', 'selection.identities'),
               (collection, 'observation_pair', 'observation.pair'),
               (inputs, 'observe', 'observation.arrays'),
               (collection, 'observation_hash', 'observation.hash'),
               (collection, 'distribution', 'selection.distribution'),
               (collection, 'probability_hash', 'selection.probability_hash'),
               (collection, 'save_gzip', 'persistence.episode_gzip'),
               (generation, 'invoke', 'generation.external_command'),
               (generation, 'import_root', 'generation.import_validate'),
               (generation, 'write', 'generation.write')]
    with ExitStack() as stack:
        stack.enter_context(patch.object(subprocess, 'Popen', popen))
        stack.enter_context(patch.object(environment.FactoryRuntime, '__init__', runtime_init))
        stack.enter_context(patch.object(environment.FactoryRuntime, 'rpc', rpc))
        stack.enter_context(patch.object(json, 'dumps', dumps))
        stack.enter_context(patch.object(json, 'loads', loads))
        for owner, attr, name in targets:
            stack.enter_context(patch.object(owner, attr, timings.wrap(name, getattr(owner, attr))))
        for model, name in [(actor, 'inference.actor_cpu'), (critic, 'inference.critic_cpu')]:
            if model is not None:
                stack.enter_context(patch.object(model, 'forward', timings.wrap(name, model.forward)))
        yield


def python_summary(profile):
    rows = [dict(file=filename, line=line, function=function, primitive_calls=primitive,
                 calls=calls, exclusive_seconds=exclusive, cumulative_seconds=cumulative)
            for (filename, line, function), (primitive, calls, exclusive, cumulative, _)
            in pstats.Stats(profile).stats.items()]
    return dict(exclusive=sorted(rows, key=lambda x: x['exclusive_seconds'], reverse=True)[:35],
                cumulative=sorted(rows, key=lambda x: x['cumulative_seconds'], reverse=True)[:35])


def node_summary(directory):
    totals = defaultdict(float)
    samples = 0
    for path in directory.glob('*.cpuprofile'):
        data = json.loads(path.read_text())
        nodes = {node['id']: node['callFrame'] for node in data['nodes']}
        for node, delta in zip(data.get('samples', []), data.get('timeDeltas', [])):
            frame = nodes[node]
            key = frame['url'], frame['functionName'], frame['lineNumber']
            totals[key] += delta / 1e6
            samples += 1
    rows = [dict(file=k[0], function=k[1], line=k[2] + 1, sampled_seconds=v) for k, v in totals.items()]
    return dict(files=len(list(directory.glob('*.cpuprofile'))), samples=samples,
                exclusive=sorted(rows, key=lambda x: x['sampled_seconds'], reverse=True)[:35],
                scope='V8 sampling includes runtime setup/idle and instrumentation; sampled totals are not uninstrumented wall time.')
