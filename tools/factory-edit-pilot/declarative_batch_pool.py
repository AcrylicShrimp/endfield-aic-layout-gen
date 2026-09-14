"""Ordered logical rollout batches on an explicitly sized physical worker pool."""
from copy import deepcopy
from pathlib import Path
import time

import torch

from blueprint_execution_pool import BlueprintPool
from blueprint_ppo_update import digest, require
import declarative_learning_policy as policy


class MultiplexPool(BlueprintPool):
    def __init__(self, config, library, cores, output, guard, saved=None):
        count=config['training']['streams'];physical=len(cores)
        require(type(count) is int and count>0 and 1<=physical<=count, 'declarative.batch-pool: physical workers must fit the positive logical stream count')
        if saved is not None:require([s['index'] for s in saved]==list(range(count)),'declarative.batch-pool: exact logical restore coverage')
        transport=deepcopy(config);transport['training']['streams']=physical;transport['logical_configuration']=config
        grouped=None if saved is None else [dict(index=i,streams=saved[i::physical]) for i in range(physical)]
        super().__init__(transport,library,cores,output,guard,saved=grouped,
            worker_entry=Path(__file__).with_name('declarative_batch_worker.py'),policy=policy)
        self.config=config

    def logical(self, commands):
        count=self.config['training']['streams'];physical=len(self.workers)
        require(len(commands)==count,'declarative.batch-pool: complete logical operation required')
        operation=commands[0]['operation']
        require(all(c['operation']==operation for c in commands),'declarative.batch-pool: one operation per exchange')
        grouped=[dict(operation=operation,commands=[dict(commands[j],logical_stream=j) for j in range(i,count,physical)]) for i in range(physical)]
        replies=self.exchange(grouped)
        mapped={r['logical_stream']:r['result'] for reply in replies for r in reply['replies']}
        require(sorted(mapped)==list(range(count)),'declarative.batch-pool: logical reply coverage')
        return [mapped[i] for i in range(count)]

    def snapshot(self):
        return [r['stream'] for r in self.logical([dict(operation='snapshot') for _ in range(self.config['training']['streams'])])]

    def assign_starts(self, slots):
        return self.logical([dict(operation='start',start=slot) for slot in slots])

    def collect(self, model, sampler):
        count=self.config['training']['streams'];began=time.perf_counter();before=digest(model.state_dict())
        replies=self.logical([dict(operation='begin') for _ in range(count)])
        inputs=[]
        for _ in range(self.config['training']['chunk_actions']):
            inputs.append([r['input'] for r in replies])
            with torch.no_grad():commands=self.policy.sample(model,inputs[-1],sampler)
            replies=self.logical([dict(operation='step',**c) for c in commands])
        with torch.no_grad():values=self.policy.values(model,[r['input'] for r in replies])
        chunks=self.logical([dict(operation='finish',value=float(v)) for v in values])
        require(digest(model.state_dict())==before,'declarative.batch-pool: behavior changed inside batch')
        samples=[dict(c['samples'][step],input=inputs[step][i])
            for step in range(self.config['training']['chunk_actions']) for i,c in enumerate(chunks)]
        require(all(s['input']['blueprint_sha256']==s['observation']['blueprint_sha256'] and
            s['input']['revision']==s['observation']['revision'] for s in samples), 'declarative.batch-pool: changed input ownership')
        return dict(schema=self.config['chunk_schema'],model_before=before,native_before=[c['native_before'] for c in chunks],
            samples=samples,
            frontiers=[c['frontier'] for c in chunks],completed=[c['completed'] for c in chunks if c['completed'] is not None],
            collection_seconds=time.perf_counter()-began)
