"""Bounded cold-mask readiness, paired fresh-input training and final comparison."""
import argparse
from copy import deepcopy
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from cold_mask_arm import (BASE,ROOT,FIXED,FIELDS,bound,configuration,digest,endpoint,new_bank,prepare,
    protocol,read,require,reserve,source_binding,write)
from blueprint_execution_resources import ResourceGuard,cpu_allocation
from blueprint_ppo_store import checked,load,sha
from declarative_learning_launch import panel as generate_panel
from declarative_quality_storage import sync_fresh


def child(branch, environment, guard, stop_after=None):
    command=[sys.executable,str(Path(__file__).with_name('cold_mask_arm.py')),'--output',str(branch),'--environment',str(environment)]
    if stop_after is not None:command+=['--stop-after',str(stop_after)]
    with (Path(branch)/'console.log').open('ab') as log:
        process=subprocess.Popen(command,stdout=log,stderr=subprocess.STDOUT)
        try:
            while process.poll() is None:
                guard();time.sleep(.5)
            require(process.returncode==0,f'cold-mask.child: failed arm {branch}; inspect console.log')
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:process.wait(timeout=120)
                except subprocess.TimeoutExpired:
                    process.kill();process.wait()


def actual_bytes(*directories):
    files={}
    for directory in directories:
        for path in Path(directory).rglob('*'):
            if path.is_file():
                stat=path.stat();files[stat.st_dev,stat.st_ino]=stat.st_size
    return sum(files.values())


def check_disjoint(panels, job):
    reserved={c['geometry_identity'] for p in panels for c in read(p)['cases']}
    require(not reserved.intersection(r['geometry_sha256'] for r in job['inputs'].values()),
        'cold-mask.leakage: held-out geometry appears in training ledger')


def summarize_arm(value):
    records=[read(checked(ref,value['owner'])) for ref in value['job']['records']]
    return dict(seed=value['proof']['origin']['seed'],variant=value['proof']['origin']['variant'],
        updates=value['job']['update'],seconds=value['job']['seconds'],retained_bytes=value['job']['retained_bytes'],
        mean_collection_seconds=sum(r['collection_seconds'] for r in records)/len(records),
        mean_learner_seconds=sum(r['ppo']['execution']['update_seconds'] for r in records)/len(records),
        mean_publication_seconds=sum(r['publication_seconds'] for r in records)/len(records),
        replay_labels=sum(r['ppo']['success']['label_count'] for r in records),
        cuda_peak_allocated_bytes=value['job']['cuda_peak_allocated_bytes'],resource=value['job']['resource'],
        checkpoint=bound(checked(value['job']['latest_checkpoint'],value['owner'])),
        evaluation=value['job']['evaluation'])


