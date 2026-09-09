"""Shared held-out cohorts for candidate comparisons after each seed completes."""
from ppo_harness_state import require


def comparisons(value, protocol):
    """Use one cohort across every candidate and checkpoint for a given seed."""
    additions = {}
    for seed in protocol['config']['seeds']:
        jobs = [j for j in value['jobs'] if j['seed'] == seed]
        expected = set(protocol['config']['candidates'])
        if {j['candidate'] for j in jobs} != expected or any(j['status'] != 'complete' for j in jobs):
            continue
        evaluations = {j['id']: [r for r in j['records'] if r['phase']['kind'] == 'evaluation'] for j in jobs}
        cohorts = {}
        for job in jobs:
            require([r['phase']['batch'] for r in evaluations[job['id']]] == protocol['config']['evaluation']['at'],
                    'comparison-schedule', 'completed job lacks scheduled evaluations', 'comparison', job['id'])
            for record in evaluations[job['id']]:
                cohorts[(job['id'], record['phase']['batch'])] = {
                    r['identity']['generation_seed']: r for r in record['results'] if r['status'] == 'complete'}
        shared = sorted(set.intersection(*(set(c) for c in cohorts.values())))
        for generation_seed in shared:
            identities = {(rows[generation_seed]['generation']['request_sha256'],
                           rows[generation_seed]['generation']['initial_sha256']) for rows in cohorts.values()}
            require(len(identities) == 1, 'comparison-reset', 'shared evaluation factory inputs differ', 'comparison', generation_seed)
        for job in jobs:
            points = []
            baseline = cohorts[(job['id'], 0)]
            for record in evaluations[job['id']]:
                current = cohorts[(job['id'], record['phase']['batch'])]
                values, deltas = [], []
                for generation_seed in shared:
                    initial, final = baseline[generation_seed]['row'], current[generation_seed]['row']
                    area = initial['initial_metrics']['score'][0]
                    values.append(100 * (1 - final['best_metrics']['score'][0] / area))
                    deltas.append(100 * (initial['best_metrics']['score'][0] - final['best_metrics']['score'][0]) / area)
                points.append(dict(phase=dict(kind='comparison', batch=record['phase']['batch']),
                    selected_training_actions=record['selected_training_actions'], elapsed_seconds=record['elapsed_seconds'],
                    quality=dict(mean_area_reduction_percent=sum(values) / len(values) if values else None,
                                 accepted=len(shared), attempted=protocol['config']['evaluation']['attempts']),
                    paired_to_initial=dict(paired_delta_pp=sum(deltas) / len(deltas) if deltas else None, shared_seeds=shared),
                    comparison=dict(candidates=sorted(expected), cohort_scope='all candidates and all evaluation checkpoints for this seed',
                                    independent_admitted=record['quality']['accepted'])))
            additions[job['id']] = points
    return additions


def tracking_jobs(value, protocol):
    matched = comparisons(value, protocol)
    return [dict(job, records=job['records'] + matched.get(job['id'], [])) for job in value['jobs']]
