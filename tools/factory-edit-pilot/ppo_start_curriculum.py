"""Durable previous-batch best factories for fresh on-policy PPO trajectories."""
from collections import Counter
from copy import deepcopy
import gzip
import json
from pathlib import Path
import random
import time

from ppo_harness_state import (require, read, write, sha, mkdir, manifest, checked_path,
                               owned, atomic_file, sync_ancestors)


SCHEMA = 'ppo-best-state-starts-v1'


def checked_input(root, reference):
    path = owned(root, reference['path'])
    require(path.is_file(), 'curriculum-missing', 'committed curriculum input is missing', 'curriculum', path)
    return checked_path(root, reference)


def validate(config):
    require(isinstance(config, dict) and set(config) == {'schema', 'percentages', 'selection_seed'}
            and config['schema'] == SCHEMA and config['percentages'] == [80, 10, 10]
            and all(type(n) is int for n in config['percentages'])
            and type(config['selection_seed']) is int and 0 <= config['selection_seed'] < 2**48,
            'curriculum-config', 'expected schema ppo-best-state-starts-v1, percentages [80,10,10] and integer selection_seed',
            'prepare', 'start_curriculum')


def quotas(attempts, batch):
    require(type(batch) is int and batch >= 1, 'curriculum-batch', 'positive training batch required', 'curriculum', batch)
    q = lambda b: attempts * b // 10 - attempts * (b - 1) // 10 if b > 0 else 0
    one, two = q(batch), q(batch - 1)
    return {0: attempts - one - two, 1: one, 2: two}


def choose(config, job, batch, attempts, results):
    """Select donors without scores, replacement, global RNG or arrival order."""
    requested = quotas(attempts, batch)
    pools = {0: [], 1: []}
    for result in sorted(results, key=lambda r: r['identity']['index']):
        identity = result['identity']
        require(identity['job'] == job and identity['phase'] == 'training' and identity['batch'] == batch - 1,
                'curriculum-parent', 'donors must belong to this job and the immediately preceding training batch',
                'curriculum', identity)
        if result['status'] != 'complete':
            continue
        require(result['row']['budget_complete'], 'curriculum-parent', 'incomplete episode cannot donate', 'curriculum', identity)
        depth = result['generation']['start_curriculum']['depth']
        require(depth in (0, 1, 2), 'curriculum-depth', 'parent depth must be 0, 1 or 2', 'curriculum', identity)
        if depth < 2:
            pools[depth].append(result)
    slots = [dict(depth=0, requested_depth=0) for _ in range(requested[0])]
    diagnostics = []
    for depth in (1, 2):
        pool = pools[depth - 1]
        count = min(requested[depth], len(pool))
        rng = random.Random(f'{config["selection_seed"]}:{job}:{batch}:donors:{depth}')
        for parent in rng.sample(pool, count):
            slots.append(dict(depth=depth, requested_depth=depth, donor=parent))
        missing = requested[depth] - count
        if missing:
            bootstrap = batch <= depth
            code = 'curriculum-bootstrap' if bootstrap else 'curriculum-donor-shortage'
            diagnostics.append(dict(code='ppo-harness.' + code, severity='info' if bootstrap else 'warning',
                                    stage='curriculum', entity=f'{job}/training-{batch}/depth-{depth}',
                                    message=f'{missing} depth-{depth} slots use fresh starts because only {count} complete donors are available'))
            slots.extend(dict(depth=0, requested_depth=depth, fallback=code) for _ in range(missing))
    random.Random(f'{config["selection_seed"]}:{job}:{batch}:slots').shuffle(slots)
    return dict(schema=SCHEMA, job=job, batch=batch, requested_counts={str(k): v for k, v in requested.items()},
                actual_counts=dict(Counter(str(s['depth']) for s in slots)), diagnostics=diagnostics, slots=slots)


def phase_plan(root, protocol, job, phase):
    config = protocol['config']
    if phase['kind'] != 'training' or 'start_curriculum' not in config:
        return None
    if phase['batch'] == 1:
        return choose(config['start_curriculum'], job['id'], 1, config['training']['attempts'], [])
    parent = next((r for r in reversed(job['records']) if r['phase']['kind'] == 'training'), None)
    require(parent is not None and parent['phase']['batch'] == phase['batch'] - 1,
            'curriculum-parent', 'previous committed training batch is missing', 'curriculum', job['id'])
    record = read(checked_input(root, parent))
    require('next_training_starts' in record, 'curriculum-plan', 'committed parent lacks a start plan', 'curriculum', parent)
    plan = read(checked_input(root, record['next_training_starts']))
    require(plan['schema'] == SCHEMA and plan['job'] == job['id'] and plan['batch'] == phase['batch']
            and len(plan['slots']) == config['training']['attempts'],
            'curriculum-plan', 'committed plan differs from the requested training phase', 'curriculum', parent)
    return plan


