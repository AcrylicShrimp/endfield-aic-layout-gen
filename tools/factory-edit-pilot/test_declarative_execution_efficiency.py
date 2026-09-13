"""Value-preserving transport/storage, sparse live replay and exact victim order."""
from copy import deepcopy
import gc
import gzip
import os
from pathlib import Path
import pickle
import random
import tempfile
import unittest

import numpy as np
import torch

from blueprint_ppo_store import load, reference, save, write
from blueprint_ppo_update import digest
from blueprint_success_buffer import Evidence
from declarative_learning_buffer import Buffer
from declarative_replay_retention import ReplacementIndex
from declarative_replay_storage import compact, references
from ppo_tensor_pickle import dumps
import test_declarative_learning as baseline
from test_declarative_replay_retention import CONFIG, STRATA, entry


class TensorStorageTests(unittest.TestCase):
    @unittest.skipUnless(os.sys.platform=='linux','requires anonymous Linux shared memory')
    def test_persistent_shared_inputs_do_not_hold_one_descriptor_per_message(self):
        from ppo_harness_transport import Reader,encode
        reader=Reader(1024*1024,owned_buffers=True);saved=[];before=len(list(Path('/proc/self/fd').iterdir()))
        for index in range(1100):
            frame,fd=encode({'input':torch.tensor([index],dtype=torch.int64)})
            saved.extend(reader.feed(frame,[fd]))
        reader.close()
        self.assertLessEqual(len(list(Path('/proc/self/fd').iterdir())),before+1)
        self.assertEqual([row['input'].item() for row in saved],list(range(1100)))

    def test_out_of_band_values_types_aliases_and_buffer_lifetime(self):
        tensor=torch.arange(24,dtype=torch.float32).reshape(4,6).T
        value={'tensor':tensor,'same':tensor,'tuple':(torch.tensor(True),np.arange(4))}
        buffers=[];packed=dumps(value,buffer_callback=buffers.append)
        restored=pickle.loads(packed,buffers=buffers)
        del value,tensor,packed;gc.collect()
        torch.testing.assert_close(restored['tensor'],torch.arange(24,dtype=torch.float32).reshape(4,6).T,rtol=0,atol=0)
        self.assertIs(restored['tensor'],restored['same'])
        self.assertIsInstance(restored['tuple'],tuple)
        self.assertIsInstance(restored['tuple'][1],np.ndarray)

    def test_standard_torch_archive_and_mixed_empty_shared_dtypes(self):
        base=np.arange(16,dtype=np.uint8)
        values=[torch.from_numpy(base[:0].view(dtype)) for dtype in (np.float32,np.float64,np.int64,np.uint8)]
        values += [torch.tensor(3.),torch.arange(8,dtype=torch.bfloat16),torch.ones(2,requires_grad=True)]
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'saved.pt.gz';save(path,values)
            with gzip.open(path,'rb') as f:restored=torch.load(f,map_location='cpu',weights_only=False)
            for a,b in zip(values,restored,strict=True):
                torch.testing.assert_close(a,b,rtol=0,atol=0)
                self.assertEqual(a.requires_grad,b.requires_grad)
            with gzip.open(Path(temporary)/'old.pt.gz','wb') as f:torch.save({'old':torch.arange(3)},f)
            self.assertEqual(load(Path(temporary)/'old.pt.gz')['old'].tolist(),[0,1,2])

    def test_heap_eviction_matches_full_search_through_ties_and_readmission(self):
        rng=random.Random(49)
        rows={str(i):entry(str(i),rng.choice(list(STRATA)),rng.randrange(1,20),
            rng.choice([None,0.,.1,1.,5.])) for i in range(100)}
        for row in rows.values():
            if row['utility']:row['utility'].update(draws=rng.randrange(1,15),updates=rng.randrange(1,5),last_update=rng.randrange(1,30))
        index=ReplacementIndex(rows,30,CONFIG,STRATA)
        for step in range(200):
            incoming=entry('new',rng.choice(list(STRATA)),30,None)
            counts={s:sum(r['input']['draw']['product']==s for r in rows.values())+(incoming['input']['draw']['product']==s) for s in STRATA}
            chosen=max({r['input']['draw']['product'] for r in rows.values()},key=lambda s:(counts[s],s==incoming['input']['draw']['product'],s))
            candidates=[r for r in rows.values() if r['input']['draw']['product']==chosen]
            mature=[r for r in candidates if r['utility'] and r['utility']['draws']>=8 and r['utility']['updates']>=2]
            if mature:
                expected=min(mature,key=lambda r:(r['utility']['ema']/(1+max(0,30-r['utility']['last_update'])/64),r['admitted_update'],r['id']))
            else:expected=min(candidates,key=lambda r:(r['admitted_update'],r['id']))
            victim,detail=index.choose(incoming)
            self.assertEqual(victim,expected['id']);self.assertEqual(detail['stratum'],chosen)
            del rows[victim]
            # Deliberately reuse the removed identity: lazy heaps must reject
            # its stale token even when the new row has the same id/update.
            incoming['id']=victim;rows[victim]=incoming;index.add(incoming)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class ReplayStorageTests(unittest.TestCase):
    def test_sparse_storage_preserves_unfinished_credit_and_terminal_replay(self):
        helper=baseline.NativeLearningTests();helper.setUp();chunks=helper.preparation_chunks()
        config=helper.config;expected=Buffer();memory=baseline.MemoryEvidence()
        for n,payload in enumerate(chunks,1):
            memory.begin(n,payload);expected.ingest(payload,n,memory,config['success'],1.,config['strata'])
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);job={'records':[],'replay_sources':{}};actual=Buffer();evidence=Evidence(root,job)
            for n,payload in enumerate(chunks,1):
                evidence.begin(n,payload);actual.ingest(payload,n,evidence,config['success'],1.,config['strata'])
                retained=compact(payload,references(actual.state).get(n,set()))
                self.assertNotIn('native_before',retained);self.assertNotIn('frontiers',retained)
                self.assertLessEqual(len(retained['samples']),len(payload['samples']))
                path=root/f'chunks/{n:04d}.pt.gz';save(path,retained);ref=reference(path,root)
                record=root/f'updates/{n:04d}.json';write(record,dict(update=n,chunk=ref))
                job['records'].append(reference(record,root));job['replay_sources'][str(n)]=ref
                actual=Buffer(actual.snapshot());evidence=Evidence(root,job)
            self.assertEqual(actual.snapshot(),expected.snapshot())
            self.assertEqual(digest(actual.eligible(evidence,3,config['strata'])),digest(expected.eligible(memory,3,config['strata'])))
            with self.assertRaisesRegex(ValueError,'source sample'):evidence.sample((1,100000))

