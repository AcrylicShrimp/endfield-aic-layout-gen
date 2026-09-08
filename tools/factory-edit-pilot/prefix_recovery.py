"""Conditional recovery on declared training prefixes with frozen audited actors."""

import argparse
import json
from pathlib import Path
import time

from evaluation_batch import episodes
from evidence import write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import digest, read
from native import Runtime, check, layout, trace
from native_training_comparison import policy_hash
from reference.acceptance import Catalog, check as reference_check
from reference.environment import Environment, serialize
from tree_search_pilot import freeze


def prefixes(source, spec):
    result = {}
    for start in spec['starts']:
        if start['id'] in result or type(start['commands']) is not int or start['commands'] < 0:
            raise ValueError('prefix_recovery.start: duplicate identifier or invalid prefix length')
        if start['file'] is None:
            if start['commands'] != 0:
                raise ValueError('prefix_recovery.start: root must have zero setup commands')
            result[start['id']] = []
            continue
        path = source/spec['evidence_directory']/start['file']
        if digest(path) != start['sha256']:
            raise ValueError(f'prefix_recovery.evidence: {path}: prefix source changed')
        rows = read(path)
        if not 0 < start['commands'] <= len(rows):
            raise ValueError(f'prefix_recovery.start: {path}: invalid prefix length')
        selected = rows[:start['commands']]
        if any(not row['applied'] or row['reward'] != 0 for row in selected):
            raise ValueError(f'prefix_recovery.start: {path}: prefix is not zero-reward applied work')
        result[start['id']] = [row['command'] for row in selected]
    if not result:
        raise ValueError('prefix_recovery.start: no starts')
    return result


def replay(catalog, config, prefix, rows, expected):
    # Setup is separately counted; replay still checks every setup command.
    env = Environment(catalog, {**config, 'horizon': len(prefix)+len(rows)})
    for command in prefix:
        if command not in env.candidates().to_dicts():
            raise ValueError('prefix_recovery.replay: setup command outside reference domain')
        reward, _, outcome = env.step(command)
        if reward != 0 or not outcome['applied'] or serialize(env.best) != serialize(env.root):
            raise ValueError('prefix_recovery.replay: setup changed incumbent or failed')
    for row in rows:
        if row['command'] not in env.candidates().to_dicts():
            raise ValueError('prefix_recovery.replay: policy command outside reference domain')
        reward, _, outcome = env.step(row['command'])
        if abs(reward-row['reward']) > 1e-10 or any(outcome[key] != row[key]
                for key in ('applied', 'complete', 'new_completion', 'improved', 'diagnostics')):
            raise ValueError('prefix_recovery.replay: reference transition differs')
    if serialize(env.best) != expected:
        raise ValueError('prefix_recovery.replay: selected incumbent differs')
    return {'status': 'passed', 'setup_commands': len(prefix), 'policy_commands': len(rows)}


