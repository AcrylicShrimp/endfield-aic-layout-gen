"""Duration boundaries, checkpoint selection and exact v2 import contracts."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

import torch

from blueprint_ppo_store import ROOT, load, read, reference, save, sha, write
from blueprint_ppo_update import digest
from declarative_duration_control import evaluation_due, evaluation_updates, select_best, stop_reason
from declarative_duration_import import initialize, parent
from declarative_learning_launch import validate
from declarative_learning_runtime import CHECKPOINT, Learner, account_write, retain
from types import SimpleNamespace
from declarative_learning_buffer import Buffer


def config():return read(ROOT/'tools/factory-edit-pilot/declarative-four-hour.json')


class DurationTests(unittest.TestCase):
    def test_variable_stream_counts_keep_wave_and_execution_contracts(self):
        for count in (32,64,128,256):
            c=config();c['training']['streams']=count;c['execution']='logical-stream-pool-v1'
            validate(c)
            self.assertIsNone(stop_reason(c,33,15000,True))
            self.assertEqual(stop_reason(c,34,15000,True),'requested_stop')
        for count in (0,-1,True,32.5):
            c=config();c['training']['streams']=count
            with self.assertRaises(ValueError):validate(c)

    def test_registered_config_and_complete_wave_duration_or_requested_stop(self):
        c=config();validate(c)
        self.assertIsNone(stop_reason(c,33,15000,True))
        self.assertIsNone(stop_reason(c,32,15000))
        self.assertEqual(stop_reason(c,34,14400),'duration')
        self.assertEqual(stop_reason(c,34,100,True),'requested_stop')
        self.assertIsNone(stop_reason(c,34,100))
        bad=deepcopy(c);bad['limits']['arm_seconds']=14400
        with self.assertRaisesRegex(ValueError,'finalization'):validate(bad)

    def test_evaluation_cadence_and_final_endpoint_are_not_update_ceiling(self):
        c=config()
        self.assertEqual(evaluation_updates(c,102),[32,64,96,102])
        self.assertEqual(evaluation_updates(c,102,[34,66]),[32,34,64,66,96,102])
        with self.assertRaisesRegex(ValueError,'pause evaluation'):evaluation_updates(c,102,[35])
        self.assertFalse(evaluation_due(c,34));self.assertTrue(evaluation_due(c,34,True))
        self.assertTrue(evaluation_due(c,64));self.assertFalse(evaluation_due(c,65))

    def test_best_models_use_fixed_cohort_area_before_tiles_and_turns(self):
        c=config();job=dict(latest_checkpoint={'path':'initial.pt','sha256':'a'})
        def row(product,area,tiles,turns):return dict(product=product,factory_validated=True,total_area_reduction_percent=area,best_score=[100,tiles,turns])
        first=dict(update=32,rows=[row('food',10,30,20),row('watered-grass',20,30,20)])
        select_best(job,c,first,{'path':'first.json'})
        job['latest_checkpoint']={'path':'later.pt','sha256':'b'}
        second=dict(update=64,rows=[row('food',9,0,0),row('watered-grass',22,40,30)])
        select_best(job,c,second,{'path':'second.json'})
        self.assertEqual(job['best_evaluations']['large']['update'],32)
        self.assertEqual(job['best_evaluations']['small']['update'],64)
        self.assertEqual(job['best_evaluations']['all']['update'],64)

    def test_evicted_active_replay_does_not_remove_best_checkpoint_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);job=dict(best_evaluations={'large':{'update':2}},checkpoint_chunks={'2':[1,2]})
            for folder in ('chunks','checkpoints'):
                for n in range(1,6):
                    p=root/folder/f'{n:04d}.pt.gz';p.parent.mkdir(exist_ok=True);p.write_bytes(b'fixture');account_write(job,p)
            learner=SimpleNamespace(ordinal=5,buffer=Buffer(),config={'retention':{'checkpoints':[],'recovery_updates':2}})
            retain(learner,root,job)
            self.assertTrue((root/'chunks/0001.pt.gz').exists())
            self.assertTrue((root/'checkpoints/0002.pt.gz').exists())
            self.assertFalse((root/'checkpoints/0003.pt.gz').exists())

    def source(self, root):
        c=config();old=deepcopy(c);old.pop('duration');old['success'].pop('retention')
        old['training'].update(first_update=0,updates=32);old['evaluation'].update(at=[0,16,32]);old['evaluation'].pop('interval_updates')
        write(root/'launch.json',dict(config=old));write(root/'state.json',dict(status='complete'))
        owner=root/'seed-29';owner.mkdir()
        slots=[dict(stream=i,wave=17,depth=0) for i in range(32)]
        streams=[dict(index=i,episode=16,native=None,next_start=s) for i,s in enumerate(slots)]
        model={'fixture':torch.tensor([1.,2.])}
        saved=dict(schema=CHECKPOINT,seed=29,update=32,protocol_sha256=sha(root/'launch.json'),model=model,model_sha256=digest(model),
            actor_optimizer={'step':torch.tensor(2.)},critic_optimizer={'step':torch.tensor(3.)},sampler_rng=torch.tensor([4],dtype=torch.uint8),
            shuffle_rng=torch.tensor([5],dtype=torch.uint8),streams=streams,buffer={'active':{},'entries':{'a':{'episode_chunks':[31,32]}}})
        save(owner/'checkpoints/0032.pt.gz',saved)
        records=[]
        for n in range(1,33):
            chunk=owner/f'chunks/{n:04d}.pt.gz'
            if n in (31,32):chunk.parent.mkdir(exist_ok=True);chunk.write_bytes(b'fixture')
            ref=reference(chunk,owner) if chunk.exists() else {'path':str(chunk.relative_to(owner)),'sha256':'not-retained'}
            record=owner/f'updates/{n:04d}.json';write(record,dict(update=n,chunk=ref));records.append(reference(record,owner))
        plan=owner/'curriculum/next.json';write(plan,dict(slots=slots))
        evaluation=owner/'evaluation/0032/report.json';write(evaluation,dict(update=32,rows=[]))
        job=dict(status='complete',update=32,latest_checkpoint=reference(owner/'checkpoints/0032.pt.gz',owner),records=records,
            inputs={str(i):{} for i in range(512)},curriculum_plans={'17':reference(plan,owner)},evaluation=[dict(update=32,**reference(evaluation,owner))])
        write(owner/'state.json',job)
        return c,saved

    def test_v2_import_preserves_all_learning_and_selected_next_start_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source=root/'parent';c,saved=self.source(source)
            value=parent(source,c);output=root/'child';write(output/'launch.json',dict(config=c))
            receipt=initialize(value,dict(config=c),output)
            actual=load(output/'seed-29'/receipt['new_checkpoint']['path'])
            for key,expected in receipt['preserved_sha256'].items():self.assertEqual(expected,digest(saved[key]));self.assertEqual(digest(actual[key]),expected)
            self.assertEqual(actual['protocol_sha256'],sha(output/'launch.json'))
            self.assertEqual(load(source/'seed-29/checkpoints/0032.pt.gz')['protocol_sha256'],sha(source/'launch.json'))
            self.assertTrue((output/'seed-29/chunks/0031.pt.gz').exists())
            self.assertEqual(read(output/'seed-29/state.json')['random'],[])

    def test_parent_rejects_changed_generator_or_missing_next_wave(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);c,saved=self.source(root)
            changed=deepcopy(c);changed['training_input']['seed_base']+=1
            with self.assertRaisesRegex(ValueError,'input contract'):parent(root,changed)
            value=parent(root,c);path=root/'seed-29/curriculum/next.json';path.unlink()
            output=root/'child';write(output/'launch.json',dict(config=c))
            with self.assertRaises((ValueError,FileNotFoundError)):initialize(value,dict(config=c),output)


if __name__=='__main__':unittest.main()
