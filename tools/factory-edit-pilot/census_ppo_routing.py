"""Count committed PPO routing failures, preserving retained-trace coverage."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import statistics
import time

from ppo_harness_state import checked_path, check_sources, read, require, sha, write


def bucket():
    result = {key: 0 for key in (
        'phases attempts complete_episodes selected episode_seconds expansions '
        'detail_episodes detail_actions detail_work_failures detail_region_actions '
        'detail_executor_seconds detail_failed_seconds detail_budget_failed_seconds '
        'detail_expansions detail_failed_expansions detail_budget_failed_expansions '
        'detail_pose_actions detail_pose_failures').split()}
    result.update({key: Counter() for key in (
        'statuses kinds detail_statuses detail_fail_codes detail_region_statuses '
        'detail_attempt_counts detail_failure_attempt_counts detail_actions_by_factory').split()})
    result.update(failed_expansions=[], found_expansions=[])
    return result


def work_failure(step):
    return any(d['code'] in ('local-work-budget', 'region-work-exhausted')
               for d in step['diagnostics'])


def add_episode(b, row, trace):
    b['complete_episodes'] += 1
    b['selected'] += row['selected']
    b['statuses'].update(row['statuses'])
    b['kinds'].update(row['kinds'])
    b['episode_seconds'] += row['seconds']
    b['expansions'] += row['route_expansions']
    if trace is None:
        return
    b['detail_episodes'] += 1
    b['detail_actions'] += len(trace)
    for step in trace:
        failed = step['status'] in ('executor_rejected', 'rejected', 'late')
        budget = work_failure(step)
        b['detail_statuses'][step['status']] += 1
        b['detail_executor_seconds'] += step['executor_seconds']
        b['detail_expansions'] += step['route_expansions']
        b['detail_actions_by_factory'][str(row['instances'])] += 1
        if step['action']['kind'] == 'pose':
            b['detail_pose_actions'] += 1
            b['detail_pose_failures'] += int(failed)
        if failed:
            b['detail_fail_codes'].update({d['code'] for d in step['diagnostics']})
            b['detail_failed_seconds'] += step['executor_seconds']
            b['detail_failed_expansions'] += step['route_expansions']
        b['failed_expansions' if failed else 'found_expansions'].append(step['route_expansions'])
        if budget:
            b['detail_work_failures'] += 1
            b['detail_budget_failed_seconds'] += step['executor_seconds']
            b['detail_budget_failed_expansions'] += step['route_expansions']
        if 'region' in step:
            region = step['region']
            count = str(len(region['attempts']))
            b['detail_region_actions'] += 1
            b['detail_region_statuses'][region['termination']] += 1
            b['detail_attempt_counts'][count] += 1
            if budget:
                b['detail_failure_attempt_counts'][count] += 1


def run(args):
    root, out = args.run.resolve(), args.output.resolve()
    protocol = read(root / 'protocol.json')
    check_sources(protocol)
    require(not out.exists(), 'census-output', 'fresh output directory required', entity=out)
    if args.snapshot:
        snapshot = read(args.snapshot)
        require(snapshot['protocol_sha256'] == sha(root / 'protocol.json'),
                'census-protocol', 'snapshot protocol differs', entity=args.snapshot)
    else:
        state = read(root / 'state.json')
        jobs = [j for j in state['jobs'] if j['id'] == args.job]
        require(len(jobs) == 1, 'census-job', 'expected one job', entity=args.job)
        j = jobs[0]
        snapshot = dict(run=str(root), protocol_sha256=state['protocol_sha256'],
                        completed_batches=j['completed_batches'], observed_epoch=time.time(),
                        job=j['id'], records=j['records'])
    refs = snapshot['records']
    evaluations = [r for r in refs if r['phase']['kind'] == 'evaluation']
    require(evaluations, 'census-evaluation', 'no committed evaluation', entity=root)
    last_eval = evaluations[-1]
    out.mkdir(parents=True)
    if args.snapshot:
        (out / 'snapshot.json').write_bytes(args.snapshot.read_bytes())
    else:
        write(out / 'snapshot.json', snapshot)
    buckets = {key: bucket() for key in ('training', 'evaluation', 'latest_evaluation')}
    latest = []
    for ref in refs:
        record = read(checked_path(root, ref))
        keys = [ref['phase']['kind']] + (['latest_evaluation'] if ref == last_eval else [])
        for key in keys:
            buckets[key]['phases'] += 1
        for result in record['results']:
            for key in keys:
                buckets[key]['attempts'] += 1
            if result['status'] != 'complete':
                continue
            row = result['row']
            path = Path(row['directory']) / 'episode.json.gz'
            try:
                with gzip.open(path, 'rt') as stream:
                    trace = json.load(stream)['trace']
            except FileNotFoundError:
                trace = None
            for key in keys:
                add_episode(buckets[key], row, trace)
            if ref == last_eval:
                require(trace is not None, 'census-trace', 'latest evaluation trace missing', entity=path)
                fails = [step for step in trace if work_failure(step)]
                if fails:
                    latest.append(dict(episode_index=result['identity']['index'],
                        generation_seed=result['identity']['generation_seed'], instances=row['instances'],
                        product=result['generation']['draw']['product'], result=result,
                        selected_action_number=fails[0]['selected'], budget_failures=len(fails)))
    for b in buckets.values():
        for key in ('failed_expansions', 'found_expansions'):
            values = sorted(b.pop(key))
            b[key + '_summary'] = dict(count=len(values),
                median=statistics.median(values) if values else None,
                p95=values[int(.95 * (len(values) - 1))] if values else None)
    report = dict(schema='routing-failure-census-v1', snapshot_sha256=sha(out / 'snapshot.json'),
        completed_batches=snapshot['completed_batches'], latest_evaluation=last_eval, buckets=buckets,
        sample_count=len(latest), sample=[{k: v for k, v in r.items() if k != 'result'} for r in latest])
    write(out / 'census.json', report)
    write(out / 'sample.json', dict(record=last_eval, rows=latest))
    print(json.dumps(dict(output=str(out), sample_count=len(latest),
                         completed_batches=snapshot['completed_batches'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--job')
    parser.add_argument('--snapshot', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
