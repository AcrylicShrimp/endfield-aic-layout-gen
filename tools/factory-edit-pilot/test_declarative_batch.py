"""Logical batch input/identity and complete-frozen-batch contract checks."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest
from declarative_batch_input import CanonicalInputs
from declarative_batch_experiment import configuration
from declarative_learning_curriculum import initial_plan


class BatchTests(unittest.TestCase):
    def test_canonical_mapping_preserves_actual_identity_and_rejects_wrong_slot(self):
        canonical=dict(identity=dict(stream=0,episode=35,slot=35,first_seed=10280),reference=dict(path='episode-35.json',sha256='x'),input_sha256='payload')
        a=object.__new__(CanonicalInputs);a.streams=8
        b=object.__new__(CanonicalInputs);b.streams=32
        left=a.mapped(canonical,3,4);right=b.mapped(canonical,3,1)
        self.assertEqual(left['reference'],right['reference']);self.assertEqual(left['input_sha256'],right['input_sha256'])
        self.assertEqual(left['canonical_identity'],right['canonical_identity'])
        self.assertEqual(left['identity']['slot'],right['identity']['slot'])
        with self.assertRaisesRegex(ValueError,'slot'):b.mapped(canonical,4,1)

    def test_registered_arms_hold_numerical_settings_and_total_actions(self):
        protocol=read(ROOT/'tools/factory-edit-pilot/declarative-batch-comparison.json')
        a=configuration(protocol,17,256,Path('/tmp/batch-test'))
        b=configuration(protocol,17,1024,Path('/tmp/batch-test'))
        for key in ('ppo','success','native','actions','start_curriculum','training_input'):
            self.assertEqual(a[key],b[key],key)
        for c,size in ((a,256),(b,1024)):
            self.assertEqual(c['training']['updates']*size,32768)
            self.assertEqual([n*size for n in c['evaluation']['at']],[0,16384,32768])
            self.assertEqual(c['training']['episode_actions'],64)

    def test_initial_large_wave_reports_three_unfilled_depth_one_slots(self):
        protocol=read(ROOT/'tools/factory-edit-pilot/declarative-batch-comparison.json')
        with tempfile.TemporaryDirectory() as tmp:
            config=configuration(protocol,17,1024,Path(tmp))
            plan,_=initial_plan(config,17,Path(tmp))
            self.assertEqual(plan['requested'],{'0':29,'1':3,'2':0})
            self.assertEqual(plan['actual'],{'0':32})
            self.assertEqual(sum('fallback' in s for s in plan['slots']),3)
            self.assertEqual([s['stream'] for s in plan['slots']],list(range(32)))
            again,_=initial_plan(config,17,Path(tmp));self.assertEqual(digest(plan),digest(again))


if __name__=='__main__':unittest.main()
