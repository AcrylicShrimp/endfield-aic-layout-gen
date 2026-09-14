"""Bounded compiled joint likelihood experiment on immutable real inputs."""
import argparse
from copy import deepcopy
import os
from pathlib import Path
import signal
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import torch
from blueprint_ppo_store import load,read,write
from blueprint_ppo_update import require
from compiled_actor import CompiledActor
import declarative_learning_policy as policy


class Joint(torch.nn.Module):
    def __init__(self,model):super().__init__();self.model=model
    def forward(self,prepared):return policy.predict(self.model,prepared)


def compare(source, checkpoint, output):
    require(not output.exists(),'masked-compile.output: fresh diagnostic output required')
    output.mkdir(parents=True);signal.alarm(240)
    os.environ['CUBLAS_WORKSPACE_CONFIG']=':4096:8'
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    saved=load(checkpoint);model=policy.ActorCritic().cuda();model.load_state_dict(saved['model'])
    rows=list(load(source)['samples'].values())
    batches=[policy.device_rows(policy.prepare_rows(rows[i:i+32]),'cuda') for i in (0,32,64)]
    compiled=None;result=dict(schema='masked-compile-comparison-v1',source=str(source),checkpoint=str(checkpoint))
    try:
        compiled=CompiledActor(Joint(deepcopy(model)),output/'compiler-cache')
        expected=Joint(model);began=time.perf_counter();errors=[]
        for batch in batches:
            a=expected(batch);b=compiled(batch)
            for left,right in zip(a,b,strict=True):
                errors.append(float((left-right).abs().max()))
                torch.testing.assert_close(left,right,atol=1e-5,rtol=1e-5)
            model.zero_grad(set_to_none=True);compiled.forward._orig_mod.zero_grad(set_to_none=True)
            sum(x.square().mean() for x in a).backward();sum(x.square().mean() for x in b).backward()
            for left,right in zip(model.parameters(),compiled.forward._orig_mod.model.parameters(),strict=True):
                if left.grad is not None:torch.testing.assert_close(left.grad,right.grad,atol=2e-5,rtol=1e-4)
        torch.cuda.synchronize();result['warmup_seconds']=time.perf_counter()-began
        result['maximum_output_error']=max(errors);compiled.phase='measurement';measurements=[]
        for label,call in [('eager',expected),('compiled',compiled)]*2:
            began=time.perf_counter()
            for index in range(30):
                call.zero_grad(set_to_none=True) if label=='eager' else compiled.forward._orig_mod.zero_grad(set_to_none=True)
                values=call(batches[index%len(batches)])
                sum(x.square().mean() for x in values).backward()
            torch.cuda.synchronize();measurements.append(dict(kind=label,steps=30,seconds=time.perf_counter()-began))
        result.update(status='passed',measurements=measurements,compiler=compiled.report())
    except Exception as error:
        result.update(status='rejected',diagnostic=dict(code='masked-compile.unsuitable',message=str(error)[:3000]))
    finally:
        if compiled:compiled.close()
        write(output/'report.json',result);signal.alarm(0)
    return result


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',type=Path,required=True);p.add_argument('--checkpoint',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);a=p.parse_args();print(compare(a.source,a.checkpoint,a.output))
