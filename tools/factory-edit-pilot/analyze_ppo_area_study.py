"""Descriptive critic calibration against realized returns, without changing learning."""
import argparse
from collections import Counter
import math
from pathlib import Path

from full_factory_large_learning import unpack
from full_factory_screen import sha
from ppo_area_model import require
from random_factory_generation import read, write


def calibration(pairs):
    if not pairs:
        return dict(samples=0)
    n = len(pairs)
    prediction = math.fsum(p[0] for p in pairs) / n
    target = math.fsum(p[1] for p in pairs) / n
    errors = [p - t for p, t in pairs]
    bias = math.fsum(errors) / n
    variance = math.fsum((t - target) ** 2 for _, t in pairs) / n
    residual_variance = math.fsum((e - bias) ** 2 for e in errors) / n
    return dict(samples=n, prediction_mean=prediction, realized_return_mean=target,
        bias=bias, mean_squared_error=math.fsum(e * e for e in errors) / n,
        explained_variance=1 - residual_variance / variance if variance > 1e-12 else None)


def evaluation_behavior(manifest):
    require(sha(manifest['path']) == manifest['sha256'], 'supplement evaluation hash')
    evaluation = read(manifest['path'])
    kinds, improvements, statuses, rewards = Counter(), Counter(), Counter(), {}
    episodes = 0
    for result in evaluation['results']:
        require(sha(result['result_path']) == result['result_sha256'], 'supplement evaluation result hash')
        require(result['status'] == 'complete', 'complete registered evaluation episode')
        summary = unpack(Path(result['row']['directory']) / 'episode.json.gz')
        require(len(summary['trace']) == result['row']['selected'], 'evaluation trace horizon')
        episodes += 1
        for step in summary['trace']:
            kind = step['action']['kind']
            kinds[kind] += 1
            statuses[step['status']] += 1
            improvements[kind] += step['status'] == 'improved'
            rewards.setdefault(kind, []).append(step['reward'])
    require(episodes > 0, 'nonempty evaluation behavior bank')
    return dict(batch=evaluation['update'], episodes=episodes, selections=sum(kinds.values()),
                selected_kinds=dict(kinds), new_best_score_kinds=dict(improvements), statuses=dict(statuses),
                mean_area_reduction_pp_by_rewarded_kind={kind: 100 * math.fsum(values) / episodes for kind, values in rewards.items()})


def run(args):
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    progress = read(args.screen / 'progress.json')
    require(progress['protocol_sha256'] == sha(protocol_path), 'supplement protocol identity')
    require(progress['status'] == 'complete' or args.allow_incomplete, 'completed run or explicit partial analysis')
    seeds = []
    for state in progress['seeds']:
        reference_manifest = protocol['reference'][str(state['seed'])]
        require(sha(reference_manifest['path']) == reference_manifest['sha256'], 'direct reference report hash')
        reference = read(reference_manifest['path'])
        batches, by_product, delayed = [], {}, Counter()
        for manifest in state['groups']:
            require(sha(manifest['path']) == manifest['sha256'], 'supplement committed batch hash')
            group = read(manifest['path'])
            reference_group = reference['groups'][group['group']]
            require(sha(reference_group['path']) == reference_group['sha256'], 'direct reference batch hash')
            matched = read(reference_group['path'])['results']
            require(len(group['results']) == len(matched), 'same factory-attempt denominator')
            for result, original in zip(group['results'], matched):
                require(result['status'] == original['status'], 'same admitted and rejected factory attempts')
                for key in ('generation_seed', 'request_sha256', 'initial_sha256', 'geometry_sha256'):
                    require(result['generation'].get(key) == original['generation'].get(key), f'matched generated {key}')
            pairs, offset = [], 0
            counts = Counter()
            for result in group['results']:
                require(sha(result['result_path']) == result['result_sha256'], 'supplement result hash')
                if result['status'] != 'complete':
                    continue
                summary = unpack(Path(result['row']['directory']) / 'episode.json.gz')
                trace = summary['trace']
                require(len(trace) == result['row']['selected'], 'supplement complete trace')
                product = result['generation']['draw']['product']
                previous_area = summary['initial_metrics']['score'][0]
                for index, step in enumerate(trace):
                    pair = step['old_value'], step['reward_to_go']
                    require(all(math.isfinite(v) for v in pair), 'finite critic/return pair')
                    pairs.append(pair)
                    by_product.setdefault(product, []).append(pair)
                    credit = group['credit'][offset + index]
                    if step['current_score'][0] > previous_area and any(t['reward'] > 0 for t in trace[index + 1:]):
                        counts['worsening_actions_with_later_record'] += 1
                        counts['positive_raw_gae'] += credit['raw_advantage'] > 0
                        counts['positive_normalized_gae'] += credit['normalized_advantage'] > 0
                        counts['immediate_zero_reward'] += step['reward'] == 0
                    previous_area = step['current_score'][0]
                offset += len(trace)
            require(offset == len(group['credit']), 'supplement credit alignment')
            delayed.update(counts)
            batches.append(dict(batch=group['group'] + 1, behavior_critic_vs_realized_return=calibration(pairs),
                delayed_credit=dict(counts), final_epoch_gae_target_explained_variance=group['epochs'][-1]['explained_variance']))
        seeds.append(dict(seed=state['seed'], batches=batches, delayed_credit=dict(delayed),
            all_committed_factory_inputs_match_direct_reference=True,
            evaluation_behavior=[evaluation_behavior(manifest) for manifest in state['evaluations']],
            product_behavior_calibration={name: calibration(pairs) for name, pairs in by_product.items()}))
    write(args.output, dict(schema='ppo-area-descriptive-supplement-v1', status=progress['status'],
        protocol_sha256=sha(protocol_path), analysis_source_sha256=sha(__file__), completed_batches=progress['updates'],
        scope='Post-hoc description of training trajectories, not held-out critic accuracy or a causal detour experiment.',
        metric='Behavior critic before collection versus actual undiscounted remaining return; no bootstrap in this diagnostic target.',
        behavior_scope='Evaluation action/reward attribution is descriptive: masks and states change, and these counts do not measure causal action importance or compare a fixed-kind heuristic.',
        seeds=seeds))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--allow-incomplete', action='store_true')
    args = parser.parse_args()
    try:
        run(args)
    except BaseException as error:
        write(args.output.with_suffix('.error.json'), dict(code='ppo-area.supplement-failed', severity='error',
            stage='descriptive-analysis', entity=str(args.screen), message=str(error)))
        raise
