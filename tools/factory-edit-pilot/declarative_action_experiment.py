"""Explicit learning-free action experiment with bounded, isolated child processes."""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from declarative_action_runtime import ROOT, Runtime, bound, check, read, reference, sha, write


def worker(job):
    os.sched_setaffinity(0,{job['core']})
    spec=bound(job['spec'])
    library=job['library']['path']
    check(sha(library)==job['library']['sha256'],'library-changed','Native binary changed',library)
    output=Path(job['output'])
    output.mkdir(parents=True,exist_ok=True)
    if job['kind']=='compare':
        from declarative_action_compare import compare_case
        result=compare_case(spec,library,job['case'],output/'comparison')
    else:
        import torch
        from random_factory_generation import generate, geometry_identity, resolve_spec
        from native_factory_runtime import FactoryRuntime
        torch.set_num_threads(1)
        generator=resolve_spec(ROOT/spec['generator'])
        check('frozen_starts' not in generator,'generator-contract','Fixed factory fallback is forbidden in this experiment')
        generator['native_library']=library
        catalogs=output/'catalogs.json'
        write(catalogs,generator['catalogs'])
        runtime=FactoryRuntime(library,catalogs)
        began=time.monotonic()
        def guard():
            check(time.monotonic()-began<spec['panel']['generation_seconds'],'generation-budget','Fresh generation exceeded its allowance',output)
        seed=spec['panel']['seed_base']+job['attempt']
        case,record=generate(runtime,generator,seed,output/'generated',guard)
        result=dict(status=record['status'],generation_seed=seed,attempt=job['attempt'],
            generation=reference(output/'generated/generation.json'),draw=record['draw'])
        if case:
            original=read(case['initial']);request=read(case['request'])
            edit=Runtime(spec,library,verify_build=False)
            try:
                new=edit.open(request,original)
                base=edit.observe(new)
                old=edit.open(request,original,old=True)
                previous=edit.observe(old)
                check(base['blueprint_sha256']==previous['blueprint_sha256'],'admission-identity','Old/new admission disagree on original blueprint')
                new.finish_json();old.finish_json()
            except ValueError as error:
                result.update(status='editor-admission-rejected',diagnostic=str(error))
            else:
                result.update(case=dict(product=record['draw']['product'],generation_seed=seed,rate=record['draw']['rate'],
                    geometry_identity=geometry_identity(original),geometry_changed=record['geometry_changed'],
                    facilities=len(original['placements']),initial_metrics=base['initial_metrics'],
                    files={k:reference(case[k]) for k in ('request','initial','original')},generation=result['generation']))
    result['worker']=dict(pid=os.getpid(),core=job['core'])
    write(output/'result.json',result)
    return result


