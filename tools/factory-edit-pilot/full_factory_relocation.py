"""Controlled distant/group vocabulary learning on complete validated factories."""
import argparse
from collections import Counter
import json
from pathlib import Path
import platform
import re
import subprocess
import time

import torch

from full_factory_environment import ROOT,FactoryRuntime,FactoryEpisode,digest
from full_factory_policy import SCHEMA,FullFactoryPolicy,observe
from full_factory_screen import TRAIN_ROOTS,advantages,sha,save_gzip,write
from full_factory_selection import physical_key,action_key,SelectionMemory,distribution,geometry_choice

MODES=('local','filtered-local','distant','groups')
ARMS=[dict(id=mode,balanced=False,memory=False,horizon=8,vocabulary=dict(mode=mode)) for mode in MODES]
CONTROLS=[dict(a,id=a['id']+'-uniform') for a in ARMS]
BANKS=(1230000,1240000)


from full_factory_landscape import episode

def run(args):
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    started=time.perf_counter()
    def guard():
        if time.perf_counter()-started>=args.screen_seconds:raise TimeoutError('factory.relocation-screen-budget')
    inputs=json.loads(args.inputs.read_text())
    for row in inputs['rows']:
        for key,name in [('request','request.json'),('initial','initial.json'),('original','original-request.json')]:
            if sha(row[key])!=row['files_sha256'][name]:raise ValueError(f'factory.input-hash: {row[key]}')
    cases=list(inputs['rows'])
    original=next(r for r in cases if r['id']=='closure-original')
    cases.extend([dict(original,id='closure-strong',initial=str(args.strong.resolve())),
                  dict(original,id='closure-incumbent',initial=str(args.incumbent.resolve()))])
    # Freeze transitive JS kernels and the explicit Python source set before outcomes.
    pending=[Path(__file__).with_name('full_factory_executor.mjs')];js=set()
    while pending:
        p=pending.pop().resolve()
        if p in js:continue
        js.add(p)
        pending.extend((p.parent/v).resolve() for v in re.findall(r"from\s+['\"](\.[^'\"]+)['\"]",p.read_text()))
    sources=sorted(js|{p.resolve() for p in Path(__file__).parent.glob('full_factory*.py')})
    cats=json.loads(Path(inputs['catalogs']).read_text())
    protocol=dict(schema_version=1,experiment='relocation-vocabulary-v2',observation_schema=SCHEMA,
                  arms=ARMS,controls=CONTROLS,seed=17,banks=BANKS,training_actions=args.training_actions,
                  evaluation_actions=32,group_episodes=4,training_roots=TRAIN_ROOTS,cases=cases,
                  route_work=args.route_work,episode_seconds=args.episode_seconds,cell_seconds=args.episode_seconds,screen_seconds=args.screen_seconds,
                  learning_rate=.0003,entropy=.01,gradient_clip=.5,depth=0,width=32,
                  torch=torch.__version__,python=platform.python_version(),node=subprocess.check_output(['node','--version'],text=True).strip(),
                  inputs_sha256=sha(args.inputs),strong_sha256=sha(args.strong),incumbent_sha256=sha(args.incumbent),
                  native_sha256=sha(args.native_library),catalogs={k:dict(path=p,sha256=sha(p)) for k,p in cats.items()},
                  source_sha256={str(p.relative_to(ROOT)):sha(p) for p in sources})
    args.output.mkdir();write(args.output/'protocol.json',protocol)
    runtime=FactoryRuntime(args.native_library,inputs['catalogs'])
    write(args.output/'native-build.json',runtime.build)
    rows,groups,cells=[],[],[]
    def retain(summary,best,name):
        destination=args.output/name;destination.mkdir()
        save_gzip(destination/'episode.json.gz',summary);save_gzip(destination/'best.json.gz',best)
        row={k:v for k,v in summary.items() if k!='trace'};row['directory']=name;rows.append(row)
        return row
    def evaluate(policy,config,label):
        before={k:v.clone() for k,v in policy.state_dict().items()} if policy is not None else None
        for case in cases:
            for bank in BANKS:
                attempts=[];cell_started=time.perf_counter()
                for attempt in range(32//config['horizon']):
                    guard()
                    remaining=args.episode_seconds-(time.perf_counter()-cell_started)
                    if remaining<=0:raise TimeoutError(f'factory.evaluation-cell-budget: {case["id"]}')
                    cell_args=argparse.Namespace(**vars(args));cell_args.episode_seconds=remaining
                    result,best,_,_=episode(runtime,case,policy,config,bank+attempt,cell_args)
                    if result['selected']!=config['horizon']:raise TimeoutError(f'factory.evaluation-episode-budget: {case["id"]}')
                    result.update(arm=label,phase='evaluation',bank=bank,attempt=attempt)
                    attempts.append(retain(result,best,f'{label}--{case["id"]}--{bank}--{attempt}'))
                if time.perf_counter()-cell_started>args.episode_seconds:raise TimeoutError(f'factory.evaluation-cell-budget: {case["id"]}')
                chosen=min(attempts,key=lambda r:r['best_metrics']['score'])
                cells.append(dict(arm=label,case=case['id'],bank=bank,instances=case['instances'],seconds=time.perf_counter()-cell_started,
                                  initial_metrics=chosen['initial_metrics'],best_metrics=chosen['best_metrics'],
                                  selected=sum(r['selected'] for r in attempts),attempts=[r['directory'] for r in attempts],
                                  best=chosen['directory'],physical_best_sha256=chosen['physical_best_sha256']))
        if policy is not None and not all(torch.equal(v,before[k]) for k,v in policy.state_dict().items()):
            raise ValueError('factory.frozen-policy-mutated')
        print(json.dumps(dict(event='evaluation',arm=label,cells=len(cells),seconds=time.perf_counter()-started)),flush=True)
    try:
        for config in ARMS:
            torch.manual_seed(17)
            policy=FullFactoryPolicy(0)
            optimizer=torch.optim.Adam(policy.parameters(),lr=.0003)
            label=config['id']
            def checkpoint(name,next_group):
                torch.save(dict(schema=SCHEMA,config=config,policy=policy.state_dict(),optimizer=optimizer.state_dict(),
                                rng=torch.get_rng_state(),next_group=next_group,protocol_sha256=sha(args.output/'protocol.json')),
                           args.output/f'{label}-{name}.pt')
            checkpoint('untrained',0);evaluate(policy,config,label+'-untrained')
            for group in range(args.training_actions//(4*config['horizon'])):
                guard();case=next(r for r in cases if r['id']==TRAIN_ROOTS[group%len(TRAIN_ROOTS)])
                scores,terms,ents=[],[],[]
                for attempt in range(4):
                    guard()
                    result,best,logps,entropies=episode(runtime,case,policy,config,1700000+group*4+attempt,args,training=True)
                    result.update(arm=label,phase='training',group=group,attempt=attempt)
                    retain(result,best,f'{label}-train-{group}-{attempt}')
                    if result['selected']!=config['horizon']:raise TimeoutError('factory.training-episode-budget')
                    scores.append(tuple(result['best_metrics']['score']))
                    terms.append(torch.stack(logps).sum()/config['horizon']);ents.extend(entropies)
                ranks=advantages(scores)
                loss=-(torch.stack(terms)*torch.tensor(ranks)).mean()-.01*torch.stack(ents).mean()
                optimizer.zero_grad();loss.backward()
                grad=float(torch.nn.utils.clip_grad_norm_(policy.parameters(),.5))
                if not torch.isfinite(loss) or not torch.isfinite(torch.tensor(grad)):raise ValueError('factory.nonfinite-gradient')
                optimizer.step()
                record=dict(arm=label,group=group,case=case['id'],scores=scores,ranks=ranks,loss=float(loss.detach()),gradient_norm=grad)
                groups.append(record)
                with (args.output/'groups.jsonl').open('a') as stream:stream.write(json.dumps(record)+'\n')
                checkpoint('progress',group+1)
                if (group+1)%16==0:print(json.dumps(dict(event='training',arm=label,group=group+1,seconds=time.perf_counter()-started)),flush=True)
            checkpoint('final',args.training_actions//(4*config['horizon']));evaluate(policy,config,label+'-final')
        for config in CONTROLS:evaluate(None,config,config['id'])
        write(args.output/'relocation.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
                                              seconds=time.perf_counter()-started,rows=rows,cells=cells,groups=groups))
    except BaseException as error:
        write(args.output/'partial.json',dict(schema_version=1,status='incomplete',error=str(error),rows=rows,cells=cells,groups=groups))
        raise
    finally:runtime.close()
    print(json.dumps(dict(status='complete',episodes=len(rows),cells=len(cells),seconds=time.perf_counter()-started)),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('inputs','strong','incumbent','native-library','output'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--training-actions',type=int,default=1024)
    parser.add_argument('--route-work',type=int,default=5000)
    parser.add_argument('--episode-seconds',type=float,default=30)
    parser.add_argument('--screen-seconds',type=float,default=1800)
    args=parser.parse_args()
    if args.training_actions<128 or args.training_actions%128 or min(args.route_work,args.episode_seconds,args.screen_seconds)<=0:
        parser.error('Training actions must be a positive multiple of 128; all work/time bounds must be positive')
    run(args)
