"""Isolated two-update execution measurement; never imported by normal training."""
import argparse
from copy import deepcopy
import inspect
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',type=Path,required=True);p.add_argument('--parent',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--inputs',type=Path,required=True)
    p.add_argument('--workers',type=int,default=8);p.add_argument('--behavior-device',default='cpu')
    p.add_argument('--compact-parent',type=Path)
    p.add_argument('--restore-midpoint',action='store_true')
    p.add_argument('--seconds',type=int,default=900)
    a=p.parse_args();sys.path.insert(0,str(a.source/'tools/factory-edit-pilot'))
    import torch
    from blueprint_execution_resources import ResourceGuard,cpu_allocation,process_tree
    from blueprint_ppo_store import checked,load,read,reference,save,sha,write
    from blueprint_ppo_update import digest,require
    from blueprint_success_buffer import Evidence
    from declarative_learning_launch import pool
    from declarative_learning_run import register_inputs
    from declarative_learning_runtime import Learner,publish,retain
    import declarative_learning_policy as policy
    import declarative_learning_runtime as runtime
    from declarative_learning_buffer import Buffer
    require(not a.output.exists(),'execution probe requires a fresh output')
    require(read(a.parent/'state.json')['status']=='paused','execution probe requires a paused main run')
    a.output.mkdir(parents=True);a.output=a.output.resolve()
    allocation=cpu_allocation(a.workers);torch.set_num_threads(1);torch.set_num_interop_threads(1)
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8');torch.use_deterministic_algorithms(True)
    signal.signal(signal.SIGALRM,lambda *_: (_ for _ in ()).throw(TimeoutError('isolated execution probe deadline')))
    signal.alarm(a.seconds)
    original=read(a.parent/'launch.json');seed=original['config']['seeds'][0]
    source=a.parent/f'seed-{seed}';owner=a.output/f'seed-{seed}';owner.mkdir()
    old=read(source/'state.json');require(old['status']=='paused','execution probe requires a durable paused learner')
    saved=load(checked(old['latest_checkpoint'],source));first=saved['update'];job=deepcopy(old)
    selected=deepcopy(original['config']);selected['training_input']['directory']=str(a.inputs.resolve())
    selected['training'].update(first_update=first,updates=first+2)
    selected['execution_options']=dict(workers=a.workers,behavior_device=a.behavior_device)
    selected['retention'].update(checkpoints=[first,first+1,first+2]);selected['tracking']['mode']='disabled'
    launch=dict(schema='declarative-execution-probe-v1',config=selected,source_root=str(a.source.resolve()),
        parent=dict(directory=str(a.parent.resolve()),launch_sha256=sha(a.parent/'launch.json'),checkpoint=old['latest_checkpoint']),
        library=original['source']['native_library'],diagnostic_script_sha256=sha(__file__))
    write(a.output/'launch.json',launch);identity=sha(a.output/'launch.json')
    job.update(status='ready',seconds=0.,retained_bytes=0,retained_files={})
    for name,size in old['retained_files'].items():
        path=Path(name);target=owner/path.relative_to(source)
        if a.compact_parent and path.parent.name in ('chunks','replay'):continue
        target.parent.mkdir(parents=True,exist_ok=True);os.link(path,target)
        job['retained_files'][str(target)]=size;job['retained_bytes']+=size
    if a.compact_parent:
        compacted=read(a.compact_parent/'manifest.json')
        require(compacted['status']=='passed' and compacted['parent']['checkpoint']==old['latest_checkpoint'],
                'execution probe compact sources differ from parent')
        job.update(replay_sources=deepcopy(compacted['replay_sources']),checkpoint_chunks=deepcopy(compacted['checkpoint_chunks']))
        for ref in job['replay_sources'].values():
            path=checked(ref,a.compact_parent);target=owner/ref['path'];target.parent.mkdir(parents=True,exist_ok=True)
            os.link(path,target);job['retained_files'][str(target)]=path.stat().st_size;job['retained_bytes']+=path.stat().st_size
    saved['protocol_sha256']=identity
    guard=ResourceGuard(a.output,a.seconds,selected['limits']['matrix_rss_bytes'],selected['limits']['min_free_bytes'])
    phases={};counts={}
    def measured(label,fn):
        def call(*args,**kwargs):
            began=time.perf_counter()
            try:return fn(*args,**kwargs)
            finally:
                phases[label]=phases.get(label,0.)+time.perf_counter()-began
                counts[label]=counts.get(label,0)+1
        return call
    policy.sample=measured('policy_sample',policy.sample)
    policy.values=measured('policy_values',policy.values)
    Buffer.eligible=measured('replay_eligible',Buffer.eligible)
    Buffer.ingest=measured('replay_ingest',Buffer.ingest)
    runtime.save=measured('durable_save',runtime.save)
    learner=Learner(seed,selected,identity,saved);del saved
    evidence=Evidence(owner,job);rows=[];workers=None;monitor=None;restoration=None
    try:
        with (a.output/'gpu.csv').open('w') as gpu:
            monitor=subprocess.Popen(['nvidia-smi','--query-gpu=timestamp,utilization.gpu,utilization.memory,memory.used,power.draw',
                '--format=csv,noheader','-lms','500'],stdout=gpu,stderr=subprocess.DEVNULL)
        workers=pool(selected,original['source']['native_library'],allocation,a.output/'workers',guard,saved=learner.streams)
        os.sched_setaffinity(0,allocation['coordinator'])
        for step in range(2):
            guard();began=time.perf_counter();before=dict(phases);cpu_before=sum(p['cpu_seconds'] for p in process_tree(os.getpid()).values())
            before_workers=dict(workers.phase_seconds)
            payload=workers.collect(learner.behavior_copy(),learner.sampler)
            register_inputs(payload,job);record=learner.update(payload,evidence);snapshots=workers.snapshot()
            options=dict(workers=workers)
            if 'evidence' in inspect.signature(publish).parameters:options['evidence']=evidence
            publish(learner,payload,record,snapshots,owner,job,**options);retain(learner,owner,job)
            seconds=time.perf_counter()-began;cpu_after=sum(p['cpu_seconds'] for p in process_tree(os.getpid()).values())
            row=dict(update=learner.ordinal,seconds=seconds,cpu_seconds=cpu_after-cpu_before,
                phases={k:v-before.get(k,0.) for k,v in phases.items()},collection=record['collection_seconds'],
                learner=record['ppo']['execution']['update_seconds'],publication=record['publication_seconds'],
                inputs=record.get('quality'),replay=record['buffer'],
                completed=len(payload['completed']),model_before=record['ppo']['model_before'],model_after=record['ppo']['model_after'],
                sampler_sha256=digest(learner.sampler.get_state()),logical_streams=len(snapshots),
                worker_phases={k:v-before_workers.get(k,0.) for k,v in workers.phase_seconds.items()},
                record=job['records'][-1])
            rows.append(row);write(a.output/'progress.json',dict(rows=rows));print({k:v for k,v in row.items() if k in ('update','seconds','phases','collection','learner','publication')},flush=True)
            if step==0 and a.restore_midpoint:
                restore_began=time.perf_counter();workers.close();workers=None
                checkpoint=load(checked(job['latest_checkpoint'],owner))
                require(all(s['native'] is not None and s['policy_actions']==32 for s in checkpoint['streams']),
                        'execution probe missing live midpoint')
                learner=Learner(seed,selected,identity,checkpoint);evidence=Evidence(owner,job)
                workers=pool(selected,original['source']['native_library'],allocation,a.output/'restored-workers',guard,saved=learner.streams)
                from declarative_learning_verify_curriculum import compare_restore
                compare_restore(checkpoint['streams'],workers.snapshot())
                restoration=dict(streams=len(learner.streams),seconds=time.perf_counter()-restore_began)
        require(all(s['native'] is None for s in workers.snapshot()),'execution probe must finish full episodes')
    finally:
        if workers:workers.close()
        if monitor:
            monitor.terminate()
            try:monitor.wait(timeout=3)
            except subprocess.TimeoutExpired:monitor.kill();monitor.wait()
        os.sched_setaffinity(0,allocation['allowed']);signal.alarm(0)
    write(a.output/'report.json',dict(schema='declarative-execution-probe-v1',status='passed',rows=rows,
        config=selected,allocation=allocation,resource=guard.last,seconds=time.monotonic()-guard.began,restoration=restoration,
        input_access='Both variants share explicit diagnostic generated inputs; the second may reuse construction. The parent remains paused.',
        scope='Two actual updates; sparse boundary timers and bounded GPU observations only. Cold replay hydration is reported separately.'))


if __name__=='__main__':main()
