"""Audit the completed factorial screen and compare paired seed-level outcomes."""
import argparse
from copy import deepcopy
import gzip
import hashlib
from itertools import product, combinations
import json
import math
from pathlib import Path
import statistics

from ppo_actor_critic import FIELDS, treatment, label
from ppo_critic_metrics import episode as value_moments, aggregate as value_metrics
from ppo_episode_metrics import episode_metrics, progress_metrics


def check(condition, message):
    if not condition:
        raise ValueError(f'ppo-combinations.audit: {message}')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path, manifest, expected=None):
    path = Path(path)
    raw = path.read_bytes()
    digest = sha(raw)
    check(expected is None or digest == expected, f'hash differs: {path}')
    manifest[str(path.resolve())] = digest
    return json.loads(gzip.decompress(raw) if path.suffix == '.gz' else raw)


def verify_file(path, manifest, expected):
    path = Path(path).resolve()
    key = str(path)
    if key not in manifest:
        with path.open('rb') as stream:
            manifest[key] = hashlib.file_digest(stream, 'sha256').hexdigest()
    check(manifest[key] == expected, f'hash differs: {path}')


def close(actual, expected, name):
    if isinstance(expected, dict):
        check(set(actual) == set(expected), f'field mismatch: {name}')
        for key in expected:
            close(actual[key], expected[key], f'{name}.{key}')
    elif expected is None:
        check(actual is None, f'expected null: {name}')
    elif isinstance(expected, (int, float)):
        check(math.isclose(actual, expected, abs_tol=1e-9, rel_tol=1e-8), f'value differs: {name}')
    else:
        check(actual == expected, f'value differs: {name}')


def audit_record(record, retained, manifest):
    rows = []
    for result in record['results']:
        read(result['result_path'], manifest, result['result_sha256'])
        task = read(Path(result['result_path']).with_name('task.json'), manifest, result['input_sha256'])
        check(task['identity'] == result['identity'] and task['policy_sha256'] == result['policy_sha256']
              and task['critic_sha256'] == result['critic_sha256'], 'result task identity')
        if result['status'] != 'complete':
            continue
        row = result['row']
        if retained:
            summary = read(Path(row['directory'])/'episode.json.gz', manifest)
            trace = summary['trace']
            check(len(trace) == row['selected'] and summary['budget_complete'], 'complete native trace required')
            for key in ('area_reward_sum', 'failure_cost_sum', 'reward_sum'):
                field = {'area_reward_sum': 'area_reward', 'failure_cost_sum': 'failure_cost', 'reward_sum': 'reward'}[key]
                close(sum(s[field] for s in trace), row[key], key)
            close(value_moments(trace), row['critic_prediction'], 'predictive moments')
            for key, value in episode_metrics(trace).items():
                close(value, row[key], key)
            if record['phase']['kind'] == 'training':
                verify_file(result['experience'], manifest, result['experience_sha256'])
        rows.append(row)
    recomputed = value_metrics(rows)['critic_prediction']
    close(recomputed, record['quality']['critic_prediction'], 'predictive aggregate')
    close(100*statistics.mean(r['area_reward_sum'] for r in rows),
          record['quality']['mean_area_reduction_percent'], 'area reduction')
    for key, value in progress_metrics(rows).items():
        close(value, record['quality'][key], key)
    check(sum(r['failed_actions'] for r in rows) == record['quality']['failed_actions'], 'failed action accounting')
    if record['phase']['kind'] == 'training':
        ppo = record['ppo']
        expected_steps = math.ceil(ppo['selected']/record['_minibatch_size'])
        check(ppo['actor_optimizer_steps'] == expected_steps*ppo['actor_epochs'], 'actor step accounting')
        check(ppo['critic_optimizer_steps'] == expected_steps*ppo['critic_epochs'], 'critic step accounting')
        tail = ppo['critic_tail']
        check(ppo['critic_epochs'] == ppo['actor_epochs'] + len(tail['epochs']), 'critic epoch accounting')
        if tail['epochs']:
            check(tail['actor_parameters_and_optimizer_unchanged'] and tail['joint_shuffle_unchanged'], 'tail policy preservation')
            for epoch in tail['epochs']:
                close(epoch['approximate_kl'], ppo['epochs'][-1]['approximate_kl'], 'tail KL')
        expected = record['_epochs'] if ppo['actor_critic']['critic_after_kl'] else ppo['actor_epochs']
        check(ppo['critic_epochs'] == expected, 'registered critic schedule')