def terminate(process):
    if process.poll() is None:
        try:
            os.killpg(process.pid,signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def parallel(jobs, spec, output, library, config_ref, deadline):
    cores=sorted(os.sched_getaffinity(0))
    workers=spec['limits']['workers']
    check(1<=workers<=8 and len(cores)>=workers,'workers','Distinct worker CPU cores are unavailable')
    pending=list(enumerate(jobs));active={};results=[]
    output.mkdir(parents=True,exist_ok=True)
    library_ref=reference(library)
    try:
        while pending or active:
            check(time.monotonic()<deadline,'wall-budget','Experiment command wall allowance reached',output)
            free=[i for i in range(workers) if i not in active]
            for slot in free:
                if not pending:
                    break
                index,job=pending.pop(0)
                directory=output/f'job-{index:03d}'
                directory.mkdir()
                payload=dict(job,core=cores[slot],spec=config_ref,library=library_ref,output=str(directory.resolve()))
                write(directory/'job.json',payload)
                stdout=(directory/'stdout.log').open('w');stderr=(directory/'stderr.log').open('w')
                process=subprocess.Popen([sys.executable,str(Path(__file__).resolve()),'_worker','--job',str(directory/'job.json')],
                    stdout=stdout,stderr=stderr,start_new_session=True,
                    env=dict(os.environ,OMP_NUM_THREADS='1',MKL_NUM_THREADS='1',OPENBLAS_NUM_THREADS='1'))
                active[slot]=dict(process=process,began=time.monotonic(),directory=directory,stdout=stdout,stderr=stderr,index=index,payload=payload)
            for slot,item in list(active.items()):
                process=item['process'];directory=item['directory']
                reason=None
                if time.monotonic()-item['began']>spec['limits']['worker_seconds']:
                    reason='worker-time'
                try:
                    status=Path(f'/proc/{process.pid}/status').read_text()
                    rss=next((int(line.split()[1])*1024 for line in status.splitlines() if line.startswith('VmRSS:')),0)
                    if rss>spec['limits']['worker_rss_bytes']:
                        reason='worker-rss'
                except FileNotFoundError:
                    pass
                if reason:
                    terminate(process)
                    write(directory/'resource-failure.json',dict(code=f'declarative-study.{reason}',severity='error',
                        stage='action-experiment',message='Isolated worker exceeded its registered resource guard',entity=process.pid))
                code=process.poll()
                if code is None:
                    continue
                item['stdout'].close();item['stderr'].close()
                if code==0 and (directory/'result.json').exists():
                    result=read(directory/'result.json')
                else:
                    result=dict(status='worker-failed',job=item['payload'],code=code,reason=reason,
                        stderr=reference(directory/'stderr.log'))
                results.append((item['index'],result))
                del active[slot]
                write(output/'progress.json',dict(results=[r for _,r in sorted(results)],active=len(active),pending=len(pending)))
                print(json.dumps(dict(stage=jobs[0]['kind'],completed=len(results),total=len(jobs),status=result['status'])),flush=True)
            if active:
                time.sleep(.1)
    finally:
        for item in active.values():
            terminate(item['process']);item['stdout'].close();item['stderr'].close()
    return [r for _,r in sorted(results)]


def prepare(spec, library, output, config_ref, deadline):
    output.mkdir(parents=True)
    attempts=[];cases=[];identities=set()
    maximum=spec['panel']['max_attempts'];target=spec['panel']['cases']
    for start in range(0,maximum,spec['limits']['workers']):
        jobs=[dict(kind='generate',attempt=i) for i in range(start,min(maximum,start+spec['limits']['workers']))]
        results=parallel(jobs,spec,output/f'wave-{start:03d}',library,config_ref,deadline)
        for result in results:
            if result['status']=='accepted':
                case=result['case']
                if case['geometry_identity'] in identities:
                    result['status']='duplicate-geometry'
                elif len(cases)<target:
                    identities.add(case['geometry_identity']);case['index']=len(cases);cases.append(case)
                else:
                    result['status']='surplus-admitted'
            attempts.append(result)
        write(output/'progress.json',dict(attempts=attempts,cases=cases))
        if len(cases)==target:
            break
    result=dict(schema='declarative-action-panel-v1',status='complete' if len(cases)==target else 'incomplete',
        cases=cases,attempts=attempts,selection='first distinct generated inputs admitted by both editors; no editing outcome selection',
        coverage=dict(products=dict(Counter(c['product'] for c in cases)),facilities=[c['facilities'] for c in cases],
            rates=[c['rate'] for c in cases],geometry_changed=sum(c['geometry_changed'] for c in cases),
            statuses=dict(Counter(r['status'] for r in attempts))))
    write(output/'panel.json',result)
    check(result['status']=='complete','panel-incomplete','Fresh input admission exhausted its registered attempts',output)
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=('probe','prepare','compare','_worker'))
    parser.add_argument('--spec',type=Path)
    parser.add_argument('--native-library',type=Path)
    parser.add_argument('--output',type=Path)
    parser.add_argument('--panel',type=Path)
    parser.add_argument('--probe-report',type=Path)
    parser.add_argument('--job',type=Path)
    args=parser.parse_args()
    if args.command=='_worker':
        try:
            worker(read(args.job))
        except BaseException as error:
            print(json.dumps(dict(code='declarative-study.worker-failed',severity='error',stage='action-experiment',
                message=str(error),entity=str(args.job))),file=sys.stderr)
            raise
        return
    check(args.spec and args.native_library and args.output,'arguments','--spec, --native-library and --output are required')
    spec=read(args.spec)
    check(spec['schema']=='declarative-action-experiment-v1','schema','Unsupported experiment specification',args.spec)
    check(not args.output.exists(),'output-exists','Use a fresh output directory',args.output)
    runtime=Runtime(spec,args.native_library)
    deadline=time.monotonic()+spec['limits']['wall_seconds']
    started=time.monotonic()
    config_ref=reference(args.spec)
    if args.command=='probe':
        from declarative_action_probe import probe
        result=probe(spec,args.native_library,args.output.resolve())
        print(json.dumps(dict(status=result['status'],seconds=result['seconds'],scenarios=len(result['scenarios']))))
        return
    check(args.probe_report is not None,'probe-required','Run the isolated semantic gate and pass --probe-report')
    gate=read(args.probe_report)
    check(gate['status']=='passed' and gate['native_build']==runtime.build,'probe-stale','Semantic gate must match this native build',args.probe_report)
    options=spec['comparison']
    planned=spec['panel']['cases']*(options['streams']*options['decisions']*len(options['arms'])+
        options['independent_proposals']*len(options['independent_arms']))
    check(planned<=spec['limits']['total_decisions'],'decision-budget','Registered comparison exceeds total decision cap')
    if args.command=='prepare':
        result=prepare(spec,args.native_library,args.output.resolve(),config_ref,deadline)
    else:
        from declarative_action_compare import summaries
        check(args.panel is not None,'panel-required','Pass the prepared actual input panel with --panel')
        panel=read(args.panel)
        check(panel['status']=='complete' and len(panel['cases'])==spec['panel']['cases'],'panel-incomplete','Panel is incomplete',args.panel)
        jobs=[dict(kind='compare',case=case) for case in panel['cases']]
        args.output.mkdir(parents=True)
        write(args.output/'launch.json',dict(spec=config_ref,panel=reference(args.panel),probe=reference(args.probe_report),
            library=reference(args.native_library),planned_decisions=planned,runtime_inputs=runtime.inputs,started_at=time.time()))
        records=parallel(jobs,spec,args.output/'workers',args.native_library,config_ref,deadline)
        check(all(r['status']=='complete' for r in records),'comparison-incomplete','One or more comparison workers failed',args.output)
        cases=[bound(r['report']) for r in records]
        result=dict(schema='declarative-action-experiment-result-v1',status='complete',seconds=time.monotonic()-started,
            trained=False,panel=reference(args.panel),probe=reference(args.probe_report),spec=config_ref,
            records=records,summary=summaries(cases,options),native_build=runtime.build,runtime_inputs=runtime.inputs,
            coverage=panel['coverage'],source={str(p.relative_to(ROOT)):sha(p) for p in Path(__file__).parent.glob('declarative_*.py')},
            controls=[dict(case=c['case']['index'],**row) for c in cases for row in c['controls']])
        write(args.output/'report.json',result)
    print(json.dumps(dict(status=result['status'],seconds=time.monotonic()-started,output=str(args.output.resolve()))))


if __name__=='__main__':
    try:
        main()
    except Exception as error:
        print(json.dumps(dict(code='declarative-study.failed',severity='error',stage='action-experiment',message=str(error))),file=sys.stderr)
        raise SystemExit(1)
