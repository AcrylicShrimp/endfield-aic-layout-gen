"""Root PG, inverse-curriculum PG and imitation pilots on complete factories."""
import argparse
from collections import Counter
import json
from pathlib import Path
import random
import time

import torch

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT,FactoryRuntime,FactoryEpisode
from full_factory_landscape import episode
from full_factory_policy import SCHEMA,FullFactoryPolicy,observe
from full_factory_screen import advantages,sha,save_gzip,write

ARMS=[dict(id='root-pg',depth=0,method='root_pg'),dict(id='mixed-pg',depth=0,method='mixed_pg'),
      dict(id='inverse-d0',depth=0,method='imitation'),dict(id='inverse-d2',depth=2,method='imitation')]
CONFIG=dict(horizon=8,balanced=False,memory=False,relocation=dict(mode='groups'))
BANKS=(1250000,1260000)


def run(args):
    began=time.perf_counter();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    data=json.loads((args.curriculum/'curriculum.json').read_text());admission=json.loads((args.curriculum/'protocol.json').read_text())
    if data['status']!='complete':raise ValueError('factory.curriculum-not-admitted')
    if sha(args.curriculum/'protocol.json')!=data['protocol_sha256']:raise ValueError('factory.curriculum-protocol-hash')
    for pair in data['pairs']:
        if sha(pair['case']['initial'])!=pair['case']['initial_sha256']:raise ValueError(f'factory.curriculum-input-hash: {pair["id"]}')
    cases=list(admission['cases'])
    if args.additional_heavy:
        original=next(c for c in cases if c['id']=='closure-original')
        cases.append(dict(original,id='closure-generator',initial=str(args.additional_heavy.resolve()),initial_sha256=sha(args.additional_heavy)))
    for case in cases:
        if sha(case['initial'])!=case['initial_sha256']:raise ValueError(f'factory.root-input-hash: {case["initial"]}')
    for file,expected in admission['source_sha256'].items():
        if sha(ROOT/file)!=expected:raise ValueError(f'factory.admission-source-hash: {file}')
    roots=[next(c for c in cases if c['id']==id) for id in admission['training_roots']]
    training=[p for p in data['pairs'] if p['split']=='training'];validation=[p for p in data['pairs'] if p['split']=='validation']
    lessons={c['id']:[p for p in training if p['root']==c['id']] for c in roots}
    args.output.mkdir()
    protocol=dict(schema_version=1,experiment='full-factory-inverse-curriculum-v1',observation_schema=SCHEMA,arms=ARMS,
                  config=CONFIG,banks=BANKS,cases=cases,training_roots=admission['training_roots'],training_pairs=[p['id'] for p in training],validation_pairs=[p['id'] for p in validation],
                  training_actions=args.training_actions,imitation_updates=args.imitation_updates,route_work=args.route_work,
                  cell_seconds=args.episode_seconds,screen_seconds=args.screen_seconds,seed=17,learning_rate=.0003,entropy=.01,gradient_clip=.5,
                  source_sha256=source_manifest(),native_sha256=sha(args.native_library),curriculum_path=str(args.curriculum.resolve()),
                  curriculum_sha256=sha(args.curriculum/'curriculum.json'),curriculum_protocol_sha256=sha(args.curriculum/'protocol.json'),
                  catalogs=admission['catalogs'],catalog_sha256=admission['catalog_sha256'])
    write(args.output/'protocol.json',protocol);runtime=FactoryRuntime(args.native_library,admission['catalogs']);rows=[];cells=[];groups=[];inverse_rows=[]
    def guard():
        if time.perf_counter()-began>args.screen_seconds:raise TimeoutError('factory.curriculum-screen-budget')
    def retain(summary,best,name):
        destination=args.output/name;destination.mkdir();save_gzip(destination/'episode.json.gz',summary);save_gzip(destination/'best.json.gz',best)
        row={k:v for k,v in summary.items() if k!='trace'};row['directory']=name;rows.append(row);return row
    def checkpoint(policy,label,optimizer=None,next_update=0):
        torch.save(dict(schema=SCHEMA,depth=policy.depth,policy=policy.state_dict(),optimizer=optimizer.state_dict() if optimizer else None,
                        rng=torch.get_rng_state(),next_update=next_update,protocol_sha256=sha(args.output/'protocol.json')),args.output/f'{label}.pt')
    def evaluate(policy,label):
        frozen={k:v.clone() for k,v in policy.state_dict().items()} if policy is not None else None
        for case in cases:
            for bank in BANKS:
                cell_start=time.perf_counter();attempts=[]
                for attempt in range(4):
                    guard();remaining=args.episode_seconds-(time.perf_counter()-cell_start)
                    if remaining<=0:raise TimeoutError(f'factory.evaluation-cell-budget: {case["id"]}')
                    bounded=argparse.Namespace(**vars(args));bounded.episode_seconds=remaining
                    summary,best,_,_=episode(runtime,case,policy,CONFIG,bank+attempt,bounded)
                    if summary['selected']!=8:raise TimeoutError(f'factory.evaluation-episode-budget: {case["id"]}')
                    summary.update(arm=label,phase='evaluation',bank=bank,attempt=attempt)
                    attempts.append(retain(summary,best,f'{label}--{case["id"]}--{bank}--{attempt}'))
                seconds=time.perf_counter()-cell_start
                if seconds>args.episode_seconds:raise TimeoutError('factory.evaluation-cell-recording-budget')
                chosen=min(attempts,key=lambda r:r['best_metrics']['score'])
                cells.append(dict(arm=label,case=case['id'],bank=bank,instances=case['instances'],seconds=seconds,
                                  initial_metrics=chosen['initial_metrics'],best_metrics=chosen['best_metrics'],selected=32,
                                  attempts=[r['directory'] for r in attempts],best=chosen['directory']))
        for index,pair in enumerate(validation):
            for selection in ('sample','greedy'):
                guard();env=FactoryEpisode(runtime,pair['case'],horizon=1,elapsed=args.episode_seconds,work=args.route_work);desc=env.describe()
                with torch.no_grad():
                    logits=policy(observe(env.state['current'],desc,env.state,1)) if policy is not None else torch.zeros(len(desc['actions']))
                    dist=torch.distributions.Categorical(logits=logits)
                    chosen=(int(logits.argmax()) if selection=='greedy' else int(torch.multinomial(dist.probs,1,generator=torch.Generator().manual_seed(1270000+index))))
                row=env.step(desc['actions'][chosen]);directory=f'{label}-inverse-{index}-{selection}'
                result=env.summary();result.update(arm=label,phase='inverse',selection=selection,pair=pair['id'],teacher_chosen=desc['actions'][chosen]==pair['inverse'])
                destination=args.output/directory;destination.mkdir();save_gzip(destination/'episode.json.gz',result);save_gzip(destination/'best.json.gz',env.state['best'])
                inverse_rows.append(dict(arm=label,pair=pair['id'],selection=selection,root=pair['root'],directory=directory,teacher_chosen=result['teacher_chosen'],status=row['status'],
                                         initial_metrics=result['initial_metrics'],best_metrics=result['best_metrics'],seconds=result['elapsed_seconds']))
        if policy is not None and not all(torch.equal(v,frozen[k]) for k,v in policy.state_dict().items()):raise ValueError('factory.frozen-policy-mutated')
        print(json.dumps(dict(event='evaluation',arm=label,cells=len(cells),inverse=len(inverse_rows),seconds=time.perf_counter()-began)),flush=True)
    cache={}
    def lesson_observation(pair):
        if pair['id'] not in cache:
            env=FactoryEpisode(runtime,pair['case'],horizon=1,elapsed=args.episode_seconds,work=args.route_work);desc=env.describe()
            cache[pair['id']]=(observe(env.state['current'],desc,env.state,1),desc['actions'].index(pair['inverse']))
        return cache[pair['id']]
    try:
        for depth in (0,2):
            torch.manual_seed(17);policy=FullFactoryPolicy(depth);checkpoint(policy,f'untrained-d{depth}');evaluate(policy,f'untrained-d{depth}')
        for arm in ARMS:
            guard();cache.clear();start=time.perf_counter();torch.manual_seed(17);policy=FullFactoryPolicy(arm['depth']);optimizer=torch.optim.Adam(policy.parameters(),lr=.0003)
            label=arm['id']
            if arm['method']=='imitation':
                order=list(training);random.Random(17).shuffle(order)
                for update in range(args.imitation_updates):
                    guard();pair=order[update%len(order)];obs,target=lesson_observation(pair);logits=policy(obs)
                    loss=-torch.log_softmax(logits,0)[target];optimizer.zero_grad();loss.backward()
                    grad=float(torch.nn.utils.clip_grad_norm_(policy.parameters(),.5))
                    if not torch.isfinite(loss) or not torch.isfinite(torch.tensor(grad)):raise ValueError('factory.nonfinite-imitation-gradient')
                    optimizer.step();record=dict(arm=label,update=update,pair=pair['id'],loss=float(loss.detach()),gradient_norm=grad,teacher_probability=float(torch.softmax(logits.detach(),0)[target]),teacher_top1=int(logits.detach().argmax())==target)
                    groups.append(record)
                    if (update+1)%128==0:
                        checkpoint(policy,label+'-progress',optimizer,update+1);print(json.dumps(dict(event='imitation',arm=label,update=update+1,seconds=time.perf_counter()-began)),flush=True)
            else:
                for group in range(args.training_actions//32):
                    guard();scores=[];terms=[];entropies=[]
                    if arm['method']=='mixed_pg':
                        root_index=(group//2)%len(roots);case=roots[root_index]
                        if group%2:case=lessons[case['id']][(group//(2*len(roots)))%len(lessons[case['id']])]['case']
                    else:case=roots[group%len(roots)]
                    for attempt in range(4):
                        guard();summary,best,logps,ents=episode(runtime,case,policy,CONFIG,1800000+group*4+attempt,args,training=True)
                        if summary['selected']!=8:raise TimeoutError('factory.training-episode-budget')
                        summary.update(arm=label,phase='training',group=group,attempt=attempt);retain(summary,best,f'{label}-train-{group}-{attempt}')
                        scores.append(tuple(summary['best_metrics']['score']));terms.append(torch.stack(logps).mean());entropies.extend(ents)
                    ranks=advantages(scores);loss=-(torch.stack(terms)*torch.tensor(ranks)).mean()-.01*torch.stack(entropies).mean()
                    optimizer.zero_grad();loss.backward();grad=float(torch.nn.utils.clip_grad_norm_(policy.parameters(),.5))
                    if not torch.isfinite(loss) or not torch.isfinite(torch.tensor(grad)):raise ValueError('factory.nonfinite-pg-gradient')
                    optimizer.step();groups.append(dict(arm=label,group=group,case=case['id'],scores=scores,ranks=ranks,loss=float(loss.detach()),gradient_norm=grad))
                    checkpoint(policy,label+'-progress',optimizer,group+1)
                    if (group+1)%32==0:print(json.dumps(dict(event='pg',arm=label,group=group+1,seconds=time.perf_counter()-began)),flush=True)
            updates=args.imitation_updates if arm['method']=='imitation' else args.training_actions//32
            checkpoint(policy,label+'-final',optimizer,updates);write(args.output/f'{label}-training.json',dict(arm=label,seconds=time.perf_counter()-start,updates=updates))
            write(args.output/'groups.json',groups);evaluate(policy,label+'-final')
        evaluate(None,'uniform')
        report=dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),seconds=time.perf_counter()-began,rows=rows,cells=cells,groups=groups,inverse=inverse_rows)
        write(args.output/'screen.json',report)
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',error=str(error),rows=rows,cells=cells,groups=groups,inverse=inverse_rows));raise
    finally:runtime.close()
    print(json.dumps(dict(status='complete',episodes=len(rows)+len(inverse_rows),cells=len(cells),seconds=report['seconds'])))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('curriculum','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--additional-heavy',type=Path)
    p.add_argument('--training-actions',type=int,default=4096);p.add_argument('--imitation-updates',type=int,default=1024)
    p.add_argument('--route-work',type=int,default=5000);p.add_argument('--episode-seconds',type=float,default=30);p.add_argument('--screen-seconds',type=float,default=1800)
    args=p.parse_args()
    if args.training_actions<32 or args.training_actions%32 or min(args.imitation_updates,args.route_work,args.episode_seconds,args.screen_seconds)<=0:p.error('Positive bounds and PG action count divisible by 32 required')
    run(args)
