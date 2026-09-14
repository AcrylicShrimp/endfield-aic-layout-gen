"""Declarative worker on the established persistent shared-memory transport."""
from copy import copy
import os
from pathlib import Path
import torch

from blueprint_execution_worker import Worker as TransportWorker, main
from blueprint_ppo_store import checked, load, read, reference, write
from blueprint_ppo_update import require
from declarative_learning_environment import Runtime, Stream, native_step_stream, finish_stream
from declarative_learning_evaluate import evaluate
from declarative_learning_policy import ActorCritic


class Worker(TransportWorker):
    def __init__(self):
        super().__init__()
        self.case_cache = {}
        self.native_chunk = False

    def execute(self, command):
        operation = command['operation']
        if operation == 'initialize':
            self.runtime = Runtime(command['config'],command['library'])
            saved = command.get('saved')
            self.stream = Stream(self.runtime,command['index']) if saved is None else Stream.restore(self.runtime,saved)
            return dict(pid=os.getpid(),affinity=sorted(os.sched_getaffinity(0)),editor='declarative-edit-v1')
        if operation == 'panel':
            require(self.before is None and self.runtime.inputs is not None, 'declarative panel needs fresh generation')
            payload, metadata = self.runtime.inputs.acquire(self.stream.index,command['episode'])
            root = self.runtime.inputs.root
            record = read(checked(metadata['reference'],root))
            if record['schema']=='blueprint-fresh-episode-v2':
                # A held-out panel explicitly exports its public input files.
                # Normal training stores only the compressed actual input.
                archived=load(checked(record['files']['input'],root))
                folder=root/'panel-inputs'/f'episode-{metadata["identity"]["slot"]:08d}'
                files={}
                for key,value in (('request',payload['request']),('initial',payload['original']),('original',archived['source_graph'])):
                    path=folder/f'{key}.json';write(path,value);files[key]=reference(path,root)
            else:files={k:record['files'][k] for k in ('request','initial','original')}
            selected = copy(self.runtime)
            selected.request,selected.original = payload['request'],payload['original']
            session,_ = selected.open(64,60000)
            import json
            observation = json.loads(session.observation_json())
            selected.finish(session)
            return dict(case=dict(directory=str(root),generation_seed=metadata['generation_seed'],
                product=metadata['draw']['product'],rate=metadata['draw']['rate'],geometry_identity=metadata['geometry_sha256'],
                facilities=len(payload['original']['placements']),initial_metrics=observation['initial_metrics'],
                files=files,input=metadata))
        if operation == 'start':
            require(self.before is None and self.stream.session is None and
                command['start']['stream']==self.stream.index and command['start']['wave']==self.stream.episode+1,
                'declarative.curriculum-plan: cannot replace an unfinished episode')
            self.stream.next_start=command['start']
            return dict(assigned=True)
        if operation == 'begin':
            require(self.before is None, 'declarative worker has a partial chunk')
            self.observation = self.stream.observe()
            self.before, self.samples = self.stream.snapshot(), []
            self.stream.session.begin_learning_chunk(self.runtime.config['training']['chunk_actions'],
                bool(self.runtime.config['retention'].get('training_traces')))
            self.native_chunk = True
            self.observation,self.prepared = self.stream.runtime.observe(self.stream.session)
            return dict(input=self.prepared)
        if operation == 'step':
            require(self.before is not None and not self.observation['done'], 'declarative worker needs a live chunk')
            require(len(self.samples)<self.runtime.config['training']['chunk_actions'], 'declarative chunk overflow')
            sample, self.observation, self.prepared = native_step_stream(self.stream,self.observation,self.prepared,command)
            self.samples.append(sample)
            return dict(input=self.prepared)
        if operation == 'finish':
            require(len(self.samples)==self.runtime.config['training']['chunk_actions'], 'declarative chunk coverage')
            rows=self.stream.session.take_learning_chunk();self.native_chunk=False
            require(len(rows)==len(self.samples),'declarative.native-chunk: complete native evidence coverage')
            for sample,row in zip(self.samples,rows,strict=True):sample.update(row)
            self.observation=self.stream.observation()
            frontier, completed = finish_stream(self.stream,self.observation,self.prepared,command['value'])
            # The coordinator already owns the exact inputs used to select
            # these actions. Do not transmit the entire tensor/domain batch a
            # second time. Worker-local inputs remain available for failure
            # preservation until this successful finish response is formed.
            samples=[{k:v for k,v in row.items() if k!='input'} for row in self.samples]
            result = dict(native_before=self.before,samples=samples,frontier=frontier,completed=completed)
            self.before, self.samples = None, []
            return result
        if operation == 'snapshot':
            require(self.before is None, 'declarative snapshot requires a complete chunk')
            return dict(stream=self.stream.snapshot())
        if operation == 'model':
            require(self.before is None, 'declarative model update inside live chunk')
            if self.model is None:
                ppo = self.runtime.config['ppo']
                self.model = ActorCritic(ppo['width'],ppo['message_rounds'])
            self.model.load_state_dict(command['state'],strict=True); self.model.eval()
            return dict(loaded=True)
        if operation == 'evaluate':
            require(self.before is None and self.model is not None, 'declarative evaluation requires idle worker/model')
            case = command['case']
            identity = (case['directory'],case['files']['request']['sha256'],case['files']['initial']['sha256'])
            if identity not in self.case_cache:
                root = Path(case['directory'])
                self.case_cache[identity] = (read(checked(case['files']['request'],root)),read(checked(case['files']['initial'],root)))
            selected = copy(self.runtime)
            selected.request, selected.original = self.case_cache[identity]
            return dict(row=evaluate(selected,self.model,case,command))
        raise ValueError(f'unknown declarative worker operation: {operation}')

    def preserve(self, directory, error):
        # Preserve native evidence even if a charged step failed its Python
        # guard before its sample was appended. It is never a completed chunk.
        rows=self.stream.session.take_learning_chunk() if self.native_chunk else []
        self.native_chunk=False
        for sample,row in zip(self.samples,rows):sample.update(row)
        directory.mkdir(parents=True,exist_ok=True)
        torch.save(dict(schema='blueprint-worker-partial-v1',error=str(error),
            stream=self.stream.snapshot() if self.stream is not None else None,
            native_before=self.before,samples=self.samples,native_evidence=rows),directory/f'worker-{os.getpid()}.pt')


if __name__ == '__main__':
    main(Worker)
