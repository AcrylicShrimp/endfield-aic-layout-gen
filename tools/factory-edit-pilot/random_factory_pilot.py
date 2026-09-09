"""Freeze, measure parallel fresh experience, and run a bounded learning check."""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import random
import re
import signal
import time

import torch

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT, digest
from full_factory_input_contracts import InputContractPolicy
from full_factory_large_learning import unpack
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from random_factory_collection import Pool, update, check_sources
from random_factory_generation import resolve_spec, read, write


SCHEMA = 'fresh-factory-experience-v1'


def configure_torch():
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)


def new_model(spec, seed):
    torch.manual_seed(seed)
    return InputContractPolicy(spec['policy']['depth'], spec['policy']['width'])


def checkpoint(path, model, optimizer=None, **metadata):
    saved = dict(schema=SCHEMA, policy=model.state_dict(), policy_sha256=parameter_hash(model),
                 rng=torch.get_rng_state(), **metadata)
    if optimizer is not None:
        saved['optimizer'] = optimizer.state_dict()
    temporary = path.with_suffix('.tmp')
    torch.save(saved, temporary)
    temporary.replace(path)
    return dict(path=str(path.resolve()), sha256=sha(path), policy_sha256=saved['policy_sha256'])


def prepare(args):
    configure_torch()
    spec = resolve_spec(args.config)
    if args.smoke:
        spec['scaling'].update(workers=[1, 2, 4], repetitions=1, attempts=8)
        spec['selection']['horizon'] = 2
        spec['generation']['setup_steps'] = [2, 2]
        spec['learning'].update(groups=2, batch_attempts=4, horizon=2, seeds=[17],
                                evaluate_at=[0, 2], evaluation_attempts=4)
    if (spec['policy']['candidate'] != 'action-only' or not spec['selection']['balanced']
            or not spec['selection']['memory'] or len(set(spec['scaling']['workers'])) != len(spec['scaling']['workers'])
            or any(type(w) is not int or not 1 <= w <= 64 for w in spec['scaling']['workers'])
            or not 2 <= spec['scaling']['attempts'] <= 256
            or not 2 <= spec['learning']['batch_attempts'] <= 256
            or not 1 <= spec['learning']['groups'] <= 256
            or spec['learning']['evaluate_at'] != sorted(set(spec['learning']['evaluate_at']))
            or spec['learning']['evaluate_at'][0] != 0
            or spec['learning']['evaluate_at'][-1] != spec['learning']['groups']):
        raise ValueError('random-pilot.config-contract')
    args.output.mkdir()
    catalog_file = (args.output / 'catalogs.json').resolve()
    write(catalog_file, spec['catalogs'])
    paths = {ROOT / name for name in source_manifest()}
    paths.update(Path(__file__).parent.glob('random_factory_*.py'))
    paths.update([args.config, catalog_file, *(Path(spec[key]) for key in ('native_library', 'cli', 'validator')),
                  *map(Path, spec['catalogs'].values()), *(Path(p['source']) for p in spec['products'])])
    pending = [Path(__file__).with_name(name) for name in ('full_factory_families.mjs', 'full_factory_inputs.mjs')]
    seen = set()
    while pending:
        path = pending.pop().resolve()
        if path in seen:
            continue
        seen.add(path)
        pending.extend((path.parent / name).resolve() for name in re.findall(r"from\s+['\"](\.[^'\"]+)['\"]", path.read_text()))
    paths.update(seen)
    protocol = dict(schema=SCHEMA, smoke=args.smoke, spec=spec, catalog_file=str(catalog_file),
        sources={str(path.resolve()): sha(path) for path in sorted(paths)},
        host=dict(node=os.uname().nodename, cpus=os.cpu_count(), torch=torch.__version__),
        scope='Fresh production/rate/geometry draws; same-policy collection and short learning, not twelve-hour training.')
    write(args.output / 'protocol.json', protocol)
    initial = checkpoint(args.output / 'scaling-initial.pt', new_model(spec, spec['policy']['seed']))
    write(args.output / 'scaling-initial.json', initial)
    print(dict(status='frozen', host=protocol['host'], smoke=args.smoke), flush=True)


