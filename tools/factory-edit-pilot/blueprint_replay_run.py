"""One registered learner arm at a time; durable update publication and retention."""
from copy import deepcopy
import json
import os
from pathlib import Path
import time

import torch

from blueprint_duration_metrics import define_metrics, metrics
from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import ResourceGuard
from blueprint_ppo_store import checked, load, read, reference, save, sha, write
from blueprint_replay_anchors import label_metrics
from blueprint_replay_contract import Sources, check, diagnostic
from blueprint_replay_launch import check_launch
from blueprint_replay_panel import evaluation_cases
from blueprint_replay_runtime import Child, admitted_sources, child_config, publish, retain
from blueprint_success_buffer import Evidence
from ppo_harness_environment import load_environment, tracking_destination


def evaluate(child, pool, cases, kind, directory, guard, job):
    ordinal = child.ordinal
    began = time.monotonic()
    output = directory / kind / f'{ordinal:04d}'
    rows = pool.evaluate(child.behavior_copy(), cases, child.seed, ordinal, output, guard.began + guard.seconds)
    record = dict(schema='blueprint-replay-evaluation-v1', update=ordinal, rows=rows,
        seconds=time.monotonic() - began, raw_retained=ordinal in child.spec['retention']['evaluation_traces'],
        cases=[reference(p, directory) for p in sorted(output.glob('*.pt.gz'))])
    write(output / 'report.json', record)
    job.setdefault(kind, []).append(dict(update=ordinal, **reference(output / 'report.json', directory)))
    if ordinal not in child.spec['retention']['evaluation_traces']:
        for path in output.glob('*.pt.gz'):
            path.unlink()
    write(directory / 'state.json', job)
    return record


def run_arm(launch, parent, imports, binding, matrix, name, arm, seed, state, matrix_guard):
    import wandb
    directory = matrix / name
    directory.mkdir(exist_ok=True)
    config = child_config(parent, launch['spec'], launch['training_input_directory'])
    spec, device = launch['spec'], launch['device']
    protocol_hash = state['launch_sha256']
    if (directory / 'state.json').exists():
        job = read(directory / 'state.json')
        if job['status'] == 'complete':
            return job
        saved = load(checked(job['latest_checkpoint'], directory))
        child = Child(parent, imports, binding, spec, arm, device, protocol_hash, saved)
    else:
        child = Child(parent, imports, binding, spec, arm, device, protocol_hash)
        checkpoint = directory / 'checkpoints/0000.pt.gz'
        save(checkpoint, child.checkpoint(child.streams))
        job = dict(schema='blueprint-replay-balance-state-v1', status='running', update=0, records=[],
            latest_checkpoint=reference(checkpoint, directory), seconds=0.)
        write(directory / 'state.json', job)
    original_seconds = job['seconds']
    check(original_seconds < spec['limits']['arm_seconds'], 'resource-exhausted', 'arm', 'arm already exhausted registered time', name)
    guard_resource = ResourceGuard(directory, spec['limits']['arm_seconds'] - original_seconds,
        spec['limits']['matrix_rss_bytes'], spec['limits']['min_free_bytes'])
    def guard():
        matrix_guard()
        guard_resource()
    destination = tracking_destination(required=True)
    tracking = wandb.init(**destination, mode='online', name=name, group='blueprint-replay-balance-20260913',
        id=job.get('wandb_id'), resume='allow', dir=str(directory), config=dict(arm=arm, seed=seed,
            launch_sha256=protocol_hash, parent=binding, device=device, workers=8, spec=spec))
    job['wandb_id'], job['wandb_url'] = tracking.id, tracking.url
    define_metrics(tracking)
    evidence = Evidence(directory, job)
    allocation = launch['allocation']
    pool = None
    panel = read(checked(launch['transfer_panel'], matrix))
    try:
        pool = BlueprintPool(config, launch['native_library'], allocation['workers'], directory / 'workers', guard, saved=child.streams)
        os.sched_setaffinity(0, allocation['coordinator'])
        job['workers'] = pool.identities
        while True:
            ordinal = child.ordinal
            guard()
            if ordinal % 4 == 0:
                label_path = directory / f'anchors/{ordinal:04d}.json'
                if not label_path.exists():
                    shared_zero = matrix / f'anchor-initial/seed-{seed}.json'
                    if ordinal == 0 and shared_zero.exists():
                        labels = read(shared_zero)
                    else:
                        labels = dict(update=ordinal, rows=label_metrics(child.behavior_copy(), imports))
                        if ordinal == 0:
                            write(shared_zero, labels)
                    write(label_path, labels)
                if not any(r['update'] == ordinal for r in job.get('historical', [])):
                    evaluation = evaluate(child, pool, launch['historical'], 'historical', directory, guard_resource, job)
                    baseline = read(checked(job['historical'][0], directory))['rows']
                    tracking.log(metrics(evaluation, ordinal, 256, seconds=original_seconds + time.monotonic() - guard_resource.began,
                        initial=baseline))
                if ordinal in spec['transfer_panel']['updates'] and not any(r['update'] == ordinal for r in job.get('transfer', [])):
                    evaluate(child, pool, evaluation_cases(panel), 'transfer', directory, guard_resource, job)
            if ordinal == spec['child_updates']:
                check(all(s['native'] is None for s in pool.snapshot()), 'parent-incompatible', 'arm', 'final update has live episodes')
                break
            payload = pool.collect(child.behavior_copy(), child.sampler)
            record = child.update(payload, evidence, config)
            snapshots = pool.snapshot()
            record.update(worker_cpu_seconds=pool.worker_cpu_seconds[:], max_step_concurrency=pool.max_step_concurrency)
            publish(child, payload, record, snapshots, directory, job)
            job['seconds'] = original_seconds + time.monotonic() - guard_resource.began
            retain(child, directory, job)
            tracking.log(metrics(record, child.ordinal, 256, seconds=job['seconds']))
            check(sum(p.stat().st_size for p in directory.rglob('*') if p.is_file()) < spec['limits']['arm_output_bytes'],
                'resource-exhausted', 'arm', 'arm output ceiling reached', directory)
            print(json.dumps(dict(stage='train', name=name, update=child.ordinal, reward=record['area_reward'],
                own_entries=record['buffer']['own_entries'], actor_epochs=record['ppo']['actor_epochs'], seconds=job['seconds'])), flush=True)
        job.update(status='complete', seconds=original_seconds + time.monotonic() - guard_resource.began,
            resource=guard_resource.last, max_step_concurrency=pool.max_step_concurrency)
        write(directory / 'state.json', job)
        tracking.finish()
        return job
    except BaseException as error:
        job.update(status='failed', seconds=original_seconds + time.monotonic() - guard_resource.began,
            diagnostic=diagnostic(error, 'arm', name))
        write(directory / 'state.json', job)
        tracking.finish(exit_code=1)
        raise
    finally:
        if pool:
            pool.close()
        os.sched_setaffinity(0, allocation['allowed'])


