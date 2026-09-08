"""Frozen temperature comparison on the existing opened original-root screen."""
import argparse
import json
from pathlib import Path
import time

import torch

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT, FactoryRuntime
from full_factory_landscape import episode
from full_factory_policy import SCHEMA, FullFactoryPolicy
from full_factory_screen import sha, write, save_gzip


class TemperaturePolicy(torch.nn.Module):
    def __init__(self, policy, temperature):
        super().__init__()
        if not 0 < temperature <= 1:
            raise ValueError('decoding.temperature-range')
        self.policy, self.temperature = policy, temperature

    def forward(self, observation):
        return self.policy(observation)/self.temperature


def run(args):
    began=time.perf_counter();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    parent=json.loads((args.parent/'protocol.json').read_text())
    report=json.loads((args.parent/'report.json').read_text())
    if report['status']!='complete' or report['protocol_sha256']!=sha(args.parent/'protocol.json'):
        raise ValueError('decoding.parent-incomplete')
    for file,expected in parent['sources'].items():
        if sha(file)!=expected:raise ValueError(f'decoding.parent-input: {file}')
    for file,expected in parent['source_sha256'].items():
        if sha(ROOT/file)!=expected:raise ValueError(f'decoding.parent-source: {file}')
    checkpoint=torch.load(args.parent/'learned.pt',map_location='cpu',weights_only=False)
    if checkpoint['schema']!=SCHEMA or checkpoint['protocol_sha256']!=report['protocol_sha256']:
        raise ValueError('decoding.checkpoint-provenance')
    policy=FullFactoryPolicy(parent['depth'],parent['width']);policy.load_state_dict(checkpoint['policy']);policy.eval()
    frozen={k:v.clone() for k,v in policy.state_dict().items()}
    plan=parent['plan'];args.output.mkdir();rows=[]
    protocol=dict(schema_version=1,parent=str(args.parent.resolve()),temperatures=[.25,.05],
        parent_protocol_sha256=report['protocol_sha256'],parent_report_sha256=sha(args.parent/'report.json'),
        checkpoint_sha256=sha(args.parent/'learned.pt'),native_sha256=sha(args.native_library),
        cases=parent['cases'],catalogs=parent['catalogs'],horizon=plan['horizon'],banks=plan['banks'],
        episode_seconds=plan['episode_seconds'],route_work=plan['route_work'],source_sha256=source_manifest())
    write(args.output/'protocol.json',protocol)
    runtime=FactoryRuntime(args.native_library,parent['catalogs'])
    bounded=argparse.Namespace(episode_seconds=plan['episode_seconds'],route_work=plan['route_work'])
    config=dict(horizon=plan['horizon'],balanced=False,memory=True,geometry=False)
    try:
        for temperature in protocol['temperatures']:
            actor=TemperaturePolicy(policy,temperature)
            for case in parent['cases']:
                for bank in plan['banks']:
                    summary,best,_,_=episode(runtime,case,actor,config,bank,bounded)
                    name=f't{temperature}--{case["id"]}--{bank}';directory=args.output/name;directory.mkdir()
                    save_gzip(directory/'episode.json.gz',summary);save_gzip(directory/'best.json.gz',best)
                    rows.append(dict(temperature=temperature,case=case['id'],family=case['family'],bank=bank,directory=str(directory.resolve()),
                        initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],selected=summary['selected'],
                        seconds=summary['total_seconds'],statuses=summary['statuses'],kinds=summary['kinds'],
                        budget_complete=summary['selected']==plan['horizon'] and not any(t['status']=='late' for t in summary['trace'])))
            print(json.dumps(dict(temperature=temperature,episodes=len(rows),seconds=time.perf_counter()-began)),flush=True)
        if any(not torch.equal(v,frozen[k]) for k,v in policy.state_dict().items()):raise ValueError('decoding.policy-mutated')
        for file,expected in protocol['source_sha256'].items():
            if sha(ROOT/file)!=expected:raise ValueError(f'decoding.source-changed: {file}')
        if sha(args.parent/'learned.pt')!=protocol['checkpoint_sha256']:raise ValueError('decoding.checkpoint-changed')
        write(args.output/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
                                           seconds=time.perf_counter()-began,policy_immutable=True,rows=rows))
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',code='decoding.failed',stage='evaluation',message=str(error),rows=rows));raise
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('parent','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
