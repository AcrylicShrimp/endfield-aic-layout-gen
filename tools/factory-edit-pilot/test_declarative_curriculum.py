"""Curriculum quotas, success filtering, exact inherited input and corruption."""
from copy import deepcopy
from pathlib import Path
import json
import tempfile
import unittest
from unittest.mock import patch

from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest
from declarative_learning_curriculum import choose, next_plan, inherited
from declarative_learning_environment import Stream
from declarative_learning_buffer import Buffer
import test_declarative_learning as learning_tests

CONFIG=read(ROOT/'tools/factory-edit-pilot/declarative-curriculum-short.json')['start_curriculum']


def parents(wave, count=8):
    return [dict(seed=17,wave=wave-1,stream=i,depth=0 if i<7 else 1,
        complete=True,validated=True,start_score=[100],best_score=[90]) for i in range(count)]


class CurriculumTests(unittest.TestCase):
    def test_fractional_quota_and_depth_two_alignment(self):
        totals={0:0,1:0,2:0}
        for wave in range(2,102):
            plan=choose(CONFIG,17,wave,8,parents(wave))
            for depth,count in plan['requested'].items():totals[int(depth)]+=count
            self.assertEqual(sum(plan['actual'].values()),8)
            self.assertEqual(plan,choose(CONFIG,17,wave,8,list(reversed(parents(wave)))))
        self.assertEqual(totals,{0:640,1:80,2:80})

    def test_only_strict_area_success_and_no_replacement(self):
        rows=parents(3)
        for row in rows:row['best_score']=[100,0]
        rows[2]['best_score']=[99,1000]
        plan=choose(CONFIG,17,3,8,rows)
        self.assertEqual(plan['actual'],{'0':7,'1':1})
        self.assertEqual([s['parent']['stream'] for s in plan['slots'] if s['depth']], [2])
        self.assertTrue(plan['diagnostics'])
        rows[2]['complete']=False
        with self.assertRaisesRegex(ValueError,'incomplete'):choose(CONFIG,17,3,8,rows)

    def test_restored_replay_hydrates_each_chunk_once_in_identity_order(self):
        buffer=Buffer()
        for i,refs in enumerate(([2,1],[1,2],[2,1])):
            buffer.state['entries'][str(i)]=dict(admitted_update=2,input=dict(draw=dict(product='small')),
                labels=[dict(reference=(n,0),stage='delete_band',return_to_terminal=1.) for n in refs])
        class Evidence:
            def __init__(self):self.last=None;self.loads=[]
            def sample(self,ref):
                if ref[0]!=self.last:self.loads.append(ref[0]);self.last=ref[0]
                return dict(input=ref[0],decision=ref)
        evidence=Evidence();rows=buffer.eligible(evidence,3,{'small':['small']})
        self.assertEqual(evidence.loads,[1,2])
        self.assertEqual([r['id'] for r in rows],['0','1','2'])
        self.assertEqual([[r['input'] for r in s['rows']] for s in rows],[[2,1],[1,2],[2,1]])
        buffer.eligible(evidence,4,{'small':['small']});self.assertEqual(evidence.loads,[1,2])


class NativeCurriculumTests(unittest.TestCase):
    def test_parent_best_admission_live_restore_and_corrupt_input(self):
        helper=learning_tests.NativeLearningTests();helper.setUp()
        _,payload=helper.preparation_chunks()
        row=payload['completed'][0]
        self.assertLess(row['result']['best_metrics']['score'][0],row['initial_score'][0])
        from random_factory_generation import geometry_identity
        original=payload['native_before'][0]['native']['original']
        request=payload['native_before'][0]['native']['request']
        metadata=dict(generation_seed=17,request_sha256=digest(request),geometry_sha256=geometry_identity(original),
            draw=dict(product='fixture-diagnostic'))
        row.update(episode=8,training_input=metadata)
        config=deepcopy(helper.config);config['start_curriculum']=CONFIG
        with tempfile.TemporaryDirectory() as tmp:
            directory=Path(tmp)
            plan,_=next_plan(config,17,directory,payload,dict(path='chunk.pt.gz',sha256='1'*64),legacy=True)
            slot=plan['slots'][0];self.assertEqual(slot['depth'],1)
            value,_=inherited(slot,0,9)
            self.assertEqual(value['original'],row['result']['best'])
            # Use the real native admission and restoration code, with explicit
            # fixture inputs; this diagnostic never creates a training factory.
            runtime=helper.runtime;runtime.config=config;runtime.inputs=object()
            stream=Stream(runtime,0);stream.episode=9;stream.next_start=slot
            observation=stream.observe()
            self.assertEqual(observation['initial_metrics'],row['result']['best_metrics'])
            self.assertEqual(observation['best_metrics'],observation['initial_metrics'])
            saved=stream.snapshot();restored=Stream.restore(runtime,saved)
            self.assertEqual(restored.metadata,saved['metadata'])
            self.assertEqual(json.loads(restored.session.observation_json())['initial_metrics'],observation['initial_metrics'])
            edited_geometry=geometry_identity(row['result']['best'])
            self.assertNotEqual(metadata['geometry_sha256'],edited_geometry)
            runtime.reserved=frozenset([edited_geometry])
            with patch.object(type(runtime),'open',side_effect=AssertionError('opened reserved episode')), \
                 patch.object(type(runtime),'restore',side_effect=AssertionError('restored reserved episode')):
                rejected=Stream(runtime,0);rejected.episode=9;rejected.next_start=slot
                with self.assertRaisesRegex(ValueError,'input-reservation'):rejected.observe()
                self.assertIsNone(rejected.session);self.assertEqual(rejected.policy_actions,0)
                with self.assertRaisesRegex(ValueError,'input-reservation'):Stream.restore(runtime,saved)
            runtime.reserved=frozenset()
            # The second depth retains the same root and the exact new best.
            second=deepcopy(payload);second['completed'][0].update(episode=9,training_input=stream.metadata['training_input'])
            second['completed'][0]['initial_score']=[observation['initial_metrics']['score'][0]+1]
            second['native_before'][0]['native']['request']=request
            plan2,_=next_plan(config,17,directory,second,dict(path='chunk2.pt.gz',sha256='2'*64))
            slot2=plan2['slots'][0];self.assertEqual(slot2['depth'],2)
            value2,_=inherited(slot2,0,10)
            self.assertEqual(value2['original'],row['result']['best'])
            self.assertEqual(slot2['root'],slot['root'])
            path=directory/slot['input']['path'];path.write_text('{}')
            with self.assertRaisesRegex(ValueError,'identity'):inherited(slot,0,9)
            path.unlink()
            with self.assertRaises((ValueError,FileNotFoundError)):inherited(slot,0,9)


if __name__=='__main__':unittest.main()
