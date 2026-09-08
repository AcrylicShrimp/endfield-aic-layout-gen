"""Inspect gradients on reconstructed historical rollouts without training."""

import argparse
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch

from evidence import provenance, write
from gradient_contributions import COMPONENTS, components, cosine, summarize, targets
from mastery_policy_selection import digest, read
from native import Runtime, layout, trace
from native_training_comparison import equal, policy_hash, rows
from policy import Policy, batch
from prefix_recovery import replay
from reference.acceptance import Catalog
from retained_tradeoff import COMPARISON


def require(condition, path, message):
    if not condition:
        raise ValueError(f'retained_gradients.input: {path}: {message}')


def recorded_checkpoint(directory, branch, index):
    entry = next(row for row in branch['updates'] if row['update'] == index)
    path = directory/entry['checkpoint']
    require(digest(path) == entry['checkpoint_sha256'], path, 'checkpoint hash differs')
    saved = torch.load(path, weights_only=False, map_location='cpu')
    require(equal(saved['policy'], saved['evaluation_policy']), path, 'learner and actor differ')
    return saved, path, entry['checkpoint_sha256']


def reconstruct(actor, runtimes, catalogs, records, config, tolerance):
    """Retain original episode-major rollout order and collector forward shapes."""
    keys = list(runtimes)
    require(len(records) == 64 and len(keys) == 16, 'collection', 'expected four rounds of sixteen streams')
    observations = [[] for _ in records]
    max_log, max_value, setup = 0.0, 0.0, 0
    before = policy_hash(actor.state_dict())
    for offset in range(0, 64, 16):
        wave = records[offset:offset+16]
        require([r['summary']['case'] for r in wave] == keys, 'collection', 'stream/case ordering differs')
        engines = [runtimes[key].new_engine() for key in keys]
        for env, row in zip(engines, wave):
            env.reset_with_prefix_json(json.dumps(row['prefix']))
            setup += len(row['prefix'])
            require(row['samples']['dones'] == [False]*15+[True], 'collection', 'episode boundary differs')
        for step in range(16):
            observed = [env.observe() for env in engines]
            with torch.no_grad():
                data = {k: v.to('cuda') for k, v in batch([o[0] for o in observed]).items()}
                logits, values = actor(data)
                log_probs = logits.log_softmax(-1).cpu()
                values = values.cpu().tolist()
            for i, (env, observation, row) in enumerate(zip(engines, observed, wave)):
                action = row['samples']['actions'][step]
                require(0 <= action < len(observation[0]['candidates']), keys[i], 'recorded action outside domain')
                max_log = max(max_log, abs(log_probs[i, action].item()-row['samples']['log_probs'][step]))
                max_value = max(max_value, abs(values[i]-row['samples']['values'][step]))
                reward, done, _ = env.step(observation[1], action)
                require(abs(reward-row['samples']['rewards'][step]) < 1e-10
                        and done == row['samples']['dones'][step], keys[i], 'native reward or termination differs')
                observations[offset+i].append(observation[0])
        for key, env, row in zip(keys, engines, wave):
            require(trace(env) == row['trace'] and layout(env, 'best') == row['layout'], key,
                    'native command/candidate replay or incumbent differs')
            replay(catalogs[key], config, row['prefix'], row['trace'], row['layout'])
    require(max(max_log, max_value) <= tolerance, 'collection', f'forward reconstruction differs: {max_log}, {max_value}')
    require(policy_hash(actor.state_dict()) == before, 'collection', 'incoming actor changed')
    rollout = {key: [v for row in records for v in row['samples'][key]]
               for key in ('actions', 'log_probs', 'values', 'rewards', 'dones')}
    rollout['observations'] = [o for episode in observations for o in episode]
    return rollout, {'episodes': 64, 'native_replay_commands': 1024, 'reference_replay_commands': 1024,
                     'setup_commands_per_engine': setup, 'max_log_probability_error': max_log,
                     'max_value_error': max_value}


