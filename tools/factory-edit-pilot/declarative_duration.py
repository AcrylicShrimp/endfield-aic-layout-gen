"""Prepare and explicitly start one verified duration-bounded v2 continuation."""
import argparse
from copy import deepcopy
from pathlib import Path
import time

from blueprint_execution_resources import cpu_allocation
from blueprint_ppo_store import read, reference, sha, write
from blueprint_ppo_update import digest, require
from declarative_duration_import import initialize, parent
from declarative_learning_launch import sources, validate


def build(config, library, panel, parent_path, verification, output, *, diagnostic=False):
    output=Path(output).resolve();require(not output.exists(), f'declarative.duration: fresh output required: {output}')
    validate(config);value=parent(parent_path,config);receipt=sources(config,library)
    require(receipt['native_sha256']==value['launch']['source']['native_sha256'], 'declarative.duration: changed native binary')
    for name in ('declarative_learning_policy.py','declarative_learning_input.py','blueprint_edit_input.py','blueprint_ppo_batch.py','blueprint_ppo_update.py'):
        path='tools/factory-edit-pilot/'+name
        require(receipt['files_sha256'][path]==value['launch']['source']['files_sha256'][path], f'declarative.duration: changed model/credit contract {name}')
    require(sha(panel)==value['launch']['panel']['sha256'], 'declarative.duration: changed frozen evaluation panel')
    allocation=cpu_allocation(8)
    if not diagnostic:
        proof=read(verification)
        require(proof['status']=='passed' and proof['continuity_closed'] and proof['config_sha256']==digest(config) and
            proof['source_sha256']==receipt['code_sha256'] and proof['parent']==value['binding'] and proof['panel_sha256']==sha(panel),
            'declarative.duration: matching completed server readiness receipt required')
        require(proof['allocation']['workers']==allocation['workers'], 'declarative.duration: changed verified worker allocation')
    selected=deepcopy(config);selected['training_input']['directory']=str(output/'training-inputs')
    if diagnostic:
        selected['training_input']['seed_base']=614200000
    output.mkdir(parents=True)
    launch=dict(schema='declarative-learning-launch-v1',config=selected,source=receipt,allocation=allocation,
        panel=dict(path=str(Path(panel).resolve()),sha256=sha(panel)),continuation=value['binding'],
        verification=dict(path=str(Path(verification).resolve()),sha256=sha(verification)),
        created_unix_seconds=time.time(),forecast_seconds=config['duration']['seconds'],duration_ready=not diagnostic)
    write(output/'launch.json',launch);imported=initialize(value,launch,output);write(output/'continuation.json',imported)
    require(imported['preserved_sha256']=={k:digest(value['saved'][k]) for k in imported['preserved_sha256']},
        'declarative.duration: importer changed parent learning or episode state')
    write(output/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,
        launch_sha256=sha(output/'launch.json'),jobs={}))
    return dict(status='ready',output=str(output),parent=value['binding'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=('config','verify','prepare','run','report'))
    parser.add_argument('--config',type=Path,default=Path(__file__).with_name('declarative-four-hour.json'))
    parser.add_argument('--output',type=Path);parser.add_argument('--native-library',type=Path)
    parser.add_argument('--panel',type=Path);parser.add_argument('--parent',type=Path);parser.add_argument('--verification',type=Path)
    parser.add_argument('--environment',type=Path,default=Path('.tracking.env'))
    args=parser.parse_args();config=read(args.config);validate(config)
    if args.command=='config':
        result=dict(status='prepared-config',seed=config['seeds'][0],batch=config['training']['streams']*config['training']['chunk_actions'],
            additional_seconds=config['duration']['seconds'],launch='explicit verify, prepare and run commands are required; no automatic start')
    else:
        if args.output is None:parser.error('requires --output')
        if args.command in ('verify','prepare'):
            if any(v is None for v in (args.native_library,args.panel,args.parent)):parser.error('requires --native-library, --panel and --parent')
            if args.command=='prepare':
                if args.verification is None:parser.error('prepare requires --verification')
                result=build(config,args.native_library,args.panel,args.parent,args.verification,args.output)
            else:
                from declarative_duration_verify import verify
                result=verify(config,args.native_library,args.panel,args.parent,args.output,args.environment)
        elif args.command=='run':
            require(bool(read(args.output/'launch.json')['config'].get('duration')), 'declarative.duration: selected launch is not a duration run')
            from blueprint_duration_store import exclusive_run
            from declarative_duration_control import requested_stop
            from declarative_learning_run import run
            with exclusive_run(args.output), requested_stop() as requested:
                result=run(args.output,args.environment,requested)
                if result['status']=='complete':
                    from declarative_learning_report import report
                    report(args.output)
        else:
            from declarative_learning_report import report
            result=report(args.output)
    print({k:v for k,v in result.items() if k in ('status','output','seed','batch','additional_seconds','launch','continuity_closed')})


if __name__=='__main__':main()
