"""Explicit isolated quality-study policy forks and normal observed training."""
import argparse
from collections import defaultdict
from copy import deepcopy
import json
import os
from pathlib import Path
import shutil
import sys
import time
from unittest.mock import patch

BASE=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(BASE),str(BASE/'diagnostics')]

from blueprint_execution_resources import cpu_allocation
from blueprint_ppo_store import ROOT,checked,load,read,reference,sha,write
from blueprint_ppo_update import digest,require
from declarative_execution_migrate import initialize,parent
from declarative_experiment_setup import link_parent_sources
from declarative_learning_launch import sources,validate
from declarative_learning_runtime import CHECKPOINT,Learner
from declarative_quality_observer import Observer,quartiles
from declarative_replay_sampling import priority_enabled,validate_priority
from declarative_success_features import extract,validate_metadata

SCHEMA='declarative-quality-branch-v1'
PROFILES=('baseline-8','uniform-16','uniform-32','gain-8','score-8','geometry-8','priority-8','protected-score-8')


def profile_success(previous,profile):
    require(profile in PROFILES,'quality-study.profile: unknown profile')
    value=deepcopy(previous);value.pop('sampling',None)
    retention=value['retention']
    for key in ('category_fraction','protection_updates','immature_order'):retention.pop(key,None)
    retention['schema']='declarative-replay-utility-v1'
    count=int(profile.rsplit('-',1)[1])
    if profile!='baseline-8':
        sampling=dict(schema='declarative-success-sampling-v1',kind='uniform',sequences_per_minibatch=count)
        if profile in ('gain-8','score-8','geometry-8','protected-score-8'):
            sampling.update(kind='score_mixture',mixture=.5,score_profile={
                'gain-8':'gain_only','geometry-8':'rare_3pp_exterior_capped'}.get(profile,'rare_3pp'))
        if profile=='priority-8':sampling.update(kind='positive_advantage',alpha=.6,beta=.1,epsilon=1e-6)
        value['sampling']=sampling
    if profile=='protected-score-8':
        retention.update(schema='declarative-replay-category-v2',category_fraction=.1,protection_updates=64,immature_order='area')
    return value


def source_binding(config,library):
    result=sources(config,library)
    extra=[*Path(__file__).parent.glob('declarative_quality_*.py'),
        BASE/'diagnostics/declarative_experiment_setup.py',BASE/'diagnostics/declarative_retention_corpus.py']
    fixed=Path(__file__).with_name('declarative-quality-study.json')
    if fixed.exists():extra.append(fixed)
    result['files_sha256'].update({str(p.relative_to(ROOT)):sha(p) for p in extra})
    result['code_sha256']=digest(result['files_sha256'])
    return result


def endpoint(directory,*,live_job=None):
    directory=Path(directory).resolve();launch=read(directory/'launch.json');matrix=read(directory/'state.json')
    proof=read(directory/'quality-branch.json')
    require(proof['schema']==SCHEMA and proof['launch_sha256']==sha(directory/'launch.json'),
        'quality-study.parent: unbound study branch')
    for name,expected in launch['source']['files_sha256'].items():
        require(sha(ROOT/name)==expected,f'quality-study.source: changed {name}')
    seed=launch['config']['seeds'][0];owner=directory/f'seed-{seed}'
    job=read(live_job) if live_job else read(owner/'state.json')
    require(live_job is not None or matrix['status']==job['status']=='complete','quality-study.parent: incomplete endpoint')
    saved=load(checked(job['latest_checkpoint'],owner))
    require(saved['schema']==CHECKPOINT and saved['protocol_sha256']==sha(directory/'launch.json') and
        saved['update']==job['update'] and saved['model_sha256']==digest(saved['model']), 'quality-study.parent: checkpoint identity')
    require(all((s['native'] is not None and s['policy_actions']==32) if live_job else
        (s['native'] is None and s['next_start'] is not None) for s in saved['streams']),
        'quality-study.parent: expected live or complete wave boundary')
    return dict(directory=directory,owner=owner,launch=launch,matrix=matrix,job=job,saved=saved,
        binding=dict(directory=str(directory),launch_sha256=sha(directory/'launch.json'),
            state_sha256=sha(directory/'state.json'),job_sha256=sha(live_job or owner/'state.json'),
            checkpoint=job['latest_checkpoint']),study=proof,live=live_job is not None)


