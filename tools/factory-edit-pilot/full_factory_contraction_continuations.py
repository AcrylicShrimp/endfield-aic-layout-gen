"""Measure short ordinary contraction continuations from audited valid first edits."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT,FactoryRuntime,FactoryEpisode,digest
from full_factory_screen import sha,write,save_gzip


def eligible(rows):
    return [row for row in rows if row['completed'] and row['factory_validated'] and row['action']['kind'] not in ['insert','restore_best']]


def run(args):
    began=time.perf_counter();p=json.loads((args.parent/'protocol.json').read_text());r=json.loads((args.parent/'report.json').read_text());audit=json.loads((args.parent/'audit.json').read_text())
    if r['status']!='complete' or r['protocol_sha256']!=sha(args.parent/'protocol.json') or audit['status']!='passed' or audit['report_sha256']!=sha(args.parent/'report.json'):
        raise ValueError('contraction-continuations.parent-audit')
    rows=json.loads(gzip.decompress((args.parent/'outcomes.json.gz').read_bytes()));config=json.loads(args.config.read_text())
    if r['outcomes_sha256']!=sha(args.parent/'outcomes.json.gz') or audit['replayed']!=len(rows):raise ValueError('contraction-continuations.parent-outcomes')
    if config.get('schema_version')!=1 or config['horizon']!=5 or not 0<config['episode_seconds']<=config['study_seconds']<=3600:
        raise ValueError(f'contraction-continuations.invalid-config: {args.config}')
    sources=dict(p['sources'])
    for file,h in sources.items():
        if sha(file)!=h:raise ValueError(f'contraction-continuations.input-hash: {file}')
    for file,h in p['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'contraction-continuations.source-hash: {file}')
    for f in [args.parent/name for name in ['protocol.json','report.json','audit.json','outcomes.json.gz']]+[args.config,args.native_library]:sources[str(f.resolve())]=sha(f)
    firsts=eligible(rows);case=p['case'];args.output.mkdir()
    protocol=dict(schema_version=1,experiment='large-original-contraction-continuations',parent=str(args.parent.resolve()),case=case,catalogs=p['catalogs'],
        config=config,first_action_ids=[row['id'] for row in firsts],parent_trials=len(rows),initial_metrics=p['initial_metrics'],
        sources=sources,source_sha256=source_manifest(),training_performed=False)
    write(args.output/'protocol.json',protocol);runtime=FactoryRuntime(args.native_library,p['catalogs']);trajectories=[];retained={};status='complete'
    best=json.loads(Path(case['initial']).read_text());best_metrics=p['initial_metrics']
    try:
        root=FactoryEpisode(runtime,case,horizon=5,elapsed=config['episode_seconds'],work=config['route_work'])
        if any(a['kind']=='contract' for a in root.describe()['actions']):raise ValueError('contraction-continuations.source-not-exhausted')
        for source in firsts:
            remaining=config['study_seconds']-(time.perf_counter()-began)
            if remaining<=0:status='budget_exhausted';break
            env=FactoryEpisode(runtime,case,horizon=5,elapsed=min(config['episode_seconds'],remaining),work=config['route_work'])
            first=env.step(source['action']);first['current_sha256']=digest(env.state['current'])
            if first['status']!=source['trace']['status'] or first['executor_status']!=source['trace']['executor_status'] or first['route_expansions']!=source['trace']['route_expansions'] or first['current_sha256']!=source['current_sha256']:
                raise ValueError(f'contraction-continuations.first-edit-changed: {source["id"]}')
            termination='time_budget';exposed=None;remaining_contractions=None;observation_seconds=0
            while time.perf_counter()-env.started<env.elapsed and time.perf_counter()-began<config['study_seconds']:
                start=time.perf_counter();description=env.describe();observation_seconds+=time.perf_counter()-start
                if time.perf_counter()-env.started>=env.elapsed or time.perf_counter()-began>=config['study_seconds']:break
                choices=[(i,a) for i,a in enumerate(description['actions']) if a['kind']=='contract'];remaining_contractions=len(choices)
                if exposed is None:exposed=remaining_contractions
                if not choices:termination='contraction_exhausted';break
                if env.selected>=config['horizon']:termination='horizon';break
                index,action=choices[0];step=env.step(action);step.update(vocabulary_index=index,available_contractions=remaining_contractions,current_sha256=digest(env.state['current']))
                if step['status']!='improved':termination='candidate_failed';break
            summary=env.summary();row=dict(id=len(trajectories),source_id=source['id'],first_kind=source['action']['kind'],first_stratum=source['stratum'],
                exposed_contractions=exposed,remaining_contractions=remaining_contractions,termination=termination,
                completed=termination in ['contraction_exhausted','horizon'],observation_seconds=observation_seconds,summary=summary)
            trajectories.append(row);metric=env.state['best_metrics'];kind=row['first_kind']
            if kind not in retained or metric['score']<retained[kind]['score']:
                destination=args.output/(kind+'-best.json.gz');save_gzip(destination,env.state['best']);retained[kind]=dict(trajectory=row['id'],source_id=source['id'],score=metric['score'],path=str(destination.resolve()))
            if metric['score']<best_metrics['score']:best=env.state['best'];best_metrics=metric
            if len(trajectories)%16==0:
                save_gzip(args.output/'trajectories.json.gz',trajectories)
                print(json.dumps(dict(trajectories=len(trajectories),exposed=sum(bool(t['exposed_contractions']) for t in trajectories),
                    area_improved=sum(t['summary']['best_metrics']['score'][0]<p['initial_metrics']['score'][0] for t in trajectories),best=best_metrics['score'],seconds=time.perf_counter()-began)),flush=True)
        save_gzip(args.output/'trajectories.json.gz',trajectories);save_gzip(args.output/'best.json.gz',best)
    finally:runtime.close()
    for file,h in sources.items():
        if sha(file)!=h:raise ValueError(f'contraction-continuations.changed-input: {file}')
    for file,h in protocol['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'contraction-continuations.changed-source: {file}')
    write(args.output/'report.json',dict(schema_version=1,status=status,protocol_sha256=sha(args.output/'protocol.json'),seconds=time.perf_counter()-began,
        planned=len(firsts),trajectories=len(trajectories),selected=sum(t['summary']['selected'] for t in trajectories),initial_metrics=p['initial_metrics'],best_metrics=best_metrics,
        exposed=sum(bool(t['exposed_contractions']) for t in trajectories),area_improved=sum(t['summary']['best_metrics']['score'][0]<p['initial_metrics']['score'][0] for t in trajectories),
        terminations=dict(Counter(t['termination'] for t in trajectories)),retained=retained,
        targets=[t['id'] for t in trajectories if t['completed'] and t['summary']['best_metrics']['score']==best_metrics['score'] and best_metrics['score']<p['initial_metrics']['score']],
        trajectories_sha256=sha(args.output/'trajectories.json.gz'),training_performed=False))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['parent','native-library','config','output']:parser.add_argument('--'+name,type=Path,required=True)
    run(parser.parse_args())
