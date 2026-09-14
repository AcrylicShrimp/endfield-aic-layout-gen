"""Isolated completed-parent normal cycles, optionally with bounded function profiles."""
import argparse
import atexit
from copy import deepcopy
import cProfile
import os
from pathlib import Path
import pstats
import sys

BASE=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(BASE))

from blueprint_ppo_store import checked,load,read,sha,write
from blueprint_ppo_update import require


def summary(profile):
    rows=[dict(file=f,line=l,function=n,calls=c,self_seconds=s,cumulative_seconds=t)
        for (f,l,n),(_,c,s,t,_) in pstats.Stats(profile).stats.items()]
    return sorted(rows,key=lambda r:r['cumulative_seconds'],reverse=True)[:50]


def prepare(parent, output, updates, input_source):
    from blueprint_execution_resources import cpu_allocation
    from declarative_execution_migrate import initialize
    from declarative_learning_launch import sources,validate
    launch=read(parent/'launch.json');matrix=read(parent/'state.json')
    owner=parent/f'seed-{launch["config"]["seeds"][0]}';job=read(owner/'state.json')
    saved=load(checked(job['latest_checkpoint'],owner))
    require(matrix['status']==job['status']=='complete' and saved['update']==job['update']
        and saved['protocol_sha256']==sha(parent/'launch.json') and all(s['native'] is None for s in saved['streams']),
        'masked-cycle.parent: completed bound wave required')
    require(not output.exists() and updates>0 and updates%2==0,'masked-cycle.output: fresh bounded wave required')
    binding=dict(directory=str(parent),launch_sha256=sha(parent/'launch.json'),job_sha256=sha(owner/'state.json'))
    value=dict(directory=parent,owner=owner,launch=launch,matrix=matrix,job=job,saved=saved,binding=binding)
    config=deepcopy(launch['config']);first=saved['update']
    config['training'].update(first_update=first,updates=first+updates)
    config['evaluation']['at']=[first]
    config['duration'].update(seconds=600.,finish_grace_seconds=300.)
    config['limits'].update(arm_seconds=900.,matrix_seconds=900.)
    config['tracking']['mode']='disabled'
    if input_source:config['training_input']['directory']=str(input_source)
    config['retention']['checkpoints']=sorted({first,first+updates})
    validate(config)
    source=sources(config,Path(__file__).resolve().parents[3]/'target/release/libaic_pilot_native.so')
    compact_dir=output.with_name(output.name+'-sources');compact_dir.mkdir(parents=True)
    compacted=dict(status='passed',parent=binding,replay_sources=job['replay_sources'],checkpoint_chunks=job['checkpoint_chunks'])
    for ref in compacted['replay_sources'].values():
        target=compact_dir/ref['path'];target.parent.mkdir(parents=True,exist_ok=True)
        os.link(checked(ref,owner),target)
    write(compact_dir/'manifest.json',compacted)
    initialize(value,config,source,compacted,compact_dir,compact_dir/'manifest.json',output,
        cpu_allocation(config['execution_options']['workers']),diagnostic=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--parent',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--input-source',type=Path)
    parser.add_argument('--updates',type=int,default=4)
    parser.add_argument('--environment',type=Path,required=True)
    parser.add_argument('--profile',action='store_true')
    args=parser.parse_args()
    prepare(args.parent.resolve(),args.output.resolve(),args.updates,args.input_source)
    import declarative_learning_run as runner
    if args.profile:
        from blueprint_execution_pool import BlueprintPool
        original=BlueprintPool.__init__
        def instrument(self,*a,**kw):
            kw['worker_entry']=Path(__file__).with_name('masked_profile_worker.py')
            return original(self,*a,**kw)
        BlueprintPool.__init__=instrument
        os.environ['AIC_MASKED_PROFILE_OUTPUT']=str(args.output.resolve())
        profile=cProfile.Profile()
        try:profile.runcall(runner.run,args.output,args.environment)
        finally:write(args.output/'coordinator-profile.json',summary(profile))
    else:runner.run(args.output,args.environment)


if __name__=='__main__':main()
