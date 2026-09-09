"""Verify completed screen accounting and actual W&B scalars without replaying learning."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import math
from pathlib import Path
import time

from ppo_harness_comparison import tracking_jobs
from ppo_harness_environment import load_environment
from ppo_harness_monitor import snapshot
from ppo_harness_state import read, write, checked_path, require, sha, check_sources
from ppo_harness_tracking import metrics


def check(root, cloud):
    began = time.monotonic()
    protocol = read(root / 'protocol.json')
    check_sources(protocol)
    value = snapshot(root)
    require(value['status'] == 'complete', 'record-check-incomplete', 'a completed run is required', entity=root)
    require([j['id'] for j in value['jobs']] == [j['id'] for j in protocol['jobs']],
            'record-check-jobs', 'job order differs from the frozen schedule', entity=root)
    checked = set()
    summaries = []
    for job in value['jobs']:
        require(job['status'] == 'complete' and [r['phase'] for r in job['records']] == protocol['phases'],
                'record-check-phases', 'completed job differs from the frozen phase schedule', entity=job['id'])
        actions, training_seeds, evaluated = 0, set(), []
        for record in job['records']:
            training = record['phase']['kind'] == 'training'
            schedule = protocol['config']['training' if training else 'evaluation']
            results = record['results']
            require(len(results) == schedule['attempts'], 'record-check-attempts', 'scheduled attempts missing', entity=job['id'])
            admitted = [r for r in results if r['status'] == 'complete']
            require(all(r['row']['budget_complete'] and r['row']['selected'] == protocol['config']['training']['horizon'] for r in admitted),
                    'record-check-horizon', 'an admitted episode lacks the complete horizon', entity=job['id'])
            selected = sum(r['row']['selected'] for r in admitted)
            require(record['quality']['accepted'] == len(admitted) and record['quality']['selected'] == selected,
                    'record-check-accounting', 'summary differs from episode accounting', entity=job['id'])
            seeds = []
            for index, result in enumerate(results):
                ordinal = (record['phase']['batch']-1)*schedule['attempts'] + index if training else index
                expected = schedule['generation_seed_base'] + job['seed']*100000 + ordinal
                require(result['identity']['generation_seed'] == expected and result['identity']['index'] == index,
                        'record-check-seed', 'result is outside the frozen task schedule', entity=job['id'])
                seeds.append(expected)
            if training:
                require(not training_seeds.intersection(seeds), 'record-check-freshness', 'training generation seed reused', entity=job['id'])
                training_seeds.update(seeds)
                actions += selected
            else:
                evaluated.append(seeds)
            require(record['selected_training_actions'] == actions, 'record-check-actions', 'cumulative action axis differs', entity=job['id'])
            for reference in (record['behavior_checkpoint'], record['checkpoint']):
                key = (reference['path'], reference['sha256'])
                if key not in checked:
                    checked_path(root, reference)
                    checked.add(key)
        require(actions == job['selected_training_actions'] and all(s == evaluated[0] for s in evaluated) and
                not training_seeds.intersection(evaluated[0]), 'record-check-job', 'job totals or held-out separation differ', entity=job['id'])
        summaries.append(dict(job=job['id'], training_actions=actions, unique_training_draws=len(training_seeds)))
    # This also verifies the shared cohort and identical factory hashes across
    # every candidate/checkpoint. It never calls a model or native executor.
    exported = tracking_jobs(value, protocol)
    if cloud:
        tracking = read(root / 'tracking.json')['jobs']
        destination = protocol['config']['tracking']
        def check_cloud(job):
            import wandb
            saved = tracking[job['id']]
            remote = wandb.Api(timeout=15).run(f"{destination['entity']}/{destination['project']}/{saved['run_id']}")
            rows = list(remote.scan_history(page_size=100))
            by_step = {int(row['_step']): row for row in rows}
            require(len(rows) == len(by_step) and set(by_step) == set(range(len(job['records']))),
                    'cloud-records', 'missing, duplicate or unexpected history ordinals', entity=job['id'])
            count = 0
            for ordinal, record in enumerate(job['records']):
                for key, expected in metrics(record).items():
                    actual = by_step[ordinal].get(key)
                    require(type(actual) in (int, float) and math.isclose(actual, expected, rel_tol=1e-8, abs_tol=1e-8),
                            'cloud-scalar', f'committed scalar differs: {key}', entity=job['id'])
                    count += 1
            return dict(job=job['id'], history_records=len(rows), scalars=count, url=saved['url'])
        with ThreadPoolExecutor(max_workers=4) as executor:
            cloud_results = list(executor.map(check_cloud, exported))
    else:
        cloud_results = None
    return dict(schema='ppo-harness-record-check-v1', status='complete', protocol_sha256=value['protocol_sha256'],
                source_sha256=sha(__file__), seconds=time.monotonic()-began, jobs=summaries,
                checked_checkpoint_files=len(checked), shared_cohorts_verified=True, cloud=cloud_results)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--check-wandb', action='store_true')
    args = parser.parse_args()
    load_environment(Path(__file__).resolve().parents[2] / '.env')
    result = check(args.run.resolve(), args.check_wandb)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write(args.output, result)
    print(json.dumps(dict(status=result['status'], jobs=len(result['jobs']), checkpoint_files=result['checked_checkpoint_files'],
                         cloud_scalars=sum(j['scalars'] for j in result['cloud']) if result['cloud'] else None,
                         output=str(args.output.resolve())), allow_nan=False))
