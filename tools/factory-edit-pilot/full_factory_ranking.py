"""Train bounded same-root action rankings, then evaluate opened original factories."""
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
from full_factory_screen import sha, write, save_gzip


def training_pairs(rows, updates=256, seed=17):
    """Balance positive kinds; negatives are measured, completed, same-root outcomes."""
    family_splits={}
    for row in rows:
        family_splits.setdefault(row['family'],set()).add(row['split'])
    if any(len(s)!=1 for s in family_splits.values()):
        raise ValueError('ranking.family-leakage')
    train=[r for r in rows if r['split']=='train' and r['completed']]
    negatives={r['root']:[] for r in train}
    positives={}
    for r in train:
        if not r['improving']:negatives[r['root']].append(r)
    for r in train:
        if r['improving'] and negatives[r['root']]:
            positives.setdefault(r['action']['kind'],{}).setdefault(r['root'],[]).append(r)
    if not positives:raise ValueError('ranking.no-observed-pairs')
    rng=random.Random(seed);kinds=sorted(positives);rng.shuffle(kinds);pairs=[]
    for i in range(updates):
        kind=kinds[i%len(kinds)];root=rng.choice(sorted(positives[kind]))
        a,b=rng.choice(positives[kind][root]),rng.choice(negatives[root])
        pairs.append(dict(update=i,root=root,positive=a['id'],negative=b['id'],kind=kind))
    return pairs


def ranking_loss(logits, positive, negative):
    return torch.nn.functional.softplus(logits[negative]-logits[positive])


