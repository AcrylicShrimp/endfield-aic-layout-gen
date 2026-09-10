"""Inspect recorded KL stopping and reconstruct retained GAE on CPU, without replaying PPO."""
import argparse
import json
import math
from pathlib import Path
import statistics

import numpy as np

from audit_ppo_action_contributions import check, read, sha


def advantages(trace, lam):
    values = []
    next_value = next_advantage = 0.0
    for step in reversed(trace):
        advantage = step['reward'] + next_value - step['old_value'] + lam * next_advantage
        values.append(advantage)
        next_value, next_advantage = step['old_value'], advantage
    return np.asarray(values[::-1], dtype=np.float32)


def window(records):
    def mean(get):
        return statistics.mean(get(r) for r in records)
    return dict(batches=[r['phase']['batch'] for r in records],
        mean_area_reward=mean(lambda r: r['quality']['mean_area_reward']),
        mean_raw_advantage_std=mean(lambda r: r['ppo']['raw_advantage_std']),
        mean_first_epoch_kl=mean(lambda r: r['ppo']['epochs'][0]['approximate_kl']),
        mean_first_epoch_entropy=mean(lambda r: r['ppo']['epochs'][0]['entropy']),
        mean_first_epoch_max_actor_gradient_before_clip=mean(lambda r: r['ppo']['epochs'][0]['actor_gradient_norm_max']),
        mean_first_epoch_optimizer_steps=mean(lambda r: r['ppo']['epochs'][0]['optimizer_steps']),
        mean_first_epoch_outside_clip_fraction=mean(lambda r: r['ppo']['epochs'][0]['outside_clip_fraction']),
        one_epoch_updates=sum(len(r['ppo']['epochs']) == 1 for r in records),
        kl_stopped=sum(r['ppo']['kl_stopped'] for r in records))


def main(args):
    root, output = args.run.resolve(), args.output.resolve()
    check(not output.exists(), f'fresh output required: {output}')
    check(args.through >= args.window > 0, 'positive window within committed prefix required')
    manifest = []
    protocol = read(root / 'protocol.json', manifest)
    state_bytes = (root / 'state.json').read_bytes()
    state = json.loads(state_bytes)
    check(len(state['jobs']) == 1 and state['protocol_sha256'] == manifest[0]['sha256'], 'one matching job/protocol required')
    manifest.append(dict(path=str(output / 'state-snapshot.json'), sha256=sha(state_bytes)))
    records = []
    for ref in state['jobs'][0]['records']:
        if ref['phase']['kind'] == 'training' and ref['phase']['batch'] <= args.through:
            record = read(root / ref['path'], manifest)
            check(manifest[-1]['sha256'] == ref['sha256'], f'record hash differs: {ref["path"]}')
            records.append(record)
    check([r['phase']['batch'] for r in records] == list(range(1, args.through + 1)), 'complete committed prefix required')
    retained = []
    for batch in args.trace_batches:
        check(1 <= batch <= args.through, 'retained batch must be within prefix')
        record = records[batch - 1]
        samples = []
        for result in record['results']:
            if result['status'] != 'complete':
                continue
            trace = read(Path(result['row']['directory']) / 'episode.json.gz', manifest)['trace']
            check(len(trace) == result['row']['selected'], 'complete retained trace required')
            raw = advantages(trace, protocol['config']['ppo']['gae_lambda'])
            for position, (step, value) in enumerate(zip(trace, raw), 1):
                samples.append(dict(episode=result['identity']['index'], position=position, kind=step['action']['kind'],
                    area_reward=step['area_reward'], reward=step['reward'], old_value=step['old_value'],
                    raw_advantage=float(value), old_log_probability=step['old_log_probability'], status=step['status']))
        check(len(samples) == record['ppo']['selected'], 'all retained training actions required')
        raw = np.asarray([s['raw_advantage'] for s in samples], dtype=np.float32)
        expected = record['ppo']
        for actual, key in ((float(raw.mean()), 'raw_advantage_mean'), (float(raw.std()), 'raw_advantage_std')):
            check(math.isclose(actual, expected[key], rel_tol=1e-5, abs_tol=1e-7), f'reconstructed GAE differs: batch {batch}, {key}')
        normalized = (raw - expected['raw_advantage_mean']) / max(expected['raw_advantage_std'], 1e-8)
        order = np.argsort(np.abs(normalized))
        retained.append(dict(batch=batch, selected=len(samples), raw_advantage_std=expected['raw_advantage_std'],
            maximum_step_area_reward=max(s['area_reward'] for s in samples),
            area_steps_at_least_25_percent=sum(s['area_reward'] >= .25 for s in samples),
            normalized_advantage_min=float(normalized.min()), normalized_advantage_max=float(normalized.max()),
            top_one_percent_squared_advantage_share=float((normalized[order[-max(1, len(samples) // 100):]] ** 2).sum() / (normalized ** 2).sum()),
            largest_absolute_advantages=[dict(samples[i], normalized_advantage=float(normalized[i])) for i in order[-5:][::-1]]))
    report = dict(schema='ppo-kl-record-diagnosis-v1', run=str(root), through=args.through,
        source_sha256=sha(Path(__file__).read_bytes()), protocol_sha256=state['protocol_sha256'],
        ppo=protocol['config']['ppo'], early=window(records[:args.window]), recent=window(records[-args.window:]),
        retained_batches=retained,
        limitations=['Recorded associations do not identify causal shares of KL change.',
                    'Epoch-end records cannot identify which minibatch crossed the threshold.',
                    'Raw gradient maxima are measured before clipping, not applied Adam step sizes.'])
    output.mkdir(parents=True)
    (output / 'state-snapshot.json').write_bytes(state_bytes)
    manifest_bytes = (json.dumps(manifest, indent=2) + '\n').encode()
    (output / 'inputs.json').write_bytes(manifest_bytes)
    report['input_manifest_sha256'] = sha(manifest_bytes)
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(dict(output=str(output), early=report['early'], recent=report['recent'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--through', type=int, required=True)
    parser.add_argument('--window', type=int, default=20)
    parser.add_argument('--trace-batches', type=int, nargs='+', required=True)
    parser.add_argument('--output', type=Path, required=True)
    main(parser.parse_args())