def tasks(directory, policy, generation_seeds, action_seeds, horizon, training, phase):
    directory.mkdir()
    paths = []
    for index, (generation_seed, action_seed) in enumerate(zip(generation_seeds, action_seeds)):
        location = directory / f'episode-{index:04d}'
        location.mkdir()
        path = location / 'task.json'
        write(path, dict(identity=dict(phase=phase, index=index, generation_seed=generation_seed,
                                      action_seed=action_seed),
            generation_seed=generation_seed, action_seed=action_seed, horizon=horizon, training=training,
            policy=policy['path'], policy_file_sha256=policy['sha256'], policy_sha256=policy['policy_sha256']))
        paths.append(path.resolve())
    if not paths or len(generation_seeds) != len(action_seeds):
        raise ValueError('random-pilot.task-seed-shape')
    return paths


def semantics(result):
    record = dict(status=result['status'], identity=result['identity'], policy=result['policy_sha256'],
                  admission=result['generation']['admission_status'], draw=result['generation']['draw'])
    if result['status'] == 'complete':
        row = result['row']
        trace = unpack(Path(row['directory']) / 'episode.json.gz')['trace']
        keys = ('action', 'status', 'observation_sha256', 'allowed_sha256', 'probabilities_sha256',
                'old_log_probability', 'current_before_sha256', 'current_sha256', 'best_sha256',
                'current_score', 'best_score')
        record.update(request=result['generation']['request_sha256'], initial=result['generation']['initial_sha256'],
            geometry=result['generation']['geometry_sha256'], gradient=result.get('gradient_values_sha256'),
            trace=[{key: step[key] for key in keys} for step in trace])
    return digest(record)


def describe_results(results):
    accepted = [r for r in results if r['status'] == 'complete']
    return dict(attempted=len(results), accepted=len(accepted), rejected=len(results) - len(accepted),
        selected=sum(r['row']['selected'] for r in accepted),
        unique_requests=len({r['generation']['request_sha256'] for r in accepted}),
        unique_geometry=len({r['generation']['geometry_sha256'] for r in accepted}),
        products=sorted({r['generation']['draw']['product'] for r in accepted}),
        distinct_rates=len({(r['generation']['draw']['product'], r['generation']['draw']['rate']['duration_ms']) for r in accepted}),
        facility_counts=sorted({r['row']['instances'] for r in accepted}),
        changed_geometry=sum(r['generation']['geometry_changed'] for r in accepted),
        total_generation_seconds=sum(r['generation']['seconds'] for r in results),
        total_episode_seconds=sum(r['row']['seconds'] for r in accepted))


