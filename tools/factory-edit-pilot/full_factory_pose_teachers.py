"""Collect broad, shallow ordinary pose outcomes on two states of one large original."""
import argparse
from collections import Counter, defaultdict
import gzip
import json
from pathlib import Path
import random
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT, FactoryRuntime, FactoryEpisode, digest
from full_factory_screen import sha, write, save_gzip


def stratum(action,witness):
    if action['kind']!='pose':return None
    poses=action['poses']
    rotated=any(q['rotation']!=witness['placements'][q['member']]['rotation'] for q in poses)
    if rotated:return 'single_rotation' if len(poses)==1 else 'compound_rotation'
    if len(poses)>1:return 'group_translation'
    q=poses[0];p=witness['placements'][q['member']]
    return 'local_single' if abs(q['x']-p['x'])+abs(q['y']-p['y'])<=2 else 'distant_single'


def selected_poses(description,witness,quotas,seed):
    center=[sum(p[axis]+p[size]/2 for p in witness['placements'])/len(witness['placements'])
            for axis,size in [('x','width'),('y','height')]]
    groups={kind:defaultdict(list) for kind in quotas};population=Counter()
    for index,action in enumerate(description['actions']):
        kind=stratum(action,witness)
        if kind is None:continue
        population[kind]+=1;travel=0;inward=0
        for q in action['poses']:
            p=witness['placements'][q['member']];swap=abs(q['rotation']-p['rotation'])%180!=0
            width,height=(p['height'],p['width']) if swap else (p['width'],p['height'])
            travel+=abs(q['x']-p['x'])+abs(q['y']-p['y'])
            for axis,size,extent,c in [('x','width',width,center[0]),('y','height',height,center[1])]:
                inward+=abs(p[axis]+p[size]/2-c)-abs(q[axis]+extent/2-c)
        members=tuple(q['member'] for q in action['poses'])
        groups[kind][members].append((travel,-inward,index,action))
    selected=[];rng=random.Random(seed);report={}
    for kind,quota in quotas.items():
        members=sorted(groups[kind]);rng.shuffle(members)
        lists=[sorted(groups[kind][m],key=lambda row:row[:3]) for m in members];count=0;round=0
        while count<quota and any(round<len(xs) for xs in lists):
            for xs in lists:
                if count>=quota:break
                if round<len(xs):
                    _,_,index,action=xs[round];selected.append(dict(stratum=kind,vocabulary_index=index,action=action));count+=1
            round+=1
        report[kind]=dict(population=population[kind],member_groups=len(members),quota=quota,selected=count)
    return selected,report


def displacement(before,after,action):
    selected={before['placements'][q['member']]['instance'] for q in action['poses']}
    old={p['instance']:p for p in before['placements']};now={p['instance']:p for p in after['placements']}
    if old.keys()!=now.keys():raise ValueError('pose-teachers.facility-membership')
    fixed=[id for id in old if id not in selected]
    if not fixed:raise ValueError('pose-teachers.expected-unselected-frame')
    ref=fixed[0];dx=now[ref]['x']-old[ref]['x'];dy=now[ref]['y']-old[ref]['y']
    if any(now[id]['x']-old[id]['x']!=dx or now[id]['y']-old[id]['y']!=dy or now[id]['rotation']!=old[id]['rotation'] for id in fixed):
        raise ValueError('pose-teachers.unselected-facility-changed')
    translated=sum(now[id]['x']-old[id]['x']!=dx or now[id]['y']-old[id]['y']!=dy for id in selected)
    rotated=sum(now[id]['rotation']!=old[id]['rotation'] for id in selected)
    return dict(translated=translated,rotated=rotated,frame_shift=[dx,dy])


