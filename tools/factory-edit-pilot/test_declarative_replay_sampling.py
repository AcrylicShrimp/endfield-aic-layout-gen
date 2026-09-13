"""Actual draw distributions, raw SIL measurements and authoritative resume state."""
from collections import Counter
from copy import deepcopy
import math
import os
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import torch

from blueprint_execution_update import draw_sequences, imitation, prepare_minibatch, summarize_imitation
from blueprint_ppo_update import digest
from declarative_learning_buffer import Buffer
from declarative_learning_runtime import Learner
from declarative_replay_sampling import Distribution, PriorityBootstrap, observe_priorities, probabilities, validate
from declarative_success_features import extract
from test_declarative_category_retention import featured
from test_declarative_replay_retention import STRATA
import declarative_learning_policy as policy
import test_declarative_learning as fixture


def setting(kind='uniform',count=8):
    result=dict(schema='declarative-success-sampling-v1',kind=kind,sequences_per_minibatch=count)
    if kind=='score_mixture':result.update(mixture=.5,score_profile='rare_3pp')
    if kind=='positive_advantage':result.update(alpha=.6,beta=.1,epsilon=1e-6)
    return result


def sequences():
    return [dict(id=str(i),stratum='large' if i%2 else 'small',
        retention_features_v1=dict(has_place=i%3==0,gain=.01+i/100,exterior_release=.1),
        sampling_priority_v1=dict(value=float(i),observed_update=1)) for i in range(8)]


