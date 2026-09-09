"""Collect full-score alternatives on a validated contraction-exhausted large state."""
import argparse
from collections import Counter,defaultdict
import json
from pathlib import Path
import random
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT,FactoryRuntime,FactoryEpisode,digest
from full_factory_pose_teachers import selected_poses,displacement
from full_factory_screen import sha,write,save_gzip


def alternatives(description,witness,config):
    poses,populations=selected_poses(description,witness,config['pose_quotas'],config['seed'])
    selected=list(poses);rng=random.Random(config['seed']+1)
    components={c['id']:c for c in witness['logistics_components']}
    terminals={t['id']:t for n in witness['transport_networks'] for t in n['terminals']}
    for kind,quota in config['other_quotas'].items():
        grouped=defaultdict(list);population=0
        for index,a in enumerate(description['actions']):
            if a['kind']!=kind:continue
            population+=1
            group=(a['network'] if kind=='reroute' else a['terminal'] if kind=='port' else a['component']
                   if kind in ['component','release_bridge'] else a['axis'] if kind=='insert' else 'control')
            if kind=='reroute':key=(-len(description['chains'][a['chain']]['cells']),index)
            elif kind in ['port','component']:
                p=(terminals[a['terminal']] if kind=='port' else components[a['component']])['position']
                key=(abs(a['x']-p['x'])+abs(a['y']-p['y']),index)
            else:key=(rng.random(),index)
            grouped[group].append((key,index,a))
        groups=sorted(grouped,key=str);rng.shuffle(groups);lists=[]
        for group in groups:
            choices=sorted(grouped[group],key=lambda row:row[0])
            if kind=='reroute':
                interleaved=[]
                while choices:
                    interleaved.append(choices.pop(0))
                    if choices:interleaved.append(choices.pop())
                choices=interleaved
            lists.append(choices)
        count=0;round=0
        while count<quota and any(round<len(xs) for xs in lists):
            for xs in lists:
                if count>=quota:break
                if round<len(xs):
                    _,index,a=xs[round];selected.append(dict(stratum=kind,vocabulary_index=index,action=a));count+=1
            round+=1
        populations[kind]=dict(population=population,member_groups=len(groups),quota=quota,selected=count)
    if len({row['vocabulary_index'] for row in selected})!=len(selected):raise ValueError('dense-contrasts.duplicate-selection')
    return selected,populations


def run(args):
    began=time.perf_counter();p=json.loads((args.preparation/'protocol.json').read_text());r=json.loads((args.preparation/'report.json').read_text())
    audit=json.loads((args.preparation/'audit.json').read_text());config=json.loads(args.config.read_text())
    if r['status']!='contraction_exhausted' or r['protocol_sha256']!=sha(args.preparation/'protocol.json') or r['remaining_contractions']!=0:
        raise ValueError('dense-contrasts.preparation-incomplete')
    if audit['status']!='passed' or audit['report_sha256']!=sha(args.preparation/'report.json') or audit['replayed']!=r['selected']:
        raise ValueError('dense-contrasts.preparation-audit')
    if config.get('schema_version')!=1 or config['action_seconds']<=0 or not config['action_seconds']<=config['study_seconds']<=3600:
        raise ValueError(f'dense-contrasts.invalid-config: {args.config}')
    sources=dict(p['sources'])
    for file,h in sources.items():
        if sha(file)!=h:raise ValueError(f'dense-contrasts.input-hash: {file}')
    for file,h in p['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'dense-contrasts.source-hash: {file}')
    case=r['case']
    for f in [args.preparation/name for name in ['protocol.json','report.json','audit.json','inputs.json']]+[args.config,args.native_library,Path(case['initial'])]:sources[str(f.resolve())]=sha(f)
    if sha(case['initial'])!=case['initial_sha256']:raise ValueError('dense-contrasts.prepared-input')
    args.output.mkdir();runtime=FactoryRuntime(args.native_library,p['catalogs']);rows=[];retained={};status='complete'
    try:
        env=FactoryEpisode(runtime,case,horizon=1,elapsed=config['action_seconds'],work=config['route_work']);description=env.describe()
        if any(a['kind']=='contract' for a in description['actions']):raise ValueError('dense-contrasts.contractions-remain')
        initial=env.state['current'];initial_metrics=env.state['initial_metrics'];best=initial;best_metrics=initial_metrics
        selections,populations=alternatives(description,initial,config)
        protocol=dict(schema_version=1,experiment='contraction-exhausted-full-score-contrasts',preparation=str(args.preparation.resolve()),case=case,
            catalogs=p['catalogs'],config=config,initial_metrics=initial_metrics,selections=selections,populations=populations,
            sources=sources,source_sha256=source_manifest(),training_performed=False,description_seconds=description['seconds'],
            selection_policy='Pose member diversity; other entity diversity; reroutes alternate long/short within networks; ports/components prefer short travel')
        write(args.output/'protocol.json',protocol)
        for selection in selections:
            remaining=config['study_seconds']-(time.perf_counter()-began)
            if remaining<=0:status='budget_exhausted';break
            env=FactoryEpisode(runtime,case,horizon=1,elapsed=min(config['action_seconds'],remaining),work=config['route_work']);trace=env.step(selection['action'])
            valid=trace['status'] in ['improved','accepted_equal','accepted_exploration']
            row=dict(id=len(rows),**selection,trace=trace,completed=trace['status'] not in ['late','budget_exhausted'],factory_validated=valid,
                displacement=displacement(initial,env.state['current'],selection['action']) if valid and selection['action']['kind']=='pose' else None,
                current_sha256=digest(env.state['current']),candidate_metrics=env.state['current_metrics'] if valid else None,best_metrics=env.state['best_metrics'])
            rows.append(row)
            if valid:
                metric=env.state['current_metrics'];kind=selection['stratum']
                if kind not in retained or metric['score']<retained[kind]['score']:
                    destination=args.output/(kind+'-candidate.json.gz');save_gzip(destination,env.state['current'])
                    retained[kind]=dict(row=row['id'],score=metric['score'],path=str(destination.resolve()))
                if metric['score']<best_metrics['score']:best=env.state['current'];best_metrics=metric
            if len(rows)%24==0:print(json.dumps(dict(selected=len(rows),valid=sum(row['factory_validated'] for row in rows),
                improved=sum(row['trace']['status']=='improved' for row in rows),best=best_metrics['score'],seconds=time.perf_counter()-began)),flush=True)
        save_gzip(args.output/'best.json.gz',best);save_gzip(args.output/'outcomes.json.gz',rows)
    finally:runtime.close()
    for file,h in sources.items():
        if sha(file)!=h:raise ValueError(f'dense-contrasts.changed-input: {file}')
    for file,h in protocol['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'dense-contrasts.changed-source: {file}')
    write(args.output/'report.json',dict(schema_version=1,status=status,protocol_sha256=sha(args.output/'protocol.json'),
        initial_metrics=initial_metrics,best_metrics=best_metrics,selected=len(rows),planned=len(selections),seconds=time.perf_counter()-began,
        statuses=dict(Counter(row['trace']['status'] for row in rows)),retained=retained,
        targets=[row['id'] for row in rows if row['trace']['status']=='improved' and row['best_metrics']['score']==best_metrics['score']],
        route_expansions=sum(row['trace']['route_expansions'] for row in rows),outcomes_sha256=sha(args.output/'outcomes.json.gz'),training_performed=False))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['preparation','native-library','config','output']:parser.add_argument('--'+name,required=True,type=Path)
    run(parser.parse_args())
