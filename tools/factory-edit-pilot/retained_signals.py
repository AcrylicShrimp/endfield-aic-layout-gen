"""Inspect saved value and advantage signals before and after first improvement."""

import argparse
from collections import defaultdict
import json
from pathlib import Path

import numpy as np
import torch

from evidence import write
from mastery_policy_selection import digest, read
from policy import advantages
from retained_tradeoff import COMPARISON


GRADIENTS = 'c19f71ef3ae92c62e55bc1ea7d1b7bc024b8d4ee3f262d48b1395c6ca38641cf'
PHASES = ('before_first_reward', 'first_reward', 'after_first_reward', 'no_positive_episode')


def require(condition, path, message):
    if not condition:
        raise ValueError(f'retained_signals.input: {path}: {message}')


def reference_signals(rewards, values, dones, gamma, lam):
    """Finite-horizon discounted returns and a closed sum of TD residuals."""
    r, v, d = np.asarray(rewards), np.asarray(values), np.asarray(dones)
    if (r.ndim != 1 or r.shape != v.shape or r.shape != d.shape or len(r) == 0
            or not d[-1] or not np.isfinite(r).all() or not np.isfinite(v).all()):
        raise ValueError('retained_signals.shape: finite complete episodes required')
    returns, gae = np.zeros(len(r)), np.zeros(len(r))
    for i in range(len(r)):
        for j in range(i, len(r)):
            returns[i] += gamma**(j-i)*r[j]
            next_value = 0 if d[j] else v[j+1]
            gae[i] += (gamma*lam)**(j-i)*(r[j]+gamma*next_value-v[j])
            if d[j]:
                break
    return returns, gae


def phases(rewards):
    first = next((i for i, reward in enumerate(rewards) if reward > 0), None)
    if first is None:
        return ['no_positive_episode']*len(rewards)
    return ['before_first_reward' if i < first else 'first_reward' if i == first else 'after_first_reward'
            for i in range(len(rewards))]


def summarize(records, config):
    rewards = [v for r in records for v in r['samples']['rewards']]
    values = [v for r in records for v in r['samples']['values']]
    dones = [v for r in records for v in r['samples']['dones']]
    gae, _ = advantages(rewards, values, dones, 0.0, config['gamma'], config['gae_lambda'])
    returns, independent = reference_signals(rewards, values, dones, config['gamma'], config['gae_lambda'])
    error = float(np.max(np.abs(gae-independent)))
    require(error < 1e-6, 'GAE', 'closed residual sum differs')
    raw_mean, raw_std = float(gae.mean()), float(gae.std())
    normalized = ((gae-gae.mean())/(gae.std()+1e-8)).astype(np.float64)
    gae = gae.astype(np.float64)
    mc = returns-np.asarray(values)
    mc_normalized = (mc-mc.mean())/(mc.std()+1e-8)
    groups, cursor = defaultdict(lambda: defaultdict(float)), 0
    for record in records:
        sample, summary = record['samples'], record['summary']
        require(len(sample['rewards']) == 16 and sample['dones'] == [False]*15+[True]
                and len(record['trace']) == 16
                and all(t['reward'] == reward for t, reward in zip(record['trace'], sample['rewards']))
                and abs(sum(sample['rewards'])-summary['return']) < 1e-10,
                summary['case'], 'episode/trace accounting differs')
        for phase in phases(sample['rewards']):
            g = groups[summary['case'], summary['start_reason'], phase]
            g['observations'] += 1
            g['positive_rewards'] += rewards[cursor] > 0
            g['positive_sampled_returns'] += returns[cursor] > 0
            g['negative_raw_gae'] += gae[cursor] < 0
            g['negative_normalized_gae'] += normalized[cursor] < 0
            g['raw_sign_disagreements'] += np.sign(gae[cursor]) != np.sign(mc[cursor])
            g['normalized_sign_disagreements'] += np.sign(normalized[cursor]) != np.sign(mc_normalized[cursor])
            g['reward_sum'] += rewards[cursor]
            g['value_sum'] += values[cursor]
            g['sampled_return_sum'] += returns[cursor]
            g['value_minus_sampled_return_sum'] += values[cursor]-returns[cursor]
            g['squared_value_error_sum'] += (values[cursor]-returns[cursor])**2
            g['raw_gae_sum'] += gae[cursor]
            g['normalized_gae_sum'] += normalized[cursor]
            g['absolute_normalized_gae_sum'] += abs(normalized[cursor])
            g['sampled_mc_advantage_sum'] += mc[cursor]
            cursor += 1
    counts = {'observations', 'positive_rewards', 'positive_sampled_returns', 'negative_raw_gae',
              'negative_normalized_gae', 'raw_sign_disagreements', 'normalized_sign_disagreements'}
    return {'observations': cursor, 'raw_gae_mean': raw_mean, 'raw_gae_std': raw_std,
            'closed_sum_max_error': error,
            'groups': [{'case': case, 'start_reason': reason, 'phase': phase,
                        **{k: int(v) if k in counts else float(v) for k, v in numbers.items()}}
                       for (case, reason, phase), numbers in groups.items()]}