class SamplingTests(unittest.TestCase):
    def test_explicit_uniform_eight_preserves_default_ids_and_both_rngs(self):
        for rows in ([],sequences()[::2],sequences()):
            distribution=Distribution(rows,setting(),'balanced_available')
            generators=[(torch.Generator().manual_seed(29),torch.Generator().manual_seed(41)) for _ in range(2)]
            for _ in range(20):
                a=draw_sequences(rows,'balanced_available',*generators[0])
                b=draw_sequences(rows,'balanced_available',*generators[1],distribution)
                self.assertEqual(a,b)
            for a,b in zip(*generators,strict=True):self.assertTrue(torch.equal(a.get_state(),b.get_state()))

    def test_counts_balance_missing_stratum_and_replacement(self):
        for kind in ('uniform','score_mixture','positive_advantage'):
            for count in (8,16,32):
                config=setting(kind,count)
                distribution=Distribution(sequences(),config,'balanced_available')
                selected=distribution.draw(torch.Generator().manual_seed(8),torch.Generator().manual_seed(9))
                self.assertEqual(Counter(r['stratum'] for r in selected),{'large':count//2,'small':count//2})
                one=Distribution(sequences()[:1],config,'balanced_available')
                selected=one.draw(torch.Generator(),torch.Generator())
                self.assertEqual(len(selected),count);self.assertEqual(len({r['id'] for r in selected}),1)
                self.assertEqual(Distribution([],config,'balanced_available').draw(torch.Generator(),torch.Generator()),[])
                with self.assertRaisesRegex(ValueError,'stratum-empty'):Distribution(sequences()[:1],config,'balanced')
                with self.assertRaisesRegex(ValueError,'stratum-empty'):Distribution([],config,'balanced')

    def test_score_probability_bounds_and_seeded_frequency(self):
        rows=sequences()[::2];config=setting('score_mixture');expected,_=probabilities(rows,config)
        self.assertAlmostEqual(sum(expected),1.)
        self.assertTrue(all(.5/len(rows)<=p<=4.5/len(rows) for p in expected))
        distribution=Distribution(rows,config,'balanced_available');draw=torch.Generator().manual_seed(818);order=torch.Generator().manual_seed(19)
        counts=Counter(r['id'] for _ in range(4000) for r in distribution.draw(draw,order))
        for row,p in zip(rows,expected,strict=True):self.assertLess(abs(counts[row['id']]/32000-p),.012)
        self.assertEqual(digest(distribution.groups[0]['cdf']),digest(Distribution(rows,config,'balanced_available').groups[0]['cdf']))

    def test_positive_reference_formula_zero_priority_and_partial_correction(self):
        rows=sequences();config=setting('positive_advantage');p,c=probabilities(rows,config)
        mass=[(r['sampling_priority_v1']['value']+1e-6)**.6 for r in rows]
        expected=[v/sum(mass) for v in mass];raw=[(1/(len(rows)*v))**.1 for v in expected]
        for a,b in zip(p,expected,strict=True):self.assertAlmostEqual(a,b)
        for a,b in zip(c,raw,strict=True):self.assertAlmostEqual(a,b/max(raw))
        self.assertGreater(p[0],0);self.assertEqual(max(c),1.)
        for row in rows:row['sampling_priority_v1']['value']=0.
        self.assertEqual(probabilities(rows,config),([1/8]*8,[1.]*8))

    def test_normalization_and_raw_measurements_are_independent_of_correction(self):
        labels=[dict(stage=stage,return_to_terminal=1.) for stage in ('a','a','b')]
        seq=dict(id='x',origin='own',stratum='small',rows=labels)
        likelihood=SimpleNamespace(prepare_rows=lambda rows:dict(count=len(rows)),device_rows=lambda rows,device:rows,
            predict=lambda model,rows:(-torch.tensor([2.,4.,10.]*(rows['count']//3)),torch.zeros(rows['count']),torch.zeros(rows['count'])))
        for count in (8,16,32):
            for correction in (1.,.5):
                prepared=prepare_minibatch([],[],torch.empty(0),torch.empty(0),[dict(seq,sil_correction=correction)]*count,
                    likelihood,measure_replay_utility=True)['sil']
                loss,measured=imitation(None,prepared,'cpu',likelihood)
                self.assertAlmostEqual(float(loss),6.5*correction)
                raw=summarize_imitation([measured],prepared['groups'],prepared['utility_groups'],measure_priority=True)
                self.assertEqual(raw['sequence_utility'],[dict(id='x',draws=count,mean_weighted_nll=6.5,mean_positive_advantage=1.)])

    def test_current_max_bootstrap_handles_removal_and_reused_identity(self):
        a=featured('a');a['sampling_priority_v1']=dict(value=10.,observed_update=2)
        b=featured('b');b['sampling_priority_v1']=dict(value=3.,observed_update=2)
        index=PriorityBootstrap({'a':a,'b':b},STRATA)
        self.assertEqual(index.maximum(a),10.)
        index.remove('a');self.assertEqual(index.maximum(a),3.)
        a['sampling_priority_v1']=dict(value=1.,observed_update=None);index.add(a)
        index.remove('b');self.assertEqual(index.maximum(a),1.)
        index.remove('a');self.assertEqual(index.maximum(a),1.)

    def test_invalid_settings_metadata_and_identity_order_reject(self):
        for config in (dict(setting(),sequences_per_minibatch=9),dict(setting(),alpha=.6),
                       dict(setting('score_mixture'),mixture=1),dict(setting('positive_advantage'),beta=1)):
            with self.assertRaisesRegex(ValueError,'replay-sampling'):validate(config)
        rows=sequences()
        with self.assertRaisesRegex(ValueError,'identity-order'):Distribution(rows[::-1],setting(),'balanced_available')
        del rows[0]['sampling_priority_v1']
        with self.assertRaisesRegex(ValueError,'priority-metadata'):Distribution(rows,setting('positive_advantage'),'balanced_available')
        del rows[1]['retention_features_v1']
        with self.assertRaisesRegex(ValueError,'retention-metadata'):Distribution(rows,setting('score_mixture'),'balanced_available')


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class NativeSamplingTests(unittest.TestCase):
    def prepared(self):
        from test_declarative_success_arrivals import ArrivalTests
        helper=ArrivalTests();config,chunks=helper.chunks();config['success']['max_sequences']=4096
        learner=Learner(17,config,'sampling-test');evidence=fixture.MemoryEvidence()
        for chunk in chunks:helper.update(learner,chunk,evidence)
        saved=deepcopy(learner.checkpoint([]))
        for i,entry in enumerate(saved['buffer']['entries'].values()):
            entry['retention_features_v1']=extract([evidence.sample(l['reference']) for l in entry['labels']],entry['id'])
            entry['sampling_priority_v1']=dict(value=1.+i,observed_update=1)
        return helper,config,chunks,evidence,saved

    def test_warm_priority_refresh_and_restored_next_update_agree_for_all_settings(self):
        helper,config,chunks,evidence,saved=self.prepared()
        for kind in ('uniform','score_mixture','positive_advantage'):
            for count in (8,16,32):
                selected=deepcopy(config);selected['success']['sampling']=setting(kind,count)
                warm=Learner(17,selected,'sampling-test',saved)
                warm.buffer.eligible(evidence,3,selected['strata'])
                identities=sorted(warm.buffer.state['entries'])
                observe_priorities(warm.buffer.state['entries'],[dict(id=identities[0],mean_positive_advantage=.001)],2)
                restored=Learner(17,selected,'sampling-test',deepcopy(warm.checkpoint([])))
                left=Distribution(warm.buffer.eligible(evidence,3,selected['strata']),setting(kind,count),'balanced_available')
                right=Distribution(restored.buffer.eligible(evidence,3,selected['strata']),setting(kind,count),'balanced_available')
                self.assertEqual(left.summary(),right.summary())
                records=[]
                for learner in (warm,restored):
                    source=fixture.MemoryEvidence();source.chunks=deepcopy(evidence.chunks)
                    _,record=helper.update(learner,chunks[0],source)
                    records.append(dict(checkpoint=digest(learner.checkpoint([])),selected=record['ppo']['success']['selected_ids']))
                self.assertEqual(*records)

    def test_explicit_uniform_default_equivalence_and_once_per_update_kl_tail(self):
        helper,config,chunks,evidence,saved=self.prepared();results=[]
        for explicit in (False,True):
            selected=deepcopy(config)
            if explicit:selected['success']['sampling']=setting()
            learner=Learner(17,selected,'sampling-test',saved);source=fixture.MemoryEvidence();source.chunks=deepcopy(evidence.chunks)
            with patch('declarative_replay_sampling.Distribution',wraps=Distribution) as prepared:
                _,record=helper.update(learner,chunks[0],source)
                self.assertEqual(prepared.call_count,int(explicit))
            success=record['ppo']['success'];results.append(dict(checkpoint=digest(learner.checkpoint([])),
                ids=success['selected_ids'],rng=success['rng_sha256'],order=success['order_rng_sha256'],loss=success['weighted_loss']))
        self.assertEqual(*results)
        selected=deepcopy(config);selected['ppo']['target_kl']=0.;selected['success']['sampling']=setting('positive_advantage',32)
        learner=Learner(17,selected,'sampling-test',saved)
        _,record=helper.update(learner,chunks[0],evidence)
        self.assertEqual(record['ppo']['actor_epochs'],1);self.assertEqual(record['ppo']['critic_epochs'],4)
        self.assertEqual(len(record['ppo']['success']['selected_ids']),1)
        self.assertTrue(record['ppo']['critic_tail_preserves_actor'])

    def test_bootstrap_restore_rejection_and_uniform_needs_no_features(self):
        helper,config,chunks,evidence,saved=self.prepared()
        selected=deepcopy(config);selected['success']['sampling']=setting('positive_advantage')
        learner=Learner(17,selected,'sampling-test');source=fixture.MemoryEvidence()
        with patch('declarative_success_features.extract',side_effect=AssertionError('priority does not use geometry')):
            for chunk in chunks:helper.update(learner,chunk,source)
        self.assertTrue(all(e['sampling_priority_v1']==dict(value=1.,observed_update=None) for e in learner.buffer.state['entries'].values()))
        for e in learner.buffer.state['entries'].values():e['sampling_priority_v1']=dict(value=7.,observed_update=2)
        with patch('declarative_replay_sampling.PriorityBootstrap',side_effect=AssertionError('duplicates do not bootstrap')):
            for n,chunk in enumerate(chunks,3):
                source.begin(n,chunk);learner.buffer.ingest(chunk,n,source,selected['success'],1.,selected['strata'])
        self.assertTrue(all(e['sampling_priority_v1']==dict(value=7.,observed_update=2) for e in learner.buffer.state['entries'].values()))
        bad=deepcopy(saved);del next(iter(bad['buffer']['entries'].values()))['sampling_priority_v1']
        with self.assertRaisesRegex(ValueError,'priority-metadata'):Learner(17,selected,'sampling-test',bad)
        selected['success']['sampling']=setting('uniform',32)
        for e in bad['buffer']['entries'].values():e.pop('retention_features_v1',None)
        Learner(17,selected,'sampling-test',bad)

    def test_priority_refresh_adds_no_forward_calls(self):
        helper,config,chunks,evidence,saved=self.prepared();counts=[]
        config['ppo']['target_kl']=1e9
        for kind in ('uniform','positive_advantage'):
            selected=deepcopy(config);selected['success']['sampling']=setting(kind)
            learner=Learner(17,selected,'sampling-test',saved)
            source=fixture.MemoryEvidence();source.chunks=deepcopy(evidence.chunks)
            with patch.object(policy,'predict',wraps=policy.predict) as predict:
                _,record=helper.update(learner,chunks[0],source);counts.append(predict.call_count)
            self.assertEqual(record['ppo']['actor_epochs'],4)
            observed=record['ppo']['success']['sequence_utility']
            self.assertEqual(all('mean_positive_advantage' in r for r in observed),kind=='positive_advantage')
        self.assertEqual(*counts)


if __name__=='__main__':unittest.main()
