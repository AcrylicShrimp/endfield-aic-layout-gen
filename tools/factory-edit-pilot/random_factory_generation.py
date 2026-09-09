"""Generate a fresh complete request and independently validated randomized start."""
from collections import Counter
from copy import deepcopy
import json
from pathlib import Path
import random
import subprocess
import time

from full_factory_environment import ROOT, FactoryEpisode, digest
from full_factory_screen import sha
from full_factory_selection import SelectionMemory, action_key, physical_key


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


def invoke(arguments, directory, name, seconds):
    began = time.monotonic()
    result = subprocess.run(arguments, capture_output=True, text=True, timeout=seconds, check=False)
    (directory / f'{name}.stdout').write_text(result.stdout)
    (directory / f'{name}.stderr').write_text(result.stderr)
    if result.returncode:
        raise RuntimeError(f'random-factory.command: {name}, exit {result.returncode}, {directory}')
    return time.monotonic() - began


def import_root(spec, roots, directory):
    output = directory / 'input'
    invoke(['node', str(Path(__file__).with_name('full_factory_inputs.mjs')), '--roots', str(roots),
            '--validator', spec['validator'], '--output-directory', str(output)],
           directory, 'import', 30)
    inputs = read(output / 'inputs.json')
    if len(inputs['rows']) != 1:
        raise ValueError(f'random-factory.import-count: {roots}')
    return inputs['rows'][0]


def generate(runtime, spec, seed, directory, guard):
    started = time.monotonic()
    directory = Path(directory)
    directory.mkdir()
    chosen = draw(spec, seed)
    write(directory / 'draw.json', dict(schema_version=1, generation_seed=seed, **chosen))
    generation = spec['generation']
    study = dict(schema_version=1, cli=spec['cli'], validator=spec['validator'], catalogs=spec['catalogs'],
        **{key: generation[key] for key in ('flow_elapsed_ms', 'validator_ms', 'max_facilities', 'construction')},
        families=[dict(id='random-factory', split='train',
            sources=[dict(id=chosen['product'], path=chosen['source'])], rates=[chosen['rate']])])
    write(directory / 'admission-study.json', study)
    guard()
    admission_seconds = invoke(['node', str(Path(__file__).with_name('full_factory_families.mjs')),
        '--study', str(directory / 'admission-study.json'), '--output-directory', str(directory / 'admission')],
        directory, 'admission', 60)
    admission = read(directory / 'admission/report.json')
    attempt = admission['rows'][0]
    record = dict(schema_version=1, generation_seed=seed, draw=chosen, admission_seconds=admission_seconds,
                  admission_status=attempt['status'], original_attempt=attempt)
    if attempt['status'] != 'accepted':
        record.update(status='rejected', seconds=time.monotonic() - started)
        write(directory / 'generation.json', record)
        return None, record
    guard()
    base = directory / 'constructor-input'
    base.mkdir()
    case = import_root(spec, directory / 'admission/roots.json', base)
    setup_started = time.monotonic()
    env = FactoryEpisode(runtime, case, chosen['setup_steps'], generation['setup_seconds'], spec['selection']['route_work'])
    constructor_geometry = geometry_identity(env.state['current'])
    memory = SelectionMemory(env.state['current'])
    rng = random.Random(chosen['setup_seed'])
    for _ in range(chosen['setup_steps']):
        guard()
        description = env.describe(generation['setup_options'])
        before = physical_key(env.state['current'])
        keys = [action_key(action, description, env.state['current'], physical_key(env.state['best']))
                for action in description['actions']]
        allowed, _ = memory.mask(before, keys, description['actions'], True)
        kinds = sorted({action['kind'] for action, ok in zip(description['actions'], allowed)
                        if ok and action['kind'] in generation['setup_kinds']})
        if not kinds:
            raise ValueError(f'random-factory.setup-exhausted: {directory}')
        kind = rng.choice(kinds)
        index = rng.choice([i for i, (action, ok) in enumerate(zip(description['actions'], allowed))
                            if ok and action['kind'] == kind])
        step = env.step(description['actions'][index])
        if step['status'] == 'late':
            raise TimeoutError(f'random-factory.setup-late: {directory}')
        memory.record(before, physical_key(env.state['current']), keys[index])
        write(directory / 'setup-progress.json', env.summary())
    roots = read(directory / 'admission/roots.json')
    roots['roots'][0]['id'] = f'generated-{seed}'
    roots['roots'][0]['witness'] = deepcopy(env.state['current'])
    write(directory / 'randomized-roots.json', roots)
    write(directory / 'setup.json', env.summary())
    (directory / 'setup-progress.json').unlink()
    setup_seconds = time.monotonic() - setup_started
    guard()
    # Re-run independent original-graph and geometry acceptance on the actual
    # randomized current state; the setup incumbent is not the episode reset.
    final = directory / 'randomized-input'
    final.mkdir()
    case = import_root(spec, directory / 'randomized-roots.json', final)
    check = FactoryEpisode(runtime, case, 1, generation['setup_seconds'], spec['selection']['route_work'])
    geometry = geometry_identity(check.state['current'])
    record.update(status='accepted', case=case, request_sha256=sha(case['request']),
        initial_sha256=sha(case['initial']), geometry_sha256=geometry,
        constructor_geometry_sha256=constructor_geometry, geometry_changed=geometry != constructor_geometry,
        initial_metrics=check.state['initial_metrics'], setup_seconds=setup_seconds,
        setup_selected=env.selected, setup_kinds=dict(Counter(t['action']['kind'] for t in env.trace)),
        setup_statuses=dict(Counter(t['status'] for t in env.trace)),
        actual_start_is_current=digest(check.state['current']) == digest(env.state['current']),
        seconds=time.monotonic() - started)
    if not record['actual_start_is_current']:
        raise ValueError(f'random-factory.reset-identity: {directory}')
    write(directory / 'generation.json', record)
    return case, record
