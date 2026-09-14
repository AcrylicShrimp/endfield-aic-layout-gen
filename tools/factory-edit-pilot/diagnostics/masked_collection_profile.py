"""Diagnostic-only collection attribution; no per-action trace or normal-run hooks."""
import atexit
import gc
from functools import wraps
from pathlib import Path
import time

from blueprint_ppo_store import write


def install(output):
    import blueprint_execution_pool as execution
    import declarative_learning_policy as policy
    from declarative_batch_pool import MultiplexPool
    from ppo_harness_transport import Reader

    rows=[];active=None;phases=[];gc_started={}
    def garbage_collection(event, info):
        generation=info['generation']
        if event=='start':
            gc_started[generation]=(time.perf_counter(),active,phases[-1] if phases else 'outside')
        elif generation in gc_started:
            began,target,phase=gc_started.pop(generation)
            if target is not None:
                key=f'gc_generation_{generation}/{phase}'
                row=target.setdefault(key,dict(calls=0,seconds=0.,collected=0,uncollectable=0))
                row['calls']+=1;row['seconds']+=time.perf_counter()-began
                row['collected']+=info['collected'];row['uncollectable']+=info['uncollectable']
    gc.callbacks.append(garbage_collection)
    def instrument(owner,name,label):
        original=getattr(owner,name)
        @wraps(original)
        def measured(*args,**kwargs):
            if active is None:return original(*args,**kwargs)
            key=label(*args,**kwargs) if callable(label) else label
            began=time.perf_counter()
            phases.append(key)
            try:return original(*args,**kwargs)
            finally:
                phases.pop()
                row=active.setdefault(key,dict(calls=0,seconds=0.))
                row['calls']+=1;row['seconds']+=time.perf_counter()-began
        setattr(owner,name,measured)
    instrument(execution,'send','command_encode_send')
    instrument(execution,'receive','response_receive')
    instrument(Reader,'feed','response_decode')
    instrument(execution.BlueprintPool,'exchange',lambda self,commands:'exchange_'+commands[0]['operation'])
    instrument(policy,'sample','policy_sample')
    instrument(policy,'values','policy_values')
    instrument(execution.selectors.EpollSelector,'select','selector_wait')
    original=MultiplexPool.collect
    @wraps(original)
    def collect(self,*args,**kwargs):
        nonlocal active
        active={};began=time.perf_counter()
        try:return original(self,*args,**kwargs)
        finally:
            rows.append(dict(collection=len(rows)+1,seconds=time.perf_counter()-began,stages=active))
            active=None
    MultiplexPool.collect=collect
    atexit.register(lambda:write(Path(output)/'collection-profile.json',dict(
        schema='masked-collection-profile-v1',nested_timings=True,rows=rows)))
