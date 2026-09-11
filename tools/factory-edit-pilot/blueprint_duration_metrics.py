"""Shared harness metric names with explicit blueprint episode/evaluation semantics."""


def require(condition, message):
    if not condition:
        raise ValueError(f'blueprint.duration-metrics: {message}')


def training_quality(payload):
    selected = len(payload['samples'])
    failed = sum(s['outcome']['status'] == 'rejected' for s in payload['samples'])
    quality = dict(failed_actions=failed, failure_fraction=failed / selected if selected else 0.)
    completed = payload['completed']
    if completed:
        require(all(e['result']['factory_validated'] for e in completed), 'unvalidated completed training episode')
        reductions = [100 * (1 - e['result']['best_metrics']['score'][0] / e['initial_score'][0]) for e in completed]
        quality.update(accepted=len(completed), attempted=len(completed),
            mean_area_reduction_percent=sum(reductions) / len(completed),
            mean_area_reward=sum(e['counts']['area_reward'] for e in completed) / len(completed),
            mean_reward=sum(e['counts']['area_reward'] for e in completed) / len(completed))
    return quality


def evaluation_metrics(rows, initial):
    require(bool(rows) and bool(initial), 'evaluation and baseline must contain cases')
    identity = lambda row: (row['start'], row['sample'])
    baseline = {identity(row): row for row in initial}
    current = {identity(row): row for row in rows}
    require(len(baseline) == len(initial) and len(current) == len(rows), 'duplicate evaluation case identity')
    require(baseline.keys() == current.keys(), 'evaluation cohort differs from initialization')
    require(all(current[k]['start_score'] == baseline[k]['start_score'] for k in current), 'evaluation reset drift')
    for key in ('generation_seed', 'request_sha256', 'initial_sha256'):
        require(all(current[k].get(key) == baseline[k].get(key) for k in current), f'evaluation {key} drift')
    mean = sum(r['total_area_reduction_percent'] for r in rows) / len(rows)
    delta = sum(current[k]['total_area_reduction_percent'] - baseline[k]['total_area_reduction_percent'] for k in current) / len(rows)
    result = {}
    for axis in ('actions', 'batches', 'seconds'):
        prefix = f'evaluation/all_admitted/by_{axis}'
        result.update({f'{prefix}/area_reduction_percent': mean, f'{prefix}/delta_from_initial_pp': delta})
    failed = sum(r['counts'].get('status/rejected', 0) for r in rows)
    actions = sum(r['policy_actions'] for r in rows)
    result.update({'evaluation/accepted': len(rows), 'evaluation/attempted': len(rows),
        'evaluation/coverage_fraction': 1., 'evaluation/paired_cases': len(current),
        'evaluation/failed_actions': failed, 'evaluation/failure_fraction': failed / actions if actions else 0.})
    for name in sorted({r['start'] for r in rows if 'generation_seed' not in r}):
        group = [r for r in rows if r['start'] == name]
        result[f'evaluation/{name}/mean_area_reduction_percent'] = sum(r['total_area_reduction_percent'] for r in group) / len(group)
        result[f'evaluation/{name}/mean_best_area'] = sum(r['best_score'][0] for r in group) / len(group)
    return result


def metrics(record, update, actions_per_batch, *, seconds=None, initial=None):
    require(record['update'] == update, 'update identity')
    result = {'train/actions': update * actions_per_batch, 'train/batches': update}
    if seconds is not None:
        result['time/elapsed_seconds'] = seconds
    if initial is not None:
        result.update(evaluation_metrics(record['rows'], initial))
        return result
    require(record['actions'] == actions_per_batch, 'batch action count')
    result.update({'training/area_reward': record['area_reward'],
                   'timing/collection_seconds': record['collection_seconds']})
    result.update({f'training/{key}': value for key, value in record.get('quality', {}).items()})
    for prefix, source in [('buffer', record['buffer']), ('ppo', record['ppo']),
                           ('success', record['ppo']['success']), ('ppo', record['ppo']['epochs'][-1])]:
        result.update({f'{prefix}/{key}': number for key, number in source.items()
                       if type(number) in (int, float, bool) and key != 'seconds'})
    result['ppo/epochs'] = len(record['ppo']['epochs'])
    if 'seconds' in record['ppo']:
        result['timing/update_seconds'] = record['ppo']['seconds']
    return result


def define_metrics(run):
    for axis in ('train/actions', 'train/batches', 'time/elapsed_seconds'):
        run.define_metric(axis)
    for prefix in ('training', 'ppo', 'timing', 'success', 'buffer', 'evaluation'):
        run.define_metric(f'{prefix}/*', step_metric='train/actions')
    for axis, step in [('actions', 'train/actions'), ('batches', 'train/batches'), ('seconds', 'time/elapsed_seconds')]:
        run.define_metric(f'evaluation/all_admitted/by_{axis}/*', step_metric=step)
