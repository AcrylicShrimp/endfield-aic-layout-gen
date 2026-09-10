"""Attribute recorded best-area progress to actions; measure noncausal follow-ups."""
import argparse
from collections import Counter
import gzip
import hashlib
import json
import math
from pathlib import Path


FAILURES = {'executor_rejected', 'rejected', 'late'}


def check(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path, manifest):
    data = path.read_bytes()
    manifest.append({'path': str(path), 'sha256': sha(data)})
    return json.loads(gzip.decompress(data) if path.suffix == '.gz' else data)


def bucket():
    return {'episodes': 0, 'actions': 0, 'area_pp': 0.0, 'kinds': {}, 'positions': {}}


def add_episode(target, row, trace):
    initial = row['initial_metrics']['score'][0]
    previous = initial
    gains = []
    costs = []
    for step in trace:
        check(step['physical_unchanged'] == (step['physical_before'] == step['physical_after']),
              'physical unchanged flag differs from state identities')
        best = step['best_score'][0]
        gain = (previous - best) / initial
        check(gain >= 0, 'best area increased')
        check(math.isclose(gain, step.get('area_reward', step['reward']), abs_tol=1e-12),
              'recorded area reward differs from best-area change')
        cost = row.get('reward_config', {}).get('failure_penalty', 0.0) if step['status'] in FAILURES else 0.0
        check(step.get('failure_cost', 0.0) == cost and
              math.isclose(step['reward'], gain - cost, abs_tol=1e-12),
              'recorded failure cost differs from shaped reward')
        check(step['best_area_before'] == previous, 'best-area sequence is discontinuous')
        gains.append(100 * gain)
        costs.append(100 * cost)
        previous = best
    check(len(trace) == row['selected'], 'trace action coverage differs')
    check(Counter(s['action']['kind'] for s in trace) == Counter(row['kinds']), 'kind counts differ')
    check(Counter(s['status'] for s in trace) == Counter(row['statuses']), 'status counts differ')
    expected = 100 * (initial - row['best_metrics']['score'][0]) / initial
    check(previous == row['best_metrics']['score'][0] and
          math.isclose(sum(gains), expected, abs_tol=1e-10), 'area attribution does not telescope')
    target['episodes'] += 1
    target['actions'] += len(trace)
    target['area_pp'] += expected
    for i, (step, gain, cost) in enumerate(zip(trace, gains, costs)):
        kind = step['action']['kind']
        c = target['kinds'].setdefault(kind, Counter())
        c['selected'] += 1
        c['failed'] += step['status'] in FAILURES
        c['score_improved'] += step['status'] == 'improved'
        c['area_improved'] += gain > 0
        c['area_pp'] += gain
        c['failure_cost_pp'] += cost
        c['physical_unchanged'] += step['physical_unchanged']
        c['accepted_nonimproving'] += gain == 0 and step['status'] not in FAILURES
        c['accepted_nonimproving_changed'] += (
            gain == 0 and step['status'] not in FAILURES and not step['physical_unchanged'])
        # Complete four-step observation windows only. Overlapping follow-ups are
        # descriptive associations, never additive or causal area attribution.
        if gain == 0 and step['status'] not in FAILURES and i + 4 < len(trace):
            c['follow4_eligible'] += 1
            c['follow4_any_gain'] += any(x > 0 for x in gains[i + 1:i + 5])
            before_restore = []
            for j in range(i + 1, i + 5):
                if trace[j]['action']['kind'] == 'restore_best':
                    break
                before_restore.append(gains[j])
            c['follow4_gain_before_restore'] += any(x > 0 for x in before_restore)
            state = 'unchanged' if step['physical_unchanged'] else 'changed'
            c[f'follow4_{state}_eligible'] += 1
            c[f'follow4_{state}_gain_before_restore'] += any(x > 0 for x in before_restore)
        position = f'{i // 8 * 8 + 1:02d}-{i // 8 * 8 + 8:02d}'
        p = target['positions'].setdefault(position, Counter())
        p['selected'] += 1
        p['area_pp'] += gain
        p['failure_cost_pp'] += cost
        p['failed'] += step['status'] in FAILURES


