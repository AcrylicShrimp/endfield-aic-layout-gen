"""Post-screen fixed-root routing probe into an explicitly selected empty rectangle."""
import argparse
from collections import Counter
import json
from pathlib import Path
import time

from full_factory_environment import FactoryRuntime,FactoryEpisode
from full_factory_screen import sha,write


def run(args):
    started=time.perf_counter();inputs=json.loads(args.inputs.read_text())
    case=next((c for c in inputs['rows'] if c['id']==args.case),None)
    if case is None:raise ValueError(f'factory.hole-case: {args.inputs}: unknown case {args.case}')
    case=dict(case,initial=str(args.witness.resolve()))
    runtime=FactoryRuntime(args.native_library,inputs['catalogs']);rows=[]
    try:
        initial=FactoryEpisode(runtime,case,horizon=1,elapsed=args.seconds,work=args.route_work)
        desc=initial.describe();x,y,width,height=args.rectangle
        candidates=[a for a in desc['actions'] if a['kind']=='relocate' and a['x']>=x and a['y']>=y and
                    a['x']+a['width']<=x+width and a['y']+a['height']<=y+height]
        count=min(args.selections,len(candidates))
        indices=([round(i*(len(candidates)-1)/(count-1)) for i in range(count)] if count>1 else list(range(count)))
        args.output.mkdir()
        write(args.output/'protocol.json',dict(schema_version=1,case=case,rectangle=args.rectangle,
              selection='Evenly spaced indices across root vocabulary order, without routing pre-evaluation',
              selected_indices=indices,candidates=candidates,relocation=desc['relocation'],
              inputs_sha256=sha(args.inputs),witness_sha256=sha(args.witness),seconds=args.seconds,
              route_work=args.route_work,native_sha256=sha(args.native_library),source_sha256=sha(Path(__file__))))
        for index in indices:
            remaining=args.seconds-(time.perf_counter()-started)
            if remaining<=0:break
            env=FactoryEpisode(runtime,case,horizon=1,elapsed=remaining,work=args.route_work)
            if env.done:break
            row=env.step(candidates[index]);row['candidate_index']=index
            if not row['unchanged']:write(args.output/f'candidate-{index}.json',env.state['current'])
            rows.append(row)
        result=dict(schema_version=1,status='complete' if len(rows)==len(indices) else 'time_budget',
                    candidates=len(candidates),selected=len(rows),changed=sum(not r['unchanged'] for r in rows),
                    diagnostics=dict(Counter(d['code'] for r in rows for d in r['diagnostics'])),
                    initial_metrics=initial.state['initial_metrics'],trace=rows,seconds=time.perf_counter()-started,
                    scope='Post-selected empty-region root probe; separate from learning evaluation; no global impossibility proof')
        write(args.output/'report.json',result)
        print(json.dumps({k:v for k,v in result.items() if k!='trace'}))
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('inputs','witness','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--case',required=True);p.add_argument('--rectangle',type=int,nargs=4,required=True,metavar=('X','Y','WIDTH','HEIGHT'))
    p.add_argument('--selections',type=int,default=32);p.add_argument('--seconds',type=float,default=60)
    p.add_argument('--route-work',type=int,default=5000);args=p.parse_args()
    if args.selections<1 or args.seconds<=0 or not 1<=args.route_work<=200000 or min(args.rectangle[2:])<=0:p.error('Positive selection/time/rectangle sizes and route work 1..200000 required')
    run(args)
