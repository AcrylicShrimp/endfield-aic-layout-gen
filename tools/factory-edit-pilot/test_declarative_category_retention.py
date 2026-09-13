"""Category eligibility, indexed oracle agreement and real terminal admission."""
from collections import Counter
from copy import deepcopy
import math
import os
import random
import unittest
from unittest.mock import patch

from blueprint_ppo_update import digest
from declarative_learning_buffer import Buffer
from declarative_learning_runtime import Learner
from declarative_replay_retention import ReplacementIndex, stratum, validate, victim
from declarative_success_features import validate_metadata
from test_declarative_replay_retention import CONFIG, STRATA, entry
import test_declarative_learning as baseline

CATEGORY = dict(CONFIG, schema='declarative-replay-category-v2', category_fraction=.1,
    protection_updates=64, immature_order='oldest')


def featured(identity, place=False, **kwargs):
    return dict(entry(identity, **kwargs), retention_features_v1=dict(has_place=place, gain=.1, exterior_release=0.))


def oracle(entries, incoming, ordinal, config):
    counts = Counter(stratum(e, STRATA) for e in entries.values())
    name = stratum(incoming, STRATA)
    chosen = max(counts, key=lambda s:(counts[s]+(s==name), s==name, s))
    rows = [e for e in entries.values() if stratum(e, STRATA)==chosen]
    place = sum(e['retention_features_v1']['has_place'] for e in rows)
    if place <= math.ceil(config['category_fraction']*len(rows)):
        eligible = [e for e in rows if not e['retention_features_v1']['has_place'] or
            ordinal-e['admitted_update']>=config['protection_updates']]
        rows = eligible or rows
    mature = [e for e in rows if e.get('utility') and e['utility']['draws']>=config['minimum_draws'] and
        e['utility']['updates']>=config['minimum_updates']]
    def key(e):
        if mature:
            u=e['utility']
            return (u['ema']/(1+max(0,ordinal-u['last_update'])/config['stale_updates']),e['admitted_update'],e['id'])
        f=e['retention_features_v1'];score=f['gain']
        if config['immature_order']=='area_capped_exterior':score+=min(f['exterior_release'],score)
        return (e['admitted_update'],e['id']) if config['immature_order']=='oldest' else (e['admitted_update'],score,e['id'])
    return min(mature or rows,key=key)['id']


