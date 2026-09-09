"""Profile one retained PPO collection episode and minibatch before optimization."""
import argparse
import cProfile
from pathlib import Path
import pstats
import time

import torch

from finish_ppo_confirmation import load_confirmation
from direct_area_audit import physics_signature
from full_factory_environment import FactoryRuntime
from full_factory_input_contracts import observation_hash
from full_factory_ppo_learning import rollout
from full_factory_screen import sha
from ppo_area_model import models, require
from ppo_area_update import load_experience, save_experience
from probe_ppo_precision import reference_workload, make_cell, minibatch, selection_tensors
from random_factory_generation import read, write
from random_factory_pilot import configure_torch


def python_summary(profile):
    rows = []
    for (filename, line, function), (primitive, calls, exclusive, cumulative, _) in pstats.Stats(profile).stats.items():
        rows.append(dict(file=filename, line=line, function=function, primitive_calls=primitive, calls=calls,
                         exclusive_seconds=exclusive, cumulative_seconds=cumulative))
    return dict(by_exclusive=sorted(rows, key=lambda r: r['exclusive_seconds'], reverse=True)[:25],
                by_cumulative=sorted(rows, key=lambda r: r['cumulative_seconds'], reverse=True)[:25])


def run(args):
    configure_torch()
    if args.output.exists():
        raise ValueError(f'ppo-profile.output-exists: {args.output}')
    began = time.monotonic()
    def guard():
        if time.monotonic() - began > args.seconds:
            raise TimeoutError(f'ppo-profile.budget: {args.seconds} seconds')
    confirmation = load_confirmation(args.screen)
    workload = reference_workload(args.screen)
    protocol, spec, config = workload['protocol'], workload['spec'], workload['config']
    args.output.mkdir(parents=True)
    report = dict(schema='ppo-pipeline-profile-v1', status='running', source_sha256=sha(__file__),
                  reference_source_sha256=sha(Path(__file__).with_name('probe_ppo_precision.py')),
                  finalizer_source_sha256=sha(Path(__file__).with_name('finish_ppo_confirmation.py')),
                  workload=workload['identity'], torch_version=torch.__version__, threads=torch.get_num_threads(),
                  phase_timings=[dict(seed=s['seed'], batches=s['batches'],
                                     collection_wall_seconds=s['collection_seconds'],
                                     optimization_and_reconstruction_wall_seconds=s['optimization_and_reconstruction_seconds'])
                                 for s in confirmation['seeds']],
                  timing_scope='Collection and update are sequential. Per-function cumulative times overlap; profiled durations include instrumentation and are not throughput.',
                  implementation=dict(collection_workers=protocol['controls']['workers'], learner_threads=1,
                                      training_device='cpu', full_update_executions_per_batch=2,
                                      sequential_observations_inside_minibatch=True))
    write(args.output / 'report.json', report)
    guard()
    sample = next(r for r in workload['group_record']['results'] if r['status'] == 'complete')
    task = read(Path(sample['result_path']).with_name('task.json'))
    actor, critic = models(spec, 0)
    actor.load_state_dict(workload['checkpoint']['policy'])
    critic.load_state_dict(workload['checkpoint']['critic'])
    selection = dict(spec['selection'], horizon=task['horizon'])
    runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
    collector_profile = cProfile.Profile()
    try:
        started = time.monotonic()
        with collector_profile:
            row, payload = rollout(runtime, dict(config=selection), sample['generation']['case'], actor, critic,
                                   'action-only', task['action_seed'], args.output / 'collection-replay', True, guard)
            payload.update(policy_sha256=row['policy_sha256'], input_sha256=sample['input_sha256'])
            save_experience(args.output / 'profile-experience.pt.gz', payload)
            sha(args.output / 'profile-experience.pt.gz')
        duration = time.monotonic() - started
        # Equivalence checks are outside the measured collection section.
        require(physics_signature(dict(sample, row=row)) == physics_signature(sample), 'profile physical replay')
        original = load_experience(sample)
        require(len(payload['steps']) == len(original['steps']) and
                all(observation_hash(a) == observation_hash(b) for a, b in zip(payload['steps'], original['steps'])),
                'profile actor/critic observation replay')
        report['collection_replay'] = dict(result=sample['result_path'], result_sha256=sample['result_sha256'],
                                           seconds=duration, validation=dict(physical_and_sampling_match=True,
                                                                           critic_observations_match=True),
                                           scope='Episode and experience serialization; excludes generation, checkpoint loading and replay equivalence checks.',
                                           python=python_summary(collector_profile))
        collector_profile.dump_stats(str(args.output / 'collection.pstats'))
    finally:
        runtime.close()
    print('Collection replay profile complete; actions, physics and observations matched.', flush=True)
    write(args.output / 'report.json', report)
    guard()
    checkpoint, steps, advantages, targets = (workload[k] for k in ('checkpoint', 'steps', 'advantages', 'targets'))
    data = [dict(step, _selection=selection_tensors(step, config, torch.device('cpu'))) for step in steps]
    def prepared_models():
        return make_cell(spec, 17, config, checkpoint, torch.device('cpu'))
    def update(model_set):
        return minibatch(model_set, data, advantages, targets, config, torch.device('cpu'), None, None, False)
    model_set = prepared_models()
    update(model_set)
    del model_set
    guard()
    model_set = prepared_models()
    started = time.monotonic()
    update_result = update(model_set)
    report['uninstrumented_reference_minibatch_seconds'] = time.monotonic() - started
    report['reference_optimizer_steps'] = update_result['actual_optimizer_steps']
    del model_set
    guard()
    learner_profile = cProfile.Profile()
    model_set = prepared_models()
    with learner_profile:
        update(model_set)
    report['learner_python'] = python_summary(learner_profile)
    learner_profile.dump_stats(str(args.output / 'learner.pstats'))
    del model_set
    print('Uninstrumented and Python-profiled CPU minibatch complete.', flush=True)
    write(args.output / 'report.json', report)
    guard()
    model_set = prepared_models()
    with torch.profiler.profile(activities=[torch.profiler.ProfilerActivity.CPU],
                                record_shapes=True, profile_memory=True) as operator_profile:
        update(model_set)
    operator_profile.export_chrome_trace(str(args.output / 'learner-operators.json'))
    operators = [dict(operator=e.key, calls=e.count, self_cpu_ms=e.self_cpu_time_total / 1000,
                      total_cpu_ms=e.cpu_time_total / 1000, self_cpu_memory_bytes=e.self_cpu_memory_usage)
                 for e in operator_profile.key_averages()]
    report['learner_operators'] = sorted(operators, key=lambda e: e['self_cpu_ms'], reverse=True)[:30]
    report['observation_shapes'] = [dict(candidates=len(step['allowed']),
                                        current_raster=list(step['critic_observation']['current']['raster'].shape),
                                        best_is_current=step['critic_observation']['best_is_current']) for step in steps]
    report.update(status='complete', seconds=time.monotonic() - began)
    write(args.output / 'report.json', report)
    print(dict(status='complete', seconds=report['seconds'], top_operators=report['learner_operators'][:5]), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=600)
    arguments = parser.parse_args()
    if not 0 < arguments.seconds <= 600:
        parser.error('seconds must be in (0, 600]')
    preexisting = arguments.output.exists()
    try:
        run(arguments)
    except BaseException as error:
        path = arguments.output / 'report.json'
        if not preexisting and path.exists():
            failure = read(path)
            failure.update(status='failed', diagnostic=dict(code='ppo-profile.failed', severity='error',
                                                           stage='profiling', entity=str(arguments.screen), message=str(error)))
            write(path, failure)
        raise
