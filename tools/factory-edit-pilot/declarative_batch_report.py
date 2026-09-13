"""Equal-action paired comparison from compact saved records, without rollouts."""
from collections import Counter
from pathlib import Path
import statistics

from blueprint_ppo_store import checked, read, reference, sha, write
from blueprint_ppo_update import require
from declarative_learning_report import summarize


def mean(values):return statistics.mean(values) if values else None


def report(output):
    root=Path(output).resolve();experiment=read(root/'experiment.json');state=read(root/'state.json')
    require(state['status']=='complete','declarative.batch-report: registered matrix not complete')
    protocol=experiment['protocol'];jobs=[];geometry=set();by_seed={}
    for entry in experiment['jobs']:
        directory=Path(entry['directory']);launch=read(directory/'launch.json');seed=entry['seed'];batch=entry['batch']
        job=read(directory/f'seed-{seed}/state.json');config=launch['config'];owner=directory/f'seed-{seed}'
        require(job['status']=='complete' and job['update']*batch==protocol['actions_per_arm'],'declarative.batch-report: incomplete action endpoint')
        records=[read(checked(ref,owner)) for ref in job['records']]
        require(sum(r['actions'] for r in records)==protocol['actions_per_arm'] and len(job['inputs'])==protocol['actions_per_arm']//64,
            'declarative.batch-report: training action/input coverage')
        geometry.update(r['geometry_sha256'] for r in job['inputs'].values())
        by_seed.setdefault(seed,{})[batch]=job['inputs']
        jobs.append((entry,job,config,records,owner))
    panel=read(experiment['panel']['path']);eligible={c['geometry_identity'] for c in panel['cases'] if c['geometry_identity'] not in geometry}
    arms=[]
    for entry,job,config,records,owner in jobs:
        completed=[e for r in records for e in r['curriculum_completed']]
        waves={r['start_curriculum']['wave'] for r in job['inputs'].values()}
        plans=[read(checked(job['curriculum_plans'][str(w)],owner)) for w in sorted(waves)]
        requested=sum((Counter(p['requested']) for p in plans),Counter());actual=Counter(str(r['start_curriculum']['depth']) for r in job['inputs'].values())
        evaluations=[]
        require([r['update'] for r in job['evaluation']]==config['evaluation']['at'],'declarative.batch-report: evaluation action boundaries')
        for ref in job['evaluation']:
            evaluation=read(checked(ref,owner));rows=evaluation['rows']
            require(len(rows)==len(panel['cases'])*config['evaluation']['replicates'] and all(r['factory_validated'] for r in rows),
                'declarative.batch-report: complete validated evaluation cohort')
            evaluations.append(dict(actions=ref['update']*entry['batch'],update=ref['update'],seconds=evaluation['seconds'],
                by_stratum={k:summarize([r for r in rows if r['product'] in ps]) for k,ps in config['strata'].items()},
                disjoint={k:summarize([r for r in rows if r['product'] in ps],eligible) for k,ps in config['strata'].items()}))
        arms.append(dict(seed=entry['seed'],batch=entry['batch'],updates=job['update'],actions=protocol['actions_per_arm'],
            episodes=len(job['inputs']),seconds=job['seconds'],actions_per_second=protocol['actions_per_arm']/job['seconds'],evaluations=evaluations,
            curriculum=dict(requested=dict(requested),actual=dict(actual),fill_fraction={str(d):actual[str(d)]/requested[str(d)] if requested[str(d)] else None for d in (1,2)},
                by_depth={str(d):dict(episodes=sum(e['depth']==d for e in completed),improved=sum(e['depth']==d and e['best_area']<e['start_area'] for e in completed),
                    mean_further_reduction_percent=mean([e['start_area_reduction_percent'] for e in completed if e['depth']==d])) for d in (0,1,2)}),
            inputs=dict(fresh=actual['0'],inherited=actual['1']+actual['2'],products=dict(Counter(r['draw']['product'] for r in job['inputs'].values())),
                generation_attempts=sum(r['attempts'] for r in job['inputs'].values()),rejected_generation_attempts=sum(max(0,r['attempts']-1) for r in job['inputs'].values())),
            timings=dict(collection=sum(r['collection_seconds'] for r in records),optimizer=sum(r['ppo']['execution']['update_seconds'] for r in records),
                publication=sum(r['publication_seconds'] for r in records),input_access_worker_sum=sum(r['input_access_seconds'] for r in records),
                evaluation=sum(r['seconds'] for r in evaluations)),
            actor_epochs=dict(Counter(r['ppo']['actor_epochs'] for r in records)),final_buffer=records[-1]['buffer'],
            workers=job['workers'],concurrency=job['max_step_concurrency'],resource=job['resource'],cuda_peak=job['cuda_peak_allocated_bytes'],
            checkpoint=job['latest_checkpoint'],retained_bytes=job['retained_bytes'],wandb_url=job['wandb_url']))
    paired=[]
    for seed,inputs in by_seed.items():
        a,b=inputs[256],inputs[1024];common=[k for k in a.keys()&b.keys() if a[k]['start_curriculum']['depth']==b[k]['start_curriculum']['depth']==0]
        require(all(a[k]['input_sha256']==b[k]['input_sha256'] and a[k]['reference']==b[k]['reference'] for k in common),
            'declarative.batch-report: paired fresh actual input identity changed')
        paired.append(dict(seed=seed,common_actual_fresh_slots=len(common),same_actual_payloads=True))
    comparisons=[]
    for action in protocol['evaluation_actions']:
        for stratum in jobs[0][2]['strata']:
            per_seed=[]
            for seed in protocol['seeds']:
                selected={arm['batch']:next(e for e in arm['evaluations'] if e['actions']==action) for arm in arms if arm['seed']==seed}
                left=selected[256]['disjoint'][stratum];right=selected[1024]['disjoint'][stratum]
                require(left['area_reduction_by_case'].keys()==right['area_reduction_by_case'].keys(),'declarative.batch-report: common comparison factories')
                differences={k:right['area_reduction_by_case'][k]-v for k,v in left['area_reduction_by_case'].items()}
                per_seed.append(dict(seed=seed,batch_256=left['mean_area_reduction_percent'],batch_1024=right['mean_area_reduction_percent'],
                    difference_pp=mean(list(differences.values())),paired_factories=len(differences),by_factory_pp=differences))
            comparisons.append(dict(actions=action,stratum=stratum,seeds=per_seed,mean_difference_pp=mean([s['difference_pp'] for s in per_seed])))
    result=dict(schema='declarative-batch-report-v1',status='complete',seconds=state['seconds'],arms=arms,comparisons=comparisons,
        paired_inputs=paired,evaluation=dict(factories=len(panel['cases']),common_disjoint=len(eligible),
            overlapping_seeds=[c['generation_seed'] for c in panel['cases'] if c['geometry_identity'] in geometry]),
        interpretation='Two paired learner seeds; identical action budgets and optimizer settings. Different logical wave sizes can change realized curriculum. Counterbalanced arm order mitigates shared input-cache order effects; worker input-access sums overlap collection wall time. No convergence or isolated gradient-noise causal claim.',
        experiment=reference(root/'experiment.json',root),report_source_sha256=sha(__file__))
    write(root/'report.json',result);return result
