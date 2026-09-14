"""One registered own-arm amendment at an empty native episode boundary."""
from copy import deepcopy
import os
from pathlib import Path
import shutil

from declarative_quality_cold import (FIELDS,FIXED,SCHEMA,bank_contract,bound,check_bound,config_for,
    protocol,require_readiness,reservation,source_binding,cpu_allocation)
from declarative_quality_branch import digest,read,require,sha,write
from blueprint_ppo_store import checked,load,reference,save
from declarative_learning_curriculum import replace_reserved_selected
from declarative_quality_storage import inode

ALLOWED_SOURCE_CHANGES={
    'tools/factory-edit-pilot/declarative_learning_curriculum.py',
    'tools/factory-edit-pilot/test_declarative_curriculum.py',
    *('tools/factory-edit-pilot/operations/'+name for name in (
        'declarative-quality-study.json','declarative_quality_cold.py',
        'declarative_quality_continuation.py','declarative_quality_study.py'))}


def prior():
    f=protocol();v=f['continuation'];study_path=check_bound(v['study']);study=read(study_path)
    closeout=read(check_bound(v['closeout']));branch=Path(study['active_arm']['directory'])
    launch=read(check_bound(v['launch']));proof=read(branch/'quality-branch.json');owner=branch/'seed-71'
    job=read(owner/'state.json');checkpoint=checked(job['latest_checkpoint'],owner)
    require(bound(checkpoint)==v['checkpoint'] and study['status']=='incomplete' and
        study['executed_updates']==job['update']==closeout['executed_updates']==86 and
        not study['arms'] and not study.get('selection') and closeout['committed_input_overlap']==0 and
        launch['cold_origin']==proof['origin'] and proof['launch_sha256']==v['launch']['sha256'] and
        proof['seed']==71 and proof['profile']=='baseline-8' and proof['first_update']==0 and proof['endpoint']==128,
        'quality-cold.continuation: foreign or changed own-arm prefix')
    old_root=study_path.parents[2]
    for name,h in launch['source']['files_sha256'].items():
        require(sha(old_root/name)==h,f'quality-cold.continuation: changed original source {name}')
    saved=load(checkpoint);initial=load(owner/'checkpoints/0000.pt.gz')
    require(saved['protocol_sha256']==proof['launch_sha256'] and saved['seed']==71 and saved['update']==86 and
        len(saved['streams'])==32 and all(s['native'] is None and s['episode']==43 for s in saved['streams']) and
        initial['update']==0 and {k:digest(initial[k]) for k in FIELDS}==proof['preserved'],
        'quality-cold.continuation: nonterminal checkpoint or changed empty origin')
    reserved=set(reservation(study['panels'])['geometry_sha256'])
    require(len(job['inputs'])==1376 and all(row['geometry_sha256'] not in reserved for row in job['inputs'].values()),
        'quality-cold.continuation: committed prefix input overlap')
    return dict(study=study,branch=branch,owner=owner,launch=launch,proof=proof,job=job,saved=saved,
        bindings=dict(launch=v['launch'],checkpoint=v['checkpoint'],job=bound(owner/'state.json'),
            origin=bound(branch/'quality-branch.json'),study=v['study'],closeout=v['closeout']))


def link_owned(source,target,budget):
    source=Path(source);target=Path(target)
    require(source.is_file() and not source.is_symlink(),'quality-cold.continuation: invalid immutable source')
    target.parent.mkdir(parents=True,exist_ok=True);os.link(source,target)
    key,_=inode(source);budget.excluded.add(key)


def import_bank(value,shared,config,library,budget):
    """Existing relocation preserves actual complete files and canonical identity."""
    from declarative_execution_migrate import relocate_inputs
    source=Path(value['launch']['config']['training_input']['directory'])
    contract,_=bank_contract(config,library)
    require(read(source/'manifest.json')==contract,'quality-cold.continuation: changed generation contract')
    require(all((p/'episode.json').is_file() for p in source.glob('episode-*')),
        'quality-cold.continuation: incomplete input requires explicit quarantine before recovery')
    relocate_inputs(source,shared,config)
    # Relocation links immutable versions; catalogs and receipt have new identities.
    for path in Path(shared).rglob('*'):
        if path.is_file() and path.stat().st_nlink>1:budget.excluded.add(inode(path)[0])
    budget.value['excluded']=sorted(budget.excluded)