def run(root, source, protocol, library, output):
    spec = read(protocol)
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'selected_training_prefix_recovery'
            or spec['device'] != 'cpu' or spec['widths'] != [1, 2]
            or spec['horizons'] != [16, 128] or spec['episodes'] != 32
            or spec['sampling_seed_start'] < 0 or spec['batch_size'] < 1):
        raise ValueError('prefix_recovery.protocol: unsupported matrix or budget')
    starts = prefixes(source, spec)
    cases = admitted_cases(root, read(root/spec['geometry_protocol']))['train']
    if spec['case'] not in cases:
        raise ValueError(f"prefix_recovery.case: {spec['case']}: case is outside training split")
    if output.exists():
        raise ValueError(f'prefix_recovery.output: {output}: choose a fresh directory')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec,
              'protocol_sha256': digest(protocol), 'selections': {}, 'cells': []}
    started = time.perf_counter()
    try:
        frozen = {}
        # Freeze every actor before any diagnostic outcomes are observed.
        for width in spec['widths']:
            directory = output/f'width{width}'
            directory.mkdir()
            actor, config, selection = freeze(root, source, {**spec, 'width': width}, cases,
                                               library, protocol, directory)
            frozen[width] = actor, config
            result['selections'][str(width)] = selection
        write(output/'recovery.json', result)
        path = cases[spec['case']]
        catalog = Catalog(root, path)
        for width, (actor, base_config) in frozen.items():
            for horizon in spec['horizons']:
                config = {**base_config, 'horizon': horizon}
                runtime = Runtime(root, path, config, library)
                initial = runtime.new_engine().initial_score
                for start, prefix in starts.items():
                    cell_started = time.perf_counter()
                    records, layouts, traces = [], [], []
                    for seed, env, total in episodes(actor, runtime, spec['episodes'], horizon,
                            spec['sampling_seed_start'], spec['batch_size'], prefix):
                        best = layout(env, 'best')
                        native_result = check(env, best)
                        independent = reference_check(catalog, best)
                        if (not native_result['valid'] or not independent['valid']
                                or native_result['score'] != independent['score']):
                            raise ValueError('prefix_recovery.acceptance: native/reference output mismatch')
                        counts = json.loads(env.episode_json())['counts']
                        if counts['commands'] != horizon or abs(total-(initial[0]-independent['score'][0])/initial[0]) > 1e-10:
                            raise ValueError('prefix_recovery.accounting: wrong action or reward total')
                        translated = [f['id'] for f in runtime.case['facilities']
                                      if best['placements'][f['id']][:2] != f['pose'][:2]]
                        records.append({'seed': seed, 'score': independent['score'], 'counts': counts,
                            'translated_facilities': translated, 'return': total})
                        layouts.append(best)
                        traces.append(trace(env))
                    evaluation_seconds = time.perf_counter()-cell_started
                    ordered = sorted(range(len(records)), key=lambda i: (records[i]['score'], i))
                    selected = {'best': ordered[0], 'median': ordered[len(ordered)//2]}
                    replays = {}
                    for role, index in selected.items():
                        replays[role] = replay(catalog, config, prefix, traces[index], layouts[index])
                        destination = output/f'width{width}-{start}-h{horizon}-{role}.json'
                        write(destination, {'seed': records[index]['seed'], 'prefix': prefix,
                                            'trace': traces[index], 'layout': layouts[index]})
                        replays[role].update(path=destination.name, sha256=digest(destination))
                    row = {'width': width, 'start': start, 'horizon': horizon, 'episodes': records,
                        'setup_commands_per_episode': len(prefix), 'initial_score': list(initial),
                        'translated_successes': sum(r['score'][0] < initial[0] and bool(r['translated_facilities']) for r in records),
                        'changed_completion_episodes': sum(r['counts'].get('new_completion', 0) > 0 for r in records),
                        'best_score': records[selected['best']]['score'], 'median_score': records[selected['median']]['score'],
                        'evaluation_seconds': evaluation_seconds,
                        'validation_seconds': time.perf_counter()-cell_started-evaluation_seconds,
                        'replays': replays}
                    result['cells'].append(row)
                    write(output/'recovery.json', result)
                    print(json.dumps({k: row[k] for k in ('width', 'start', 'horizon', 'translated_successes', 'median_score')}), flush=True)
        for width, (actor, _) in frozen.items():
            if policy_hash(actor.state_dict()) != result['selections'][str(width)]['policy_sha256']:
                raise ValueError('prefix_recovery.policy: frozen actor changed')
        if len(result['cells']) != len(spec['widths'])*len(spec['horizons'])*len(starts):
            raise ValueError('prefix_recovery.matrix: incomplete diagnostic cells')
        result['status'] = 'complete'
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'prefix_recovery.interrupted', 'severity': 'error',
            'stage': 'selected_prefix_recovery', 'message': str(error), 'entity': spec['case']})
        write(output/'recovery.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'recovery.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.root.resolve(), args.source.resolve(), args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
