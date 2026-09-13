"""Explicit serial reference for bounded execution measurements only."""
from copy import copy
from pathlib import Path
import os
import time

import torch

from blueprint_ppo_store import checked, read
from blueprint_ppo_update import digest
from declarative_learning_environment import Runtime, Stream, finish_stream, step_stream
from declarative_learning_evaluate import evaluate
import declarative_learning_policy as policy


class Serial:
    def __init__(self, config, library, guard, saved=None):
        self.config,self.guard=config,guard
        self.runtime=Runtime(config,library)
        self.streams=[Stream(self.runtime,i) for i in range(config['training']['streams'])] if saved is None else [Stream.restore(self.runtime,s) for s in saved]
        self.identities=[dict(pid=os.getpid(),affinity=sorted(os.sched_getaffinity(0)),mode='serial-reference')]
        self.max_step_concurrency=1
        self.case_cache={}

    def snapshot(self):
        return [s.snapshot() for s in self.streams]

    def collect(self, model, sampler):
        began=time.perf_counter()
        observations=[s.observe() for s in self.streams]
        before=self.snapshot()
        inputs=[s.runtime.prepare(o) for s,o in zip(self.streams,observations,strict=True)]
        samples=[]
        for _ in range(self.config['training']['chunk_actions']):
            self.guard()
            with torch.no_grad():selected=policy.sample(model,inputs,sampler)
            next_observations=[]
            for stream,observation,prepared,command in zip(self.streams,observations,inputs,selected,strict=True):
                sample,after=step_stream(stream,observation,prepared,command)
                samples.append(sample);next_observations.append(after)
            observations=next_observations
            inputs=[s.runtime.prepare(o) for s,o in zip(self.streams,observations,strict=True)]
        with torch.no_grad():values=policy.values(model,inputs)
        frontiers,completed=[],[]
        for stream,observation,prepared,value in zip(self.streams,observations,inputs,values,strict=True):
            frontier,result=finish_stream(stream,observation,prepared,value)
            frontiers.append(frontier)
            if result is not None:completed.append(result)
        return dict(schema='declarative-learning-chunk-v1',model_before=digest(model.state_dict()),native_before=before,
            samples=samples,frontiers=frontiers,completed=completed,collection_seconds=time.perf_counter()-began)

    def assign_starts(self, slots):
        from blueprint_ppo_update import require
        require(len(slots)==len(self.streams), 'declarative.curriculum-plan: serial slot coverage')
        for stream,slot in zip(self.streams,slots,strict=True):
            require(stream.session is None and slot['stream']==stream.index and slot['wave']==stream.episode+1,
                'declarative.curriculum-plan: cannot replace a live serial episode')
            stream.next_start=slot

    def evaluate(self, model, cases, seed, update, output, deadline, evaluation=None, *, uniform=False):
        rows=[]
        for case in cases:
            self.guard()
            identity=(case['directory'],case['files']['request']['sha256'],case['files']['initial']['sha256'])
            if identity not in self.case_cache:
                root=Path(case['directory'])
                self.case_cache[identity]=(read(checked(case['files']['request'],root)),read(checked(case['files']['initial'],root)))
            runtime=copy(self.runtime);runtime.request,runtime.original=self.case_cache[identity]
            command=dict(seed=seed,update=update,output=str(output),deadline=deadline,
                evaluation=evaluation or self.config['evaluation'],action_seed=case['action_seed'],sample=case['sample'],uniform=uniform)
            rows.append(evaluate(runtime,model,case,command))
        return rows

    def close(self):
        pass
