"""Bounded explicit native/reference parity over retained actual input files."""
import argparse
from pathlib import Path
import os
import sys
import unittest

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
BASE=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(BASE))

import torch
from blueprint_ppo_store import checked,load,read,sha,write
from blueprint_ppo_update import require
from declarative_learning_environment import Runtime
import declarative_learning_policy as policy
from test_declarative_learning import diagnostic_config
from test_declarative_native_ownership import compare_input


def run(launch_path,bank,output,factories,deep_factories,steps,device):
    require(not output.exists(),'native-parity.output: fresh receipt required')
    require(0<factories<=256 and 0<=deep_factories<=factories and 0<steps<=64,
        'native-parity.bounds: at most 256 actual inputs and 64 steps each')
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    launch=read(launch_path)
    require(launch['masked_long']['probe'],'native-parity.scope: readiness launch required')
    # Explicit diagnostic admission of archived actual payloads. This does not
    # migrate or relabel the generator contract of the original input bank.
    config=diagnostic_config();config['actions']=launch['config']['actions']
    runtime=Runtime(config,launch['source']['native_library'])
    model,_,_=policy.learners(17,config['ppo']);model=model.to(device)
    check=unittest.TestCase();rows=[]
    for index in range(factories):
        path=bank/f'episode-{index:08d}/episode.json';record=read(path)
        require(record['schema']=='blueprint-fresh-episode-v2','native-parity.input: compact actual input required')
        actual=load(checked(record['files']['input'],bank))['payload']
        runtime.request,runtime.original=actual['request'],actual['original']
        session,_=runtime.open(64,60000)
        count=steps if index<deep_factories else 1
        for step in range(count):
            _,native=runtime.observe(session);_,oracle=compare_input(check,runtime,session,native)
            seed=index*steps+step
            with torch.no_grad():
                selected=policy.sample(model,[native],torch.Generator().manual_seed(seed))[0]
                expected=policy.sample(model,[oracle],torch.Generator().manual_seed(seed))[0]
            check.assertEqual(selected,{k:v for k,v in expected.items() if k!='action'})
            result=runtime.step_index(session,native,selected['decision'])
            check.assertEqual(result['action'],expected['action'])
        rows.append(dict(slot=index,product=record['draw']['product'],geometry_sha256=record['geometry_sha256'],
            input_reference=record['files']['input'],steps=count))
    result=dict(schema='declarative-native-parity-v1',status='passed',device=device,
        observations=sum(row['steps'] for row in rows),factories=factories,
        checks=['bit-exact features except independently sampled elapsed-time feature',
            'attempt count, ordered root commands, masks and every conditional port rotation',
            'deterministic ordered decisions, selected native command, log probability, value and entropy'],
        launch=dict(path=str(launch_path),sha256=sha(launch_path)),input_bank=str(bank),rows=rows)
    write(output,result);print({k:result[k] for k in ('status','factories','observations','device')},flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--launch',type=Path,required=True);p.add_argument('--input-bank',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--factories',type=int,default=128)
    p.add_argument('--deep-factories',type=int,default=12);p.add_argument('--steps',type=int,default=8)
    p.add_argument('--device',default='cuda:0');a=p.parse_args()
    run(a.launch.resolve(),a.input_bank.resolve(),a.output.resolve(),a.factories,a.deep_factories,a.steps,a.device)
