"""Generate a fresh complete request and independently validated randomized start."""
from collections import Counter
import json
from pathlib import Path
import random
import time

from native_factory_runtime import ROOT
from full_factory_environment import digest
from native_factory_admission import admit, accept_case
from full_factory_screen import sha
from full_factory_selection import physical_key


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    path = Path(path)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


def geometry_identity(witness):
    """Native-normalized geometry, excluding utilization and transport flow values."""
    def without_rates(value):
        if isinstance(value, dict):
            return {key: without_rates(item) for key, item in value.items()
                    if key not in ('rate', 'utilizations')}
        if isinstance(value, list):
            return [without_rates(item) for item in value]
        return value
    return physical_key(without_rates(witness))


def resolve_spec(path):
    spec = read(path)
    if spec.get('schema_version') != 1 or not spec.get('products'):
        raise ValueError(f'random-factory.config: {path}')
    for key in ('native_library', 'cli', 'validator'):
        spec[key] = str((ROOT / spec[key]).resolve())
    spec['catalogs'] = {key: str((ROOT / value).resolve()) for key, value in spec['catalogs'].items()}
    ids = set()
    step = spec['generation']['duration_step_ms']
    if type(step) is not int or step <= 0:
        raise ValueError('random-factory.rate-grid')
    for product in spec['products']:
        product['source'] = str((ROOT / product['source']).resolve())
        low, high = product['duration_ms']
        if product['id'] in ids or not 0 < low <= high <= 1000000000 or low % step or high % step:
            raise ValueError(f'random-factory.product: {product}')
        ids.add(product['id'])
    low, high = spec['generation']['setup_steps']
    if not 1 <= low <= high <= 32 or not spec['generation']['setup_kinds']:
        raise ValueError('random-factory.setup-config')
    return spec


def draw(spec, seed):
    rng = random.Random(seed)
    product = rng.choice(spec['products'])
    step = spec['generation']['duration_step_ms']
    low, high = product['duration_ms']
    return dict(product=product['id'], source=product['source'],
                rate=dict(quantity=1, duration_ms=step * rng.randint(low // step, high // step)),
                setup_steps=rng.randint(*spec['generation']['setup_steps']),
                setup_seed=rng.getrandbits(63))


def generate(runtime, spec, seed, directory, guard, *, compact=False):
    if 'frozen_starts' in spec:
        from ppo_frozen_starts import applies, materialize
        if applies(spec['frozen_starts'], seed):
            return materialize(runtime, spec, seed, directory, guard)
    started = time.monotonic()
    directory = Path(directory)
    directory.mkdir()
    chosen = draw(spec, seed)
    if not compact:write(directory / 'draw.json', dict(schema_version=1, generation_seed=seed, **chosen))
    generation = spec['generation']
    guard()
    admission_started = time.monotonic()
    case, attempt = admit(runtime, spec, chosen, directory / 'admission', guard,compact=compact)
    record = dict(schema_version=2, backend=runtime.metadata['backend'], generation_seed=seed, draw=chosen,
                  admission_seconds=time.monotonic() - admission_started,
                  admission_status=attempt['status'], original_attempt=attempt)
    if attempt['status'] != 'accepted':
        record.update(status='rejected', seconds=time.monotonic() - started)
        write(directory / 'generation.json', record)
        return None, record
    guard()
    setup_started = time.monotonic()
    env = runtime.open_episode(case, chosen['setup_steps'], generation['setup_seconds'], spec['selection']['route_work'],
                               vocabulary=generation['setup_options'])
    constructor_geometry = geometry_identity(json.loads(env.export_layout('current')))
    rng = random.Random(chosen['setup_seed'])
    try:
        for _ in range(chosen['setup_steps']):
            guard()
            view = env.candidates()
            kinds = sorted({runtime.kinds[k] for k, ok in zip(view['kind_ids'], view['allowed'])
                            if ok and runtime.kinds[k] in generation['setup_kinds']})
            if not kinds:
                raise ValueError(f'random-factory.setup-exhausted: {directory}')
            kind = rng.choice(kinds)
            index = rng.choice([i for i, (k, ok) in enumerate(zip(view['kind_ids'], view['allowed']))
                                if ok and runtime.kinds[k] == kind])
            step = env.step(view['handle'], index)
            if step['status'] == 'late':
                raise TimeoutError(f'random-factory.setup-late: {directory}')
    except BaseException:
        if compact:
            # A failed setup is not replayable from its seed alone. Preserve
            # its actual request/current/best once, without normal-step traces.
            from blueprint_ppo_store import save
            save(directory/'setup-partial.pt.gz',dict(schema='random-factory-partial-v1',case=case,
                setup=env.summary(),current=json.loads(env.export_layout('current')),
                best=json.loads(env.export_layout('best'))))
        else:
            write(directory / 'setup-partial.json', env.summary())
            write(directory / 'setup-current-partial.json', json.loads(env.export_layout('current')))
            write(directory / 'setup-best-partial.json', json.loads(env.export_layout('best')))
        raise
    setup = env.summary()
    if not compact:write(directory / 'setup.json', setup)
    current = json.loads(env.export_layout('current'))
    setup_seconds = time.monotonic() - setup_started
    guard()
    request=case['request'] if compact else read(case['request'])
    graph=case['original'] if compact else read(case['original'])
    case, check = accept_case(runtime, request, graph, current, f'generated-{seed}',
        directory / 'randomized-input', generation['setup_seconds'], spec['selection']['route_work'],memory=compact)
    normalized = json.loads(check.export_layout('current'))
    geometry = geometry_identity(normalized)
    record.update(status='accepted', request_sha256=digest(request) if compact else sha(case['request']),
        initial_sha256=digest(current) if compact else sha(case['initial']), geometry_sha256=geometry,
        constructor_geometry_sha256=constructor_geometry, geometry_changed=geometry != constructor_geometry,
        initial_metrics=check.summary()['initial_metrics'], setup_seconds=setup_seconds,
        setup_selected=env.selected, setup_kinds=dict(Counter(t['action']['kind'] for t in setup['trace'])),
        setup_statuses=dict(Counter(t['status'] for t in setup['trace'])),
        actual_start_is_current=digest(normalized) == digest(current), seconds=time.monotonic() - started)
    if not record['actual_start_is_current']:
        raise ValueError(f'random-factory.reset-identity: {directory}')
    if compact:record['schema_version']=3
    else:record['case']=case
    write(directory / 'generation.json', record)
    return case, record
