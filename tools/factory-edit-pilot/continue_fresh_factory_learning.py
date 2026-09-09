"""Continue an audited fresh-factory policy with bounded duration and online audits."""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import shutil
import signal
import time

import torch

from full_factory_environment import FactoryRuntime
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from random_factory_audit import optimizer_hash, require, verify_result, replay, evaluation_curve
from random_factory_collection import Pool, check_sources, update
from random_factory_generation import read, write
from random_factory_pilot import SCHEMA, configure_torch, new_model, checkpoint, tasks, describe_results, semantics


KIND = 'duration-bounded-fresh-factory-v1'


def checked(manifest):
    require(sha(manifest['path']) == manifest['sha256'], str(manifest['path']))
    return read(manifest['path'])


def restore(spec, manifest):
    require(sha(manifest['path']) == manifest['sha256'], 'restore checkpoint file')
    saved = torch.load(manifest['path'], map_location='cpu', weights_only=False)
    require(saved['schema'] == SCHEMA, 'restore checkpoint schema')
    model = new_model(spec, saved['seed'])
    model.load_state_dict(saved['policy'], strict=True)
    require(parameter_hash(model) == saved['policy_sha256'] == manifest['policy_sha256'], 'restored policy')
    optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
    optimizer.load_state_dict(saved['optimizer'])
    torch.set_rng_state(saved['rng'])
    return model, optimizer, saved


def prepare(args):
    configure_torch()
    source_protocol_path = args.source / 'protocol.json'
    source_protocol = read(source_protocol_path)
    check_sources(source_protocol)
    audit_path = args.source / 'audit/report.json'
    audit = read(audit_path)
    require(audit['status'] == 'complete' and not audit['smoke'], 'audited full source pilot')
    require(audit['protocol_sha256'] == sha(source_protocol_path)
            and audit['learning_sha256'] == sha(args.source / 'learning/report.json')
            and audit['scaling_sha256'] == sha(args.source / 'scaling/report.json'), 'source audit identities')
    learning = read(args.source / 'learning/report.json')
    reports = [(manifest, checked(manifest)) for manifest in learning['rows']]
    selected = [(manifest, report) for manifest, report in reports if report['seed'] == args.seed]
    require(len(selected) == 1, 'source seed')
    source_manifest, source = selected[0]
    version = source['checkpoints'][-1]
    original_spec = source_protocol['spec']
    model, optimizer, saved = restore(original_spec, version)
    require(saved['seed'] == args.seed and saved['next_group'] == original_spec['learning']['groups']
            and saved['protocol_sha256'] == sha(source_protocol_path), 'source endpoint position')
    require(0 < args.seconds <= 86400 and 1 <= args.workers <= 64, 'continuation operational bounds')
    args.output.mkdir()
    spec = deepcopy(original_spec)
    if args.smoke:
        spec['generation']['setup_steps'] = [2, 2]
        spec['learning'].update(batch_attempts=4, horizon=2, evaluation_attempts=4)
    initial = checkpoint(args.output / 'initial.pt', model, optimizer, seed=args.seed,
        next_group=saved['next_group'], next_attempt=saved['next_group'] * original_spec['learning']['batch_attempts'],
        source_checkpoint_sha256=version['sha256'])
    files = {**source_protocol['sources'], str(Path(__file__).resolve()): sha(__file__)}
    supervisor = Path(__file__).with_name('fresh_factory_process_supervisor.py').resolve()
    files[str(supervisor)] = sha(supervisor)
    for path in (source_protocol_path, audit_path, Path(source_manifest['path']),
                 Path(version['path']), args.source / 'learning/report.json', args.source / 'scaling/report.json'):
        files[str(path.resolve())] = sha(path)
    protocol = dict(schema=SCHEMA, kind=KIND, smoke=args.smoke, spec=spec,
        sources=files, catalog_file=source_protocol['catalog_file'], initial=initial,
        initial_optimizer_sha256=optimizer_hash(optimizer), seed=args.seed,
        source=dict(protocol=str(source_protocol_path.resolve()), audit=str(audit_path.resolve()),
                    seed_report=source_manifest, checkpoint=version),
        continuation=dict(workers=2 if args.smoke else args.workers,
            training_seconds=min(args.seconds, 300) if args.smoke else args.seconds,
            finishing_seconds=120 if args.smoke else 600,
            evaluate_every_updates=64, max_new_updates=2 if args.smoke else None,
            disk_reserve_bytes=16 * 1024 ** 3),
        scope='Continue the saved policy, Adam and RNG on new factory draws; no fixed factory pool.')
    write(args.output / 'protocol.json', protocol)
    print(dict(status='frozen', smoke=args.smoke, seed=args.seed,
               next_update=saved['next_group'], workers=protocol['continuation']['workers']), flush=True)


