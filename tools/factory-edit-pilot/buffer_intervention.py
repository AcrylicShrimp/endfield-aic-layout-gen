"""Separate checkpoint weight and normalization-state effects without training."""

import argparse
from copy import deepcopy
import json
from pathlib import Path
import statistics
import time

import torch

from clock_intervention import paired_counts
from evaluation_batch import episodes
from evidence import write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import digest, read
from native import Runtime, check, layout, trace
from native_training_comparison import policy_hash, rows
from prefix_recovery import replay
from reference.acceptance import Catalog, check as reference_check
from tree_search_pilot import freeze


CONDITIONS = {'old_old': (0, 0), 'new_new': (1, 1), 'new_old': (1, 0), 'old_new': (0, 1)}


def combine(actor, weights, buffers):
    parameter_names = set(dict(actor.named_parameters()))
    buffer_names = set(dict(actor.named_buffers()))
    expected = {f'normalizations.{layer}.{name}' for layer in range(3)
                for name in ('running_mean', 'running_var', 'num_batches_tracked')}
    if (buffer_names != expected or set(weights) != set(buffers)
            or set(weights) != parameter_names | buffer_names or parameter_names & buffer_names):
        raise ValueError('buffer_intervention.state: unexpected or unclassified state keys')
    return {key: (buffers if key in buffer_names else weights)[key].clone() for key in weights}


