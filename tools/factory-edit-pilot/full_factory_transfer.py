"""Frozen learned/parent/geometry comparison on freshly admitted original factories."""
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
from full_factory_screen import sha,write,save_gzip


def admitted_cases(admission,inputs):
    accepted={r['id']:r for r in admission['rows'] if r['factory_validated']}
    if admission['status']!='measured' or len(accepted)!=admission['accepted'] or len(inputs['rows'])!=len(accepted):
        raise ValueError('transfer.admission-count')
    cases=[]
    for row in inputs['rows']:
        a=accepted.pop(row['id'])
        if row['instances']!=a['facilities']:raise ValueError('transfer.admission-size')
        cases.append(dict(row,family=a['family'],family_split=a['split'],evaluation_role='fresh-size-development'))
    if accepted:raise ValueError('transfer.admission-membership')
    return cases


def run(args):
    began=time.perf_counter();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    learned=json.loads((args.learned_study/'protocol.json').read_text());lr=json.loads((args.learned_study/'report.json').read_text())
    if lr['status']!='complete' or lr['protocol_sha256']!=sha(args.learned_study/'protocol.json'):raise ValueError('transfer.learned-study')
    parent=Path(learned['parent']);inputs=json.loads(args.inputs.read_text());admission=json.loads(args.admission.read_text())
    cases=admitted_cases(admission,inputs)
    if not cases:raise ValueError('transfer.no-admitted-roots')
    checkpoints={'learned':args.learned_study/'best-mass-final.pt','parent':parent/'learned.pt'}
    sources=dict(learned['sources'])
    for f in [args.inputs,args.admission,args.learned_study/'protocol.json',args.learned_study/'report.json',args.native_library,*checkpoints.values()]:
        sources[str(f.resolve())]=sha(f)
    for record in admission['provenance']+inputs['provenance']:sources[record['path']]=record['sha256']
    for f,h in sources.items():
        if sha(f)!=h:raise ValueError(f'transfer.input-hash: {f}')
    for f,h in learned['source_sha256'].items():
        if sha(ROOT/f)!=h:raise ValueError(f'transfer.source-hash: {f}')
    catalog_paths=json.loads(Path(inputs['catalogs']).read_text());old_paths=json.loads(Path(learned['catalogs']).read_text())
    if {k:sha(f) for k,f in catalog_paths.items()}!={k:sha(f) for k,f in old_paths.items()}:raise ValueError('transfer.catalogs')
    sources[str(Path(inputs['catalogs']).resolve())]=sha(inputs['catalogs'])
    parent_initials={sha(c['initial']) for c in learned['cases']}
    for c in cases:
        for key,name in [('initial','initial.json'),('request','request.json'),('original','original-request.json')]:
            if sha(c[key])!=c['files_sha256'][name]:raise ValueError(f'transfer.root-input: {c[key]}')
            sources[str(Path(c[key]).resolve())]=sha(c[key])
        if sha(c['initial']) in parent_initials:raise ValueError('transfer.reused-original')
    args.output.mkdir();protocol=dict(schema_version=1,experiment='frozen-fresh-size-transfer',inputs=str(args.inputs.resolve()),
        admission=str(args.admission.resolve()),learned_study=str(args.learned_study.resolve()),parent=str(parent.resolve()),
        cases=cases,catalogs=inputs['catalogs'],sources=sources,source_sha256=source_manifest(),
        methods=['parent','learned','geometry'],temperature=.05,banks=[1310000,1320000],horizon=8,episode_seconds=30,
        route_work=5000,screen_seconds=900,checkpoints={k:str(f.resolve()) for k,f in checkpoints.items()},schema=SCHEMA,
        depth=learned['depth'],width=learned['width'],attempted=admission['attempted'],accepted=admission['accepted'])
    write(args.output/'protocol.json',protocol);runtime=FactoryRuntime(args.native_library,inputs['catalogs']);rows=[]
    try:
        for method in protocol['methods']:
            model=None;frozen=None
            if method!='geometry':
                checkpoint=torch.load(checkpoints[method],map_location='cpu',weights_only=False)
                expected=sha(args.learned_study/'protocol.json') if method=='learned' else sha(parent/'protocol.json')
                if checkpoint['schema']!=SCHEMA or checkpoint['protocol_sha256']!=expected:raise ValueError('transfer.checkpoint-provenance')
                model=FullFactoryPolicy(protocol['depth'],protocol['width']);model.load_state_dict(checkpoint['policy']);model.eval()
                frozen={k:v.clone() for k,v in model.state_dict().items()}
            actor=TemperaturePolicy(model,.05) if model is not None else None
            config=dict(horizon=8,balanced=False,memory=True,geometry=method=='geometry')
            bounded=argparse.Namespace(episode_seconds=30,route_work=5000)
            for case in cases:
                for bank in protocol['banks']:
                    if time.perf_counter()-began>=900:raise TimeoutError('transfer.study-budget')
                    summary,best,_,_=episode(runtime,case,actor,config,bank,bounded)
                    directory=args.output/f'{method}--{case["id"]}--{bank}';directory.mkdir()
                    save_gzip(directory/'episode.json.gz',summary);save_gzip(directory/'best.json.gz',best)
                    improvements=Counter(t['action']['kind'] for t in summary['trace'] if t['status']=='improved')
                    rows.append(dict(method=method,case=case['id'],family=case['family'],family_split=case['family_split'],
                        evaluation_role=case['evaluation_role'],instances=case['instances'],bank=bank,directory=str(directory.resolve()),
                        initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],selected=summary['selected'],
                        seconds=summary['total_seconds'],kinds=summary['kinds'],statuses=summary['statuses'],improvements=dict(improvements),
                        budget_complete=summary['selected']==8 and not any(t['status']=='late' for t in summary['trace'])))
            if model is not None and any(not torch.equal(v,frozen[k]) for k,v in model.state_dict().items()):raise ValueError('transfer.policy-mutated')
            print(json.dumps(dict(method=method,episodes=len(rows),seconds=time.perf_counter()-began)),flush=True)
        for f,h in sources.items():
            if sha(f)!=h:raise ValueError(f'transfer.input-changed: {f}')
        for f,h in protocol['source_sha256'].items():
            if sha(ROOT/f)!=h:raise ValueError(f'transfer.source-changed: {f}')
        write(args.output/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
            seconds=time.perf_counter()-began,policy_immutable=True,attempted=admission['attempted'],accepted=len(cases),rows=rows))
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',code='transfer.failed',severity='error',stage='frozen-evaluation',message=str(error),rows=rows));raise
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('inputs','admission','learned-study','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
