"""Bounded boundary-release teachers using only offered complete-factory actions."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import FactoryRuntime, FactoryEpisode
from full_factory_selection import physical_key
from full_factory_screen import sha, write, save_gzip


def cells(p):
    return ((x, y) for y in range(p['y'], p['y']+p['height']) for x in range(p['x'], p['x']+p['width']))


def boundary(side, initial):
    axis = 1 if side == 'south' else 0
    edge = initial['height']-2 if side == 'south' else 1
    def weight(xy):
        return max(0, xy[axis]-edge+1 if side == 'south' else edge-xy[axis]+1)
    return weight


def potential(w, metrics, weight):
    bodies = {xy for p in w['placements'] for xy in cells(p)}
    anchors = {(t['position']['x'], t['position']['y']) for n in w['transport_networks'] for t in n['terminals']}
    anchors.update((c['position']['x'], c['position']['y']) for c in w['logistics_components'])
    occupied = bodies | anchors | {(p['x'], p['y']) for n in w['transport_networks'] for p in n['cells']}
    return [metrics['score'][0], sum(map(weight, bodies)), sum(map(weight, anchors)),
            sum(map(weight, occupied)), *metrics['score'][1:]]


def shortlist(w, desc, weight):
    """Geometry-only teacher ranking; no execution or hidden successor scoring."""
    placements = w['placements'];near = set()
    for p in placements:
        # Three-cell neighborhood around the initial band includes approach ports.
        expanded = dict(p, x=p['x']-3, y=p['y']-3, width=p['width']+6, height=p['height']+6)
        if any(weight(xy) for xy in cells(expanded)):near.add(p['instance'])
    pools = {kind: [] for kind in ('pose_gain', 'pose_near', 'port', 'other')}
    for ordinal, action in enumerate(desc['actions']):
        kind = action['kind'];gain = 0
        if kind == 'pose':
            for q in action['poses']:
                p=placements[q['member']];swap=abs(q['rotation']-p['rotation'])%180!=0
                final=dict(p,x=q['x'],y=q['y'],width=p['height'] if swap else p['width'],height=p['width'] if swap else p['height'])
                gain+=sum(map(weight,cells(p)))-sum(map(weight,cells(final)))
            if gain>0:pool='pose_gain'
            elif any(placements[q['member']]['instance'] in near for q in action['poses']):pool='pose_near'
            else:continue
        elif kind == 'port':
            if action['instance'] not in near:continue
            pool=kind
        elif kind in ('component', 'release_bridge', 'reroute'):
            if kind == 'reroute':points = desc['chains'][action['chain']]['cells']
            elif kind == 'component':points = [next(c['position'] for c in w['logistics_components'] if c['id']==action['component'])]
            else:points = [action]
            if not any(weight((p['x']+dx, p['y']+dy)) for p in points for dx,dy in ((0,0),(3,0),(-3,0),(0,3),(0,-3))):continue
            pool = 'other'
        elif kind == 'contract':pool = 'other';gain = 1
        else:continue
        pools[pool].append((-gain, ordinal, action))
    selected = []
    for kind, limit in (('pose_gain',24),('pose_near',8),('port',8),('other',8)):
        selected.extend(sorted(pools[kind],key=lambda v:v[:2])[:limit])
    return [a for _,_,a in selected], {kind: len(values) for kind,values in pools.items()}


def run(args):
    began = time.perf_counter();inputs = json.loads(args.inputs.read_text())
    case = next(c for c in inputs['rows'] if c['id']==args.case)
    for field,name in [('initial','initial.json'),('request','request.json'),('original','original-request.json')]:
        if sha(case[field]) != case['files_sha256'][name]:raise ValueError(f'factory.input-hash: {case[field]}')
    case = dict(case, initial=str(args.initial.resolve()))
    args.output.mkdir();scratch = args.output/'candidate-input.json'
    protocol = dict(schema_version=1,experiment='full-factory-boundary-trajectory-v1',side=args.side,
                    beam=args.beam,depth=args.depth,selected_limit=args.selected,seconds=args.seconds,route_work=args.route_work,
                    case=case,initial_sha256=sha(args.initial),inputs=str(args.inputs.resolve()),inputs_sha256=sha(args.inputs),
                    catalogs=inputs['catalogs'],native_sha256=sha(args.native_library),source_sha256=source_manifest())
    write(args.output/'protocol.json',protocol)
    runtime = FactoryRuntime(args.native_library,inputs['catalogs']);nodes={};trace=[];frontier=[];seen={};best=0
    selected=0;depths=[];status='incomplete';area_node=None;counts=Counter()
    def unpack(node):return json.loads(gzip.decompress(nodes[node]['payload']))
    def preserve(report_name):
        retained=set(frontier+[best]+([area_node] if area_node is not None else []))
        for node in list(retained):
            while nodes[node]['parent'] is not None:node=nodes[node]['parent'];retained.add(node)
        for node in retained:save_gzip(args.output/f'node-{node}.json.gz',unpack(node))
        write(args.output/report_name,dict(status=status,protocol_sha256=sha(args.output/'protocol.json'),selected=selected,
              seconds=time.perf_counter()-began,counts=dict(counts),initial=nodes[0]['metrics'],best=best,area_node=area_node,
              frontier=frontier,depths=depths,trace=trace,nodes=[{k:v for k,v in node.items() if k!='payload'} for node in nodes.values()],
              retained_nodes=sorted(retained)))
    try:
        root=FactoryEpisode(runtime,case,1,args.seconds,args.route_work)
        initial=root.state['current_metrics'];weight=boundary(args.side,initial)
        def admit(w,metrics,parent,action,depth):
            node=len(nodes);identity=physical_key(w)
            nodes[node]=dict(id=node,parent=parent,action=action,depth=depth,metrics=metrics,physical_sha256=identity,
                             potential=potential(w,metrics,weight),payload=gzip.compress(json.dumps(w).encode(),mtime=0))
            seen[identity]=node;return node
        admit(root.state['current'],initial,None,None,0);frontier=[0]
        for depth in range(args.depth):
            queues=[];children=[];summary=dict(depth=depth,parents=frontier[:],pools=[],selected=0,unique_children=0)
            for parent in frontier:
                if time.perf_counter()-began>=args.seconds:status='time_limit';break
                w=unpack(parent);actions,pools=shortlist(w,runtime.rpc('describe',w,options={}),weight)
                queues.append((parent,actions));summary['pools'].append(dict(parent=parent,offered=pools,shortlisted=len(actions)))
            if status=='time_limit':break
            # Round-robin across parents prevents the first parent consuming the last budget.
            for choice in range(max((len(q) for _,q in queues),default=0)):
                for parent,actions in queues:
                    if choice>=len(actions):continue
                    if selected>=args.selected:status='selected_limit';break
                    remaining=args.seconds-(time.perf_counter()-began)
                    if remaining<=0:status='time_limit';break
                    w=unpack(parent);write(scratch,w)
                    remaining=args.seconds-(time.perf_counter()-began)
                    if remaining<=0:status='time_limit';break
                    env=FactoryEpisode(runtime,dict(case,initial=str(scratch.resolve())),1,remaining,args.route_work)
                    action=actions[choice];result=env.step(action);selected+=1;summary['selected']+=1
                    if time.perf_counter()-began>=args.seconds:result['status']='late';status='time_limit'
                    counts[result['status']]+=1
                    row=dict(parent=parent,action=action,selected=selected,result=result,child=None)
                    if not result['unchanged'] and result['status']!='late':
                        current=env.state['current'];identity=physical_key(current)
                        if identity in seen:row['revisited_node']=seen[identity]
                        else:
                            child=admit(current,env.state['current_metrics'],parent,action,depth+1)
                            row['child']=child;children.append(child);summary['unique_children']+=1
                            if nodes[child]['metrics']['score']<nodes[best]['metrics']['score']:best=child
                            if nodes[child]['metrics']['score'][0]<initial['score'][0]:area_node=child;status='area_trajectory'
                    trace.append(row)
                    if status in ('selected_limit','time_limit','area_trajectory'):break
                if status in ('selected_limit','time_limit','area_trajectory'):break
            depths.append(summary)
            if status in ('selected_limit','time_limit','area_trajectory'):break
            frontier=sorted(children,key=lambda i:(nodes[i]['potential'],i))[:args.beam]
            if not frontier:status='frontier_exhausted';break
            print(json.dumps(dict(depth=depth+1,selected=selected,children=len(children),potential=nodes[frontier[0]]['potential'],
                                  best=nodes[best]['metrics']['score'],seconds=time.perf_counter()-began)),flush=True)
        else:status='depth_limit'
        preserve('trajectory.json');scratch.unlink(missing_ok=True)
        print(json.dumps(dict(status=status,selected=selected,nodes=len(nodes),best=nodes[best]['metrics']['score'],seconds=time.perf_counter()-began)))
    except BaseException:
        if nodes:preserve('partial.json')
        raise
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('inputs','initial','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--case',required=True);p.add_argument('--side',choices=('south','west'),required=True)
    p.add_argument('--beam',type=int,default=8);p.add_argument('--depth',type=int,default=12)
    p.add_argument('--selected',type=int,default=4608);p.add_argument('--seconds',type=float,default=180)
    p.add_argument('--route-work',type=int,default=5000)
    args=p.parse_args()
    if not 1<=args.beam<=32 or not 1<=args.depth<=32 or args.selected<1 or args.seconds<=0:p.error('Invalid trajectory budgets')
    run(args)
