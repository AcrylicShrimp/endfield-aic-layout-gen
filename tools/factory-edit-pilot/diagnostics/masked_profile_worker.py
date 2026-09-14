"""Diagnostic-only representative persistent worker function attribution."""
import atexit
import cProfile
import os
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from blueprint_execution_worker import main
from blueprint_ppo_store import write
from declarative_batch_worker import MultiplexWorker
from masked_cycle_profile import summary
import blueprint_execution_worker as execution


class ProfileWorker(MultiplexWorker):
    def __init__(self):
        super().__init__()
        self.profile=None

    def execute(self, command):
        if command['operation']=='initialize' and command['index']==0:
            self.profile=cProfile.Profile()
            original_send=execution.send
            execution.send=lambda *a,**kw:self.profile.runcall(original_send,*a,**kw)
            atexit.register(lambda:write(Path(os.environ['AIC_MASKED_PROFILE_OUTPUT'])/'worker-profile.json',summary(self.profile)))
        if self.profile:return self.profile.runcall(super().execute,command)
        return super().execute(command)


if __name__=='__main__':main(ProfileWorker)
