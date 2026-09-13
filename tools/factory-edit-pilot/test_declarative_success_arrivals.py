"""Complete terminal attempts without observer effects on the actual learner."""
from copy import deepcopy
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import torch

sys.path.insert(0,str(Path(__file__).parent/'diagnostics'))
from declarative_success_arrivals import Collector
from blueprint_ppo_store import load, reference, save
from blueprint_ppo_update import digest
from declarative_learning_runtime import Learner
from declarative_replay_storage import compact, references
from test_declarative_replay_retention import CONFIG
import declarative_learning_policy as policy
import test_declarative_learning as fixture


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class ArrivalTests(unittest.TestCase):
    def chunks(self):
        native=fixture.NativeLearningTests();native.setUp();a=native.preparation_chunks()
        stream=fixture.Stream(native.runtime,1)
        action=dict(kind='place',instance='C1',x=26,y=16,rotation=180,ports=[
            dict(node=6,port='input-belt-2'),dict(node=11,port='input-belt-3'),dict(node=20,port='output-belt-1')])
        b=[fixture.scripted_chunk(native.model,stream,[action,action]) for _ in range(2)]
        chunks=[]
        for x,y in zip(a,b,strict=True):
            value=deepcopy(x)
            for key in ('native_before','samples','frontiers','completed'):value[key]+=deepcopy(y[key])
            chunks.append(value)
        config=deepcopy(native.config);config['success'].update(retention=CONFIG,max_sequences=1)
        return config,chunks

    def update(self,learner,payload,evidence):
        payload=deepcopy(payload)
        with torch.no_grad():lp,ent,values=policy.predict(learner.model,policy.prepare_rows(payload['samples']))
        for row,l,e,v in zip(payload['samples'],lp,ent,values,strict=True):
            row.update(old_logp=float(l),old_value=float(v),entropy=float(e))
        payload['model_before']=digest(learner.model.state_dict())
        return payload,learner.update(payload,evidence)

    def publish(self,collector,learner,payload,record,evidence,root,job):
        n=learner.ordinal;collector.before_publish(learner,payload,record,evidence)
        path=root/f'replay/{n:04d}.pt.gz';save(path,compact(payload,references(learner.buffer.state).get(n,set())))
        job['replay_sources'][str(n)]=reference(path,root);job['latest_checkpoint']={'path':f'checkpoints/{n}.pt.gz','sha256':'fixture'}
        job['records'].append({'path':f'updates/{n}.json','sha256':'fixture'})
        collector.after_retain(learner,root,job)

    def test_observer_preserves_two_updates_and_keeps_same_update_evicted_rows(self):
        config,chunks=self.chunks();results=[];forward_counts=[]
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);collector=Collector({'saved':{'update':0}},root/'corpus',lambda:None)
            for enabled in (False,True):
                learner=Learner(17,config,'arrival-test');evidence=fixture.MemoryEvidence();records=[];calls=[0]
                if enabled:learner.buffer.admission_observer=collector.observe
                job=dict(replay_sources={},retained_files={},retained_bytes=0,records=[])
                original=policy.predict
                def counted(*args,**kwargs):calls[0]+=1;return original(*args,**kwargs)
                with patch.object(policy,'predict',counted):
                    for chunk in chunks:
                        payload,record=self.update(learner,chunk,evidence)
                        records.append(dict(checkpoint=digest(learner.checkpoint([])),events=record['events'],
                            selected=record['ppo']['success']['selected_ids']))
                        if enabled:self.publish(collector,learner,payload,record,evidence,root/'baseline',job)
                results.append(records);forward_counts.append(calls[0])
            self.assertEqual(*results);self.assertEqual(*forward_counts)
            boundary=load(root/'corpus'/collector.boundaries[-1]['reference']['path'])
            self.assertEqual(len(boundary['attempts']),2)
            self.assertEqual(boundary['counts']['admitted'],2);self.assertEqual(boundary['counts']['evicted'],1)
            self.assertEqual([a['key'] for a in boundary['attempts']],[(2,0,0),(2,1,0)])
            evicted=next(e['id'] for e in boundary['events'] if e['kind']=='evicted')
            entry=next(a['entry'] for a in boundary['attempts'] if a['entry']['id']==evicted)
            baseline=load(root/'baseline/replay/0002.pt.gz')
            self.assertEqual(baseline['samples'],{})
            self.assertIn(0,load(root/'corpus/union/0002.pt.gz')['samples'])
            # Later baseline pruning does not remove union rows or historical
            # checkpoint/live roots needed by a counterfactual retained entry.
            (root/'baseline/replay/0002.pt.gz').unlink()
            for label in entry['labels']:
                n,i=label['reference'];self.assertIn(i,load(root/f'corpus/union/{n:04d}.pt.gz')['samples'])
            self.assertEqual(boundary['mandatory_checkpoints'],[1,2])
            self.assertTrue(load(root/'corpus/boundaries/0001.pt.gz')['live_roots'])

    def test_active_duplicate_and_readmission_keep_distinct_attempt_keys(self):
        config,chunks=self.chunks();learner=Learner(17,config,'attempt-test');evidence=fixture.MemoryEvidence();attempts=[]
        learner.buffer.admission_observer=lambda key,entry,source:attempts.append((key,deepcopy(entry)))
        # Retain both IDs for an active-duplicate wave, then constrain capacity
        # for an actual ordered eviction/readmission wave.
        config['success']['max_sequences']=2
        all_events=[]
        for chunk in chunks*2:
            _,record=self.update(learner,chunk,evidence);all_events+=record['events']
        self.assertEqual(sum(e['kind']=='duplicate' for e in all_events),2)
        retired=next(iter(learner.buffer.state['entries']))
        removed=learner.buffer.state['entries'].pop(retired);learner.buffer.cache.pop(retired,None)
        learner.buffer.label_count-=len(removed['labels'])
        for chunk in chunks:
            _,record=self.update(learner,chunk,evidence);all_events+=record['events']
        self.assertEqual(len({key for key,_ in attempts}),6)
        self.assertEqual(sum(e['kind']=='admitted' and e['id']==retired for e in all_events),2)
        self.assertNotIn('admission_observer',learner.buffer.snapshot())

    def test_multiple_successes_at_one_terminal_keep_full_later_return(self):
        native=fixture.NativeLearningTests();native.setUp();stream=fixture.Stream(native.runtime,0)
        joint=dict(kind='place',instance='C1',x=26,y=16,rotation=180,ports=[
            dict(node=6,port='input-belt-2'),dict(node=11,port='input-belt-3'),dict(node=20,port='output-belt-1')])
        noop=dict(kind='place',instance='C0',x=26,y=2,rotation=90)
        chunk=fixture.scripted_chunk(native.model,stream,[joint,dict(kind='delete_band',axis='x',at=24,count=1),noop,noop])
        config=native.config;learner=Learner(17,config,'terminal-test');attempts=[]
        learner.buffer.admission_observer=lambda key,entry,source:attempts.append((key,deepcopy(entry)))
        _,record=self.update(learner,chunk,fixture.MemoryEvidence())
        self.assertGreaterEqual(len(attempts),2)
        self.assertEqual([key for key,_ in attempts],[(1,0,i) for i in range(len(attempts))])
        self.assertAlmostEqual(attempts[0][1]['labels'][0]['return_to_terminal'],sum(s['reward'] for s in chunk['samples']))
        self.assertEqual(len(attempts),sum(e['kind']=='admitted' for e in record['events']))


if __name__=='__main__':unittest.main()