def scaling(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    spec, cells = protocol['spec'], []
    initial = read(args.screen / 'scaling-initial.json')
    baseline = {}
    if 1 not in spec['scaling']['workers']:
        raise ValueError('random-pilot.serial-reference-required')
    counts = [count for count in spec['scaling']['workers'] if count != 1]
    random.Random(spec['scaling']['order_seed']).shuffle(counts)
    counts.insert(0, 1)
    began = time.monotonic()
    deadline = began + spec['limits']['study_seconds']
    output = args.screen / 'scaling'
    output.mkdir()
    for count in counts:
        directory = output / f'workers-{count:02d}'
        directory.mkdir()
        cell = dict(workers=count, status='incomplete', repetitions=[])
        try:
            with Pool(protocol_path, count, directory / 'pool') as pool:
                for repetition in range(spec['scaling']['repetitions']):
                    n = spec['scaling']['attempts']
                    jobs = tasks(directory / f'repetition-{repetition}', initial,
                        [spec['scaling']['generation_seed_base'] + repetition * 1000 + i for i in range(n)],
                        [spec['scaling']['action_seed_base'] + repetition * 1000 + i for i in range(n)],
                        spec['selection']['horizon'], True, f'scaling-{repetition}')
                    results, timing = pool.collect(jobs, deadline)
                    model = new_model(spec, spec['policy']['seed'])
                    optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
                    accounting = update(model, optimizer, results, spec['learning'])
                    signature = dict(episodes=[semantics(result) for result in results],
                        policy_after=accounting['policy_after_sha256'], gradients=accounting['applied_gradient_sha256'])
                    baseline.setdefault(repetition, signature)
                    parity = signature == baseline[repetition]
                    summary = describe_results(results)
                    record = dict(repetition=repetition, **timing, **summary, semantic_parity=parity,
                        valid_actions_per_second=summary['selected'] / timing['seconds'],
                        accounting=accounting, signature=signature, results=results)
                    write(directory / f'repetition-{repetition}.json', record)
                    cell['repetitions'].append(dict(path=str((directory / f'repetition-{repetition}.json').resolve()),
                        sha256=sha(directory / f'repetition-{repetition}.json'),
                        **{key: record[key] for key in ('seconds', 'accepted', 'rejected', 'selected', 'semantic_parity',
                            'valid_actions_per_second', 'peak_sampled_rss_bytes', 'startup_seconds')}))
                    print(dict(stage='scaling', workers=count, repetition=repetition, parity=parity,
                               actions_per_second=record['valid_actions_per_second']), flush=True)
            cell['status'] = 'complete' if all(r['semantic_parity'] for r in cell['repetitions']) else 'parity-mismatch'
        except Exception as error:
            cell.update(status='failed', diagnostic=dict(code='random-pilot.scaling-cell-failed', severity='error',
                stage='parallel-scaling', entity=count, message=str(error)))
            print(dict(stage='scaling-failed', workers=count, message=str(error)), flush=True)
            if isinstance(error, InterruptedError):
                write(directory / 'interrupted.json', cell)
                raise
        cells.append(cell)
        write(output / 'progress.json', dict(status='incomplete', cells=cells))
        if time.monotonic() >= deadline:
            break
    check_sources(protocol)
    valid = [c for c in cells if c['status'] == 'complete' and len(c['repetitions']) == spec['scaling']['repetitions']]
    if not valid or len(cells) != len(counts):
        raise ValueError('random-pilot.scaling-incomplete: no complete full-matrix selection')
    selected = max(valid, key=lambda c: sum(r['selected'] for r in c['repetitions']) / sum(r['seconds'] for r in c['repetitions']))
    report = dict(schema=SCHEMA, status='measured', cells=cells, selected_workers=selected['workers'],
        protocol_sha256=sha(protocol_path), seconds=time.monotonic() - began,
        scope='Worker selection uses throughput of complete semantically matching cells; startup is reported separately.')
    write(output / 'report.json', report)
    print(dict(status='measured', selected_workers=selected['workers']), flush=True)


def learn(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    spec = protocol['spec']
    scaling_report = read(args.screen / 'scaling/report.json')
    if scaling_report['protocol_sha256'] != sha(protocol_path):
        raise ValueError('random-pilot.scaling-protocol')
    output = args.screen / 'learning'
    output.mkdir()
    began = time.monotonic()
    deadline = began + spec['limits']['study_seconds']
    rows = []
    try:
        with Pool(protocol_path, scaling_report['selected_workers'], output / 'pool') as pool:
            for seed in spec['learning']['seeds']:
                directory = output / f'seed-{seed}'
                directory.mkdir()
                model = new_model(spec, seed)
                optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
                version = checkpoint(directory / 'initial.pt', model, optimizer, seed=seed, next_group=0,
                                     protocol_sha256=sha(protocol_path))
                groups, evaluations, checkpoints = [], [], [version]
                for group in range(spec['learning']['groups'] + 1):
                    if group in spec['learning']['evaluate_at']:
                        n = spec['learning']['evaluation_attempts']
                        jobs = tasks(directory / f'evaluation-{group:04d}', version,
                            [spec['learning']['evaluation_seed_base'] + seed * 100000 + i for i in range(n)],
                            [spec['learning']['evaluation_seed_base'] + 10000000 + seed * 100000 + i for i in range(n)],
                            spec['learning']['horizon'], False, f'evaluation-{seed}-{group}')
                        results, timing = pool.collect(jobs, deadline)
                        evaluation = dict(update=group, policy_sha256=version['policy_sha256'],
                                          results=results, timing=timing, **describe_results(results))
                        evaluations.append(evaluation)
                        write(directory / f'evaluation-{group:04d}.json', evaluation)
                        print(dict(stage='evaluation', seed=seed, update=group, accepted=evaluation['accepted']), flush=True)
                    if group == spec['learning']['groups']:
                        break
                    n = spec['learning']['batch_attempts']
                    batch = directory / f'group-{group:04d}'
                    jobs = tasks(batch, version,
                        [spec['learning']['generation_seed_base'] + seed * 100000 + group * n + i for i in range(n)],
                        [spec['learning']['action_seed_base'] + seed * 100000 + group * n + i for i in range(n)],
                        spec['learning']['horizon'], True, f'training-{seed}-{group}')
                    results, timing = pool.collect(jobs, deadline)
                    accounting = update(model, optimizer, results, spec['learning'])
                    version = checkpoint(batch / 'checkpoint.pt', model, optimizer, seed=seed, next_group=group + 1,
                                         protocol_sha256=sha(protocol_path))
                    checkpoints.append(version)
                    record = dict(group=group, **accounting, timing=timing, results=results,
                                  checkpoint=version, **describe_results(results))
                    groups.append(record)
                    write(batch / 'group.json', record)
                    write(directory / 'progress.json', dict(status='incomplete', seed=seed, groups=groups,
                                                            evaluations=evaluations, checkpoints=checkpoints))
                    print(dict(stage='learning', seed=seed, groups=group + 1, nonflat=not accounting['flat_returns'],
                               accepted=record['accepted']), flush=True)
                record = dict(status='complete', seed=seed, groups=groups, evaluations=evaluations, checkpoints=checkpoints)
                write(directory / 'report.json', record)
                rows.append(dict(path=str((directory / 'report.json').resolve()), sha256=sha(directory / 'report.json')))
        check_sources(protocol)
        write(output / 'report.json', dict(schema=SCHEMA, status='complete', protocol_sha256=sha(protocol_path),
            selected_workers=scaling_report['selected_workers'], seconds=time.monotonic() - began, rows=rows))
    except BaseException as error:
        write(output / 'partial.json', dict(status='incomplete', code='random-pilot.learning-failed', severity='error',
            stage='learning', entity=str(output), message=str(error), completed=rows))
        raise


if __name__ == '__main__':
    def interrupted(signum, _):
        raise InterruptedError(f'random-pilot.interrupted: signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    freeze = sub.add_parser('prepare')
    freeze.add_argument('--config', type=Path, required=True)
    freeze.add_argument('--output', type=Path, required=True)
    freeze.add_argument('--smoke', action='store_true')
    for name in ('scaling', 'learn'):
        sub.add_parser(name).add_argument('--screen', type=Path, required=True)
    args = parser.parse_args()
    try:
        dict(prepare=prepare, scaling=scaling, learn=learn)[args.command](args)
    except BaseException as error:
        location = getattr(args, 'screen', getattr(args, 'output', None))
        if location is not None and location.exists():
            write(location / f'{args.command}-partial.json', dict(status='incomplete',
                code='random-pilot.command-failed', severity='error', stage=args.command,
                entity=str(location), message=str(error)))
        raise
