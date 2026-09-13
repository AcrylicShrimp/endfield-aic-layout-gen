"""Contract-specific failures: joint likelihood, preparation credit and restoration."""
from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import unittest

import torch

from blueprint_execution_update import draw_sequences, update_packed
from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest
from declarative_learning_buffer import Buffer
from declarative_learning_environment import Runtime, Stream, finish_stream, semantic, step_stream
import declarative_learning_policy as policy
from declarative_learning_runtime import Learner


class MemoryEvidence:
    def __init__(self):
        self.chunks = {}
    def begin(self, ordinal, chunk):
        self.chunks[ordinal] = chunk
    def sample(self, reference):
        return self.chunks[reference[0]]['samples'][reference[1]]


def diagnostic_config():
    config = read(ROOT/'tools/factory-edit-pilot/declarative-learning-short.json')
    config.update(purpose='fixed-fixture-diagnostic',device='cpu')
    config['training_input'] = dict(schema='blueprint-training-input-v1',kind='fixed-fixture-diagnostic')
    config['training'].update(streams=1,chunk_actions=2,episode_actions=4,elapsed_ms=60000)
    config['success']['fixture_diagnostic'] = True
    config['strata'] = {'small':['fixture-diagnostic']}
    return config


def decision_for(prepared, action, observation):
    base = {k:v for k,v in action.items() if k!='ports'}
    root = next(i for i,value in enumerate(prepared['actions']) if value==base)
    indices = []
    if action['kind']=='place':
        mapping = {p['node']:p['port'] for p in action.get('ports',[])}
        for group in prepared['port_groups'][prepared['root_owners'][root]]:
            name = mapping.get(group['node'],observation['port_assignments'][group['node']])
            indices.append(group['ports'].index(name))
    return dict(root=root,ports=indices)


def scripted_chunk(model, stream, actions):
    observation = stream.observe()
    before = stream.snapshot()
    samples = []
    for action in actions:
        prepared = stream.runtime.prepare(observation)
        decision = decision_for(prepared,action,observation)
        selected = dict(input=prepared,decision=decision)
        with torch.no_grad():
            lp,ent,value = policy.predict(model,policy.prepare_rows([selected]))
        command = dict(decision=decision,action=policy.action(prepared,decision),old_logp=float(lp[0]),old_value=float(value[0]),entropy=float(ent[0]))
        sample,observation = step_stream(stream,observation,prepared,command)
        samples.append(sample)
    prepared = stream.runtime.prepare(observation)
    with torch.no_grad():
        value = policy.values(model,[prepared])[0]
    frontier,complete = finish_stream(stream,observation,prepared,value)
    return dict(schema='declarative-learning-chunk-v1',model_before=digest(model.state_dict()),native_before=[before],
        samples=samples,frontiers=[frontier],completed=[] if complete is None else [complete],collection_seconds=0.)


