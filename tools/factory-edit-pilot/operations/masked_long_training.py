"""Prepare and run cold masked training through the established normal runner."""
import argparse
from copy import deepcopy
from pathlib import Path
import sys
import time

BASE=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(BASE),str(BASE/'diagnostics')]
from blueprint_duration_store import exclusive_run
from blueprint_execution_resources import cpu_allocation
from blueprint_ppo_store import ROOT,checked,load,read,reference,save,sha,write
from blueprint_ppo_update import digest,require
from declarative_duration_control import requested_stop
from declarative_execution_migrate import relocate_inputs
from declarative_learning_launch import sources,validate
from declarative_learning_runtime import Learner,account_write
from declarative_quality_cold import zero_proof

FIXED=Path(__file__).with_name('masked-long-training.json')


def bound(path):return dict(path=str(Path(path).resolve()),sha256=sha(path))


def configuration(reservation, *, probe=False):
    f=read(FIXED);c=deepcopy(read(ROOT/f['numerical_base'])['numerical_base'])
    from declarative_band_mask import MODE
    c.update(purpose='masked-long-training-readiness' if probe else 'masked-long-training',seeds=[f['initialization_seed']])
    c['actions']['masking']=MODE
    c['training'].update(first_update=0,updates=f['probe_updates'] if probe else 1_000_000)
    c['training_input'].update(directory='@run',seed_base=f['training_generation_seed'],geometry_reservation=reservation)
    c['evaluation'].update(at=[0],interval_updates=f['evaluation_interval_updates'])
    c['execution_options']['workers']=f['workers']
    c['retention'].update(checkpoints=[0,1,2,f['probe_updates']] if probe else [0],training_traces=[],evaluation_traces=[])
    seconds=600 if probe else f['duration_seconds'];grace=300 if probe else f['finish_grace_seconds']
    c['duration'].update(seconds=seconds,finish_grace_seconds=grace)
    c['limits'].update(arm_seconds=seconds+grace,matrix_seconds=seconds+grace+120,
        arm_output_bytes=f['retained_output_bytes'],matrix_rss_bytes=f['max_rss_bytes'],min_free_bytes=f['min_free_bytes'])
    c['tracking'].update(mode='disabled' if probe else 'online',group='masked-long-training-20260914')
    validate(c);return c


def source_binding(config, library):
    source=sources(config,library)
    for path in (FIXED,Path(__file__).resolve(),ROOT/read(FIXED)['numerical_base']):
        source['files_sha256'][str(path.relative_to(ROOT))]=sha(path)
    source['code_sha256']=digest(source['files_sha256']);return source


def prepare(output, panel, reservation, library, *, probe=False, verification=None, shared=None, probe_config=None):
    output=output.resolve();require(not output.exists(),f'masked-long.output: fresh output required: {output}')
    require(probe or probe_config is None,'masked-long.config: custom configuration is restricted to probes')
    config=configuration(reservation,probe=probe) if probe_config is None else deepcopy(probe_config)
    validate(config);source=source_binding(config,library)
    allocation=cpu_allocation(config['execution_options']['workers'])
    p=read(panel)
    require(p['status']=='complete' and len(p['cases'])==32 and
        {c['geometry_identity'] for c in p['cases']}<=set(reservation['geometry_sha256']),
        'masked-long.panel: complete reserved 32-factory development panel required')
    if not probe:
        require(verification is not None,'masked-long.readiness: completed server verification required')
        proof=read(verification)
        require(proof['status']=='passed' and proof['continuity_closed'] and proof['protocol']==bound(FIXED)
            and proof['source_sha256']==source['code_sha256'] and proof['config_sha256']==digest(config)
            and proof['panel']==bound(panel) and proof['allocation']['workers']==allocation['workers'],
            'masked-long.readiness: source/config/panel/allocation differs from completed verification')
    output.mkdir(parents=True)
    if shared:relocate_inputs(shared,output/'training-inputs',config)
    if config['training_input']['directory']=='@run' or shared:
        config['training_input']['directory']=str(output/'training-inputs')
    launch=dict(schema='declarative-learning-launch-v1',config=config,source=source,allocation=allocation,
        panel=bound(panel),verification=bound(FIXED if probe else verification),batch_experiment=True,
        masked_long=dict(protocol=bound(FIXED),probe=probe),created_unix_seconds=time.time())
    write(output/'launch.json',launch);identity=sha(output/'launch.json')
    seed=config['seeds'][0];owner=output/f'seed-{seed}'
    learner=Learner(seed,config,identity);saved=learner.checkpoint(None)
    job=dict(schema='declarative-learning-state-v1',status='ready',update=0,seconds=0.,records=[],inputs={},evaluation=[])
    empty=zero_proof(saved,job);path=owner/'checkpoints/0000.pt.gz'
    save(path,saved);account_write(job,path);job['latest_checkpoint']=reference(path,owner)
    write(owner/'state.json',job)
    write(output/'cold-origin.json',dict(schema='masked-long-cold-origin-v1',seed=seed,launch_sha256=identity,
        empty_state=empty,model_sha256=saved['model_sha256'],learned_state_imported=False))
    write(output/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,launch_sha256=identity,jobs={}))
    return dict(status='ready',output=str(output),seed=seed,probe=probe)


