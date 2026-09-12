"""Fresh, durable episode inputs; paired policies may share a slot, never recycle it."""
from copy import deepcopy
import fcntl
import json
from pathlib import Path
import time

from blueprint_ppo_store import ROOT, checked, read, reference, sha, write
from blueprint_ppo_update import digest, require

SCHEMA = 'blueprint-training-input-v1'
GENERATOR_SOURCES = (
    'blueprint_training_input.py', 'random_factory_generation.py', 'native_factory_admission.py',
    'native_factory_runtime.py', 'native.py', 'full_factory_environment.py',
    'full_factory_selection.py', 'full_factory_screen.py',
)


def validate_input(config):
    value = config.get('training_input', {})
    require(value.get('schema') == SCHEMA, 'blueprint.training-input.schema: declare an explicit training input')
    require(value.get('kind') in ('fresh-random', 'fixed-fixture-diagnostic'),
            'blueprint.training-input.kind: fresh-random or fixed-fixture-diagnostic required')
    if value['kind'] == 'fixed-fixture-diagnostic':
        require('diagnostic' in config['purpose'] or 'readiness' in config['purpose'],
                'blueprint.training-input.fixture: fixed inputs are restricted to explicit diagnostics')
        return
    require(config['starts'] == [dict(name='fresh-random', prefix=[])],
            'blueprint.training-input.starts: fresh episodes have no fixed-facility prefixes')
    require(type(value.get('seed_base')) is int and value['seed_base'] >= 0
            and type(value.get('max_attempts')) is int and 1 <= value['max_attempts'] <= 32,
            'blueprint.training-input.seeds: nonnegative seed base and bounded attempts required')
    require(bool(value.get('spec')) and bool(value.get('directory')),
            'blueprint.training-input.paths: generation spec and durable directory required')
    require(type(value.get('episode_setup_seconds')) in (int, float) and value['episode_setup_seconds'] > 0,
            'blueprint.training-input.budget: positive total episode setup deadline required')


def source_paths(config):
    """Bind generator code, real runtime data and the constructor binary at preparation."""
    if config['training_input']['kind'] != 'fresh-random':
        return []
    path = ROOT / config['training_input']['spec']
    spec = read(path)
    require('frozen_starts' not in spec, f'blueprint.training-input.frozen: {path}')
    return [path, ROOT / spec['cli'], *(ROOT / p for p in spec['catalogs'].values()),
            *(ROOT / p['source'] for p in spec['products']),
            *(Path(__file__).with_name(name) for name in GENERATOR_SOURCES)]


