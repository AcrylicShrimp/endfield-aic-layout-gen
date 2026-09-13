"""Normal optimized training: no profiler, proof replay or automatic artifact audit."""
from pathlib import Path
import os
import time

import torch

from blueprint_duration_metrics import define_metrics, metrics
from blueprint_execution_resources import ResourceGuard
from blueprint_ppo_store import checked, load, read, reference, save, sha, write
from blueprint_ppo_update import digest, require
from blueprint_success_buffer import Evidence
from declarative_learning_launch import evaluation_cases, pool
from declarative_learning_runtime import Learner, account_remove, account_write, publish, retain
from ppo_harness_environment import load_environment, tracking_destination


def evaluate(learner, workers, cases, directory, job, guard, *, uniform=False):
    ordinal=learner.ordinal
    folder='random' if uniform else 'evaluation'
    output=directory/f'{folder}/{ordinal:04d}'
    began=time.monotonic()
    rows=workers.evaluate(learner.behavior_copy(),cases,learner.seed,ordinal,output,guard.began+guard.seconds,uniform=uniform)
    record=dict(schema='declarative-learning-evaluation-v1',update=ordinal,rows=rows,seconds=time.monotonic()-began,
        raw_retained=ordinal in learner.config['retention']['evaluation_traces'])
    # The exact finite list comes from the evaluation cases, not directory scans.
    for case in cases:
        path=output/f'factory-{case["generation_seed"]}-{case["sample"]}.pt.gz'
        account_write(job,path)
        if not record['raw_retained']:
            account_remove(job,path)
    path=output/'report.json';write(path,record);account_write(job,path)
    job.setdefault(folder,[]).append(dict(update=ordinal,**reference(path,directory)))
    write(directory/'state.json',job)
    return record


def register_inputs(payload, job):
    inputs=job.setdefault('inputs',{})
    for before in payload['native_before']:
        if before['policy_actions']!=0:
            continue
        value=before['metadata']['training_input']
        key=str(value['identity']['slot'])
        record=dict(reference=value['reference'],input_sha256=value['input_sha256'],request_sha256=value['request_sha256'],
            geometry_sha256=value['geometry_sha256'],blueprint_sha256=value['blueprint_sha256'],
            generation_seed=value['generation_seed'],draw=value['draw'],attempts=len(value['attempts']),
            facilities=len(before['native']['original']['placements']),
            input_directory=value.get('input_directory'),start_curriculum=value.get('start_curriculum'))
        require(key not in inputs or inputs[key]==record, 'declarative changed actual training input slot')
        inputs[key]=record