def run(matrix, environment):
    matrix = Path(matrix).resolve()
    launch, state = read(matrix / 'launch.json'), read(matrix / 'state.json')
    check(state['launch_sha256'] == sha(matrix / 'launch.json'), 'source-invalid', 'launch', 'launch identity changed')
    check(state['status'] != 'complete', 'source-invalid', 'launch', 'matrix already complete')
    check_launch(launch, Sources())
    load_environment(environment)
    spec = launch['spec']
    matrix_guard = ResourceGuard(matrix, spec['limits']['matrix_seconds'] - state['seconds'],
        spec['limits']['matrix_rss_bytes'], spec['limits']['min_free_bytes'])
    previous_seconds = state['seconds']
    state['status'] = 'running'
    write(matrix / 'state.json', state)
    parents = {}
    try:
        for item in launch['order']:
            seed, arm_name = item['seed'], item['arm']
            name = f'{arm_name}-seed-{seed}'
            if state['jobs'].get(name, {}).get('status') == 'complete':
                continue
            if seed not in parents:
                parents[seed] = admitted_sources(launch['source_directory'], seed)
            parent, imports, binding = parents[seed]
            check(binding == launch['bindings'][str(seed)], 'parent-incompatible', 'launch', 'parent admission binding changed', seed)
            arm = deepcopy(next(a for a in spec['arms'] if a['name'] == arm_name))
            job = run_arm(launch, parent, imports, binding, matrix, name, arm, seed, state, matrix_guard)
            state['jobs'][name] = dict(status=job['status'], update=job['update'], state=reference(matrix / name / 'state.json', matrix),
                wandb_url=job['wandb_url'])
            state['seconds'] = previous_seconds + time.monotonic() - matrix_guard.began
            write(matrix / 'state.json', state)
        state['status'] = 'complete'
    except BaseException as error:
        state.update(status='failed', error=str(error))
        raise
    finally:
        state['seconds'] = previous_seconds + time.monotonic() - matrix_guard.began
        write(matrix / 'state.json', state)
