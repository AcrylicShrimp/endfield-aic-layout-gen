"""Cold origin/seed isolation, fixed physical bins and reclaimed ownership guards."""
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch
from contextlib import nullcontext
import unittest

sys.path[:0]=[str(Path(__file__).resolve().parents[1]),str(Path(__file__).resolve().parent)]
from declarative_quality_cold import config_for,zero_proof,prepare,protocol,require_readiness,FIELDS
from declarative_quality_branch import PROFILES,digest,read,write
from declarative_quality_observer import fixed_bins,Observer
from declarative_quality_analysis import allocation
from declarative_quality_storage import Budget,run_budgeted
from declarative_learning_runtime import Learner
from test_declarative_quality_branch import entry
from types import SimpleNamespace


class ColdTests(unittest.TestCase):
    def test_fresh_models_pair_within_seed_and_differ_between_seeds(self):
        hashes={}
        for seed in (71,113):
            for profile in PROFILES:
                config=config_for(seed,profile,'development' if seed==71 else 'confirmation','/tmp/cold',600)
                config['execution_options']['behavior_device']='cpu'
                learner=Learner(seed,config,'cold-test',device='cpu');saved=learner.checkpoint(None)
                job=dict(records=[],inputs={});proof=zero_proof(saved,job)
                if seed in hashes:self.assertEqual(hashes[seed],proof)
                hashes[seed]=proof
                for key in ('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','buffer'):
                    self.assertIn(key,proof)
                with self.assertRaisesRegex(ValueError,'nonempty'):
                    zero_proof(saved,dict(records=['old'],inputs={}))
                bad=deepcopy(saved);bad['buffer']['entries']['old']={}
                with self.assertRaisesRegex(ValueError,'nonempty'):zero_proof(bad,job)
        self.assertNotEqual(hashes[71]['model'],hashes[113]['model'])
        self.assertNotEqual(hashes[71]['sampler_rng'],hashes[113]['sampler_rng'])
    def test_unregistered_family_rejected(self):
        for seed,profile,stage,seconds in ((29,'baseline-8','development',600),(71,'unknown','development',600),
                (113,'baseline-8','development',600),(71,'baseline-8','confirmation',600),
                (71,'score-8','readiness',601),(71,'baseline-8','development',21601)):
            with self.subTest(seed=seed,profile=profile,stage=stage,seconds=seconds),self.assertRaisesRegex(ValueError,'family'):
                config_for(seed,profile,stage,'/tmp/cold',seconds)
    def test_protocol_and_atomic_publication_failure_leave_no_runnable_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);output=root/'arm';panel=root/'panel.json';bank=root/'bank';f=protocol()
            write(panel,dict(schema='declarative-learning-panel-v1',status='complete',cases=[
                dict(action_seed=f['readiness_action_seed']+i*100,generation_seed=f['readiness_generation_seed']+i*8) for i in range(32)]))
            write(bank/'manifest.json',{'seed_base':f['readiness_generation_seed']});write(bank/'catalogs.json',{})
            with patch('declarative_quality_cold.source_binding',return_value={'code_sha256':'code'}),\
                 patch('declarative_quality_cold.cpu_allocation',return_value={}),\
                 patch('declarative_quality_cold.bank_contract',return_value=({'seed_base':f['readiness_generation_seed']},{})),\
                 patch('declarative_execution_migrate.relocate_inputs',side_effect=OSError('injected publication failure')):
                with self.assertRaisesRegex(OSError,'publication failure'):
                    prepare(output,seed=71,profile='score-8',stage='readiness',panel=panel,library='fixture',shared=bank,seconds=600)
            self.assertFalse(output.exists());self.assertFalse(output.with_name('arm.staging').exists())
            write(bank/'manifest.json',{'seed_base':613900000})
            with patch('declarative_quality_cold.source_binding',return_value={'code_sha256':'code'}),\
                 patch('declarative_quality_cold.cpu_allocation',return_value={}),\
                 patch('declarative_quality_cold.bank_contract',return_value=({'seed_base':f['readiness_generation_seed']},{})):
                with self.assertRaisesRegex(ValueError,'old banks'):
                    prepare(output,seed=71,profile='score-8',stage='readiness',panel=panel,library='fixture',shared=bank,seconds=600)
            self.assertFalse(output.exists())
    def test_warm_receipt_cannot_authorize_cold_family(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'receipt.json';write(path,dict(schema='declarative-quality-readiness-v1',status='passed'))
            with self.assertRaisesRegex(ValueError,'cold family'):require_readiness(path,{}, {})
    def test_fixed_bins_label_native_gain_and_report_absent_groups(self):
        partitions=fixed_bins({'small':['fixture-diagnostic'],'large':['other']})
        learner=SimpleNamespace(config=dict(strata={'small':['fixture-diagnostic'],'large':['other']},success={'coefficient':.05}),
            buffer=SimpleNamespace(state={'entries':{}},admission_observer=None))
        observer=Observer(learner,partitions)
        self.assertEqual(observer.key(entry('a',.03)),('small/deletion','gain_3_to_10pct'))
        observer.begin();record=dict(update=1,events=[],ppo={'success':{'selected_ids':[]}})
        observed=observer.finish(record);result=allocation([{'quality_allocation':observed}])
        self.assertEqual(len(result['groups']),24)
        self.assertTrue(all(r['draws']==r['terminal_attempts']==0 and r['mean_native_gain_per_draw'] is None for r in result['groups']))
    def test_removal_hooks_cover_runtime_and_evaluation_and_failed_unlink(self):
        import declarative_learning_runtime as runtime
        import declarative_learning_run as runner
        import os
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);branch=root/'branch';write(branch/'launch.json',{'config':{'training_input':{'directory':str(root/'bank')}}})
            budget=Budget(root/'budget.json',10_000_000);budget.save()
            def execute(*_):
                job={};one=branch/'one';one.write_bytes(b'x'*100);runtime.account_write(job,one)
                linked=branch/'linked';os.link(one,linked);runner.account_write(job,linked)
                runtime.account_remove(job,one)
                self.assertTrue(linked.exists())
                runner.account_remove(job,linked)
                two=branch/'two';two.write_bytes(b'x'*200);runner.account_write(job,two)
                with patch.object(Path,'unlink',side_effect=OSError('injected removal failure')):
                    with self.assertRaises(OSError):runner.account_remove(job,two)
                return {'status':'complete'}
            run_budgeted(branch,None,budget.path,execute=execute)
            current=Budget(budget.path);key,size=__import__('declarative_quality_storage').inode(branch/'two')
            self.assertEqual(current.value['versions'][key],200)
            self.assertFalse(any(v==100 for v in current.value['versions'].values()))
    def test_pure_evaluation_rejects_learning_state_mutation_and_requires_selection(self):
        from declarative_quality_cold import pure_evaluate,bound,FIXED
        from blueprint_ppo_store import save,reference
        for mutation in (False,True):
            with self.subTest(mutation=mutation),tempfile.TemporaryDirectory() as tmp:
                root=Path(tmp);owner=root/'seed-71';config=config_for(71,'baseline-8','development',root,600)
                config['device']='cpu';config['execution_options']['behavior_device']='cpu'
                learner=Learner(71,config,'fixture');learner.ordinal=128;saved=learner.checkpoint([])
                checkpoint=owner/'checkpoints/0128.pt.gz';save(checkpoint,saved)
                panel=root/'panel.json';write(panel,{'cases':[]})
                summary=root/'summary.json';write(summary,{'profile':'baseline-8','checkpoint':bound(checkpoint)})
                selection=root/'selection.json';write(selection,dict(schema='declarative-cold-quality-selection-v1',
                    selected=['gain-8','score-8'],protocol=bound(FIXED),panels={'confirmation':bound(panel)},development=[bound(summary)]))
                value=dict(study=dict(seed=71,endpoint=128,profile='baseline-8',launch_sha256='fixture'),
                    owner=owner,job={'latest_checkpoint':reference(checkpoint,owner)},saved=saved,
                    launch=dict(config=config,source={'native_library':'fixture'},allocation={}))
                def evaluate(actual,workers,cases,output,job,guard):
                    write(output/'evaluation/0128/report.json',{'rows':[]})
                    if mutation:actual.sampler.manual_seed(999)
                    return {'rows':[]}
                with patch('declarative_quality_cold.endpoint',return_value=value),\
                     patch('declarative_quality_cold.pool',return_value=nullcontext(None)),\
                     patch('declarative_quality_cold.ResourceGuard',return_value=SimpleNamespace(began=0)),\
                     patch('declarative_learning_run.evaluate',side_effect=evaluate),\
                     patch('torch.set_num_threads'),patch('torch.set_num_interop_threads'):
                    if mutation:
                        with self.assertRaisesRegex(ValueError,'ordinal changed'):
                            pure_evaluate(root,panel,selection,root/'eval',600)
                    else:
                        result=pure_evaluate(root,panel,selection,root/'eval',600)
                        self.assertEqual(result['training_updates'],0);self.assertEqual(result['checkpoint'],bound(checkpoint))
                    write(selection,dict(schema='warm-selection',selected=[]))
                    with self.assertRaises((ValueError,KeyError)):
                        pure_evaluate(root,panel,selection,root/'invalid',600)
                    self.assertFalse((root/'invalid').exists())
    def test_command_entry_reconciles_unsaved_created_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);branch=root/'branch';write(branch/'launch.json',{'config':{'training_input':{'directory':str(root/'bank')}}})
            budget=Budget(root/'budget.json',1000);budget.save();(branch/'unsaved').write_bytes(b'x'*2000)
            with self.assertRaisesRegex(ValueError,'ceiling'):run_budgeted(branch,None,budget.path,execute=lambda *_:self.fail('wrote before reconciliation'))


if __name__=='__main__':unittest.main()
