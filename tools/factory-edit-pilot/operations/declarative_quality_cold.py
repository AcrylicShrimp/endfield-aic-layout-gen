"""Registered cold initialization, own-origin recovery and pure endpoint evaluation."""
from copy import deepcopy
import os
from pathlib import Path
import shutil
import time
from unittest.mock import patch

from declarative_quality_branch import BASE,ROOT,PROFILES,digest,read,require,sha,source_binding,write,profile_success
from declarative_quality_observer import Observer,fixed_bins
from blueprint_execution_resources import cpu_allocation,ResourceGuard
from blueprint_ppo_store import checked,load,reference,save
from declarative_learning_runtime import CHECKPOINT,Learner,account_write
from declarative_learning_launch import validate,pool,evaluation_cases
from declarative_learning_buffer import Buffer

FIXED=Path(__file__).with_name('declarative-quality-study.json')
SCHEMA='declarative-cold-quality-arm-v1'
FIELDS=('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','buffer')


def bound(path):return dict(path=str(Path(path).resolve()),sha256=sha(path))


def check_bound(ref):
    path=Path(ref['path']);require(sha(path)==ref['sha256'],f'quality-cold.binding: changed {path}');return path


def protocol():
    f=read(FIXED)
    require(f['schema']=='declarative-cold-quality-study-v2' and tuple(f['profiles'])==PROFILES and
        f['initialization_seeds']==[71,113] and f['max_updates']==1408 and f['max_seconds']==18000 and
        f['total_cold_updates']==1536 and f['previous_attempt']['executed_updates']==128 and
        f['wall_deadline_unix_seconds']==1789374130.8190546 and
        f['development_updates']==f['confirmation_updates']==128,'quality-cold.protocol: unregistered protocol')
    return f


def remaining_seconds(f,*,now=None,cap=None):
    seconds=min(f['max_seconds'],f['wall_deadline_unix_seconds']-(time.time() if now is None else now))
    if cap is not None:seconds=min(seconds,cap)
    require(seconds>0,'quality-cold.deadline: original absolute deadline expired')
    return seconds


def prior_cost(f):
    value=f['previous_attempt'];study=read(check_bound(value['study']));closeout=read(check_bound(value['closeout']))
    require(study['status']==closeout['status']=='incomplete' and
        study['executed_updates']==closeout['executed_updates']==value['executed_updates']==128 and
        study['began_unix']+21600==f['wall_deadline_unix_seconds'] and
        type(value['owned_bytes']) is int and 0<value['owned_bytes']<f['max_added_bytes'],
        'quality-cold.prior: discarded attempt accounting differs')
    return value['owned_bytes']


