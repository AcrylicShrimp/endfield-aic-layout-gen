"""Validated compact chronology and one-pass frozen sequence contributions."""
from collections import Counter, defaultdict
from pathlib import Path
import time
from types import SimpleNamespace

import torch

from blueprint_ppo_store import checked, load, read, sha
from blueprint_ppo_update import digest, require
from blueprint_execution_update import imitation, prepare_minibatch, summarize_imitation
import declarative_learning_policy as policy
from declarative_replay_retention import stratum
from declarative_success_features import validate_metadata


def need(ok, code, message):
    require(ok, f'retention-screen.{code}: {message}')


def roots(value):
    return {int(n):set(indices) for n,indices in value.items() if indices}


class Corpus:
    def __init__(self, path, guard):
        self.root=Path(path).resolve().parent; self.manifest=read(path); m=self.manifest
        need(m.get('schema')=='declarative-success-arrivals-v1' and m.get('complete'), 'corpus', 'complete arrival corpus required')
        launch=Path(m['child']['directory'])/'launch.json'
        need(sha(launch)==m['child']['launch_sha256'],'binding','child launch changed')
        self.config=read(launch)['config']; self.strata=self.config['strata']
        need(sha(Path(m['parent']['directory'])/'launch.json')==m['parent']['launch_sha256'],'binding','parent launch changed')
        self.initial=load(checked(m['initial_pool'],self.root)); self.updates=[]
        self.entries={e['id']:e for e in self.initial['entries']}; self.first={}
        need(len(self.entries)==4096,'capacity','initial pool must have actual capacity 4096')
        self.sources={int(n):dict(row,coverage=set(row['coverage'])) for n,row in m['union_sources'].items()}
        self.checkpoints={int(n):roots(row['rows']) for n,row in self.initial['checkpoint_roots'].items()}
        for ordinal,boundary in enumerate(m['boundaries'],m['initial_update']+1):
            guard(); row=load(checked(boundary['reference'],self.root))
            need(boundary['update']==row['update']==ordinal,'order','gap in complete updates')
            need(row['sil_calls']==len(row['selected_ids']),'schedule','incomplete minibatch schedule')
            keys=[tuple(a['key']) for a in row['attempts']]
            need(keys==sorted(set(keys)) and all(k[0]==ordinal for k in keys),'order','terminal attempt order')
            events=[e['id'] for e in row['events'] if e['kind'] in ('admitted','duplicate')]
            need(events==[a['entry']['id'] for a in row['attempts']],'coverage','attempt/event mismatch')
            for a in row['attempts']:
                e=a['entry'];need(e['admitted_update']==ordinal,'future-observation','incorrect admission ordinal')
                need(all(l['reference'][0]<=ordinal for l in e['labels']),'future-observation','future label')
                for l in e['labels']:self.verify_reference(l['reference'])
                if e['id'] not in self.entries:self.first[e['id']]=ordinal;self.entries[e['id']]=e
                else:need(e['retention_features_v1']==self.entries[e['id']]['retention_features_v1'],'identity','conflicting features')
            self.checkpoints[ordinal]=roots(row['checkpoint_rows']);self.updates.append(row)
        need(bool(self.updates) and self.updates[-1]['update']==m['final_update'],'order','incomplete final boundary')
        for e in self.entries.values():
            validate_metadata(e['retention_features_v1'],e['id'])
            e['stratum']=stratum(e,self.strata)
            need(bool(e['labels']) and len(e['labels'])<=262144,'capacity','invalid label count')
            for l in e['labels']:self.verify_reference(l['reference'])
            need(e.get('utility') is None or e['utility']['last_update']<=m['initial_update'],'future-observation','future initial utility')
        required_checkpoints={m['initial_update']}|{n for row in self.updates for n in row['mandatory_checkpoints']}
        self.verify_checkpoints(required_checkpoints)
        for row in self.updates:self.verify_roots(roots(row['live_roots']))
        end=m['final_update']; half=m['initial_update']+len(self.updates)//2
        self.cohort={i:dict(anchor=n,stratum=self.entries[i]['stratum'],
            place=self.entries[i]['retention_features_v1']['has_place'],
            deletion=all(l['stage']=='delete_band' for l in self.entries[i]['labels']),
            episode=(self.entries[i]['stream'],self.entries[i]['episode']))
            for i,n in self.first.items() if n<=half and n+32<=end}
        self.coverage=dict(new_ids=len(self.first),updates=len(self.updates),cohort_ids=len(self.cohort),
            facility_episodes={s:len({tuple(c['episode']) for c in self.cohort.values() if c['place'] and c['stratum']==s}) for s in self.strata})
        self.coverage['sufficient']=len(self.first)>=4096 and len(self.updates)>=33 and all(n>=10 for n in self.coverage['facility_episodes'].values())
        self.storage=[]
        for row in self.updates:
            mandatory=set(roots(row['live_roots']))
            for n in row['mandatory_checkpoints']:
                need(n in self.checkpoints,'source','missing retained checkpoint root')
                mandatory.update(self.checkpoints[n])
            self.storage.append(dict(mandatory=mandatory,mandatory_bytes=self.price(mandatory),
                original_baseline_published_bytes=sum(m['baseline_sources'][str(n)]['bytes'] for n in row['baseline_retained_sources'])))

    def verify_reference(self, ref):
        n,i=ref;need(n in self.sources and i in self.sources[n]['coverage'],'source',f'row outside union coverage {n}:{i}')

    def verify_roots(self, refs):
        for n,indices in refs.items():
            for i in indices:self.verify_reference((n,i))

    def verify_checkpoints(self, required):
        # The capture inventories old migration snapshots as well. Only normal
        # retained checkpoint roots at the priced boundaries are mandatory.
        for n in required:
            need(n in self.checkpoints,'source',f'missing mandatory checkpoint {n}')
            self.verify_roots(self.checkpoints[n])

    def price(self, chunks):
        return sum(self.sources[n]['bytes'] for n in chunks)