def evaluation(record):
    q = record['quality']
    return dict(batch=record['phase']['batch'], accepted=q['accepted'], selected=q['selected'],
                area_reduction_pp=q['mean_area_reduction_percent'], later_reduction_pp=q['mean_later_area_reduction_percent'],
                failure_percent=100*q['failed_actions']/q['selected'],
                critic_mae_pp=100*q['critic_prediction']['all']['mae'],
                critic_late_mae_pp=100*q['critic_prediction']['late']['mae'],
                critic_late_bias_pp=100*q['critic_prediction']['late']['bias'],
                critic_prediction=q['critic_prediction'],
                pairs=[dict(generation_seed=r['identity']['generation_seed'],
                            area_reduction_pp=100*r['row']['area_reward_sum'],
                            later_reduction_pp=100*r['row']['later_area_reward_sum'])
                       for r in record['results'] if r['status'] == 'complete'])


def effects(cells):
    """Average matched factor effects within seed; contexts are not replicates."""
    lookup = {tuple(c['factors'][k] for k in FIELDS): c for c in cells}
    seeds = sorted(cells[0]['jobs'])
    metrics = ('area_reduction_pp', 'later_reduction_pp', 'failure_percent', 'critic_mae_pp',
               'critic_late_mae_pp', 'critic_late_bias_pp', 'training_seconds', 'update_seconds')
    result = []
    for size in (1, 2, 3):
        for selected in combinations(range(3), size):
            remaining = [i for i in range(3) if i not in selected]
            by_seed = {}
            conditional = []
            for seed in seeds:
                contexts = []
                for rest in product((False, True), repeat=len(remaining)):
                    delta = dict.fromkeys(metrics, 0.)
                    for bits in product((False, True), repeat=len(selected)):
                        factors = [False]*3
                        for i, bit in zip(remaining, rest):
                            factors[i] = bit
                        for i, bit in zip(selected, bits):
                            factors[i] = bit
                        sign = (-1)**(len(selected)-sum(bits))
                        summary = lookup[tuple(factors)]['jobs'][seed]['summary']
                        for metric in metrics:
                            delta[metric] += sign*summary[metric]
                    contexts.append(delta)
                    conditional.append(dict(seed=seed, context={FIELDS[i]: bit for i, bit in zip(remaining, rest)}, differences=delta))
                by_seed[seed] = {metric: statistics.mean(d[metric] for d in contexts) for metric in metrics}
            result.append(dict(factors=[FIELDS[i] for i in selected], by_seed=by_seed, conditional=conditional,
                mean={metric: statistics.mean(d[metric] for d in by_seed.values()) for metric in metrics}))
    return result


def schedule_prefixes(updates):
    """Only the first critic tail can separate otherwise identical schedules."""
    result = []
    for shared, history in product((False, True), repeat=2):
        controls, tails = updates[(shared, history, False)], updates[(shared, history, True)]
        check(set(controls) == set(tails), 'schedule-pair seeds differ')
        for seed, left in controls.items():
            right = tails[seed]
            check(len(left) == len(right), 'schedule-pair update counts differ')
            first_tail = None
            for number, (a, b) in enumerate(zip(left, right), 1):
                for key in ('policy_before_sha256', 'critic_before_sha256', 'policy_after_sha256',
                            'actor_optimizer_sha256', 'shuffle_rng_sha256', 'selected', 'kl_stopped', 'epochs'):
                    check(a[key] == b[key], f'joint prefix differs: shared={shared}, history={history}, seed={seed}, batch={number}, {key}')
                if a['kl_stopped']:
                    first_tail = number
                    break
                for key in ('critic_after_sha256', 'critic_optimizer_sha256'):
                    check(a[key] == b[key], f'critic differs before first tail: seed={seed}, batch={number}, {key}')
            result.append(dict(shared_backbone=shared, critic_history=history, seed=seed,
                               exact_joint_prefix_batches=first_tail or len(left), first_tail_batch=first_tail))
    return result


