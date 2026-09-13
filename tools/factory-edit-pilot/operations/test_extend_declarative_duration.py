"""Time-only extension preserves actual learner state and rejects unsafe changes."""
from datetime import datetime,timezone,timedelta
from pathlib import Path
import tempfile
import unittest

import extend_declarative_duration as operation
from blueprint_ppo_store import ROOT,load,read,reference,save,sha,write
from blueprint_ppo_update import digest
from declarative_learning_runtime import CHECKPOINT


class ExtensionTests(unittest.TestCase):
    def fixture(self,root):
        c=read(ROOT/'tools/factory-edit-pilot/declarative-four-hour.json');write(root/'launch.json',dict(config=c,verification={'retained':'proof'}))
        owner=root/'seed-29';saved=dict(schema=CHECKPOINT,seed=29,update=80,protocol_sha256=sha(root/'launch.json'),
            streams=[dict(native=None,next_start={'actual_input':i}) for i in range(32)],buffer={'active':{},'entries':{}},
            model={'weights':[1,2,3]},actor_optimizer={'step':7},critic_optimizer={'step':9},sampler_rng=[3],shuffle_rng=[4])
        path=owner/'checkpoints/0080.pt.gz';save(path,saved)
        job=dict(status='paused',stop_reason='requested_stop',update=80,seconds=3000,latest_checkpoint=reference(path,owner))
        write(owner/'state.json',job);write(root/'state.json',dict(status='paused',launch_sha256=sha(root/'launch.json'),seconds=3010,jobs={'29':{}}))
        return c,saved

    def test_extension_changes_only_time_limits_and_preserves_checkpoint(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);old,saved=self.fixture(root);now=datetime(2026,9,13,13,tzinfo=timezone.utc)
            result=operation.extend(root,now+timedelta(hours=12),now=now)
            launch=read(root/'launch.json');c=launch['config'];actual=load(root/'seed-29'/result['checkpoint']['path'])
            self.assertEqual(c['duration']['seconds'],46200);self.assertEqual(c['limits']['arm_seconds'],46500)
            self.assertEqual(c['limits']['matrix_seconds'],46560)
            for key in old.keys()-{'duration','limits'}:self.assertEqual(c[key],old[key])
            for key in ('matrix_rss_bytes','min_free_bytes','arm_output_bytes'):self.assertEqual(c['limits'][key],old['limits'][key])
            for key in saved.keys()-{'protocol_sha256'}:self.assertEqual(digest(actual[key]),digest(saved[key]))
            self.assertEqual(actual['protocol_sha256'],sha(root/'launch.json'))
            archived=root/launch['duration_amendments'][0]['previous_launch']['path']
            self.assertEqual(sha(archived),saved['protocol_sha256'])
            self.assertEqual(load(root/'seed-29/checkpoints/0080.pt.gz')['protocol_sha256'],saved['protocol_sha256'])
            self.assertEqual(read(root/'state.json')['seconds'],3010)

    def test_rejects_running_past_naive_and_nonextension(self):
        with self.assertRaisesRegex(ValueError,'UTC offset'):operation.deadline('2026-09-14T10:00:00')
        self.assertEqual(operation.deadline('2026-09-14T10:00:00+09:00').hour,1)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.fixture(root);now=datetime(2026,9,13,13,tzinfo=timezone.utc);before=sha(root/'launch.json')
            for until in (now-timedelta(seconds=1),now+timedelta(seconds=1)):
                with self.assertRaises(ValueError):operation.extend(root,until,now=now)
            state=read(root/'state.json');state['status']='running';write(root/'state.json',state)
            with self.assertRaisesRegex(ValueError,'pause'):operation.extend(root,now+timedelta(hours=12),now=now)
            self.assertEqual(before,sha(root/'launch.json'))


if __name__=='__main__':unittest.main()
