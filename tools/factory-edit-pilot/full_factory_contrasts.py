"""Collect charged, original-root contrasts with ordinary pose and route actions."""
import argparse
from collections import Counter
import json
from pathlib import Path
import random
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT, FactoryRuntime, FactoryEpisode
from full_factory_screen import sha, write, save_gzip


def shortlist(w, d, seed):
    actions=d['actions'];selected=[]
    inserts={(a['axis'],a['position']):a for a in actions if a['kind']=='insert'}
    pairs=[(a,inserts[a['axis'],a['position']]) for a in actions if a['kind']=='contract' and (a['axis'],a['position']) in inserts][:4]
    for pair in pairs:selected.extend(pair)
    centers=[sum(p[axis]+p[extent]/2 for p in w['placements'])/len(w['placements'])
             for axis,extent in [('x','width'),('y','height')]]
    def gain(a):
        total=0
        for q in a['poses']:
            p=w['placements'][q['member']];swap=abs(q['rotation']-p['rotation'])%180!=0
            width,height=(p['height'],p['width']) if swap else (p['width'],p['height'])
            total+=sum(abs(p[axis]+p[extent]/2-c)-abs(q[axis]+size/2-c)
                       for axis,extent,size,c in [('x','width',width,centers[0]),('y','height',height,centers[1])])
        return total/len(a['poses'])
    poses=[a for a in actions if a['kind']=='pose'];poses.sort(key=gain,reverse=True)
    selected.extend(poses[:16]);rest=poses[16:];random.Random(seed).shuffle(rest);selected.extend(rest[:16])
    routes=[a for a in actions if a['kind']=='reroute']
    routes.sort(key=lambda a:(a['transport']!='pipe',-len(d['chains'][a['chain']]['cells'])))
    selected.extend(routes[:8]);selected.extend([a for a in actions if a['kind']=='port'][:4])
    return selected


def run(args):
    began=time.perf_counter();parent=json.loads((args.parent/'protocol.json').read_text())
    original_report=json.loads((args.parent/'report.json').read_text())
    if original_report['status']!='complete' or original_report['protocol_sha256']!=sha(args.parent/'protocol.json'):
        raise ValueError('contrast.parent-incomplete')
    for p,h in parent['sources'].items():
        if sha(p)!=h:raise ValueError(f'contrast.parent-input: {p}')
    args.output.mkdir();rows=[];cases=[]
    protocol=dict(schema_version=1,parent=str(args.parent.resolve()),parent_protocol_sha256=sha(args.parent/'protocol.json'),
        cases=parent['cases'],catalogs=parent['catalogs'],seed=90217,root_seconds=60,seconds=900,route_work=5000,
        max_pairs=4,geometry_poses=16,sampled_poses=16,reroutes=8,ports=4,source_sha256=source_manifest(),native_sha256=sha(args.native_library))
    write(args.output/'protocol.json',protocol);runtime=FactoryRuntime(args.native_library,parent['catalogs'])
    try:
        for ci,case in enumerate(parent['cases']):
            if time.perf_counter()-began>=protocol['seconds']:raise TimeoutError('contrast.study-budget')
            start=time.perf_counter();root=FactoryEpisode(runtime,case,1,60,5000);d=root.describe()
            actions=shortlist(root.state['current'],d,90217+ci);local=[]
            for ordinal,a in enumerate(actions):
                remaining=min(60-(time.perf_counter()-start),900-(time.perf_counter()-began))
                if remaining<=0:break
                env=FactoryEpisode(runtime,case,1,remaining,5000);result=env.step(a)
                row=dict(id=f'{case["id"]}-{ordinal}',root=case['id'],family=case['family'],split=case['split'],action=a,
                    initial_metrics=root.state['initial_metrics'],result=result,
                    completed=result['status'] not in ('late','budget_exhausted'),
                    improving=result['status']=='improved',best_metrics=env.state['best_metrics'])
                if row['improving']:
                    p=args.output/(row['id']+'.json.gz');save_gzip(p,env.state['best']);row['best']=str(p.resolve());row['best_sha256']=sha(p)
                rows.append(row);local.append(row)
            summary=dict(case=case['id'],family=case['family'],split=case['split'],offered=len(d['actions']),shortlisted=len(actions),
                         selected=len(local),seconds=time.perf_counter()-start,status='complete' if len(local)==len(actions) else 'root-budget',
                         improvements=dict(Counter(r['action']['kind'] for r in local if r['improving'])),
                         outcomes=dict(Counter(r['result']['status'] for r in local)))
            cases.append(summary);write(args.output/(case['id']+'-attempts.json'),local)
            print(json.dumps(summary),flush=True)
        for p,h in protocol['source_sha256'].items():
            if sha(ROOT/p)!=h:raise ValueError(f'contrast.source-changed: {p}')
        write(args.output/'contrasts.json',dict(schema_version=1,status='measured',protocol_sha256=sha(args.output/'protocol.json'),
                                               seconds=time.perf_counter()-began,cases=cases,rows=rows))
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',code='contrast.failed',stage='collection',message=str(error),cases=cases,rows=rows));raise
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('parent','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
