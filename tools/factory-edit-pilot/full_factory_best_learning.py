"""Compare full-score best-action imitation and measured preferences on originals."""
import argparse
from collections import Counter
import json
from pathlib import Path
import random
import time

import numpy as np
import torch

from full_factory_curriculum import source_manifest
from full_factory_decoding import TemperaturePolicy
from full_factory_environment import ROOT, FactoryRuntime, FactoryEpisode
from full_factory_landscape import episode
from full_factory_policy import SCHEMA, FullFactoryPolicy, observe
from full_factory_ranking import ranking_loss
from full_factory_screen import sha, write, save_gzip


def teachers(rows):
    """Full-score minima among completed improving observations, including exact ties."""
    if len({r['id'] for r in rows})!=len(rows):raise ValueError('best-learning.duplicate-label')
    family_splits={};roots={}
    for r in rows:
        family_splits.setdefault(r['family'],set()).add(r['split'])
        roots.setdefault(r['root'],[]).append(r)
    if any(len(s)!=1 for s in family_splits.values()):raise ValueError('best-learning.family-leakage')
    result=[]
    for root,group in roots.items():
        if len({(r['family'],r['split']) for r in group})!=1:raise ValueError('best-learning.root-membership')
        positive=[r for r in group if r['completed'] and r['improving']]
        if not positive:continue
        score=min(r['best_metrics']['score'] for r in positive)
        result.append(dict(root=root,family=group[0]['family'],split=group[0]['split'],score=score,
            targets=[r['id'] for r in positive if r['best_metrics']['score']==score],
            worse=[r['id'] for r in group if r['completed'] and r['best_metrics']['score']>score]))
    return result


def schedule(lessons, updates=256, seed=17):
    train=[r for r in lessons if r['split']=='train' and r['worse']]
    if not train:raise ValueError('best-learning.no-training-teachers')
    rng=random.Random(seed);rng.shuffle(train);result=[]
    for i in range(updates):
        t=train[i%len(train)]
        result.append(dict(update=i,root=t['root'],targets=t['targets'],
                           positive=rng.choice(t['targets']),negative=rng.choice(t['worse'])))
    return result


def best_mass_loss(logits, targets):
    if not targets or len(set(targets))!=len(targets):raise ValueError('best-learning.target-set')
    return torch.logsumexp(logits,0)-torch.logsumexp(logits[targets],0)


