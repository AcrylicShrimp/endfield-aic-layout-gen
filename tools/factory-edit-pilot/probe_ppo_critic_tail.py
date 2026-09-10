"""Test extra critic fitting on mature checkpoints and identical held-out paths."""
import argparse
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor, as_completed
from copy import deepcopy
import json
import multiprocessing
import os
from pathlib import Path
import time

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')

import torch

from full_factory_on_policy import parameter_hash
from native_factory_runtime import FactoryRuntime
from ppo_actor_critic import require
from ppo_area_model import models
from ppo_area_update import load_experience, save_experience
from ppo_critic_metrics import episode as prediction_moments, aggregate, summarize, moments
from ppo_critic_tail import finish
from ppo_harness_run import learners
from ppo_harness_schedule import cleanup_phase
from ppo_harness_state import read, write, sha, checked_path, check_sources, restore_checkpoint
from ppo_training_batch import prepare, critic_values
from ppo_training_rollout import rollout
from ppo_training_update import update, collect_steps, statistics
from ppo_area_audit import verify_result
from random_factory_generation import generate


def initialize(spec, weights, config, candidate):
    global _runtime, _actor, _critic, _spec, _config, _candidate
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    saved = torch.load(weights, map_location='cpu', weights_only=False)
    _actor, _critic = models(spec, 0)
    _actor.load_state_dict(saved['policy'])
    _critic.load_state_dict(saved['critic'])
    require(parameter_hash(_actor) == saved['policy_sha256'] and parameter_hash(_critic) == saved['critic_sha256'],
            'held-out worker checkpoint identity')
    _runtime = FactoryRuntime(spec['native_library'], saved['catalog_file'])
    _spec, _config, _candidate = spec, config, candidate


def collect(task):
    index, directory, generation_seed, action_seed = task
    directory = Path(directory)
    directory.mkdir()
    began = time.monotonic()
    def guard():
        require(time.monotonic() - began < 120, f'held-out task time budget: {directory}')
    policy_hash, critic_hash = parameter_hash(_actor), parameter_hash(_critic)
    identity = dict(index=index, generation_seed=generation_seed, action_seed=action_seed)
    request = dict(identity=identity, generation_seed=generation_seed, action_seed=action_seed,
                   policy_sha256=policy_hash, critic_sha256=critic_hash, horizon=_config['horizon'], training=True)
    write(directory/'task.json', request)
    case, generation = generate(_runtime, _spec, generation_seed, directory/'generated', guard)
    result = dict(identity=identity, generation=generation, input_sha256=sha(directory/'task.json'),
                  policy_sha256=policy_hash, critic_sha256=critic_hash,
                  status='generation-rejected' if case is None else 'complete')
    if case is not None:
        # training=True requests immutable observations; these never enter a fit.
        row, payload = rollout(_runtime, dict(config=_config), case, _actor, _critic, _candidate,
                               action_seed, directory/'episode', True, guard)
        payload.update(policy_sha256=policy_hash, input_sha256=result['input_sha256'])
        path = directory/'experience.pt.gz'
        save_experience(path, payload)
        result.update(row=row, experience=str(path), experience_sha256=sha(path))
    write(directory/'result.json', result)
    return dict(result, result_path=str(directory/'result.json'), result_sha256=sha(directory/'result.json'))


def values(critic, steps, config, device, microbatch):
    result = []
    with torch.no_grad():
        for offset in range(0, len(steps), microbatch):
            batch = prepare(steps[offset:offset+microbatch], config, 32, device)
            result.extend(critic_values(critic, batch).cpu().tolist())
    return result


