"""Run a bounded PPO/critic/GAE comparison after completed direct-area learning."""
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
from direct_area_audit import physics_signature
from direct_area_reward import SCHEMA as REWARD_SCHEMA
from ppo_area_audit import native_batch, replay, transition, verify_checkpoint
from ppo_area_model import SCHEMA, require, models
from ppo_area_update import update
from native_factory_runtime import FactoryRuntime, ROOT
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from random_factory_audit import optimizer_hash
from random_factory_collection import Pool, check_sources
from random_factory_generation import read, write, resolve_spec
from random_factory_pilot import configure_torch, tasks as factory_tasks, describe_results


def manifest(path):
    return dict(path=str(Path(path).resolve()), sha256=sha(path))


def checked(record):
    require(sha(record['path']) == record['sha256'], record['path'])
    return read(record['path'])


def checkpoint(path, actor, critic, aopt, copt, rng, **metadata):
    saved = dict(schema=SCHEMA, policy=actor.state_dict(), critic=critic.state_dict(),
        policy_sha256=parameter_hash(actor), critic_sha256=parameter_hash(critic),
        actor_optimizer=aopt.state_dict(), critic_optimizer=copt.state_dict(),
        rng=torch.get_rng_state(), shuffle_rng=rng.get_state(), **metadata)
    temporary = Path(path).with_suffix('.tmp')
    torch.save(saved, temporary)
    temporary.replace(path)
    return dict(**manifest(path), policy_sha256=saved['policy_sha256'], critic_sha256=saved['critic_sha256'])


def learners(spec, seed, config):
    actor, critic = models(spec, seed)
    aopt, copt = (torch.optim.Adam(m.parameters(), lr=config['learning_rate']) for m in (actor, critic))
    rng = torch.Generator().manual_seed(seed + 31000000)
    return actor, critic, aopt, copt, rng


def tasks(directory, version, *arguments):
    paths = factory_tasks(directory, version, *arguments)
    for path in paths:
        value = read(path)
        value['critic_sha256'] = version['critic_sha256']
        write(path, value)
    return paths


def prepare(args):
    configure_torch()
    controls = read(args.config)
    cfg = controls['ppo']
    require(controls['schema_version'] == 1 and cfg['gamma'] == 1 and 0 < cfg['clip_epsilon'] < 1
            and 0 <= cfg['gae_lambda'] <= 1 and 1 <= cfg['epochs'] <= 16
            and 1 <= cfg['minibatch_size'] <= 1024 and 0 < cfg['target_kl'] < 1, 'PPO configuration')
    source_path = args.reference / 'protocol.json'
    source = read(source_path)
    check_sources(source)
    audit_path = args.reference / 'full-audit/report.json'
    audit = read(audit_path)
    require(audit['status'] == 'complete' and not audit['smoke'] and
            audit['protocol_sha256'] == sha(source_path), 'completed audited direct-area reference')
    report_path = args.reference / 'report.json'
    require(audit['report_sha256'] == sha(report_path), 'audited direct-area report identity')
    report = read(report_path)
    require(report['status'] == 'complete', 'direct-area experiment completed before PPO')
    spec = deepcopy(source['spec'])
    old_root = Path(spec['cli']).parents[2]
    for name in ('native_library', 'cli', 'validator'):
        original = Path(spec[name])
        spec[name] = str(ROOT / original.relative_to(old_root))
        require(sha(spec[name]) == sha(original), f'unchanged {name}')
    for name, path in spec['catalogs'].items():
        spec['catalogs'][name] = str(ROOT / Path(path).relative_to(old_root))
        require(sha(spec['catalogs'][name]) == sha(path), 'unchanged catalog')
    for product in spec['products']:
        path = product['source']
        product['source'] = str(ROOT / Path(path).relative_to(old_root))
        require(sha(product['source']) == sha(path), 'unchanged product source')
    for name in ('learning_rate', 'entropy_coefficient', 'gradient_clip'):
        require(cfg[name] == spec['learning'][name], f'unchanged {name}')
    reference = {str(state['seed']): manifest(args.reference / f"seed-{state['seed']}" / 'report.json')
                 for state in report['seeds']}
    controls['evaluate_at'] = spec['learning']['evaluate_at']
    args.output.mkdir(parents=True)
    if args.smoke:
        spec['generation']['setup_steps'] = [2, 2]
        spec['learning'].update(groups=2, batch_attempts=4, horizon=2, seeds=[17], evaluation_attempts=4)
        controls.update(workers=2, evaluate_at=[0, 1, 2], study_seconds=1200)
    require(0 < controls['study_seconds'] <= 14400 and 1 <= controls['workers'] <= 64, 'study bounds')
    catalogs = args.output / 'catalogs.json'
    write(catalogs, spec['catalogs'])
    paths = set(Path(__file__).parent.glob('*.py'))
    for name in source['sources']:
        original = Path(name)
        if original.is_relative_to(old_root):
            local = ROOT / original.relative_to(old_root)
            if local.is_file():
                paths.add(local)
    paths.update([catalogs, args.config, source_path, audit_path, report_path])
    paths.update(Path(r['path']) for r in reference.values())
    paths.update(Path(r['path']) for r in source['reference'].values())
    protocol = dict(schema=SCHEMA, smoke=args.smoke, spec=spec, controls=controls,
        catalog_file=str(catalogs.resolve()), reference=reference, rank_reference=source['reference'],
        sources={str(p.resolve()): sha(p) for p in sorted(paths)},
        host=dict(node=os.uname().nodename, cpus=os.cpu_count(), torch=torch.__version__))
    write(args.output / 'protocol.json', protocol)
    print(json.dumps(dict(status='frozen', smoke=args.smoke, workers=controls['workers'], ppo=cfg)), flush=True)


