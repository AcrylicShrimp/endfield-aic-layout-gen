"""Bounded CPU before/after measurements of the optimized PPO execution."""
import argparse
from copy import deepcopy
from pathlib import Path
import statistics
import time

import torch

from direct_area_audit import physics_signature
from full_factory_environment import FactoryRuntime
from full_factory_input_contracts import observation_hash
from full_factory_ppo_learning import rollout as reference_rollout
from full_factory_screen import sha
from ppo_area_study import learners
from ppo_area_update import load_experience, save_experience
from ppo_training_batch import prepare, distribution, critic_values, require
from ppo_training_rollout import rollout
from ppo_training_update import train_minibatch
from probe_ppo_precision import reference_workload, minibatch, vector, difference
from random_factory_generation import read, write
from random_factory_pilot import configure_torch


def fresh(workload):
    spec, config, checkpoint = (workload[k] for k in ('spec', 'config', 'checkpoint'))
    actor, critic, aopt, copt, _ = learners(spec, 17, config)
    actor.load_state_dict(checkpoint['policy'])
    critic.load_state_dict(checkpoint['critic'])
    aopt.load_state_dict(deepcopy(checkpoint['actor_optimizer']))
    copt.load_state_dict(deepcopy(checkpoint['critic_optimizer']))
    return actor, critic, aopt, copt