class FreshInputs:
    def __init__(self, config, library, module, blueprint_spec):
        from random_factory_generation import resolve_spec
        self.config, self.module, self.blueprint_spec = config, module, blueprint_spec
        self.options = config['training_input']
        require(self.options['directory'] != '@run',
                'blueprint.training-input.directory: prepare the run before opening its private input store')
        self.root = (ROOT / self.options['directory']).resolve()
        self.spec = resolve_spec(ROOT / self.options['spec'])
        self.spec['native_library'] = str(library)
        require('frozen_starts' not in self.spec, 'blueprint.training-input.frozen: cannot reuse a frozen training bank')
        require({k: sha(p) for k, p in self.spec['catalogs'].items()} ==
                {k: sha(ROOT / p) for k, p in blueprint_spec['catalogs'].items()},
                'blueprint.training-input.catalogs: generator and blueprint catalogs differ')
        self.contract = dict(schema='blueprint-fresh-input-store-v1', seed_base=self.options['seed_base'],
            max_attempts=self.options['max_attempts'], streams=config['training']['streams'],
            episode_setup_seconds=self.options['episode_setup_seconds'],
            spec_sha256=sha(ROOT / self.options['spec']), native_sha256=sha(library),
            blueprint_config=blueprint_spec['config'],
            sources={str(p.resolve().relative_to(ROOT)): sha(p) for p in source_paths(config)})
        self.native = None

    def identity(self, stream, episode):
        streams = self.config['training']['streams']
        require(type(stream) is int and 0 <= stream < streams and type(episode) is int and episode >= 0,
                'blueprint.training-input.slot: invalid stream/episode identity')
        slot = episode * streams + stream
        return dict(stream=stream, episode=episode, slot=slot,
                    first_seed=self.options['seed_base'] + slot * self.options['max_attempts'])

    def load(self, ref, stream, episode):
        record = read(checked(ref, self.root))
        require(record['schema'] == 'blueprint-fresh-episode-v1' and record['identity'] == self.identity(stream, episode),
                'blueprint.training-input.identity: wrong episode input record')
        require(record['contract_sha256'] == digest(self.contract), 'blueprint.training-input.contract: changed generation contract')
        for bound in record['files'].values():
            checked(bound, self.root)
        require(0 < len(record['attempts']) <= self.options['max_attempts'],
                'blueprint.training-input.attempts: invalid generation attempt coverage')
        for index, attempt in enumerate(record['attempts']):
            generation = read(checked(attempt['generation'], self.root))
            require(attempt['seed'] == record['identity']['first_seed'] + index == generation['generation_seed']
                    and attempt['status'] == generation['status'],
                    'blueprint.training-input.attempts: generation identity/status mismatch')
        require(record['generation_seed'] == record['attempts'][-1]['seed']
                and record['attempts'][-1]['status'] == 'accepted' and generation['draw'] == record['draw'],
                'blueprint.training-input.attempts: accepted draw mismatch')
        payload = read(checked(record['files']['input'], self.root))
        require(digest(payload) == record['input_sha256'], 'blueprint.training-input.hash: episode payload changed')
        return payload, dict(reference=deepcopy(ref), identity=record['identity'],
            generation_seed=record['generation_seed'], draw=record['draw'],
            input_sha256=record['input_sha256'], request_sha256=record['request_sha256'],
            geometry_sha256=record['geometry_sha256'], blueprint_sha256=record['blueprint_sha256'],
            setup_seconds=record['setup_seconds'], attempts=record['attempts'])

    def acquire(self, stream, episode):
        identity = self.identity(stream, episode)
        self.root.mkdir(parents=True, exist_ok=True)
        # Hold the shared lock through publication. Paired arms consume exactly
        # the same actual input, including any non-replayable setup outcomes.
        with (self.root / '.generation.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            manifest = self.root / 'manifest.json'
            if manifest.exists():
                require(read(manifest) == self.contract, 'blueprint.training-input.store: incompatible input directory')
            else:
                require(not any(self.root.iterdir()) or set(p.name for p in self.root.iterdir()) == {'.generation.lock'},
                        'blueprint.training-input.store: unowned nonempty directory')
                write(manifest, self.contract)
            directory = self.root / f'episode-{identity["slot"]:08d}'
            completed = directory / 'episode.json'
            if completed.exists():
                return self.load(reference(completed, self.root), stream, episode)
            require(not directory.exists(), f'blueprint.training-input.incomplete: preserve interrupted input at {directory}')
            directory.mkdir()
            began = time.monotonic()
            def guard():
                require(time.monotonic() - began < self.options['episode_setup_seconds'],
                        f'blueprint.training-input.deadline: episode setup exceeded bound at {directory}')
            attempts = []
            try:
                if self.native is None:
                    from native_factory_runtime import FactoryRuntime
                    catalogs = self.root / 'catalogs.json'
                    write(catalogs, self.spec['catalogs'])
                    self.native = FactoryRuntime(self.spec['native_library'], catalogs)
                from random_factory_generation import generate
                for attempt in range(self.options['max_attempts']):
                    guard()
                    generation_seed = identity['first_seed'] + attempt
                    case, generation = generate(self.native, self.spec, generation_seed, directory / f'attempt-{attempt}', guard)
                    attempts.append(dict(seed=generation_seed, status=generation['status'],
                        generation=reference(directory / f'attempt-{attempt}/generation.json', self.root)))
                    if case is None:
                        continue
                    # Freeze the new blueprint only after randomized complete
                    # factory generation. Setup may change the original wiring;
                    # policy actions may never change this admitted blueprint.
                    session = self.module.BlueprintEditSession.open(str(ROOT), json.dumps(self.blueprint_spec['catalogs']),
                        Path(case['request']).read_text(), Path(case['initial']).read_text(),
                        json.dumps(self.blueprint_spec['config']))
                    native = json.loads(session.checkpoint_json())
                    payload = dict(request=native['request'], original=native['original'])
                    write(directory / 'input.json', payload)
                    files = {k: reference(Path(case[k]), self.root) for k in ('request', 'initial', 'original')}
                    files['input'] = reference(directory / 'input.json', self.root)
                    row = dict(schema='blueprint-fresh-episode-v1', identity=identity, contract_sha256=digest(self.contract),
                        generation_seed=generation_seed, draw=generation['draw'], files=files,
                        input_sha256=digest(payload), request_sha256=digest(native['request']),
                        geometry_sha256=generation['geometry_sha256'], blueprint_sha256=native['blueprint_sha256'],
                        attempts=attempts, setup_seconds=time.monotonic() - began)
                    write(completed, row)
                    return self.load(reference(completed, self.root), stream, episode)
                raise ValueError(f'blueprint.training-input.exhausted: {len(attempts)} generation attempts at {directory}')
            except BaseException as error:
                write(directory / 'failure.json', dict(status='failed', identity=identity, attempts=attempts,
                    diagnostic=dict(code='blueprint.training-input.failed', severity='error', stage='training-input',
                                    entity=str(directory), message=str(error))))
                raise
