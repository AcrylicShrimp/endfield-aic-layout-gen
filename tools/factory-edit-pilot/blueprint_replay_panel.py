"""Freeze a product-selected transfer panel before observing any child policy."""
from concurrent.futures import ProcessPoolExecutor
from copy import deepcopy
import json
import multiprocessing
import os
from pathlib import Path
import time

import torch

from blueprint_ppo_store import ROOT, read, reference, sha, write
from blueprint_replay_contract import Sources, check
from full_factory_selection import physical_key
from random_factory_generation import generate, geometry_identity, resolve_spec


def identities(initial):
    return physical_key(initial), geometry_identity(initial)


def excluded_inputs(parent, imports, historical):
    source = Sources()
    root = Path(parent['parent_protocol']['config']['training_input']['directory'])
    records = sorted(root.glob('episode-*/episode.json'))
    check(len(records) == 2048, 'evaluation-contaminated', 'transfer-preparation', 'old input bank coverage differs', root)
    exact, geometry = set(), set()
    for path in records:
        record = read(source.path(path))
        payload = read(source.reference(root, record['files']['input']))
        e, g = identities(payload['original'])
        exact.add(e)
        geometry.add(g)
    for case in historical:
        e, g = identities(read(source.reference(Path(case['directory']), case['files']['initial'])))
        exact.add(e)
        geometry.add(g)
    for sequence in imports:
        # The saved pre-label native state retains the exact original factory.
        e, g = identities(sequence['anchor_states'][0]['native']['original'])
        exact.add(e)
        geometry.add(g)
    return exact, geometry, source.bindings


def candidate(config, library, output, index, product, attempt, options, core):
    from native_factory_runtime import FactoryRuntime
    from native import load_native
    os.sched_setaffinity(0, {core})
    torch.set_num_threads(1)
    directory = Path(output) / f'case-{index:02d}'
    directory.mkdir(exist_ok=True)
    spec = resolve_spec(ROOT / config['training_input']['spec'])
    spec['native_library'] = library
    spec['products'] = [p for p in spec['products'] if p['id'] == product]
    check(len(spec['products']) == 1, 'source-invalid', 'transfer-preparation', 'unknown fixed product', product)
    catalogs = directory / 'catalogs.json'
    write(catalogs, spec['catalogs'])
    runtime = FactoryRuntime(library, catalogs)
    began = time.monotonic()
    def guard():
        check(time.monotonic() - began < config['training_input']['episode_setup_seconds'],
            'resource-exhausted', 'transfer-preparation', 'panel generation exceeded setup allowance', directory)
    seed = options['seed_base'] + index * options['max_attempts'] + attempt
    case, report = generate(runtime, spec, seed, directory / f'attempt-{attempt}', guard)
    result = dict(index=index, product=product, attempt=attempt, generation_seed=seed,
        status=report['status'], generation=reference(directory / f'attempt-{attempt}/generation.json', output))
    if case is None:
        return result
    module = load_native(library)
    probe = read(ROOT / config['probe_spec'])
    session = module.BlueprintEditSession.open(str(ROOT), json.dumps(probe['catalogs']),
        Path(case['request']).read_text(), Path(case['initial']).read_text(), json.dumps(probe['config']))
    native = json.loads(session.checkpoint_json())
    observation = json.loads(session.observation_json())
    exact, geometry = identities(native['original'])
    result.update(exact_identity=exact, geometry_identity=geometry,
        case=dict(generation_seed=seed, product=product, initial_metrics=observation['initial_metrics'],
            files={k: reference(Path(case[k]), output) for k in ('request', 'initial', 'original')},
            exact_identity=exact, geometry_identity=geometry, directory=str(output)))
    return result


def prepare_panel(parent, imports, historical, output, options, cores):
    output = Path(output).resolve()
    check(not output.exists(), 'source-invalid', 'transfer-preparation', 'panel output already exists', output)
    output.mkdir(parents=True)
    exact, geometry, sources = excluded_inputs(parent, imports, historical)
    products = [p for p, n in options['products'].items() for _ in range(n)]
    attempts, accepted = [], {}
    with ProcessPoolExecutor(max_workers=len(cores), mp_context=multiprocessing.get_context('spawn')) as pool:
        for attempt in range(options['max_attempts']):
            pending = [(index, pool.submit(candidate, parent['parent_protocol']['config'],
                parent['parent_protocol']['native_library'], output, index, product, attempt, options, cores[index % len(cores)]))
                for index, product in enumerate(products) if index not in accepted]
            # Admission order is fixed and independent of worker completion order.
            for index, future in pending:
                result = future.result()
                if result['status'] == 'accepted':
                    e, g = result['exact_identity'], result['geometry_identity']
                    if e in exact or g in geometry:
                        result.update(status='rejected-identity', exact_overlap=e in exact, geometry_overlap=g in geometry)
                    else:
                        exact.add(e)
                        geometry.add(g)
                        accepted[index] = result['case']
                attempts.append(result)
                write(output / 'progress.json', dict(attempts=attempts, accepted=sorted(accepted)))
            if len(accepted) == len(products):
                break
    check(len(accepted) == 16, 'evaluation-contaminated', 'transfer-preparation',
        'panel admission exhausted without sixteen unique valid cases', sorted(accepted))
    cases = [accepted[index] for index in range(16)]
    result = dict(schema='blueprint-replay-transfer-panel-v1', cases=cases, attempts=attempts,
        excluded_source_hashes=sources, selection='product, native validity and input identity only',
        generator_sha256=sha(ROOT / parent['parent_protocol']['config']['training_input']['spec']), options=options)
    write(output / 'panel.json', result)
    return result


def evaluation_cases(panel):
    options = panel['options']
    return [dict(case, sample=replicate, action_seed=options['action_seed_base'] + 100 * index + replicate)
        for index, case in enumerate(panel['cases']) for replicate in range(options['replicates'])]
