"""Measure native outcomes across the numerically non-exact padded learner cutover."""
import argparse
import json
from pathlib import Path

from compare_ppo_supply_runs import digest, read_run


def compare(before_root, after_root, output):
    if output.exists():
        raise ValueError(f'fresh output required: {output}')
    before, after = read_run(before_root), read_run(after_root)
    protocols = [json.loads((root / 'protocol.json').read_text()) for root in (before_root, after_root)]
    if protocols[0]['config'] != protocols[1]['config']:
        raise AssertionError('readiness configurations differ')
    if len(before['records']) != len(after['records']):
        raise AssertionError('phase counts differ')
    rows = []
    fields = ('best_sha256', 'initial_metrics', 'best_metrics', 'selected', 'kinds', 'statuses', 'improvements')
    for left, right in zip(before['records'], after['records']):
        if left['phase'] != right['phase'] or len(left['results']) != len(right['results']):
            raise AssertionError('phase identity or attempt count differs')
        changed = dict.fromkeys(('status', *fields), 0)
        complete_pairs = 0
        area = dict(before=0, after=0)
        for a, b in zip(left['results'], right['results']):
            if a['identity'] != b['identity']:
                raise AssertionError('episode identity differs')
            changed['status'] += a['status'] != b['status']
            if a['status'] == b['status'] == 'complete':
                complete_pairs += 1
                for key in fields:
                    changed[key] += a['row'][key] != b['row'][key]
                for name, result in (('before', a), ('after', b)):
                    area[name] += result['row']['best_metrics']['score'][0]
        row = dict(phase=left['phase'], attempts=len(left['results']), complete_pairs=complete_pairs,
                   changed_results=changed, complete_pair_best_area_sum=area,
                   timing={name: {key: record[key] for key in ('phase_seconds', 'collection', 'update_seconds') if key in record}
                           for name, record in (('before', left), ('after', right))})
        if 'ppo' in left:
            row['ppo_exact'] = {key: left['ppo'][key] == right['ppo'][key] for key in
                ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
                 'critic_optimizer_sha256', 'shuffle_rng_sha256', 'epochs', 'selected', 'optimizer_steps', 'kl_stopped')}
        rows.append(row)
    for run in (before, after):
        run.pop('records')
    report = dict(schema='ppo-indexing-native-comparison-v1', status='complete',
                  source_sha256=digest(Path(__file__)), before=before, after=after, phases=rows,
                  interpretation='Measured paired readiness outcomes; no bit-exact or long-run learning equivalence claim')
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(dict(status='complete', before=before, after=after, changed_results=[r['changed_results'] for r in rows])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    compare(args.before.resolve(), args.after.resolve(), args.output.resolve())