def reservation(panels):
    f=protocol();require(set(panels) in ({'readiness'},{'development','confirmation'}),
        'quality-cold.reservation: both scientific panels or the diagnostic panel required')
    hashes=[]
    for stage,ref in panels.items():
        p=read(check_bound(ref))
        require(p['schema']=='declarative-learning-panel-v1' and p['status']=='complete' and len(p['cases'])==32,
            'quality-cold.reservation: complete 32-factory panel required')
        require(all(c['action_seed']==f[stage+'_action_seed']+i*100 and
            f[stage+'_generation_seed']<=c['generation_seed']<f[stage+'_generation_seed']+
            ((f['max_generated_per_panel']+7)//8)*f['numerical_base']['training']['streams']*
            f['numerical_base']['training_input']['max_attempts'] for i,c in enumerate(p['cases'])),
            'quality-cold.reservation: registered stage identities required')
        hashes.extend(c['geometry_identity'] for c in p['cases'])
    require(len(set(hashes))==len(hashes),'quality-cold.reservation: duplicate panel geometries')
    result=dict(schema='blueprint-geometry-reservation-v1',geometry_sha256=sorted(hashes))
    from blueprint_training_input import reserved_geometries
    reserved_geometries(dict(geometry_reservation=result));return result


def config_for(seed,profile,stage,output,seconds,geometry_reservation=None):
    f=protocol();diagnostic=stage=='readiness'
    require((diagnostic and seed==71 and profile=='score-8') or
        (stage=='development' and seed==71 and profile in PROFILES) or
        (stage=='confirmation' and seed==113 and profile in PROFILES), 'quality-cold.family: unregistered variant')
    require(0<seconds<=(600 if diagnostic else f['max_seconds']),'quality-cold.family: wall allowance')
    c=deepcopy(f['numerical_base']);c['seeds']=[seed];c['success']=profile_success(c['success'],profile)
    c['training'].update(first_update=0,updates=4 if diagnostic else 128)
    c['training_input'].update(directory=str(Path(output).resolve()/'training-inputs'),
        seed_base=f['readiness_generation_seed'] if diagnostic else f['training_generation_seed'])
    if geometry_reservation is not None:c['training_input']['geometry_reservation']=deepcopy(geometry_reservation)
    c['panel'].update(seed_base=f[stage+'_generation_seed'],action_seed_base=f[stage+'_action_seed'])
    c['retention'].update(checkpoints=[0,1,2,4] if diagnostic else [0,128],training_traces=[],evaluation_traces=[])
    grace=min(300.,seconds/3);c['duration'].update(seconds=seconds-grace,finish_grace_seconds=grace)
    c['limits'].update(arm_seconds=seconds,matrix_seconds=seconds)
    validate(c);return c


def monitor(directory):
    directory=Path(directory).resolve();launch=read(directory/'launch.json')
    owner=directory/f'seed-{launch["config"]["seeds"][0]}'
    state,job=read(directory/'state.json'),read(owner/'state.json')
    require(state['status']==job['status']=='paused' and job['update']==1908,'quality-cold.main: expected paused main at 1908')
    return dict(directory=str(directory),files=[bound(p) for p in
        (directory/'launch.json',directory/'state.json',owner/'state.json',checked(job['latest_checkpoint'],owner))])


def source_check(source):
    for name,h in source['files_sha256'].items():require(sha(ROOT/name)==h,f'quality-cold.source: changed {name}')
    require(sha(source['native_library'])==source['native_sha256'],'quality-cold.source: changed native library')


def require_readiness(path,source,allocation):
    v=read(path);f=protocol()
    require(v['schema']=='declarative-cold-quality-readiness-v1' and v['status']=='passed' and
        v['protocol']==bound(FIXED) and v['numerical_base_sha256']==digest(f['numerical_base']) and
        v['source']==source and v['allocation']['workers']==allocation['workers'] and
        v['executed_updates']==5 and v['seconds']<=600 and v['continuity_closed'] and
        v['reservation_cost']['rejected_attempts']>0 and
        v['wall_deadline_unix_seconds']==f['wall_deadline_unix_seconds'],
        'quality-cold.readiness: stale or unregistered cold family receipt')
    check_bound(v['cuda_proof']);return v


def bank_contract(config,library):
    from declarative_learning_environment import Runtime
    runtime=Runtime(config,library);store=runtime.inputs.store
    return store.contract,store.spec['catalogs']


def new_bank(path,config,library):
    path=Path(path);require(not path.exists(),'quality-cold.inputs: new empty bank required')
    contract,catalogs=bank_contract(config,library)
    path.mkdir();write(path/'manifest.json',contract);write(path/'catalogs.json',catalogs)
    return bound(path/'manifest.json')


def zero_proof(saved,job):
    require(saved['update']==0 and not saved['actor_optimizer']['state'] and not saved['critic_optimizer']['state'] and
        digest(saved['buffer'])==digest(Buffer().snapshot()) and
        not job['records'] and not job['inputs'] and not job.get('curriculum_plans') and
        not job.get('evaluation') and not job.get('replay_sources') and saved['streams'] is None,
        'quality-cold.initial: nonempty learned state/history')
    return {k:digest(saved[k]) for k in FIELDS}


def prepare(output,*,seed,profile,stage,panel,library,shared,seconds,reservation_panels,readiness=None,selection=None):
    output=Path(output).resolve();require(not output.exists(),f'quality-cold.output: fresh directory required {output}')
    reserved=reservation(reservation_panels)
    require(reservation_panels.get(stage)==bound(panel),'quality-cold.reservation: active panel binding differs')
    config=config_for(seed,profile,stage,output,seconds,reserved);diagnostic=stage=='readiness'
    source=source_binding(config,library);allocation=cpu_allocation(config['execution_options']['workers'])
    if not diagnostic:require_readiness(readiness,source,allocation)
    if stage=='confirmation':
        selected=read(selection)
        require(selected['schema']=='declarative-cold-quality-selection-v1' and selected['protocol']==bound(FIXED) and
            profile in ['baseline-8',*selected['selected']] and selected['panels']['confirmation']==bound(panel),
            'quality-cold.confirmation: immutable selection required before initialization')
    p=read(panel);require(p['schema']=='declarative-learning-panel-v1' and p['status']=='complete',
        'quality-cold.panel: complete panel required')
    f=protocol()
    require(len(p['cases'])==32 and all(c['action_seed']==f[stage+'_action_seed']+i*100 and
        f[stage+'_generation_seed']<=c['generation_seed']<f[stage+'_generation_seed']+((f['max_generated_per_panel']+7)//8)*config['training']['streams']*config['training_input']['max_attempts']
        for i,c in enumerate(p['cases'])),
        'quality-cold.panel: registered stage panel required')
    contract,catalogs=bank_contract(config,library)
    require(read(Path(shared)/'manifest.json')==contract and read(Path(shared)/'catalogs.json')==catalogs,
        'quality-cold.inputs: bank contract differs; old banks cannot initialize cold arms')
    staging=output.with_name(output.name+'.staging');require(not staging.exists(),'quality-cold.output: stale staging directory')
    staging.mkdir(parents=True)
    try:
        from declarative_execution_migrate import relocate_inputs
        relocate_inputs(shared,staging/'training-inputs',config)
        launch=dict(schema='declarative-learning-launch-v1',config=config,source=source,allocation=allocation,
            panel=bound(panel),verification=bound(FIXED if diagnostic else readiness),batch_experiment=True,
            cold_origin=dict(directory=str(output),seed=seed,profile=profile,stage=stage,protocol=bound(FIXED),
                reservation_panels=deepcopy(reservation_panels)),
            created_unix_seconds=time.time())
        write(staging/'launch.json',launch);identity=sha(staging/'launch.json')
        learner=Learner(seed,config,identity);saved=learner.checkpoint(None)
        job=dict(schema='declarative-learning-state-v1',status='ready',update=0,seconds=0.,records=[],inputs={},evaluation=[])
        initial=zero_proof(saved,job);owner=staging/f'seed-{seed}';path=owner/'checkpoints/0000.pt.gz'
        save(path,saved);account_write(job,path)
        job['retained_files']={str(output/f'seed-{seed}'/Path(k).relative_to(owner)):v for k,v in job['retained_files'].items()}
        job['latest_checkpoint']=reference(path,owner);write(owner/'state.json',job)
        proof=dict(schema=SCHEMA,status='ready',profile=profile,replicate=1 if seed==71 else 2,seed=seed,
            stage=stage,origin=launch['cold_origin'],launch_sha256=identity,first_update=0,
            endpoint=config['training']['updates'],preserved=initial,initial_buffer_sha256=initial['buffer'],
            selection=bound(selection) if selection else None,quality_partition=fixed_bins(config['strata']),source_sha256=source['code_sha256'])
        write(staging/'quality-branch.json',proof)
        write(staging/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,launch_sha256=identity,jobs={}))
        staging.rename(output);return proof
    except BaseException:
        shutil.rmtree(staging);raise


def endpoint(directory,*,complete=True):
    directory=Path(directory).resolve();launch=read(directory/'launch.json');proof=read(directory/'quality-branch.json')
    require(proof['schema']==SCHEMA and proof['origin']==launch['cold_origin'] and
        proof['launch_sha256']==sha(directory/'launch.json') and proof['seed']==launch['config']['seeds'][0] and
        proof['profile']==launch['cold_origin']['profile'],'quality-cold.origin: foreign or warm checkpoint origin')
    source_check(launch['source']);owner=directory/f'seed-{proof["seed"]}'
    job=read(owner/'state.json');matrix=read(directory/'state.json');saved=load(checked(job['latest_checkpoint'],owner))
    require(saved['schema']==CHECKPOINT and saved['protocol_sha256']==proof['launch_sha256'] and
        saved['seed']==proof['seed'] and saved['update']==job['update'] and saved['model_sha256']==digest(saved['model']),
        'quality-cold.checkpoint: foreign state or corrupted checkpoint')
    if complete:require(matrix['status']==job['status']=='complete' and job['update']==proof['endpoint'],
        'quality-cold.endpoint: unfinished registered arm')
    return dict(directory=directory,owner=owner,launch=launch,study=proof,job=job,matrix=matrix,saved=saved)


def snapshot_live(directory,owner,job):
    """One bounded diagnostic recovery copy; preserve the same launch/origin."""
    destination=directory.parent/'restored';require(not destination.exists(),'quality-cold.recovery: fresh diagnostic destination required')
    destination.mkdir();new_owner=destination/owner.name;new_owner.mkdir()
    for name in ('launch.json','quality-branch.json'):shutil.copyfile(directory/name,destination/name)
    copied=deepcopy(job);copied.update(status='ready',seconds=0.,wandb_id=None,wandb_url=None)
    copied['retained_files']={}
    for name in job['retained_files']:
        old=Path(name);new=new_owner/old.relative_to(owner);new.parent.mkdir(parents=True,exist_ok=True)
        os.link(old,new);copied['retained_files'][str(new)]=old.stat().st_size
    copied['retained_bytes']=sum(copied['retained_files'].values())
    write(new_owner/'state.json',copied)
    write(destination/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,
        launch_sha256=sha(directory/'launch.json'),jobs={}))
    write(destination/'diagnostic-recovery.json',dict(origin=read(directory/'quality-branch.json')['origin'],
        initial_checkpoint=bound(checked(job['latest_checkpoint'],owner)),saved_state_sha256=digest(load(checked(job['latest_checkpoint'],owner))),
        first_update=1,endpoint=2))