def run(source, gradient_source, output):
    if output.exists():
        raise ValueError(f'retained_signals.output: {output}: choose a fresh file')
    require(digest(source/'comparison.json') == COMPARISON, source, 'comparison hash differs')
    require(digest(gradient_source/'gradients.json') == GRADIENTS, gradient_source, 'gradient hash differs')
    comparison, gradients = read(source/'comparison.json'), read(gradient_source/'gradients.json')
    require(comparison['status'] == gradients['status'] == 'complete'
            and comparison['decision']['candidate_for_larger_test'] is False, source, 'expected failed original criterion')
    result = {'schema_version': 1, 'purpose': 'retained_advantage_signals', 'status': 'complete',
              'comparison_sha256': COMPARISON, 'gradients_sha256': GRADIENTS,
              'new_commands': 0, 'new_updates': 0, 'original_criterion_passed': False, 'cells': []}
    checked = {source/'comparison.json': COMPARISON, gradient_source/'gradients.json': GRADIENTS}
    for rate in (1, 0.1):
        branch = next(b for b in comparison['branches'] if b['seed'] == 1030000 and b['multiplier'] == rate)
        directory = source/f'seed1030000-lr{rate:g}'
        collection = directory/'collected.jsonl'
        require(digest(collection) == branch['collected_sha256'], collection, 'collection hash differs')
        checked[collection] = branch['collected_sha256']
        records = [json.loads(line) for line in collection.read_text().splitlines()]
        entry = next(x for x in branch['updates'] if x['update'] == 8)
        checkpoint = directory/entry['checkpoint']
        require(digest(checkpoint) == entry['checkpoint_sha256'], checkpoint, 'configuration checkpoint hash differs')
        checked[checkpoint] = entry['checkpoint_sha256']
        config = torch.load(checkpoint, map_location='cpu', weights_only=False)['config']
        for index in range(9, 17):
            cell = summarize(records[(index-1)*64:index*64], config)
            require(cell['observations'] == 1024, collection, 'incomplete rollout')
            previous = next(c for c in gradients['cells'] if c['multiplier'] == rate and c['update'] == index)
            discrepancy = 0.0
            for row in previous['analysis']['cases']:
                for start in row['starts']:
                    selected = [g for g in cell['groups'] if g['case'] == row['case'] and g['start_reason'] == start['start_reason']]
                    n = sum(g['observations'] for g in selected)
                    require(n == start['observations'], collection, 'case/start observation count differs')
                    mean = sum(g['normalized_gae_sum'] for g in selected)/n
                    discrepancy = max(discrepancy, abs(mean+start['incoming_losses']['policy']))
            require(discrepancy < 1e-4, collection, 'incoming policy loss disagrees with normalized advantages')
            result['cells'].append({'multiplier': rate, 'update': index,
                'config_checkpoint_sha256': entry['checkpoint_sha256'], 'gamma': config['gamma'],
                'gae_lambda': config['gae_lambda'], 'incoming_policy_loss_max_discrepancy': discrepancy, **cell})
    require(all(digest(p) == expected for p, expected in checked.items()), source, 'input changed during analysis')
    output.parent.mkdir(parents=True, exist_ok=True)
    write(output, result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--gradients', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.source.resolve(), args.gradients.resolve(), args.output.resolve())
