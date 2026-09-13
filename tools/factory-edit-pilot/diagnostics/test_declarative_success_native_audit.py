"""Complete physical attribution, explicit missing evidence and native boundaries."""
from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch

from declarative_success_native_audit import (Runtime, canonical_witness, case_checkpoint, comparison,
    eligibility, execute, physical_key, reference, resource_limited, validate_prepend)
import test_declarative_learning as fixture


class AuditTests(unittest.TestCase):
    def result(self,identity='same',bonus=.1,status='reproduced_success'):
        return dict(status=status,final_physical_sha256=identity,final={'best_metrics':{'score':[80,1,1]}},
            features=dict(gain=.2,capped_exterior_bonus=bonus))

    def test_controls_and_inconclusive_reversals_cannot_pass_eligibility(self):
        controls=[dict(role=r,control_reproduced=True) for r in ('outward_success','non_loop_compaction')]
        pairs=[comparison(self.result(),self.result(identity='different'),family) for family in ('pose_reversal','band_pose_contraction')]
        result=eligibility(controls+[dict(role='reversal',comparisons=pairs)])
        self.assertFalse(result['geometry_eligible']);self.assertEqual(result['decisive_families'],[])

    def test_only_same_complete_endpoint_gain_and_success_allow_attribution(self):
        baseline=self.result();same=comparison(baseline,self.result(bonus=.15),'band_pose_contraction')
        self.assertTrue(same['decisive']);self.assertTrue(same['gratuitous_loop_bonus'])
        for actual in (self.result(identity='body-only-match'),self.result(status='invalid_action'),
                       self.result(status='resource_exhausted')):
            self.assertFalse(comparison(baseline,actual,'pose_reversal')['decisive'])
        different=self.result();different['features']['gain']=.3
        self.assertFalse(comparison(baseline,different,'pose_reversal')['decisive'])
        controls=[dict(role=r,control_reproduced=True) for r in ('outward_success','non_loop_compaction')]
        controls.append(dict(role='reversal',comparisons=[comparison(baseline,baseline,'pose_reversal'),same]))
        self.assertFalse(eligibility(controls)['geometry_eligible'])
        controls[-1]['comparisons'][-1]=comparison(baseline,baseline,'band_pose_contraction')
        self.assertTrue(eligibility(controls)['geometry_eligible'])
        self.assertFalse(eligibility(controls,resource_stop=True)['geometry_eligible'])

    def test_geometric_export_is_not_a_native_state_and_routing_caps_are_explicit(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'geometry.json';path.write_text(json.dumps(dict(before=[[0,0,0,2,2,0]],native_validated=True)))
            with self.assertRaisesRegex(ValueError,'geometric exports'):
                case_checkpoint(None,dict(input=reference(path)),lambda:None)
        self.assertTrue(resource_limited(dict(diagnostics=[{'code':'declarative.elapsed'}])))
        self.assertTrue(resource_limited(dict(details={'routes':[{'status':'budget_exhausted'}]})))
        self.assertFalse(resource_limited(dict(details={'routes':[{'status':'unreachable'}]})))

    def test_canonical_translation_preserves_terminals_segments_components_and_ports(self):
        witness=dict(placements=[dict(instance='a',x=2,y=3,width=2,height=3,rotation=90)],
            transport_networks=[dict(cells=[dict(x=8,y=9,z=0)],
                terminals=[dict(position=dict(x=1,y=2),endpoint=dict(instance='a',port='input'))],
                segments=[{'from':dict(x=1,y=2),'to':dict(x=8,y=9)}])],
            logistics_components=[dict(position=dict(x=0,y=1),kind='component')])
        moved=deepcopy(witness)
        def shift(value):
            if isinstance(value,dict):
                if 'x' in value and 'y' in value:value.update(x=value['x']+17,y=value['y']+21)
                for child in value.values():shift(child)
            if isinstance(value,list):
                for child in value:shift(child)
        shift(moved)
        self.assertEqual(physical_key(canonical_witness(witness)),physical_key(canonical_witness(moved)))
        moved['transport_networks'][0]['terminals'][0]['endpoint']['port']='other'
        self.assertNotEqual(physical_key(canonical_witness(witness)),physical_key(canonical_witness(moved)))


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'requires selected release native library')
class NativeAuditTests(unittest.TestCase):
    def setUp(self):
        helper=fixture.NativeLearningTests();helper.setUp();self.runtime=Runtime(helper.config,os.environ['AIC_NATIVE_LIBRARY'])
        self.session=self.runtime.open(helper.runtime.request,helper.runtime.original)
        self.checkpoint=json.loads(self.session.checkpoint_json())
        self.action=dict(kind='place',instance='C1',x=26,y=16,rotation=180,ports=[
            dict(node=6,port='input-belt-2'),dict(node=11,port='input-belt-3'),dict(node=20,port='output-belt-1')])

    def test_actual_success_reproduces_from_complete_checkpoint_and_rebinds_revision(self):
        actions=[dict(kind='insert_band',axis='x',at=24,count=1),dict(kind='delete_band',axis='x',at=24,count=1),self.action]
        revisions=[];step=self.runtime.step
        def recorded(session,observation,action):
            revisions.append(observation['revision']);return step(session,observation,action)
        with patch.object(self.runtime,'step',recorded):result=execute(self.runtime,self.checkpoint,actions,lambda:None)
        self.assertEqual(revisions,[0,1,2]);self.assertEqual(result['status'],'reproduced_success')
        self.assertEqual(result['final']['best_metrics']['score'][0],665)
        other=execute(self.runtime,self.checkpoint,actions,lambda:None)
        self.assertEqual(other['final_physical_sha256'],result['final_physical_sha256'])

    def test_illegal_counterfactual_ports_do_not_become_success_from_retained_best(self):
        action=deepcopy(self.action);action['ports'][0]['port']='not-a-native-port'
        result=execute(self.runtime,self.checkpoint,[action],lambda:None)
        self.assertEqual(result['status'],'invalid_action');self.assertIsNone(result['features'])
        self.assertTrue(result['final']['factory_validated'])
        observation=self.runtime.observe(self.session)
        with self.assertRaisesRegex(ValueError,'exact initial facility pose and ports'):
            validate_prepend(dict(family='pose_reversal',prepend=[action,action]),observation)

    def test_expired_native_checkpoint_is_resource_limited(self):
        session=self.runtime.open(self.checkpoint['request'],self.checkpoint['original'],limits=dict(elapsed_ms=1))
        time.sleep(.005);checkpoint=json.loads(session.checkpoint_json())
        result=execute(self.runtime,checkpoint,[self.action],lambda:None)
        self.assertEqual(result['status'],'resource_exhausted')


if __name__=='__main__':unittest.main()
