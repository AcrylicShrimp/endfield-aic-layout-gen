"""Chronology, arm-local evidence, frozen utility and registered decision gates."""
from collections import Counter
from copy import deepcopy
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import torch
from blueprint_execution_update import imitation, prepare_minibatch
from declarative_retention_corpus import Corpus, frozen_batch
from declarative_retention_screen import aggregate, decide, horizon_counts, profile, profiles, run_arm
from test_declarative_replay_retention import CONFIG, STRATA
import declarative_learning_policy as policy


class ScreenTests(unittest.TestCase):
    def test_registered_matrix_and_geometry_gate(self):
        self.assertEqual(len(profiles(False)),19);self.assertEqual(len(profiles(True)),25)
        self.assertEqual(len({p['id'] for p in profiles(True)}),25)
        self.assertFalse(any(p['geometry'] for p in profiles(False)))

    def test_common_anchor_survives_eviction_and_readmission_with_censoring(self):
        cohort={'x':dict(anchor=5,stratum='small',place=True,deletion=False,episode=(0,1))}
        records={'x':dict(draws=0,positive_draws=0,retained=0)};cache={'x':dict(raw=1)}
        for n in range(5,39):
            present={'x'} if n in (5,6,35,36,37,38) else set()
            # Re-admission at 35 cannot contribute same-update draws; the draw
            # at 38 is outside the original horizon and cannot reset its clock.
            draws=Counter(x=2) if n in (6,36,37,38) else Counter()
            horizon_counts(cohort,n,draws,present,records,cache)
        self.assertEqual(records['x'],dict(draws=6,positive_draws=6,retained=4))
        # Another arm never admits it; denominator and anchor remain identical.
        other={'x':dict(draws=0,positive_draws=0,retained=0)}
        for n in range(6,38):horizon_counts(cohort,n,Counter(),set(),other,cache)
        summary=aggregate(cohort,other,{},cache)['small']
        self.assertEqual(summary['cohort_ids'],1);self.assertEqual(summary['at_least_once'],0)

    def test_references_outside_declared_complete_union_reject(self):
        corpus=Corpus.__new__(Corpus);corpus.sources={4:dict(coverage={0,2})}
        corpus.verify_reference((4,2))
        for ref in ((4,1),(5,0)):
            with self.assertRaisesRegex(ValueError,'retention-screen.source'):corpus.verify_reference(ref)
        corpus.checkpoints={4:{4:{2}},1:{1:{0}}}
        corpus.verify_checkpoints({4})
        with self.assertRaisesRegex(ValueError,'retention-screen.source'):corpus.verify_checkpoints({1,4})

    def test_actual_capacity_order_own_observations_and_identical_seed_trace(self):
        def entry(i,ordinal=1):
            return dict(id=f'{i:04d}',admitted_update=ordinal,uses=0,occurrences=1,utility=None,stream=i%2,episode=i,
                input=dict(draw=dict(product='large' if i%2 else 'small')),labels=[dict(reference=(ordinal,i),stage='delete_band')],
                retention_features_v1=dict(has_place=False,gain=.1,exterior_release=0.))
        initial=[entry(i) for i in range(4096)];incoming=entry(4096,3)
        all_entries={e['id']:dict(e,stratum='large' if e['stream'] else 'small',screen_geometry=e['id']) for e in initial+[incoming]}
        corpus=SimpleNamespace(initial=dict(entries=initial),entries=all_entries,config=dict(success=dict(retention=CONFIG,sampler='balanced_available')),
            strata=STRATA,manifest=dict(initial_update=2),cohort={},
            updates=[dict(update=3,sil_calls=2,attempts=[dict(entry=incoming),dict(entry=deepcopy(incoming))]),dict(update=4,sil_calls=2,attempts=[])],
            storage=[dict(mandatory={1},mandatory_bytes=100,original_baseline_published_bytes=100)]*2,price=lambda chunks:100*len(chunks))
        cache={i:dict(raw=float(int(i)%3),positive=1.) for i in all_entries}
        p=profile('v1','uniform',8);a,raw=run_arm(corpus,cache,p,17,lambda:None);b,_=run_arm(corpus,cache,p,17,lambda:None)
        self.assertEqual(a['trace_sha256'],b['trace_sha256']);self.assertEqual(a['draws'],32)
        self.assertEqual(a['reasons']['admitted'],1);self.assertEqual(a['reasons']['duplicate'],1)
        self.assertEqual([r['sequences'] for r in raw['boundaries']],[4096,4096])
        self.assertEqual(a['restored_initial_uses'],0)
        c,_=run_arm(corpus,cache,profile('v1','uniform',32),17,lambda:None)
        self.assertEqual(c['draws'],128);self.assertNotEqual(c['trace_sha256'],a['trace_sha256'])
        self.assertTrue(all(e['utility'] is None for e in initial))

    def test_decision_zero_denominator_ceiling_and_no_changed_threshold(self):
        def row(p,seed,exposure=.5,geometries=10):
            return dict(profile=p,seed=seed,capacities_valid=True,source_valid=True,peak_source_bytes=100,
                metrics={s:dict(at_least_once=exposure,positive_at_least_once=exposure,deletion_geometries=geometries) for s in STRATA})
        base=[row(profile('v1','uniform',8),s) for s in (17,29,41)]
        arms=[row(profile('v1','uniform',16),s,.55) for s in (17,29,41)]
        self.assertEqual(decide(base+arms,dict(sufficient=True),True)['verdict'],'candidate')
        arms[0]['metrics']['small']['at_least_once']=.49
        self.assertEqual(decide(base+arms,dict(sufficient=True),True)['verdict'],'no_candidate')
        base[0]['metrics']['small']['deletion_geometries']=0
        self.assertEqual(decide(base+arms,dict(sufficient=True),True)['verdict'],'insufficient_evidence')
        base=[row(profile('v1','uniform',8),s,.98) for s in (17,29,41)]
        arms=[row(profile('v1','uniform',16),s,1.) for s in (17,29,41)]
        verdict=decide(base+arms,dict(sufficient=True),True)
        self.assertEqual(verdict['verdict'],'no_candidate');self.assertEqual(set(verdict['ceiling_strata']),set(STRATA))

    @unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires release native library')
    def test_frozen_cache_matches_actual_packed_imitation_and_stage_normalization(self):
        import test_declarative_learning as fixture
        native=fixture.NativeLearningTests();native.setUp();stream=fixture.Stream(native.runtime,0)
        chunk=fixture.scripted_chunk(native.model,stream,[dict(kind='delete_band',axis='x',at=24,count=1)]*2)
        rows=[dict(input=s['input'],decision=s['decision'],stage='delete_band',return_to_terminal=2.) for s in chunk['samples']]
        sequence=dict(id='sequence',origin='own',stratum='small',rows=rows)
        cache=frozen_batch(native.model,[sequence],'cpu')['sequence']
        prepared=prepare_minibatch([dict(rows[0],old_logp=0.)],[0],torch.zeros(1),torch.zeros(1),[sequence],policy,True)['sil']
        with torch.no_grad():loss,measurement=imitation(native.model,prepared,'cpu',policy)
        self.assertAlmostEqual(cache['raw'],float(loss),places=5)
        self.assertAlmostEqual(cache['positive'],float(measurement[:,0].mean()),places=5)


if __name__=='__main__':unittest.main()
