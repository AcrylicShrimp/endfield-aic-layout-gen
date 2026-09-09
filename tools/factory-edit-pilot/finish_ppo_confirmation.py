"""Summarize an explicitly shortened PPO confirmation without repeating training."""
import argparse
from pathlib import Path
import time

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from direct_area_study import checked
from full_factory_screen import sha
from plot_ppo_area_study import snapshot
from ppo_area_model import require
from random_factory_generation import read, write


def finish(screen):
    target = screen / 'confirmation.json'
    require(not target.exists(), f'confirmation output already exists: {target}')
    protocol_path, progress_path = screen / 'protocol.json', screen / 'progress.json'
    amendment_path, stop_path = screen / 'user-budget-amendment.json', screen / 'user-budget-stop.json'
    protocol, progress, amendment, stop = map(read, (protocol_path, progress_path, amendment_path, stop_path))
    require(progress['status'] != 'running', 'coordinator must have stopped before finalization')
    require(amendment['schema'] == 'ppo-user-budget-amendment-v1' and
            amendment['protocol_sha256'] == progress['protocol_sha256'] == sha(protocol_path), 'user amendment identity')
    require(stop['status'] == 'stop-requested-at-evaluation', 'supervisor reached the requested evaluation')
    limits = amendment['seed_group_limits']
    require({str(s['seed']) for s in progress['seeds']} == set(limits), 'all requested confirmation seeds')
    seeds, total_actions = [], 0
    for state in progress['seeds']:
        seed, limit = state['seed'], limits[str(state['seed'])]
        require(0 < limit <= protocol['spec']['learning']['groups'] and len(state['groups']) == limit,
                f'exact user batch limit for seed {seed}')
        scheduled = [n for n in protocol['controls']['evaluate_at'] if n <= limit]
        require(scheduled[-1] == limit and [e['update'] for e in state['evaluations']] == scheduled,
                f'all requested evaluations for seed {seed}')
        groups, evaluations = [checked(g) for g in state['groups']], [checked(e) for e in state['evaluations']]
        previous_actor, previous_critic = None, None
        for index, group in enumerate(groups):
            require(group['group'] == index and group['transition_reconstructed'], 'committed online transition record')
            require(group['audit']['native_initial_and_best'] and group['audit']['experience_and_rewards'],
                    'existing native and experience acceptance record')
            if previous_actor is not None:
                require((group['policy_before_sha256'], group['critic_before_sha256']) == (previous_actor, previous_critic),
                        'contiguous committed actor/critic versions')
            previous_actor, previous_critic = group['policy_after_sha256'], group['critic_after_sha256']
        for evaluation in evaluations:
            require(evaluation['audit']['native_initial_and_best'] and evaluation['audit']['experience_and_rewards'],
                    'existing evaluation acceptance record')
        require(state['checkpoint'] == groups[-1]['checkpoint'], 'final committed checkpoint reference')
        require(sha(state['checkpoint']['path']) == state['checkpoint']['sha256'], 'retained final checkpoint bytes')
        selections = sum(g['selected'] for g in groups)
        require(selections == state['selected'], 'committed selection accounting')
        total_actions += selections
        seeds.append(dict(seed=seed, batches=limit, selections=selections,
                          attempts=sum(g['attempted'] for g in groups), admitted=sum(g['accepted'] for g in groups),
                          rejected=sum(g['rejected'] for g in groups), checkpoint=state['checkpoint'],
                          groups=state['groups'], evaluations=state['evaluations'],
                          optimizer_steps=sum(g['optimizer_steps'] for g in groups),
                          kl_stopped_batches=sum(g['kl_stopped'] for g in groups),
                          collection_seconds=sum(g['timing']['seconds'] for g in groups),
                          optimization_and_reconstruction_seconds=sum(g['timing']['optimization_and_reconstruction_seconds'] for g in groups)))
    require(total_actions == progress['selected'], 'total committed selection accounting')
    final_seed = next(s for s in progress['seeds'] if s['seed'] == stop['seed'])
    require(stop['checkpoint'] == final_seed['checkpoint'] and stop['evaluation'] == final_seed['evaluations'][-1],
            'supervisor stopped at the retained endpoint')
    curve = snapshot(screen)
    curve.update(status='complete-user-shortened', diagnostic=None)
    for seed in curve['seeds']:
        seed['status'] = 'complete-user-shortened'
    confirmation = dict(schema='ppo-user-shortened-confirmation-v1', status='complete',
                        completion_kind='user-shortened', original_runner_status=progress['status'],
                        created_epoch=time.time(), protocol_sha256=sha(protocol_path),
                        progress_sha256=sha(progress_path), amendment_sha256=sha(amendment_path),
                        stop_sha256=sha(stop_path), finalizer_source_sha256=sha(__file__),
                        batches=sum(limits.values()), selections=total_actions, seeds=seeds, curve=curve,
                        verification_scope='Existing online native/experience/optimizer records and final checkpoint file identities; no additional optimizer reconstruction or repeated factory replay.')
    write(target, confirmation)
    draw_curve(confirmation, screen / 'confirmation-view')
    print(dict(status=confirmation['status'], completion_kind=confirmation['completion_kind'], batches=confirmation['batches']), flush=True)
    return confirmation


def draw_curve(confirmation, output):
    output.mkdir()
    figure, axes = plt.subplots(1, 2, figsize=(12, 4.5), layout='constrained')
    for seed in confirmation['curve']['seeds']:
        points = seed['points']
        for axis, metric in zip(axes, ('area_reduction_percent', 'delta_pp')):
            axis.plot([point['update'] for point in points],
                      [point['products']['all'][metric] for point in points], 'o-', label=f"Seed {seed['seed']}")
    axes[0].set_ylabel('Mean area reduction from starting layout (%)')
    axes[1].set_ylabel('Additional reduction vs. untrained policy (pp)')
    for axis in axes:
        axis.set_xlabel('Completed rollout batches')
        axis.grid(axis='y', alpha=.2)
        axis.legend(frameon=False)
    figure.suptitle('PPO + critic + GAE: user-shortened confirmation\n16 fixed evaluation factories per seed, 32 edits per factory')
    for extension in ('png', 'svg'):
        figure.savefig(output / f'performance.{extension}', dpi=160)
    plt.close(figure)
    write(output / 'curve.json', confirmation['curve'])


def load_confirmation(screen):
    path = screen / 'confirmation.json'
    result = read(path)
    require(result['schema'] == 'ppo-user-shortened-confirmation-v1' and result['status'] == 'complete', 'finished user-shortened confirmation')
    for name, key in (('protocol.json', 'protocol_sha256'), ('progress.json', 'progress_sha256'),
                      ('user-budget-amendment.json', 'amendment_sha256'), ('user-budget-stop.json', 'stop_sha256')):
        require(sha(screen / name) == result[key], f'confirmation source identity: {screen / name}')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    finish(parser.parse_args().screen)
