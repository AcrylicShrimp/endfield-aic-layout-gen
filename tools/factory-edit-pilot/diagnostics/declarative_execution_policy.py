"""One bounded CPU/GPU inference comparison on a recorded logical rollout."""
import argparse
from copy import deepcopy
import os
from pathlib import Path
import signal
import sys
import time


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',type=Path,required=True);p.add_argument('--parent',type=Path,required=True)
    p.add_argument('--baseline',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();sys.path.insert(0,str(a.source/'tools/factory-edit-pilot'))
    import torch
    from blueprint_ppo_store import checked,load,read,write
    from blueprint_ppo_update import digest,require
    from declarative_execution_migrate import parent
    import declarative_learning_policy as policy
    require(not a.output.exists(),'policy diagnostic requires fresh output')
    signal.signal(signal.SIGALRM,lambda *_: (_ for _ in ()).throw(TimeoutError('policy diagnostic deadline')));signal.alarm(120)
    torch.set_num_threads(1);os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8');torch.use_deterministic_algorithms(True)
    value=parent(a.parent);saved=value['saved'];config=value['launch']['config'];seed=saved['seed'];ordinal=saved['update']+1
    source=a.baseline/f'seed-{seed}';job=read(source/'state.json')
    payload=load(checked(read(checked(job['records'][ordinal-1],source))['chunk'],source))
    model,_,_=policy.learners(seed,config['ppo'],'cpu');model.load_state_dict(saved['model']);model.eval()
    groups=[payload['samples'][i:i+32] for i in range(0,len(payload['samples']),32)]
    results={};timings={}
    with torch.no_grad():
        for device in ('cpu','cuda:0'):
            selected=deepcopy(model).to(device);warm=torch.Generator().manual_seed(1)
            policy.sample(selected,[s['input'] for s in groups[0]],warm)
            if device!='cpu':torch.cuda.synchronize()
            sampler=torch.Generator();sampler.set_state(saved['sampler_rng']);rows=[];began=time.perf_counter()
            for group in groups:rows.extend(policy.sample(selected,[s['input'] for s in group],sampler))
            if device!='cpu':torch.cuda.synchronize()
            timings[device]=time.perf_counter()-began
            results[device]=dict(rows=rows,sampler_sha256=digest(sampler.get_state()))
    left,right=results['cpu'],results['cuda:0']
    require(left['sampler_sha256']==right['sampler_sha256'],'policy device changed sampler draw sequence')
    require(all(a['decision']==b['decision'] and a['action']==b['action'] for a,b in zip(left['rows'],right['rows'],strict=True)),
            'policy device changed sampled action on recorded inputs')
    require(all(a['decision']==b['decision'] for a,b in zip(left['rows'],payload['samples'],strict=True)),
            'CPU action sampling changed from retained predecessor')
    differences={}
    for key in ('old_logp','old_value','entropy'):
        x=torch.tensor([r[key] for r in left['rows']]);y=torch.tensor([r[key] for r in right['rows']])
        torch.testing.assert_close(x,y,rtol=1e-4,atol=2e-5);differences[key]=float((x-y).abs().max())
    write(a.output,dict(schema='declarative-policy-device-proof-v1',status='passed',rows=len(payload['samples']),
        timing_seconds=timings,max_absolute_difference=differences,sampled_actions_equal=True,
        sampler_sha256=left['sampler_sha256'],parent=value['binding'],
        scope='One warmup batch and one timed pass per device on identical retained observations/model/RNG; no environment work.'))
    signal.alarm(0)


if __name__=='__main__':main()
