"""Bounded native collection/update/resume integration check, never a training mode."""
import argparse
from pathlib import Path
import time

import torch

from benchmark_ppo_optimization import fresh
from direct_area_study import checked
from finish_ppo_confirmation import load_confirmation
from native_factory_runtime import FactoryRuntime
from full_factory_input_contracts import CONTRACTS
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from ppo_area_audit import native_batch
from ppo_area_study import learners
from ppo_area_update import save_experience, load_experience
from ppo_training_batch import EXECUTION_SCHEMA, require
from ppo_training_rollout import rollout
from ppo_training_update import update, collect_steps
from random_factory_collection import check_sources
from random_factory_generation import read, write
from random_factory_pilot import configure_torch


def run(args):
    configure_torch()
    require(not args.output.exists(), f'fresh native check output: {args.output}')
    began = time.monotonic()
    def guard():
        require(time.monotonic() - began < 180, 'native execution check budget')
    confirmation = load_confirmation(args.screen)
    protocol = read(args.screen / 'protocol.json')
    check_sources(protocol)
    seed = next(s for s in confirmation['seeds'] if s['seed'] == 17)
    group = checked(seed['groups'][7])
    initial = checked(seed['groups'][6])['checkpoint']
    require(sha(initial['path']) == initial['sha256'], 'initial checkpoint identity')
    checkpoint = torch.load(initial['path'], map_location='cpu', weights_only=False)
    config = dict(protocol['spec']['selection'], **protocol['controls']['ppo'])
    workload = dict(spec=protocol['spec'], config=config, checkpoint=checkpoint)
    actor, critic, aopt, copt = fresh(workload)
    rng = torch.Generator().set_state(checkpoint['shuffle_rng'])
    model_set = (actor, critic, aopt, copt, rng)
    samples = [r for r in group['results'] if r['status'] == 'complete'][:2]
    args.output.mkdir(parents=True)
    report = dict(schema='ppo-native-execution-check-v1', status='running', input_contract=args.candidate, initial_checkpoint=initial,
                  sources={name: sha(Path(__file__).with_name(name)) for name in
                           ['check_ppo_training_execution.py', 'ppo_training_update.py', 'ppo_training_batch.py',
                            'ppo_training_rollout.py', 'ppo_training_observation.py', 'ppo_training_world.py']}, batches=[],
                  scope='Two native episodes per batch, two on-policy batches, saved nonempty Adam/shuffle state, one bounded continuation comparison outside training.')
    runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
    try:
        for batch_index in range(2):
            batch_dir = args.output / f'batch-{batch_index}'
            batch_dir.mkdir()
            results = []
            for episode_index, original in enumerate(samples):
                guard()
                directory = batch_dir / f'episode-{episode_index}'
                directory.mkdir()
                task = read(Path(original['result_path']).with_name('task.json'))
                policy_path = Path(initial['path']) if batch_index == 0 else args.output / 'checkpoint.pt'
                task.update(policy_sha256=parameter_hash(actor), critic_sha256=parameter_hash(critic),
                            policy=str(policy_path), policy_file_sha256=sha(policy_path),
                            action_seed=task['action_seed'] + batch_index * 10000000)
                task['identity'] = dict(phase=f'native-check-{batch_index}', index=episode_index,
                                        generation_seed=task['generation_seed'], action_seed=task['action_seed'])
                write(directory / 'task.json', task)
                (directory / 'generated').mkdir()
                write(directory / 'generated/generation.json', original['generation'])
                input_sha256 = sha(directory / 'task.json')
                row, payload = rollout(runtime, dict(config=dict(config, horizon=task['horizon'])),
                                       original['generation']['case'], actor, critic, args.candidate, task['action_seed'],
                                       directory / 'rollout', True, guard)
                payload.update(policy_sha256=row['policy_sha256'], input_sha256=input_sha256)
                experience = directory / 'experience.pt.gz'
                save_experience(experience, payload)
                result = dict(original, row=row, candidate=args.candidate, identity=task['identity'], input_sha256=input_sha256, policy_sha256=task['policy_sha256'], critic_sha256=task['critic_sha256'],
                              experience=str(experience), experience_sha256=sha(experience),
                              result_path=str(directory / 'result.json'))
                result.pop('result_sha256', None)
                write(directory / 'result.json', result)
                result['result_sha256'] = sha(directory / 'result.json')
                results.append(result)
            # The extra witness/experience audit is deliberate integration-test work.
            audit = native_batch(runtime, results, batch_dir / 'test-audit')
            experiences = [load_experience(r) for r in results]
            if batch_index == 1:
                state = torch.load(args.output / 'checkpoint.pt', weights_only=True)
                resumed = learners(protocol['spec'], 17, config)
                for model, key in zip(resumed[:4], ('policy', 'critic', 'actor_optimizer', 'critic_optimizer')):
                    model.load_state_dict(state[key])
                resumed[4].set_state(state['shuffle_rng'])
                require(resumed[2].state and resumed[3].state, 'resume nonempty Adam')
                steps, advantages, targets, _ = collect_steps(actor, critic, results, config, experiences=experiences)
                require(len(steps) == config['minibatch_size'], 'one complete representative minibatch')
                torch.save(dict(spec=protocol['spec'], config=config, checkpoint=state, steps=steps,
                                advantages=advantages, targets=targets,
                                identity=dict(input_contract=args.candidate, native_report=str(args.output / 'report.json'),
                                              checkpoint_sha256=sha(args.output / 'checkpoint.pt'),
                                              experiences=[r['experience_sha256'] for r in results])),
                           args.output / 'workload.pt')
            started = time.monotonic()
            metrics = update(*model_set, results, config, microbatch=8, guard=guard, experiences=experiences)
            seconds = time.monotonic() - started
            if batch_index == 0:
                torch.save(dict(schema=EXECUTION_SCHEMA, policy=actor.state_dict(), critic=critic.state_dict(),
                                actor_optimizer=aopt.state_dict(), critic_optimizer=copt.state_dict(),
                                shuffle_rng=rng.get_state()), args.output / 'checkpoint.pt')
            else:
                continuation = update(*resumed, results, config, microbatch=8, guard=guard, experiences=experiences)
                for key in ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
                            'critic_optimizer_sha256', 'shuffle_rng_sha256', 'credit', 'epochs'):
                    require(metrics[key] == continuation[key], f'exact resumed {key}')
                report['native_checkpoint_continuation_exact'] = True
            report['batches'].append(dict(batch=batch_index, results=results, audit=audit, update=metrics, update_seconds=seconds))
            write(args.output / 'report.json', report)
            print(dict(batch=batch_index, native_valid=True, selections=metrics['selected'],
                       optimizer_steps=metrics['optimizer_steps'], update_seconds=seconds), flush=True)
    finally:
        runtime.close()
    report.update(status='complete', seconds=time.monotonic() - began)
    write(args.output / 'report.json', report)
    print(dict(status='complete', native_checkpoint_continuation_exact=True, seconds=report['seconds']), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--candidate', choices=CONTRACTS, required=True)
    args = parser.parse_args()
    existed = args.output.exists()
    try:
        run(args)
    except BaseException as error:
        if not existed and args.output.exists():
            path = args.output / 'report.json'
            report = read(path) if path.exists() else dict(schema='ppo-native-execution-check-v1')
            report.update(status='failed', diagnostic=dict(code='ppo-training.native-check-failed', severity='error',
                                                          stage='integration-check', entity=str(args.screen), message=str(error)))
            write(path, report)
        raise
