"""Compare complete bounded native runs across the data-supply schema cutover."""
import argparse
import hashlib
import json
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_run(root):
    state = json.loads((root / 'state.json').read_text())
    if state['status'] != 'complete' or len(state['jobs']) != 1:
        raise ValueError(f'one complete job required: {root}')
    records = []
    for ref in state['jobs'][0]['records']:
        path = root / ref['path']
        if digest(path) != ref['sha256']:
            raise ValueError(f'record hash mismatch: {path}')
        records.append(json.loads(path.read_text()))
    samples = [json.loads(line) for path in root.glob('sessions/*/resources*.jsonl')
               for line in path.read_text().splitlines()]
    return dict(root=str(root), protocol_sha256=digest(root / 'protocol.json'),
                elapsed_seconds=state['elapsed_seconds'],
                peak_rss_bytes=max((s['rss_bytes'] for s in samples), default=None),
                records=records)


def main(args):
    if args.output.exists():
        raise ValueError(f'fresh output required: {args.output}')
    before, after = read_run(args.before), read_run(args.after)
    if len(before['records']) != len(after['records']):
        raise AssertionError('phase counts differ')
    rows = []
    keys = ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
            'critic_optimizer_sha256', 'shuffle_rng_sha256', 'epochs', 'selected',
            'optimizer_steps', 'kl_stopped')
    for a, b in zip(before['records'], after['records']):
        if a['phase'] != b['phase'] or len(a['results']) != len(b['results']):
            raise AssertionError('phase identities or attempt counts differ')
        for left, right in zip(a['results'], b['results']):
            if left['identity'] != right['identity'] or left['status'] != right['status']:
                raise AssertionError('episode identity/outcome differs')
            if left['status'] == 'complete':
                for key in ('best_sha256', 'initial_metrics', 'best_metrics', 'selected',
                            'kinds', 'statuses', 'improvements'):
                    if left['row'][key] != right['row'][key]:
                        raise AssertionError(f'episode changed: {a["phase"]}, {key}')
        for key in keys if 'ppo' in a else ():
            if a['ppo'][key] != b['ppo'][key]:
                raise AssertionError(f'PPO changed: {a["phase"]}, {key}')
        rows.append(dict(phase=a['phase'], all_outcomes_exact=True, all_ppo_states_and_epochs_exact='ppo' in a,
                         before={k: a[k] for k in ('phase_seconds', 'collection', 'update_seconds') if k in a},
                         after={k: b[k] for k in ('phase_seconds', 'collection', 'update_seconds') if k in b}))
    for value in (before, after):
        value.pop('records')
    report = dict(schema='ppo-supply-run-comparison-v1', status='complete',
                  source_sha256=digest(Path(__file__)), before=before, after=after, phases=rows)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(dict(status='complete', phases=len(rows), all_exact=True, before=before, after=after)))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    main(parser.parse_args())
