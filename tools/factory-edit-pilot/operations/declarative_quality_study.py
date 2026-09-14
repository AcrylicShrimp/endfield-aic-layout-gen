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
from declarative_quality_branch import (BASE,ROOT,PROFILES,checked,digest,load,
    read,require,sha,source_binding,write)
from declarative_quality_cold import endpoint,prepare
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


def reservation_cost(value):
    bank=Path(value['launch']['config']['training_input']['directory']);slots=attempts=rejected=0;seconds=0.
    for row in value['job']['inputs'].values():
        if row.get('start_curriculum',{}).get('depth',0)!=0:continue
        record=read(checked(row['reference'],bank));slots+=1;attempts+=len(record['attempts'])
        rejected+=sum(a.get('reservation_rejected',False) for a in record['attempts']);seconds+=record['setup_seconds']
    return dict(fresh_slots=slots,generation_attempts=attempts,rejected_attempts=rejected,setup_seconds=seconds)


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
    if (Path(branch)/'initial-evaluation-reuse.json').exists():
        initial_seconds=read(checked(job['evaluation'][0],owner))['seconds']
        result['timings']['evaluation']-=initial_seconds
        result['reused_initial_evaluation_seconds']=initial_seconds
    result['learning_curve']=[dict(update=ref['update'],quality=aggregate([geometry_outcomes(
        read(checked(ref,owner))['rows'],panel_value,config['strata'])],config['strata'])) for ref in job['evaluation']]
    result['mechanism_activity']=dict(buffer_entries=[r['buffer']['own_entries'] for r in records],
        evictions=sum(e['kind']=='evicted' for r in records for e in r['events']))
    result['reservation_cost']=reservation_cost(value)
    write(Path(branch)/'quality-summary.json',result);return result