def audit_transition(model, optimizer, results, config):
    """Reconstruct the same update from an independent copy of the pre-update state."""
    other_model = deepcopy(model)
    other_optimizer = torch.optim.Adam(other_model.parameters(), lr=config['learning_rate'])
    other_optimizer.load_state_dict(deepcopy(optimizer.state_dict()))
    accounting = update(model, optimizer, results, config)
    repeated = update(other_model, other_optimizer, results, config)
    require(accounting == repeated and parameter_hash(model) == parameter_hash(other_model)
            and optimizer_hash(optimizer) == optimizer_hash(other_optimizer), 'online optimizer reconstruction')
    return accounting


def cleanup_construction(results, directory):
    """Drop audited, regenerable constructor candidate histories; retain actual roots."""
    directory = Path(directory).resolve()
    removed = []
    for result in results:
        generation = result['generation']
        location = Path(generation['original_attempt']['directory']).resolve()
        require(location.is_relative_to(directory), 'cleanup owned constructor directory')
        names = ['construction.html']
        protected = set()
        if result['status'] == 'complete':
            case = generation['case']
            protected.update(Path(case[key]).resolve() for key in ('request', 'initial', 'original'))
            protected.update((Path(case['request']).parent / name).resolve() for name in case.get('files_sha256', {}))
            require(all(Path(case[key]).is_file() for key in ('request', 'initial', 'original')),
                    'cleanup retained episode inputs')
            require((location / 'root.json').is_file()
                    and (location.parent / 'roots.json').is_file(), 'cleanup retained constructor roots')
            names.append('construction.json')
        for name in names:
            path = location / name
            if path.exists():
                require(path.resolve() not in protected, 'cleanup cannot remove an episode input')
                removed.append(dict(path=str(path), sha256=sha(path), bytes=path.stat().st_size))
                path.unlink()
    record = dict(status='complete', removed=removed, bytes_removed=sum(r['bytes'] for r in removed),
        scope='Only constructor HTML and accepted constructor candidate reports; actual inputs, roots, failures and learning evidence retained.')
    write(directory / 'retention.json', record)
    return dict(path=str(directory / 'retention.json'), sha256=sha(directory / 'retention.json'),
                files=len(removed), bytes_removed=record['bytes_removed'])


def native_audit(runtime, results, scratch):
    scratch.mkdir(exist_ok=True)
    for result in results:
        verify_result(runtime, result, scratch)
    (scratch / 'best.json').unlink(missing_ok=True)
    return dict(verified_results=len(results), native_initial_and_best=True, gradient_payloads=True)


def evaluate(pool, runtime, screen, protocol, version, completed, deadline, final=False):
    spec, seed = protocol['spec'], protocol['seed']
    n = spec['learning']['evaluation_attempts']
    label = f'evaluation-{completed:06d}' + ('-final' if final else '')
    directory = screen / label
    jobs = tasks(directory, version,
        [spec['learning']['evaluation_seed_base'] + seed * 100000 + i for i in range(n)],
        [spec['learning']['evaluation_seed_base'] + 10000000 + seed * 100000 + i for i in range(n)],
        spec['learning']['horizon'], False, f'evaluation-{seed}-{completed}')
    results, timing = pool.collect(jobs, deadline)
    audit = native_audit(runtime, results, directory / 'audit')
    retention = cleanup_construction(results, directory)
    record = dict(update=completed, policy_sha256=version['policy_sha256'], results=results,
                  timing=timing, audit=audit, retention=retention, **describe_results(results))
    if not protocol['smoke']:
        source = checked(protocol['source']['seed_report'])
        record['comparison_to_source_initial'] = evaluation_curve([source['evaluations'][0], record])[-1]
        record['comparison_to_resumed_policy'] = evaluation_curve([source['evaluations'][-1], record])[-1]
    path = screen / f'{label}.json'
    write(path, record)
    print(dict(stage='continuation-evaluation', update=completed, accepted=record['accepted'],
               comparison_to_source_initial=record.get('comparison_to_source_initial')), flush=True)
    return dict(path=str(path.resolve()), sha256=sha(path), update=completed)