def compare(matrix_path, output, native):
    check(not output.exists(), f'fresh output required: {output}')
    manifest = {}
    matrix = read(matrix_path, manifest)
    check(matrix['schema'] == 'ppo-actor-critic-matrix-v1' and matrix['status'] == 'complete'
          and not matrix['readiness'] and len(matrix['cells']) == 8, 'completed eight-cell screen required')
    read(matrix['source_config']['path'], manifest, matrix['source_config']['sha256'])
    cells, controls, geometries, initial_actors, initial_critics = [], None, {}, {}, {}
    updates = {}
    native_runtime = None
    output.mkdir(parents=True)
    verified_layouts = 0
    for cell in matrix['cells']:
        root = Path(cell['root'])
        read(root.with_suffix('.json'), manifest, cell['config_sha256'])
        protocol = read(root/'protocol.json', manifest, cell['protocol_sha256'])
        for path, expected in protocol['sources'].items():
            verify_file(path, manifest, expected)
        state = read(root/'state.json', manifest, cell['state_sha256'])
        check(state['protocol_sha256'] == cell['protocol_sha256'], 'state protocol identity')
        check(state['status'] == 'complete' and len(state['jobs']) == 2, 'both seeds must finish')
        config = deepcopy(protocol['config'])
        factors = config['model'].pop('actor_critic')
        check(factors == cell['factors'] and label(factors) == cell['name'], 'cell treatment mismatch')
        if controls is None:
            controls = config
        check(config == controls, 'non-factor configuration differs')
        if native and native_runtime is None:
            from native_factory_runtime import FactoryRuntime
            native_runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
        item = dict(name=cell['name'], factors=factors, root=str(root), jobs={})
        update_key = tuple(factors[k] for k in FIELDS)
        updates[update_key] = {}
        for job in state['jobs']:
            check(job['completed_batches'] == config['training']['batches'], 'incomplete training prefix')
            seed = str(job['seed'])
            train, evaluations = [], []
            for reference in job['records']:
                record = read(root/reference['path'], manifest, reference['sha256'])
                phase = record['phase']
                check(record['seed'] == job['seed'], 'record seed differs')
                record.update(_minibatch_size=config['ppo']['minibatch_size'], _epochs=config['ppo']['epochs'])
                retained = phase['kind'] == 'evaluation' or phase['batch'] in config['retention']['training_batches']
                audit_record(record, retained, manifest)
                for r in record['results']:
                    g = r['generation']
                    identity = (seed, phase['kind'], phase['batch'], r['identity']['index'])
                    physical = tuple(g.get(k) for k in ('status', 'generation_seed', 'request_sha256', 'initial_sha256', 'geometry_sha256'))
                    check(geometries.setdefault(identity, physical) == physical, f'matched input differs: {identity}')
                if phase['kind'] == 'training':
                    train.append(record)
                    checkpoint = record['checkpoint']
                    data = (root/checkpoint['path']).read_bytes()
                    check(sha(data) == checkpoint['sha256'], 'checkpoint hash differs')
                    manifest[str(root/checkpoint['path'])] = sha(data)
                else:
                    evaluations.append(evaluation(record))
                    if phase['batch'] == 0:
                        checkpoint = record['checkpoint']
                        verify_file(root/checkpoint['path'], manifest, checkpoint['sha256'])
                        actors = {r['policy_sha256'] for r in record['results']}
                        critics = {r['critic_sha256'] for r in record['results']}
                        check(len(actors) == len(critics) == 1, 'one initial parameter set required')
                        check(initial_actors.setdefault(seed, actors) == actors and initial_critics.setdefault(seed, critics) == critics,
                              'initial actor or critic parameters differ between treatments')
                    if native and phase['batch'] == config['training']['batches']:
                        from ppo_area_audit import verify_result
                        for r in record['results']:
                            verify_result(native_runtime, r, output)
                            verified_layouts += r['status'] == 'complete'
            check([r['phase']['batch'] for r in train] == list(range(1, config['training']['batches']+1)), 'training sequence differs')
            check([r['batch'] for r in evaluations] == config['evaluation']['at'], 'evaluation schedule differs')
            summary = {k: evaluations[-1][k] for k in ('area_reduction_pp', 'later_reduction_pp', 'failure_percent',
                'critic_mae_pp', 'critic_late_mae_pp', 'critic_late_bias_pp')}
            summary.update(training_seconds=sum(r['phase_seconds'] for r in train), update_seconds=sum(r['update_seconds'] for r in train),
                training_actions=sum(r['ppo']['selected'] for r in train), kl_stopped_batches=sum(r['ppo']['kl_stopped'] for r in train),
                mean_actor_epochs=statistics.mean(r['ppo']['actor_epochs'] for r in train),
                mean_critic_epochs=statistics.mean(r['ppo']['critic_epochs'] for r in train),
                actor_steps=sum(r['ppo']['actor_optimizer_steps'] for r in train), critic_steps=sum(r['ppo']['critic_optimizer_steps'] for r in train),
                critic_tail_seconds=sum(r['ppo']['critic_tail']['seconds'] for r in train))
            item['jobs'][seed] = dict(summary=summary, evaluations=evaluations,
                training=[dict(batch=r['phase']['batch'], actor_epochs=r['ppo']['actor_epochs'], critic_epochs=r['ppo']['critic_epochs'],
                    first_epoch_kl=r['ppo']['epochs'][0]['approximate_kl'], value_predictions=r['quality']['critic_prediction']) for r in train])
            updates[update_key][seed] = [r['ppo'] for r in train]
        cells.append(item)
        print(dict(audited=cell['name'], jobs={s:j['summary'] for s,j in item['jobs'].items()}), flush=True)
    if native_runtime is not None:
        native_runtime.close()
    (output/'best.json').unlink(missing_ok=True)
    check(len({tuple(c['factors'][k] for k in FIELDS) for c in cells}) == 8, 'complete unique factorial required')
    report = dict(schema='ppo-actor-critic-comparison-v1', cells=cells, effects=effects(cells),
                  schedule_prefixes=schedule_prefixes(updates),
                  source_sha256=sha(Path(__file__).read_bytes()),
                  matched_input_identities=len(geometries), native_final_layouts_verified=verified_layouts,
                  initial_actor_hashes={k:next(iter(v)) for k,v in initial_actors.items()},
                  initial_critic_hashes={k:next(iter(v)) for k,v in initial_critics.items()},
                  limitations=['Two initialization seeds; factorial contexts and actions are not independent replicates.',
                    'Reused held-out evaluation factories; no untouched final confirmation test.',
                    'Shared-model critic tails update the private head; independent tails update the whole critic.',
                    'Fresh starts isolate input distribution; earlier inherited-start curriculum is not tested.',
                    'Value errors follow each policy trajectory; changed visitation and return variance can also change MAE.',
                    'Later absolute gains can shrink when gains move earlier; they are not a standalone measure of late-action quality.',
                    'Predictive errors use stochastic realized returns, not known conditional expectations.'])
    inputs = (json.dumps(manifest, indent=2)+'\n').encode()
    report['input_manifest_sha256'] = sha(inputs)
    (output/'inputs.json').write_bytes(inputs)
    (output/'report.json').write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--validate-final-layouts', action='store_true')
    args = parser.parse_args()
    compare(args.matrix.resolve(), args.output.resolve(), args.validate_final_layouts)
