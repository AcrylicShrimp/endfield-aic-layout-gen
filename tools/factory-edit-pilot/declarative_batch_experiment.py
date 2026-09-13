"""Registered equal-action batch experiment: verify, prepare, run and report."""
import argparse
import os
import signal
from copy import deepcopy
from pathlib import Path
import subprocess
import sys
import time

from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_ppo_store import ROOT, read, reference, sha, write
from blueprint_ppo_update import digest, require
from declarative_learning_launch import sources, validate


def configuration(protocol, seed, batch, root, *, diagnostic=False):
    config=read(ROOT/protocol['base_config']);streams=batch//32
    require(seed in protocol['seeds'] and batch in (256,1024),'declarative.batch-config: registered seed/batch')
    count=6 if diagnostic else protocol['actions_per_arm']//batch
    config.update(purpose='declarative-batch-readiness' if diagnostic else 'declarative-batch-comparison',
        execution='logical-stream-pool-v1',seeds=[seed])
    config['training'].update(first_update=0,updates=count,streams=streams)
    config['training_input'].update(directory=str((root/f'input-bank-{seed}').resolve()),canonical_streams=1,
        seed_base=(protocol['diagnostic_seed_base'] if diagnostic else protocol['input_seed_base'])+
            protocol['seeds'].index(seed)*protocol['input_seed_stride'])
    config['evaluation']['at']=[0,count] if diagnostic else [n//batch for n in protocol['evaluation_actions']]
    config['retention'].update(checkpoints=[0,3,count] if diagnostic else config['evaluation']['at'],
        evaluation_traces=[0,count])
    config['tracking']['group']=protocol['tracking_group']
    validate(config)
    return config


def prepare(protocol, library, panel, verification, output):
    output=Path(output).resolve();require(not output.exists(),f'declarative.batch-prepare: fresh directory required: {output}')
    receipt=read(verification)
    require(receipt['status']=='passed' and receipt['continuity_closed'] and receipt['protocol_sha256']==digest(protocol),
        'declarative.batch-prepare: matching continuity receipt required')
    allocation=cpu_allocation(8)
    source=sources(configuration(protocol,protocol['seeds'][0],256,output),library)
    require(source['code_sha256']==receipt['source']['code_sha256'] and source['native_sha256']==receipt['source']['native_sha256'],
        'declarative.batch-prepare: changed verified source/native')
    require(sha(panel)==receipt['panel_sha256'] and allocation['workers']==receipt['allocation']['workers'],
        'declarative.batch-prepare: changed evaluation/allocation')
    output.mkdir(parents=True)
    jobs=[]
    for seed,batch in protocol['order']:
        directory=output/f'seed-{seed}-batch-{batch}';directory.mkdir()
        config=configuration(protocol,seed,batch,output)
        launch=dict(schema='declarative-learning-launch-v1',config=config,source=source,allocation=allocation,
            panel=dict(path=str(Path(panel).resolve()),sha256=sha(panel)),
            verification=dict(path=str(Path(verification).resolve()),sha256=sha(verification)),
            forecast_seconds=receipt['forecast_by_batch'][str(batch)],created_unix_seconds=time.time(),
            batch_experiment=dict(protocol_sha256=digest(protocol),batch=batch,actions=protocol['actions_per_arm']))
        write(directory/'launch.json',launch)
        write(directory/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,
            launch_sha256=sha(directory/'launch.json'),jobs={}))
        jobs.append(dict(seed=seed,batch=batch,directory=str(directory),launch=reference(directory/'launch.json',output)))
    write(output/'experiment.json',dict(schema=protocol['schema'],protocol=protocol,jobs=jobs,source=source,
        panel=jobs and launch['panel'],verification=launch['verification']))
    write(output/'state.json',dict(status='ready',seconds=0.,completed=[]))
    return dict(status='ready',jobs=len(jobs),forecast_seconds=sum(receipt['forecast_by_batch'][str(j['batch'])] for j in jobs))


def run(output, environment):
    output=Path(output).resolve();experiment=read(output/'experiment.json');state=read(output/'state.json')
    require(state['status']!='complete','declarative.batch-run: experiment already complete')
    guard=ResourceGuard(output,experiment['protocol']['seconds']-state['seconds'],32*1024**3,16*1024**3)
    previous=state['seconds'];state['status']='running';write(output/'state.json',state)
    try:
        for job in experiment['jobs']:
            directory=Path(job['directory']);name=directory.name
            if name in state['completed']:continue
            guard();print(dict(stage='batch-arm',seed=job['seed'],batch=job['batch'],status='starting'),flush=True)
            with (directory/'run.log').open('a') as log:
                process=subprocess.Popen([sys.executable,str(Path(__file__).with_name('declarative_learning.py')),
                    'run','--output',str(directory),'--environment',str(Path(environment).resolve())],stdout=log,stderr=log,start_new_session=True)
                try:
                    while process.poll() is None:guard();time.sleep(.25)
                    require(process.returncode==0,f'declarative.batch-run: arm failed; see {directory / "run.log"}')
                finally:
                    if process.poll() is None:
                        os.killpg(process.pid,signal.SIGTERM)
                        try:process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid,signal.SIGKILL);process.wait()
            state['completed'].append(name);state['seconds']=previous+time.monotonic()-guard.began;write(output/'state.json',state)
            print(dict(stage='batch-arm',seed=job['seed'],batch=job['batch'],status='complete',seconds=state['seconds']),flush=True)
        state['status']='complete'
    except BaseException as error:
        state.update(status='failed',diagnostic=dict(code='declarative.batch-run-failed',severity='error',stage='training',entity=str(output),message=str(error)))
        raise
    finally:
        state['seconds']=previous+time.monotonic()-guard.began;write(output/'state.json',state)
    return state


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=('verify','prepare','run','report'))
    parser.add_argument('--protocol',type=Path,default=Path(__file__).with_name('declarative-batch-comparison.json'))
    parser.add_argument('--native-library',type=Path);parser.add_argument('--panel',type=Path)
    parser.add_argument('--verification',type=Path);parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--environment',type=Path,default=Path('.tracking.env'))
    args=parser.parse_args();protocol=read(args.protocol)
    require(protocol['schema']=='declarative-batch-comparison-v1' and protocol['actions_per_arm']==32768,
        'declarative.batch-protocol: registered equal-action protocol required')
    if args.command in ('verify','prepare') and (args.native_library is None or args.panel is None):parser.error('requires --native-library and --panel')
    if args.command=='verify':
        from declarative_batch_verify import verify
        result=verify(protocol,args.native_library,args.panel,args.output)
    elif args.command=='prepare':
        if args.verification is None:parser.error('prepare requires --verification')
        result=prepare(protocol,args.native_library,args.panel,args.verification,args.output)
    elif args.command=='run':result=run(args.output,args.environment)
    else:
        from declarative_batch_report import report
        result=report(args.output)
    print({k:v for k,v in result.items() if k in ('status','seconds','jobs','forecast_seconds','continuity_closed')})


if __name__=='__main__':main()