def metadata(value,guard=lambda:None):
    """One source decode per chunk; consume complete sequences then release rows."""
    entries=value['saved']['buffer']['entries'];needed=defaultdict(list);pending={};result={}
    for identity,e in entries.items():
        if 'retention_features_v1' in e:
            validate_metadata(e['retention_features_v1'],identity);result[identity]=e['retention_features_v1'];continue
        pending[identity]=[None]*len(e['labels'])
        for i,label in enumerate(e['labels']):needed[label['reference'][0]].append((identity,i,label['reference'][1]))
    remaining={i:len(v) for i,v in pending.items()}
    for n,refs in sorted(needed.items()):
        guard();payload=load(checked(value['job']['replay_sources'][str(n)],value['owner']))
        for identity,i,index in refs:
            pending[identity][i]=payload['samples'][index];remaining[identity]-=1
            if remaining[identity]==0:result[identity]=extract(pending.pop(identity),identity)
        del payload
    require(not pending and result.keys()==entries.keys(),'quality-study.metadata: missing native sources')
    return result


def bootstrap_priorities(value,guard=lambda:None):
    from blueprint_success_buffer import Evidence
    from declarative_learning_buffer import Buffer
    from declarative_retention_corpus import frozen_batch
    import declarative_learning_policy as policy
    import torch
    config=value['launch']['config'];saved=value['saved']
    model=policy.ActorCritic(config['ppo']['width'],config['ppo']['message_rounds']).to(config['device'])
    model.load_state_dict(saved['model'],strict=True);model.eval()
    buffer=Buffer(saved['buffer']);buffer.hydrate(Evidence(value['owner'],value['job']),config['strata'])
    result={};batch=[];labels=nodes=0
    def flush():
        if batch:
            guard();result.update(frozen_batch(model,batch,config['device']))
    for _,sequence in sorted(buffer.cache.items()):
        size=sum(len(r['input']['node_static']) for r in sequence['rows'])
        if batch and (labels+len(sequence['rows'])>256 or nodes+size>32768):flush();batch=[];labels=nodes=0
        batch.append(sequence);labels+=len(sequence['rows']);nodes+=size
    flush();del model,buffer
    return {k:dict(value=v['positive'],observed_update=None) for k,v in result.items()}


def transition_saved(value,success,features,replicate,priorities=None):
    require(replicate in (1,2),'quality-study.replicate: expected 1 or 2')
    saved=deepcopy(value['saved']);entries=saved['buffer']['entries']
    require(features.keys()==entries.keys(),'quality-study.metadata: initial entry coverage')
    for identity,e in entries.items():
        validate_metadata(features[identity],identity)
        require('retention_features_v1' not in e or e['retention_features_v1']==features[identity],
            'quality-study.metadata: conflicting bound native quality')
        e['retention_features_v1']=deepcopy(features[identity])
    previous=value['launch']['config']['success']
    if priority_enabled(success.get('sampling')):
        if priority_enabled(previous.get('sampling')):
            require(priorities is None,'quality-study.priority: continuation must not bootstrap')
        else:
            require(priorities is not None and priorities.keys()==entries.keys(),'quality-study.priority: initial bootstrap required')
            for identity,e in entries.items():e['sampling_priority_v1']=deepcopy(priorities[identity])
        for identity,e in entries.items():validate_priority(e.get('sampling_priority_v1'),identity)
    if replicate==2 and 'study' not in value:
        import torch
        saved['sampler_rng']=torch.Generator().manual_seed(714400029).get_state()
        saved['shuffle_rng']=torch.Generator().manual_seed(714500029).get_state()
    elif 'study' in value:
        require(replicate==value['study']['replicate'],'quality-study.replicate: continuation cannot change RNG branch')
    return saved


def assert_contract(config,previous,profile,updates,seconds,panel_config):
    expected=deepcopy(previous);first=previous['training'].get('first_update',0)
    # Compare complete objects after replacing only the explicitly allowed fields.
    for key in ('training','evaluation','duration','limits','retention','tracking','training_input','panel'):
        for field in {
            'training':('first_update','updates'), 'evaluation':('at','interval_updates','replicates'),
            'duration':('seconds','finish_grace_seconds'), 'limits':('arm_seconds','matrix_seconds'),
            'retention':('checkpoints','training_traces','evaluation_traces'), 'tracking':('mode',),
            'training_input':('directory',), 'panel':tuple(panel_config)}[key]:
            expected[key][field]=config[key][field]
    expected['success']=profile_success(previous['success'],profile)
    require(config==expected,'quality-study.contract: forbidden learning/input/native change')
    require(config['training']['updates']-config['training']['first_update']==updates and 0<updates<=96 and
        config['limits']['arm_seconds']==seconds,'quality-study.bounds: endpoint or duration differs')
    validate(config)


