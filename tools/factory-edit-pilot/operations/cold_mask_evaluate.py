"""Fixed-endpoint 2×2 training/evaluation mask comparison on a new held-out panel."""
from copy import deepcopy
from pathlib import Path
import time

import torch
from cold_mask_arm import bound, endpoint, protocol, read, require, write, digest
from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import cpu_allocation
from declarative_learning_launch import evaluation_cases
from declarative_learning_policy import ActorCritic
from declarative_quality_analysis import aggregate, geometry_outcomes, paired
from band_mask_experiment import behavior


def evaluate(study, output, guard):
    f=protocol();panel=read(study['panels']['confirmation']['path']);output=Path(output)
    values={}
    for seed,trained in f['order']:
        value=endpoint(Path(study['directory'])/f'seed-{seed}-{trained}')
        values[seed,trained]=value
    config=deepcopy(values[71,'original']['launch']['config'])
    config['actions'].pop('masking',None)
    config['training']['streams']=config['execution_options']['workers']
    config['evaluation']['replicates']=f['action_replicates']
    allocation=cpu_allocation(config['training']['streams'])
    cases=evaluation_cases(panel,config);blocks=[]
    with BlueprintPool(config,values[71,'original']['launch']['source']['native_library'],allocation['workers'],
            output/'workers',guard,worker_entry=Path(__file__).with_name('band_mask_worker.py')) as workers:
        model=ActorCritic(config['ppo']['width'],config['ppo']['message_rounds']).eval()
        for seed,trained in f['order']:
            saved=values[seed,trained]['saved'];model.load_state_dict(saved['model'],strict=True)
            for mode in (f['final_evaluation_modes'] if seed==71 else list(reversed(f['final_evaluation_modes']))):
                guard();folder=output/f'seed-{seed}-train-{trained}-eval-{mode}'
                folder.mkdir(parents=True,exist_ok=True)
                if (folder/'report.json').exists():
                    block=read(folder/'report.json')
                    require(block['checkpoint']==bound(values[seed,trained]['owner']/values[seed,trained]['job']['latest_checkpoint']['path'])
                        and block['panel']==study['panels']['confirmation'], 'cold-mask.evaluation: changed saved block binding')
                else:
                    began=time.monotonic();cpu_before=sum(workers.worker_cpu_seconds)
                    rows=workers.evaluate(model,[dict(c,mask_variant=mode) for c in cases],seed,128,folder,guard.began+guard.seconds)
                    block=dict(seed=seed,trained=trained,mode=mode,rows=rows,seconds=time.monotonic()-began,
                        worker_cpu_seconds=sum(workers.worker_cpu_seconds)-cpu_before,
                        checkpoint=bound(values[seed,trained]['owner']/values[seed,trained]['job']['latest_checkpoint']['path']),
                        panel=study['panels']['confirmation'])
                    require(digest(model.state_dict())==saved['model_sha256'] and all(r['model_sha256']==saved['model_sha256']
                        for r in workers.exchange([dict(operation='model_digest') for _ in workers.workers])),
                        'cold-mask.evaluation: frozen weights changed')
                    write(folder/'report.json',block)
                blocks.append(block)
                print(dict(stage='confirmation',seed=seed,trained=trained,mode=mode,episodes=len(block['rows'])),flush=True)
        identities=workers.identities
    outcomes={};quality={};by_seed={}
    for trained in f['variants']:
        for mode in f['final_evaluation_modes']:
            key=f'{trained}/{mode}';outcomes[key]=[]
            for seed in f['initialization_seeds']:
                rows=next(b['rows'] for b in blocks if (b['seed'],b['trained'],b['mode'])==(seed,trained,mode))
                require(all(r['seed']==seed and r['update']==128 and r['policy_actions']==64 and
                    r['band_mask']['enabled']==(mode=='masked') for r in rows),'cold-mask.evaluation: changed endpoint/variant/budget')
                outcomes[key].append(geometry_outcomes(rows,panel,config['strata'],replicates=f['action_replicates']))
            quality[key]=aggregate(outcomes[key],config['strata'])
    comparisons={}
    pairs={'pipeline':('masked/masked','original/original'),
        'learning_with_same_mask':('masked/masked','original/masked'),
        'execution_mask_only':('original/masked','original/original')}
    for name,(left,right) in pairs.items():
        comparisons[name]=paired(quality[left],quality[right],config['strata'],seed=f['bootstrap_seed'],resamples=f['bootstrap_resamples'])
        by_seed[name]={str(seed):paired(aggregate([outcomes[left][i]],config['strata']),
            aggregate([outcomes[right][i]],config['strata']),config['strata'],seed=f['bootstrap_seed'],resamples=f['bootstrap_resamples'])
            for i,seed in enumerate(f['initialization_seeds'])}
    result=dict(schema='cold-mask-confirmation-v1',status='complete',quality=quality,comparisons=comparisons,by_seed=by_seed,
        behavior={key:behavior([r for b in blocks if f'{b["trained"]}/{b["mode"]}'==key for r in b['rows']]) for key in quality},
        blocks=[{k:v for k,v in b.items() if k!='rows'} for b in blocks],workers=identities)
    write(output/'result.json',result)
    return result