def run(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    require(protocol['kind'] == KIND, 'continuation protocol')
    require(not (args.screen / 'progress.json').exists(), 'fresh continuation execution')
    spec, limits = protocol['spec'], protocol['continuation']
    model, optimizer, saved = restore(spec, protocol['initial'])
    require(optimizer_hash(optimizer) == protocol['initial_optimizer_sha256'], 'initial Adam continuity')
    completed, next_attempt = saved['next_group'], saved['next_attempt']
    began = time.monotonic()
    deadline = began + limits['training_seconds']
    runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
    version = protocol['initial']
    groups, evaluations, selected, attempted, admitted, bytes_removed = [], [], 0, 0, 0, 0
    termination, final_evaluation = None, None

    def progress(status, **extra):
        write(args.screen / 'progress.json', dict(status=status, kind=KIND, smoke=protocol['smoke'],
            protocol_sha256=sha(protocol_path), seed=protocol['seed'], completed_global_updates=completed,
            new_updates=len(groups), next_attempt=next_attempt, checkpoint=version,
            attempted=attempted, admitted=admitted, rejected=attempted - admitted, selected=selected,
            evaluations=evaluations, seconds=time.monotonic() - began, bytes_removed=bytes_removed, **extra))

    try:
        progress('running', stage='starting')
        with Pool(protocol_path, limits['workers'], args.screen / 'pool') as pool:
            initial_evaluation = evaluate(pool, runtime, args.screen, protocol, version, completed, deadline)
            evaluations.append(initial_evaluation)
            bytes_removed += checked(initial_evaluation)['retention']['bytes_removed']
            if not protocol['smoke']:
                source = checked(protocol['source']['seed_report'])
                reference = source['evaluations'][-1]['results']
                require([semantics(r) for r in checked(initial_evaluation)['results']]
                        == [semantics(r) for r in reference], 'resumed endpoint evaluation identity')
            while True:
                if limits['max_new_updates'] is not None and len(groups) >= limits['max_new_updates']:
                    termination = 'registered-update-limit'
                    break
                if time.monotonic() >= deadline:
                    termination = 'training-time-limit'
                    break
                if shutil.disk_usage(args.screen).free < limits['disk_reserve_bytes']:
                    termination = 'disk-reserve-limit'
                    break
                n = spec['learning']['batch_attempts']
                directory = args.screen / f'group-{completed:06d}'
                jobs = tasks(directory, version,
                    [spec['learning']['generation_seed_base'] + protocol['seed'] * 100000 + next_attempt + i for i in range(n)],
                    [spec['learning']['action_seed_base'] + protocol['seed'] * 100000 + next_attempt + i for i in range(n)],
                    spec['learning']['horizon'], True, f'training-{protocol["seed"]}-{completed}')
                try:
                    results, timing = pool.collect(jobs, deadline)
                except TimeoutError:
                    if time.monotonic() < deadline:
                        raise
                    termination = 'training-time-limit'
                    break
                audit = native_audit(runtime, results, directory / 'audit')
                accounting = audit_transition(model, optimizer, results, spec['learning'])
                candidate_version = checkpoint(directory / 'checkpoint.pt', model, optimizer, seed=protocol['seed'],
                    next_group=completed + 1, next_attempt=next_attempt + n, protocol_sha256=sha(protocol_path))
                rng = torch.get_rng_state()
                restored_model, restored_optimizer, restored = restore(spec, candidate_version)
                require(parameter_hash(restored_model) == parameter_hash(model)
                        and optimizer_hash(restored_optimizer) == optimizer_hash(optimizer)
                        and torch.equal(restored['rng'], rng), 'saved checkpoint continuity')
                audit.update(optimizer_reconstructed=True, optimizer_sha256=optimizer_hash(optimizer),
                             actual_checkpoint_reopened=True)
                retention = cleanup_construction(results, directory)
                record = dict(group=completed, next_attempt=next_attempt + n, **accounting,
                    timing=timing, results=results, checkpoint=candidate_version, audit=audit, retention=retention,
                    **describe_results(results))
                write(directory / 'group.json', record)
                manifest = dict(group=completed, path=str((directory / 'group.json').resolve()),
                    sha256=sha(directory / 'group.json'), checkpoint=candidate_version)
                with open(args.screen / 'groups.jsonl', 'a') as stream:
                    stream.write(json.dumps(manifest, allow_nan=False) + '\n')
                    stream.flush()
                    os.fsync(stream.fileno())
                groups.append(manifest)
                version = candidate_version
                completed += 1
                next_attempt += n
                selected += record['selected']
                attempted += n
                admitted += record['accepted']
                bytes_removed += retention['bytes_removed']
                progress('running', stage='learning')
                print(dict(stage='continuation-learning', update=completed, new_updates=len(groups),
                           accepted=record['accepted'], selected=selected), flush=True)
                if completed % limits['evaluate_every_updates'] == 0:
                    try:
                        evaluations.append(evaluate(pool, runtime, args.screen, protocol, version, completed, deadline))
                        bytes_removed += checked(evaluations[-1])['retention']['bytes_removed']
                    except TimeoutError:
                        if time.monotonic() < deadline:
                            raise
                        termination = 'training-time-limit'
                        break
                    progress('running', stage='evaluated')
        # An interrupted collection pool has been closed before the finishing pool.
        if evaluations[-1]['update'] == completed:
            final_evaluation = dict(status='complete', manifest=evaluations[-1])
        elif termination != 'disk-reserve-limit':
            try:
                with Pool(protocol_path, limits['workers'], args.screen / 'finishing-pool') as pool:
                    result = evaluate(pool, runtime, args.screen, protocol, version, completed,
                                      time.monotonic() + limits['finishing_seconds'], final=True)
                evaluations.append(result)
                bytes_removed += checked(result)['retention']['bytes_removed']
                final_evaluation = dict(status='complete', manifest=result)
            except TimeoutError as error:
                final_evaluation = dict(status='incomplete', diagnostic=str(error))
        else:
            final_evaluation = dict(status='incomplete', diagnostic='Evaluation interrupted at a resource bound.')
        check_sources(protocol)
        progress('complete', termination=termination, final_evaluation=final_evaluation)
        report = dict(read(args.screen / 'progress.json'), groups=groups,
            groups_manifest_sha256=sha(args.screen / 'groups.jsonl') if groups else None,
            scope='Bounded continuation finished; every committed batch received native and optimizer audits. Partial budget-interrupted attempts do not update the policy.')
        write(args.screen / 'report.json', report)
        print(dict(status='complete', termination=termination, updates=completed), flush=True)
    except BaseException as error:
        diagnostic = dict(code='fresh-continuation.failed', severity='error', stage='continuation',
                          entity=str(args.screen), message=str(error))
        progress('incomplete', diagnostic=diagnostic)
        write(args.screen / 'partial.json', dict(status='incomplete', diagnostic=diagnostic, checkpoint=version))
        raise
    finally:
        runtime.close()


def audit(args):
    configure_torch()
    require(0 < args.seconds <= 7200, 'continuation audit time bound')
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    report = read(args.screen / 'report.json')
    require(report['status'] == 'complete' and report['protocol_sha256'] == sha(protocol_path), 'finished continuation')
    args.output.mkdir()
    spec = protocol['spec']
    model, optimizer, saved = restore(spec, protocol['initial'])
    require(optimizer_hash(optimizer) == protocol['initial_optimizer_sha256'], 'audit initial optimizer')
    source_model, source_optimizer, source_saved = restore(spec, protocol['source']['checkpoint'])
    require(parameter_hash(model) == parameter_hash(source_model)
            and optimizer_hash(optimizer) == optimizer_hash(source_optimizer)
            and torch.equal(saved['rng'], source_saved['rng']), 'actual source state continuity')
    runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
    verified, replays = 0, []
    began = time.monotonic()
    def guard():
        if time.monotonic() - began >= args.seconds:
            raise TimeoutError('fresh-continuation.audit-budget')
    try:
        require(sha(args.screen / 'groups.jsonl') == report['groups_manifest_sha256'], 'group index')
        manifests = [json.loads(line) for line in (args.screen / 'groups.jsonl').read_text().splitlines()]
        require(manifests == report['groups'], 'ordered group manifests')
        completed, cursor = saved['next_group'], saved['next_attempt']
        for manifest in manifests:
            guard()
            group = checked(manifest)
            require(group['group'] == completed and group['next_attempt'] == cursor + spec['learning']['batch_attempts'], 'attempt continuity')
            require(len(group['results']) == spec['learning']['batch_attempts'], 'complete attempted batch')
            for index, result in enumerate(group['results']):
                expected = spec['learning']['generation_seed_base'] + protocol['seed'] * 100000 + cursor + index
                action_seed = spec['learning']['action_seed_base'] + protocol['seed'] * 100000 + cursor + index
                require(result['identity']['generation_seed'] == expected
                        and result['identity']['action_seed'] == action_seed, 'fresh generation and action cursor')
                verify_result(runtime, result, args.output)
                verified += 1
            accounting = update(model, optimizer, group['results'], spec['learning'])
            require(all(group[key] == value for key, value in accounting.items()), 'reconstructed update')
            other, other_optimizer, after = restore(spec, group['checkpoint'])
            require(parameter_hash(other) == parameter_hash(model)
                    and optimizer_hash(other_optimizer) == optimizer_hash(optimizer)
                    and after['next_group'] == completed + 1 and after['next_attempt'] == group['next_attempt']
                    and after['protocol_sha256'] == sha(protocol_path), 'actual checkpoint continuity')
            require(sha(group['retention']['path']) == group['retention']['sha256'], 'retention manifest')
            if manifest == manifests[0] or manifest == manifests[-1]:
                result = next(r for r in group['results'] if r['status'] == 'complete')
                replays.append(replay(runtime, protocol, result, args.output / f'replay-{completed}', guard))
            completed += 1
            cursor = group['next_attempt']
        evaluations = [checked(manifest) for manifest in report['evaluations']]
        for evaluation in evaluations:
            for result in evaluation['results']:
                guard()
                verify_result(runtime, result, args.output)
                verified += 1
        require(completed == report['completed_global_updates'] and cursor == report['next_attempt'], 'terminal cursor')
        require(report['checkpoint'] == (manifests[-1]['checkpoint'] if manifests else protocol['initial']), 'terminal checkpoint')
        check_sources(protocol)
        source_curve = None if protocol['smoke'] else evaluation_curve(
            [checked(protocol['source']['seed_report'])['evaluations'][0], *evaluations])
        write(args.output / 'report.json', dict(status='complete', smoke=protocol['smoke'],
            protocol_sha256=sha(protocol_path), continuation_report_sha256=sha(args.screen / 'report.json'),
            verified_results=verified, optimizer_updates=len(manifests), replays=replays,
            replayed_actions=sum(r['selected'] for r in replays),
            curve_vs_resumed_policy=evaluation_curve(evaluations), curve_vs_source_initial=source_curve,
            seconds=time.monotonic() - began))
        (args.output / 'best.json').unlink(missing_ok=True)
        print(dict(status='complete', verified=verified, optimizer_updates=len(manifests)), flush=True)
    finally:
        runtime.close()


if __name__ == '__main__':
    def interrupted(signum, _):
        raise InterruptedError(f'fresh-continuation.interrupted: {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    freeze = sub.add_parser('prepare')
    freeze.add_argument('--source', type=Path, required=True)
    freeze.add_argument('--output', type=Path, required=True)
    freeze.add_argument('--seed', type=int, default=17)
    freeze.add_argument('--workers', type=int, default=24)
    freeze.add_argument('--seconds', type=int, default=43200)
    freeze.add_argument('--smoke', action='store_true')
    sub.add_parser('run').add_argument('--screen', type=Path, required=True)
    check = sub.add_parser('audit')
    check.add_argument('--screen', type=Path, required=True)
    check.add_argument('--output', type=Path, required=True)
    check.add_argument('--seconds', type=int, default=3600)
    arguments = parser.parse_args()
    try:
        dict(prepare=prepare, run=run, audit=audit)[arguments.command](arguments)
    except BaseException as error:
        location = getattr(arguments, 'output', getattr(arguments, 'screen', None))
        if location is not None and location.exists():
            write(location / f'{arguments.command}-partial.json', dict(status='incomplete',
                code='fresh-continuation.command-failed', severity='error', stage=arguments.command,
                entity=str(location), message=str(error)))
        raise
