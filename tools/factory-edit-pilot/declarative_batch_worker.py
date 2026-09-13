"""Several persistent logical native streams on one physical transport worker."""
from pathlib import Path
from types import SimpleNamespace
import os

from blueprint_execution_worker import main
from blueprint_ppo_update import require
from declarative_learning_environment import Runtime, Stream
from declarative_learning_worker import Worker


class MultiplexWorker:
    def __init__(self):
        self.children={}

    def execute(self, command):
        if command['operation']=='initialize':
            index=command['index'];self.stream=SimpleNamespace(index=index)
            config=command['config']['logical_configuration']
            count=command['config']['training']['streams']
            runtime=Runtime(config,command['library'])
            saved=command['saved']
            saved={} if saved is None else {s['index']:s for s in saved['streams']}
            indices=list(range(index,config['training']['streams'],count))
            require(not saved or sorted(saved)==indices,'declarative.batch-worker: restored logical ownership')
            for logical in indices:
                child=Worker();child.runtime=runtime
                child.stream=Stream(runtime,logical) if logical not in saved else Stream.restore(runtime,saved[logical])
                self.children[logical]=child
            return dict(pid=os.getpid(),affinity=sorted(os.sched_getaffinity(0)),editor='declarative-edit-v1',logical_streams=indices)
        if 'commands' in command:
            replies=[]
            for request in command['commands']:
                index=request['logical_stream']
                require(index in self.children,'declarative.batch-worker: wrong logical owner')
                require(request['operation']==command['operation'],'declarative.batch-worker: mixed operation')
                replies.append(dict(logical_stream=index,result=self.children[index].execute(request)))
            return dict(replies=replies)
        # Evaluation uses one child per physical worker and never touches any
        # training stream, including its unfinished draft or sampling RNG.
        return next(iter(self.children.values())).execute(command)

    def preserve(self, directory, error):
        for index,child in self.children.items():
            child.preserve(Path(directory)/f'logical-{index}',error)


if __name__=='__main__':main(MultiplexWorker)
