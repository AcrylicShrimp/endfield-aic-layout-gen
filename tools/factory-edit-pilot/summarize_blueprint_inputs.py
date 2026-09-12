"""Verify actual per-episode generation coverage from durable learner records."""
import argparse
from collections import Counter
from pathlib import Path

from blueprint_ppo_store import checked, read, reference, sha, write
from blueprint_ppo_update import digest, require


def summarize(output):
    protocol, state = read(output / 'protocol.json'), read(output / 'state.json')
    require(state['protocol_sha256'] == sha(output / 'protocol.json'), 'input summary protocol binding')
    config = protocol['config']
    require(config['training_input']['kind'] == 'fresh-random', 'input summary requires fresh random training')
    root = Path(config['training_input']['directory'])
    manifest = read(root / 'manifest.json')
    jobs = []
    for seed, job in state['jobs'].items():
        inputs, completed = [], 0
        for record_ref in job['records']:
            record = read(checked(record_ref, output))
            inputs.extend(record['training_inputs'])
            completed += record['quality'].get('accepted', 0)
        slots = [row['identity']['slot'] for row in inputs]
        expected_episodes = (job['update'] + 1) // 2 * config['training']['streams']
        require(config['training']['episode_actions'] == 2 * config['training']['chunk_actions'],
                'input summary expects the declared two-chunk episodes')
        require(slots == list(range(expected_episodes)), 'input summary: missing, reordered or recycled episode slots')
        require(completed == job['update'] // 2 * config['training']['streams'], 'input summary: complete episode coverage')
        generations = []
        for metadata in inputs:
            row = read(checked(metadata['reference'], root))
            require(row['identity'] == metadata['identity'] and row['input_sha256'] == metadata['input_sha256']
                    and row['contract_sha256'] == digest(manifest), 'input summary: episode input binding')
            for value in row['files'].values():
                checked(value, root)
            for attempt in row['attempts']:
                checked(attempt['generation'], root)
            require(digest(read(checked(row['files']['input'], root))) == row['input_sha256'], 'input summary: payload binding')
            generations.append(row)
        require(len({row['generation_seed'] for row in generations}) == len(generations), 'input summary: recycled generation seeds')
        jobs.append(dict(seed=int(seed), updates=job['update'], started_episodes=len(inputs), completed_episodes=completed,
            unique_generation_seeds=len({row['generation_seed'] for row in generations}),
            products=dict(Counter(row['draw']['product'] for row in generations)),
            distinct_product_rates=len({(row['draw']['product'], row['draw']['rate']['duration_ms']) for row in generations}),
            distinct_requests=len({row['request_sha256'] for row in generations}),
            distinct_geometries=len({row['geometry_sha256'] for row in generations}),
            generation_attempts=sum(len(row['attempts']) for row in generations),
            setup_seconds=sum(row['setup_seconds'] for row in generations),
            ordered_input_digest=digest([(row['identity'], row['input_sha256']) for row in generations]),
            input_records=[row['reference'] for row in inputs]))
    return dict(schema='blueprint-input-summary-v1', status='passed', protocol=reference(output / 'protocol.json', output),
        state=reference(output / 'state.json', output), input_store=str(root), manifest_sha256=sha(root / 'manifest.json'), jobs=jobs)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(not args.output.exists(), f'new summary output required: {args.output}')
    result = summarize(args.run.resolve())
    write(args.output, result)
    print({k: v for k, v in result.items() if k != 'jobs'})
    for job in result['jobs']:
        print({k: v for k, v in job.items() if k != 'input_records'})
