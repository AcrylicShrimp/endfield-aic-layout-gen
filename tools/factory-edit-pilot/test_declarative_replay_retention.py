"""Replacement, restored eligibility and zero-extra-forward utility measurement."""
from copy import deepcopy
import os
from types import SimpleNamespace
import unittest

import torch

from blueprint_execution_update import summarize_imitation, update_packed
from blueprint_ppo_update import digest
from declarative_learning_buffer import Buffer
from declarative_learning_runtime import Learner
from declarative_replay_retention import observe, victim
import test_declarative_learning as baseline
import declarative_learning_policy as policy

CONFIG=dict(schema='declarative-replay-utility-v1',ema_alpha=.5,minimum_draws=8,minimum_updates=2,stale_updates=64)
STRATA=dict(small=['small'],large=['large'])


def entry(identity, size='small', ordinal=1, utility=0.):
    return dict(id=identity,input={'draw':{'product':size}},admitted_update=ordinal,labels=[{}],
        utility=None if utility is None else dict(ema=utility,draws=8,updates=2,last_update=2))


class RetentionTests(unittest.TestCase):
    def test_contribution_is_stage_balanced_and_uses_existing_measurements(self):
        # One two-stage draw: two labels of stage A and one of stage B.
        values=torch.tensor([[1.,0.,1.,2.],[1.,0.,1.,4.],[1.,0.,1.,10.]])
        result=summarize_imitation([values],[('own','small','a'),('own','small','a'),('own','small','b')],
            [('id',.25,True),('id',.25,False),('id',.5,False)])
        self.assertEqual(result['sequence_utility'],[dict(id='id',draws=1,mean_weighted_nll=6.5)])

    def test_rare_stratum_and_unobserved_experience_are_protected(self):
        entries={e['id']:e for e in (entry('large','large'),entry('low',utility=.01),
            entry('high',utility=10.),entry('new',ordinal=9,utility=None))}
        removed,detail=victim(entries,entry('incoming','large'),10,CONFIG,STRATA)
        self.assertEqual(removed,'low');self.assertEqual(detail['reason'],'lowest_observed_contribution')
        self.assertEqual(detail['stratum'],'small')

    def test_no_observation_fallback_and_staleness_are_explicit_and_deterministic(self):
        rows={e['id']:e for e in (entry('b',ordinal=1,utility=None),entry('a',ordinal=1,utility=None),entry('c',ordinal=2,utility=None))}
        removed,detail=victim(rows,entry('incoming'),10,CONFIG,STRATA)
        self.assertEqual(removed,'a');self.assertEqual(detail['reason'],'insufficient_observations_oldest')
        rows={'old':entry('old',utility=2.),'recent':entry('recent',utility=1.)}
        rows['recent']['utility']['last_update']=200
        self.assertEqual(victim(rows,entry('incoming'),200,CONFIG,STRATA)[0],'old')

    def test_observed_utility_survives_restore_and_rejects_duplicate_update(self):
        state=Buffer().snapshot();state['entries']={'a':entry('a',utility=None)}
        observe(state['entries'],[dict(id='a',draws=4,mean_weighted_nll=4.)],3,CONFIG)
        restored=Buffer(state)
        observe(restored.state['entries'],[dict(id='a',draws=5,mean_weighted_nll=0.)],4,CONFIG)
        self.assertEqual(restored.state['entries']['a']['utility'],dict(ema=2.,draws=9,updates=2,last_update=4))
        with self.assertRaisesRegex(ValueError,'repeated update'):
            observe(restored.state['entries'],[dict(id='a',draws=1,mean_weighted_nll=1.)],4,CONFIG)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class NativeRetentionTests(unittest.TestCase):
    def fixture(self):
        native=baseline.NativeLearningTests();native.setUp()
        return native,native.preparation_chunks()

    def test_full_buffer_replaces_at_admission_and_releases_cache_and_source_references(self):
        native,chunks=self.fixture();evidence=baseline.MemoryEvidence();buffer=Buffer()
        config=dict(native.config['success'],max_sequences=1,retention=CONFIG)
        for n,chunk in enumerate(chunks,1):
            evidence.begin(n,chunk);buffer.ingest(chunk,n,evidence,config,1.,native.config['strata'])
        old_id=next(iter(buffer.state['entries']));old=buffer.state['entries'].pop(old_id)
        old.update(id='retired',utility=dict(ema=0.,draws=8,updates=2,last_update=2))
        buffer.state['entries']['retired']=old;buffer.state['selected_uses']={'retired':5}
        buffer.eligible(evidence,3,native.config['strata'])
        for n,chunk in enumerate(chunks,3):
            evidence.begin(n,chunk);events=buffer.ingest(chunk,n,evidence,config,1.,native.config['strata'])
        self.assertEqual(len(buffer.state['entries']),1)
        self.assertNotIn('retired',buffer.cache)
        self.assertEqual(buffer.retained_chunks(),{3,4})
        self.assertEqual(buffer.state['selected_uses_total'],5)
        self.assertEqual(buffer.state['evictions'],1)
        self.assertEqual(sum(e['kind']=='evicted' for e in events),1)
        self.assertEqual(Buffer(buffer.snapshot()).label_count,buffer.label_count)

    def test_observation_changes_neither_optimizer_transition_nor_forward_count(self):
        native,chunks=self.fixture();config=baseline.diagnostic_config();learner=Learner(17,config,'fixture');evidence=baseline.MemoryEvidence()
        for chunk in chunks:
            chunk=deepcopy(chunk)
            with torch.no_grad():lp,ent,values=policy.predict(learner.model,policy.prepare_rows(chunk['samples']))
            for row,l,e,v in zip(chunk['samples'],lp,ent,values,strict=True):row.update(old_logp=float(l),old_value=float(v),entropy=float(e))
            chunk['model_before']=digest(learner.model.state_dict());learner.update(chunk,evidence)
        saved=deepcopy(learner.checkpoint([]));sequences=learner.buffer.eligible(evidence,3,config['strata'])
        results=[];calls=[]
        for enabled in (False,True):
            restored=Learner(17,config,'fixture',saved);counter=[0]
            def predict(*args):counter[0]+=1;return policy.predict(*args)
            likelihood=SimpleNamespace(prepare_rows=policy.prepare_rows,device_rows=policy.device_rows,predict=predict)
            r=update_packed(restored.model,restored.actor,restored.critic,restored.shuffle,chunks[1]['samples'],chunks[1]['frontiers'],
                config['ppo'],sequences,.05,'balanced_available',17,3,3,'cpu',likelihood=likelihood,measure_replay_utility=enabled)
            results.append({k:r[k] for k in ('model_after','actor_optimizer','critic_optimizer','shuffle_rng')});calls.append(counter[0])
            if enabled:self.assertGreater(sum(x['draws'] for x in r['success']['sequence_utility']),0)
        self.assertEqual(*results);self.assertEqual(*calls)


if __name__=='__main__':unittest.main()
