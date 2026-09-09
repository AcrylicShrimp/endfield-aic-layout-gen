"""Reopen a completed direct-area study and independently rebuild every Adam transition."""
import argparse
from collections import Counter
from pathlib import Path
import time

import torch

from analyze_fresh_factory_pilot import coverage
from direct_area_audit import verify_result, replay, verify_checkpoint
from direct_area_reward import require, update
from direct_area_study import checked
from full_factory_environment import FactoryRuntime
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from plot_direct_area_study import snapshot
from random_factory_audit import optimizer_hash
from random_factory_collection import check_sources
from random_factory_generation import read, write
from random_factory_pilot import configure_torch, new_model


def run(args):
    configure_torch()
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    report_path = args.screen / 'report.json'
    report = read(report_path)
    require(report['status'] == 'complete' and report['protocol_sha256'] == sha(protocol_path), 'completed audited protocol')
    require(len(report['seeds']) == len(protocol['spec']['learning']['seeds']), 'all registered seeds')
    args.output.mkdir()
    began = time.monotonic()
    runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
    verified, transitions, replayed, seeds = 0, 0, [], []
    def guard():
        if time.monotonic() - began > args.seconds:
            raise TimeoutError('direct-reward.audit-budget')
    try:
        for state in report['seeds']:
            seed, spec = state['seed'], protocol['spec']
            require(state['status'] == 'complete' and len(state['groups']) == spec['learning']['groups'], 'all registered groups')
            groups = [checked(g) for g in state['groups']]
            evaluations = [checked(e) for e in state['evaluations']]
            require([e['update'] for e in evaluations] == protocol['controls']['evaluate_at'], 'all registered evaluations')
            model = new_model(spec, seed)
            optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
            initial = torch.load(args.screen / f'seed-{seed}' / 'initial.pt', map_location='cpu', weights_only=False)
            require(parameter_hash(model) == initial['policy_sha256'], 'untrained initialization')
            optimizer.load_state_dict(initial['optimizer'])
            torch.set_rng_state(initial['rng'])
            previous = parameter_hash(model)
            for index, group in enumerate(groups):
                guard()
                require(group['group'] == index and group['policy_before_sha256'] == previous, 'contiguous policy chain')
                for result in group['results']:
                    guard()
                    verify_result(runtime, result, args.output)
                    verified += 1
                repeated = update(model, optimizer, group['results'], spec['learning'])
                require(all(repeated[key] == group[key] for key in repeated), 'reconstructed accounting')
                require(optimizer_hash(optimizer) == group['optimizer_sha256'], 'reconstructed Adam state')
                checkpoint = group['checkpoint']
                require(sha(checkpoint['path']) == checkpoint['sha256'], 'checkpoint file identity')
                verify_checkpoint(checkpoint['path'], model, optimizer)
                previous = parameter_hash(model)
                transitions += 1
                if index in (0, len(groups) - 1):
                    sample = next(r for r in group['results'] if r['status'] == 'complete')
                    replayed.append(replay(runtime, protocol, sample, args.output / f'replay-{seed}-{index}', guard))
            require(previous == state['checkpoint']['policy_sha256'], 'final policy identity')
            for evaluation in evaluations:
                guard()
                expected = initial['policy_sha256'] if evaluation['update'] == 0 else groups[evaluation['update'] - 1]['policy_after_sha256']
                require(evaluation['policy_sha256'] == expected, 'evaluation checkpoint position')
                for result in evaluation['results']:
                    verify_result(runtime, result, args.output)
                    verified += 1
            results = [r for group in groups for r in group['results']]
            seeds.append(dict(seed=seed, training=coverage(results),
                zero_reward_episodes=sum(g['zero_reward_episodes'] for g in groups),
                reward_gradient_norms=[g['reward_gradient_norm'] for g in groups],
                entropy_gradient_norms=[g['entropy_gradient_norm'] for g in groups]))
        curve = snapshot(args.screen)
        write(args.output / 'report.json', dict(status='complete', smoke=protocol['smoke'], protocol_sha256=sha(protocol_path),
            report_sha256=sha(report_path), audit_source_sha256=sha(__file__), verified_results=verified,
            reconstructed_transitions=transitions, replayed=replayed, learning=seeds, curve=curve,
            seconds=time.monotonic() - began))
        (args.output / 'best.json').unlink(missing_ok=True)
        print(dict(status='audited', verified_results=verified, transitions=transitions,
                   replayed_actions=sum(r['selected'] for r in replayed)), flush=True)
    finally:
        runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=3600)
    args = parser.parse_args()
    if not 0 < args.seconds <= 7200:
        parser.error('audit seconds must be in (0, 7200]')
    run(args)