def run(args):
    root, output = args.run.resolve(), args.output.resolve()
    require(not output.exists(), f'fresh mature-critic probe output required: {output}')
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    require(torch.cuda.is_available(), 'mature-critic probe requires the original CUDA environment')
    torch.backends.cuda.matmul.fp32_precision = 'ieee'
    torch.backends.cudnn.conv.fp32_precision = 'ieee'
    torch.backends.cudnn.benchmark = False
    protocol, state = read(root/'protocol.json'), read(root/'state.json')
    check_sources(protocol)
    require(sha(root/'protocol.json') == state['protocol_sha256'], 'parent protocol identity')
    require(state['status'] in ('complete', 'stopped') and len(state['jobs']) == 1, 'terminal single-job parent required')
    require('actor_critic' not in protocol['spec']['model'], 'probe requires the original independent critic')
    job = state['jobs'][0]
    config = dict(protocol['spec']['selection'], **protocol['config']['ppo'], reward=protocol['config']['reward'])
    selection = dict(config, horizon=protocol['config']['training']['horizon'])
    output.mkdir(parents=True)
    inputs = {str(root/'protocol.json'): sha(root/'protocol.json'), str(root/'state.json'): sha(root/'state.json')}
    report = dict(schema='ppo-mature-critic-tail-v1', status='running', source_sha256=sha(__file__),
                  parent=str(root), protocol_sha256=state['protocol_sha256'], batches=[],
                  scope='Frozen actors and identical held-out trajectories; no actor improvement is measured.',
                  limits=dict(seconds=args.seconds, workers=args.workers, held_out_attempts=args.attempts),
                  limitations=['Three checkpoints share one training lineage; they are not independent seeds.',
                               'Realized returns include policy randomness and are not exact conditional expectations.'])
    write(output/'report.json', report)
    started = time.monotonic()
    def guard():
        require(time.monotonic() - started < args.seconds, 'mature critic probe total time budget')
    geometry = {}
    try:
        parent_seeds = set()
        for reference in job['records']:
            guard()
            path = checked_path(root, reference)
            inputs[str(path)] = reference['sha256']
            parent_seeds.update(r['identity']['generation_seed'] for r in read(path)['results'])
        held_out_seeds = set(range(411000000, 411000000+args.attempts))
        require(parent_seeds.isdisjoint(held_out_seeds), 'held-out factory seeds overlap the parent run')
        report.update(parent_generation_seeds_checked=len(parent_seeds), held_out_generation_seeds=sorted(held_out_seeds),
                      held_out_seeds_excluded_from_parent=True)
        for number in args.batches:
            guard()
            directory = output/f'batch-{number:04d}'
            directory.mkdir()
            reference = next(r for r in job['records'] if r['phase'] == dict(kind='training', batch=number))
            path = checked_path(root, reference)
            inputs[str(path)] = sha(path)
            record = read(path)
            model_set = learners(protocol, job['seed'])
            actor, critic, actor_optimizer, critic_optimizer, rng = model_set
            restore_checkpoint(root, record['behavior_checkpoint'], model_set, state['protocol_sha256'], job['id'], number-1)
            path = checked_path(root, record['behavior_checkpoint'])
            inputs[str(path)] = sha(path)
            payloads = []
            for result in record['results']:
                if result['status'] == 'complete':
                    payloads.append(load_experience(result))
                    inputs[result['experience']] = result['experience_sha256']
                else:
                    payloads.append(None)
            steps, advantages, targets, _ = collect_steps(actor, critic, record['results'], config, experiences=payloads)
            actual = update(*model_set, record['results'], config, protocol['config']['execution']['microbatch'], guard,
                            experiences=payloads)
            for key in ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
                        'critic_optimizer_sha256', 'shuffle_rng_sha256', 'epochs'):
                require(actual[key] == record['ppo'][key], f'original update replay differs: batch {number}, {key}')
            completed = len(actual['epochs'])
            require(completed < config['epochs'], f'batch {number} did not stop early')
            baseline = deepcopy(critic)
            weights = directory/'post-policy-update.pt'
            torch.save(dict(policy=actor.state_dict(), critic=critic.state_dict(),
                            policy_sha256=parameter_hash(actor), critic_sha256=parameter_hash(critic),
                            catalog_file=protocol['catalog_file']), weights)
            device = next(actor.parameters()).device
            with ThreadPoolExecutor(max_workers=1) as executor:
                tail = finish(actor, critic, actor_optimizer, critic_optimizer, rng, steps, advantages, targets,
                              config, completed, protocol['config']['execution']['microbatch'], device, guard, executor, statistics)
            torch.save(dict(critic=critic.state_dict(), critic_optimizer=critic_optimizer.state_dict(),
                            critic_sha256=parameter_hash(critic)), directory/'after-critic-tail.pt')
            del steps, payloads, advantages, targets
            held_out = directory/'held-out'
            held_out.mkdir()
            tasks = [(i, str(held_out/f'episode-{i:04d}'), 411000000+i, 421000000+i) for i in range(args.attempts)]
            results = []
            with ProcessPoolExecutor(max_workers=args.workers, mp_context=multiprocessing.get_context('spawn'),
                    initializer=initialize, initargs=(protocol['spec'], str(weights), selection, job['candidate'])) as pool:
                pending = [pool.submit(collect, task) for task in tasks]
                for future in as_completed(pending, timeout=max(1, args.seconds-(time.monotonic()-started))):
                    guard()
                    results.append(future.result())
            results.sort(key=lambda r:r['identity']['index'])
            before, after, paired = [], [], []
            positions = [[] for _ in range(selection['horizon'])]
            runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
            try:
                for result in results:
                    guard()
                    verify_result(runtime, result, directory)
                    inputs[result['result_path']] = result['result_sha256']
                    g = result['generation']
                    key = result['identity']['generation_seed']
                    identity = tuple(g.get(k) for k in ('status', 'request_sha256', 'initial_sha256', 'geometry_sha256'))
                    require(geometry.setdefault(key, identity) == identity, f'held-out initial factory differs: {key}')
                    if result['status'] != 'complete':
                        continue
                    payload = load_experience(result)
                    inputs[result['experience']] = result['experience_sha256']
                    trajectory = payload['steps']
                    expected = [s['old_value'] for s in trajectory]
                    observed = values(baseline, trajectory, config, device, 32)
                    torch.testing.assert_close(torch.tensor(observed), torch.tensor(expected), rtol=5e-5, atol=5e-6)
                    changed = values(critic, trajectory, config, device, 32)
                    a = prediction_moments([dict(s, old_value=v) for s,v in zip(trajectory, observed)])
                    b = prediction_moments([dict(s, old_value=v) for s,v in zip(trajectory, changed)])
                    remaining, returns = 0., []
                    for step in reversed(trajectory):
                        remaining += step['reward']
                        returns.append(remaining)
                    returns.reverse()
                    for index, values_at_step in enumerate(zip(observed, changed, returns)):
                        positions[index].append(values_at_step)
                    before.append(dict(critic_prediction=a))
                    after.append(dict(critic_prediction=b))
                    paired.append(dict(generation_seed=key, before={k:summarize(v) for k,v in a.items()},
                                       after={k:summarize(v) for k,v in b.items()}))
            finally:
                runtime.close()
            (directory/'best.json').unlink(missing_ok=True)
            cleanup_phase(output, held_out, results, retain_training=True)
            require(len(before) >= 2, 'at least two complete held-out episodes required')
            detail = dict(batch=number, original_actor_epochs=completed, final_critic_epochs=config['epochs'],
                original_update_exact=True, tail=tail, policy_sha256=parameter_hash(actor),
                baseline_checkpoint_sha256=sha(weights), tail_checkpoint_sha256=sha(directory/'after-critic-tail.pt'),
                before_critic_sha256=parameter_hash(baseline), after_critic_sha256=parameter_hash(critic),
                training_value_loss_before=actual['epochs'][-1]['value_loss'],
                training_value_loss_after=tail['epochs'][-1]['value_loss'],
                held_out_accepted=len(before), held_out_attempted=len(results),
                before=aggregate(before)['critic_prediction'], after=aggregate(after)['critic_prediction'], pairs=paired,
                positions=[dict(action=index+1, before=summarize(moments([(a,r) for a,b,r in rows])),
                                after=summarize(moments([(b,r) for a,b,r in rows])))
                           for index, rows in enumerate(positions)])
            report['batches'].append(detail)
            write(output/'report.json', report)
            print(dict(batch=number, held_out=len(before), before=detail['before']['all'], after=detail['after']['all']), flush=True)
            del model_set, actor, critic, baseline, actor_optimizer, critic_optimizer
            torch.cuda.empty_cache()
        report.update(status='complete', seconds=time.monotonic()-started)
    except BaseException as error:
        report.update(status='failed', seconds=time.monotonic()-started,
                      diagnostic=dict(code='ppo-combinations.mature-probe-failed', severity='error',
                                      stage='critic-probe', entity=str(root), message=str(error)))
        raise
    finally:
        write(output/'inputs.json', inputs)
        report['input_manifest_sha256'] = sha(output/'inputs.json')
        write(output/'report.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--batches', type=int, nargs='+', default=[128, 192, 256])
    parser.add_argument('--attempts', type=int, default=32)
    parser.add_argument('--workers', type=int, default=8)
    parser.add_argument('--seconds', type=float, default=900)
    args = parser.parse_args()
    if not (1 <= args.workers <= 8 and 2 <= args.attempts <= 32 and 0 < args.seconds <= 900):
        parser.error('workers 1..8, attempts 2..32 and seconds in (0,900] are required')
    if not args.batches or len(set(args.batches)) != len(args.batches) or any(b < 2 for b in args.batches):
        parser.error('unique retained batch numbers >=2 are required')
    run(args)
