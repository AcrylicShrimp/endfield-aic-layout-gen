"""Bounded sampling/history/horizon comparison on complete validated factories."""
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

ARMS=[dict(id='flat',balanced=False,memory=False,horizon=8),
      dict(id='balanced',balanced=True,memory=False,horizon=8),
      dict(id='memory',balanced=False,memory=True,horizon=8),
      dict(id='balanced-memory',balanced=True,memory=True,horizon=8),
      dict(id='longer',balanced=True,memory=True,horizon=32)]
CONTROLS=[dict(id='uniform-flat',balanced=False,memory=False,horizon=8,geometry=False),
          dict(id='uniform-memory',balanced=True,memory=True,horizon=32,geometry=False),
          dict(id='geometry-memory',balanced=False,memory=True,horizon=32,geometry=True)]
BANKS=(1190000,1200000,1210000,1220000)


def episode(runtime,case,policy,config,seed,args,training=False):
    began=time.perf_counter()
    env=FactoryEpisode(runtime,case,config['horizon'],args.episode_seconds,args.route_work)
    memory=SelectionMemory(env.state['current'])
    current_key=best_key=physical_key(env.state['current'])
    rng=torch.Generator().manual_seed(seed)
    logps,entropies=[],[]
    observation_seconds=selection_seconds=0
    while not env.done:
        start=time.perf_counter()
        desc=env.describe(config.get('vocabulary'))
        obs=observe(env.state['current'],desc,env.state,1-env.selected/config['horizon']) if policy is not None else None
        observation_seconds+=time.perf_counter()-start
        start=time.perf_counter()
        keys=[action_key(a,desc,env.state['current'],best_key) for a in desc['actions']]
        allowed,exhausted=memory.mask(current_key,keys,desc['actions'],config['memory'])
        if env.done:break
        with torch.set_grad_enabled(training):
            logits=policy(obs) if policy is not None else torch.zeros(len(allowed))
            dist=distribution(logits,desc['actions'],allowed,config['balanced'])
            index=(geometry_choice(env.state['current'],desc['actions'],allowed) if config.get('geometry') else
                   int(torch.multinomial(dist.probs.detach().cpu(),1,generator=rng)))
            if training:
                logp=dist.log_prob(torch.tensor(index,device=logits.device))
                entropy=dist.entropy()
        selection_seconds+=time.perf_counter()-start
        if env.done:break
        before=current_key
        row=env.step(desc['actions'][index])
        start=time.perf_counter()
        current_key=before if row['unchanged'] else physical_key(env.state['current'])
        physical_unchanged,physical_revisited=memory.record(before,current_key,keys[index])
        if row['status']=='improved':best_key=current_key
        row.update(poses=desc['poses'],physical_before=before,physical_after=current_key,physical_unchanged=physical_unchanged,
                   physical_revisited=physical_revisited,action_key=keys[index],available=len(allowed),
                   masked=sum(not x for x in allowed),memory_exhausted=exhausted)
        selection_seconds+=time.perf_counter()-start
        if training:
            logps.append(logp);entropies.append(entropy)
    summary=env.summary()
    summary.update(sample_seed=seed,config=config,observation_seconds=observation_seconds,selection_seconds=selection_seconds,
                   total_seconds=time.perf_counter()-began,physical_best_sha256=best_key,
                   statuses=dict(Counter(t['status'] for t in env.trace)),kinds=dict(Counter(t['action']['kind'] for t in env.trace)),
                   diagnostics=dict(Counter(d['code'] for t in env.trace for d in t['diagnostics'])),
                   route_expansions=sum(t['route_expansions'] for t in env.trace),
                   unchanged=sum(t['unchanged'] for t in env.trace),revisited=sum(t['revisited'] for t in env.trace),
                   physical_unchanged=sum(t['physical_unchanged'] for t in env.trace),
                   physical_revisited=sum(t['physical_revisited'] for t in env.trace),
                   masked=sum(t['masked'] for t in env.trace),memory_exhausted=sum(t['memory_exhausted'] for t in env.trace))
    return summary,env.state['best'],logps,entropies


def run(args):
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    started=time.perf_counter()
    def guard():
        if time.perf_counter()-started>=args.screen_seconds:raise TimeoutError('factory.landscape-budget')
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
    protocol=dict(schema_version=1,experiment='selection-landscape-v1',observation_schema=SCHEMA,
                  arms=ARMS,controls=CONTROLS,seed=17,banks=BANKS,training_actions=args.training_actions,
                  evaluation_actions=32,group_episodes=4,training_roots=TRAIN_ROOTS,cases=cases,
                  route_work=args.route_work,episode_seconds=args.episode_seconds,screen_seconds=args.screen_seconds,
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
                attempts=[]
                for attempt in range(32//config['horizon']):
                    guard()
                    result,best,_,_=episode(runtime,case,policy,config,bank+attempt,args)
                    if result['selected']!=config['horizon']:raise TimeoutError(f'factory.evaluation-episode-budget: {case["id"]}')
                    result.update(arm=label,phase='evaluation',bank=bank,attempt=attempt)
                    attempts.append(retain(result,best,f'{label}--{case["id"]}--{bank}--{attempt}'))
                chosen=min(attempts,key=lambda r:r['best_metrics']['score'])
                cells.append(dict(arm=label,case=case['id'],bank=bank,instances=case['instances'],
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
        write(args.output/'landscape.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
                                              seconds=time.perf_counter()-started,rows=rows,cells=cells,groups=groups))
    except BaseException as error:
        write(args.output/'partial.json',dict(schema_version=1,status='incomplete',error=str(error),rows=rows,cells=cells,groups=groups))
        raise
    finally:runtime.close()
    print(json.dumps(dict(status='complete',episodes=len(rows),cells=len(cells),seconds=time.perf_counter()-started)),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('inputs','strong','incumbent','native-library','output'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--training-actions',type=int,default=4096)
    parser.add_argument('--route-work',type=int,default=5000)
    parser.add_argument('--episode-seconds',type=float,default=30)
    parser.add_argument('--screen-seconds',type=float,default=1800)
    args=parser.parse_args()
    if args.training_actions<128 or args.training_actions%128 or min(args.route_work,args.episode_seconds,args.screen_seconds)<=0:
        parser.error('Training actions must be a positive multiple of 128; all work/time bounds must be positive')
    run(args)