def parity(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    spec = protocol['spec']
    directory = args.screen / 'parity'
    directory.mkdir()
    config = dict(spec['selection'], **protocol['controls']['ppo'])
    actor, critic, aopt, copt, rng = learners(spec, 17, config)
    initial = checkpoint(directory / 'initial.pt', actor, critic, aopt, copt, rng)
    signatures, cells = [], []
    runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
    try:
        for workers in (1, 2):
            cell = directory / f'workers-{workers}'
            cell.mkdir()
            with Pool(protocol_path, workers, cell / 'pool', Path(__file__).with_name('ppo_area_worker.py')) as pool:
                jobs = tasks(cell / 'batch', initial, [98000001 + i for i in range(4)],
                             [99000001 + i for i in range(4)], 2, True, 'parity')
                results, timing = pool.collect(jobs)
                proof = native_batch(runtime, results, cell / 'audit')
                actor, critic, aopt, copt, rng = learners(spec, 17, config)
                accounting = transition(actor, critic, aopt, copt, rng, results, config, lambda: None)
                signatures.append(dict(physics=[physics_signature(r) for r in results],
                    accounting=accounting))
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
    config = dict(spec['selection'], **controls['ppo'])
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
            raise TimeoutError('ppo-area.run-budget')
        require(shutil.disk_usage(args.screen).free > controls['disk_reserve_bytes'], 'disk reserve')
        pool.guard(deadline)
    try:
        publish()
        with Pool(protocol_path, controls['workers'], args.screen / 'pool', Path(__file__).with_name('ppo_area_worker.py')) as pool:
            for seed in spec['learning']['seeds']:
                directory = args.screen / f'seed-{seed}'
                directory.mkdir()
                baseline = checked(protocol['reference'][str(seed)])
                actor, critic, aopt, copt, rng = learners(spec, seed, config)
                initial_reference = checked(baseline['evaluations'][0])
                require(parameter_hash(actor) == initial_reference['policy_sha256'], 'matched untrained actor')
                version = checkpoint(directory / 'initial.pt', actor, critic, aopt, copt, rng, seed=seed, next_group=0,
                                     protocol_sha256=sha(protocol_path), reward_schema=REWARD_SCHEMA)
                state = dict(seed=seed, started_epoch=time.time(), status='running', groups=[], evaluations=[],
                             checkpoint=version, selected=0, diagnostics=[])
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
                                    [physics_signature(r) for r in initial_reference['results']],
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
                                    [physics_signature(r) for r in checked(baseline['groups'][0])['results']],
                                    'exact reference first-batch trajectories')
                        sample = next(r for r in results if r['status'] == 'complete')
                        audit['replay'] = replay(runtime, protocol, sample, batch / 'replay', guard)
                    update_started = time.monotonic()
                    accounting = transition(actor, critic, aopt, copt, rng, results, config, guard)
                    timing['optimization_and_reconstruction_seconds'] = time.monotonic() - update_started
                    version = checkpoint(batch / 'checkpoint.pt', actor, critic, aopt, copt, rng, seed=seed, next_group=group + 1,
                                         protocol_sha256=sha(protocol_path), reward_schema=REWARD_SCHEMA)
                    verify_checkpoint(version['path'], actor, critic, aopt, copt, rng)
                    retention = cleanup_construction(results, batch)
                    record = dict(group=group, checkpoint=version, audit=audit, retention=retention,
                                  timing=timing, results=results, **(describe_results(results) | accounting))
                    path = batch / 'group.json'
                    write(path, record)
                    state['groups'].append(dict(**manifest(path), group=group))
                    state['diagnostics'].append(dict(update=group + 1, epochs=len(accounting['epochs']), **accounting['epochs'][-1],
                        raw_advantage_mean=accounting['raw_advantage_mean'], raw_advantage_std=accounting['raw_advantage_std']))
                    state.update(checkpoint=version, selected=state['selected'] + record['selected'])
                    progress['updates'] += 1
                    progress['selected'] += record['selected']
                    publish()
                    print(json.dumps(dict(stage='learning', seed=seed, update=group + 1,
                        optimizer_steps=accounting['optimizer_steps'], epochs=len(accounting['epochs']),
                        ratio=accounting['epochs'][-1]['ratio_mean'], kl=accounting['epochs'][-1]['approximate_kl'])), flush=True)
                state['status'] = 'complete'
                write(directory / 'report.json', state)
                publish()
        check_sources(protocol)
        progress['status'] = 'complete'
        publish()
        write(args.screen / 'report.json', progress)
    except BaseException as error:
        progress.update(status='incomplete', diagnostic=dict(code='ppo-area.run-failed', severity='error',
            stage='learning', entity=str(args.screen), message=str(error)))
        publish()
        raise
    finally:
        runtime.close()


if __name__ == '__main__':
    def interrupted(signum, _):
        raise InterruptedError(f'ppo-area.interrupted: {signum}')
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