def save_next_plan(root, directory, config, job, batch, results):
    """Copy only selected donors before phase commit/ordinary output cleanup."""
    plan = choose(config['start_curriculum'], job, batch + 1, config['training']['attempts'], results)
    require(not directory.exists(), 'curriculum-plan', 'carry-forward directory must be fresh', 'curriculum', directory)
    mkdir(directory)
    for index, slot in enumerate(plan['slots']):
        donor = slot.pop('donor', None)
        if donor is None:
            continue
        identity, generation, row = donor['identity'], donor['generation'], donor['row']
        source = checked_input(root, dict(path=donor['result_path'], sha256=donor['result_sha256']))
        parent_start = generation['start_curriculum']
        lineage = parent_start['root']
        require(lineage['batch'] == batch + 1 - slot['depth'], 'curriculum-lineage',
                'inherited start must descend from consecutive batches', 'curriculum', identity)
        target = directory / f'slot-{index:04d}'
        mkdir(target)
        inputs = {'request': owned(root, generation['case']['request']),
                  'original': owned(root, generation['case']['original']),
                  'best': owned(root, Path(row['directory']) / 'best.json.gz')}
        for key in ('request', 'original'):
            path = inputs[key]
            require(sha(path) == generation['case']['files_sha256'][path.name], 'curriculum-input',
                    'parent obligations changed before carry-forward', 'curriculum', path)
        best = json.loads(gzip.decompress(inputs['best'].read_bytes()))
        from full_factory_environment import digest
        require(read(source)['row'] == row and digest(best) == row['best_sha256'],
                'curriculum-best', 'parent best geometry differs from its completed episode', 'curriculum', source)
        files = {}
        for key, path in inputs.items():
            copied = target / ('best.json.gz' if key == 'best' else key + '.json')
            data = path.read_bytes()
            atomic_file(copied, lambda stream, data=data: stream.write(data))
            files[key] = manifest(root, copied)
        slot.update(files=files, expected_metrics=row['best_metrics'], best_sha256=row['best_sha256'], root=lineage, draw=generation['draw'],
                    constructor_geometry_sha256=generation['constructor_geometry_sha256'],
                    parent=dict(job=job, batch=batch, index=identity['index'], depth=parent_start['depth'],
                                result=manifest(root, source), policy_sha256=donor['policy_sha256']))
    path = directory / 'manifest.json'
    write(path, plan)
    sync_ancestors(path, root)
    return manifest(root, path)


def annotate_fresh(generation, task):
    start = deepcopy(task['initial_start'])
    require(start['depth'] == 0, 'curriculum-depth', 'fresh generation requires depth zero', 'curriculum', task['identity'])
    if generation['status'] == 'accepted':
        start['root'] = dict(batch=task['identity']['batch'], generation_seed=task['generation_seed'],
                             initial_area=generation['initial_metrics']['score'][0],
                             initial_sha256=generation['initial_sha256'], request_sha256=generation['request_sha256'])
    generation['start_curriculum'] = start


def materialize(root, runtime, spec, task, directory, guard):
    from random_factory_generation import geometry_identity
    from full_factory_environment import digest
    began = time.monotonic()
    start = deepcopy(task['initial_start'])
    parent = start['parent']
    require(task['training'] and start['depth'] in (1, 2) and parent['job'] == task['job']
            and parent['batch'] == task['identity']['batch'] - 1 and parent['depth'] == start['depth'] - 1
            and start['root']['batch'] == task['identity']['batch'] - start['depth'],
            'curriculum-parent', 'inherited task has invalid ancestry', 'curriculum', task['identity'])
    guard()
    directory.mkdir()
    case = dict(id=f'inherited-{task["generation_seed"]}', dialect='closure-factory', files_sha256={})
    witness = None
    for key in ('request', 'original', 'best'):
        data = checked_input(root, start['files'][key]).read_bytes()
        target_key = 'initial' if key == 'best' else key
        if key == 'best':
            data = gzip.decompress(data)
            witness = json.loads(data)
        target = directory / f'{target_key}.json'
        atomic_file(target, lambda stream, data=data: stream.write(data))
        case[target_key] = str(target)
        case['files_sha256'][target.name] = sha(target)
    case['instances'] = len(read(case['request'])['instances'])
    require(sha(case['request']) == start['root']['request_sha256'], 'curriculum-request',
            'inherited factory obligations changed', 'curriculum', case['request'])
    admitted = time.monotonic()
    check = runtime.open_episode(case, 1, spec['generation']['setup_seconds'], spec['selection']['route_work'])
    normalized = json.loads(check.export_layout('current'))
    require(check.summary()['initial_metrics'] == start['expected_metrics'] and
            digest(normalized) == digest(witness) == start['best_sha256'],
            'curriculum-admission', 'admitted start differs from the parent best state', 'curriculum', task['identity'])
    reference = directory / 'reference.json'
    write(reference, dict(schema_version=2, backend=runtime.metadata['backend'], factory_validated=True,
                          request_sha256=sha(case['request']), metrics=check.summary()['initial_metrics']))
    case['files_sha256'][reference.name] = sha(reference)
    admission = directory / 'admission'
    admission.mkdir()
    geometry = geometry_identity(normalized)
    generation = dict(schema_version=2, backend=runtime.metadata['backend'], generation_seed=task['generation_seed'],
        status='accepted', admission_status='accepted', case=case, draw=start['draw'], start_curriculum=start,
        request_sha256=sha(case['request']), initial_sha256=sha(case['initial']), geometry_sha256=geometry,
        constructor_geometry_sha256=start['constructor_geometry_sha256'],
        geometry_changed=geometry != start['constructor_geometry_sha256'],
        initial_metrics=check.summary()['initial_metrics'], setup_seconds=0., setup_selected=0,
        setup_kinds={}, setup_statuses={}, actual_start_is_current=True,
        admission_seconds=time.monotonic() - admitted, seconds=time.monotonic() - began,
        original_attempt=dict(directory=str(admission), status='accepted', factory_validated=True, diagnostics=[]))
    write(directory / 'generation.json', generation)
    guard()
    return case, generation
