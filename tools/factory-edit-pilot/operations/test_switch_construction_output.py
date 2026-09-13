from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path
import tempfile
import unittest

import test_extend_declarative_duration as fixture_module
import switch_construction_output as operation
from blueprint_ppo_store import load, read, reference, save, sha, write
from blueprint_ppo_update import digest


class StorageSwitchTests(unittest.TestCase):
    def fixture(self, root):
        config, saved = fixture_module.ExtensionTests().fixture(root)
        config['training_input']['directory'] = str(root/'old-inputs')
        source = dict(native_sha256='native', files_sha256={key:'old' for key in operation.CHANGED_SOURCES})
        source['files_sha256']['tools/factory-edit-pilot/declarative_learning_policy.py'] = 'unchanged'
        launch = read(root/'launch.json')
        launch.update(config=config, source=source, duration_amendments=[{'requested_until_utc':'2026-09-14T01:00:00+00:00'}])
        write(root/'launch.json', launch)
        owner = root/'seed-29'
        saved.update(protocol_sha256=sha(root/'launch.json'), model_sha256=digest(saved['model']))
        save(owner/'checkpoints/0080.pt.gz', saved)
        job = read(owner/'state.json')
        job.update(latest_checkpoint=reference(owner/'checkpoints/0080.pt.gz',owner),
                   source={'directory':str(root/'parent')},inputs={
                       '0':{'reference':{'path':'episode-00000000/episode.json'},'input_directory':None},
                       '512':{'reference':{'path':'episode-00000512/episode.json'},'input_directory':None}})
        write(root/'parent/launch.json',{'config':{'training_input':{'directory':str(root/'parent-inputs')}}})
        write(root/'parent-inputs/episode-00000000/episode.json',{'actual':'parent'})
        write(root/'old-inputs/episode-00000512/episode.json',{'actual':'continued'})
        write(owner/'state.json',job)
        matrix = read(root/'state.json');matrix['launch_sha256']=sha(root/'launch.json');write(root/'state.json',matrix)
        current=deepcopy(source)
        for key in operation.CHANGED_SOURCES:current['files_sha256'][key]='new'
        proof=dict(status='passed',normal_input_roundtrip=True,final_output_comparison=True,source=current)
        write(root/'verification.json',proof)
        return saved, config

    def test_new_bank_preserves_old_inputs_and_entire_learning_state(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);saved,old=self.fixture(root)
            now=datetime(2026,9,13,16,tzinfo=timezone.utc)
            result=operation.switch(root,root/'verification.json',now=now)
            actual=load(root/'seed-29'/result['checkpoint']['path'])
            for key in saved.keys()-{'protocol_sha256'}:
                self.assertEqual(digest(actual[key]),digest(saved[key]))
            launch=read(root/'launch.json');self.assertEqual(actual['protocol_sha256'],sha(root/'launch.json'))
            self.assertEqual(launch['config']['duration']['seconds'],35400)
            self.assertEqual(result['remaining_seconds'],32400)
            self.assertFalse(Path(result['future_input_bank']).exists())
            job=read(root/'seed-29/state.json')
            self.assertEqual(read(Path(job['inputs']['0']['input_directory'])/job['inputs']['0']['reference']['path']),{'actual':'parent'})
            self.assertEqual(read(Path(job['inputs']['512']['input_directory'])/job['inputs']['512']['reference']['path']),{'actual':'continued'})
            for key in old.keys()-{'training_input','duration','limits'}:
                self.assertEqual(launch['config'][key],old[key])
            self.assertEqual(load(root/'seed-29/checkpoints/0080.pt.gz'),saved)

    def test_rejects_unverified_changes_and_unfinished_wave(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);self.fixture(root);before=sha(root/'launch.json')
            proof=read(root/'verification.json')
            proof['source']['files_sha256']['tools/factory-edit-pilot/declarative_learning_policy.py']='changed'
            write(root/'verification.json',proof)
            with self.assertRaisesRegex(ValueError,'source change'):
                operation.switch(root,root/'verification.json')
            self.assertEqual(before,sha(root/'launch.json'))
            saved=load(root/'seed-29/checkpoints/0080.pt.gz');saved['streams'][0]['native']={'unfinished':True}
            save(root/'seed-29/checkpoints/0080.pt.gz',saved)
            job=read(root/'seed-29/state.json');job['latest_checkpoint']=reference(root/'seed-29/checkpoints/0080.pt.gz',root/'seed-29');write(root/'seed-29/state.json',job)
            with self.assertRaisesRegex(ValueError,'complete-wave'):
                operation.switch(root,root/'verification.json')


if __name__=='__main__':
    unittest.main()
