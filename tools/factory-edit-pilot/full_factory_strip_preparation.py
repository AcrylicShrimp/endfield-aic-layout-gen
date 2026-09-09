"""Prepare a full original by charged ordinary contractions to operator exhaustion."""
import argparse
import json
from pathlib import Path
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT,FactoryRuntime,FactoryEpisode,digest
from full_factory_screen import sha,write,save_gzip


def run(args):
    began=time.perf_counter();parent=json.loads((args.parent/'protocol.json').read_text());report=json.loads((args.parent/'report.json').read_text())
    if report['status']!='complete' or report['protocol_sha256']!=sha(args.parent/'protocol.json'):raise ValueError('strip-preparation.parent-incomplete')
    for file,h in parent['sources'].items():
        if sha(file)!=h:raise ValueError(f'strip-preparation.input-hash: {file}')
    for file,h in parent['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'strip-preparation.source-hash: {file}')
    case=next(c for c in parent['cases'] if c['state']=='original')
    if case['instances']!=116:raise ValueError('strip-preparation.expected-declared-large-original')
    sources=dict(parent['sources'])
    for f in [args.parent/'protocol.json',args.parent/'report.json',args.native_library]:sources[str(f.resolve())]=sha(f)
    args.output.mkdir();protocol=dict(schema_version=1,experiment='ordinary-contraction-exhaustion',case=case,catalogs=parent['catalogs'],
        parent=str(args.parent.resolve()),proposals=256,seconds=300,route_work=5000,sources=sources,source_sha256=source_manifest())
    write(args.output/'protocol.json',protocol);runtime=FactoryRuntime(args.native_library,parent['catalogs']);status='budget_exhausted';remaining=None;observation_seconds=0
    try:
        env=FactoryEpisode(runtime,case,horizon=256,elapsed=max(.001,300-(time.perf_counter()-began)),work=5000)
        while not env.done and time.perf_counter()-began<300:
            start=time.perf_counter();d=env.describe();observation_seconds+=time.perf_counter()-start
            contractions=[(i,a) for i,a in enumerate(d['actions']) if a['kind']=='contract'];remaining=len(contractions)
            if env.done or time.perf_counter()-began>=300:break
            if not contractions:status='contraction_exhausted';break
            index,action=contractions[0];row=env.step(action)
            row.update(available_contractions=remaining,vocabulary_index=index,current_sha256=digest(env.state['current']))
            if row['status']!='improved':status='candidate_failed';break
            if env.selected%16==0:print(json.dumps(dict(selected=env.selected,remaining_before=remaining,score=env.state['best_metrics']['score'],seconds=time.perf_counter()-began)),flush=True)
        summary=env.summary();summary.update(status=status,remaining_contractions=remaining,observation_seconds=observation_seconds)
        save_gzip(args.output/'preparation.json.gz',summary);write(args.output/'best.json',env.state['best'])
        prepared=dict(case,id=case['id']+'--contraction-exhausted',state='contraction-exhausted',initial=str((args.output/'best.json').resolve()),
            initial_sha256=sha(args.output/'best.json'))
        prepared['files_sha256']=dict(case['files_sha256'],**{'initial.json':prepared['initial_sha256']})
        if status=='contraction_exhausted':write(args.output/'inputs.json',dict(schema_version=1,catalogs=parent['catalogs'],rows=[prepared]))
    finally:runtime.close()
    for file,h in sources.items():
        if sha(file)!=h:raise ValueError(f'strip-preparation.changed-input: {file}')
    for file,h in protocol['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'strip-preparation.changed-source: {file}')
    result=dict(schema_version=1,status=status,protocol_sha256=sha(args.output/'protocol.json'),case=prepared if status=='contraction_exhausted' else None,
        selected=summary['selected'],initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],seconds=time.perf_counter()-began,
        observation_seconds=observation_seconds,remaining_contractions=remaining,preparation_sha256=sha(args.output/'preparation.json.gz'),
        diagnostics=[] if status=='contraction_exhausted' else [dict(code='factory.strip-preparation-stopped',severity='warning',stage='strip-preparation',
            message='Preparation did not establish contraction exhaustion within its declared work/time bounds',entity=case['id'])])
    write(args.output/'report.json',result);print(json.dumps(result),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['parent','native-library','output']:parser.add_argument('--'+name,type=Path,required=True)
    run(parser.parse_args())
