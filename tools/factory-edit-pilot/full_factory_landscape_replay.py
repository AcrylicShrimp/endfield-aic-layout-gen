"""Replay one evaluation bank, boundary training groups and best heavy outcomes."""
import argparse
import gzip
import json
from pathlib import Path
import time

import torch

from full_factory_environment import FactoryRuntime,FactoryEpisode
from full_factory_policy import SCHEMA,FullFactoryPolicy,observe
from full_factory_selection import physical_key,action_key,SelectionMemory,distribution,geometry_choice
from full_factory_screen import sha


def unpack(path):
    with gzip.open(path,'rt') as stream:return json.load(stream)


def run(args):
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    began=time.perf_counter()
    p=json.loads((args.landscape/'protocol.json').read_text());report=json.loads((args.landscape/'landscape.json').read_text())
    inputs=json.loads(args.inputs.read_text());cases={c['id']:c for c in p['cases']}
    heavy={c['case'] for c in report['cells'] if c['instances']==59}
    heavy_bests={min((c for c in report['cells'] if c['case']==case),key=lambda c:c['best_metrics']['score'])['best'] for case in heavy}
    last={a['id']:p['training_actions']//(p['group_episodes']*a['horizon'])-1 for a in p['arms']}
    selected=[r for r in report['rows'] if r['directory'] in heavy_bests or
              (r['phase']=='evaluation' and r['bank']==p['banks'][0]) or
              (r['phase']=='training' and r['group'] in (0,last[r['arm']]))]
    models={}
    for a in p['arms']:
        for phase in ('untrained','final'):
            label=a['id']+'-'+phase;checkpoint=torch.load(args.landscape/f'{label}.pt',map_location='cpu',weights_only=False)
            if checkpoint.get('schema') != SCHEMA:raise ValueError(f'factory.checkpoint-schema: {label} requires {SCHEMA}')
            model=FullFactoryPolicy(0);model.load_state_dict(checkpoint['policy']);models[label]=model
    originals={label:{k:v.clone() for k,v in model.state_dict().items()} for label,model in models.items()}
    runtime=FactoryRuntime(args.native_library,inputs['catalogs']);count=sampling=0
    try:
        for ordinal,row in enumerate(selected):
            recorded=unpack(args.landscape/row['directory']/'episode.json.gz')
            config=row['config'];env=FactoryEpisode(runtime,cases[row['case']],config['horizon'],p['episode_seconds'],p['route_work'])
            memory=SelectionMemory(env.state['current']);current=best=physical_key(env.state['current'])
            rng=torch.Generator().manual_seed(row['sample_seed'])
            model=models.get(row['arm']) if row['phase']=='evaluation' else models[row['arm']+'-untrained'] if row['group']==0 else None
            verify_sampling=row['phase']=='evaluation' or row['group']==0
            for t in recorded['trace']:
                desc=env.describe(config.get('vocabulary'));keys=[action_key(a,desc,env.state['current'],best) for a in desc['actions']]
                allowed,exhausted=memory.mask(current,keys,desc['actions'],config['memory'])
                index=desc['actions'].index(t['action'])
                assert allowed[index],(row['directory'],'selected masked action')
                assert keys[index]==t['action_key']
                assert sum(not x for x in allowed)==t['masked'] and exhausted==t['memory_exhausted']
                if verify_sampling:
                    with torch.no_grad():
                        logits=model(observe(env.state['current'],desc,env.state,1-env.selected/config['horizon'])) if model is not None else torch.zeros(len(allowed))
                        dist=distribution(logits,desc['actions'],allowed,config['balanced'])
                        choice=geometry_choice(env.state['current'],desc['actions'],allowed) if config.get('geometry') else int(torch.multinomial(dist.probs,1,generator=rng))
                    assert choice==index,(row['directory'],'sampling');sampling+=1
                before=current;observed=env.step(t['action'])
                current=before if observed['unchanged'] else physical_key(env.state['current'])
                unchanged,revisited=memory.record(before,current,keys[index])
                if observed['status']=='improved':best=current
                assert (before,current,unchanged,revisited)==(t['physical_before'],t['physical_after'],t['physical_unchanged'],t['physical_revisited'])
                for key in ('selected','status','executor_status','route_expansions','current_score','best_score','unchanged','revisited','diagnostics'):
                    assert observed[key]==t[key],(row['directory'],key)
                count+=1
            assert env.state['best']==unpack(args.landscape/row['directory']/'best.json.gz')
            assert physical_key(env.state['best'])==row['physical_best_sha256']
            if (ordinal+1)%200==0:print(json.dumps(dict(episodes=ordinal+1,actions=count)),flush=True)
        assert all(torch.equal(v,originals[label][k]) for label,model in models.items() for k,v in model.state_dict().items())
        result=dict(schema_version=1,status='passed',episodes=len(selected),selected_actions=count,sampling_actions=sampling,
                    seconds=time.perf_counter()-began,selected_directories=[r['directory'] for r in selected],
                    scope='All methods/cases in first bank, first/last training groups, and best heavy outputs; no reconstruction of optimizer updates',
                    source_sha256=sha(Path(__file__)),report_sha256=sha(args.landscape/'landscape.json'))
        args.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='selected_directories'}))
    finally:runtime.close()


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('inputs','landscape','native-library','output'):parser.add_argument('--'+name,type=Path,required=True)
    run(parser.parse_args())
