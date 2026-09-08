"""Recompute first-bank and inverse validation choices; replay PG boundary groups."""
import argparse
import gzip
import json
from pathlib import Path
import time

import torch

from full_factory_environment import FactoryRuntime,FactoryEpisode
from full_factory_policy import SCHEMA,FullFactoryPolicy,observe
from full_factory_screen import sha
from full_factory_selection import physical_key,action_key,SelectionMemory,distribution


def unpack(path):
    with gzip.open(path,'rt') as stream:return json.load(stream)


def run(args):
    began=time.perf_counter();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    p=json.loads((args.screen/'protocol.json').read_text());r=json.loads((args.screen/'screen.json').read_text());data=json.loads((Path(p['curriculum_path'])/'curriculum.json').read_text())
    pairs={pair['id']:pair for pair in data['pairs']};cases={c['id']:c for c in p['cases']};cases.update({id:pair['case']for id,pair in pairs.items()})
    labels=['untrained-d0','untrained-d2']+[a['id']+'-final'for a in p['arms']];models={}
    for label in labels:
        checkpoint=torch.load(args.screen/f'{label}.pt',weights_only=False)
        if checkpoint.get('schema')!=SCHEMA:raise ValueError(f'factory.checkpoint-schema: {label}')
        assert checkpoint['protocol_sha256']==sha(args.screen/'protocol.json')
        model=FullFactoryPolicy(checkpoint['depth']);model.load_state_dict(checkpoint['policy']);models[label]=model
    heavy_bests={min((c for c in r['cells'] if c['case']==case),key=lambda c:c['best_metrics']['score'])['best']for case in {c['case']for c in r['cells']if c['instances']==59}}
    chosen=[row for row in r['rows']if row['directory']in heavy_bests or (row['phase']=='evaluation' and row['bank']==p['banks'][0]) or
            (row['phase']=='training' and row['group']in (0,p['training_actions']//32-1))]
    original={label:{k:v.clone() for k,v in model.state_dict().items()}for label,model in models.items()};runtime=FactoryRuntime(args.native_library,p['catalogs']);count=sampling=0
    try:
        for ordinal,row in enumerate(chosen):
            record=unpack(args.screen/row['directory']/'episode.json.gz');env=FactoryEpisode(runtime,cases[row['case']],8,p['cell_seconds'],p['route_work'])
            current=best=physical_key(env.state['current']);memory=SelectionMemory(env.state['current']);rng=torch.Generator().manual_seed(row['sample_seed'])
            model=models.get(row['arm']) if row['phase']=='evaluation' else models['untrained-d0'] if row['group']==0 else None
            verify=row['phase']=='evaluation' or row['group']==0
            for t in record['trace']:
                desc=env.describe();keys=[action_key(a,desc,env.state['current'],best)for a in desc['actions']];index=desc['actions'].index(t['action']);assert keys[index]==t['action_key']
                if verify:
                    with torch.no_grad():
                        logits=model(observe(env.state['current'],desc,env.state,1-env.selected/8)) if model else torch.zeros(len(keys))
                        dist=distribution(logits,desc['actions'],[True]*len(keys),False);selected=int(torch.multinomial(dist.probs,1,generator=rng))
                    assert selected==index,(row['directory'],'sampling');sampling+=1
                before=current;observed=env.step(t['action']);current=before if observed['unchanged']else physical_key(env.state['current']);unchanged,revisited=memory.record(before,current,keys[index])
                if observed['status']=='improved':best=current
                assert (before,current,unchanged,revisited)==(t['physical_before'],t['physical_after'],t['physical_unchanged'],t['physical_revisited'])
                for key in ('status','executor_status','route_expansions','current_score','best_score','diagnostics'):assert observed[key]==t[key],(row['directory'],key)
                count+=1
            assert env.state['best']==unpack(args.screen/row['directory']/'best.json.gz')
            if (ordinal+1)%200==0:print(json.dumps(dict(root_episodes=ordinal+1,selected=count)),flush=True)
        for row in r['inverse']:
            record=unpack(args.screen/row['directory']/'episode.json.gz');env=FactoryEpisode(runtime,pairs[row['pair']]['case'],1,p['cell_seconds'],p['route_work']);desc=env.describe();model=models.get(row['arm'])
            with torch.no_grad():
                logits=model(observe(env.state['current'],desc,env.state,1))if model else torch.zeros(len(desc['actions']))
                dist=torch.distributions.Categorical(logits=logits)
                index=int(logits.argmax())if row['selection']=='greedy'else int(torch.multinomial(dist.probs,1,generator=torch.Generator().manual_seed(1270000+p['validation_pairs'].index(row['pair']))))
            t=record['trace'][0];assert desc['actions'][index]==t['action'];result=env.step(desc['actions'][index])
            for key in ('status','executor_status','route_expansions','current_score','best_score','diagnostics'):assert result[key]==t[key],(row['directory'],key)
            assert env.state['best']==unpack(args.screen/row['directory']/'best.json.gz');count+=1;sampling+=1
        assert all(torch.equal(v,original[label][k])for label,model in models.items()for k,v in model.state_dict().items())
        result=dict(status='passed',root_episodes=len(chosen),inverse_episodes=len(r['inverse']),selected=count,sampling=sampling,seconds=time.perf_counter()-began,
                    scope='First root bank, PG first/last groups, heavy bests, all inverse validations; optimizer streams not reconstructed',source_sha256=sha(Path(__file__)),screen_sha256=sha(args.screen/'screen.json'))
        args.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('screen','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
