"""Audit every PPO behavior/credit record and checkpoint, with declared full update replays."""
import argparse
from pathlib import Path
import time

import torch

from analyze_fresh_factory_pilot import coverage
from direct_area_study import checked
from full_factory_environment import FactoryRuntime
from full_factory_large_learning import unpack
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from plot_ppo_area_study import snapshot
from ppo_area_audit import verify_result, verify_checkpoint, replay
from ppo_area_model import require
from ppo_area_study import learners
from ppo_area_update import collect_steps, update, sha_bytes
from random_factory_audit import optimizer_hash
from random_factory_collection import check_sources
from random_factory_generation import read, write
from random_factory_pilot import configure_torch


def delayed_gains(results, credits):
    count, examples, offset = 0, [], 0
    for result in results:
        if result['status'] != 'complete':
            continue
        summary = unpack(Path(result['row']['directory']) / 'episode.json.gz')
        previous = summary['initial_metrics']['score'][0]
        for index, step in enumerate(summary['trace']):
            future = next((j for j in range(index + 1, len(summary['trace'])) if summary['trace'][j]['reward'] > 0), None)
            if step['current_score'][0] > previous and future is not None:
                count += 1
                if len(examples) < 3:
                    examples.append(dict(result=result['result_path'], product=result['generation']['draw']['product'],
                        worsening_action=index + 1, later_record_action=future + 1,
                        current_area_before=previous, current_area_after=step['current_score'][0],
                        immediate_reward=step['reward'], future_return=step['reward_to_go'],
                        predicted_remaining_gain=step['old_value'], **credits[offset + index]))
            previous = step['current_score'][0]
        offset += len(summary['trace'])
    require(offset == len(credits), 'credit record episode alignment')
    return count, examples