def finish(target):
    episodes, actions, area = target['episodes'], target['actions'], target['area_pp']
    target['mean_area_reduction_percent'] = area / episodes
    check(math.isclose(sum(c['area_pp'] for c in target['kinds'].values()), area, abs_tol=1e-8),
          'kind contributions do not sum to total area')
    for c in target['kinds'].values():
        c['selection_percent'] = 100 * c['selected'] / actions
        c['failure_percent'] = 100 * c['failed'] / c['selected']
        c['area_improvement_percent'] = 100 * c['area_improved'] / c['selected']
        c['mean_contribution_pp'] = c['area_pp'] / episodes
        c['mean_failure_cost_pp'] = c['failure_cost_pp'] / episodes
        c['area_contribution_percent'] = 100 * c['area_pp'] / area if area else 0
        c['area_pp_per_selection'] = c['area_pp'] / c['selected']
    for c in target['positions'].values():
        c['mean_contribution_pp'] = c['area_pp'] / episodes
        c['mean_failure_cost_pp'] = c['failure_cost_pp'] / episodes
    return target


def run(args):
    check(not args.output.exists(), f'output already exists: {args.output}')
    manifest, result, cohort, seen = [], {}, None, set()
    for spec in args.run:
        label, offset, directory = spec.split('=', 2)
        offset = int(offset)
        root = Path(directory).resolve()
        groups = result.setdefault(label, {})
        files = sorted(root.glob('evaluation-*/record-*.json'))
        check(files, f'no evaluation records in {root}')
        for path in files:
            batch = int(path.parent.name.split('-')[1]) + offset
            if batch > args.through or (offset and batch == offset):
                continue
            check((label, batch) not in seen, f'duplicate evaluation: {label} batch {batch}')
            seen.add((label, batch))
            record = read(path, manifest)
            check(record['phase'] == {'kind': 'evaluation', 'batch': batch - offset},
                  f'phase mismatch: {path}')
            signature = sorted((r['identity']['generation_seed'], r['generation']['initial_sha256'])
                               for r in record['results'])
            if cohort is None:
                cohort = signature
            check(signature == cohort, f'evaluation initial-input cohort differs: {path}')
            interval = 'initial' if batch == 0 else f'{(batch - 1) // 64 * 64 + 8:04d}-{(batch - 1) // 64 * 64 + 64:04d}'
            names = [interval, f'batch-{batch:04d}']
            for entry in record['results']:
                check(entry['status'] == 'complete', f'incomplete evaluation: {path}')
                row = entry['row']
                episode_path = Path(row['directory']) / 'episode.json.gz'
                check(episode_path.exists(), f'missing trace; refuse partial attribution: {episode_path}')
                episode = read(episode_path, manifest)
                for name in names:
                    add_episode(groups.setdefault(name, bucket()), row, episode['trace'])
            measured = groups[f'batch-{batch:04d}']
            check(math.isclose(measured['area_pp'] / measured['episodes'],
                               record['quality']['mean_area_reduction_percent'], abs_tol=1e-10),
                  f'phase quality differs from attributed area: {path}')
    for groups in result.values():
        for group in groups.values():
            finish(group)
    args.output.mkdir(parents=True)
    manifest_bytes = (json.dumps(manifest, indent=2) + '\n').encode()
    (args.output / 'inputs.json').write_bytes(manifest_bytes)
    report = {'schema': 'ppo-action-contribution-audit-v1', 'through_batch': args.through,
              'source_sha256': sha(Path(__file__).read_bytes()), 'run_arguments': args.run,
              'input_manifest_sha256': sha(manifest_bytes), 'input_files': len(manifest),
              'matched_evaluation_inputs': len(cohort),
              'method': 'Per-action decrease in episode-best area / episode-initial area; pure area before failure costs.',
              'follow_up_limit': 'Observed next-four-action gains are overlapping associations, not causal contributions.',
              'runs': result}
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'output': str(args.output), 'input_files': len(manifest),
                      'report_sha256': sha((args.output / 'report.json').read_bytes())}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='append', required=True, metavar='LABEL=BATCH_OFFSET=JOB_DIRECTORY')
    parser.add_argument('--through', type=int, required=True)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