def run(output, environment, stop_after=None):
    import declarative_learning_run as runner
    launch=read(output/'launch.json');probe=launch['masked_long']['probe']
    require(stop_after is None or probe,'masked-long.stop: update stop is a readiness operation')
    owner=output/f'seed-{launch["config"]["seeds"][0]}'
    with exclusive_run(output),requested_stop() as requested:
        return runner.run(output,environment,lambda:requested() or
            (stop_after is not None and read(owner/'state.json')['update']>=stop_after))


def verify(probe, checks, output):
    launch=read(probe/'launch.json');config=launch['config'];seed=config['seeds'][0];owner=probe/f'seed-{seed}'
    job=read(owner/'state.json');matrix=read(probe/'state.json');tests=read(checks)
    require(matrix['status']==job['status']=='complete' and job['update']==read(FIXED)['probe_updates']
        and 2 in job['pause_boundaries'],'masked-long.readiness: completed real normal-run pause/resume required')
    source=source_binding(config,launch['source']['native_library'])
    require(source==launch['source'] and tests['status']=='passed' and tests['source_sha256']==source['code_sha256'],
        'masked-long.readiness: stale source or verification checks')
    saved=load(checked(job['latest_checkpoint'],owner))
    require(saved['update']==job['update'] and saved['protocol_sha256']==sha(probe/'launch.json')
        and all(s['native'] is None and s['next_start'] is not None for s in saved['streams']),
        'masked-long.readiness: committed complete wave and selected next starts required')
    records=[read(checked(ref,owner)) for ref in job['records']]
    require(all(len(r['worker_cpu_seconds'])==read(FIXED)['workers'] for r in records)
        and any(r['buffer']['eligible'] for r in records) and any(r['curriculum_completed'] for r in records),
        'masked-long.readiness: persistent workers, own successful replay and completed episodes required')
    from declarative_input_history import materialize
    inputs=materialize(job,owner);reserved=set(config['training_input']['geometry_reservation']['geometry_sha256'])
    require(not reserved.intersection(r['geometry_sha256'] for r in inputs.values()),'masked-long.readiness: reserved training geometry')
    require({r['start_curriculum']['depth'] for r in inputs.values()}>={0,1},'masked-long.readiness: fresh and inherited inputs required')
    long=configuration(config['training_input']['geometry_reservation'])
    result=dict(schema='masked-long-readiness-v1',status='passed',continuity_closed=True,protocol=bound(FIXED),
        source_sha256=source['code_sha256'],config_sha256=digest(long),panel=launch['panel'],allocation=launch['allocation'],
        checks=bound(checks),probe=str(probe),probe_state=bound(owner/'state.json'),
        checkpoint=job['latest_checkpoint'],cold_origin=bound(probe/'cold-origin.json'),
        pause_resume_boundary=2,updates=job['update'],input_count=job['input_count'],resource=job['resource'],
        rows=[dict(update=r['update'],collection_seconds=r['collection_seconds'],publication_seconds=r['publication_seconds'],
            update_seconds=r['ppo']['execution']['update_seconds'],prepare_worker_seconds=r['ppo']['execution']['prepare_worker_seconds'],
            input_access_seconds=r['input_access_seconds'],replay_samples=r['replay_samples_retained']) for r in records])
    require(not output.exists(),'masked-long.readiness: fresh receipt required');write(output,result);return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=('prepare-probe','prepare','run','verify'))
    parser.add_argument('--output',type=Path,required=True);parser.add_argument('--panel',type=Path)
    parser.add_argument('--reservation',type=Path);parser.add_argument('--native-library',type=Path)
    parser.add_argument('--verification',type=Path);parser.add_argument('--shared-inputs',type=Path)
    parser.add_argument('--environment',type=Path);parser.add_argument('--stop-after',type=int)
    parser.add_argument('--probe',type=Path);parser.add_argument('--checks',type=Path)
    a=parser.parse_args()
    if a.command in ('prepare-probe','prepare'):
        if any(v is None for v in (a.panel,a.reservation,a.native_library)):parser.error('prepare requires --panel, --reservation and --native-library')
        result=prepare(a.output,a.panel,read(a.reservation),a.native_library,probe=a.command=='prepare-probe',
            verification=a.verification,shared=a.shared_inputs)
    elif a.command=='run':
        if a.environment is None:parser.error('run requires --environment')
        result=run(a.output,a.environment,a.stop_after)
    else:
        if a.probe is None or a.checks is None:parser.error('verify requires --probe and --checks')
        result=verify(a.probe,a.checks,a.output)
    print({k:v for k,v in result.items() if k in ('status','output','seed','probe','updates','continuity_closed')})