def run_observed(directory,environment):
    import declarative_learning_run as runner
    from declarative_duration_control import requested_stop
    directory=Path(directory).resolve();value=endpoint(directory,complete=False);proof=value['study'];launch=value['launch']
    recovery=read(directory/'diagnostic-recovery.json') if (directory/'diagnostic-recovery.json').exists() else None
    if recovery:
        require(proof['stage']=='readiness' and recovery['origin']==proof['origin'] and value['saved']['update']==1 and
            digest(value['saved'])==recovery['saved_state_sha256'],'quality-cold.recovery: foreign diagnostic restore')
    else:
        require(str(directory)==proof['origin']['directory'],'quality-cold.recovery: wrong arm directory')
        if value['saved']['update']==0:
            initial={k:digest(value['saved'][k]) for k in FIELDS}
            require(initial==proof['preserved'],'quality-cold.initial: changed zero initializer')
    if proof['stage']=='confirmation':check_bound(proof['selection'])
    if proof['stage']!='readiness':require_readiness(launch['verification']['path'],launch['source'],launch['allocation'])
    original_retain=runner.retain
    class ObservedLearner(Learner):
        def __init__(self,*args,**kwargs):
            super().__init__(*args,**kwargs);self.quality=Observer(self,proof['quality_partition'])
        def update(self,payload,evidence):
            self.quality.begin();record=super().update(payload,evidence)
            record['quality_allocation']=self.quality.finish(record);return record
    def retain(learner,owner,job):
        original_retain(learner,owner,job)
        if proof['stage']=='readiness' and not recovery and learner.ordinal==1:snapshot_live(directory,owner,job)
    with requested_stop() as stop,patch.object(runner,'Learner',ObservedLearner),patch.object(runner,'retain',retain):
        result=runner.run(directory,environment,lambda:stop() or bool(recovery))
    if recovery:
        require(result['status']=='paused' and read(value['owner']/'state.json')['update']==2,
            'quality-cold.recovery: expected exactly one valid followup')
        return dict(result,status='complete')
    return result