def frozen_batch(model, sequences, device):
    # No PPO rows exist in this diagnostic. Reuse production SIL preparation
    # while bypassing only its unrelated empty PPO graph packing.
    likelihood=SimpleNamespace(prepare_rows=lambda rows:policy.prepare_rows(rows) if rows else None)
    prepared=prepare_minibatch([],[],torch.empty(0),torch.empty(0),sequences,likelihood,True)['sil']
    with torch.no_grad():
        loss,measurements=imitation(model,prepared,device,policy)
    result=summarize_imitation([measurements],prepared['groups'],prepared['utility_groups'],True)['sequence_utility']
    return {r['id']:dict(raw=r['mean_weighted_nll'],positive=r['mean_positive_advantage']) for r in result}


def frozen_cache(corpus, guard, progress, device='cpu'):
    began=time.monotonic();m=corpus.manifest
    saved=load(checked(m['parent_checkpoint'],corpus.root));guard()
    need(saved['update']==m['initial_update'] and digest(saved['model'])==saved['model_sha256'],'model','frozen parent model mismatch')
    need(m['parent_checkpoint']['sha256']==m['parent']['checkpoint']['sha256'],'model','checkpoint not bound to parent')
    model=policy.ActorCritic(corpus.config['ppo']['width'],corpus.config['ppo']['message_rounds']).to(device)
    model.load_state_dict(saved['model'],strict=True);model.eval();del saved
    # Decode each source once; discard physical rollouts immediately after retaining
    # the referenced packed inputs. Inference batches never cross the row/node cap.
    needed=defaultdict(list);rows={i:[None]*len(e['labels']) for i,e in corpus.entries.items()}
    for identity,e in corpus.entries.items():
        for index,l in enumerate(e['labels']):needed[l['reference'][0]].append((identity,index,l))
    left={i:len(v) for i,v in rows.items()};cache={};batch=[];nodes=labels=0;decoded=0
    def flush():
        nonlocal batch,nodes,labels
        if batch:
            guard();cache.update(frozen_batch(model,batch,device));batch=[];nodes=labels=0
            progress(dict(stage='frozen-inference',completed=len(cache),total=len(rows),decoded_sources=decoded,seconds=time.monotonic()-began))
    for n,source in sorted(corpus.sources.items()):
        guard();path=checked(source['reference'],corpus.root)
        need(path.stat().st_size==source['bytes'],'source','union byte size mismatch')
        payload=load(path);decoded+=1
        need(set(payload['samples'])==source['coverage'],'source','union source coverage mismatch')
        for identity,index,l in needed[n]:
            sample=payload['samples'][l['reference'][1]]
            row=dict(input=sample['input'],decision=sample['decision'],stage=l['stage'],reference=l['reference'],return_to_terminal=l['return_to_terminal'])
            rows[identity][index]=row;left[identity]-=1
            if left[identity]:continue
            sequence=rows[identity];size=sum(len(r['input']['node_static']) for r in sequence)
            if batch and (labels+len(sequence)>256 or nodes+size>32768):flush()
            batch.append(dict(id=identity,origin='own',stratum=corpus.entries[identity]['stratum'],rows=sequence));labels+=len(sequence);nodes+=size
            # Canonical geometry identity is source-generated, not a facility type
            # count. Keep the bound generator geometry when available.
            metadata=corpus.entries[identity]['input']
            corpus.entries[identity]['screen_geometry']=metadata.get('geometry_sha256',metadata.get('input_sha256'))
        del payload
        # Rows already inferred are no longer needed; pending sequences retain
        # only policy inputs, not native observations or checkpoints.
        if batch:flush()
        for identity,_,_ in needed[n]:
            if identity in cache:rows[identity]=None
    flush();need(len(cache)==len(rows),'source','incomplete frozen cache')
    return cache,dict(seconds=time.monotonic()-began,model_sha256=digest(model.state_dict()),
        sequences=len(cache),decoded_sources=decoded,device=device,identity=digest(cache))
