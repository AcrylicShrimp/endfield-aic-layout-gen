"""Compare fixed-command restart schedules on already opened development geometry."""

import argparse
from collections import Counter
import json
from pathlib import Path
import time

import torch

from evidence import append, provenance, write
from mastery_policy_selection import digest, read, verify_case
from native import Runtime, check, layout, trace
from native_training_comparison import policy_hash
from policy import Policy
from reference.acceptance import Catalog, check as reference_check


def validate_protocol(spec):
    total, episodes = spec['total_commands'], spec['episodes']
    horizons, banks = spec['attempt_horizons'], spec['seed_banks']
    if (spec.get('schema_version') != 1 or type(total) is not int or total <= 0
            or type(episodes) is not int or episodes < 2 or not horizons
            or any(type(h) is not int or h <= 0 or total % h for h in horizons)
            or len(set(horizons)) != len(horizons) or len(banks) != 2
            or any(type(s) is not int or s < 0 for s in banks)
            or abs(banks[0]-banks[1]) < episodes
            or type(spec['required_successes']) is not int
            or not 1 <= spec['required_successes'] <= episodes or not spec['cases']):
        raise ValueError('restart.protocol: invalid equal-budget schedules, cases or sampling banks')


def evaluate_cell(root, runtime, actor, config, spec, horizon, bank, directory):
    directory.mkdir(parents=True)
    catalog = Catalog(root, runtime.path)
    engine = runtime.new_engine()
    records, layouts, traces = [], [], []
    started = time.perf_counter()
    for episode in range(spec['episodes']):
        generator = torch.Generator(device='cpu').manual_seed(bank+episode)
        best, best_trace, best_score, attempts = None, None, None, []
        total_counts = Counter()
        for attempt in range(spec['total_commands']//horizon):
            engine.reset()
            first_gain = None
            for step in range(horizon):
                observation, handle = engine.observe()
                action, _, _ = actor.act(observation, generator)
                reward, _, _ = engine.step(handle, action)
                if reward > 0 and first_gain is None:
                    first_gain = step+1
            candidate = layout(engine, 'best')
            accepted = check(engine, candidate)
            counts = json.loads(engine.episode_json())['counts']
            if not accepted['valid'] or counts['commands'] != horizon:
                raise ValueError(f'restart.attempt: {directory}: invalid incumbent or command accounting')
            total_counts.update(counts)
            attempts.append({'attempt': attempt, 'score': accepted['score'],
                             'first_improvement_step': first_gain, 'counts': counts})
            if best_score is None or accepted['score'] < best_score:
                best, best_trace, best_score = candidate, trace(engine), accepted['score']
        accepted = reference_check(catalog, best)
        if not accepted['valid'] or accepted['score'] != best_score or total_counts['commands'] != spec['total_commands']:
            raise ValueError(f'restart.validation: {directory}: independent validity/score or total budget differs')
        translated = [f['id'] for f in runtime.case['facilities']
                      if best['placements'][f['id']][:2] != f['pose'][:2]]
        row = {'rollout_seed': bank+episode, 'score': best_score,
               'return': (engine.initial_score[0]-best_score[0])/engine.initial_score[0],
               'translated_facilities': translated, 'counts': dict(total_counts), 'attempts': attempts,
               'resets_after_initial': len(attempts)-1}
        append(directory/'evaluations.jsonl', row)
        write(directory/f'episode-{episode}-best.json', best)
        write(directory/f'episode-{episode}-best-trace.json', best_trace)
        records.append(row)
        layouts.append(best)
        traces.append(best_trace)
    ordered = sorted(range(len(records)), key=lambda i: records[i]['score'])
    median = ordered[len(ordered)//2]
    write(directory/'evaluation-best.json', layouts[ordered[0]])
    write(directory/'evaluation-median.json', layouts[median])
    write(directory/'evaluation-0-representative-trace.json', traces[median])
    validation = verify_case(root, runtime.path, {**config, 'horizon': horizon}, directory, 0)
    # Replay the best representative independently as well, without changing median files.
    best_dir = directory/'best-replay'
    best_dir.mkdir()
    append(best_dir/'evaluations.jsonl', records[ordered[0]])
    for label in ('best', 'median'):
        write(best_dir/f'evaluation-{label}.json', layouts[ordered[0]])
    write(best_dir/'evaluation-0-representative-trace.json', traces[ordered[0]])
    best_validation = verify_case(root, runtime.path, {**config, 'horizon': horizon}, best_dir, 0)
    successes = sum(r['return'] > 0 and bool(r['translated_facilities']) for r in records)
    return {'translated_successes': successes, 'passed': successes >= spec['required_successes'],
            'median_score': records[median]['score'], 'best_score': records[ordered[0]]['score'],
            'charged_commands': spec['episodes']*spec['total_commands'],
            'resets_after_initial': sum(r['resets_after_initial'] for r in records),
            'seconds': time.perf_counter()-started, 'validation': validation, 'best_validation': best_validation}


def run(root, protocol_path, library, output):
    spec = read(protocol_path)
    validate_protocol(spec)
    policy_path = root/spec['policy']
    if digest(policy_path) != spec['policy_sha256']:
        raise ValueError(f'restart.policy: {policy_path}: changed selected policy')
    for case in spec['cases'].values():
        if digest(root/case['path']) != case['sha256']:
            raise ValueError(f"restart.case: {case['path']}: changed case")
    saved = torch.load(policy_path, map_location='cpu', weights_only=False)
    config = {**saved['config'], 'device': 'cpu'}
    torch.set_num_threads(config['threads'])
    first = next(iter(spec['cases'].values()))
    runtime = Runtime(root, root/first['path'], config, library)
    current = provenance(runtime, protocol_path)
    original = saved['source']['provenance']
    if (not current['git_worktree_clean'] or current['native']['build']['files_sha256'] != original['native']['build']['files_sha256']
            or any(digest(p) != original['files_sha256'][str(p.relative_to(root))]
                   for p in [root/'tools/factory-edit-pilot/policy.py', *runtime.paths.values()])):
        raise ValueError('restart.source: dirty source or changed policy/catalog/native build')
    example, _ = runtime.new_engine().observe()
    actor = Policy(example, normalization=config.get('normalization', 'none'),
                   width_multiplier=config.get('width_multiplier', 1))
    actor.load_state_dict(saved['policy'])
    actor.eval()
    before = policy_hash(actor.state_dict())
    if before != saved['source']['policy_sha256']:
        raise ValueError('restart.policy: inference tensors differ from selected policy')
    if output.exists():
        raise ValueError(f'restart.output: {output}: choose a fresh directory')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path),
              'provenance': current, 'policy_sha256': before, 'methods': {}, 'purpose': spec['purpose']}
    started = time.perf_counter()
    try:
        for horizon in spec['attempt_horizons']:
            cells = []
            result['methods'][str(horizon)] = {'cells': cells}
            for bank in spec['seed_banks']:
                for key, case in spec['cases'].items():
                    cell_config = {**config, 'horizon': horizon}
                    runtime = Runtime(root, root/case['path'], cell_config, library)
                    cell = evaluate_cell(root, runtime, actor, cell_config, spec, horizon, bank,
                                         output/f'horizon{horizon}'/f'bank{bank}'/key)
                    cells.append({'case': key, 'seed_start': bank, **cell})
                    if policy_hash(actor.state_dict()) != before:
                        raise ValueError('restart.mutation: policy or BN buffers changed')
                    write(output/'diagnosis.json', result)
                    print(json.dumps({'horizon': horizon, 'case': key, 'bank': bank,
                                      'successes': cell['translated_successes']}), flush=True)
            result['methods'][str(horizon)].update(passed=all(c['passed'] for c in cells),
                median_score_sum=[sum(c['median_score'][i] for c in cells) for i in range(3)])
        eligible = [h for h in spec['attempt_horizons'] if result['methods'][str(h)]['passed']]
        selected = min(eligible, key=lambda h: (*result['methods'][str(h)]['median_score_sum'],
                       spec['total_commands']//h)) if eligible else None
        result.update(status='passed' if selected is not None else 'not_met', selected_attempt_horizon=selected,
                      diagnostic={'code': 'restart.development_only', 'severity': 'info', 'stage': 'diagnosis',
                                  'message': 'Opened geometry diagnosis does not establish held-out success'})
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'restart.interrupted', 'severity': 'error',
                      'stage': 'diagnosis', 'message': str(error)})
        write(output/'diagnosis.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'diagnosis.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    result = run(Path(__file__).resolve().parents[2], args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'selected_attempt_horizon': result.get('selected_attempt_horizon')}))


if __name__ == '__main__':
    main()
