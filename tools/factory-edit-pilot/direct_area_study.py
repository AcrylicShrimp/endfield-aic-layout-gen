"""Freeze and run an audited direct-area reward comparison against an external rank pilot."""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import shutil
import signal
import time

import torch

from continue_fresh_factory_learning import cleanup_construction
from direct_area_audit import native_batch, physics_signature, replay, transition, verify_checkpoint
from direct_area_reward import SCHEMA, require, update
from full_factory_environment import FactoryRuntime, ROOT
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from random_factory_audit import optimizer_hash
from random_factory_collection import Pool, check_sources
from random_factory_generation import read, write, resolve_spec
from random_factory_pilot import configure_torch, new_model, checkpoint, tasks, describe_results


def manifest(path):
    return dict(path=str(Path(path).resolve()), sha256=sha(path))


def checked(record):
    require(sha(record['path']) == record['sha256'], record['path'])
    return read(record['path'])


def prepare(args):
    configure_torch()
    controls = read(args.config)
    require(controls['schema_version'] == 1 and controls['reward_schema'] == SCHEMA, 'study configuration schema')
    spec = resolve_spec(ROOT / controls['distribution'])
    source_path = args.reference / 'protocol.json'
    source = read(source_path)
    check_sources(source)
    audit_path = args.reference / 'audit/report.json'
    audit = read(audit_path)
    require(audit['status'] == 'complete' and not audit['smoke'] and
            audit['protocol_sha256'] == sha(source_path), 'audited full rank reference')
    learning_path = args.reference / 'learning/report.json'
    require(audit['learning_sha256'] == sha(learning_path), 'audited reference learning report')
    # Paths differ across isolated checkouts; actual generator/native bytes must match.
    for name in ('generation', 'policy', 'selection', 'learning'):
        require(spec[name] == source['spec'][name], f'unchanged {name} settings')
    for name in ('native_library', 'cli', 'validator'):
        require(sha(spec[name]) == sha(source['spec'][name]), f'unchanged {name} bytes')
    require({k: sha(v) for k, v in spec['catalogs'].items()} ==
            {k: sha(v) for k, v in source['spec']['catalogs'].items()}, 'unchanged catalog bytes')
    require(len(spec['products']) == len(source['spec']['products']) and all(
        a['id'] == b['id'] and a['duration_ms'] == b['duration_ms'] and sha(a['source']) == sha(b['source'])
        for a, b in zip(spec['products'], source['spec']['products'])), 'unchanged product sources')
    reference = {str(record['seed']): entry for entry in read(learning_path)['rows']
                 for record in [checked(entry)]}
    args.output.mkdir()
    if args.smoke:
        spec['generation']['setup_steps'] = [2, 2]
        spec['learning'].update(groups=2, batch_attempts=4, horizon=2, seeds=[17], evaluation_attempts=4)
        controls.update(workers=2, evaluate_at=[0, 1, 2], study_seconds=600)
    spec['learning']['evaluate_at'] = controls['evaluate_at']
    require(controls['evaluate_at'][0] == 0 and controls['evaluate_at'][-1] == spec['learning']['groups']
            and controls['evaluate_at'] == sorted(set(controls['evaluate_at']))
            and 0 < controls['study_seconds'] <= 7200 and 1 <= controls['workers'] <= 64, 'study bounds')
    catalogs = args.output / 'catalogs.json'
    write(catalogs, spec['catalogs'])
    old_root = Path(source['spec']['cli']).parents[2]
    paths = set()
    for name in source['sources']:
        original = Path(name)
        if original.is_relative_to(old_root):
            local = ROOT / original.relative_to(old_root)
            if local.is_file():
                paths.add(local)
    paths.update(Path(__file__).parent.glob('*.py'))
    paths.update([catalogs, args.config, ROOT / controls['distribution'], source_path, audit_path, learning_path])
    paths.update(Path(r['path']) for r in reference.values())
    protocol = dict(schema=SCHEMA, smoke=args.smoke, spec=spec, controls=controls,
        catalog_file=str(catalogs.resolve()), reference=reference,
        sources={str(p.resolve()): sha(p) for p in sorted(paths)},
        host=dict(node=os.uname().nodename, cpus=os.cpu_count(), torch=torch.__version__))
    write(args.output / 'protocol.json', protocol)
    print(json.dumps(dict(status='frozen', smoke=args.smoke, workers=controls['workers'])), flush=True)


