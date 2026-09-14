"""Verify native collection choices, true-terminal credit and actual PPO/SIL update.

Use recorded native observations, including their real elapsed-time feature. Do
not compare separately timed environments as if wall-clock state were identical.
"""
import argparse
import json
import os
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import torch
from safetensors.torch import load_file
import declarative_learning_policy as policy
from declarative_native_input import RootChoices,PortGroups
from blueprint_ppo_update import credit
from rust_learner_compare import compare_tensors,oracle,write


def prepared(row):
    source=row['input']
    result=dict(schema='declarative-learning-graph-v1',native_domain='declarative-native-domain-v1',root_mask_schema='declarative-root-mask-v1')
    for name,width in [('node_static',14),('node_state',10),('edge_static',10),('edge_state',4),
                       ('path_cells',2),('action_features',12),('port_features',12)]:
        result[name]=torch.tensor(source[name],dtype=torch.float32).reshape(-1,width)
    for name in ('global_features','kind_log_counts'):
        result[name]=torch.tensor(source[name],dtype=torch.float32)
    result['edge_index']=torch.tensor(source['edge_index'],dtype=torch.long).reshape(-1,2).T.contiguous()
    result['path_edges']=torch.tensor(source['path_edges'],dtype=torch.long)
    result['action_references']=torch.tensor(source['action_references'],dtype=torch.long).reshape(-1,2)
    result['root_invalid']=torch.tensor(source['root_invalid'],dtype=torch.bool)
    roots=torch.tensor(source['root_meta'],dtype=torch.long)
    result.update(actions=RootChoices(roots),root_owners=roots[:,1],port_groups=PortGroups(row['domain']['groups'],result.pop('port_features')))
    return result


def verify(run,output):
    assert not output.exists()
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    output.mkdir(parents=True)
    report=json.loads((run/'report.json').read_text())
    device=json.loads((run/'checkpoint/manifest.json').read_text())['device']
    results=[]
    for record in report['records']:
        folder=run/f'chunk-{record["chunk"]:04d}'
        manifest=json.loads((folder/'manifest.json').read_text())
        before=load_file(folder/'before.safetensors')
        model=policy.ActorCritic(manifest['model']['width'],manifest['model']['message_rounds']).to(device)
        model.load_state_dict({n.removeprefix('model.'):t for n,t in before.items() if n.startswith('model.')},strict=True)
        rows=json.loads((folder/'first-observations.json').read_text())
        sampler=torch.Generator().set_state(before['rng.sampler'])
        with torch.no_grad(): choices=policy.sample(model,[prepared(r) for r in rows],sampler)
        for expected,row in zip(choices,rows,strict=True):
            actual=row['choice']
            assert expected['decision']==dict(root=actual['root'],ports=actual['ports'])
            for key in ('old_logp','old_value','entropy'):
                torch.testing.assert_close(expected[key],actual[key],atol=2e-6,rtol=2e-5,msg=key)
        state=folder/'first-sampler.safetensors'
        assert state.exists(), 'This gate requires first-wave RNG capture'
        assert torch.equal(sampler.get_state(),load_file(state)['sampler'])
        transitions=json.loads((folder/'transitions.json').read_text())
        saved_credit=json.loads((folder/'credit.json').read_text())
        samples=[dict(stream=r['stream'],reward=r['reward'],terminal=r['terminal'],old_value=r['choice']['old_value']) for r in transitions]
        frontiers=[dict(stream=i,bootstrap=b,terminal=[r for r in samples if r['stream']==i][-1]['terminal']) for i,b in enumerate(saved_credit['bootstrap'])]
        raw,advantages,targets=credit(samples,frontiers,saved_credit)
        torch.testing.assert_close(raw,torch.tensor(saved_credit['raw']),atol=0,rtol=0)
        torch.testing.assert_close(targets,torch.tensor(saved_credit['targets']),atol=0,rtol=0)
        tensors=load_file(folder/'input.safetensors')
        torch.testing.assert_close(advantages[:manifest['ppo_graphs']],tensors['ppo.advantages'],atol=0,rtol=0)
        target=output/f'chunk-{record["chunk"]:04d}'
        expected=oracle(folder/'manifest.json',target,device,manifest['update'].get('adam_backend','foreach'),1)
        actual=load_file(folder/'after.safetensors');wanted=load_file(target/'state.safetensors')
        # Native apply() performs only shuffle + numerical update. The standalone
        # kernel oracle additionally exercises three diagnostic RNG streams.
        keep=lambda values:{n:t for n,t in values.items() if not n.startswith('rng.') or n=='rng.shuffle'}
        comparison=compare_tensors(keep(actual),keep(wanted))
        gradients=compare_tensors(load_file(folder/'gradients.safetensors'),load_file(target/'first-gradients.safetensors'))
        for key in ('loss','value_loss','imitation_loss','actor_gradient_norm','critic_gradient_norm'):
            torch.testing.assert_close(record['update'][key],expected['records'][0][key],atol=2e-6,rtol=2e-5,msg=key)
        results.append(dict(chunk=record['chunk'],samples=len(samples),first_wave_choices=len(rows),first_wave_rng_exact=True,
            credit_exact=True,model_adam_shuffle=comparison,gradients=gradients))
    result=dict(schema='aic-native-collection-oracle-v1',status='passed',results=results,torch_version=torch.__version__,device=device)
    write(output/'summary.json',result)
    print(json.dumps(result,indent=2))


if __name__=='__main__':
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();verify(args.run,args.output)