class CategoryTests(unittest.TestCase):
    def test_scarce_mature_place_is_protected_until_grace_expires(self):
        rows={e['id']:e for e in (featured('place', True), featured('ordinary',utility=None))}
        self.assertEqual(victim(rows,entry('incoming'),64,CATEGORY,STRATA)[0],'ordinary')
        self.assertEqual(victim(rows,entry('incoming'),65,CATEGORY,STRATA)[0],'place')
        only={'place':rows['place']}
        removed,detail=victim(only,entry('incoming'),2,CATEGORY,STRATA)
        self.assertEqual(removed,'place');self.assertTrue(detail['capacity_override'])

    def test_zero_fraction_preserves_v1_decisions(self):
        rows={str(i):featured(str(i),i%3==0,ordinal=i%5,utility=None if i%2 else i/10) for i in range(100)}
        a,b=deepcopy(rows),deepcopy(rows)
        left=ReplacementIndex(a,10,CONFIG,STRATA)
        right=ReplacementIndex(b,10,dict(CATEGORY,category_fraction=0),STRATA)
        for _ in range(len(rows)):
            x,dx=left.choose(entry('incoming'));y,dy=right.choose(entry('incoming'))
            self.assertEqual(x,y);self.assertEqual(dx,{k:dy[k] for k in dx})
            del a[x];del b[y]

    def test_randomized_index_matches_oracle_across_admission_and_floor_crossings(self):
        rng=random.Random(513)
        for order in ('oldest','area','area_capped_exterior'):
            config=dict(CATEGORY,immature_order=order)
            rows={}
            def new(i):
                e=featured(str(i),rng.random()<.15,size=rng.choice(['small','large']),
                    ordinal=rng.randrange(1,100),utility=rng.choice([None,.1,1.,5.]))
                e['retention_features_v1'].update(gain=rng.uniform(.01,.9),exterior_release=rng.random())
                return e
            for i in range(200):rows[str(i)]=new(i)
            index=ReplacementIndex(rows,100,config,STRATA)
            for i in range(200,700):
                incoming=new(i)
                expected=oracle(rows,incoming,100,config)
                selected,_=index.choose(incoming)
                self.assertEqual(selected,expected)
                del rows[selected]
                if i%4:
                    rows[incoming['id']]=incoming;index.add(incoming)

    def test_invalid_configuration_and_metadata_fail_at_boundary(self):
        for change in (dict(category_fraction=.21),dict(category_fraction=float('nan')),
                       dict(protection_updates=True),dict(immature_order='unknown')):
            with self.assertRaisesRegex(ValueError,'category-config'):validate(dict(CATEGORY,**change))
        for value in (None,{},dict(has_place=1,gain=.2,exterior_release=0),
                      dict(has_place=True,gain=float('nan'),exterior_release=0)):
            with self.assertRaisesRegex(ValueError,'retention-metadata'):validate_metadata(value,'entry')


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class NativeCategoryTests(unittest.TestCase):
    def test_terminal_features_capacity_duplicate_restore_and_disabled_cost(self):
        native=baseline.NativeLearningTests();native.setUp();chunks=native.preparation_chunks()
        evidence=baseline.MemoryEvidence();buffer=Buffer()
        config=dict(native.config['success'],max_sequences=1,max_labels=3,retention=CATEGORY)
        for n,chunk in enumerate(chunks,1):
            evidence.begin(n,chunk);buffer.ingest(chunk,n,evidence,config,1.,native.config['strata'])
        identity=next(iter(buffer.state['entries']));original=deepcopy(buffer.state['entries'][identity])
        self.assertFalse(original['retention_features_v1']['has_place'])
        buffer.hydrate(evidence,native.config['strata'])
        for n,chunk in enumerate(chunks,3):
            evidence.begin(n,chunk);events=buffer.ingest(chunk,n,evidence,config,1.,native.config['strata'])
        self.assertEqual(buffer.state['entries'][identity]['admitted_update'],original['admitted_update'])
        self.assertEqual(buffer.state['entries'][identity]['occurrences'],2)
        self.assertEqual(events[-1]['kind'],'duplicate')
        old=buffer.state['entries'].pop(identity);old['id']='retired';buffer.state['entries']['retired']=old
        buffer.cache={'retired':buffer.cache.pop(identity)}
        for n,chunk in enumerate(chunks,5):
            evidence.begin(n,chunk);buffer.ingest(chunk,n,evidence,config,1.,native.config['strata'])
        self.assertNotIn('retired',buffer.cache);self.assertEqual(buffer.retained_chunks(),{5,6})
        restored=Buffer(buffer.snapshot())
        self.assertEqual(restored.eligible(evidence,7,native.config['strata']),buffer.eligible(evidence,7,native.config['strata']))
        learner=Learner(17,native.config,'test');learner.buffer=buffer
        saved=deepcopy(learner.checkpoint([]));selected=deepcopy(native.config);selected['success']=config
        self.assertEqual(Learner(17,selected,'test',saved).buffer.snapshot(),saved['buffer'])
        del saved['buffer']['entries'][identity]['retention_features_v1']
        with self.assertRaisesRegex(ValueError,'retention-metadata'):Learner(17,selected,'test',saved)
        disabled=Buffer();evidence=baseline.MemoryEvidence()
        with patch('declarative_success_features.extract',side_effect=AssertionError('unneeded extraction')):
            for n,chunk in enumerate(chunks,1):
                evidence.begin(n,chunk);disabled.ingest(chunk,n,evidence,native.config['success'],1.,native.config['strata'])
        self.assertNotIn('retention_features_v1',disabled.state['entries'][identity])

    def test_actual_rotated_place_success_uses_native_witness_area(self):
        native=baseline.NativeLearningTests();native.setUp()
        stream=baseline.Stream(native.runtime,0)
        action=dict(kind='place',instance='C1',x=26,y=16,rotation=180,ports=[
            dict(node=6,port='input-belt-2'),dict(node=11,port='input-belt-3'),dict(node=20,port='output-belt-1')])
        chunk=baseline.scripted_chunk(native.model,stream,[action]*4)
        evidence=baseline.MemoryEvidence();evidence.begin(1,chunk);buffer=Buffer()
        buffer.ingest(chunk,1,evidence,dict(native.config['success'],retention=CATEGORY),1.,native.config['strata'])
        feature=next(iter(buffer.state['entries'].values()))['retention_features_v1']
        self.assertTrue(feature['has_place'])
        self.assertAlmostEqual(feature['gain'],1-665/chunk['samples'][0]['observation']['best_metrics']['score'][0])

    def test_production_capacity_hydrates_each_source_once_and_releases_victim_cache(self):
        native=baseline.NativeLearningTests();native.setUp();chunks=native.preparation_chunks()
        evidence=baseline.MemoryEvidence();buffer=Buffer()
        config=dict(native.config['success'],max_sequences=4096,max_labels=262144,retention=CATEGORY)
        for n,chunk in enumerate(chunks,1):
            evidence.begin(n,chunk);buffer.ingest(chunk,n,evidence,config,1.,native.config['strata'])
        original=next(iter(buffer.state['entries'].values()))
        buffer.state['entries']={str(i):dict(deepcopy(original),id=str(i),labels=deepcopy(original['labels'][:1+i%3]))
            for i in range(4096)}
        buffer=Buffer(buffer.snapshot());visited=[];sample=evidence.sample
        def counted(ref):
            if not visited or visited[-1]!=ref[0]:visited.append(ref[0])
            return sample(ref)
        evidence.sample=counted
        buffer.hydrate(evidence,native.config['strata'])
        self.assertEqual(visited,[1,2]);self.assertEqual(len(buffer.cache),4096)
        for n,chunk in enumerate(chunks,3):
            evidence.begin(n,chunk);events=buffer.ingest(chunk,n,evidence,config,1.,native.config['strata'])
        removed=[e['id'] for e in events if e['kind']=='evicted']
        self.assertEqual(len(removed),1);self.assertNotIn(removed[0],buffer.cache)
        self.assertEqual(len(buffer.state['entries']),4096);self.assertLessEqual(buffer.label_count,262144)


if __name__=='__main__':unittest.main()