class MigrationTests(unittest.TestCase):
    def test_input_location_migration_preserves_actual_files_and_parent_locator(self):
        from unittest.mock import patch
        from blueprint_ppo_store import read,sha
        from declarative_execution_migrate import relocate_inputs
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source=root/'old';output=root/'new';spec=root/'spec.json'
            write(spec,{'schema_version':1});write(root/'old-data.json',{'catalog':'unchanged'});write(root/'new-data.json',{'catalog':'unchanged'})
            old_catalogs={'facilities':str(root/'old-data.json')};new_catalogs={'facilities':str(root/'new-data.json')}
            write(source/'catalogs.json',old_catalogs);write(source/'manifest.json',{'seed_base':7,'spec_sha256':sha(spec)})
            write(source/'episode-00000001/input.json',{'request':'actual','original':'actual'})
            config={'training_input':{'spec':'spec.json','seed_base':7}}
            with patch('blueprint_ppo_store.ROOT',root),patch('random_factory_generation.resolve_spec',return_value={'catalogs':new_catalogs}):
                receipt=relocate_inputs(source,output,config)
                write(root/'new-data.json',{'catalog':'changed'})
                with self.assertRaisesRegex(ValueError,'differ in content'):relocate_inputs(source,root/'bad',config)
            self.assertEqual(read(source/'catalogs.json'),old_catalogs)
            self.assertEqual(read(output/'catalogs.json'),new_catalogs)
            self.assertEqual(sha(source/'episode-00000001/input.json'),sha(output/'episode-00000001/input.json'))
            self.assertEqual(receipt['linked_files'],3)

    def test_execution_settings_cannot_change_learning_inputs_or_remaining_budget(self):
        from declarative_execution_migrate import compatible
        from blueprint_ppo_store import ROOT,read
        c=read(ROOT/'tools/factory-edit-pilot/declarative-four-hour.json');changed=deepcopy(c)
        changed['execution_options']=dict(workers=16,behavior_device='cuda:0')
        changed['retention']['training_traces']=[];compatible(changed,c)
        for group,key,value in [('ppo','learning_rate',.1),('duration','seconds',28800),('training_input','seed_base',100)]:
            invalid=deepcopy(changed);invalid[group][key]=value
            with self.assertRaisesRegex(ValueError,'changed'):compatible(invalid,c)

    def test_compact_replay_retention_keeps_best_and_live_recovery_sources(self):
        from types import SimpleNamespace
        from declarative_learning_runtime import account_write,retain
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);job=dict(best_evaluations={'large':{'update':2}},checkpoint_chunks={'2':[1,2],'4':[3,4]},replay_sources={})
            for folder in ('replay','checkpoints'):
                for n in range(1,7):
                    p=root/folder/f'{n:04d}.pt.gz';p.parent.mkdir(exist_ok=True);p.write_bytes(b'fixture');account_write(job,p)
                    if folder=='replay':job['replay_sources'][str(n)]=reference(p,root)
            learner=SimpleNamespace(ordinal=5,buffer=Buffer(),config={'retention':{'checkpoints':[],'recovery_updates':2,'training_traces':[]}})
            retain(learner,root,job)
            self.assertEqual(set(job['replay_sources']),{'1','2','3','4','5'})
            self.assertTrue((root/'checkpoints/0002.pt.gz').exists())
            self.assertFalse((root/'replay/0006.pt.gz').exists())


if __name__=='__main__':unittest.main()
