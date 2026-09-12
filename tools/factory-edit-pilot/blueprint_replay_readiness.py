"""Bounded complete-cycle comparison; main training cannot bypass its receipt."""
from copy import deepcopy
import json
import os
from pathlib import Path
import statistics
import time

import torch

from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_evaluation_cohort import evaluate_cohort, load_cohort
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_store import ROOT, read, reference, save, write
from blueprint_ppo_update import digest
from blueprint_replay_contract import check
from blueprint_replay_runtime import Child, admitted_sources, child_config, publish
from blueprint_replay_anchors import label_metrics
from blueprint_success_buffer import Evidence


def run_readiness(sources, output, spec):
    output = Path(output).resolve()
    check(not output.exists(), 'source-invalid', 'readiness', 'readiness output already exists', output)
    output.mkdir(parents=True)
    began = time.monotonic()
    allocation = cpu_allocation(8)
    measured_files = ['blueprint_execution_pool.py', 'blueprint_execution_worker.py', 'blueprint_execution_update.py',
        'blueprint_execution_resources.py', 'blueprint_ppo_store.py', 'blueprint_ppo_collect.py',
        'blueprint_ppo_environment.py', 'blueprint_ppo_evaluate.py', 'blueprint_ppo_batch.py', 'blueprint_ppo_update.py',
        'blueprint_edit_input.py', 'blueprint_training_input.py', 'blueprint_replay_runtime.py',
        'blueprint_replay_buffer.py', 'blueprint_replay_anchors.py', 'blueprint_replay_readiness.py',
        'ppo_harness_transport.py', 'random_factory_generation.py', 'blueprint_success_buffer.py']
    from blueprint_ppo_store import sha
    source_hashes = {name: sha(Path(__file__).with_name(name)) for name in measured_files}
    guard = ResourceGuard(output, 7200, spec['limits']['matrix_rss_bytes'], spec['limits']['min_free_bytes'])
    parent, imports, binding = admitted_sources(sources, 17)
    config = child_config(parent, spec, output / 'training-inputs', readiness=True)
    library = parent['parent_protocol']['native_library']
    probe = read(ROOT / config['probe_spec'])
    cases, cohort_sources = load_cohort(config['evaluation']['cohort'], 32,
        {k: ROOT / p for k, p in probe['catalogs'].items()})
    identity = digest(dict(spec=spec, binding=binding, config=config, allocation=allocation))
    arm = next(a for a in spec['arms'] if a['name'] == 'B50')
    modes = [('serial-cpu', False, 'cpu'), ('pool-cpu', True, 'cpu'), ('pool-cuda', True, 'cuda:0')]
    check(torch.cuda.is_available(), 'execution-gate', 'readiness', 'CUDA initialization unavailable; no fallback')
    results = []
    for name, parallel, device in modes:
        cold = time.monotonic()
        child = Child(parent, imports, binding, spec, arm, device, identity)
        runtime = Runtime(config, library)
        pool = BlueprintPool(config, library, allocation['workers'], output / name / 'workers', guard,
            saved=child.streams) if parallel else None
        # Spawn with the full permitted mask, then isolate the coordinator.
        os.sched_setaffinity(0, allocation['coordinator'])
        startup = time.monotonic() - cold
        try:
            for repetition in range(spec['readiness']['warmup_cycles'] + spec['readiness']['measured_cycles']):
                cycle_began = time.monotonic()
                directory = output / name / f'cycle-{repetition}'
                child = Child(parent, imports, binding, spec, arm, device, identity)
                if pool:
                    pool.exchange([dict(operation='initialize', config=config, library=library, index=i, saved=s)
                        for i, s in enumerate(child.streams)])
                else:
                    streams = [Stream.restore(runtime, s) for s in child.streams]
                job = dict(schema='blueprint-replay-balance-state-v1', update=0, records=[])
                evidence = Evidence(directory, job)
                save(directory / 'checkpoints/0000.pt.gz', child.checkpoint(child.streams))
                write(directory / 'anchors/0000.json', dict(update=0, rows=label_metrics(child.behavior_copy(), imports)))
                phases = dict(collect=0., learner=0., storage=0., evaluation=0.)
                actions, episodes = 0, 0
                for _ in range(spec['readiness']['updates_per_cycle']):
                    behavior = child.behavior_copy()
                    payload = pool.collect(behavior, child.sampler) if pool else collect(behavior, streams, child.sampler, config, guard)
                    phases['collect'] += payload['collection_seconds']
                    record = child.update(payload, evidence, config)
                    phases['learner'] += record['ppo']['execution']['update_seconds']
                    snapshots = pool.snapshot() if pool else [s.snapshot() for s in streams]
                    publish(child, payload, record, snapshots, directory, job)
                    phases['storage'] += record['storage_seconds']
                    actions += len(payload['samples'])
                    episodes += len(payload['completed'])
                eval_began = time.monotonic()
                behavior = child.behavior_copy()
                eval_dir = directory / 'evaluation/0004'
                eval_dir.mkdir(parents=True)
                rows = pool.evaluate(behavior, cases, 17, 4, eval_dir, guard.began + guard.seconds) if pool else \
                    evaluate_cohort(runtime, behavior, 17, 4, eval_dir, guard, cases)
                write(eval_dir / 'report.json', dict(update=4, rows=rows))
                write(directory / 'anchors/0004.json', dict(update=4, rows=label_metrics(child.behavior_copy(), imports)))
                phases['evaluation'] = time.monotonic() - eval_began
                check(actions == 1024 and episodes == 16 and len(rows) == 32,
                    'execution-gate', 'readiness', 'complete-cycle workload coverage differs')
                guard()
                result = dict(mode=name, device=device, repetition=repetition, warmup=repetition == 0,
                    seconds=time.monotonic() - cycle_began, phases=phases, train_actions=actions, episodes=episodes,
                    evaluation_actions=[r['policy_actions'] for r in rows], evaluation_cases=len(rows),
                    checkpoint_publications=5, resource=deepcopy(guard.last),
                    device_peak_bytes=torch.cuda.max_memory_allocated() if device.startswith('cuda') else 0,
                    model_sha256=digest(child.model.state_dict()), cold_startup_seconds=startup,
                    workers=pool.identities if pool else [], worker_cpu_seconds=pool.worker_cpu_seconds[:] if pool else [],
                    max_step_concurrency=pool.max_step_concurrency if pool else 1)
                results.append(result)
                write(output / 'progress.json', dict(results=results, allocation=allocation))
                print(json.dumps(dict(stage='readiness-cycle', **result)), flush=True)
        finally:
            if pool:
                pool.close()
            os.sched_setaffinity(0, allocation['allowed'])
    medians = {name: statistics.median(r['seconds'] for r in results if r['mode'] == name and not r['warmup'])
        for name, _, _ in modes}
    selected = 'pool-cpu' if medians['pool-cpu'] < medians['pool-cuda'] else 'pool-cuda'
    speedup = medians['serial-cpu'] / medians[selected]
    # 480 normal cycles, 45 transfer plus 15 initialization evaluations,
    # 3,600 post-run anchor branches (conservative: no shared-zero discount),
    # measured startup/admission plus setup reserve. No extra actions are credited.
    eval_median = statistics.median(r['phases']['evaluation'] for r in results if r['mode'] == selected and not r['warmup'])
    startup = next(r['cold_startup_seconds'] for r in results if r['mode'] == selected)
    forecast = medians[selected] * 480 + eval_median * 60 + 3600 * 2 / 8 + startup * 15 + 600
    receipt = dict(schema='blueprint-replay-readiness-v1', status='measured', spec_sha256=digest(spec),
        binding=binding, allocation=allocation, results=results, medians=medians, selected_mode=selected,
        device='cpu' if selected == 'pool-cpu' else 'cuda:0', speedup=speedup, forecast_seconds=forecast,
        cohort_sources=cohort_sources, measured_sources=source_hashes, seconds=time.monotonic() - began,
        throughput_passed=speedup >= spec['readiness']['minimum_speedup']
            and forecast < spec['readiness']['maximum_forecast_seconds'])
    write(output / 'measurement.json', receipt)
    check(receipt['throughput_passed'], 'execution-gate', 'readiness',
        'complete-cycle throughput or screen forecast failed', dict(speedup=speedup, forecast_seconds=forecast))
    return receipt