def run(args):
    began=time.perf_counter();torch.set_num_threads(1);torch.use_deterministic_algorithms(True);torch.manual_seed(17)
    cp=json.loads((args.collection/'protocol.json').read_text());data=json.loads((args.collection/'contrasts.json').read_text())
    audit=json.loads((args.collection/'audit.json').read_text());parent=Path(cp['parent'])
    p=json.loads((parent/'protocol.json').read_text());parent_report=json.loads((parent/'report.json').read_text())
    if audit['status']!='passed' or audit['contrasts_sha256']!=sha(args.collection/'contrasts.json') or audit['protocol_sha256']!=sha(args.collection/'protocol.json'):
        raise ValueError('ranking.collection-audit')
    if data['protocol_sha256']!=sha(args.collection/'protocol.json') or cp['parent_protocol_sha256']!=sha(parent/'protocol.json'):
        raise ValueError('ranking.collection-provenance')
    if parent_report['status']!='complete' or parent_report['protocol_sha256']!=sha(parent/'protocol.json'):
        raise ValueError('ranking.parent-incomplete')
    if cp['cases']!=p['cases'] or cp['catalogs']!=p['catalogs'] or cp['native_sha256']!=sha(args.native_library):
        raise ValueError('ranking.collection-inputs')
    sources=dict(p['sources'])
    for f in [args.collection/n for n in ('protocol.json','contrasts.json','audit.json')]+[parent/n for n in ('protocol.json','report.json','learned.pt')]:
        sources[str(f.resolve())]=sha(f)
    for f,h in sources.items():
        if sha(f)!=h:raise ValueError(f'ranking.input-hash: {f}')
    for manifest in (p['source_sha256'],cp['source_sha256']):
        for f,h in manifest.items():
            if sha(ROOT/f)!=h:raise ValueError(f'ranking.source-hash: {f}')
    labels=data['rows'];lookup={r['id']:r for r in labels};pairs=training_pairs(labels)
    if len(lookup)!=len(labels) or audit['replayed']!=len(labels):raise ValueError('ranking.label-membership')
    for r in audit['rows']:
        if any(r[k]!=lookup[r['id']][k] for k in ('root','split','completed','improving')):raise ValueError('ranking.audit-label')
    args.output.mkdir();protocol=dict(schema_version=1,experiment='original-root-pairwise-ranking',parent=str(parent.resolve()),
        collection=str(args.collection.resolve()),cases=p['cases'],catalogs=p['catalogs'],sources=sources,source_sha256=source_manifest(),
        schema=SCHEMA,depth=p['depth'],width=p['width'],seed=17,updates=256,learning_rate=.0003,gradient_clip=.5,
        schedule='Round-robin shuffled positive kinds; seeded uniform root/positive/negative within kind; completed training roots only',
        observation_cache_bytes=128*1024*1024,screen_seconds=900,temperatures=[1,.05],horizon=8,banks=p['plan']['banks'],
        episode_seconds=30,route_work=5000,pairs=pairs)
    write(args.output/'protocol.json',protocol)
    checkpoint=torch.load(parent/'learned.pt',map_location='cpu',weights_only=False)
    if checkpoint['schema']!=SCHEMA or checkpoint['protocol_sha256']!=sha(parent/'protocol.json'):raise ValueError('ranking.checkpoint-provenance')
    model=FullFactoryPolicy(p['depth'],p['width']);model.load_state_dict(checkpoint['policy'])
    optimizer=torch.optim.Adam(model.parameters(),lr=.0003)
    runtime=FactoryRuntime(args.native_library,p['catalogs']);cache={};cache_bytes=0;updates=[];scores=[];rows=[];stage='labels'
    def guard():
        if time.perf_counter()-began>=protocol['screen_seconds']:raise TimeoutError('ranking.screen-budget')
    def observation(case):
        nonlocal cache_bytes
        if case['id'] in cache:return cache[case['id']]
        env=FactoryEpisode(runtime,case,8,30,5000);d=env.describe();obs=observe(env.state['current'],d,env.state,1)
        value=obs,{r['id']:d['actions'].index(r['action']) for r in labels if r['root']==case['id']}
        size=sum(v.nbytes for v in obs.values() if isinstance(v,np.ndarray))
        if cache_bytes+size<=protocol['observation_cache_bytes']:cache[case['id']]=value;cache_bytes+=size
        return value
    def label_scores(phase):
        for c in p['cases']:
            guard();obs,indices=observation(c)
            with torch.no_grad():logits=model(obs)
            if not torch.isfinite(logits).all():raise ValueError('ranking.nonfinite-logits')
            for r in labels:
                if r['root']==c['id']:scores.append(dict(phase=phase,id=r['id'],logit=float(logits[indices[r['id']]]),
                                                        top1=indices[r['id']]==int(logits.argmax())))
    def save(step,name):
        torch.save(dict(schema=SCHEMA,depth=p['depth'],width=p['width'],policy=model.state_dict(),optimizer=optimizer.state_dict(),
            rng=torch.get_rng_state(),step=step,protocol_sha256=sha(args.output/'protocol.json')),args.output/(name+'.pt'))
    try:
        label_scores('parent');stage='training'
        cases={c['id']:c for c in p['cases']}
        for pair in pairs:
            guard();obs,indices=observation(cases[pair['root']]);logits=model(obs)
            loss=ranking_loss(logits,indices[pair['positive']],indices[pair['negative']]);optimizer.zero_grad();loss.backward()
            grad=float(torch.nn.utils.clip_grad_norm_(model.parameters(),.5))
            if not torch.isfinite(loss) or not np.isfinite(grad):raise ValueError('ranking.nonfinite-training')
            optimizer.step();updates.append(dict(pair,loss=float(loss.detach()),gradient_norm=grad))
            if len(updates)%64==0:
                save(len(updates),'progress');write(args.output/'updates.json',updates)
                print(json.dumps(dict(stage=stage,updates=len(updates),seconds=time.perf_counter()-began)),flush=True)
        save(len(updates),'learned');model.eval();label_scores('ranked');write(args.output/'label-scores.json',scores)
        frozen={k:v.clone() for k,v in model.state_dict().items()};stage='evaluation'
        config=dict(horizon=8,balanced=False,memory=True,geometry=False);bounded=argparse.Namespace(episode_seconds=30,route_work=5000)
        for t in protocol['temperatures']:
            actor=TemperaturePolicy(model,t)
            for c in p['cases']:
                for bank in protocol['banks']:
                    guard();summary,best,_,_=episode(runtime,c,actor,config,bank,bounded)
                    directory=args.output/f't{t}--{c["id"]}--{bank}';directory.mkdir()
                    save_gzip(directory/'episode.json.gz',summary);save_gzip(directory/'best.json.gz',best)
                    rows.append(dict(temperature=t,case=c['id'],family=c['family'],split=c['split'],bank=bank,directory=str(directory.resolve()),
                        initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],selected=summary['selected'],
                        seconds=summary['total_seconds'],statuses=summary['statuses'],kinds=summary['kinds'],
                        budget_complete=summary['selected']==8 and not any(a['status']=='late' for a in summary['trace'])))
            print(json.dumps(dict(stage=stage,temperature=t,episodes=len(rows),seconds=time.perf_counter()-began)),flush=True)
        if any(not torch.equal(v,frozen[k]) for k,v in model.state_dict().items()):raise ValueError('ranking.policy-mutated')
        for f,h in sources.items():
            if sha(f)!=h:raise ValueError(f'ranking.input-changed: {f}')
        for f,h in protocol['source_sha256'].items():
            if sha(ROOT/f)!=h:raise ValueError(f'ranking.source-changed: {f}')
        write(args.output/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
            seconds=time.perf_counter()-began,cache_array_bytes=cache_bytes,updates=len(updates),
            positive_kinds=dict(Counter(pair['kind'] for pair in pairs)),policy_immutable=True,rows=rows))
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',code='ranking.failed',severity='error',stage=stage,message=str(error),updates=updates,rows=rows));raise
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('collection','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
