"""One declared cold-runtime control for the accepted large original."""
import argparse
from collections import Counter
import json
from pathlib import Path
import time

import torch
from full_factory_curriculum import source_manifest
from full_factory_decoding import TemperaturePolicy
from full_factory_environment import ROOT, FactoryRuntime
from full_factory_landscape import episode
from full_factory_policy import SCHEMA, FullFactoryPolicy
from full_factory_screen import sha, write, save_gzip


def run(args):
    began=time.perf_counter(); torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    p=json.loads((args.parent/'protocol.json').read_text()); r=json.loads((args.parent/'report.json').read_text())
    if r['status']!='complete' or r['protocol_sha256']!=sha(args.parent/'protocol.json'):
        raise ValueError('cold-transfer.parent-incomplete')
    for file,digest in p['sources'].items():
        if sha(file)!=digest:raise ValueError(f'cold-transfer.parent-input: {file}')
    for file,digest in p['source_sha256'].items():
        if sha(ROOT/file)!=digest:raise ValueError(f'cold-transfer.parent-source: {file}')
    cases=[c for c in p['cases'] if 65<=c['instances']<=128]
    if len(cases)!=1:raise ValueError('cold-transfer.expected-one-large-original')
    sources=dict(p['sources']); sources.update({str((args.parent/f).resolve()):sha(args.parent/f) for f in ['protocol.json','report.json']})
    sources[str(args.native_library.resolve())]=sha(args.native_library)
    args.output.mkdir()
    protocol=dict(schema_version=1,parent_screen=str(args.parent.resolve()),cases=cases,catalogs=p['catalogs'],
        checkpoints=p['checkpoints'],methods=p['methods'],bank=1310000,temperature=.05,horizon=8,
        episode_seconds=30,route_work=5000,screen_seconds=180,description_cache='fresh runtime per episode',
        sources=sources,source_sha256=source_manifest(),schema=SCHEMA,depth=p['depth'],width=p['width'])
    write(args.output/'protocol.json',protocol); rows=[]
    for method in protocol['methods']:
        if time.perf_counter()-began>=180:raise TimeoutError('cold-transfer.study-budget')
        model=None; frozen=None
        if method!='geometry':
            checkpoint=torch.load(p['checkpoints'][method],map_location='cpu',weights_only=False)
            if checkpoint['schema']!=SCHEMA:raise ValueError('cold-transfer.schema')
            model=FullFactoryPolicy(p['depth'],p['width']);model.load_state_dict(checkpoint['policy']);model.eval()
            frozen={k:v.clone() for k,v in model.state_dict().items()}
        runtime=FactoryRuntime(args.native_library,p['catalogs'])
        try:
            if runtime.description_cache or runtime.description_cache_bytes:raise ValueError('cold-transfer.nonempty-cache')
            config=dict(horizon=8,balanced=False,memory=True,geometry=method=='geometry')
            bounded=argparse.Namespace(episode_seconds=30,route_work=5000)
            summary,best,_,_=episode(runtime,cases[0],TemperaturePolicy(model,.05) if model is not None else None,config,1310000,bounded)
            directory=args.output/method;directory.mkdir()
            save_gzip(directory/'episode.json.gz',summary);save_gzip(directory/'best.json.gz',best)
            rows.append(dict(method=method,case=cases[0]['id'],directory=str(directory.resolve()),
                initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],selected=summary['selected'],
                seconds=summary['total_seconds'],observation_seconds=summary['observation_seconds'],selection_seconds=summary['selection_seconds'],
                kinds=summary['kinds'],statuses=summary['statuses'],route_expansions=summary['route_expansions'],
                improvements=dict(Counter(t['action']['kind'] for t in summary['trace'] if t['status']=='improved')),
                budget_complete=summary['selected']==8 and not any(t['status']=='late' for t in summary['trace'])))
            if model is not None and any(not torch.equal(v,frozen[k]) for k,v in model.state_dict().items()):raise ValueError('cold-transfer.policy-mutated')
            write(args.output/'progress.json',dict(schema_version=1,status='incomplete',rows=rows));print(json.dumps(rows[-1]),flush=True)
        finally:runtime.close()
    for file,digest in sources.items():
        if sha(file)!=digest:raise ValueError(f'cold-transfer.changed-input: {file}')
    for file,digest in protocol['source_sha256'].items():
        if sha(ROOT/file)!=digest:raise ValueError(f'cold-transfer.changed-source: {file}')
    write(args.output/'report.json',dict(schema_version=1,status='complete',policy_immutable=True,
        protocol_sha256=sha(args.output/'protocol.json'),seconds=time.perf_counter()-began,rows=rows))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('parent','native-library','output'):parser.add_argument('--'+name,required=True,type=Path)
    run(parser.parse_args())