def run(args):
    began=time.perf_counter();config=json.loads(args.config.read_text())
    if (config.get('schema_version')!=1 or set(config.get('quotas',{}))!={'local_single','distant_single','group_translation','single_rotation','compound_rotation'}
        or any(not isinstance(q,int) or isinstance(q,bool) or not 1<=q<=1024 for q in config['quotas'].values())
        or not isinstance(config.get('seed'),int) or not 0<config.get('action_seconds',0)<=config.get('study_seconds',0)<=3600
        or not isinstance(config.get('route_work'),int) or not 1<=config['route_work']<=200000):
        raise ValueError(f'pose-teachers.invalid-study-config: {args.config}')
    parent=json.loads((args.parent/'protocol.json').read_text());pr=json.loads((args.parent/'report.json').read_text())
    if pr['status']!='complete' or pr['protocol_sha256']!=sha(args.parent/'protocol.json') or len(parent['cases'])!=1:
        raise ValueError('pose-teachers.parent-protocol')
    sources=dict(parent['sources'])
    for file,h in sources.items():
        if sha(file)!=h:raise ValueError(f'pose-teachers.parent-input: {file}')
    for file,h in parent['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'pose-teachers.parent-source: {file}')
    case=parent['cases'][0]
    if not 65<=case['instances']<=128 or case['family_split']!='train':raise ValueError('pose-teachers.expected-large-train-family')
    geometry=next(row for row in pr['rows'] if row['method']=='geometry')
    if not geometry['budget_complete'] or geometry['selected']!=8:raise ValueError('pose-teachers.geometry-control-incomplete')
    geometry_path=Path(geometry['directory'])/'best.json.gz';compacted=json.loads(gzip.decompress(geometry_path.read_bytes()))
    for f in [args.parent/'protocol.json',args.parent/'report.json',args.config,args.native_library,geometry_path]:sources[str(f.resolve())]=sha(f)
    args.output.mkdir();initial_path=args.output/'eight-contractions.json';write(initial_path,compacted)
    cases=[dict(case,state='original'),dict(case,id=case['id']+'--eight-contractions',state='eight-contractions',initial=str(initial_path.resolve()))]
    for c in cases:
        c['initial_sha256']=sha(c['initial']);sources[c['initial']]=c['initial_sha256']
        c['files_sha256']=dict(c['files_sha256'],**{'initial.json':c['initial_sha256']})
    runtime=FactoryRuntime(args.native_library,parent['catalogs']);prepared=[];rows=[];state_rows=[];status='complete'
    try:
        for i,c in enumerate(cases):
            env=FactoryEpisode(runtime,c,horizon=1,elapsed=config['action_seconds'],work=config['route_work'])
            desc=env.describe();actions,populations=selected_poses(desc,env.state['current'],config['quotas'],config['seed']+i)
            prepared.append(dict(case=c,actions=actions,populations=populations,description_seconds=desc['seconds'],initial_metrics=env.state['initial_metrics']))
        protocol=dict(schema_version=1,experiment='large-original-pose-teachers',parent=str(args.parent.resolve()),config=config,
            cases=cases,prepared=prepared,catalogs=parent['catalogs'],sources=sources,source_sha256=source_manifest(),
            state_count=2,distinct_originals=1,acquisition_role='opened-development-teacher-screen',training_performed=False)
        write(args.output/'protocol.json',protocol)
        for prepared_case in prepared:
            c=prepared_case['case'];directory=args.output/c['state'];directory.mkdir();best=json.loads(Path(c['initial']).read_text())
            best_metrics=prepared_case['initial_metrics'];by_stratum={};local=[]
            for selection in prepared_case['actions']:
                remaining=config['study_seconds']-(time.perf_counter()-began)
                if remaining<=0:status='budget_exhausted';break
                env=FactoryEpisode(runtime,c,horizon=1,elapsed=min(config['action_seconds'],remaining),work=config['route_work'])
                before=env.state['current'];trace=env.step(selection['action'])
                valid=trace['status'] in ['improved','accepted_equal','accepted_exploration']
                move=displacement(before,env.state['current'],selection['action']) if valid else None
                row=dict(id=len(rows),state=c['state'],**selection,trace=trace,completed=trace['status'] not in ['late','budget_exhausted'],
                    factory_validated=valid,displacement=move,current_sha256=digest(env.state['current']),
                    candidate_metrics=env.state['current_metrics'] if valid else None)
                rows.append(row);local.append(row)
                if valid:
                    kind=selection['stratum'];metric=env.state['current_metrics']
                    if kind not in by_stratum or metric['score']<by_stratum[kind]['score']:
                        candidate_path=directory/f'{kind}-candidate.json.gz';save_gzip(candidate_path,env.state['current'])
                        by_stratum[kind]=dict(row=row['id'],score=metric['score'],path=str(candidate_path.resolve()),displacement=move)
                    if metric['score']<best_metrics['score']:best=env.state['current'];best_metrics=metric
                if len(local)%24==0:print(json.dumps(dict(state=c['state'],evaluated=len(local),valid=sum(r['factory_validated'] for r in local),
                    improved=sum(r['trace']['status']=='improved' for r in local),seconds=time.perf_counter()-began)),flush=True)
            save_gzip(directory/'best.json.gz',best)
            state_rows.append(dict(state=c['state'],selected=len(local),planned=len(prepared_case['actions']),
                initial_metrics=prepared_case['initial_metrics'],best_metrics=best_metrics,best_path=str((directory/'best.json.gz').resolve()),
                statuses=dict(Counter(r['trace']['status'] for r in local)),valid=sum(r['factory_validated'] for r in local),
                translated=sum(bool(r['displacement'] and r['displacement']['translated']) for r in local),selected_candidates=by_stratum,
                route_expansions=sum(r['trace']['route_expansions'] for r in local)))
            save_gzip(args.output/'outcomes.json.gz',rows)
            write(args.output/'progress.json',dict(status='incomplete',states=state_rows))
            if status!='complete':break
    finally:runtime.close()
    for file,h in sources.items():
        if sha(file)!=h:raise ValueError(f'pose-teachers.changed-input: {file}')
    for file,h in protocol['source_sha256'].items():
        if sha(ROOT/file)!=h:raise ValueError(f'pose-teachers.changed-source: {file}')
    write(args.output/'report.json',dict(schema_version=1,status=status,protocol_sha256=sha(args.output/'protocol.json'),
        seconds=time.perf_counter()-began,selected=len(rows),planned=sum(len(p['actions']) for p in prepared),states=state_rows,
        outcomes_sha256=sha(args.output/'outcomes.json.gz'),training_performed=False))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['parent','native-library','config','output']:parser.add_argument('--'+name,required=True,type=Path)
    run(parser.parse_args())
