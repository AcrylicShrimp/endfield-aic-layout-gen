"""Registered pending-slot amendment and preservation of original evidence."""
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path[:0]=[str(Path(__file__).resolve().parents[1]),str(Path(__file__).resolve().parent)]
from blueprint_ppo_store import write,save,load,reference,sha
from blueprint_ppo_update import digest
from declarative_quality_continuation import amend_snapshot,prepare
from declarative_quality_cold import config_for,FIELDS
from declarative_quality_storage import Budget


def pending(root):
    slots=[dict(schema='declarative-best-starts-v1',wave=44,stream=i,depth=0,requested_depth=0) for i in range(32)]
    layout={'fixture':1};path=root/'input.json';write(path,dict(original=layout))
    slots[5].update(depth=1,requested_depth=1,best_sha256=digest(layout),
        parent=dict(seed=71,wave=43,stream=2,depth=0),input=dict(directory=str(root),**reference(path,root)))
    plan=dict(seed=71,wave=44,slots=slots,requested={'0':31,'1':1},actual={'0':31,'1':1},diagnostics=[],
        complete_parents={'0':32},improving_parents={'0':5})
    saved=dict(update=86,protocol_sha256='old',streams=[dict(index=i,episode=43,native=None,next_start=deepcopy(s),
        policy_actions=64,metadata={'unchanged':i}) for i,s in enumerate(slots)],
        **{k:{'unchanged':k} for k in FIELDS})
    return saved,plan,digest(layout)


class ContinuationTests(unittest.TestCase):
    def test_only_registered_pending_slot_changes_and_live_state_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp,patch('random_factory_generation.geometry_identity',side_effect=digest):
            saved,plan,reserved=pending(Path(tmp));before=deepcopy(saved)
            amended,changed,indices=amend_snapshot(saved,plan,{reserved})
            self.assertEqual(indices,[5]);self.assertEqual(saved,before)
            self.assertEqual(amended['streams'][5]['next_start']['depth'],0)
            for key in FIELDS:self.assertEqual(amended[key],saved[key])
            amended['streams'][5]['next_start']=saved['streams'][5]['next_start'];self.assertEqual(amended,saved)
            bad=deepcopy(saved);bad['streams'][0]['native']={}
            with self.assertRaisesRegex(ValueError,'live episode'):amend_snapshot(bad,plan,{reserved})
            with self.assertRaisesRegex(ValueError,'registered pending collision'):amend_snapshot(saved,plan,set())
            bad=deepcopy(plan);bad['slots'][0]['stream']=9
            with self.assertRaisesRegex(ValueError,'pending plan'):amend_snapshot(saved,bad,{reserved})

    def test_atomic_amendment_and_future_writes_leave_original_files_unchanged(self):
        for fail in (False,True):
            with self.subTest(fail=fail),tempfile.TemporaryDirectory() as tmp,patch('random_factory_generation.geometry_identity',side_effect=digest):
                root=Path(tmp);old=root/'old';owner=old/'seed-71';owner.mkdir(parents=True)
                saved,plan,geometry=pending(owner);plan_path=owner/'curriculum/plan.json';write(plan_path,plan)
                checkpoint=owner/'checkpoints/0086.pt.gz';save(checkpoint,saved)
                console=old/'console.log';console.write_text('original console\n')
                output=root/'new';config=config_for(71,'baseline-8','development',output,600)
                old_config=deepcopy(config);old_config['training_input']['directory']=str(old/'training-inputs')
                source=dict(files_sha256={},native_sha256='native',code_sha256='code')
                paths=[owner/'input.json',plan_path,checkpoint]
                job=dict(status='failed',seconds=123,update=86,curriculum_plans={'44':reference(plan_path,owner)},
                    retained_files={str(p):p.stat().st_size for p in paths},latest_checkpoint=reference(checkpoint,owner),
                    wandb_id='old-tracking',diagnostic={'failed':True})
                value=dict(study={'panels':{'development':{}}},owner=owner,job=job,saved=saved,
                    launch=dict(config=old_config,source=source),proof=dict(origin={'old':True},preserved={'initial':'proof'}),bindings={})
                budget=Budget(root/'budget.json',10_000_000,prior_bytes=sum(p.stat().st_size for p in paths))
                before={p:sha(p) for p in [*paths,console]}
                def relocate(shared,destination,config):
                    if fail:raise OSError('injected relocation failure')
                    destination.mkdir()
                with patch('declarative_quality_continuation.reservation',return_value={'geometry_sha256':[geometry]}),\
                     patch('declarative_quality_continuation.config_for',return_value=config),\
                     patch('declarative_quality_continuation.source_binding',return_value=source),\
                     patch('declarative_quality_continuation.cpu_allocation',return_value={}),\
                     patch('declarative_quality_continuation.require_readiness'),\
                     patch('declarative_quality_continuation.bound',return_value={'path':'fixture','sha256':'fixture'}),\
                     patch('declarative_execution_migrate.relocate_inputs',side_effect=relocate):
                    if fail:
                        with self.assertRaisesRegex(OSError,'injected'):prepare(value,output,root/'shared','native',600,'ready',budget)
                        self.assertFalse(output.exists());self.assertFalse(output.with_name('new.staging').exists())
                    else:
                        proof=prepare(value,output,root/'shared','native',600,'ready',budget)
                        self.assertEqual(proof['continuation']['replaced_slots'],[5])
                        new_job=__import__('json').loads((output/'seed-71/state.json').read_text())
                        self.assertIsNone(new_job['wandb_id']);self.assertNotIn('diagnostic',new_job)
                        self.assertTrue(all(Path(p).is_relative_to(output.resolve()/'seed-71') for p in new_job['retained_files']))
                        self.assertFalse((output/'console.log').exists())
                        new_checkpoint=output/'seed-71/checkpoints/0086.pt.gz'
                        self.assertNotEqual(new_checkpoint.stat().st_ino,checkpoint.stat().st_ino)
                        amended=load(new_checkpoint);self.assertEqual(amended['streams'][5]['next_start']['depth'],0)
                        for key in FIELDS:self.assertEqual(amended[key],saved[key])
                        # Normal atomic checkpoint publication and a new console
                        # cannot truncate the separately retained original files.
                        save(new_checkpoint,amended);(output/'console.log').write_text('new console')
                        budget.scan(output);budget.save();self.assertGreater(budget.value['bytes'],budget.value['prior_bytes'])
                self.assertEqual(before,{p:sha(p) for p in before})


if __name__=='__main__':unittest.main()
