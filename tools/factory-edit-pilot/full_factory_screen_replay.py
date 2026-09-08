"""Replay every selected full-factory action and verify frozen policy sampling."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import time

import torch

from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_policy import FullFactoryPolicy, SCHEMA, observe
from full_factory_screen import geometry_action


def unpack(path):
    with gzip.open(path,'rt') as stream:
        return json.load(stream)


def run(args):
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    started = time.perf_counter()
    protocol = json.loads((args.screen/'protocol.json').read_text())
    report = json.loads((args.screen/'screen.json').read_text())
    inputs = json.loads(args.inputs.read_text())
    cases = {r['id']:r for r in protocol['cases']}
    runtime = FactoryRuntime(args.native_library, inputs['catalogs'])
    policies, policies_before = {}, {}
    for depth in protocol['depths']:
        for seed in protocol['seeds']:
            for phase in ('untrained','final'):
                label = f'd{depth}-s{seed}-{phase}'
                saved = torch.load(args.screen/f'{label}.pt', map_location='cpu', weights_only=False)
                if saved.get('schema') != SCHEMA:raise ValueError(f'factory.checkpoint-schema: {label} requires {SCHEMA}')
                assert saved['schema'] == SCHEMA
                model = FullFactoryPolicy(depth)
                model.load_state_dict(saved['policy'])
                policies[label] = model
                policies_before[label] = {k:v.clone() for k,v in model.state_dict().items()}
    actions = frozen_actions = 0
    try:
        for index, row in enumerate(report['rows']):
            recorded = unpack(args.screen/row['directory']/'episode.json.gz')
            episode = FactoryEpisode(runtime,cases[row['case']],protocol['horizon'],protocol['episode_seconds'],protocol['route_work'])
            rng = torch.Generator(device='cpu').manual_seed(row['sample_seed'])
            policy = policies.get(row['arm']) if row['phase']=='evaluation' else None
            for trace in recorded['trace']:
                description = episode.describe()
                assert trace['action'] in description['actions'], (row['directory'],'undeclared action')
                if row['phase'] == 'evaluation':
                    if policy is not None:
                        obs = observe(episode.state['current'],description,episode.state,1-episode.selected/protocol['horizon'])
                        with torch.no_grad():
                            probabilities = torch.distributions.Categorical(logits=policy(obs)).probs
                        selected = int(torch.multinomial(probabilities,1,generator=rng))
                    elif row['arm'] == 'uniform':
                        selected = int(torch.randint(len(description['actions']),(1,),generator=rng))
                    else:
                        selected = geometry_action(episode.state['current'],description['actions'])
                    assert description['actions'][selected] == trace['action'], (row['directory'],'sampling')
                    frozen_actions += 1
                replayed = episode.step(trace['action'])
                for key in ('selected','action','executor_status','route_expansions','diagnostics','status','unchanged','revisited','current_score','best_score'):
                    assert replayed[key] == trace[key], (row['directory'],key,replayed[key],trace[key])
                actions += 1
            assert episode.selected == row['selected']
            assert episode.submitted == row['native_proposals']
            assert digest(episode.state['best']) == row['best_sha256']
            assert episode.state['best'] == unpack(args.screen/row['directory']/'best.json.gz')
            if (index+1)%240 == 0:
                print(json.dumps(dict(episodes=index+1,actions=actions)),flush=True)
        for label, model in policies.items():
            assert all(torch.equal(v,policies_before[label][k]) for k,v in model.state_dict().items())
        result = dict(schema_version=1,status='passed',episodes=len(report['rows']),selected_actions=actions,
                      frozen_sampling_actions=frozen_actions,seconds=time.perf_counter()-started,
                      policy_immutable=True,scope='All actions replayed; frozen evaluation sampling reproduced; training updates not reconstructed',
                      source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      report_sha256=hashlib.sha256((args.screen/'screen.json').read_bytes()).hexdigest())
        args.output.write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result))
    finally:
        runtime.close()


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs',type=Path,required=True)
    parser.add_argument('--screen',type=Path,required=True)
    parser.add_argument('--native-library',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    run(parser.parse_args())