def parity(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    spec = protocol['spec']
    directory = args.screen / 'parity'
    directory.mkdir()
    initial = checkpoint(directory / 'initial.pt', new_model(spec, 17))
    signatures, cells = [], []
    runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
    try:
        for workers in (1, 2):
            cell = directory / f'workers-{workers}'
            cell.mkdir()
            with Pool(protocol_path, workers, cell / 'pool', Path(__file__).with_name('direct_area_worker.py')) as pool:
                jobs = tasks(cell / 'batch', initial, [98000001 + i for i in range(4)],
                             [99000001 + i for i in range(4)], 2, True, 'parity')
                results, timing = pool.collect(jobs)
                proof = native_batch(runtime, results, cell / 'audit')
                model = new_model(spec, 17)
                optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
                accounting = transition(model, optimizer, results, spec['learning'])
                signatures.append(dict(physics=[physics_signature(r) for r in results],
                    gradients=[r.get('gradient_values_sha256') for r in results], accounting=accounting))
                sample = next(r for r in results if r['status'] == 'complete')
                replayed = replay(runtime, protocol, sample, cell / 'replay', lambda: None)
                retention = cleanup_construction(results, cell)
                cells.append(dict(workers=workers, proof=proof, replay=replayed, timing=timing, retention=retention))
        require(signatures[0] == signatures[1], 'serial/parallel direct reward parity')
        check_sources(protocol)
        write(directory / 'report.json', dict(status='complete', protocol_sha256=sha(protocol_path), cells=cells,
            exact_physics_sampling_gradients_and_updates=True))
    finally:
        runtime.close()
    print(json.dumps(dict(status='parity-passed', workers=[1, 2])), flush=True)


def run(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    proof = read(args.screen / 'parity/report.json')
    require(proof['status'] == 'complete' and proof['protocol_sha256'] == sha(protocol_path), 'passed same-protocol parity')
    spec, controls = protocol['spec'], protocol['controls']
    require(not (args.screen / 'progress.json').exists(), 'fresh study execution')
    began = time.monotonic()
    deadline = began + controls['study_seconds']
    progress = dict(schema=SCHEMA, smoke=protocol['smoke'], status='running', started_epoch=time.time(),
                    protocol_sha256=sha(protocol_path), seeds=[], updates=0, selected=0)
    runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
    def publish():
        progress['seconds'] = time.monotonic() - began
        progress['updated_epoch'] = time.time()
        write(args.screen / 'progress.json', progress)
    def guard():
        if time.monotonic() > deadline:
            raise TimeoutError('direct-reward.run-budget')
        require(shutil.disk_usage(args.screen).free > controls['disk_reserve_bytes'], 'disk reserve')
    try:
        publish()
        with Pool(protocol_path, controls['workers'], args.screen / 'pool', Path(__file__).with_name('direct_area_worker.py')) as pool:
            for seed in spec['learning']['seeds']:
                directory = args.screen / f'seed-{seed}'
                directory.mkdir()
                baseline = checked(protocol['reference'][str(seed)])
                model = new_model(spec, seed)
                require(parameter_hash(model) == baseline['checkpoints'][0]['policy_sha256'], 'matched untrained actor')
                optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
                version = checkpoint(directory / 'initial.pt', model, optimizer, seed=seed, next_group=0,
                                     protocol_sha256=sha(protocol_path), reward_schema=SCHEMA)
                state = dict(seed=seed, started_epoch=time.time(), status='running', groups=[], evaluations=[],
                             checkpoint=version, selected=0)
                progress['seeds'].append(state)
                for group in range(spec['learning']['groups'] + 1):
                    guard()
                    check_sources(protocol)
                    if group in controls['evaluate_at']:
                        n = spec['learning']['evaluation_attempts']
                        batch = directory / f'evaluation-{group:04d}'
                        jobs = tasks(batch, version,
                            [spec['learning']['evaluation_seed_base'] + seed * 100000 + i for i in range(n)],
                            [spec['learning']['evaluation_seed_base'] + 10000000 + seed * 100000 + i for i in range(n)],
                            spec['learning']['horizon'], False, f'evaluation-{seed}-{group}')
                        results, timing = pool.collect(jobs, deadline)
                        audit = native_batch(runtime, results, batch / 'audit')
                        if group == 0 and not protocol['smoke']:
                            require([physics_signature(r) for r in results] ==
                                    [physics_signature(r) for r in baseline['evaluations'][0]['results']],
                                    'exact reference initial evaluation trajectories')
                        if group in (0, spec['learning']['groups']):
                            sample = next(r for r in results if r['status'] == 'complete')
                            audit['replay'] = replay(runtime, protocol, sample, batch / 'replay', guard)
                        retention = cleanup_construction(results, batch)
                        record = dict(update=group, policy_sha256=version['policy_sha256'], results=results,
                            selected_training_actions=state['selected'], elapsed_seconds=time.time() - state['started_epoch'],
                            measured_epoch=time.time(), timing=timing, audit=audit, retention=retention,
                            **describe_results(results))
                        path = directory / f'evaluation-{group:04d}.json'
                        write(path, record)
                        state['evaluations'].append(dict(**manifest(path), update=group))
                        publish()
                        print(json.dumps(dict(stage='evaluation', seed=seed, update=group, accepted=record['accepted'])), flush=True)
                    if group == spec['learning']['groups']:
                        break
                    n = spec['learning']['batch_attempts']
                    batch = directory / f'group-{group:04d}'
                    jobs = tasks(batch, version,
                        [spec['learning']['generation_seed_base'] + seed * 100000 + group * n + i for i in range(n)],
                        [spec['learning']['action_seed_base'] + seed * 100000 + group * n + i for i in range(n)],
                        spec['learning']['horizon'], True, f'training-{seed}-{group}')
                    results, timing = pool.collect(jobs, deadline)
                    audit = native_batch(runtime, results, batch / 'audit')
                    if group == 0:
                        if not protocol['smoke']:
                            require([physics_signature(r) for r in results] ==
                                    [physics_signature(r) for r in baseline['groups'][0]['results']],
                                    'exact reference first-batch trajectories')
                        sample = next(r for r in results if r['status'] == 'complete')
                        audit['replay'] = replay(runtime, protocol, sample, batch / 'replay', guard)
                    accounting = transition(model, optimizer, results, spec['learning'])
                    version = checkpoint(batch / 'checkpoint.pt', model, optimizer, seed=seed, next_group=group + 1,
                                         protocol_sha256=sha(protocol_path), reward_schema=SCHEMA)
                    verify_checkpoint(version['path'], model, optimizer)
                    retention = cleanup_construction(results, batch)
                    record = dict(group=group, **accounting, checkpoint=version, audit=audit, retention=retention,
                                  timing=timing, results=results, **describe_results(results))
                    path = batch / 'group.json'
                    write(path, record)
                    state['groups'].append(dict(**manifest(path), group=group))
                    state.update(checkpoint=version, selected=state['selected'] + record['selected'])
                    progress['updates'] += 1
                    progress['selected'] += record['selected']
                    publish()
                    print(json.dumps(dict(stage='learning', seed=seed, update=group + 1,
                        reward_gradient=accounting['reward_gradient_norm'], zero_reward=accounting['zero_reward_episodes'])), flush=True)
                state['status'] = 'complete'
                write(directory / 'report.json', state)
                publish()
        check_sources(protocol)
        progress['status'] = 'complete'
        publish()
        write(args.screen / 'report.json', progress)
    except BaseException as error:
        progress.update(status='incomplete', diagnostic=dict(code='direct-reward.run-failed', severity='error',
            stage='learning', entity=str(args.screen), message=str(error)))
        publish()
        raise
    finally:
        runtime.close()


if __name__ == '__main__':
    def interrupted(signum, _):
        raise InterruptedError(f'direct-reward.interrupted: {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    freeze = commands.add_parser('prepare')
    freeze.add_argument('--config', type=Path, required=True)
    freeze.add_argument('--reference', type=Path, required=True)
    freeze.add_argument('--output', type=Path, required=True)
    freeze.add_argument('--smoke', action='store_true')
    for name in ('parity', 'run'):
        commands.add_parser(name).add_argument('--screen', type=Path, required=True)
    args = parser.parse_args()
    dict(prepare=prepare, parity=parity, run=run)[args.command](args)