def prepare(value,output,*,profile,panel,updates,seconds,replicate=1,features=None,priorities=None,
            initial_quartiles=None,input_bank=None,retain_live=False,guard=lambda:None):
    output=Path(output).resolve();require(not output.exists(),f'quality-study.output: fresh directory required: {output}')
    require(profile in PROFILES and 0<seconds<=14400,'quality-study.profile: invalid profile/budget')
    if 'study' in value:require(profile==value['study']['profile'],'quality-study.profile: continuation must preserve profile')
    panel=Path(panel).resolve();panel_value=read(panel)
    require(panel_value['schema']=='declarative-learning-panel-v1' and panel_value['status']=='complete',
        'quality-study.panel: complete bound panel required')
    config=deepcopy(value['launch']['config']);first=value['saved']['update']
    config['success']=profile_success(config['success'],profile)
    config['training'].update(first_update=first,updates=first+updates)
    config['evaluation'].update(at=[first],interval_updates=32,replicates=2)
    grace=min(300.,seconds/3);config['duration'].update(seconds=seconds-grace,finish_grace_seconds=grace)
    config['limits'].update(arm_seconds=seconds,matrix_seconds=seconds)
    config['tracking']['mode']='disabled'
    config['retention'].update(training_traces=[],evaluation_traces=[])
    config['retention']['checkpoints']=sorted({*config['retention']['checkpoints'],first,first+updates,
        *([first+1,first+2] if retain_live else [])})
    if input_bank:config['training_input']['directory']=str(Path(input_bank).resolve())
    assert_contract(config,value['launch']['config'],profile,updates,seconds,{})
    guard();source=source_binding(config,value['launch']['source']['native_library'])
    require(source['native_sha256']==value['launch']['source']['native_sha256'],'quality-study.source: changed native binary')
    features=metadata(value,guard) if features is None else features
    if priority_enabled(config['success'].get('sampling')) and not priority_enabled(value['launch']['config']['success'].get('sampling')):
        priorities=bootstrap_priorities(value,guard) if priorities is None else priorities
    saved=transition_saved(value,config['success'],features,replicate,priorities)
    if initial_quartiles is None:initial_quartiles=quartiles(saved['buffer']['entries'],config['strata'])
    allocation=cpu_allocation(config.get('execution_options',{}).get('workers',8))
    selected=dict(value,saved=saved,launch=deepcopy(value['launch']),job=deepcopy(value['job']))
    selected['launch']['panel']=dict(path=str(panel),sha256=sha(panel))
    # Evaluation evidence is panel-specific. Never carry historical best selection.
    selected['job'].update(evaluation=[],best_evaluations={},random=[])
    output.mkdir(parents=True)
    try:
        compact_directory=output/'parent-sources';compacted=link_parent_sources(selected,compact_directory)
        receipt=initialize(selected,config,source,compacted,compact_directory,compact_directory/'manifest.json',
            output,allocation,diagnostic=True)
        proof=dict(schema=SCHEMA,status='ready',profile=profile,replicate=replicate,parent=value['binding'],
            launch_sha256=sha(output/'launch.json'),source_sha256=source['code_sha256'],first_update=first,
            endpoint=first+updates,live_restore=value.get('live',False),retain_live=retain_live,
            initial_quartiles=initial_quartiles,preserved={k:digest(saved[k]) for k in
                ('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','streams')},
            initial_buffer_sha256=digest(saved['buffer']),metadata_sha256=digest(features))
        write(output/'quality-branch.json',proof);return proof
    except BaseException:
        shutil.rmtree(output);raise


def run_observed(directory,environment):
    import declarative_learning_run as runner
    from declarative_duration_control import requested_stop
    directory=Path(directory).resolve();proof=read(directory/'quality-branch.json')
    require(proof['schema']==SCHEMA and proof['launch_sha256']==sha(directory/'launch.json'),
        'quality-study.binding: branch launch changed')
    original_publish,original_retain=runner.publish,runner.retain
    class ObservedLearner(Learner):
        def __init__(self,*args,**kwargs):
            super().__init__(*args,**kwargs);self.quality=Observer(self,proof['initial_quartiles'])
        def update(self,payload,evidence):
            self.quality.begin();record=super().update(payload,evidence)
            record['quality_allocation']=self.quality.finish(record);return record
    def retain(learner,owner,job):
        original_retain(learner,owner,job)
        if proof['retain_live'] and learner.ordinal==proof['first_update']+1:
            require(all(s['native'] is not None for s in load(checked(job['latest_checkpoint'],owner))['streams']),
                'quality-study.readiness: expected live first checkpoint')
            write(directory/'live-job.json',job)
    with requested_stop() as stop,patch.object(runner,'Learner',ObservedLearner),patch.object(runner,'retain',retain):
        return runner.run(directory,environment,stop)


def main():
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='command',required=True)
    run=sub.add_parser('run');run.add_argument('--branch',type=Path,required=True);run.add_argument('--environment',type=Path,required=True)
    args=parser.parse_args()
    try:
        result=run_observed(args.branch,args.environment);print(json.dumps(dict(status=result['status'])))
        return 0 if result['status']=='complete' else 2
    except (ValueError,KeyError,TypeError,OSError,RuntimeError) as error:
        print(json.dumps(dict(code='quality-study.branch',severity='error',stage=args.command,message=str(error),
            entity=str(args.branch))),file=sys.stderr);return 2


if __name__=='__main__':raise SystemExit(main())
