"""Summarize retained compact records; no extra rollouts or source-episode replay."""
from collections import Counter
from pathlib import Path
import statistics

from blueprint_ppo_store import checked, read, reference, sha, write
from blueprint_ppo_update import require


def mean(rows):
    return statistics.mean(rows) if rows else None


def summarize(rows, eligible=None):
    selected=[row for row in rows if eligible is None or row['geometry_identity'] in eligible]
    cases={}
    for row in selected:
        cases.setdefault(row['generation_seed'],[]).append(row)
    return dict(factories=len(cases),episodes=len(selected),
        improved_episodes=sum(r['best_score'][0]<r['start_score'][0] for r in selected),
        improved_factories=sum(any(r['best_score'][0]<r['start_score'][0] for r in values) for values in cases.values()),
        mean_area_reduction_percent=mean([mean([r['total_area_reduction_percent'] for r in values]) for values in cases.values()]),
        area_reduction_by_case={str(k):mean([r['total_area_reduction_percent'] for r in values]) for k,values in cases.items()},
        actions=sum(r['policy_actions'] for r in selected),guard_counts=dict(Counter(r['guard'] for r in selected)),
        inference_seconds=sum(r['inference_seconds'] for r in selected))


def report(output):
    root=Path(output).resolve();launch,state=read(root/'launch.json'),read(root/'state.json')
    require(state['status']=='complete', 'declarative report requires completed registered training')
    config=launch['config'];expected=config['training']['updates'];first_update=config['training'].get('first_update',0)
    updates=expected-first_update
    jobs={seed:read(root/f'seed-{seed}/state.json') for seed in config['seeds']}
    baseline_inputs=jobs[config['seeds'][0]]['inputs']
    paired_inputs=len(jobs)>1 and all(job['inputs']==baseline_inputs for job in jobs.values())
    all_inputs=[row for job in jobs.values() for row in job['inputs'].values()]
    total_slots=expected*config['training']['chunk_actions']*config['training']['streams']//config['training']['episode_actions']
    require(all(len(job['inputs'])==total_slots for job in jobs.values()), 'declarative completed input coverage')
    geometry={row['geometry_sha256'] for row in all_inputs}
    panel=read(launch['panel']['path'])
    eligible={case['geometry_identity'] for case in panel['cases'] if case['geometry_identity'] not in geometry}
    seeds=[]
    for seed,job in jobs.items():
        directory=root/f'seed-{seed}'
        require(job['status']=='complete' and job['update']==expected and len(job['records'])==expected,
            'declarative endpoint/update coverage')
        records=[read(checked(ref,directory)) for ref in job['records'][first_update:]]
        require(sum(r['actions'] for r in records)==updates*config['training']['streams']*config['training']['chunk_actions'],
            'declarative registered action coverage')
        evaluations=[]
        require([r['update'] for r in job['evaluation']]==config['evaluation']['at'], 'declarative evaluation checkpoint coverage')
        for ref in job['evaluation']:
            evaluation=read(checked(ref,directory))
            require(len(evaluation['rows'])==len(panel['cases'])*config['evaluation']['replicates'], 'declarative complete evaluation cohort')
            require(all(r['factory_validated'] for r in evaluation['rows']), 'declarative invalid evaluation export')
            strata={name:summarize([r for r in evaluation['rows'] if r['product'] in products])
                    for name,products in config['strata'].items()}
            eligible_strata={name:summarize([r for r in evaluation['rows'] if r['product'] in products],eligible)
                    for name,products in config['strata'].items()}
            evaluations.append(dict(update=ref['update'],reused=bool(launch.get('continuation') and ref['update']==first_update),all=summarize(evaluation['rows']),by_stratum=strata,
                geometry_disjoint=summarize(evaluation['rows'],eligible),geometry_disjoint_by_stratum=eligible_strata,
                seconds=evaluation['seconds'],report=ref))
        quarter=max(1,updates//4)
        blocks=[]
        for start in range(0,updates,quarter):
            selected=records[start:start+quarter]
            completed=[r['quality'] for r in selected if r['quality'].get('accepted')]
            blocks.append(dict(first_update=first_update+start+1,last_update=first_update+min(start+quarter,updates),
                mean_completed_area_reduction_percent=mean([q['mean_area_reduction_percent'] for q in completed]),
                positive_reward_updates=sum(r['area_reward']>0 for r in selected),
                area_reward=sum(r['area_reward'] for r in selected)))
        completed=[r for record in records for r in record.get('curriculum_completed',[])]
        curriculum={}
        if config.get('start_curriculum'):
            selected_inputs=[r for k,r in job['inputs'].items() if int(k)>=first_update*config['training']['chunk_actions']*config['training']['streams']//config['training']['episode_actions']]
            waves={r['start_curriculum']['wave'] for r in selected_inputs}
            plans=[read(checked(job['curriculum_plans'][str(w)],directory)) for w in sorted(waves)]
            requested=sum((Counter(p['requested']) for p in plans),Counter())
            actual=Counter(str(r['start_curriculum']['depth']) for r in selected_inputs)
            curriculum=dict(requested=dict(requested),actual=dict(actual),
                fill_fraction={str(d):actual[str(d)]/requested[str(d)] if requested[str(d)] else None for d in (1,2)},
                fallback_slots=sum(r['start_curriculum']['requested_depth']!=r['start_curriculum']['depth'] for r in selected_inputs),
                by_depth={str(d):dict(episodes=sum(r['depth']==d for r in completed),
                    improved=sum(r['depth']==d and r['best_area']<r['start_area'] for r in completed),
                    mean_start_area_reduction_percent=mean([r['start_area_reduction_percent'] for r in completed if r['depth']==d]),
                    mean_root_area_reduction_percent=mean([r['root_area_reduction_percent'] for r in completed if r['depth']==d])) for d in (0,1,2)},
                plans=[job['curriculum_plans'][str(w)] for w in sorted(waves)])
        seeds.append(dict(seed=seed,first_update=first_update,curriculum=curriculum,seconds=job['seconds'],evaluations=evaluations,training_blocks=blocks,
            actions=sum(r['actions'] for r in records),completed_episodes=sum(r['quality'].get('accepted',0) for r in records),
            positive_reward_updates=sum(r['area_reward']>0 for r in records),final_buffer=records[-1]['buffer'],
            replay=dict(draws_by_stratum=dict(sum((Counter(r['ppo']['success']['draws_by_stratum']) for r in records),Counter())),
                positive_labels=sum(r['ppo']['success']['positive_labels'] for r in records),
                labels=sum(r['ppo']['success']['label_count'] for r in records)),
            timings=dict(collection=sum(r['collection_seconds'] for r in records),
                learner=sum(r['ppo']['execution']['update_seconds'] for r in records),
                prepare_worker=sum(r['ppo']['execution']['prepare_worker_seconds'] for r in records),
                publication=sum(r['publication_seconds'] for r in records),evaluation=sum(r['seconds'] for r in evaluations if not r['reused'])),
            actor_epochs=dict(Counter(r['ppo']['actor_epochs'] for r in records)),
            max_step_concurrency=job['max_step_concurrency'],worker_count=len(job['workers']),
            cuda_peak_allocated_bytes=job['cuda_peak_allocated_bytes'],resource=job['resource'],
            checkpoint=job['latest_checkpoint'],wandb_url=job['wandb_url']))
    first=config['seeds'][0]
    random_ref=jobs[first]['random'][0]
    random_rows=read(checked(random_ref,root/f'seed-{first}'))['rows']
    result=dict(schema='declarative-learning-report-v1',status='complete',seconds=state['seconds'],seeds=seeds,
        trained=True,random_baseline=dict(all=summarize(random_rows),
            by_stratum={name:summarize([r for r in random_rows if r['product'] in products]) for name,products in config['strata'].items()},
            geometry_disjoint=summarize(random_rows,eligible)),
        input_coverage=dict(actual_slots_per_seed=total_slots,episode_executions=len(all_inputs),paired_across_seeds=paired_inputs,
            parent_slots=first_update*config['training']['chunk_actions']*config['training']['streams']//config['training']['episode_actions'],
            new_slots=updates*config['training']['chunk_actions']*config['training']['streams']//config['training']['episode_actions'],
            products=dict(Counter(r['draw']['product'] for r in all_inputs)),
            unique_requests=len({r['request_sha256'] for r in all_inputs}),unique_geometries=len(geometry),
            generation_attempts=sum(r['attempts'] for r in all_inputs),
            rejected_generation_attempts=sum(max(0,r['attempts']-1) for r in all_inputs),
            facility_counts=dict(Counter(r['facilities'] for r in all_inputs))),
        evaluation=dict(factories=len(panel['cases']),geometry_disjoint=len(eligible),
            overlapping_seeds=[c['generation_seed'] for c in panel['cases'] if c['geometry_identity'] in geometry]),
        interpretation='The continuation retains its parent learner, optimizers, RNG and replay. New-action totals exclude the parent prefix. Inherited starts repeat requests deliberately. This single-seed short run is not convergence or curriculum-effect evidence.',
        launch=reference(root/'launch.json',root),verification=launch['verification'],
        report_source_sha256=sha(__file__),
        timing_scope='Reused parent evaluation is excluded from continuation execution timing; phase timers do not include all cache hydration, bookkeeping and process startup costs.')
    write(root/'report.json',result)
    return result
