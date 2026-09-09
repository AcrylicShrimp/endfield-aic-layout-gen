"""Summarize an audited fresh-factory pilot without changing its frozen protocol."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
from statistics import mean


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(f'fresh-analysis.input-contract: {message}')


def checked(manifest):
    require(sha(manifest['path']) == manifest['sha256'], manifest['path'])
    return read(manifest['path'])


def coverage(results):
    accepted = [r for r in results if r['status'] == 'complete']
    products = defaultdict(Counter)
    failures, kinds, setup_statuses = Counter(), Counter(), Counter()
    for result in results:
        generation = result['generation']
        product = products[generation['draw']['product']]
        product['attempted'] += 1
        product[result['status']] += 1
        if result['status'] != 'complete':
            failures.update(d['code'] for d in generation['original_attempt'].get('diagnostics', []))
            continue
        product['changed_geometry'] += generation['geometry_changed']
        product['area_improved'] += result['row']['best_metrics']['score'][0] < result['row']['initial_metrics']['score'][0]
        kinds.update(result['row']['kinds'])
        setup_statuses.update(generation['setup_statuses'])
    return dict(attempts=len(results), accepted=len(accepted), rejected=len(results) - len(accepted),
        unique_generation_seeds=len({r['generation']['generation_seed'] for r in results}),
        unique_requests=len({r['generation']['request_sha256'] for r in accepted}),
        unique_geometry=len({r['generation']['geometry_sha256'] for r in accepted}),
        product_rate_pairs=len({(r['generation']['draw']['product'], r['generation']['draw']['rate']['duration_ms']) for r in results}),
        facility_counts=dict(Counter(r['row']['instances'] for r in accepted)),
        products=dict(products), failure_codes=dict(failures), policy_kinds=dict(kinds),
        setup_statuses=dict(setup_statuses),
        selected=sum(r['row']['selected'] for r in accepted),
        summed_generation_seconds=sum(r['generation']['seconds'] for r in results),
        summed_rollout_seconds=sum(r['row']['seconds'] for r in accepted),
        changed_geometry=sum(r['generation']['geometry_changed'] for r in accepted))


def paired(evaluation, initial):
    baseline = {r['identity']['index']: r for r in initial['results']}
    rows = defaultdict(list)
    for result in evaluation['results']:
        before = baseline[result['identity']['index']]
        require(result['status'] == before['status'], 'paired admission')
        if result['status'] != 'complete':
            continue
        require(result['generation']['initial_sha256'] == before['generation']['initial_sha256'], 'paired starts')
        a, b = before['row']['best_metrics']['score'], result['row']['best_metrics']['score']
        area = result['row']['initial_metrics']['score'][0]
        row = dict(outcome='wins' if b < a else 'losses' if b > a else 'ties',
                   initial_area_gain=100 * (area - a[0]) / area,
                   area_gain=100 * (area - b[0]) / area,
                   paired_delta=100 * (a[0] - b[0]) / area)
        rows['all'].append(row)
        rows[result['generation']['draw']['product']].append(row)
    return dict(update=evaluation['update'], products={product: dict(samples=len(values),
        **dict(Counter(v['outcome'] for v in values)),
        initial_area_reduction_percent=mean(v['initial_area_gain'] for v in values),
        area_reduction_percent=mean(v['area_gain'] for v in values),
        paired_area_delta_pp=mean(v['paired_delta'] for v in values)) for product, values in rows.items()})


def run(args):
    protocol_path = args.screen / 'protocol.json'
    protocol = read(protocol_path)
    audit = read(args.screen / 'audit/report.json')
    require(audit['status'] == 'complete' and audit['protocol_sha256'] == sha(protocol_path), 'completed audit')
    require(not protocol['smoke'] or args.allow_smoke, 'smoke requires explicit flag')
    scaling_path, learning_path = args.screen / 'scaling/report.json', args.screen / 'learning/report.json'
    require(audit['scaling_sha256'] == sha(scaling_path) and audit['learning_sha256'] == sha(learning_path), 'audited reports')
    scaling, learning = read(scaling_path), read(learning_path)
    cells, serial = [], []
    for cell in scaling['cells']:
        if cell['status'] != 'complete':
            cells.append(dict(workers=cell['workers'], status=cell['status'], diagnostic=cell.get('diagnostic')))
            continue
        records = [checked(r) for r in cell['repetitions']]
        results = [r for record in records for r in record['results']]
        selected = sum(r['selected'] for r in records)
        elapsed = sum(r['seconds'] for r in records)
        cells.append(dict(workers=cell['workers'], status=cell['status'],
            semantic_parity=all(r['semantic_parity'] for r in records),
            valid_actions_per_second=selected / elapsed,
            valid_episodes_per_second=sum(r['accepted'] for r in records) / elapsed,
            repetition_actions_per_second=[r['valid_actions_per_second'] for r in records],
            collection_seconds=elapsed, startup_seconds=records[0]['startup_seconds'],
            peak_sampled_rss_gib=max(r['peak_sampled_rss_bytes'] for r in records) / 1024 ** 3,
            **coverage(results)))
        if cell['workers'] == 1:
            serial = results
    baseline = next(c['valid_actions_per_second'] for c in cells if c['workers'] == 1)
    for cell in cells:
        if cell['status'] == 'complete':
            cell['speedup_over_serial'] = cell['valid_actions_per_second'] / baseline
    seeds = []
    for manifest in learning['rows']:
        record = checked(manifest)
        results = [r for group in record['groups'] for r in group['results']]
        returns = [value for group in record['groups'] for value in group['returns']]
        seeds.append(dict(seed=record['seed'], training=coverage(results),
            nonflat_groups=sum(not g['flat_returns'] for g in record['groups']),
            positive_relative_returns=sum(v > 0 for v in returns),
            updates=[dict(update=g['group'] + 1, accepted=g['accepted'],
                reward_gradient_norm=g['reward_gradient_norm'], entropy_gradient_norm=g['entropy_gradient_norm'],
                pre_clip_gradient_norm=g['pre_clip_gradient_norm'],
                mean_training_area_reduction_percent=mean(100 * (1 - r['row']['best_metrics']['score'][0] /
                    r['row']['initial_metrics']['score'][0]) for r in g['results'] if r['status'] == 'complete'))
                for g in record['groups']],
            evaluation=[paired(e, record['evaluations'][0]) for e in record['evaluations']]))
    summary = dict(status='complete', smoke=protocol['smoke'],
        protocol_sha256=sha(protocol_path), audit_sha256=sha(args.screen / 'audit/report.json'),
        analysis_source_sha256=sha(__file__), selected_workers=scaling['selected_workers'],
        scaling_seconds=scaling['seconds'], learning_seconds=learning['seconds'],
        scaling=cells, serial_coverage=coverage(serial), learning=seeds,
        audit=dict(verified_results=audit['verified_results'], replayed_actions=audit['replayed_actions']),
        limitations=['Four configured products and discrete period ranges; starts use bounded native random edits.',
            'Batch size and return normalization also changed; this is not a causal isolation of task diversity.',
            'Sixteen matched evaluation draws per seed; positive training ranks alone do not establish learning.',
            'Summed stage times are worker time; throughput uses observed collection wall time.'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(summary, indent=2, allow_nan=False) + '\n')
    print(json.dumps(dict(status='complete', output=str(args.output), selected_workers=summary['selected_workers'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--allow-smoke', action='store_true')
    run(parser.parse_args())
