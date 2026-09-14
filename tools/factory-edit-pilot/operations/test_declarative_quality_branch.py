"""Quality allocation must measure frequency and preserve actual learner behavior."""
from copy import deepcopy
import os
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path[:0]=[str(Path(__file__).resolve().parents[1]),str(Path(__file__).resolve().parent)]
import torch
from blueprint_execution_update import imitation,prepare_minibatch,summarize_imitation
from blueprint_ppo_update import digest
from declarative_quality_branch import PROFILES,assert_contract,prepare,profile_success,transition_saved
from declarative_quality_observer import Observer,quartiles
from declarative_replay_sampling import probabilities
from test_declarative_replay_retention import CONFIG


def entry(identity,gain):
    return dict(id=identity,input=None,labels=[dict(stage='delete_band')],
        retention_features_v1=dict(has_place=False,gain=gain,exterior_release=0.))


class QualityTests(unittest.TestCase):
    def observer(self):
        entries={'a':entry('a',.01),'b':entry('b',.2)}
        learner=SimpleNamespace(config=dict(strata={'small':['fixture-diagnostic']},success={'coefficient':.05}),
            buffer=SimpleNamespace(state={'entries':entries},admission_observer=None))
        observer=Observer(learner,quartiles(entries,learner.config['strata']));observer.begin();return observer

    def test_identical_coverage_different_frequency_changes_native_quality(self):
        results=[]
        for ids in (['a']*7+['b'],['a']+['b']*7):
            observer=self.observer();observer.measure([[1.,0.,1.,2.]]*8,
                [(i,1.,True) for i in ids],[(1.,8)]*8)
            rows=list(observer.groups.values())
            results.append(sum(r['native_gain_mass'] for r in rows)/sum(r['draws'] for r in rows))
        self.assertEqual(results,[.03375,.17625])

    def test_actual_stage_and_priority_correction_contributions(self):
        observer=self.observer();labels=[dict(stage=s,return_to_terminal=1.) for s in ('a','a','b')]
        sequence=dict(id='a',origin='own',stratum='small',rows=labels,sil_correction=.5)
        likelihood=SimpleNamespace(prepare_rows=lambda rows:dict(count=len(rows)),device_rows=lambda rows,device:rows,
            predict=lambda model,rows:(-torch.tensor([2.,4.,10.]*(rows['count']//3)),
                torch.zeros(rows['count']),torch.tensor([0.,2.,0.]*(rows['count']//3))))
        prepared=prepare_minibatch([],[],torch.empty(0),torch.empty(0),[sequence]*8,likelihood,True,True)['sil']
        loss,measurements=imitation(None,prepared,'cpu',likelihood)
        summarize_imitation([measurements],prepared['groups'],prepared['utility_groups'],True,
            observer.measure,prepared['allocation_groups'])
        row=next(iter(observer.groups.values()))
        self.assertAlmostEqual(row['corrected_sil_contribution'],float(loss))
        self.assertEqual(row['raw_sil_contribution'],44.)
        self.assertEqual(row['positive_stage_weight'],6.);self.assertEqual(row['zero_stage_weight'],2.)
        self.assertEqual(row['corrected_positive_stage_weight'],.375)
        self.assertEqual(row['corrected_zero_stage_weight'],.125)

    def test_gain_only_is_monotonic_and_ignores_rarity_and_geometry(self):
        rows=[entry('a',.01),entry('b',.1),entry('c',.2)]
        success=profile_success({'retention':CONFIG},'gain-8');settings=success['sampling']
        expected,_=probabilities(rows,settings)
        self.assertLess(expected[0],expected[1]);self.assertLess(expected[1],expected[2])
        rows[0]['retention_features_v1'].update(has_place=True,exterior_release=1.)
        self.assertEqual(probabilities(rows,settings)[0],expected)

    def test_priority_continuation_preserves_all_utility_and_observation_state(self):
        success=profile_success({'retention':CONFIG},'priority-8');e=entry('a',.1)
        e.update(sampling_priority_v1=dict(value=.123,observed_update=99),
            utility=dict(ema=.47,draws=123,updates=17,last_update=99))
        value=dict(saved=dict(buffer={'entries':{'a':e}},sampler_rng=torch.Generator().get_state(),
            shuffle_rng=torch.Generator().get_state()),launch={'config':{'success':success}},study={'replicate':1})
        with patch('declarative_quality_branch.bootstrap_priorities',side_effect=AssertionError('no bootstrap')):
            result=transition_saved(value,success,{'a':e['retention_features_v1']},1)
        self.assertEqual(digest(value['saved']),digest(result))
        bad=deepcopy(value);del bad['saved']['buffer']['entries']['a']['sampling_priority_v1']
        with self.assertRaisesRegex(ValueError,'priority-metadata'):
            transition_saved(bad,success,{'a':e['retention_features_v1']},1)
        with self.assertRaisesRegex(ValueError,'must not bootstrap'):
            transition_saved(value,success,{'a':e['retention_features_v1']},1,{'a':e['sampling_priority_v1']})

    def test_unknown_profile_rejects_before_output_publication(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'branch'
            with self.assertRaisesRegex(ValueError,'profile'):
                prepare({},path,profile='unknown',panel=path,updates=32,seconds=600)
            self.assertFalse(path.exists())

    def test_complete_contract_rejects_optimizer_return_native_and_input_changes(self):
        previous=json.loads((Path(__file__).resolve().parents[1]/'declarative-four-hour.json').read_text())
        config=deepcopy(previous);config['success']=profile_success(previous['success'],'gain-8')
        config['training'].update(first_update=1908,updates=1940)
        config['evaluation']['at']=[1908]
        config['retention']['training_traces']=[]
        config['duration'].update(seconds=400.,finish_grace_seconds=200.)
        config['limits'].update(arm_seconds=600.,matrix_seconds=600.)
        assert_contract(config,previous,'gain-8',32,600.,{})
        for section,field,value in (('ppo','learning_rate',.003),('ppo','gamma',.9),
                ('native','route_expansions',5000),('training_input','seed_base',123),
                ('training','streams',8),('success','coefficient',.1)):
            bad=deepcopy(config);bad[section][field]=value
            with self.assertRaisesRegex(ValueError,'forbidden'):
                assert_contract(bad,previous,'gain-8',32,600.,{})

    def test_replicate_two_resets_only_declared_rngs_and_preserves_parent(self):
        e=entry('a',.1);success={'retention':CONFIG}
        saved=dict(model={'weight':torch.tensor([1.])},actor_optimizer={'state':'actual'},
            critic_optimizer={'state':'actual'},buffer={'entries':{'a':e}},streams=['original'],
            sampler_rng=torch.Generator().manual_seed(10).get_state(),shuffle_rng=torch.Generator().manual_seed(20).get_state())
        value=dict(saved=saved,launch={'config':{'success':success}});before=digest(saved)
        result=transition_saved(value,success,{'a':e['retention_features_v1']},2)
        self.assertEqual(digest(saved),before)
        for key in ('model','actor_optimizer','critic_optimizer','buffer','streams'):self.assertEqual(digest(saved[key]),digest(result[key]))
        self.assertTrue(torch.equal(result['sampler_rng'],torch.Generator().manual_seed(714400029).get_state()))
        self.assertTrue(torch.equal(result['shuffle_rng'],torch.Generator().manual_seed(714500029).get_state()))

    def test_source_rejection_and_initialization_failure_leave_no_child(self):
        config=json.loads((Path(__file__).resolve().parents[1]/'declarative-four-hour.json').read_text())
        value=dict(launch=dict(config=config,source=dict(native_library='fixture',native_sha256='native')),
            saved=dict(update=1908,buffer={'entries':{}},model={},actor_optimizer={},critic_optimizer={},
                sampler_rng=torch.Generator().get_state(),shuffle_rng=torch.Generator().get_state(),streams=[]),
            binding={'checkpoint':'immutable'},job={})
        before=digest(value)
        with tempfile.TemporaryDirectory() as root:
            root=Path(root);panel=root/'panel.json';panel.write_text(json.dumps(dict(schema='declarative-learning-panel-v1',status='complete')))
            output=root/'child'
            with patch('declarative_quality_branch.source_binding',side_effect=ValueError('missing source binding')):
                with self.assertRaisesRegex(ValueError,'source binding'):
                    prepare(value,output,profile='baseline-8',panel=panel,updates=32,seconds=600,features={})
            self.assertFalse(output.exists())
            with patch('declarative_quality_branch.source_binding',return_value={'native_sha256':'native'}),\
                 patch('declarative_quality_branch.cpu_allocation',return_value={}),\
                 patch('declarative_quality_branch.link_parent_sources',return_value={}),\
                 patch('declarative_quality_branch.initialize',side_effect=OSError('injected publication failure')):
                with self.assertRaisesRegex(OSError,'publication failure'):
                    prepare(value,output,profile='baseline-8',panel=panel,updates=32,seconds=600,features={})
            self.assertFalse(output.exists());self.assertTrue(panel.exists());self.assertEqual(digest(value),before)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class NativeQualityTests(unittest.TestCase):
    def test_observer_preserves_model_adam_rng_and_new_uniform_priority_metadata(self):
        import declarative_learning_policy as policy
        from declarative_learning_runtime import Learner
        from test_declarative_replay_sampling import NativeSamplingTests
        from test_declarative_learning import MemoryEvidence
        helper,config,chunks,evidence,saved=NativeSamplingTests().prepared()
        for profile in PROFILES:
            selected=deepcopy(config);selected['success']=profile_success(selected['success'],profile)
            results=[];calls=[]
            for enabled in (False,True):
                learner=Learner(17,selected,'sampling-test',saved)
                source=MemoryEvidence();source.chunks=deepcopy(evidence.chunks)
                observer=Observer(learner,quartiles(saved['buffer']['entries'],selected['strata'])) if enabled else None
                with patch.object(policy,'predict',wraps=policy.predict) as predict:
                    records=[]
                    for chunk in chunks:
                        if observer:observer.begin()
                        _,record=helper.update(learner,chunk,source)
                        if observer:
                            allocation=observer.finish(record)
                            self.assertEqual(sum(r['draws'] for r in allocation['allocation']),
                                sum(map(len,record['ppo']['success']['selected_ids'])))
                        records.append(dict(events=record['events'],selected=record['ppo']['success']['selected_ids']))
                    calls.append(predict.call_count)
                result=deepcopy(learner.checkpoint([]))
                if enabled:self.assertTrue(all('retention_features_v1' in e for e in result['buffer']['entries'].values()))
                for e in result['buffer']['entries'].values():e.pop('retention_features_v1',None)
                results.append(dict(checkpoint=digest(result),records=records))
            self.assertEqual(*calls);self.assertEqual(*results,profile)


if __name__=='__main__':unittest.main()