def run(args):
    began=time.perf_counter();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    cp=json.loads((args.collection/'protocol.json').read_text());data=json.loads((args.collection/'contrasts.json').read_text())
    audit=json.loads((args.collection/'audit.json').read_text());parent=Path(cp['parent'])
    p=json.loads((parent/'protocol.json').read_text());parent_report=json.loads((parent/'report.json').read_text())
    if audit['status']!='passed' or audit['contrasts_sha256']!=sha(args.collection/'contrasts.json') or audit['protocol_sha256']!=sha(args.collection/'protocol.json'):
        raise ValueError('best-learning.collection-audit')
    if data['protocol_sha256']!=sha(args.collection/'protocol.json') or cp['parent_protocol_sha256']!=sha(parent/'protocol.json'):
        raise ValueError('best-learning.collection-provenance')
    if parent_report['status']!='complete' or parent_report['protocol_sha256']!=sha(parent/'protocol.json'):
        raise ValueError('best-learning.parent-incomplete')
    if cp['cases']!=p['cases'] or cp['catalogs']!=p['catalogs'] or cp['native_sha256']!=sha(args.native_library):
        raise ValueError('best-learning.collection-inputs')
    sources=dict(p['sources'])
    for f in [args.collection/n for n in ('protocol.json','contrasts.json','audit.json')]+[parent/n for n in ('protocol.json','report.json','learned.pt')]:
        sources[str(f.resolve())]=sha(f)
    for f,h in sources.items():
        if sha(f)!=h:raise ValueError(f'best-learning.input-hash: {f}')
    for manifest in (p['source_sha256'],cp['source_sha256']):
        for f,h in manifest.items():
            if sha(ROOT/f)!=h:raise ValueError(f'best-learning.source-hash: {f}')
    labels=data['rows'];lookup={r['id']:r for r in labels};lessons=teachers(labels);order=schedule(lessons)
    if audit['replayed']!=len(labels) or {r['id'] for r in audit['rows']}!=set(lookup):raise ValueError('best-learning.audit-membership')
    for r in audit['rows']:
        if any(r[k]!=lookup[r['id']][k] for k in ('root','split','completed','improving')):raise ValueError('best-learning.audit-label')
    args.output.mkdir();protocol=dict(schema_version=1,experiment='full-score-original-action-learning',parent=str(parent.resolve()),
        collection=str(args.collection.resolve()),cases=p['cases'],catalogs=p['catalogs'],sources=sources,source_sha256=source_manifest(),
        schema=SCHEMA,depth=p['depth'],width=p['width'],seed=17,updates=256,learning_rate=.0003,gradient_clip=.5,
        arms=['best-mass','best-pair'],teachers=lessons,schedule=order,observation_cache_bytes=128*1024*1024,screen_seconds=900,
        temperatures=[1,.05],horizon=8,banks=p['plan']['banks'],episode_seconds=30,route_work=5000)
    write(args.output/'protocol.json',protocol)
    checkpoint=torch.load(parent/'learned.pt',map_location='cpu',weights_only=False)
    if checkpoint['schema']!=SCHEMA or checkpoint['protocol_sha256']!=sha(parent/'protocol.json'):raise ValueError('best-learning.checkpoint-provenance')
    runtime=FactoryRuntime(args.native_library,p['catalogs']);cache={};cache_bytes=0;updates=[];scores=[];rows=[];stage='labels';arm=None
    def guard():
        if time.perf_counter()-began>=protocol['screen_seconds']:raise TimeoutError('best-learning.screen-budget')
    def observation(case):
        nonlocal cache_bytes
        if case['id'] in cache:return cache[case['id']]
        env=FactoryEpisode(runtime,case,8,30,5000);d=env.describe();obs=observe(env.state['current'],d,env.state,1)
        value=obs,d['actions'],{r['id']:d['actions'].index(r['action']) for r in labels if r['root']==case['id']}
        size=sum(v.nbytes for v in obs.values() if isinstance(v,np.ndarray))
        if cache_bytes+size<=protocol['observation_cache_bytes']:cache[case['id']]=value;cache_bytes+=size
        return value
    def label_scores(model,phase):
        for c in p['cases']:
            guard();obs,actions,indices=observation(c)
            with torch.no_grad():logits=model(obs)
            if not torch.isfinite(logits).all():raise ValueError('best-learning.nonfinite-logits')
            lesson=next((t for t in lessons if t['root']==c['id']),None);targets=[indices[i] for i in lesson['targets']] if lesson else []
            scores.append(dict(phase=phase,root=c['id'],family=c['family'],split=c['split'],teacher_present=bool(targets),
                targets=lesson['targets'] if lesson else [],top1=int(logits.argmax()) in targets,
                target_mass=float(torch.softmax(logits,0)[targets].sum()),top_action=actions[int(logits.argmax())],
                measured_logits={i:float(logits[index]) for i,index in indices.items()}))
    def save(model,optimizer,step,name):
        torch.save(dict(schema=SCHEMA,depth=p['depth'],width=p['width'],policy=model.state_dict(),optimizer=optimizer.state_dict(),
            rng=torch.get_rng_state(),step=step,arm=arm,protocol_sha256=sha(args.output/'protocol.json')),args.output/(name+'.pt'))
    try:
        cases={c['id']:c for c in p['cases']}
        for arm in protocol['arms']:
            torch.manual_seed(17);model=FullFactoryPolicy(p['depth'],p['width']);model.load_state_dict(checkpoint['policy'])
            optimizer=torch.optim.Adam(model.parameters(),lr=.0003)
            if arm==protocol['arms'][0]:label_scores(model,'parent')
            stage='training'
            for step in order:
                guard();obs,_,indices=observation(cases[step['root']]);logits=model(obs)
                loss=(best_mass_loss(logits,[indices[i] for i in step['targets']]) if arm=='best-mass' else
                      ranking_loss(logits,indices[step['positive']],indices[step['negative']]))
                optimizer.zero_grad();loss.backward();grad=float(torch.nn.utils.clip_grad_norm_(model.parameters(),.5))
                if not torch.isfinite(loss) or not np.isfinite(grad):raise ValueError('best-learning.nonfinite-training')
                optimizer.step();updates.append(dict(step,arm=arm,loss=float(loss.detach()),gradient_norm=grad))
                if (step['update']+1)%64==0:
                    save(model,optimizer,step['update']+1,arm+'-progress');write(args.output/'updates.json',updates)
                    print(json.dumps(dict(stage=stage,arm=arm,updates=step['update']+1,seconds=time.perf_counter()-began)),flush=True)
            save(model,optimizer,256,arm+'-final');model.eval();label_scores(model,arm);write(args.output/'label-scores.json',scores)
            frozen={k:v.clone() for k,v in model.state_dict().items()};stage='evaluation'
            config=dict(horizon=8,balanced=False,memory=True,geometry=False);bounded=argparse.Namespace(episode_seconds=30,route_work=5000)
            for t in protocol['temperatures']:
                actor=TemperaturePolicy(model,t)
                for c in p['cases']:
                    for bank in protocol['banks']:
                        guard();summary,best,_,_=episode(runtime,c,actor,config,bank,bounded)
                        directory=args.output/f'{arm}-t{t}--{c["id"]}--{bank}';directory.mkdir()
                        save_gzip(directory/'episode.json.gz',summary);save_gzip(directory/'best.json.gz',best)
                        improvements=Counter(a['action']['kind'] for a in summary['trace'] if a['status']=='improved')
                        rows.append(dict(arm=arm,temperature=t,case=c['id'],family=c['family'],split=c['split'],bank=bank,directory=str(directory.resolve()),
                            initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],selected=summary['selected'],
                            seconds=summary['total_seconds'],statuses=summary['statuses'],kinds=summary['kinds'],improvements=dict(improvements),
                            budget_complete=summary['selected']==8 and not any(a['status']=='late' for a in summary['trace'])))
                print(json.dumps(dict(stage=stage,arm=arm,temperature=t,episodes=len(rows),seconds=time.perf_counter()-began)),flush=True)
            if any(not torch.equal(v,frozen[k]) for k,v in model.state_dict().items()):raise ValueError('best-learning.policy-mutated')
        for f,h in sources.items():
            if sha(f)!=h:raise ValueError(f'best-learning.input-changed: {f}')
        for f,h in protocol['source_sha256'].items():
            if sha(ROOT/f)!=h:raise ValueError(f'best-learning.source-changed: {f}')
        write(args.output/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
            seconds=time.perf_counter()-began,cache_array_bytes=cache_bytes,updates=len(updates),policy_immutable=True,rows=rows))
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',code='best-learning.failed',severity='error',stage=stage,arm=arm,
                                             message=str(error),updates=updates,rows=rows));raise
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('collection','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
