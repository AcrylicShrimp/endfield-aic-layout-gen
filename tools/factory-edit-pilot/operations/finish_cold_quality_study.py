"""Complete the registered cold study after its user-authorized clock extension.

This administrative controller is bound separately; the original frozen learning
source, protocol, readiness receipt and all scientific settings remain unchanged.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from declarative_quality_branch import PROFILES,read,require,write
from declarative_quality_cold import (bound,check_bound,prepare,protocol,monitor,
    require_readiness,reservation,source_check)
from declarative_quality_study import (child,summarize,initial_cache,install_initial,
    aggregate,geometry_outcomes,paired,ranked)
from declarative_quality_storage import Budget
from blueprint_execution_resources import ResourceGuard
from blueprint_ppo_store import checked

OLD_DEADLINE=1789374130.8190546
DEADLINE=OLD_DEADLINE+3600
SELECTED=['priority-8','uniform-16']


def remaining(*,now=None,deadline=DEADLINE):
    require(deadline==DEADLINE,'quality-clock.deadline: unregistered extension')
    value=deadline-(time.time() if now is None else now)
    require(0<value<=18000,'quality-clock.deadline: expired or excessive allowance')
    return value


def process(pid,start_ticks):
    path=Path(f'/proc/{pid}/stat')
    try:fields=path.read_text().rsplit(')',1)[1].split()
    except FileNotFoundError:return None
    require(fields[19]==str(start_ticks),'quality-clock.process: PID identity changed')
    return fields[0]


def validate_handoff(study,control,output):
    output=Path(output).resolve();active=study.get('active_arm',{})
    require(study['schema']=='declarative-cold-quality-study-v3' and study['status']=='running' and
        study['executed_updates']==1024 and study['carried_updates']==86 and study['discarded_updates']==128 and
        len(study['arms'])==8 and {(r['stage'],r['seed'],r['profile']) for r in study['arms']}==
        {('development',71,p) for p in PROFILES},'quality-clock.handoff: expected eight development endpoints')
    require(active.get('directory')==str(output/'confirmation-seed113-baseline-8') and
        active.get('seed')==113 and active.get('first_update')==0 and active.get('endpoint')==128 and
        active.get('resumed_update')==0,'quality-clock.handoff: expected original cold baseline113')
    require(control['schema']=='declarative-cold-clock-handoff-v1' and
        control['branch']==active['directory'] and control['old_deadline']==study['wall_deadline_unix_seconds']==OLD_DEADLINE and
        control['new_deadline']==DEADLINE,'quality-clock.handoff: changed clock or branch')
    selected=read(check_bound(study['selection']))
    require(selected['selected']==SELECTED and selected['protocol']==study['config'] and
        selected['panels']==study['panels'] and selected['source_sha256']==study['source']['code_sha256'] and
        selected['development']==[r['summary'] for r in study['arms']],
        'quality-clock.selection: changed immutable selection')
    require(set(study.get('pure_confirmation',{}))=={'baseline-8',*SELECTED},
        'quality-clock.handoff: incomplete pure confirmation')
    for row in study['arms']:
        summary=read(check_bound(row['summary']))
        require(summary['profile']==row['profile'] and summary['initialization_seed']==71 and
            summary['first_update']==0 and summary['endpoint']==summary['updates']==128,
            'quality-clock.handoff: incomplete development endpoint')
        for name in ('checkpoint','branch','evaluation'):check_bound(summary[name])
    for ref in study['pure_confirmation'].values():check_bound(ref)
    return selected


def append_arm(receipt,branch,summary):
    profile=summary['profile'];key=('confirmation',113,profile)
    require(key not in {(r['stage'],r['seed'],r['profile']) for r in receipt['arms']} and
        profile in ['baseline-8',*SELECTED] and summary['initialization_seed']==113 and
        summary['first_update']==0 and summary['endpoint']==summary['updates']==128 and
        receipt['executed_updates']==128*len(receipt['arms']),
        'quality-clock.accounting: duplicate, partial or inconsistent arm')
    receipt['arms'].append(dict(stage='confirmation',seed=113,profile=profile,
        summary=bound(Path(branch)/'quality-summary.json')))
    receipt['executed_updates']+=128
    receipt['new_updates']=receipt['executed_updates']-86
    receipt['total_cold_updates']=128+receipt['executed_updates']
    receipt.pop('active_arm',None)


def failed(receipt,error):
    active=receipt.get('active_arm');updates=128*len(receipt['arms'])
    if active:
        owner=Path(active['directory'])/f'seed-{active["seed"]}'
        if (owner/'state.json').exists():
            job=read(owner/'state.json');updates+=job['update']
            receipt['partial_arm']=dict(active,completed_update=job['update'],checkpoint=job['latest_checkpoint'])
    receipt.update(status='incomplete',ranking=None,executed_updates=updates,
        new_updates=max(0,updates-86),total_cold_updates=128+updates,
        diagnostic=dict(code='quality-clock.failed',severity='error',stage='study',
            message=str(error),entity='clock-extension'))


def run(output,environment):
    output=Path(output).resolve();fixed=protocol();remaining()
    entry_bindings=[bound(__file__),bound(output/'clock-handoff.json'),bound(output/'study.json')]
    require(fixed['wall_deadline_unix_seconds']==OLD_DEADLINE,'quality-clock.protocol: changed original allocation')
    control=read(output/'clock-handoff.json');receipt=read(output/'study.json')
    validate_handoff(receipt,control,output);source_check(receipt['source'])
    check_bound(receipt['config']);readiness=read(check_bound(receipt['readiness']))
    require_readiness(receipt['readiness']['path'],receipt['source'],readiness['allocation'])
    require(monitor(receipt['main']['directory'])==receipt['main'],'quality-clock.main: paused main changed')
    require(process(control['parent_pid'],control['parent_start_ticks']) in ('T','t'),
        'quality-clock.handoff: coordinator must be suspended')
    command=Path(f'/proc/{control["parent_pid"]}/cmdline').read_bytes().split(b'\0')
    require(b'run' in command and str(output).encode() in command and any(
        p.endswith(b'/operations/declarative_quality_study.py') for p in command),
        'quality-clock.handoff: foreign coordinator process')
    config=fixed['numerical_base'];guard=ResourceGuard(output,remaining(),config['limits']['matrix_rss_bytes'],
        config['limits']['min_free_bytes'])
    prior_guard=ResourceGuard(output,remaining(),config['limits']['matrix_rss_bytes'],
        config['limits']['min_free_bytes'],root_pid=control['parent_pid'])
    while process(control['child_pid'],control['child_start_ticks']) not in (None,'Z'):
        guard();prior_guard()
        combined=guard.last['rss']+prior_guard.last['rss']
        require(combined<config['limits']['matrix_rss_bytes'],'quality-clock.resources: combined handoff RSS ceiling')
        guard.peak_rss=max(guard.peak_rss,combined);time.sleep(1)
    baseline=Path(control['branch']);owner=baseline/'seed-113';job=read(owner/'state.json')
    require(job['status']==read(baseline/'state.json')['status']=='complete' and job['update']==128,
        'quality-clock.handoff: baseline child did not complete its endpoint')
    require(process(control['parent_pid'],control['parent_start_ticks']) in ('T','t'),
        'quality-clock.handoff: coordinator resumed unexpectedly')
    for ref in entry_bindings:check_bound(ref)
    os.kill(control['parent_pid'],signal.SIGKILL)
    while process(control['parent_pid'],control['parent_start_ticks']) not in (None,'Z'):
        guard();time.sleep(.1)
    guard()
    archive=output/'study-before-clock-extension.json';require(not archive.exists(),'quality-clock.handoff: already transferred')
    os.link(output/'study.json',archive)
    extension=dict(schema='declarative-cold-clock-extension-v1',previous_study=bound(archive),
        handoff=entry_bindings[1],controller=entry_bindings[0],
        previous_deadline=OLD_DEADLINE,deadline=DEADLINE,extra_seconds=3600,
        source_sha256=receipt['source']['code_sha256'],selection=receipt['selection'],
        baseline_checkpoint=bound(checked(job['latest_checkpoint'],owner)),
        started_unix=time.time(),scope='Time only; original scientific settings and frozen learning source.')
    extension_path=output/'clock-extension.json'
    with extension_path.open('x') as stream:json.dump(extension,stream,indent=2);stream.write('\n')
    os.chmod(extension_path,0o444)
    receipt['clock_extension']=bound(extension_path);receipt['wall_deadline_unix_seconds']=DEADLINE
    receipt['wall_allowance_seconds']=DEADLINE-receipt['began_unix']
    budget=Budget(output/'storage-budget.json');shared=output/'fresh-bank'
    panels=[read(check_bound(receipt['panels'][s])) for s in ('development','confirmation')]
    reservation(receipt['panels']);library=receipt['source']['native_library']
    began=time.monotonic();elapsed=time.time()-receipt['began_unix']
    def persist():
        receipt['seconds']=elapsed+time.monotonic()-began
        receipt['conservative_added_bytes']=budget.value['bytes'];write(output/'study.json',receipt)
    try:
        summary=summarize(baseline,panels,0,shared,guard);summary['initialization_seed']=113
        write(baseline/'quality-summary.json',summary);append_arm(receipt,baseline,summary)
        initial=initial_cache(baseline);model=initial['model_sha256']
        require(model!=receipt['initial_models']['71'],'quality-clock.initial: distinct seeds have identical models')
        receipt['initial_models']['113']=model
        budget.scan(output);budget.save();persist()
        for profile in SELECTED:
            guard();check_bound(receipt['selection']);branch=output/f'confirmation-seed113-{profile}'
            proof=prepare(branch,seed=113,profile=profile,stage='confirmation',
                panel=check_bound(receipt['panels']['confirmation']),library=library,shared=shared,
                seconds=remaining(),readiness=receipt['readiness']['path'],reservation_panels=receipt['panels'],
                selection=check_bound(receipt['selection']))
            require(proof['preserved']['model']==model,'quality-clock.initial: seed-113 initial model differs')
            install_initial(branch,initial);budget.scan(branch);budget.scan(shared);budget.save()
            receipt['active_arm']=dict(directory=str(branch),seed=113,first_update=0,endpoint=128,resumed_update=0)
            persist();print(dict(profile=profile,status='training',seed=113,updates=128),flush=True)
            child(branch,environment,budget,guard)
            summary=summarize(branch,panels,0,shared,guard);summary['initialization_seed']=113
            write(branch/'quality-summary.json',summary);append_arm(receipt,branch,summary)
            budget.scan(branch);budget.scan(shared);budget.save();persist()
            print(dict(profile=profile,status='complete',quality=summary['quality']['mean']),flush=True)
        confirmation={}
        for profile in ['baseline-8',*SELECTED]:
            pure=read(check_bound(receipt['pure_confirmation'][profile]))
            first=geometry_outcomes(read(check_bound(pure['evaluation']))['rows'],panels[1],config['strata'])
            row=next(r for r in receipt['arms'] if r['stage']=='confirmation' and r['profile']==profile)
            second=read(check_bound(row['summary']))['quality']
            confirmation[profile]=aggregate([first,{k:v['training_replicates'][0]
                for k,v in second['geometries'].items()}],config['strata'])
        for ref in (extension['controller'],extension['previous_study'],extension['handoff'],
                    extension['baseline_checkpoint'],receipt['clock_extension'],receipt['selection']):check_bound(ref)
        source_check(receipt['source'])
        for ref in receipt['continuation'].values():check_bound(ref)
        require(monitor(receipt['main']['directory'])==receipt['main'],'quality-clock.main: paused main changed')
        require(receipt['executed_updates']==1408 and len(receipt['arms'])==11,'quality-clock.accounting: incomplete study')
        receipt.update(status='complete',confirmation=confirmation,
            ranking=ranked({p:v['mean'] for p,v in confirmation.items()}),
            comparisons={p:paired(v,confirmation['baseline-8'],config['strata'],fixed['bootstrap_seed'],fixed['bootstrap_resamples'])
                for p,v in confirmation.items() if p!='baseline-8'},
            conclusion='Bounded early learning across two observed random initializations with paired fresh inputs; not convergence.')
        budget.scan(output);budget.save();guard()
        receipt.update(actual_added=budget.actual(output),resource=guard.last)
        receipt['cumulative_actual_added_bytes']=receipt['prior_owned_bytes']+receipt['readiness_added']['bytes']+receipt['actual_added']['bytes']
        persist();return receipt
    except BaseException as error:
        failed(receipt,error);persist();raise


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--environment',type=Path,required=True)
    args=parser.parse_args()
    try:result=run(args.output,args.environment)
    except (ValueError,RuntimeError,OSError,KeyError) as error:
        print(json.dumps(dict(code='quality-clock.failed',severity='error',stage='controller',
            message=str(error),entity=str(args.output))),file=sys.stderr);return 2
    print(json.dumps(dict(status=result['status'],executed_updates=result['executed_updates'])));return 0


if __name__=='__main__':raise SystemExit(main())
