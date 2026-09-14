"""Bounded normal cold-run CPU/GPU telemetry, with optional isolated profiling."""
import argparse
import cProfile
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

BASE=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(BASE),str(BASE/'operations')]
from blueprint_execution_resources import process_tree
from blueprint_ppo_store import read,write
from blueprint_ppo_update import require
from masked_cycle_profile import summary
from masked_long_training import run


def child(args):
    if args.compile_model:
        require(not args.profile,'masked-cold-profile.mode: compilation and function profiling are separate probes')
        from masked_compiled_probe import compiled_run
        return compiled_run(args.output,args.environment,args.measurement)
    if not args.profile:return run(args.output,args.environment,args.stop_after)
    from masked_collection_profile import install
    install(args.measurement)
    from blueprint_execution_pool import BlueprintPool
    original=BlueprintPool.__init__
    def instrument(self,*a,**kw):
        kw['worker_entry']=Path(__file__).with_name('masked_profile_worker.py')
        return original(self,*a,**kw)
    BlueprintPool.__init__=instrument
    os.environ['AIC_MASKED_PROFILE_OUTPUT']=str(args.measurement)
    profile=cProfile.Profile()
    try:return profile.runcall(run,args.output,args.environment,args.stop_after)
    finally:write(args.measurement/'coordinator-profile.json',summary(profile))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True);parser.add_argument('--environment',type=Path,required=True)
    parser.add_argument('--measurement',type=Path,required=True);parser.add_argument('--stop-after',type=int)
    parser.add_argument('--profile',action='store_true');parser.add_argument('--child',action='store_true')
    parser.add_argument('--compile-model',action='store_true')
    args=parser.parse_args()
    launch=read(args.output/'launch.json')
    require(launch['masked_long']['probe'],'masked-cold-profile.scope: readiness runs only')
    if args.child:return child(args)
    require(not args.measurement.exists(),'masked-cold-profile.output: fresh telemetry directory required')
    args.measurement.mkdir(parents=True)
    command=[sys.executable,__file__,*sys.argv[1:],'--child']
    samples=[];previous={};last=time.monotonic();began=last
    with (args.measurement/'run.log').open('w') as log,(args.measurement/'gpu.csv').open('w') as gpu_log:
        trainer=subprocess.Popen(command,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        gpu=subprocess.Popen(['nvidia-smi','--query-gpu=timestamp,utilization.gpu,utilization.memory,memory.used,power.draw,clocks.sm',
            '--format=csv,noheader,nounits','--loop-ms=500'],stdout=gpu_log,stderr=subprocess.DEVNULL)
        try:
            while trainer.poll() is None:
                now=time.monotonic();tree=process_tree(trainer.pid)
                cpu=sum(max(0,row['cpu_seconds']-previous.get(pid,row['cpu_seconds'])) for pid,row in tree.items())
                samples.append(dict(unix=time.time(),seconds=now-began,cpu_cores=cpu/max(now-last,.001),
                    rss_bytes=sum(row['rss'] for row in tree.values()),processes=len(tree)))
                previous={pid:row['cpu_seconds'] for pid,row in tree.items()};last=now
                require(now-began<1000,'masked-cold-profile.deadline: bounded diagnostic exceeded 1000 seconds')
                time.sleep(.5)
            require(trainer.returncode==0,f'masked-cold-profile.child: exit {trainer.returncode}; see {args.measurement}/run.log')
        finally:
            gpu.terminate();gpu.wait(timeout=5)
            if trainer.poll() is None:
                os.killpg(trainer.pid,signal.SIGTERM)
                try:trainer.wait(timeout=10)
                except subprocess.TimeoutExpired:os.killpg(trainer.pid,signal.SIGKILL);trainer.wait()
            write(args.measurement/'cpu.json',dict(schema='masked-cpu-telemetry-v1',physical_cores=launch['allocation']['physical_core_count'],
                profiled=args.profile,rows=samples))
    print(dict(status='complete',measurement=str(args.measurement),profiled=args.profile),flush=True)


if __name__=='__main__':main()