def run_arm(launch, matrix, seed, state, matrix_guard):
    import wandb
    config=launch['config']; directory=matrix/f'seed-{seed}'
    directory.mkdir(exist_ok=True)
    identity=state['launch_sha256']
    if (directory/'state.json').exists():
        job=read(directory/'state.json')
        if job['status']=='complete':return job
        saved=load(checked(job['latest_checkpoint'],directory))
        learner=Learner(seed,config,identity,saved)
    else:
        learner=Learner(seed,config,identity)
        job=dict(schema='declarative-learning-state-v1',status='running',update=0,seconds=0.,records=[],inputs={})
        initial=directory/'checkpoints/0000.pt.gz';save(initial,learner.checkpoint(None));account_write(job,initial)
        job['latest_checkpoint']=reference(initial,directory);write(directory/'state.json',job)
    previous=job['seconds']
    require(previous<config['limits']['arm_seconds'], 'declarative arm already exhausted its wall allowance')
    resource=ResourceGuard(directory,config['limits']['arm_seconds']-previous,
        config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    def guard():
        matrix_guard();resource()
    tracking=wandb.init(**tracking_destination(required=True),mode=config['tracking']['mode'],
        name=f'declarative-seed-{seed}',group=config['tracking']['group'],id=job.get('wandb_id'),resume='allow',dir=str(directory),
        config=dict(seed=seed,launch_sha256=identity,config=config,device=config['device'],workers=8))
    job.update(wandb_id=tracking.id,wandb_url=tracking.url)
    define_metrics(tracking)
    evidence=Evidence(directory,job)
    allocation=launch['allocation'];workers=None
    cases=evaluation_cases(read(launch['panel']['path']),config)
    try:
        workers=pool(config,launch['source']['native_library'],allocation,directory/'workers',guard,saved=learner.streams)
        os.sched_setaffinity(0,allocation['coordinator'])
        job['workers']=workers.identities
        if learner.ordinal==0 and config.get('start_curriculum') and learner.streams is None:
            from declarative_learning_curriculum import initial_plan
            plan,ref=initial_plan(config,seed,directory)
            workers.assign_starts(plan['slots'])
            job.setdefault('curriculum_plans',{})['1']=ref
            account_write(job,directory/ref['path'])
            initial=directory/'checkpoints/0000.pt.gz'
            save(initial,learner.checkpoint(workers.snapshot()));account_write(job,initial)
            job['latest_checkpoint']=reference(initial,directory)
            write(directory/'state.json',job)
        while True:
            guard();ordinal=learner.ordinal
            if ordinal in config['evaluation']['at'] and not any(row['update']==ordinal for row in job.get('evaluation',[])):
                measured=evaluate(learner,workers,cases,directory,job,resource)
                baseline=read(checked(job['evaluation'][0],directory))['rows']
                tracking.log(metrics(measured,ordinal,config['training']['streams']*config['training']['chunk_actions'],
                    seconds=previous+time.monotonic()-resource.began,initial=baseline))
            if ordinal==0 and seed==config['seeds'][0] and not job.get('random') and not launch.get('batch_experiment'):
                evaluate(learner,workers,cases,directory,job,resource,uniform=True)
            if ordinal==config['training']['updates']:
                require(all(s['native'] is None for s in workers.snapshot()), 'declarative final update has unfinished episodes')
                break
            payload=workers.collect(learner.behavior_copy(),learner.sampler)
            register_inputs(payload,job)
            record=learner.update(payload,evidence)
            snapshots=workers.snapshot()
            record.update(input_access_seconds=sum(s['metadata']['input_access_seconds'] for s in payload['native_before'] if s['policy_actions']==0),
                worker_cpu_seconds=workers.worker_cpu_seconds[:],max_step_concurrency=workers.max_step_concurrency,
                worker_phase_seconds=dict(workers.phase_seconds),native_seconds=sum(s['outcome']['seconds'] for s in payload['samples']),
                mean_root_candidates=sum(len(s['input']['actions']) for s in payload['samples'])/len(payload['samples']),
                mean_selected_ports=sum(len(s['decision']['ports']) for s in payload['samples'])/len(payload['samples']))
            publish(learner,payload,record,snapshots,directory,job,workers=workers)
            job['seconds']=previous+time.monotonic()-resource.began
            retain(learner,directory,job)
            tracking.log(metrics(record,learner.ordinal,len(payload['samples']),seconds=job['seconds']))
            require(job['retained_bytes']<config['limits']['arm_output_bytes'], 'declarative retained arm output limit')
            print(dict(stage='train',seed=seed,update=learner.ordinal,reward=record['area_reward'],
                eligible=record['buffer']['eligible'],successes=record['buffer']['own_entries'],
                actor_epochs=record['ppo']['actor_epochs'],seconds=round(job['seconds'],3)),flush=True)
        job.update(status='complete',seconds=previous+time.monotonic()-resource.began,resource=resource.last,
            max_step_concurrency=workers.max_step_concurrency,cuda_peak_allocated_bytes=torch.cuda.max_memory_allocated())
        write(directory/'state.json',job);tracking.finish()
        return job
    except BaseException as error:
        job.update(status='failed',seconds=previous+time.monotonic()-resource.began,
            diagnostic=dict(code='declarative-learning.failed',severity='error',stage='training',entity=str(directory),message=str(error)))
        write(directory/'state.json',job);tracking.finish(exit_code=1)
        raise
    finally:
        if workers:workers.close()
        os.sched_setaffinity(0,allocation['allowed'])


def run(output, environment):
    matrix=Path(output).resolve()
    launch,state=read(matrix/'launch.json'),read(matrix/'state.json')
    require(state['launch_sha256']==sha(matrix/'launch.json'), 'declarative changed launch identity')
    require(state['status']!='complete', 'declarative matrix is already complete')
    # Authenticate once at the command boundary, never per update or epoch.
    for path,expected in launch['source']['files_sha256'].items():
        from blueprint_ppo_store import ROOT
        require(sha(ROOT/path)==expected, f'declarative launch source changed: {path}')
    require(sha(launch['source']['native_library'])==launch['source']['native_sha256'], 'declarative native changed')
    require(sha(launch['panel']['path'])==launch['panel']['sha256'], 'declarative evaluation panel changed')
    require(sha(launch['verification']['path'])==launch['verification']['sha256'], 'declarative verification receipt changed')
    load_environment(environment);tracking_destination(required=True)
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    require(torch.cuda.is_available(), 'declarative registered CUDA learner unavailable')
    config=launch['config']
    guard=ResourceGuard(matrix,config['limits']['matrix_seconds']-state['seconds'],
        config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    previous=state['seconds'];state['status']='running';write(matrix/'state.json',state)
    try:
        for seed in config['seeds']:
            if state['jobs'].get(str(seed),{}).get('status')=='complete':continue
            job=run_arm(launch,matrix,seed,state,guard)
            state['jobs'][str(seed)]=dict(status=job['status'],update=job['update'],seconds=job['seconds'],
                state=reference(matrix/f'seed-{seed}/state.json',matrix),wandb_url=job['wandb_url'])
            state['seconds']=previous+time.monotonic()-guard.began;write(matrix/'state.json',state)
        state['status']='complete'
    except BaseException as error:
        state.update(status='failed',diagnostic=dict(code='declarative-learning.matrix-failed',severity='error',
            stage='training',entity=str(matrix),message=str(error)))
        raise
    finally:
        state['seconds']=previous+time.monotonic()-guard.began;write(matrix/'state.json',state)
    return state