def run(args):
    configure_torch()
    require(not args.output.exists(), f'fresh benchmark output: {args.output}')
    began = time.monotonic()
    def guard():
        require(time.monotonic() - began < args.seconds, f'benchmark budget: {args.seconds}')
    workload = reference_workload(args.screen)
    spec, config, checkpoint, steps, advantages, targets = (workload[k] for k in
        ('spec', 'config', 'checkpoint', 'steps', 'advantages', 'targets'))
    args.output.mkdir(parents=True)
    report = dict(schema='ppo-optimization-benchmark-v1', status='running', workload=workload['identity'],
                  sources={name: sha(Path(__file__).with_name(name)) for name in
                           ['benchmark_ppo_optimization.py', 'ppo_training_batch.py', 'ppo_training_update.py',
                            'ppo_training_rollout.py', 'ppo_training_observation.py']},
                  torch_version=torch.__version__, threads=torch.get_num_threads(), device='cpu',
                  scope='Same retained 64 selections and nonempty Adam checkpoint, reset before each measured update; separate native episode replay.',
                  cells=[])
    write(args.output / 'report.json', report)
    reference = None
    for size in [None, *args.microbatches]:
        guard()
        timings = []
        for repetition in range(4):
            model_set = fresh(workload)
            started = time.perf_counter()
            if size is None:
                details = minibatch(model_set, steps, advantages, targets, config, torch.device('cpu'), None, None, False)
            else:
                details = train_minibatch(*model_set, steps, advantages, targets, config, size, torch.device('cpu'), guard)
            elapsed = time.perf_counter() - started
            if repetition:
                timings.append(elapsed)
        cell = dict(name='reference-sequential' if size is None else f'batched-{size}', microbatch=size,
                    seconds=timings, median_seconds=statistics.median(timings), last_update=details)
        # Capture separately so CPU vector copies are outside all timed updates.
        if size is None:
            reference_models = deepcopy(model_set[:2])
            reference = {kind: {name: vector(model, kind == 'gradients') for name, model in
                                zip(('actor', 'critic'), model_set[:2])} for kind in ('parameters', 'gradients')}
        else:
            cell['drift'] = {kind: {name: difference(vector(model, kind == 'gradients'), reference[kind][name])
                                  for name, model in zip(('actor', 'critic'), model_set[:2])}
                             for kind in ('parameters', 'gradients')}
            cell['parameter_changes'] = []
            for model_name, actual_model, expected_model in zip(('actor', 'critic'), model_set[:2], reference_models):
                for (name, actual), (_, expected) in zip(actual_model.named_parameters(), expected_model.named_parameters()):
                    drift = float((actual.detach() - expected.detach()).abs().max())
                    if drift > 1e-7:
                        cell['parameter_changes'].append(dict(model=model_name, parameter=name, max_absolute=drift))
                    if not (model_name == 'actor' and name == 'score.2.bias'):
                        torch.testing.assert_close(actual, expected, rtol=5e-4, atol=5e-6, msg=f'{size} {model_name} {name}')
                torch.testing.assert_close(vector(actual_model, True), reference['gradients'][model_name],
                                           rtol=5e-4, atol=5e-6, msg=f'{size} gradients {model_name}')
            batch = prepare(steps, config, model_set[0].width, 'cpu')
            with torch.no_grad():
                post_logp = distribution(model_set[0], batch, config)[0]
                reference_post_logp = distribution(reference_models[0], batch, config)[0]
            cell['post_update_logp_drift'] = difference(post_logp, reference_post_logp)
            torch.testing.assert_close(post_logp, reference_post_logp, rtol=5e-5, atol=5e-6)
            actor, critic, _, _ = fresh(workload)
            batch = prepare(steps, config, actor.width, 'cpu')
            with torch.no_grad():
                logp, _, _ = distribution(actor, batch, config)
                value = critic_values(critic, batch)
            cell['behavior_drift'] = dict(log_probability=difference(logp, batch['old_logp']), value=difference(value, batch['old_values']))
            torch.testing.assert_close(logp, batch['old_logp'], rtol=5e-5, atol=5e-6)
            torch.testing.assert_close(value, batch['old_values'], rtol=5e-5, atol=5e-6)
        report['cells'].append(cell)
        write(args.output / 'report.json', report)
        print(dict(cell=cell['name'], median_seconds=cell['median_seconds'], drift=cell.get('drift')), flush=True)
    sample = next(r for r in workload['group_record']['results'] if r['status'] == 'complete')
    task = read(Path(sample['result_path']).with_name('task.json'))
    original_experience = load_experience(sample)
    report['collection'] = []
    for name, collector in [('reference', reference_rollout), ('optimized', rollout)]:
        times = []
        for repetition in range(2):
            guard()
            actor, critic, _, _ = fresh(workload)
            runtime = FactoryRuntime(spec['native_library'], workload['protocol']['catalog_file'])
            output = args.output / f'collection-{name}-{repetition}'
            try:
                started = time.perf_counter()
                arguments = [runtime, dict(config=dict(spec['selection'], horizon=task['horizon'])),
                             sample['generation']['case'], actor, critic]
                if name == 'reference':
                    arguments.append('action-only')
                row, payload = collector(*arguments, task['action_seed'], output, True, guard)
                payload.update(policy_sha256=row['policy_sha256'], input_sha256=sample['input_sha256'])
                save_experience(output / 'experience.pt.gz', payload)
                sha(output / 'experience.pt.gz')
                times.append(time.perf_counter() - started)
                require(physics_signature(dict(sample, row=row)) == physics_signature(sample), f'{name} physical replay')
                require(len(payload['steps']) == len(original_experience['steps']) and
                        all(observation_hash(a) == observation_hash(b) for a, b in zip(payload['steps'], original_experience['steps'])),
                        f'{name} actor/critic observation replay')
            finally:
                runtime.close()
        report['collection'].append(dict(name=name, seconds=times, median_seconds=statistics.median(times),
                                          actions_physics_observations_match=True))
        write(args.output / 'report.json', report)
        print(report['collection'][-1], flush=True)
    report.update(status='complete', seconds=time.monotonic() - began)
    write(args.output / 'report.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=600)
    parser.add_argument('--microbatches', type=int, nargs='+', default=[8])
    args = parser.parse_args()
    if not all(0 < size <= 64 for size in args.microbatches):
        parser.error('microbatches must be in [1, 64]')
    if not 0 < args.seconds <= 600:
        parser.error('seconds must be in (0, 600]')
    existed = args.output.exists()
    try:
        run(args)
    except BaseException as error:
        path = args.output / 'report.json'
        if not existed and path.exists():
            report = read(path)
            report.update(status='failed', diagnostic=dict(code='ppo-training.benchmark-failed', severity='error',
                                                           stage='benchmark', entity=str(args.screen), message=str(error)))
            write(path, report)
        raise