class ReplaySamplingTests(unittest.TestCase):
    def test_available_balance_preserves_absence_and_strict_mode(self):
        draw,order = torch.Generator().manual_seed(1),torch.Generator().manual_seed(2)
        self.assertEqual(draw_sequences([], 'balanced_available',draw,order),[])
        small=[dict(id='a',stratum='small')]
        self.assertEqual(len(draw_sequences(small,'balanced_available',draw,order)),8)
        both=small+[dict(id='b',stratum='large')]
        result=draw_sequences(both,'balanced_available',draw,order)
        self.assertEqual(sum(r['stratum']=='large' for r in result),4)
        with self.assertRaisesRegex(ValueError,'stratum-empty'):
            draw_sequences(small,'balanced',draw,order)


    def test_retention_keeps_sources_of_retained_interrupted_checkpoints(self):
        from types import SimpleNamespace
        from declarative_learning_runtime import account_write, retain
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            job={'checkpoint_chunks':{'3':[1,2,3]}}
            for folder in ('chunks','checkpoints'):
                (root/folder).mkdir()
                for ordinal in range(1,5):
                    path=root/folder/f'{ordinal:04d}.pt.gz';path.write_bytes(b'fixture')
                    account_write(job,path)
            learner=SimpleNamespace(ordinal=4,buffer=Buffer(),config={'retention':{'checkpoints':[0,4],'recovery_updates':2}})
            retain(learner,root,job)
            self.assertTrue((root/'chunks/0001.pt.gz').exists())
            self.assertTrue((root/'checkpoints/0003.pt.gz').exists())
            self.assertFalse((root/'checkpoints/0001.pt.gz').exists())


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class NativeLearningTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        self.config=diagnostic_config()
        self.runtime=Runtime(self.config,os.environ['AIC_NATIVE_LIBRARY'])
        self.model,_,_=policy.learners(17,self.config['ppo'])

    def observation(self):
        session,_=self.runtime.open(64,60000)
        return session,json.loads(session.observation_json())

    def test_catalog_connection_coordinates_match_native_current_ports(self):
        _,observation=self.observation()
        facilities={f['instance']:f for f in observation['facilities']}
        for option in observation['port_options']:
            f=facilities[option['instance']]
            port=self.runtime.catalog.rotated[(f['facility'],f['rotation'])][observation['port_assignments'][option['node']]]
            self.assertEqual(observation['nodes'][option['node']]['position'],dict(x=f['x']+port['x'],y=f['y']+port['y']))

    def test_joint_sampling_and_teacher_likelihood_agree_with_packed_graphs(self):
        _,observation=self.observation(); prepared=self.runtime.prepare(observation)
        rng=torch.Generator().manual_seed(15)
        with torch.no_grad():
            selected=policy.sample(self.model,[prepared]*8,rng)
            rows=[dict(input=prepared,**row) for row in selected]
            lp,ent,values=policy.predict(self.model,policy.prepare_rows(rows))
            torch.testing.assert_close(lp,torch.tensor([r['old_logp'] for r in rows]),rtol=1e-5,atol=2e-6)
            torch.testing.assert_close(ent,torch.tensor([r['entropy'] for r in rows]),rtol=1e-5,atol=2e-6)
            for i,row in enumerate(rows):
                single=policy.predict(self.model,policy.prepare_rows([row]))
                torch.testing.assert_close(single[0][0],lp[i],rtol=1e-5,atol=2e-6)
                torch.testing.assert_close(single[2][0],values[i],rtol=1e-5,atol=2e-6)
                names=[p['port'] for p in row['action'].get('ports',[])]
                self.assertEqual(len(names),len(set(names)))
        self.assertTrue(any(r['action']['kind']=='place' for r in rows))

    def test_joint_port_rotation_action_is_reachable_and_native_validated(self):
        session,observation=self.observation(); prepared=self.runtime.prepare(observation)
        action=dict(kind='place',instance='C1',x=26,y=16,rotation=180,ports=[
            dict(node=6,port='input-belt-2'),dict(node=11,port='input-belt-3'),dict(node=20,port='output-belt-1')])
        decision=decision_for(prepared,action,observation)
        decoded=policy.action(prepared,decision)
        self.assertEqual(decoded,action)
        outcome=self.runtime.step(session,observation,decoded)
        self.assertEqual(outcome['status'],'improved')
        self.assertEqual(outcome['best_metrics']['score'][0],665)
        after=json.loads(session.observation_json()); new=self.runtime.prepare(after)
        self.assertNotEqual(digest(prepared['edge_static']),digest(new['edge_static']))
        saved=json.loads(session.checkpoint_json()); restored=self.runtime.restore(saved)
        self.assertEqual(semantic(after),semantic(json.loads(restored.observation_json())))

    def preparation_chunks(self):
        stream=Stream(self.runtime,0)
        first=scripted_chunk(self.model,stream,[dict(kind='insert_band',axis='x',at=24,count=1),
            dict(kind='delete_band',axis='x',at=24,count=1)])
        saved=stream.snapshot()
        self.assertEqual(saved['policy_actions'],2)
        stream=Stream.restore(self.runtime,saved)
        second=scripted_chunk(self.model,stream,[dict(kind='delete_band',axis='x',at=24,count=1),
            dict(kind='place',instance='C0',x=26,y=2,rotation=90)])
        return first,second

    def test_preparation_survives_valid_nonimprovement_chunk_restore_and_terminal_credit(self):
        first,second=self.preparation_chunks()
        evidence,buffer=MemoryEvidence(),Buffer()
        for ordinal,chunk in enumerate((first,second),1):
            evidence.begin(ordinal,chunk)
            buffer.ingest(chunk,ordinal,evidence,self.config['success'],1.)
            if ordinal==1:
                self.assertEqual(buffer.state['entries'],{})
                buffer=Buffer(buffer.snapshot())
        self.assertEqual(buffer.state['admissions'],1)
        sequence=buffer.eligible(evidence,3,self.config['strata'])[0]
        self.assertEqual([r['stage'] for r in sequence['rows']],['insert_band','delete_band','delete_band'])
        self.assertTrue(all(r['return_to_terminal']>0 for r in sequence['rows']))
        self.assertEqual(buffer.retained_chunks(),{1,2})
        with self.assertRaisesRegex(ValueError,'current update'):
            buffer.eligible(evidence,2,self.config['strata'])

    def test_imitation_increases_whole_joint_action_probability(self):
        _,observation=self.observation(); prepared=self.runtime.prepare(observation)
        action=dict(kind='place',instance='C1',x=26,y=16,rotation=180,ports=[
            dict(node=6,port='input-belt-2'),dict(node=11,port='input-belt-3'),dict(node=20,port='output-belt-1')])
        decision=decision_for(prepared,action,observation)
        batch=policy.prepare_rows([dict(input=prepared,decision=decision)])
        optimizer=torch.optim.Adam([p for n,p in self.model.named_parameters() if not n.startswith('critic.')],lr=.003)
        before=float(policy.predict(self.model,batch)[0].detach()[0])
        for _ in range(12):
            optimizer.zero_grad(set_to_none=True)
            loss=-policy.predict(self.model,batch)[0].mean();loss.backward();optimizer.step()
        after=float(policy.predict(self.model,batch)[0].detach()[0])
        self.assertGreater(after,before)
        self.assertTrue(all(p.grad is None for p in self.model.critic.parameters()))

    def test_nonempty_adam_exact_update_replay_and_actor_safe_kl_tail(self):
        first,second=self.preparation_chunks()
        self.config['ppo']['target_kl']=0.
        evidence=MemoryEvidence()
        learner=Learner(17,self.config,'test')
        first['model_before']=digest(learner.model.state_dict())
        learner.update(first,evidence)
        # The diagnostic reuses recorded data, so explicitly bind the updated
        # policy likelihood before checking a second optimizer transition.
        with torch.no_grad():
            lp,ent,values=policy.predict(learner.model,policy.prepare_rows(second['samples']))
        for row,l,e,v in zip(second['samples'],lp,ent,values,strict=True):
            row.update(old_logp=float(l),old_value=float(v),entropy=float(e))
        second['model_before']=digest(learner.model.state_dict())
        saved=deepcopy(learner.checkpoint([]))
        results=[]
        for _ in range(2):
            restored=Learner(17,self.config,'test',saved)
            result=restored.update(deepcopy(second),evidence)
            self.assertEqual(result['ppo']['actor_epochs'],1)
            self.assertEqual(result['ppo']['critic_epochs'],4)
            self.assertTrue(result['ppo']['critic_tail_preserves_actor'])
            results.append({k:result['ppo'][k] for k in ('model_after','actor_optimizer','critic_optimizer','shuffle_rng','targets_sha256','advantages_sha256')})
        self.assertEqual(*results)


if __name__=='__main__':
    unittest.main()
