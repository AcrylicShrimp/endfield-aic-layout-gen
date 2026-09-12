"""Separate eight-update fork, exact replay and device execution evidence."""
from copy import deepcopy
import json
import os
from pathlib import Path
import time

import torch

from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_ppo_store import load, read, save, sha, write
from blueprint_ppo_update import digest
from blueprint_replay_contract import check
from blueprint_replay_runtime import Child, admitted_sources, child_config, publish
from blueprint_success_buffer import Evidence


def verify_fork(sources, measurement, output, spec):
    output = Path(output).resolve()
    check(not output.exists(), 'source-invalid', 'verification', 'fork output already exists', output)
    output.mkdir(parents=True)
    receipt = read(measurement)
    check(receipt['throughput_passed'], 'execution-gate', 'verification', 'throughput not passed')
    allocation = cpu_allocation(8)
    check(all(allocation[k] == receipt['allocation'][k] for k in ('workers', 'coordinator', 'effective_quota')),
        'execution-gate', 'verification', 'allocation differs from measurement')
    guard = ResourceGuard(output, 1800, spec['limits']['matrix_rss_bytes'], spec['limits']['min_free_bytes'])
    parent, imports, binding = admitted_sources(sources, 17)
    config = child_config(parent, spec, output / 'training-inputs', readiness=True)
    identity = digest(dict(spec=spec, binding=binding, purpose='eight-update-verification'))
    arm = deepcopy(next(a for a in spec['arms'] if a['name'] == 'B50'))
    child = Child(parent, imports, binding, spec, arm, receipt['device'], identity)
    job = dict(schema='blueprint-replay-balance-state-v1', update=0, records=[])
    evidence = Evidence(output, job)
    pool = None
    checks = []
    try:
        pool = BlueprintPool(config, parent['parent_protocol']['native_library'], allocation['workers'], output / 'workers', guard,
            saved=child.streams)
        os.sched_setaffinity(0, allocation['coordinator'])
        for ordinal in range(1, 9):
            # Only this explicitly marked diagnostic alternates replay modes.
            child.arm['sampler'] = 'uniform' if ordinal % 2 else 'balanced'
            before = deepcopy(child.checkpoint(pool.snapshot()))
            payload = pool.collect(child.behavior_copy(), child.sampler)
            update_config = deepcopy(config)
            if ordinal == 4:
                update_config['ppo']['target_kl'] = 1e-12
            record = child.update(payload, evidence, update_config)
            snapshots = pool.snapshot()
            publish(child, payload, record, snapshots, output, job)
            checks.append(dict(update=ordinal, sampler=child.arm['sampler'], actor_epochs=record['ppo']['actor_epochs'],
                own_admissions=child.buffer.own.state['admissions'], imported_labels=sum(g['labels'] for g in record['ppo']['success']['groups']
                    if g['origin'] == 'imported'), persistent=[s['policy_actions'] for s in snapshots]))
            if ordinal == 4:
                check(record['ppo']['actor_epochs'] < 4 and record['ppo']['critic_tail_preserves_actor'],
                    'source-invalid', 'verification', 'forced KL diagnostic did not exercise actor freezing')
            if ordinal == 2:
                replay = Child(parent, imports, binding, spec, deepcopy(child.arm), receipt['device'], identity, before)
                replay_evidence = Evidence(output, dict(records=job['records'][:1]))
                replay_result = replay.update(deepcopy(payload), replay_evidence, update_config)
                for field in ('model_after', 'actor_optimizer', 'critic_optimizer', 'shuffle_rng', 'targets_sha256', 'advantages_sha256'):
                    check(replay_result['ppo'][field] == record['ppo'][field], 'parent-incompatible', 'verification',
                        'same-runtime update replay differs', field)
                check(replay.buffer.snapshot() == child.buffer.snapshot(), 'parent-incompatible', 'verification', 'replayed buffer differs')
            if ordinal == 3:
                check(all(s['native'] is not None and s['policy_actions'] == 32 for s in snapshots),
                    'parent-incompatible', 'verification', 'interruption needs eight live half-episodes')
                pool.close()
                pool = None
                child = Child(parent, imports, binding, spec, deepcopy(child.arm), receipt['device'], identity,
                    load(output / 'checkpoints/0003.pt.gz'))
                os.sched_setaffinity(0, allocation['allowed'])
                pool = BlueprintPool(config, parent['parent_protocol']['native_library'], allocation['workers'], output / 'workers-restored',
                    guard, saved=child.streams)
                os.sched_setaffinity(0, allocation['coordinator'])
        check(all(s['native'] is None for s in pool.snapshot()), 'parent-incompatible', 'verification', 'fork ended with live episodes')
    finally:
        if pool:
            pool.close()
        os.sched_setaffinity(0, allocation['allowed'])
    # One actual CUDA update with both PPO and imported SIL; this profiler is
    # never called by normal arm execution or automatic completion.
    arm = next(a for a in spec['arms'] if a['name'] == 'B50')
    cuda = Child(parent, imports, binding, spec, arm, 'cuda:0', identity)
    payload = load(output / 'chunks/0001.pt.gz')
    profile_evidence = Evidence(output, dict(records=[]))
    with torch.profiler.profile(activities=[torch.profiler.ProfilerActivity.CPU, torch.profiler.ProfilerActivity.CUDA]) as profile:
        cuda_result = cuda.update(payload, profile_evidence, config)
        torch.cuda.synchronize()
    events = [dict(name=e.key, count=e.count, cpu_us=e.cpu_time_total, device_us=e.device_time_total)
        for e in profile.key_averages() if e.device_time_total > 0]
    check(events and cuda_result['ppo']['success']['label_count'] > 0,
        'execution-gate', 'verification', 'no actual CUDA execution or SIL labels recorded')
    result = dict(schema='blueprint-replay-fork-verification-v1', status='complete', measurement=str(Path(measurement).resolve()),
        measurement_sha256=sha(measurement), spec_sha256=digest(spec), binding=binding,
        device=receipt['device'], checks=checks, exact_update_replay=True, interruption_update=3,
        cuda_events=events, resource=guard.last)
    write(output / 'verification.json', result)
    return result