def run(args):
    from declarative_quality_cold import protocol,config_for,monitor,new_bank,require_readiness,pure_evaluate,reservation,remaining_seconds,prior_cost
    began=time.monotonic();fixed=protocol();config=deepcopy(fixed['numerical_base'])
    allowance=remaining_seconds(fixed);prior_bytes=prior_cost(fixed)
    output=args.output.resolve();require(not output.exists(),'quality-cold.output: fresh study directory required')
    main=monitor(args.monitor_main);source=source_binding(config,args.library)
    readiness=require_readiness(args.readiness,source,cpu_allocation(config['execution_options']['workers']))
    require(readiness['main']==main,'quality-cold.main: changed paused main after readiness')
    require(readiness['prior_owned_bytes']==prior_bytes,'quality-cold.prior: readiness omitted prior owned bytes')
    output.mkdir();guard=ResourceGuard(output,allowance,config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    guard.began=began;guard();budget=Budget(output/'storage-budget.json',fixed['max_added_bytes'],
        prior_bytes=readiness['charged_total_bytes'])
    prior_paths=args.prior_panel
    require(len(prior_paths)>=5,'quality-cold.panel: original and both warm/failed-cold panel pairs required')
    prior=[read(p) for p in prior_paths];excluded={c['geometry_identity'] for p in prior for c in p['cases']}
    receipt=dict(schema=fixed['schema'],status='running',config=bound(FIXED),source=source,main=main,
        readiness=bound(args.readiness),prior_panels=[bound(p) for p in prior_paths],began_unix=time.time(),
        arms=[],executed_updates=0,discarded_updates=128,total_cold_updates=128,main_resumed=False,
        prior_owned_bytes=prior_bytes,readiness_added=readiness['actual_added'],wall_allowance_seconds=allowance,
        wall_deadline_unix_seconds=fixed['wall_deadline_unix_seconds'])
    write(output/'study.json',receipt)
    try:
        panels={}
        for stage in ('development','confirmation'):
            panels[stage]=panel(config,args.library,output/(stage+'-panel'),stage,fixed,excluded,guard)
            excluded.update(c['geometry_identity'] for c in read(panels[stage]['path'])['cases'])
            budget.scan(output/(stage+'-panel'));budget.save();guard()
        receipt['panels']=panels;write(output/'study.json',receipt)
        reserved=reservation(panels)
        shared=output/'fresh-bank';new_bank(shared,config_for(71,'baseline-8','development',output/'bank-owner',allowance,reserved),args.library)
        panel_values=[read(panels[s]['path']) for s in panels];disjoint(panel_values,{})
        development={};initial_dev=None;initial_confirm=None;initial_models={}
        def arm(profile,stage,seed,cached=None):
            guard();directory=output/f'{stage}-seed{seed}-{profile}'
            if stage=='confirmation':check_bound(receipt['selection'])
            proof=prepare(directory,seed=seed,profile=profile,stage=stage,panel=check_bound(panels[stage]),
                library=args.library,shared=shared,seconds=allowance-(time.monotonic()-began),readiness=args.readiness,reservation_panels=panels,
                selection=check_bound(receipt['selection']) if stage=='confirmation' else None)
            model=proof['preserved']['model']
            require(seed not in initial_models or initial_models[seed]==model,'quality-cold.initial: paired initial models differ')
            require(all(k==seed or h!=model for k,h in initial_models.items()),'quality-cold.initial: distinct seeds have identical models')
            initial_models[seed]=model;install_initial(directory,cached)
            budget.scan(directory);budget.scan(shared);budget.save()
            receipt['active_arm']=dict(directory=str(directory),seed=seed,first_update=0,endpoint=128)
            write(output/'study.json',receipt)
            print(dict(stage=stage,profile=profile,seed=seed,status='training',updates=128),flush=True)
            child(directory,args.environment,budget,guard)
            summary=summarize(directory,panel_values,0,shared,guard);summary['initialization_seed']=seed
            write(directory/'quality-summary.json',summary)
            receipt['executed_updates']+=summary['updates'];require(receipt['executed_updates']<=1408,'quality-cold.updates: ceiling exceeded')
            receipt['total_cold_updates']=128+receipt['executed_updates']
            receipt.pop('active_arm');receipt['arms'].append(dict(stage=stage,profile=profile,seed=seed,summary=bound(directory/'quality-summary.json')))
            budget.scan(directory);budget.scan(shared);budget.save();guard()
            receipt.update(seconds=time.monotonic()-began,conservative_added_bytes=budget.value['bytes'],initial_models=initial_models)
            write(output/'study.json',receipt)
            print(dict(stage=stage,profile=profile,seed=seed,status='complete',quality=summary['quality']['mean'],
                executed_updates=receipt['executed_updates'],seconds=receipt['seconds']),flush=True)
            return directory,summary
        for profile in PROFILES:
            directory,summary=arm(profile,'development',71,initial_dev);development[profile]=(directory,summary)
            if initial_dev is None:initial_dev=initial_cache(directory)
        scores={p:s['quality']['mean'] for p,(_,s) in development.items()};selected=select(scores)
        selection=dict(schema='declarative-cold-quality-selection-v1',selected=selected,scores=scores,protocol=bound(FIXED),
            criterion='Actual native development endpoint at update 128; no coverage or improvement threshold.',
            source_sha256=source['code_sha256'],panels=panels,
            development=[bound(d/'quality-summary.json') for d,_ in development.values()],selected_unix=time.time())
        selection_path=output/'selection.json'
        with selection_path.open('x') as stream:json.dump(selection,stream,indent=2);stream.write('\n')
        os.chmod(selection_path,0o444);receipt['selection']=bound(selection_path);write(output/'study.json',receipt)
        first_confirmation={};confirmation={}
        for profile in ['baseline-8',*selected]:
            branch=development[profile][0];destination=output/f'confirmation-seed71-{profile}'
            command_child(branch/'confirmation-console.log',['evaluate','--branch',str(branch),'--panel',str(check_bound(panels['confirmation'])),
                '--selection',str(selection_path),'--output',str(destination),'--budget',str(budget.path),
                '--seconds',str(allowance-(time.monotonic()-began))],guard)
            pure=read(destination/'pure-evaluation.json');ev=read(check_bound(pure['evaluation']))
            first_confirmation[profile]=geometry_outcomes(ev['rows'],panel_values[1],config['strata'])
            receipt.setdefault('pure_confirmation',{})[profile]=bound(destination/'pure-evaluation.json')
            budget.value=read(budget.path);budget.scan(destination);budget.save();write(output/'study.json',receipt)
        for profile in ['baseline-8',*selected]:
            directory,two=arm(profile,'confirmation',113,initial_confirm)
            if initial_confirm is None:initial_confirm=initial_cache(directory)
            confirmation[profile]=aggregate([first_confirmation[profile],{
                k:v['training_replicates'][0] for k,v in two['quality']['geometries'].items()}],config['strata'])
        check_bound(receipt['selection']);require(monitor(args.monitor_main)==main,'quality-cold.main: paused main changed')
        require(receipt['executed_updates']==1408,'quality-cold.updates: incomplete study')
        receipt.update(status='complete',confirmation=confirmation,ranking=ranked({p:v['mean'] for p,v in confirmation.items()}),
            comparisons={p:paired(v,confirmation['baseline-8'],config['strata'],fixed['bootstrap_seed'],fixed['bootstrap_resamples'])
                for p,v in confirmation.items() if p!='baseline-8'},
            conclusion='Bounded early learning across two observed random initializations with paired fresh inputs; not convergence.')
        budget.scan(output);budget.save();guard()
        receipt.update(seconds=time.monotonic()-began,conservative_added_bytes=budget.value['bytes'],actual_added=budget.actual(output),resource=guard.last)
        receipt['cumulative_actual_added_bytes']=prior_bytes+readiness['actual_added']['bytes']+receipt['actual_added']['bytes']
        write(output/'study.json',receipt);return receipt
    except BaseException as error:
        active=receipt.get('active_arm')
        if active:
            job_path=Path(active['directory'])/f'seed-{active["seed"]}/state.json'
            if job_path.exists():
                job=read(job_path);receipt['partial_arm']=dict(active,completed_update=job['update'],checkpoint=job['latest_checkpoint'])
                receipt['executed_updates']+=job['update']
        receipt['total_cold_updates']=128+receipt['executed_updates']
        receipt.update(status='incomplete',seconds=time.monotonic()-began,diagnostic=dict(code='quality-cold.failed',
            severity='error',stage='study',message=str(error),entity=str(output)),ranking=None)
        write(output/'study.json',receipt);raise


def command_child(log_path,arguments,guard):
    with Path(log_path).open('w') as log:
        process=subprocess.Popen([sys.executable,'-u',__file__,*arguments],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            while True:
                guard()
                try:code=process.wait(timeout=1);break
                except subprocess.TimeoutExpired:pass
            require(code==0,f'quality-cold.execution: exit {code}; inspect {log_path}')
        except BaseException:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
            raise


def main():
    from declarative_quality_cold import run_observed,pure_evaluate
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='command',required=True)
    run_parser=sub.add_parser('run')
    for name in ('readiness','output','environment','library','monitor-main'):run_parser.add_argument('--'+name,type=Path,required=True)
    run_parser.add_argument('--prior-panel',type=Path,action='append',default=[])
    branch=sub.add_parser('branch')
    for name in ('output','environment','budget'):branch.add_argument('--'+name,type=Path,required=True)
    evaluate=sub.add_parser('evaluate')
    for name in ('branch','panel','selection','output','budget'):evaluate.add_argument('--'+name,type=Path,required=True)
    evaluate.add_argument('--seconds',type=float,required=True)
    readiness=sub.add_parser('readiness')
    for name in ('output','environment','library','monitor-main','cuda-proof'):readiness.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    try:
        if args.command=='run':result=run(args)
        elif args.command=='branch':result=run_budgeted(args.output,args.environment,args.budget,execute=run_observed)
        elif args.command=='evaluate':result=run_budgeted(args.branch,None,args.budget,execute=lambda *_:
            pure_evaluate(args.branch,args.panel,args.selection,args.output,args.seconds))
        else:
            from declarative_quality_cold_readiness import verify
            result=verify(args)
        print(json.dumps(dict(status=result['status'])));return 0 if result['status'] in ('complete','passed') else 2
    except (ValueError,KeyError,TypeError,OSError,RuntimeError) as error:
        print(json.dumps(dict(code='quality-cold.failed',severity='error',stage=args.command,message=str(error),entity=str(args.output))),file=sys.stderr)
        return 2


if __name__=='__main__':raise SystemExit(main())