def analyze(actor, outgoing, rollout, records, config, cases):
    incoming = policy_hash(actor.state_dict())
    next_actor = deepcopy(actor)
    next_actor.load_state_dict(outgoing['policy'])
    weights_only = deepcopy(actor)
    hybrid = weights_only.state_dict()
    for name, _ in actor.named_parameters():
        hybrid[name] = outgoing['policy'][name]
    weights_only.load_state_dict(hybrid)
    delta = torch.cat([(outgoing['policy'][name].double()-value.detach().cpu().double()).flatten()
                       for name, value in actor.named_parameters()])
    advantage, returns = targets(rollout, config)
    records_out, case_gradients, strata = [], [], []
    for case in cases:
        group_gradients, group_rows = [], []
        for reason, expected in (('current', 48), ('rehearsal', 16)):
            indices = np.asarray([16*i+j for i, row in enumerate(records)
                                  if row['summary']['case'] == case and row['summary']['start_reason'] == reason
                                  for j in range(16)])
            require(len(indices) == expected, case, f'{reason}: observation count differs')
            initial, gradients = components(actor, rollout, indices, advantage, returns, config, True)
            moved, _ = components(weights_only, rollout, indices, advantage, returns, config, False)
            final, _ = components(next_actor, rollout, indices, advantage, returns, config, False)
            group_gradients.append(gradients)
            group_rows.append({'start_reason': reason, 'observations': len(indices),
                'incoming_losses': dict(zip(COMPONENTS, initial.tolist())),
                'weight_only_loss_changes': dict(zip(COMPONENTS, (moved-initial).tolist())),
                'complete_actor_loss_changes': dict(zip(COMPONENTS, (final-initial).tolist())),
                'component_norms': dict(zip(COMPONENTS, gradients.norm(dim=1).tolist())),
                'component_dot_displacement': dict(zip(COMPONENTS, (gradients@delta).tolist()))})
        combined = 0.75*group_gradients[0]+0.25*group_gradients[1]
        case_gradients.append(combined)
        strata.append(torch.stack(group_gradients))
        records_out.append({'case': case, 'starts': group_rows,
                            'current_rehearsal_policy_cosine': cosine(group_gradients[0][0], group_gradients[1][0])})
    metrics = summarize(torch.stack(case_gradients), delta)
    for row, gradient in zip(records_out, metrics.pop('cases')):
        row['gradient'] = gradient
    pooled_starts = torch.stack(strata).mean(0)
    metrics['pooled_current_rehearsal_policy_cosine'] = cosine(pooled_starts[0, 0], pooled_starts[1, 0])
    require(policy_hash(actor.state_dict()) == incoming, 'analysis', 'incoming actor changed')
    require(policy_hash(next_actor.state_dict()) == policy_hash(outgoing['policy']), 'analysis', 'outgoing actor changed')
    return {'cases': records_out, **metrics}


