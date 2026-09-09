"""Summarize committed PPO results with common held-out cohorts and explicit coverage."""
import argparse
import json
from pathlib import Path

from ppo_harness_comparison import comparisons
from ppo_harness_monitor import snapshot
from ppo_harness_state import read, write, sha, require


def summarize(root):
    protocol = read(root / 'protocol.json')
    value = snapshot(root)
    matched = comparisons(value, protocol)
    jobs = []
    for job in value['jobs']:
        training = [r for r in job['records'] if r['phase']['kind'] == 'training']
        evaluation = [r for r in job['records'] if r['phase']['kind'] == 'evaluation']
        shared = matched.get(job['id'], [])
        points = [dict(batch=r['phase']['batch'], actions=r['selected_training_actions'],
                       elapsed_seconds=r['elapsed_seconds'], mean_area_reduction_percent=r['quality']['mean_area_reduction_percent'],
                       delta_from_initial_pp=r['paired_to_initial']['paired_delta_pp'],
                       shared_generation_seeds=r['paired_to_initial']['shared_seeds'],
                       shared_cases=r['quality']['accepted'], attempted=r['quality']['attempted']) for r in shared]
        all_training = [result for r in training for result in r['results']]
        admitted = [r for r in all_training if r['status'] == 'complete']
        job_summary = dict(id=job['id'], candidate=job['candidate'], seed=job['seed'], status=job['status'],
            completed_batches=job['completed_batches'], training_actions=job['selected_training_actions'],
            elapsed_seconds=job['elapsed_seconds'], shared_points=points,
            independent_evaluations=[dict(batch=r['phase']['batch'], quality=r['quality'], paired=r['paired_to_initial']) for r in evaluation],
            training=dict(attempted=len(all_training), admitted=len(admitted), generation_rejected=len(all_training)-len(admitted),
                unique_generation_seeds=len({r['identity']['generation_seed'] for r in all_training}),
                unique_requests=len({r['generation']['request_sha256'] for r in admitted}),
                unique_geometry=len({r['generation']['geometry_sha256'] for r in admitted}),
                distinct_rates=len({(r['generation']['draw']['product'], r['generation']['draw']['rate']['duration_ms']) for r in admitted}),
                collection_seconds=sum(r['collection']['seconds'] for r in training),
                update_seconds=sum(r['update_seconds'] for r in training),
                checkpoint_seconds=sum(r['checkpoint_seconds'] for r in training),
                optimizer_steps=sum(r['ppo']['optimizer_steps'] for r in training),
                kl_stopped_batches=sum(r['ppo']['kl_stopped'] for r in training)),
            final_ppo=training[-1]['ppo']['epochs'][-1] if training else None)
        if points:
            last, first = evaluation[-1], evaluation[0]
            initial = {r['identity']['generation_seed']: r for r in first['results'] if r['status'] == 'complete'}
            final = {r['identity']['generation_seed']: r for r in last['results'] if r['status'] == 'complete'}
            deltas = [100 * (initial[s]['row']['best_metrics']['score'][0] - final[s]['row']['best_metrics']['score'][0]) /
                      initial[s]['row']['initial_metrics']['score'][0] for s in points[-1]['shared_generation_seeds']]
            job_summary['paired_final'] = dict(wins=sum(d > 0 for d in deltas), ties=sum(d == 0 for d in deltas),
                                              losses=sum(d < 0 for d in deltas))
        jobs.append(job_summary)
    candidates = []
    for candidate in protocol['config']['candidates']:
        selected = [j for j in jobs if j['candidate'] == candidate]
        comparable = len(selected) == len(protocol['config']['seeds']) and all(j['shared_points'] and j['shared_points'][-1]['shared_cases'] for j in selected)
        candidates.append(dict(candidate=candidate, comparable=bool(comparable),
            completed_seeds=[j['seed'] for j in selected if j['status'] == 'complete'],
            final_mean_area_reduction_percent=sum(j['shared_points'][-1]['mean_area_reduction_percent'] for j in selected)/len(selected) if comparable else None,
            mean_delta_from_initial_pp=sum(j['shared_points'][-1]['delta_from_initial_pp'] for j in selected)/len(selected) if comparable else None))
    if value['status'] == 'complete':
        require({j['id'] for j in jobs} == {j['id'] for j in protocol['jobs']}, 'summary-jobs', 'complete state lacks scheduled jobs')
    return dict(schema='ppo-harness-summary-v1', status=value['status'], protocol_sha256=value['protocol_sha256'],
                summary_source_sha256=sha(__file__), candidates=candidates, jobs=jobs,
                interpretation='Equal weight per initialization seed; one common admitted evaluation cohort across all candidates and checkpoints within each seed. A shallow two-seed screen does not establish a reliable winner.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = summarize(args.run.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write(args.output, result)
    print(json.dumps(dict(status=result['status'], output=str(args.output.resolve()), candidates=result['candidates']), allow_nan=False))
