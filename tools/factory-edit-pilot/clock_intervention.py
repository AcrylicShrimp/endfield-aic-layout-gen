"""Isolate remaining-time conditioning in frozen training-case policy rollouts."""

import argparse
import json
from pathlib import Path
import time

import torch

from evaluation_batch import episodes
from evidence import write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import digest, read
from native import Runtime, check, layout, trace
from native_training_comparison import policy_hash
from prefix_recovery import replay
from reference.acceptance import Catalog, check as reference_check
from tree_search_pilot import freeze


CONDITIONS = [('native16', 16), ('native128', 128), ('clamped16_in128', 128)]


class ClampedClock(torch.nn.Module):
    """Change the inference tensor only; each batched wave has 128 forward calls."""

    def __init__(self, actor):
        super().__init__()
        self.actor = actor
        self.calls = 0

    def forward(self, data):
        t = self.calls % 128
        stats = data['stats'].clone()
        stats[:, 0] = max(0, (16-t)/16)
        self.calls += 1
        return self.actor({**data, 'stats': stats})


def paired_counts(left, right):
    if [r['seed'] for r in left] != [r['seed'] for r in right]:
        raise ValueError('clock_intervention.pairs: sampling seeds differ')
    return {'wins': sum(a['score'] < b['score'] for a, b in zip(left, right)),
            'ties': sum(a['score'] == b['score'] for a, b in zip(left, right)),
            'losses': sum(a['score'] > b['score'] for a, b in zip(left, right))}


def run(root, source, protocol, library, output):
    spec = read(protocol)
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'training_only_clock_intervention'
            or spec['widths'] != [1, 2] or spec['training_seed'] != 17
            or spec['conditions'] != [name for name, _ in CONDITIONS]
            or spec['episodes'] != 32 or spec['batch_size'] != 16
            or spec['sampling_seed_start'] != 1024000 or spec['horizon'] != 128
            or spec['device'] != 'cpu' or spec['threads'] != 1):
        raise ValueError('clock_intervention.protocol: unsupported diagnostic matrix')
    cases = admitted_cases(root, read(root/spec['geometry_protocol']))['train']
    if len(cases) != 16:
        raise ValueError('clock_intervention.cases: expected sixteen admitted training cases')
    if output.exists():
        raise ValueError(f'clock_intervention.output: {output}: choose a fresh directory')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec,
              'protocol_sha256': digest(protocol), 'selections': {}, 'cells': [], 'pairs': []}
    started = time.perf_counter()
    active_case = None
    try:
        actors = {}
        for width in spec['widths']:
            destination = output/f'width{width}'
            destination.mkdir()
            actor, config, selection = freeze(root, source, {**spec, 'width': width},
                                               cases, library, protocol, destination)
            actors[width] = actor, config
            result['selections'][str(width)] = selection
        write(output/'clock.json', result)
        for width, (actor, base_config) in actors.items():
            for active_case, path in cases.items():
                catalog = Catalog(root, path)
                paired = {}
                control_traces = None
                for condition, horizon in CONDITIONS:
                    cell_started = time.perf_counter()
                    config = {**base_config, 'horizon': horizon}
                    runtime = Runtime(root, path, config, library)
                    initial = list(runtime.new_engine().initial_score)
                    observed_actor = ClampedClock(actor) if condition == 'clamped16_in128' else actor
                    records, layouts, traces = [], [], []
                    for seed, env, total in episodes(observed_actor, runtime, spec['episodes'],
                            horizon, spec['sampling_seed_start'], spec['batch_size']):
                        best = layout(env, 'best')
                        native_result = check(env, best)
                        independent = reference_check(catalog, best)
                        if (not native_result['valid'] or not independent['valid']
                                or native_result['score'] != independent['score']):
                            raise ValueError('clock_intervention.acceptance: native/reference output mismatch')
                        counts = json.loads(env.episode_json())['counts']
                        rows = trace(env)
                        if (counts['commands'] != horizon or len(rows) != horizon
                                or abs(total-(initial[0]-independent['score'][0])/initial[0]) > 1e-10):
                            raise ValueError('clock_intervention.accounting: wrong command or reward total')
                        translated = [f['id'] for f in runtime.case['facilities']
                                      if best['placements'][f['id']][:2] != f['pose'][:2]]
                        records.append({'seed': seed, 'score': independent['score'], 'counts': counts,
                                        'translated_facilities': translated, 'return': total})
                        layouts.append(best)
                        traces.append(rows)
                    if condition == 'native16':
                        control_traces = traces
                    if condition == 'clamped16_in128':
                        if (any(short != long[:16] for short, long in zip(control_traces, traces))
                                or any(long['score'] > short['score'] for short, long
                                       in zip(paired['native16'], records))):
                            raise ValueError('clock_intervention.control: prefix or incumbent retention differs')
                        if observed_actor.calls != 256:
                            raise ValueError('clock_intervention.control: batched clock reset schedule differs')
                    evaluation_seconds = time.perf_counter()-cell_started
                    ordered = sorted(range(len(records)), key=lambda i: (records[i]['score'], i))
                    selected = {'best': ordered[0], 'median': ordered[len(ordered)//2]}
                    replays = {}
                    for role, index in selected.items():
                        replays[role] = replay(catalog, config, [], traces[index], layouts[index])
                        destination = output/f'width{width}-{active_case}-{condition}-{role}.json'
                        write(destination, {'seed': records[index]['seed'], 'trace': traces[index],
                                            'layout': layouts[index]})
                        replays[role].update(path=destination.name, sha256=digest(destination))
                    row = {'width': width, 'case': active_case, 'condition': condition,
                           'horizon': horizon, 'initial_score': initial, 'episodes': records,
                           'translated_successes': sum(r['score'][0] < initial[0]
                               and bool(r['translated_facilities']) for r in records),
                           'changed_completion_episodes': sum(r['counts'].get('new_completion', 0) > 0 for r in records),
                           'median_score': records[selected['median']]['score'],
                           'best_score': records[selected['best']]['score'],
                           'evaluation_seconds': evaluation_seconds,
                           'validation_seconds': time.perf_counter()-cell_started-evaluation_seconds,
                           'replays': replays}
                    result['cells'].append(row)
                    paired[condition] = records
                    write(output/'clock.json', result)
                comparison = {'width': width, 'case': active_case, 'prefix_control': 'passed',
                              'clamped_vs_native128': paired_counts(paired['clamped16_in128'], paired['native128']),
                              'clamped_vs_native16': paired_counts(paired['clamped16_in128'], paired['native16'])}
                result['pairs'].append(comparison)
                print(json.dumps(comparison), flush=True)
        for width, (actor, _) in actors.items():
            if policy_hash(actor.state_dict()) != result['selections'][str(width)]['policy_sha256']:
                raise ValueError('clock_intervention.policy: frozen actor changed')
        if len(result['cells']) != 96 or len(result['pairs']) != 32:
            raise ValueError('clock_intervention.matrix: incomplete cells')
        result['status'] = 'complete'
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'clock_intervention.interrupted',
            'severity': 'error', 'stage': 'policy_clock_intervention', 'message': str(error), 'entity': active_case})
        write(output/'clock.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'clock.json', result)
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
