"""Prepare and execute a bounded, replicated input-contract candidate screen."""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import random
import signal
import subprocess
import sys
import time

import torch

from full_factory_curriculum import source_manifest
from full_factory_environment import FactoryRuntime
from full_factory_input_contracts import CONTRACTS, SCHEMA, InputContractPolicy
from full_factory_input_learning import rollout, group_gradients
from full_factory_large_learning import read, verify_sources
from full_factory_on_policy import parameter_hash
from full_factory_on_policy_result import load_result
from full_factory_screen import sha


def write(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


def config_check(config):
    if config.get('schema_version') != 1 or config.get('group_episodes') != 4:
        raise ValueError('input-screen.config-schema')
    for key, maximum in dict(groups=256, horizon=64, episode_seconds=300, route_work=100000,
                             job_seconds=14400, study_seconds=43200, workers=4, width=128, depth=8).items():
        if type(config.get(key)) is not int or not 0 < config[key] <= maximum:
            raise ValueError(f'input-screen.config-bound: {key}')
    if config['width'] % 4 or type(config.get('rss_limit_bytes')) is not int or not 0 < config['rss_limit_bytes'] <= 16 * 1024**3:
        raise ValueError('input-screen.config-width-rss')
    for key in ('learning_rate', 'temperature', 'gradient_clip'):
        if type(config.get(key)) not in (int, float) or not 0 < config[key] <= 1:
            raise ValueError(f'input-screen.config-scalar: {key}')
    if type(config.get('entropy_coefficient')) not in (int, float) or not 0 <= config['entropy_coefficient'] <= .1:
        raise ValueError('input-screen.config-entropy')
    if config.get('balanced') is not True or config.get('memory') is not True:
        raise ValueError('input-screen.config-selection-contract')
    for key in ('seeds', 'evaluation_banks'):
        values = config.get(key)
        if not isinstance(values, list) or not values or len(set(values)) != len(values) or any(type(v) is not int or not 0 <= v < 2**31 for v in values):
            raise ValueError(f'input-screen.config-seeds: {key}')
    if type(config.get('training_seed_base')) is not int or not 0 <= config['training_seed_base'] < 2**31:
        raise ValueError('input-screen.config-training-seed')
    candidates = config.get('candidates')
    if not isinstance(candidates, list) or not candidates or len(set(candidates)) != len(candidates) or not set(candidates) <= set(CONTRACTS):
        raise ValueError('input-screen.config-candidates')
    checkpoints = config.get('evaluate_at')
    if not isinstance(checkpoints, list) or checkpoints != sorted(set(checkpoints)) or checkpoints[0] != 0 or checkpoints[-1] != config['groups']:
        raise ValueError('input-screen.config-evaluation-checkpoints')


def prepare(args):
    old, _ = load_result(args.training)
    config = read(args.config)
    config_check(config)
    cases = [deepcopy(c) for c in old['cases'] if c['id'] in old['training_cases'] and c['instances'] in (23, 48)]
    cases.sort(key=lambda c: (c['instances'], c['id']))
    training = [c['id'] for c in cases if c['instances'] == 23]
    if len(cases) != 8 or len(training) != 4:
        raise ValueError('input-screen.case-membership: expected four intact 23-facility and four 48-facility cases')
    if args.smoke:
        config.update(seeds=[17], groups=2, horizon=4, evaluate_at=[0, 2], evaluation_banks=[4400000],
                      job_seconds=600, study_seconds=1800)
        cases = [c for c in cases if c['id'] in training[:2]]
        training = [c['id'] for c in cases]
    config_check(config)
    jobs = []
    # Candidate order is independently shuffled per seed to spread dispatch order.
    for seed in config['seeds']:
        names = list(config['candidates'])
        random.Random(seed).shuffle(names)
        jobs.extend(dict(id=f'{name}-seed-{seed}', candidate=name, seed=seed) for name in names)
    sources = dict(old['sources'])
    sources.update({str(args.config.resolve()): sha(args.config), str((args.training / 'protocol.json').resolve()): sha(args.training / 'protocol.json'),
                    str((args.training / 'training-result.json').resolve()): sha(args.training / 'training-result.json')})
    args.output.mkdir()
    protocol = dict(schema_version=1, observation_schema=SCHEMA, experiment='input-contract-candidate-screen', smoke=args.smoke,
        config=config, contracts={n: CONTRACTS[n] for n in config['candidates']}, cases=cases, training_cases=training,
        evaluation_cases=[c['id'] for c in cases], jobs=jobs, native_library=old['native_library'], catalogs=old['catalogs'],
        sources=sources, source_sha256=source_manifest(),
        scope='Exploratory fixed-recipe input screen; controls were opened before this study. No production or convergence claim.')
    verify_sources(protocol)
    write(args.output / 'protocol.json', protocol)
    print(dict(status='frozen', smoke=args.smoke, jobs=len(jobs), training_actions=len(jobs) * config['groups'] * 4 * config['horizon']), flush=True)


def checkpoint(path, protocol_hash, model, optimizer, job, next_group):
    temporary = path.with_suffix('.tmp')
    torch.save(dict(schema=SCHEMA, protocol_sha256=protocol_hash, job=job, next_group=next_group,
        depth=model.depth, width=model.width, policy=model.state_dict(), optimizer=optimizer.state_dict(),
        rng=torch.get_rng_state(), policy_sha256=parameter_hash(model)), temporary)
    temporary.replace(path)
    return dict(path=str(path.resolve()), sha256=sha(path), policy_sha256=parameter_hash(model), next_group=next_group)


def worker(args):
    started = time.monotonic()
    runtime = None
    stage, entity = 'inputs', args.job
    rows, groups, checkpoints = [], [], []
    output = args.screen / args.job
    output.mkdir()
    def interrupted(signum, _):
        raise InterruptedError(f'input-screen.interrupted: signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        protocol = read(args.screen / 'protocol.json')
        config = protocol['config']
        config_check(config)
        verify_sources(protocol)
        protocol_hash = sha(args.screen / 'protocol.json')
        job = next(j for j in protocol['jobs'] if j['id'] == args.job)
        def guard():
            if time.monotonic() - started >= config['job_seconds']:
                raise TimeoutError(f'input-screen.job-bound: {args.job}')
        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        torch.use_deterministic_algorithms(True)
        torch.manual_seed(job['seed'])
        model = InputContractPolicy(config['depth'], config['width'])
        optimizer = torch.optim.Adam(model.parameters(), lr=config['learning_rate'])
        runtime = FactoryRuntime(protocol['native_library'], protocol['catalogs'])
        cases = {c['id']: c for c in protocol['cases']}
        order = list(protocol['training_cases'])
        random.Random(job['seed']).shuffle(order)
        initial = checkpoint(output / 'initial.pt', protocol_hash, model, optimizer, job, 0)
        checkpoints.append(initial)

        def progress():
            write(output / 'progress.json', dict(status='incomplete', job=job, stage=stage, entity=entity,
                rows=rows, groups=groups, checkpoints=checkpoints, seconds=time.monotonic() - started))

        def evaluate(update):
            nonlocal stage, entity
            stage, entity = 'evaluation', update
            directory = output / f'evaluation-{update:04d}'
            directory.mkdir()
            for case_index, case_id in enumerate(protocol['evaluation_cases']):
                for bank in config['evaluation_banks']:
                    guard()
                    seed = bank + case_index
                    row, _ = rollout(runtime, protocol, cases[case_id], model, job['candidate'], seed,
                                     directory / f'case-{case_index}-bank-{bank}', False, guard)
                    row.update(phase='evaluation', update=update, initialization_seed=job['seed'], bank=bank,
                               split='training' if case_id in protocol['training_cases'] else 'held-out-control')
                    rows.append(row)
                    progress()
            print(dict(job=args.job, evaluated_update=update, seconds=time.monotonic() - started), flush=True)

        evaluate(0)
        for group in range(config['groups']):
            guard()
            stage, entity = 'training', group
            group_started = time.monotonic()
            directory = output / f'group-{group:04d}'
            directory.mkdir()
            case = cases[order[group % len(order)]]
            before = parameter_hash(model)
            episode_gradients, episode_rows = [], []
            for attempt in range(4):
                sample_seed = config['training_seed_base'] + job['seed'] * 10000 + group * 4 + attempt
                row, gradients = rollout(runtime, protocol, case, model, job['candidate'], sample_seed,
                                         directory / f'episode-{attempt}', True, guard)
                row.update(phase='training', group=group, attempt=attempt, initialization_seed=job['seed'])
                rows.append(row)
                episode_rows.append(row)
                episode_gradients.append(gradients)
                progress()
            if parameter_hash(model) != before or any(r['policy_sha256'] != before for r in episode_rows):
                raise ValueError('input-screen.group-behavior-identity')
            optimizer.zero_grad(set_to_none=True)
            scores = [r['best_metrics']['score'] for r in episode_rows]
            accounting = group_gradients(model, episode_gradients, scores, config['entropy_coefficient'])
            guard()
            clipped = torch.nn.utils.clip_grad_norm_(model.parameters(), config['gradient_clip'])
            if not torch.isfinite(clipped):
                raise ValueError('input-screen.nonfinite-gradient')
            optimizer.step()
            after = checkpoint(directory / 'checkpoint.pt', protocol_hash, model, optimizer, job, group + 1)
            record = dict(group=group, case=case['id'], scores=scores, **accounting,
                policy_before_sha256=before, policy_after_sha256=parameter_hash(model), checkpoint=after,
                pre_clip_gradient_norm=float(clipped), seconds=time.monotonic() - group_started)
            write(directory / 'group.json', record)
            groups.append(record)
            checkpoints.append(after)
            progress()
            if (group + 1) % 4 == 0:
                print(dict(job=args.job, groups=group + 1, nonflat=sum(not g['flat_returns'] for g in groups),
                           seconds=time.monotonic() - started), flush=True)
            if group + 1 in config['evaluate_at']:
                evaluate(group + 1)
        stage, entity = 'final-verification', args.job
        verify_sources(protocol)
        guard()
        evidence = {}
        for row in rows:
            for name in ('episode.json.gz', 'best.json.gz'):
                path = Path(row['directory']) / name
                evidence[str(path.resolve())] = sha(path)
        write(output / 'evidence.json', evidence)
        report = dict(schema_version=1, status='complete', job=job, smoke=protocol['smoke'], protocol_sha256=protocol_hash,
            seconds=time.monotonic() - started, allocated_parameters=sum(p.numel() for p in model.parameters()),
            contract=protocol['contracts'][job['candidate']], groups=groups, rows=rows, checkpoints=checkpoints,
            evidence_sha256=sha(output / 'evidence.json'))
        write(output / 'report.json', report)
        print(dict(status='complete', job=args.job, seconds=report['seconds']), flush=True)
    except BaseException as error:
        write(output / 'partial.json', dict(schema_version=1, status='incomplete', code='input-screen.worker-failed',
            severity='error', stage=stage, entity=entity, message=str(error), rows=rows, groups=groups,
            checkpoints=checkpoints, seconds=time.monotonic() - started))
        raise
    finally:
        if runtime is not None:
            runtime.close()


def process_rows():
    result = subprocess.run(['ps', '-axo', 'pid=,ppid=,pgid=,rss='], capture_output=True, text=True, check=True, timeout=2)
    return [tuple(map(int, line.split())) for line in result.stdout.splitlines() if line.strip()]


def terminate_owned(children):
    for signum, timeout in ((signal.SIGTERM, 5), (signal.SIGKILL, 2)):
        for child in children:
            try:
                os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass
            except PermissionError:
                for pid, _, pgid, _ in process_rows():
                    if pgid == child.pid:
                        try:
                            if os.getpgid(pid) == child.pid:
                                os.kill(pid, signum)
                        except (ProcessLookupError, PermissionError):
                            pass
        deadline = time.monotonic() + timeout
        for child in children:
            try:
                child.wait(timeout=max(.001, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                pass
    remaining = [pid for pid, _, pgid, _ in process_rows() if pgid in {c.pid for c in children}]
    if remaining:
        raise RuntimeError(f'input-screen.cleanup: surviving owned processes {remaining}')


def run(args):
    began = time.monotonic()
    protocol = read(args.screen / 'protocol.json')
    config_check(protocol['config'])
    verify_sources(protocol)
    if any((args.screen / name).exists() for name in ('run.json', 'run-progress.json', 'run-partial.json')):
        raise ValueError('input-screen.existing-run: use a fresh screen')
    def interrupted(signum, _):
        raise InterruptedError(f'input-screen.coordinator-interrupted: signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    config = protocol['config']
    pending, active, children, logs, completed = list(protocol['jobs']), {}, [], [], []
    peak_rss, samples = 0, 0
    try:
        while pending or active:
            elapsed = time.monotonic() - began
            if elapsed >= config['study_seconds']:
                raise TimeoutError('input-screen.study-bound')
            rows = process_rows()
            groups = {c.pid for c in children}
            rss = sum(rss for pid, _, pgid, rss in rows if pid == os.getpid() or pgid in groups) * 1024
            peak_rss, samples = max(peak_rss, rss), samples + 1
            if rss > config['rss_limit_bytes']:
                raise MemoryError(f'input-screen.sampled-rss-bound: {rss}')
            while pending and len(active) < config['workers']:
                job = pending.pop(0)
                if (args.screen / job['id']).exists():
                    raise ValueError(f'input-screen.existing-job: {job["id"]}')
                log = open(args.screen / f'{job["id"]}.log', 'w')
                logs.append(log)
                environment = dict(os.environ, OMP_NUM_THREADS='1', MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1',
                                   VECLIB_MAXIMUM_THREADS='1', NUMEXPR_NUM_THREADS='1', PYTHONHASHSEED='0')
                child = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), 'worker', '--screen', str(args.screen.resolve()),
                                          '--job', job['id']], stdout=log, stderr=subprocess.STDOUT, env=environment, start_new_session=True)
                children.append(child)
                active[child.pid] = (child, job, time.monotonic())
                print(dict(stage='dispatch', job=job['id'], pid=child.pid), flush=True)
            for pid, (child, job, started) in list(active.items()):
                code = child.poll()
                if code is None:
                    if time.monotonic() - started > config['job_seconds'] + 30:
                        raise TimeoutError(f'input-screen.worker-watchdog: {job["id"]}')
                    continue
                path = args.screen / job['id'] / 'report.json'
                passed = code == 0 and path.exists()
                if passed:
                    report = read(path)
                    passed = report['status'] == 'complete' and report['job'] == job and report['protocol_sha256'] == sha(args.screen / 'protocol.json')
                completed.append(dict(job=job, status='complete' if passed else 'failed', exit_code=code,
                    report=str(path.resolve()) if passed else None, report_sha256=sha(path) if passed else None,
                    dispatch_seconds=time.monotonic() - started))
                print(dict(stage='finished', job=job['id'], status=completed[-1]['status'], seconds=completed[-1]['dispatch_seconds']), flush=True)
                del active[pid]
            write(args.screen / 'run-progress.json', dict(status='incomplete', completed=completed,
                active=[j['id'] for _, j, _ in active.values()], pending=len(pending), seconds=elapsed,
                peak_sampled_rss_bytes=peak_rss, memory_samples=samples))
            if pending or active:
                time.sleep(1)
        verify_sources(protocol)
        report = dict(schema_version=1, status='complete' if all(r['status'] == 'complete' for r in completed) else 'completed_with_failures',
            protocol_sha256=sha(args.screen / 'protocol.json'), rows=completed, seconds=time.monotonic() - began,
            peak_sampled_rss_bytes=peak_rss, memory_samples=samples,
            memory_scope='One-second sampled summed coordinator and owned process-group RSS; shared pages may be counted repeatedly.')
        write(args.screen / 'run.json', report)
        print(dict(status=report['status'], jobs=len(completed), seconds=report['seconds'], peak_rss=peak_rss), flush=True)
    except BaseException as error:
        write(args.screen / 'run-partial.json', dict(status='incomplete', code='input-screen.coordinator-failed', severity='error',
            stage='coordinator', entity=str(args.screen), message=str(error), completed=completed,
            owned_process_groups=[c.pid for c in children], seconds=time.monotonic() - began))
        raise
    finally:
        try:
            terminate_owned(children)
        finally:
            for log in logs:
                log.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    freeze = sub.add_parser('prepare')
    for name in ('training', 'config', 'output'):
        freeze.add_argument('--' + name, type=Path, required=True)
    freeze.add_argument('--smoke', action='store_true')
    sub.add_parser('run').add_argument('--screen', type=Path, required=True)
    execute = sub.add_parser('worker')
    execute.add_argument('--screen', type=Path, required=True)
    execute.add_argument('--job', required=True)
    args = parser.parse_args()
    dict(prepare=prepare, run=run, worker=worker)[args.command](args)