def run(root, source, protocol, library, output):
    spec = read(protocol)
    require(spec == {'schema_version': 1, 'purpose': 'retained_gradient_contributions',
                    'comparison_sha256': COMPARISON, 'continuation_seed': 1030000, 'multipliers': [1, 0.1],
                    'updates': list(range(9, 17)), 'study_protocol': 'tools/factory-edit-pilot/entity-learning-width1-study.json',
                    'device': 'cuda', 'reconstruction_tolerance': 0.00002, 'maximum_seconds': 900},
            protocol, 'unsupported diagnostic protocol')
    require(not output.exists(), output, 'choose a fresh directory')
    require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip(), root, 'clean source required')
    require(torch.cuda.is_available(), root, 'original CUDA runtime required')
    comparison_path = source/'comparison.json'
    require(digest(comparison_path) == COMPARISON, comparison_path, 'comparison hash differs')
    comparison = read(comparison_path)
    require(comparison['status'] == 'complete' and comparison['decision']['candidate_for_larger_test'] is False,
            comparison_path, 'expected original failed criterion')
    cases = read(root/spec['study_protocol'])['cases']
    require(list(cases) == [row['case'] for row in comparison['anchor_roots']], protocol, 'case order differs')
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol),
              'comparison_sha256': COMPARISON, 'original_criterion_passed': False,
              'new_sampled_commands': 0, 'optimizer_updates': 0, 'cells': [], 'provenance': {}}
    output.mkdir(parents=True)
    started, active = time.perf_counter(), 'source'
    checked_paths = {comparison_path: COMPARISON, protocol: digest(protocol), library: digest(library)}
    try:
        for multiplier in spec['multipliers']:
            branch = next(b for b in comparison['branches'] if b['seed'] == spec['continuation_seed'] and b['multiplier'] == multiplier)
            directory = source/f"seed{spec['continuation_seed']}-lr{multiplier:g}"
            collection = directory/'collected.jsonl'
            require(digest(collection) == branch['collected_sha256'], collection, 'collection hash differs')
            checked_paths[collection] = branch['collected_sha256']
            collected = rows(collection)
            require(len(collected) == 2048, collection, 'incomplete collection')
            for index in spec['updates']:
                active = f'lr{multiplier:g}/update{index}'
                require(time.perf_counter()-started < spec['maximum_seconds'], output, 'elapsed diagnostic budget exhausted')
                incoming, input_path, input_hash = recorded_checkpoint(directory, branch, index-1)
                outgoing, next_path, next_hash = recorded_checkpoint(directory, branch, index)
                checked_paths.update({input_path: input_hash, next_path: next_hash})
                config = incoming['config']
                require(config == outgoing['config'] and config['horizon'] == 16, input_path, 'configuration differs')
                torch.set_num_threads(config['threads'])
                torch.backends.cuda.matmul.allow_tf32 = False
                torch.backends.cudnn.allow_tf32 = False
                torch.backends.cudnn.benchmark = False
                runtimes = {key: Runtime(root, root/path, config, library) for key, path in cases.items()}
                for key, runtime in runtimes.items():
                    previous = comparison['provenance'][key]['native']
                    require(runtime.provenance == {**previous, 'library': str(library)}, key, 'native input identity changed')
                if not result['provenance']:
                    for key, runtime in runtimes.items():
                        current, previous = provenance(runtime, protocol), comparison['provenance'][key]
                        require(current['native'] == {**previous['native'], 'library': current['native']['library']}
                                and all(current[k] == previous[k] for k in ('python', 'torch', 'numpy', 'platform', 'machine'))
                                and all(current['files_sha256'][f'tools/factory-edit-pilot/{name}'] ==
                                        previous['files_sha256'][f'tools/factory-edit-pilot/{name}']
                                        for name in ('policy.py', 'curriculum.py')), key, 'runtime/source/native/input differs')
                        result['provenance'][key] = current
                actor = Policy(next(iter(runtimes.values())).new_engine().observe()[0],
                               normalization=config['normalization'], width_multiplier=1).to('cuda').eval()
                actor.load_state_dict(incoming['policy'])
                records = collected[(index-1)*64:index*64]
                catalogs = {key: Catalog(root, root/path) for key, path in cases.items()}
                rollout, validation = reconstruct(actor, runtimes, catalogs, records, config, spec['reconstruction_tolerance'])
                cell = {'multiplier': multiplier, 'update': index, 'incoming_sha256': input_hash, 'outgoing_sha256': next_hash,
                        'validation': validation, 'analysis': analyze(actor, outgoing, rollout, records, config, list(cases))}
                result['cells'].append(cell)
                write(output/'gradients.json', result)
                print(json.dumps({'cell': active, 'reconstruction': validation,
                                  'pooled_current_rehearsal_cosine': cell['analysis']['pooled_current_rehearsal_policy_cosine']}), flush=True)
        require(all(digest(path) == expected for path, expected in checked_paths.items()), source, 'input changed during analysis')
        result.update(status='complete', seconds=time.perf_counter()-started, verified_input_files=len(checked_paths))
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'retained_gradients.interrupted', 'severity': 'error',
            'stage': 'gradient_contributions', 'entity': active, 'message': str(error)})
        write(output/'gradients.json', result)
        raise
    write(output/'gradients.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    run(args.root.resolve(), args.source.resolve(), args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
