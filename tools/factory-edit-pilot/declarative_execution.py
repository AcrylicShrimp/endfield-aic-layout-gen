"""Explicit compact-source migration and verified execution-only continuation."""
import argparse
from copy import deepcopy
from pathlib import Path

from blueprint_ppo_store import read
from declarative_execution_migrate import compact_parent,migrate
from declarative_learning_launch import validate


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('command',choices=('compact','verify','prepare'));p.add_argument('--parent',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--compact-parent',type=Path)
    p.add_argument('--workers',type=int,default=16);p.add_argument('--behavior-device',choices=('cpu','cuda:0'),default='cuda:0')
    p.add_argument('--policy-proof',type=Path);p.add_argument('--replay-proof',type=Path);p.add_argument('--verification',type=Path)
    p.add_argument('--inputs',type=Path);p.add_argument('--environment',type=Path,default=Path('.tracking.env'))
    a=p.parse_args()
    if a.command=='compact':result=compact_parent(a.parent,a.output)
    else:
        launch=read(a.parent/'launch.json');config=deepcopy(launch['config'])
        config['execution_options']=dict(workers=a.workers,behavior_device=a.behavior_device)
        config['retention']['training_traces']=[];validate(config)
        if a.compact_parent is None:p.error('requires --compact-parent')
        if a.command=='verify':
            if any(v is None for v in (a.policy_proof,a.replay_proof,a.inputs)):p.error('verify requires --policy-proof, --replay-proof and --inputs')
            from declarative_execution_verify import verify
            result=verify(config,a.parent,a.compact_parent,a.policy_proof,a.replay_proof,a.inputs,a.output,a.environment)
        else:
            if a.verification is None:p.error('prepare requires --verification')
            result=migrate(config,launch['source']['native_library'],launch['panel']['path'],a.parent,a.compact_parent,a.verification,a.output)
    print({k:v for k,v in result.items() if k in ('status','continuity_closed','completed_update','preserved_seconds','remaining_seconds')})


if __name__=='__main__':main()
