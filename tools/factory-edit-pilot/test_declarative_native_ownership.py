"""Native boundary parity, historical ownership, chunk credit and recovery."""
from copy import deepcopy
import gc
import json
import os
from pathlib import Path
import pickle
import tempfile
import time
import unittest
from unittest.mock import patch

import torch

from blueprint_ppo_store import load, save
from blueprint_ppo_update import digest
from declarative_band_mask import MODE
from declarative_learning_buffer import physical
from declarative_learning_environment import Runtime, Stream
from declarative_learning_worker import Worker
from declarative_replay_storage import replay_input
import declarative_learning_policy as policy
from ppo_tensor_pickle import dumps
import test_declarative_learning as baseline


def compare_input(test, runtime, session, native):
    observation=json.loads(session.observation_json())
    oracle=runtime.prepare(observation)
    test.assertEqual(json.loads(session.learning_roots_json()),oracle['actions'])
    for key,value in oracle.items():
        if isinstance(value,torch.Tensor):
            expected=value.clone()
            if key=='global_features':
                # Observation calls happen at different wall times. Attempts
                # and every other feature must still be bit-identical.
                expected[-2]=native[key][-2]
            torch.testing.assert_close(native[key],expected,rtol=0,atol=0,msg=key)
        elif key=='root_owners':
            test.assertEqual(native[key].tolist(),value)
        elif key not in ('actions','port_groups'):
            test.assertEqual(native[key],value,key)
    test.assertEqual(len(native['port_groups']),len(oracle['port_groups']))
    for left,right in zip(native['port_groups'],oracle['port_groups'],strict=True):
        test.assertEqual(len(left),len(right))
        for a,b in zip(left,right,strict=True):
            test.assertEqual({k:v for k,v in a.items() if k!='features'},
                {k:v for k,v in b.items() if k!='features'})
            test.assertEqual(set(a['features']),set(b['features']))
            for rotation in b['features']:
                torch.testing.assert_close(a['features'][rotation],b['features'][rotation],rtol=0,atol=0)
    oracle['global_features'][-2]=native['global_features'][-2]
    return observation,oracle


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'selected native library required')
class NativeOwnershipTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        self.config=baseline.diagnostic_config()
        self.config['actions']['masking']=MODE
        self.runtime=Runtime(self.config,os.environ['AIC_NATIVE_LIBRARY'])
        self.model,_,_=policy.learners(17,self.config['ppo'])

    def worker(self, saved=None):
        worker=Worker();worker.runtime=self.runtime
        worker.stream=Stream(self.runtime,0) if saved is None else Stream.restore(self.runtime,saved)
        return worker

    def chunk(self, worker, actions):
        prepared=worker.execute(dict(operation='begin'))['input'];samples=[]
        for action in actions:
            observation,oracle=compare_input(self,self.runtime,worker.stream.session,prepared)
            decision=baseline.decision_for(oracle,action,observation)
            with torch.no_grad():
                lp,ent,value=policy.predict(self.model,policy.prepare_rows([dict(input=prepared,decision=decision)]))
            command=dict(operation='step',decision=decision,old_logp=float(lp[0]),old_value=float(value[0]),entropy=float(ent[0]))
            inputs=prepared
            prepared=worker.execute(command)['input']
            samples.append(dict(input=inputs,expected_observation=observation))
        with torch.no_grad():value=policy.values(self.model,[prepared])[0]
        result=worker.execute(dict(operation='finish',value=value))
        for sample,expected in zip(result['samples'],samples,strict=True):
            sample['input']=expected['input']
            if sample['state_changed']:
                self.assertEqual(physical(sample['observation']),physical(expected['expected_observation']))
        return dict(schema='declarative-learning-chunk-v1',model_before=digest(self.model.state_dict()),
            native_before=[result['native_before']],samples=result['samples'],frontiers=[result['frontier']],
            completed=[] if result['completed'] is None else [result['completed']],collection_seconds=0.)

    def preparation_chunks(self):
        worker=self.worker()
        first=self.chunk(worker,[dict(kind='insert_band',axis='x',at=24,count=1),
            dict(kind='delete_band',axis='x',at=24,count=1)])
        saved=worker.execute(dict(operation='snapshot'))['stream']
        self.assertEqual(saved['policy_actions'],2)
        second=self.chunk(self.worker(saved),[dict(kind='delete_band',axis='x',at=24,count=1),
            dict(kind='place',instance='C0',x=26,y=2,rotation=90)])
        return first,second

    def test_preparation_credit_across_native_chunk_and_restore(self):
        baseline.NativeLearningTests.test_preparation_survives_valid_nonimprovement_chunk_restore_and_terminal_credit(self)

    def test_nonempty_adam_restoration_uses_native_recorded_inputs(self):
        baseline.NativeLearningTests.test_nonempty_adam_exact_update_replay_and_actor_safe_kl_tail(self)

    def test_ordered_sampling_selected_action_and_sparse_archive_likelihood(self):
        session,_=self.runtime.open(64,60000)
        for seed in range(12):
            _,prepared=self.runtime.observe(session)
            _,oracle=compare_input(self,self.runtime,session,prepared)
            with torch.no_grad():
                selected=policy.sample(self.model,[prepared],torch.Generator().manual_seed(seed))[0]
                expected=policy.sample(self.model,[oracle],torch.Generator().manual_seed(seed))[0]
            self.assertEqual(selected,{k:v for k,v in expected.items() if k!='action'})
            before=deepcopy(prepared)
            result=self.runtime.step_index(session,prepared,selected['decision'])
            self.assertEqual(result['action'],expected['action'])
            for key,value in before.items():
                if isinstance(value,torch.Tensor):self.assertTrue(torch.equal(value,prepared[key]))
            sparse=replay_input(prepared,selected['decision'],result['action'])
            self.assertNotIn('native_domain',sparse);self.assertNotIn('native_handle',sparse)
            with tempfile.TemporaryDirectory() as directory:
                path=Path(directory)/'row.pt.gz';save(path,sparse);restored=load(path)
            with torch.no_grad():
                a=policy.predict(self.model,policy.prepare_rows([dict(input=prepared,**selected)]))
                b=policy.predict(self.model,policy.prepare_rows([dict(input=restored,**selected)]))
            for x,y in zip(a,b,strict=True):torch.testing.assert_close(x,y,rtol=0,atol=0)

    def test_handles_and_invalid_selection_reject_before_charging(self):
        session,_=self.runtime.open(64,60000)
        _,prepared=self.runtime.observe(session)
        handle=prepared['native_handle']
        def rejects(handle,root,ports,code):
            before=json.loads(session.checkpoint_json())
            with self.assertRaisesRegex(ValueError,code):session.step_index(handle,root,ports)
            after=json.loads(session.checkpoint_json())
            for key in ('attempts','revision','work','state'):
                self.assertEqual(before[key],after[key])
        invalid=int(prepared['root_invalid'].nonzero()[0])
        rejects(handle,invalid,[],'learning-selection')
        rejects(handle,len(prepared['actions']),[],'learning-selection')
        place=next(i for i,a in enumerate(prepared['actions']) if a['kind']=='place')
        rejects(handle,place,[],'learning-selection')
        groups=prepared['port_groups'][prepared['root_owners'][place]]
        rejects(handle,place,[999]*len(groups),'learning-selection')
        rejects(handle+1000,0,[],'learning-handle')
        other,_=self.runtime.open(64,60000);_,foreign=self.runtime.observe(other)
        rejects(foreign['native_handle'],0,[],'learning-handle')
        _,new=self.runtime.observe(session)
        rejects(handle,0,[],'learning-handle')
        with torch.no_grad():selected=policy.sample(self.model,[new],torch.Generator().manual_seed(18))[0]
        session.step_index(new['native_handle'],**selected['decision'])
        rejects(new['native_handle'],**selected['decision'],code='learning-handle')

    def test_array_lifetime_and_lazy_descriptors_survive_transport(self):
        session,_=self.runtime.open(64,60000);_,prepared=self.runtime.observe(session)
        expected=deepcopy(prepared);buffers=[]
        packed=dumps(prepared,buffer_callback=buffers.append)
        restored=pickle.loads(packed,buffers=buffers)
        self.assertEqual(restored['port_groups'].cache,{})
        del prepared,packed,session;gc.collect()
        with torch.no_grad():
            a=policy.sample(self.model,[expected],torch.Generator().manual_seed(71))
            b=policy.sample(self.model,[restored],torch.Generator().manual_seed(71))
        self.assertEqual(a,b)

    def test_partial_failure_retains_native_evidence_and_full_trace_is_opt_in(self):
        worker=self.worker();prepared=worker.execute(dict(operation='begin'))['input']
        _,oracle=compare_input(self,self.runtime,worker.stream.session,prepared)
        decision=baseline.decision_for(oracle,dict(kind='insert_band',axis='x',at=24,count=1),
            json.loads(worker.stream.session.observation_json()))
        worker.stream.session.step_index(prepared['native_handle'],**decision)
        with tempfile.TemporaryDirectory() as directory:
            worker.preserve(Path(directory),ValueError('after charged native step'))
            saved=torch.load(next(Path(directory).glob('worker-*.pt')),weights_only=False)
        self.assertEqual(saved['samples'],[])
        self.assertEqual(len(saved['native_evidence']),1)
        self.assertEqual(saved['stream']['native']['attempts'],1)
        self.assertIn('facilities',saved['native_evidence'][0]['observation'])
        self.assertNotIn('blueprint',saved['native_evidence'][0]['observation'])
        session,_=self.runtime.open(64,60000);before=json.loads(session.observation_json())
        session.begin_learning_chunk(1,True);_,prepared=self.runtime.observe(session)
        oracle=self.runtime.prepare(before)
        decision=baseline.decision_for(oracle,dict(kind='insert_band',axis='x',at=24,count=1),before)
        session.step_index(prepared['native_handle'],**decision)
        full=session.take_learning_chunk()[0]['observation']
        self.assertEqual(physical(full),physical(before));self.assertIn('blueprint',full)

    def test_evaluation_uses_numerical_path_with_and_without_trace(self):
        from declarative_learning_evaluate import evaluate
        session,_=self.runtime.open(64,60000);initial=json.loads(session.observation_json())
        case=dict(initial_metrics=initial['initial_metrics'],generation_seed=1,
            files=dict(request=dict(sha256='fixture'),initial=dict(sha256='fixture')),
            geometry_identity='fixture',product='fixture',facilities=len(initial['facilities']))
        for retain_trace in (False,True):
            with tempfile.TemporaryDirectory() as directory,patch.object(self.runtime,'prepare',side_effect=AssertionError('Python oracle used')):
                result=evaluate(self.runtime,self.model,case,dict(evaluation=dict(max_actions=4,elapsed_ms=60000,
                    action_milestones=[2,4],retain_trace=retain_trace),action_seed=45,deadline=time.monotonic()+60,
                    output=directory,sample=0,seed=1,update=0))
                self.assertEqual(result['policy_actions'],4);self.assertTrue(result['factory_validated'])
                if retain_trace:
                    trace=load(next(Path(directory).glob('*.pt.gz')))['trace']
                    self.assertEqual(len(trace),4);self.assertIn('blueprint',trace[0]['observation'])
                else:self.assertEqual(list(Path(directory).iterdir()),[])


if __name__=='__main__':unittest.main()