def pure_evaluate(branch,panel,selection,output,seconds):
    import torch
    import declarative_learning_run as runner
    value=endpoint(branch);proof=value['study'];selection_value=read(selection)
    require(proof['seed']==71 and proof['endpoint']==128 and proof['profile'] in ['baseline-8',*selection_value['selected']] and
        selection_value['schema']=='declarative-cold-quality-selection-v1' and selection_value['protocol']==bound(FIXED) and
        selection_value['panels']['confirmation']==bound(panel), 'quality-cold.confirmation: immutable selection required')
    bindings=[read(check_bound(r)) for r in selection_value['development']]
    require(any(r['profile']==proof['profile'] and r['checkpoint']==bound(checked(value['job']['latest_checkpoint'],value['owner']))
        for r in bindings),'quality-cold.confirmation: endpoint differs from selected development')
    output=Path(output).resolve();require(not output.exists(),'quality-cold.evaluation: fresh output required');output.mkdir()
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    config=value['launch']['config'];learner=Learner(proof['seed'],config,proof['launch_sha256'],value['saved'])
    before=digest(learner.checkpoint(learner.streams));checkpoint=bound(checked(value['job']['latest_checkpoint'],value['owner']))
    guard=ResourceGuard(output,seconds,config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes']);job={}
    with pool(config,value['launch']['source']['native_library'],value['launch']['allocation'],output/'workers',guard) as workers:
        result=runner.evaluate(learner,workers,evaluation_cases(read(panel),config),output,job,guard)
    require(before==digest(learner.checkpoint(learner.streams)) and checkpoint==bound(check_bound(checkpoint)),
        'quality-cold.evaluation: endpoint model/Adam/RNG/buffer/ordinal changed')
    result=dict(schema='declarative-cold-quality-evaluation-v1',status='complete',seed=71,profile=proof['profile'],
        update=128,training_updates=0,checkpoint=checkpoint,panel=bound(panel),selection=bound(selection),
        state_sha256=before,evaluation=bound(output/'evaluation/0128/report.json'),seconds=time.monotonic()-guard.began)
    write(output/'pure-evaluation.json',result);return result
