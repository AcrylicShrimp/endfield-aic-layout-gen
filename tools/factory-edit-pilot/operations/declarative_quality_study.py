"""Execute the registered eight-arm quality study with held-out confirmation."""
import argparse
from collections import Counter
from copy import deepcopy
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from declarative_quality_branch import (BASE,ROOT,PROFILES,checked,digest,endpoint,load,parent,prepare,
    read,require,sha,source_binding,write)
from declarative_quality_readiness import authenticate
from declarative_quality_analysis import aggregate,allocation,geometry_outcomes,paired,ranked,select
from declarative_quality_storage import Budget,inode,run_budgeted,sync_fresh
from blueprint_execution_resources import ResourceGuard,cpu_allocation
from blueprint_ppo_store import reference
from declarative_execution_migrate import relocate_inputs
from declarative_learning_launch import pool,sources
from declarative_learning_report import report
from declarative_learning_runtime import account_write

FIXED=Path(__file__).with_name('declarative-quality-study.json')


def bound(path):return dict(path=str(Path(path).resolve()),sha256=sha(path))


def check_bound(ref):
    path=Path(ref['path']);require(sha(path)==ref['sha256'],f'quality-study.binding: changed {path}');return path


def panel(config,library,output,stage,fixed,excluded,guard):
    """Reuse native panel workers, rejecting overlap before admitting each case."""
    output=Path(output);output.mkdir();selected=deepcopy(config)
    selected['training_input'].update(directory=str(output/'inputs'),seed_base=fixed[stage+'_generation_seed'])
    selected['panel'].update(seed_base=fixed[stage+'_generation_seed'],action_seed_base=fixed[stage+'_action_seed'],
        max_generated=fixed['max_generated_per_panel'],strata={s:fixed['factories_per_stratum'] for s in config['strata']})
    source=sources(selected,library);allocation_cpu=cpu_allocation(8);cases=[];attempts=[];identities=set();counts=Counter()
    began=time.monotonic()
    with pool(selected,library,allocation_cpu,output/'workers',guard) as workers:
        for episode in range((fixed['max_generated_per_panel']+7)//8):
            rows=workers.exchange([dict(operation='panel',episode=episode) for _ in range(8)])
            for row in rows:
                case=row['case'];matches=[s for s,products in config['strata'].items() if case['product'] in products]
                require(len(matches)==1,'quality-study.panel: unknown product stratum');stratum=matches[0]
                identity=case['geometry_identity'];status='accepted'
                if identity in excluded:status='excluded-prior-or-training-geometry'
                elif identity in identities:status='duplicate-geometry'
                elif counts[stratum]>=fixed['factories_per_stratum']:status='surplus-admitted'
                else:
                    case.update(index=len(cases),stratum=stratum,action_seed=fixed[stage+'_action_seed']+len(cases)*100)
                    cases.append(case);identities.add(identity);counts[stratum]+=1
                attempts.append(dict(status=status,generation_seed=case['generation_seed'],geometry_identity=identity,input=case['input']))
            write(output/'progress.json',dict(cases=cases,attempts=attempts))
            print(dict(stage=stage+'-panel',selected=len(cases),generated=len(attempts)),flush=True)
            if all(counts[s]==fixed['factories_per_stratum'] for s in config['strata']):break
    require(all(counts[s]==fixed['factories_per_stratum'] for s in config['strata']),
        'quality-study.panel: exhausted registered generation allowance')
    result=dict(schema='declarative-learning-panel-v1',status='complete',cases=cases,attempts=attempts,source=source,
        selection='First distinct native-valid geometries per stratum; prior/training identities rejected; no edit-outcome filter.',
        seconds=time.monotonic()-began,allocation=allocation_cpu)
    write(output/'panel.json',result);return bound(output/'panel.json')


def disjoint(panels,inputs):
    training={r['geometry_sha256'] for r in inputs.values()};seen=set()
    for value in panels:
        current={c['geometry_identity'] for c in value['cases']}
        require(len(current)==len(value['cases']) and not (current & (training|seen)),
            'quality-study.leakage: panel geometry overlaps training or another panel')
        seen.update(current)


def install_initial(branch,cached):
    """Reuse one same-model, same-panel initial evaluation; replace diagnostic links."""
    if cached is None:return
    branch=Path(branch);launch=read(branch/'launch.json');proof=read(branch/'quality-branch.json')
    require(cached['panel']==launch['panel'] and cached['model_sha256']==proof['preserved']['model'] and
        cached['update']==proof['first_update'],'quality-study.initial: changed panel/model/update')
    source=check_bound(cached['report']);owner=branch/f'seed-{launch["config"]["seeds"][0]}'
    job=read(owner/'state.json');require(job['status']=='ready' and not job['evaluation'],'quality-study.initial: already evaluated')
    target=owner/f'evaluation/{proof["first_update"]:04d}/report.json';target.parent.mkdir(parents=True,exist_ok=True)
    if target.exists():target.unlink()
    os.link(source,target);account_write(job,target)
    job['evaluation']=[dict(update=proof['first_update'],**reference(target,owner))]
    write(owner/'state.json',job);write(branch/'initial-evaluation-reuse.json',cached)


def initial_cache(branch):
    branch=Path(branch);launch=read(branch/'launch.json');proof=read(branch/'quality-branch.json')
    owner=branch/f'seed-{launch["config"]["seeds"][0]}';job=read(owner/'state.json')
    ref=job['evaluation'][0];require(ref['update']==proof['first_update'],'quality-study.initial: wrong initial update')
    return dict(panel=launch['panel'],model_sha256=proof['preserved']['model'],update=proof['first_update'],
        report=bound(checked(ref,owner)))


def child(branch,environment,budget,guard):
    branch=Path(branch)
    with (branch/'console.log').open('w') as log:
        process=subprocess.Popen([sys.executable,'-u',__file__,'branch','--output',str(branch),
            '--environment',str(environment),'--budget',str(budget.path)],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            while True:
                guard()
                try:code=process.wait(timeout=1);break
                except subprocess.TimeoutExpired:pass
            require(code==0,f'quality-study.execution: exit {code}; inspect {branch / "console.log"}')
        except BaseException:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
            raise
    budget.value=read(budget.path)


def summarize(branch,panels,first_slot,shared,guard):
    value=endpoint(branch);disjoint(panels,value['job']['inputs']);normal=report(branch)
    require(normal['evaluation']['geometry_disjoint']==len(panels[0]['cases']),
        'quality-study.leakage: silently omitted evaluation factory')
    proof=value['study'];first=proof['first_update'];job=value['job'];owner=value['owner']
    require(job['update']==proof['endpoint'],'quality-study.endpoint: duration stopped before planned endpoint')
    records=[read(checked(r,owner)) for r in job['records'][first:]]
    evaluation=read(checked(job['evaluation'][-1],owner));config=value['launch']['config']
    panel_value=read(value['launch']['panel']['path'])
    cases=geometry_outcomes(evaluation['rows'],panel_value,config['strata'])
    fresh=sync_fresh(branch,shared,first_slot,guard)
    result=dict(profile=proof['profile'],replicate=proof['replicate'],first_update=first,endpoint=job['update'],
        updates=job['update']-first,quality=aggregate([cases],config['strata']),
        allocation=allocation(records),fresh_slots=fresh,normal_report=bound(Path(branch)/'report.json'),
        evaluation=bound(checked(job['evaluation'][-1],owner)),checkpoint=bound(checked(job['latest_checkpoint'],owner)),
        branch=bound(Path(branch)/'quality-branch.json'),timings=normal['seeds'][0]['timings'],seconds=normal['seconds'],
        curriculum=normal['seeds'][0]['curriculum'],guards=normal['seeds'][0]['evaluations'][-1]['all']['guard_counts'])
    write(Path(branch)/'quality-summary.json',result);return result


def run(args):
    began=time.monotonic();fixed=read(FIXED)
    require(tuple(fixed['profiles'])==PROFILES and fixed['max_updates']==736 and fixed['max_seconds']==14400,
        'quality-study.protocol: altered registered allocation')
    output=args.output.resolve();require(not output.exists(),'quality-study.output: fresh directory required')
    value=parent(args.parent);authenticate(value,args.parent_source);config=value['launch']['config']
    readiness=read(args.readiness)
    require(readiness['status']=='passed' and readiness['continuity_closed'] and readiness['parent']==value['binding'],
        'quality-study.readiness: stale or incomplete verification')
    for name,expected in readiness['source']['files_sha256'].items():
        require(sha(ROOT/name)==expected,f'quality-study.readiness: verified source changed: {name}')
    check_bound(readiness['cuda_proof']);features=read(check_bound(readiness['initial_metadata']))
    require(features['parent']==value['binding'],'quality-study.metadata: stale initial pool')
    source=source_binding(config,value['launch']['source']['native_library'])
    output.mkdir();guard=ResourceGuard(output,fixed['max_seconds'],config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    guard.began=began;guard()
    excluded_inodes={inode(p)[0] for p in value['job']['retained_files'] if Path(p).is_file()}
    old_bank=Path(config['training_input']['directory'])
    excluded_inodes.update(inode(p)[0] for p in old_bank.rglob('*') if p.is_file())
    budget=Budget(output/'storage-budget.json',fixed['max_added_bytes'],excluded_inodes)
    prior_paths=[Path(value['launch']['panel']['path']),*args.prior_panel]
    prior=[read(p) for p in prior_paths]
    excluded={r['geometry_sha256'] for r in value['job']['inputs'].values()}
    excluded.update(c['geometry_identity'] for p in prior for c in p['cases'])
    receipt=dict(schema=fixed['schema'],status='running',config=bound(FIXED),source=source,parent=value['binding'],
        readiness=bound(args.readiness),prior_panels=[bound(p) for p in prior_paths],began_unix=time.time(),
        arms=[],executed_updates=0,main_resumed=False)
    write(output/'study.json',receipt)
    try:
        panels={}
        for stage in ('development','confirmation'):
            panels[stage]=panel(config,source['native_library'],output/(stage+'-panel'),stage,fixed,excluded,guard)
            excluded.update(c['geometry_identity'] for c in read(panels[stage]['path'])['cases'])
            budget.scan(output/(stage+'-panel'));budget.save();guard()
        receipt['panels']=panels;write(output/'study.json',receipt)
        shared=output/'fresh-bank';relocate_inputs(old_bank,shared,config)
        panel_values=[read(panels[s]['path']) for s in panels];disjoint(panel_values,value['job']['inputs'])
        first=value['saved']['update'];first_slot=first*16;quartiles=None;development={};initial_dev=None;initial_confirm=None
        def arm(profile,stage,replicate,updates,previous,cached=None):
            nonlocal quartiles,budget
            guard();directory=output/f'{stage}-r{replicate}-{profile}'
            if stage=='confirmation':check_bound(receipt['selection'])
            proof=prepare(previous,directory,profile=profile,panel=check_bound(panels[stage]),updates=updates,
                seconds=fixed['max_seconds']-(time.monotonic()-began),replicate=replicate,
                features=features['entries'] if previous is value else None,initial_quartiles=quartiles,input_bank=shared,guard=guard)
            if quartiles is None:quartiles=proof['initial_quartiles']
            install_initial(directory,cached);budget.scan(directory);budget.scan(shared);budget.save()
            print(dict(stage=stage,profile=profile,replicate=replicate,status='training',updates=updates),flush=True)
            receipt['active_arm']=dict(directory=str(directory),first_update=proof['first_update'],endpoint=proof['endpoint'])
            write(output/'study.json',receipt)
            child(directory,args.environment,budget,guard)
            summary=summarize(directory,panel_values,first_slot,shared,guard)
            receipt['executed_updates']+=summary['updates'];require(receipt['executed_updates']<=fixed['max_updates'],'quality-study.updates: budget exceeded')
            receipt.pop('active_arm',None)
            receipt['arms'].append(dict(stage=stage,profile=profile,replicate=replicate,summary=bound(directory/'quality-summary.json')))
            budget.scan(directory);budget.scan(shared);budget.save();guard()
            receipt.update(seconds=time.monotonic()-began,conservative_added_bytes=budget.value['bytes'])
            write(output/'study.json',receipt)
            print(dict(stage=stage,profile=profile,replicate=replicate,status='complete',quality=summary['quality']['mean'],
                executed_updates=receipt['executed_updates'],seconds=receipt['seconds']),flush=True)
            return directory,summary
        for profile in PROFILES:
            directory,summary=arm(profile,'development',1,32,value,initial_dev)
            development[profile]=(directory,summary)
            if initial_dev is None:initial_dev=initial_cache(directory)
        scores={p:s['quality']['mean'] for p,(_,s) in development.items()};selected=select(scores)
        selection=dict(schema='declarative-quality-selection-v1',selected=selected,scores=scores,
            criterion='Actual native development endpoint area reduction; no coverage or improvement threshold.',
            source_sha256=source['code_sha256'],parent=value['binding'],panels=panels,
            development=[bound(d/'quality-summary.json') for d,_ in development.values()],selected_unix=time.time())
        selection_path=output/'selection.json'
        with selection_path.open('x') as stream:json.dump(selection,stream,indent=2);stream.write('\n')
        os.chmod(selection_path,0o444);receipt['selection']=bound(selection_path);write(output/'study.json',receipt)
        confirmation={}
        for profile in ['baseline-8',*selected]:
            previous=endpoint(development[profile][0])
            _,one=arm(profile,'confirmation',1,64,previous)
            directory,two=arm(profile,'confirmation',2,96,value,initial_confirm)
            if initial_confirm is None:initial_confirm=initial_cache(directory)
            confirmation[profile]=aggregate([{
                k:v['training_replicates'][0] for k,v in s['quality']['geometries'].items()} for s in (one,two)],config['strata'])
        check_bound(receipt['selection']);require(parent(args.parent)['binding']==value['binding'],'quality-study.parent: main changed')
        require(receipt['executed_updates']==736,'quality-study.updates: incomplete study')
        ranking=ranked({p:v['mean'] for p,v in confirmation.items()})
        receipt.update(status='complete',confirmation=confirmation,ranking=ranking,
            comparisons={p:paired(v,confirmation['baseline-8'],config['strata'],fixed['bootstrap_seed'],fixed['bootstrap_resamples'])
                for p,v in confirmation.items() if p!='baseline-8'},
            conclusion='Observed confirmation ranking; correlated stochastic replicates from one parent, not independent initializations.')
        budget.scan(output);budget.save();guard()
        receipt.update(seconds=time.monotonic()-began,conservative_added_bytes=budget.value['bytes'],actual_added=budget.actual(output),resource=guard.last)
        write(output/'study.json',receipt);return receipt
    except BaseException as error:
        active=receipt.get('active_arm')
        if active:
            job_path=Path(active['directory'])/f'seed-{config["seeds"][0]}/state.json'
            if job_path.exists():
                job=read(job_path);receipt['partial_arm']=dict(active,completed_update=job['update'],checkpoint=job['latest_checkpoint'])
                receipt['executed_updates']+=max(0,job['update']-active['first_update'])
        receipt.update(status='incomplete',seconds=time.monotonic()-began,diagnostic=dict(code='quality-study.failed',
            severity='error',stage='study',message=str(error),entity=str(output)),ranking=None)
        write(output/'study.json',receipt);raise


def main():
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='command',required=True)
    run_parser=sub.add_parser('run')
    for name in ('parent','parent-source','readiness','output','environment'):run_parser.add_argument('--'+name,type=Path,required=True)
    run_parser.add_argument('--prior-panel',type=Path,action='append',default=[])
    branch=sub.add_parser('branch')
    for name in ('output','environment','budget'):branch.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    try:
        result=run(args) if args.command=='run' else run_budgeted(args.output,args.environment,args.budget)
        print(json.dumps(dict(status=result['status'])));return 0 if result['status']=='complete' else 2
    except (ValueError,KeyError,TypeError,OSError,RuntimeError) as error:
        print(json.dumps(dict(code='quality-study.failed',severity='error',stage=args.command,message=str(error),entity=str(args.output))),file=sys.stderr)
        return 2


if __name__=='__main__':raise SystemExit(main())