def amend_snapshot(saved,plan,reserved):
    amended=deepcopy(saved);plan=deepcopy(plan)
    require(saved['update']==86 and len(saved['streams'])==len(plan['slots'])==32 and plan['wave']==44 and
        all(s['native'] is None and s['next_start']==plan['slots'][i] for i,s in enumerate(saved['streams'])),
        'quality-cold.continuation: changed pending plan or live episode')
    def layout(slot):
        value=read(checked(slot['input'],Path(slot['input']['directory'])))
        require(digest(value['original'])==slot['best_sha256'],'quality-cold.continuation: changed selected layout')
        return value['original']
    replaced=replace_reserved_selected(plan,reserved,layout)
    require(replaced==[5],'quality-cold.continuation: expected only the registered pending collision')
    for index in replaced:amended['streams'][index]['next_start']=deepcopy(plan['slots'][index])
    restored=deepcopy(amended)
    for index in replaced:restored['streams'][index]['next_start']=deepcopy(saved['streams'][index]['next_start'])
    require(digest(restored)==digest(saved),'quality-cold.continuation: changed learned or other stream state')
    return amended,plan,replaced


def prepare(value,output,shared,library,seconds,readiness,budget):
    output=Path(output).resolve();f=protocol();panels=value['study']['panels'];reserved=reservation(panels)
    config=config_for(71,'baseline-8','development',output,seconds,reserved)
    previous=deepcopy(value['launch']['config']);previous['training_input']['directory']=config['training_input']['directory']
    for name in ('duration','limits'):previous[name]=deepcopy(config[name])
    require(previous==config,'quality-cold.continuation: changed numerical/profile/input contract')
    source=source_binding(config,library);old=value['launch']['source']
    changed={k for k in set(source['files_sha256'])|set(old['files_sha256']) if
        source['files_sha256'].get(k)!=old['files_sha256'].get(k)}
    require(changed<=ALLOWED_SOURCE_CHANGES and source['native_sha256']==old['native_sha256'],
        'quality-cold.continuation: changed learner/native/data source')
    allocation=cpu_allocation(config['execution_options']['workers']);require_readiness(readiness,source,allocation)
    require(not output.exists(),'quality-cold.continuation: new amendment directory required')
    staging=output.with_name(output.name+'.staging');require(not staging.exists(),'quality-cold.continuation: stale staging')
    job=deepcopy(value['job']);ref=job['curriculum_plans']['44'];plan=read(checked(ref,value['owner']))
    saved,plan,replaced=amend_snapshot(value['saved'],plan,set(reserved['geometry_sha256']))
    staging.mkdir(parents=True)
    try:
        owner=staging/'seed-71';final_owner=output/'seed-71'
        # Never copy mutable console, workers or tracking files.
        for name in job['retained_files']:
            path=Path(name);require(path.is_relative_to(value['owner']),'quality-cold.continuation: foreign owned file')
            link_owned(path,owner/path.relative_to(value['owner']),budget)
        from declarative_execution_migrate import relocate_inputs
        relocate_inputs(shared,staging/'training-inputs',config)
        for path in (staging/'training-inputs').rglob('*'):
            if path.is_file() and path.stat().st_nlink>1:budget.excluded.add(inode(path)[0])
        job['retained_files']={str(final_owner/Path(k).relative_to(value['owner'])):v for k,v in job['retained_files'].items()}
        job.update(status='ready',wandb_id=None,wandb_url=None);job.pop('diagnostic',None);job.pop('workers',None)
        origin=dict(directory=str(output),seed=71,profile='baseline-8',stage='development',protocol=bound(FIXED),
            reservation_panels=panels,continued_origin=value['proof']['origin'])
        launch=dict(schema='declarative-learning-launch-v1',config=config,source=source,allocation=allocation,
            panel=panels['development'],verification=bound(readiness),batch_experiment=True,cold_origin=origin)
        write(staging/'launch.json',launch);identity=sha(staging/'launch.json');saved['protocol_sha256']=identity
        def account(path):
            job['retained_files'][str(final_owner/path.relative_to(owner))]=path.stat().st_size
        plan_path=owner/'curriculum/continued-wave-000044/plan.json';write(plan_path,plan);account(plan_path)
        job['curriculum_plans']['44']=reference(plan_path,owner)
        path=owner/'checkpoints/0086.pt.gz';save(path,saved);account(path);job['latest_checkpoint']=reference(path,owner)
        job['retained_bytes']=sum(job['retained_files'].values());write(owner/'state.json',job)
        receipt=dict(schema='declarative-cold-own-continuation-v1',original=value['bindings'],
            preserved={k:digest(saved[k]) for k in FIELDS},update=86,replaced_slots=replaced,
            previous_plan=ref,amended_plan=job['curriculum_plans']['44'],source_changes=sorted(changed))
        proof=deepcopy(value['proof']);proof.update(origin=origin,launch_sha256=identity,source_sha256=source['code_sha256'],continuation=receipt)
        write(staging/'quality-branch.json',proof)
        write(staging/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,launch_sha256=identity,jobs={}))
        for ref in value['bindings'].values():check_bound(ref)
        budget.value['excluded']=sorted(budget.excluded)
        staging.rename(output);return proof
    except BaseException:
        shutil.rmtree(staging);raise