def run(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    report_path = args.screen / 'report.json'
    report = read(report_path)
    require(report['status'] == 'complete' and report['protocol_sha256'] == sha(protocol_path), 'completed PPO study')
    require(len(report['seeds']) == len(protocol['spec']['learning']['seeds']), 'all registered PPO seeds')
    args.output.mkdir()
    runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
    began = time.monotonic()
    verified, behavior_steps, online_transitions, rebuilt, replayed, seeds = 0, 0, 0, [], [], []
    def guard():
        if time.monotonic() - began > args.seconds:
            raise TimeoutError('ppo-area.audit-budget')
    try:
        for state in report['seeds']:
            seed, spec = state['seed'], protocol['spec']
            config = dict(spec['selection'], **protocol['controls']['ppo'])
            groups, evaluations = [checked(g) for g in state['groups']], [checked(e) for e in state['evaluations']]
            require(state['status'] == 'complete' and len(groups) == spec['learning']['groups'] and
                    [e['update'] for e in evaluations] == protocol['controls']['evaluate_at'], 'registered PPO groups/evaluations')
            actor, critic, aopt, copt, rng = learners(spec, seed, config)
            initial_path = args.screen / f'seed-{seed}' / 'initial.pt'
            initial = torch.load(initial_path, map_location='cpu', weights_only=False)
            require(parameter_hash(actor) == initial['policy_sha256'] and parameter_hash(critic) == initial['critic_sha256'], 'initial actor/critic')
            previous = initial
            cases, delayed_count, examples, payload_bytes, optimization_seconds = [], 0, [], 0, 0.0
            for index, group in enumerate(groups):
                guard()
                require(group['group'] == index and group['policy_before_sha256'] == previous['policy_sha256'] and
                        group['critic_before_sha256'] == previous['critic_sha256'] and group['transition_reconstructed'],
                        'contiguous online-reconstructed actor/critic transition')
                actor.load_state_dict(previous['policy'], strict=True)
                critic.load_state_dict(previous['critic'], strict=True)
                aopt.load_state_dict(previous['actor_optimizer'])
                copt.load_state_dict(previous['critic_optimizer'])
                rng.set_state(previous['shuffle_rng'])
                torch.set_rng_state(previous['rng'])
                for result in group['results']:
                    guard()
                    verify_result(runtime, result, args.output)
                    verified += 1
                    payload_bytes += result.get('experience_bytes', 0)
                steps, advantages, targets, credit = collect_steps(actor, critic, group['results'], config)
                require(all(credit[k] == group[k] for k in credit), 'all old behavior values and GAE credit')
                behavior_steps += len(steps)
                del steps, advantages, targets
                count, sample_examples = delayed_gains(group['results'], credit['credit'])
                delayed_count += count
                examples.extend(sample_examples[:max(0, 8 - len(examples))])
                cp = group['checkpoint']
                require(sha(cp['path']) == cp['sha256'], 'PPO checkpoint file')
                saved = torch.load(cp['path'], map_location='cpu', weights_only=False)
                if index in (0, len(groups) - 1):
                    repeated = update(actor, critic, aopt, copt, rng, group['results'], config, guard)
                    require(all(repeated[k] == group[k] for k in repeated), 'independent selected PPO update reconstruction')
                    verify_checkpoint(cp['path'], actor, critic, aopt, copt, rng)
                    rebuilt.append(dict(seed=seed, batch=index + 1, optimizer_steps=group['optimizer_steps']))
                    sample = next(r for r in group['results'] if r['status'] == 'complete')
                    replayed.append(replay(runtime, protocol, sample, args.output / f'replay-{seed}-{index}', guard))
                actor.load_state_dict(saved['policy'], strict=True)
                critic.load_state_dict(saved['critic'], strict=True)
                aopt.load_state_dict(saved['actor_optimizer'])
                copt.load_state_dict(saved['critic_optimizer'])
                require(parameter_hash(actor) == group['policy_after_sha256'] == saved['policy_sha256'] and
                        parameter_hash(critic) == group['critic_after_sha256'] == saved['critic_sha256'] and
                        optimizer_hash(aopt) == group['actor_optimizer_sha256'] and
                        optimizer_hash(copt) == group['critic_optimizer_sha256'], 'every checkpoint state hash')
                require(sha_bytes(saved['shuffle_rng']) == group['shuffle_rng_sha256'] and
                        torch.equal(saved['rng'], previous['rng']), 'every checkpoint RNG state')
                previous = saved
                online_transitions += 1
                optimization_seconds += group['timing']['optimization_and_reconstruction_seconds']
                cases.extend(group['results'])
            require(previous['policy_sha256'] == state['checkpoint']['policy_sha256'] and
                    previous['critic_sha256'] == state['checkpoint']['critic_sha256'], 'final PPO endpoint')
            for evaluation in evaluations:
                expected = initial['policy_sha256'] if evaluation['update'] == 0 else groups[evaluation['update'] - 1]['policy_after_sha256']
                require(evaluation['policy_sha256'] == expected, 'PPO evaluation checkpoint')
                for result in evaluation['results']:
                    guard()
                    verify_result(runtime, result, args.output)
                    verified += 1
            seeds.append(dict(seed=seed, training=coverage(cases), experience_bytes=payload_bytes,
                optimization_and_online_reconstruction_seconds=optimization_seconds,
                optimizer_steps=sum(g['optimizer_steps'] for g in groups), kl_stopped_batches=sum(g['kl_stopped'] for g in groups),
                delayed_gain_sequences=delayed_count, delayed_gain_examples=examples))
        write(args.output / 'report.json', dict(status='complete', smoke=protocol['smoke'], protocol_sha256=sha(protocol_path),
            report_sha256=sha(report_path), audit_source_sha256=sha(__file__), verified_results=verified,
            behavior_and_gae_steps=behavior_steps, online_transition_reconstructions=online_transitions,
            independently_rebuilt_batches=rebuilt, replayed=replayed, learning=seeds, curve=snapshot(args.screen),
            seconds=time.monotonic() - began,
            scope='Every witness/behavior/GAE/checkpoint verified; online replay covers all updates, offline replay covers first/last batches per seed.'))
        (args.output / 'best.json').unlink(missing_ok=True)
        print(dict(status='audited', results=verified, behavior_steps=behavior_steps,
                   independently_rebuilt_batches=len(rebuilt)), flush=True)
    finally:
        runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=7200)
    args = parser.parse_args()
    if not 0 < args.seconds <= 14400:
        parser.error('audit seconds must be in (0, 14400]')
    run(args)