def run(root, source, protocol, library, output):
    spec = read(protocol)
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'checkpoint_buffer_intervention'
            or spec['width'] != 1 or spec['training_seed'] != 43
            or spec['checkpoint_pair'] != [1179648, 1212416]
            or spec['conditions'] != list(CONDITIONS) or spec['episodes'] != 32
            or spec['sampling_seed_start'] != 1025000 or spec['batch_size'] != 16
            or spec['horizon'] != 128 or spec['device'] != 'cpu' or spec['threads'] != 1):
        raise ValueError('buffer_intervention.protocol: unsupported diagnostic matrix')
    cases = admitted_cases(root, read(root/spec['geometry_protocol']))['train']
    if len(cases) != 16:
        raise ValueError('buffer_intervention.cases: expected sixteen admitted training cases')
    if output.exists():
        raise ValueError(f'buffer_intervention.output: {output}: choose a fresh directory')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec,
              'protocol_sha256': digest(protocol), 'actors': {}, 'cells': [], 'pairs': []}
    started, active_case = time.perf_counter(), None
    try:
        anchor_directory = output/'source-guard'
        anchor_directory.mkdir()
        template, config, guard = freeze(root, source, spec, cases, library, protocol, anchor_directory)
        result['source_guard'] = guard
        directory = source/'width1-seed43'
        audit = read(directory/'study-audit.json')
        states = []
        result['checkpoints'] = []
        for count in spec['checkpoint_pair']:
            path = directory/f'checkpoint-{count}.pt'
            actual = digest(path)
            if actual != audit['checkpoints_sha256'][str(count)]:
                raise ValueError(f'buffer_intervention.checkpoint: {path}: audit hash differs')
            saved = torch.load(path, map_location='cpu', weights_only=False)
            if (saved['metadata']['completed_commands'] != count
                    or saved['metadata']['seed'] != spec['training_seed']
                    or saved['metadata']['config']['width_multiplier'] != spec['width']):
                raise ValueError(f'buffer_intervention.checkpoint: {path}: checkpoint metadata differs')
            states.append(saved['evaluation_policy'])
            result['checkpoints'].append({'commands': count, 'sha256': actual,
                'policy_sha256': policy_hash(states[-1])})
        updates_path = directory/'updates.jsonl'
        updates = [r for r in rows(updates_path)
                   if spec['checkpoint_pair'][0] < r['commands'] <= spec['checkpoint_pair'][1]]
        if [r['commands'] for r in updates] != list(range(1180672, 1212417, 1024)):
            raise ValueError('buffer_intervention.updates: intervening update schedule differs')
        result['updates'] = {'sha256': digest(updates_path), 'count': len(updates),
            'optimizer_steps': sum(r['optimizer_steps'] for r in updates),
            'metrics': {key: {'minimum': min(r[key] for r in updates),
                             'median': statistics.median(r[key] for r in updates),
                             'maximum': max(r[key] for r in updates)}
                        for key in ('approx_kl', 'gradient_norm', 'value_loss', 'entropy')}}
        actors = {}
        for name, (weight_index, buffer_index) in CONDITIONS.items():
            actor = deepcopy(template)
            actor.load_state_dict(combine(actor, states[weight_index], states[buffer_index]))
            actor.eval()
            actors[name] = actor
            path = output/f'{name}.pt'
            torch.save({'purpose': 'diagnostic_only', 'state_dict': actor.state_dict()}, path)
            result['actors'][name] = {'weight_commands': spec['checkpoint_pair'][weight_index],
                'buffer_commands': spec['checkpoint_pair'][buffer_index], 'policy_sha256': policy_hash(actor.state_dict()),
                'file': path.name, 'file_sha256': digest(path)}
        write(output/'buffers.json', result)
        for active_case, path in cases.items():
            catalog = Catalog(root, path)
            runtime = Runtime(root, path, config, library)
            initial = list(runtime.new_engine().initial_score)
            paired = {}
            for name, actor in actors.items():
                cell_started = time.perf_counter()
                records, layouts, traces = [], [], []
                for seed, env, total in episodes(actor, runtime, spec['episodes'], spec['horizon'],
                        spec['sampling_seed_start'], spec['batch_size']):
                    best = layout(env, 'best')
                    native_result, independent = check(env, best), reference_check(catalog, best)
                    if (not native_result['valid'] or not independent['valid']
                            or native_result['score'] != independent['score']):
                        raise ValueError('buffer_intervention.acceptance: native/reference output mismatch')
                    counts, commands = json.loads(env.episode_json())['counts'], trace(env)
                    if (counts['commands'] != spec['horizon'] or len(commands) != spec['horizon']
                            or abs(total-(initial[0]-independent['score'][0])/initial[0]) > 1e-10):
                        raise ValueError('buffer_intervention.accounting: wrong command or reward total')
                    translated = [f['id'] for f in runtime.case['facilities']
                                  if best['placements'][f['id']][:2] != f['pose'][:2]]
                    records.append({'seed': seed, 'score': independent['score'], 'counts': counts,
                                    'translated_facilities': translated, 'return': total})
                    layouts.append(best)
                    traces.append(commands)
                evaluation_seconds = time.perf_counter()-cell_started
                order = sorted(range(len(records)), key=lambda i: (records[i]['score'], i))
                selected = {'best': order[0], 'median': order[len(order)//2]}
                replays = {}
                for role, index in selected.items():
                    replays[role] = replay(catalog, config, [], traces[index], layouts[index])
                    path_out = output/f'{active_case}-{name}-{role}.json'
                    write(path_out, {'seed': records[index]['seed'], 'trace': traces[index], 'layout': layouts[index]})
                    replays[role].update(path=path_out.name, sha256=digest(path_out))
                row = {'case': active_case, 'condition': name, 'initial_score': initial, 'episodes': records,
                    'translated_successes': sum(r['score'][0] < initial[0] and bool(r['translated_facilities']) for r in records),
                    'changed_completion_episodes': sum(r['counts'].get('new_completion', 0) > 0 for r in records),
                    'best_score': records[selected['best']]['score'], 'median_score': records[selected['median']]['score'],
                    'evaluation_seconds': evaluation_seconds,
                    'validation_seconds': time.perf_counter()-cell_started-evaluation_seconds, 'replays': replays}
                result['cells'].append(row)
                paired[name] = records
                write(output/'buffers.json', result)
            comparison = {'case': active_case, **{f'{a}_vs_{b}': paired_counts(paired[a], paired[b])
                for a, b in [('new_new', 'old_old'), ('new_old', 'new_new'), ('old_new', 'old_old')]}}
            result['pairs'].append(comparison)
            print(json.dumps(comparison), flush=True)
        for name, actor in actors.items():
            if policy_hash(actor.state_dict()) != result['actors'][name]['policy_sha256']:
                raise ValueError('buffer_intervention.policy: diagnostic actor changed')
        if len(result['cells']) != 64 or len(result['pairs']) != 16:
            raise ValueError('buffer_intervention.matrix: incomplete diagnostic cells')
        result['status'] = 'complete'
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'buffer_intervention.interrupted',
            'severity': 'error', 'stage': 'checkpoint_buffer_intervention', 'message': str(error), 'entity': active_case})
        write(output/'buffers.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'buffers.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.root.resolve(), args.source.resolve(), args.protocol.resolve(),
        args.native_library.resolve(), args.output.resolve())