def readiness(args):
    f=protocol();output=args.output.resolve()
    require(not output.exists(),'cold-mask.readiness: fresh diagnostic output required')
    output.mkdir(parents=True)
    reservation=reserve([args.old_development,args.old_confirmation])
    config=configuration(71,'original',output/'original',reservation,readiness=True,seconds=600)
    guard=ResourceGuard(output,f['readiness_seconds'],config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    shared=output/'fresh-bank';new_bank(shared,config,args.library);rows=[];initials=[];restored_count=0
    for variant in f['variants']:
        branch=output/variant
        proof=prepare(branch,71,variant,args.old_confirmation,shared,reservation,args.library,FIXED,
            readiness=True,seconds=600)
        initials.append(proof['empty_state'])
        if variant=='masked':
            child(branch,args.environment,guard,stop_after=2)
            paused=endpoint(branch,complete=False)
            require(paused['job']['status']=='paused' and paused['job']['update']==2,
                'cold-mask.readiness: normal checkpoint pause did not reach update 2')
            write(output/'resume-boundary.json',dict(checkpoint=bound(checked(paused['job']['latest_checkpoint'],paused['owner'])),
                model_sha256=paused['saved']['model_sha256'],buffer_sha256=digest(paused['saved']['buffer'])))
            del paused
        child(branch,args.environment,guard)
        value=endpoint(branch);summary=summarize_arm(value)
        require(summary['updates']==4 and summary['replay_labels']>0 and summary['cuda_peak_allocated_bytes']>0,
            'cold-mask.readiness: missing real CUDA/PPO/success-replay execution')
        require(value['saved']['model_sha256']!=proof['empty_state']['model'],
            'cold-mask.readiness: model did not learn')
        from declarative_learning_environment import Runtime,Stream
        runtime=Runtime(value['launch']['config'],args.library)
        live=load(value['owner']/'checkpoints/0001.pt.gz')
        for saved in live['streams']:
            guard();require(saved['native'] is not None and saved['policy_actions']==32,'cold-mask.readiness: missing live checkpoint')
            restored=Stream.restore(runtime,saved);snapshot=restored.snapshot()
            expected=deepcopy(saved);observed=deepcopy(snapshot)
            require(observed['native']['elapsed_ns']>=expected['native']['elapsed_ns'],'cold-mask.readiness: native clock reset')
            observed['native']['elapsed_ns']=expected['native']['elapsed_ns']
            require(digest(observed)==digest(expected),'cold-mask.readiness: live restore changed saved state')
            restored_count+=1
            del restored
        # Audit only the compact sources retained by the four-update diagnostic.
        masked_rows=0
        for ref in value['job']['records']:
            record=read(checked(ref,value['owner']))
            chunk=load(checked(record['chunk'],value['owner']))
            require(record['actions']==1024 and not record['full_trace_retained'],'cold-mask.readiness: changed compact update')
            require(len(chunk['completed'])==(32 if record['update']%2==0 else 0) and
                all(c['result']['factory_validated'] for c in chunk['completed']), 'cold-mask.readiness: invalid native completion')
            for sample in chunk['samples'].values():
                has='root_invalid' in sample['input']
                require(has==(variant=='masked'),'cold-mask.readiness: compact replay lost mask mode')
                if has:
                    require(not bool(sample['input']['root_invalid'][sample['decision']['root']]),
                        'cold-mask.readiness: retained replay label was masked')
                    masked_rows+=1
        summary['masked_replay_rows']=masked_rows;rows.append(summary)
        check_disjoint([args.old_development,args.old_confirmation],value['job'])
        sync_fresh(branch,shared,0,guard)
        del value,live,runtime
    require(initials[0]==initials[1],'cold-mask.readiness: paired initial model/Adam/RNG/buffer differs')
    guard()
    result=dict(schema='cold-mask-readiness-v1',status='passed',protocol=bound(FIXED),
        source_sha256=source_binding(config,args.library)['code_sha256'],allocation=cpu_allocation(16),
        rows=rows,initial_state=initials[0],live_streams_restored=restored_count,normal_masked_resume=[2,4],
        seconds=time.monotonic()-guard.began,actual_bytes=actual_bytes(output),resource=guard.last)
    write(output/'readiness.json',result);print(result,flush=True)


def panels(args):
    f=protocol();output=args.output.resolve()
    require(not output.exists(),'cold-mask.panels: fresh panel output required')
    output.mkdir(parents=True);prior=[args.old_development,args.old_confirmation];result={};began=time.monotonic()
    for stage in ('development','confirmation'):
        config=deepcopy(f['numerical_base'])
        config['training_input']['geometry_reservation']=reserve(prior)
        config['panel'].update(seed_base=f[stage+'_generation_seed'],action_seed_base=f[stage+'_action_seed'],max_generated=f['max_generated_per_panel'])
        config['readiness']['seconds']=f['panel_seconds']-(time.monotonic()-began)
        require(config['readiness']['seconds']>0,'cold-mask.panels: panel generation allowance exhausted')
        generate_panel(config,args.library,output/stage)
        path=output/stage/'panel.json';result[stage]=bound(path);prior.append(path)
    write(output/'panels.json',dict(schema='cold-mask-panels-v1',status='complete',protocol=bound(FIXED),panels=result,
        excluded_prior=[bound(args.old_development),bound(args.old_confirmation)]))


def study(args):
    f=protocol();output=args.output.resolve();panel_manifest=read(args.panels)
    require(panel_manifest['status']=='complete' and panel_manifest['protocol']==bound(FIXED),'cold-mask.panels: changed scientific cohort')
    reserved_panels=[Path(r['path']) for r in panel_manifest['panels'].values()]+[Path(r['path']) for r in panel_manifest['excluded_prior']]
    reservation=reserve(reserved_panels);config=configuration(71,'original',output/'bank-owner',reservation)
    proof=read(args.readiness)
    require(proof['status']=='passed' and proof['protocol']==bound(FIXED) and
        proof['source_sha256']==source_binding(config,args.library)['code_sha256'],'cold-mask.readiness: stale gate')
    if (output/'study.json').exists():
        state=read(output/'study.json')
        require(state['protocol']==bound(FIXED) and state['panels']==panel_manifest['panels'] and
            state['readiness']==bound(args.readiness),'cold-mask.resume: changed study inputs')
        if state['status']=='complete':return
    else:
        require(not output.exists(),'cold-mask.study: new output or existing state required')
        output.mkdir(parents=True)
        state=dict(schema='cold-mask-study-v1',status='running',directory=str(output),protocol=bound(FIXED),
            panels=panel_manifest['panels'],readiness=bound(args.readiness),began_unix=time.time(),
            deadline_unix=time.time()+f['max_training_seconds'],completed=[],initials={})
        new_bank(output/'fresh-bank',config,args.library);write(output/'study.json',state)
    remaining=state['deadline_unix']-time.time()
    require(remaining>0,'cold-mask.clock: registered scientific deadline expired')
    guard=ResourceGuard(output,remaining,config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    try:
        for seed,variant in f['order']:
            guard();name=f'seed-{seed}-{variant}';branch=output/name
            state['active']=name;state['status']='running';write(output/'study.json',state)
            if not branch.exists():
                seconds=min(2700.,state['deadline_unix']-time.time())
                initial=prepare(branch,seed,variant,state['panels']['development']['path'],output/'fresh-bank',reservation,
                    args.library,args.readiness,seconds=seconds)
                key=str(seed)
                if key in state['initials']:
                    require(initial['empty_state']==state['initials'][key],'cold-mask.pair: initial model/Adam/RNG/buffer differs')
                else:state['initials'][key]=initial['empty_state']
                write(output/'study.json',state)
            matrix=read(branch/'state.json')
            if matrix['status']!='complete':child(branch,args.environment,guard)
            value=endpoint(branch);check_disjoint(reserved_panels,value['job'])
            initial_checkpoint=load(value['owner']/'checkpoints/0000.pt.gz')
            require(initial_checkpoint['update']==0 and {k:digest(initial_checkpoint[k]) for k in FIELDS}==value['proof']['empty_state'],
                'cold-mask.origin: initial learning state changed during publication')
            sync_fresh(branch,output/'fresh-bank',0,guard)
            summary=summarize_arm(value)
            if name not in [r['name'] for r in state['completed']]:state['completed'].append(dict(name=name,**summary))
            state['actual_bytes']=actual_bytes(output,args.readiness.parent,args.panels.parent)
            require(state['actual_bytes']<f['max_added_bytes'],'cold-mask.storage: study artifact ceiling reached')
            write(output/'study.json',state)
            print(dict(stage='trained',seed=seed,variant=variant,updates=summary['updates'],seconds=summary['seconds']),flush=True)
            del value,initial_checkpoint
        state['active']='confirmation';write(output/'study.json',state)
        from cold_mask_evaluate import evaluate
        evaluate(state,output/'confirmation',guard)
        state.update(status='complete',active=None,seconds=time.time()-state['began_unix'],
            confirmation=bound(output/'confirmation/result.json'),actual_bytes=actual_bytes(output,args.readiness.parent,args.panels.parent))
        require(state['actual_bytes']<f['max_added_bytes'],'cold-mask.storage: final artifact ceiling reached')
        write(output/'study.json',state)
    except BaseException as error:
        state.update(status='incomplete',error=str(error),seconds=time.time()-state['began_unix'])
        write(output/'study.json',state)
        raise


if __name__=='__main__':
    import torch
    torch.set_num_threads(1);torch.set_num_interop_threads(1)
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=('readiness','panels','study'))
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--library',type=Path,required=True)
    parser.add_argument('--environment',type=Path)
    parser.add_argument('--old-development',type=Path)
    parser.add_argument('--old-confirmation',type=Path)
    parser.add_argument('--readiness',type=Path)
    parser.add_argument('--panels',type=Path)
    args=parser.parse_args()
    if args.command in ('readiness','panels') and (args.old_development is None or args.old_confirmation is None):
        parser.error('readiness/panels require --old-development and --old-confirmation')
    if args.command in ('readiness','study') and args.environment is None:
        parser.error('readiness/study require --environment')
    if args.command=='study' and (args.readiness is None or args.panels is None):
        parser.error('study requires --readiness and --panels')
    globals()[args.command](args)
