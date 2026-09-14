"""Isolated compiled model heads on the normal learner, with eager state ownership."""
from pathlib import Path
import time
import torch

from blueprint_ppo_store import write
from compiled_actor import CompiledActor


def compiled_run(output,environment,measurement):
    import declarative_learning_runtime as runtime
    from masked_long_training import run
    original=runtime.Learner.__init__;handles=[];names=[];learner_ref=[]
    class Head(CompiledActor):
        def __init__(self,forward,model,cache):
            self.reference=forward;self.parameters=tuple(model.parameters());self.checked_modes=set()
            super().__init__(forward,cache)
        def on_start(self,args):
            self.phase=f'update-{learner_ref[0].ordinal+1}'
            super().on_start(args)
        def __call__(self,data):
            mode=torch.is_grad_enabled()
            if mode not in self.checked_modes:
                leaves=[]
                def isolated(value):
                    if isinstance(value,torch.Tensor) and value.requires_grad:
                        result=value.detach().requires_grad_(True);leaves.append(result);return result
                    if isinstance(value,dict):return {k:isolated(v) for k,v in value.items()}
                    if isinstance(value,tuple):return tuple(isolated(v) for v in value)
                    return value
                trial_data=isolated(data)
                trial=super().__call__(trial_data);expected=self.reference(trial_data)
                left=trial if isinstance(trial,tuple) else (trial,)
                right=expected if isinstance(expected,tuple) else (expected,)
                for a,b in zip(left,right,strict=True):torch.testing.assert_close(a,b,atol=1e-5,rtol=1e-5)
                if mode:
                    params=[p for p in self.parameters if p.requires_grad]+leaves
                    a=torch.autograd.grad(sum(x.square().mean() for x in left),params,allow_unused=True)
                    b=torch.autograd.grad(sum(x.square().mean() for x in right),params,allow_unused=True)
                    for x,y in zip(a,b,strict=True):
                        if x is None or y is None:assert x is y
                        else:torch.testing.assert_close(x,y,atol=2e-5,rtol=1e-4)
                self.checked_modes.add(mode)
            # AOT buffer donation deliberately consumes each graph once. The
            # diagnostic uses independent graphs, leaving the normal backward
            # and compiler memory optimization unchanged.
            return super().__call__(data)
    def initialize(self,*args,**kwargs):
        original(self,*args,**kwargs);learner_ref.append(self)
        # Behavior already owns its eager parameter copy. Wrapping bound
        # methods changes neither optimizer ownership nor checkpoint keys.
        for role,model in (('learner',self.model),('behavior',self.behavior)):
            root=Head(model.root,model,measurement/'compiler-cache')
            ports_method=model.ports
            def ports_call(data,method=ports_method):return method(*data)
            ports=Head(ports_call,model,measurement/'compiler-cache')
            model.root=root
            model.ports=lambda nodes,pooled,branches,call=ports:call((nodes,pooled,branches))
            handles.extend([root,ports]);names.extend([role+'-root',role+'-ports'])
    runtime.Learner.__init__=initialize;began=time.time()
    try:return run(output,environment)
    finally:
        # Each existing callback observes global compiler events; their event
        # lists overlap and must not be summed across handles.
        write(measurement/'compilation.json',dict(schema='masked-compiled-probe-v1',started_unix=began,
            elapsed_seconds=time.time()-began,events_are_shared_across_handles=True,
            completed_update=learner_ref[0].ordinal if learner_ref else None,
            heads={name:dict(handle.report(),parity_modes=sorted(handle.checked_modes)) for name,handle in zip(names,handles,strict=True)}))
        for handle in handles:handle.close()
        runtime.Learner.__init__=original
