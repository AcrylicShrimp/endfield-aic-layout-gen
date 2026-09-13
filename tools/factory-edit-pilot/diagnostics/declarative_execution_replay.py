"""Exactly two retained PPO updates, including compact-source process restoration."""
import argparse
from copy import deepcopy
import os
from pathlib import Path
import signal
import sys
import time


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('source','parent','baseline','compact-parent','output'):p.add_argument('--'+name,type=Path,required=True)
    a=p.parse_args();sys.path.insert(0,str(a.source/'tools/factory-edit-pilot'))
    import torch
    from blueprint_ppo_store import checked,load,read,reference,save,write
    from blueprint_ppo_update import digest,require
    from blueprint_success_buffer import Evidence
    from declarative_execution_migrate import parent
    from declarative_learning_runtime import Learner
    from declarative_replay_storage import compact,references
    require(not a.output.exists(),'retained replay diagnostic requires fresh output');a.output.mkdir(parents=True)
    signal.signal(signal.SIGALRM,lambda *_: (_ for _ in ()).throw(TimeoutError('retained replay diagnostic deadline')));signal.alarm(300)
    torch.set_num_threads(1);torch.set_num_interop_threads(1)
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8');torch.use_deterministic_algorithms(True)
    value=parent(a.parent);saved=value['saved'];first=saved['update'];seed=saved['seed'];config=value['launch']['config']
    owner=a.baseline/f'seed-{seed}';job=read(owner/'state.json');compacted=read(a.compact_parent/'manifest.json')
    require(compacted['parent']==value['binding'],'retained replay diagnostic has stale compact sources')
    job['replay_sources']=deepcopy(compacted['replay_sources'])
    for ref in job['replay_sources'].values():
        path=checked(ref,a.compact_parent);target=a.output/ref['path'];target.parent.mkdir(parents=True,exist_ok=True);os.link(path,target)
    learner=Learner(seed,config,saved['protocol_sha256'],saved);evidence=Evidence(a.output,job);rows=[]
    for ordinal in range(first+1,first+3):
        original=read(checked(job['records'][ordinal-1],owner));payload=load(checked(original['chunk'],owner))
        expected=load(checked(original['checkpoint'],owner))
        began=time.perf_counter();learner.buffer.hydrate(evidence,config['strata']);hydrate_seconds=time.perf_counter()-began
        ingest=learner.buffer.ingest;timing={}
        def measured(*args,**kwargs):
            began=time.perf_counter();result=ingest(*args,**kwargs);timing['ingest']=time.perf_counter()-began;return result
        learner.buffer.ingest=measured
        result=learner.update(payload,evidence)
        # Collection's RNG transition is outside a retained-input optimizer replay.
        # Its exact CPU/GPU draw order is checked by the separate policy probe.
        actual=learner.checkpoint(expected['streams'])
        equal={key:digest(actual[key])==digest(expected[key]) for key in ('model','actor_optimizer','critic_optimizer','shuffle_rng','buffer')}
        require(all(equal.values()),f'retained replay changed learner transition at {ordinal}: {equal}')
        # JSON records represent tuple references as arrays.
        require(digest(result['events'])==digest(original['events']),'retained replay changed ordered admission/eviction events')
        path=a.output/f'replay/{ordinal:04d}.pt.gz';retained=compact(payload,references(learner.buffer.state).get(ordinal,set()))
        save(path,retained);job['replay_sources'][str(ordinal)]=reference(path,a.output)
        evidence.published(ordinal,retained)
        rows.append(dict(update=ordinal,exact_equal=equal,events_equal=True,events=len(result['events']),
            ingest_seconds=timing['ingest'],cold_hydration_seconds=hydrate_seconds,model_sha256=actual['model_sha256']))
        if ordinal==first+1:
            path=a.output/'restored.pt.gz';save(path,actual);learner=Learner(seed,config,actual['protocol_sha256'],load(path));evidence=Evidence(a.output,job)
        print(rows[-1],flush=True)
        del payload,expected,actual,retained
    write(a.output/'report.json',dict(schema='declarative-retained-replay-proof-v1',status='passed',parent=value['binding'],rows=rows,
        process_restoration=True,scope='One exact two-update replay; model, both Adam states, shuffle RNG, complete buffer and ordered replacement events match the retained predecessor.'))
    signal.alarm(0)


if __name__=='__main__':main()
