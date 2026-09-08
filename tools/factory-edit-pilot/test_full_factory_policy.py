"""Physical edits, graph fidelity, learning signal and incumbent accounting."""
from copy import deepcopy
from fractions import Fraction
import json
import os
from pathlib import Path
import tempfile
import subprocess
import unittest

import numpy as np
import torch

from full_factory_environment import ROOT, FactoryRuntime, FactoryEpisode
from full_factory_policy import observe, FullFactoryPolicy
from full_factory_screen import advantages
from full_factory_curriculum import inverse_action


class RankTests(unittest.TestCase):
    def test_exact_lexicographic_terminal_rank(self):
        self.assertEqual(advantages([(9,1000,1000),(10,1,1),(10,1,1),(11,0,0)]), [1,0,0,-1])
        self.assertEqual(advantages([(10,1,1)]*4), [0]*4)

    def test_inverse_pose_uses_the_new_frame_when_every_facility_was_selected(self):
        before=dict(placements=[dict(instance='a',x=10,y=10,rotation=0),dict(instance='b',x=10,y=15,rotation=180)])
        action=dict(kind='pose',poses=[dict(member=0,x=20,y=20,rotation=90),dict(member=1,x=25,y=20,rotation=270)])
        # The native output is translated by (-3,-5) and its placement array reordered.
        after=dict(placements=[dict(instance='b',x=22,y=15,rotation=270),dict(instance='a',x=17,y=15,rotation=90)])
        target=dict(kind='pose',poses=[dict(member=0,x=7,y=10,rotation=180),dict(member=1,x=7,y=5,rotation=0)])
        self.assertIs(inverse_action(action,before,after,dict(actions=[dict(kind='restore_best'),target])),target)


class FactoryPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        library = Path(os.environ.get('AIC_NATIVE_LIBRARY', ROOT/'target/release/libaic_pilot_native.dylib'))
        if not library.is_file():
            raise unittest.SkipTest('Build the full-factory native library and set AIC_NATIVE_LIBRARY')
        cls.temp = tempfile.TemporaryDirectory()
        directory = Path(cls.temp.name)
        fixture = ROOT/'docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json'
        cls.witness = json.loads(fixture.read_text())
        # The immutable tracked fixture is the test obligation, never an edited proposal.
        terminals = {}
        for n in cls.witness['transport_networks']:
            for t in n['terminals']:
                ep = {k:v for k,v in t['endpoint'].items() if k in ('kind','instance','node')}
                key = (json.dumps(ep,sort_keys=True),t['direction'],n['item'])
                terminals[key] = terminals.get(key,Fraction(0)) + Fraction(t['rate']['numerator'], t['rate']['denominator'])
        request = dict(schema_version=1, instances=[dict(instance=p['instance'],recipe=p['recipe'],
                       utilization=cls.witness['utilizations'][p['instance']]) for p in cls.witness['placements']],
                       terminals=[dict(endpoint=json.loads(ep),direction=di,item=it,
                       rate=dict(numerator=rate.numerator,denominator=rate.denominator)) for (ep,di,it),rate in terminals.items()])
        (directory/'request.json').write_text(json.dumps(request))
        data = ROOT/'data/game/normalized'
        paths = {k:str(data/f'{filename}.json') for k,filename in
                 [('facilities','facilities'),('recipes','recipes'),('items','items'),
                  ('transports','transports'),('components','logistics-components')]}
        (directory/'catalogs.json').write_text(json.dumps(paths))
        cls.runtime = FactoryRuntime(library,directory/'catalogs.json')
        cls.case = dict(id='immutable-crossing',request=str(directory/'request.json'),initial=str(fixture))
        torch.set_num_threads(1)

    @classmethod
    def tearDownClass(cls):
        cls.runtime.close()
        cls.temp.cleanup()

    def episode(self):
        return FactoryEpisode(self.runtime,self.case,horizon=8)

    def test_strip_expansion_and_inverse_preserve_full_request(self):
        e = self.episode()
        original = e.state['best_metrics']
        action = next(a for a in e.describe()['actions'] if a['kind']=='insert')
        self.assertEqual(e.step(action)['status'],'accepted_exploration')
        inverse = next(a for a in e.describe()['actions'] if a['kind']=='contract'
                       and a['axis']==action['axis'] and a['position']==action['position'])
        self.assertEqual(e.step(inverse)['status'],'accepted_exploration')
        self.assertEqual(e.state['current_metrics'],original)
        self.assertEqual(e.state['best_metrics'],original)
        self.assertEqual(e.selected,2)

    def test_every_offered_insert_preserves_used_port_arms(self):
        e = self.episode()
        actions = [a for a in e.describe()['actions'] if a['kind']=='insert']
        self.assertTrue(actions)
        for action in actions:
            fresh = self.episode()
            row = fresh.step(action)
            self.assertEqual(row['status'],'accepted_exploration',action)
            self.assertTrue(row['acceptance']['candidate_validated'])
        # Reconstruct every facility / terminal arm directly from accepted catalog ports.
        description = e.describe()
        for n in e.state['current']['transport_networks']:
            for terminal in n['terminals']:
                if terminal['endpoint']['kind']!='facility':
                    continue
                port = next(p for p in description['ports'][terminal['endpoint']['instance']]
                            if p['id']==terminal['endpoint']['port'])
                for axis in ('x','y'):
                    a,b = port['position'][axis],terminal['position'][axis]
                    if a!=b:
                        self.assertFalse(any(c['axis']==axis and c['position']==max(a,b) for c in actions))

    def test_rejected_executor_and_rollback_are_charged(self):
        e = self.episode()
        initial = deepcopy(e.state['best'])
        invalid = dict(kind='insert',axis='x',position=-999,x=0,y=0)
        self.assertEqual(e.step(invalid)['status'],'executor_rejected')
        self.assertEqual(e.submitted,0)
        self.assertEqual(e.step(dict(kind='restore_best',x=0,y=0))['status'],'accepted_equal')
        self.assertEqual(e.selected,2)
        self.assertEqual(e.submitted,1)
        self.assertEqual(e.state['best'],initial)

    def test_bridge_release_uses_released_chain_ids(self):
        e = self.episode()
        action = next(a for a in e.describe()['actions'] if a['kind']=='release_bridge')
        result = e.step(action)
        self.assertFalse(any(d['code']=='local-route-priority' for d in result['diagnostics']))
        self.assertLessEqual(result['route_expansions'],e.work)
        self.assertLessEqual(e.state['best_metrics']['score'],e.state['initial_metrics']['score'])

    def test_sparse_graph_keeps_network_ownership_rates_and_gradients(self):
        e = self.episode()
        d = e.describe()
        obs = observe(e.state['current'],d,e.state,1)
        cells = obs['nodes'][:,2] == 1
        self.assertEqual(int(cells.sum()),sum(len(n['cells']) for n in e.state['current']['transport_networks']))
        # A bridge has two network-cell tokens at the same layer/XY; never collapse ownership.
        signatures = [tuple(row[[7,8,11,12]]) for row in obs['nodes'][cells]]
        self.assertLess(len(set(signatures)),len(signatures))
        arcs = obs['edge_values'][:,0] == 1
        self.assertEqual(int(arcs.sum()),sum(len(n['segments']) for n in e.state['current']['transport_networks']))
        expected = [s['rate']['numerator']/s['rate']['denominator'] for n in e.state['current']['transport_networks'] for s in n['segments']]
        np.testing.assert_allclose(obs['edge_values'][arcs,11],expected)
        p = FullFactoryPolicy(4)
        logits = p(obs)
        self.assertEqual(len(logits),len(d['actions']))
        self.assertTrue(torch.isfinite(logits).all())
        (-torch.log_softmax(logits,0)[0]).backward()
        self.assertGreater(float(p.edge.weight.grad.abs().sum()),0)
        self.assertGreater(float(p.raster[0].weight.grad.abs().sum()),0)
        broken = dict(obs,schema='legacy')
        with self.assertRaisesRegex(ValueError,'observation-schema'):
            p(broken)


    def test_group_reachability_and_required_closure(self):
        subprocess.run(['node',str(Path(__file__).with_name('test_full_factory_poses.mjs'))],check=True,capture_output=True)

    def test_old_executor_description_and_member_contracts_rejected(self):
        e=self.episode()
        self.runtime.worker.stdin.write(json.dumps(dict(schema_version=2,op='describe',witness=e.state['current']))+'\n')
        self.runtime.worker.stdin.flush()
        self.assertEqual(json.loads(self.runtime.worker.stdout.readline())['code'],'factory.executor-schema')
        d=e.describe()
        with self.assertRaisesRegex(ValueError,'description-schema'):
            observe(e.state['current'],dict(d,schema_version=2),e.state,1)
        for action in [dict(kind='move',instance=e.state['current']['placements'][0]['instance'],dx=1,dy=0),
                       dict(kind='relocate',members=[0],dx=1,dy=0),dict(kind='rotate',instance='old',rotation=90),
                       dict(kind='pose',poses=[dict(member=-1,x=0,y=0,rotation=0)]),
                       dict(kind='pose',poses=[dict(member=0,x=0,y=0,rotation=0)]*2)]:
            self.assertEqual(e.step(action)['status'],'executor_rejected')

    def test_all_group_members_and_destinations_receive_gradients(self):
        e=self.episode();d=e.describe()
        ps=e.state['current']['placements']
        x,y=min(p['x'] for p in ps[:2]),min(p['y'] for p in ps[:2])
        a=dict(kind='pose',poses=[dict(member=i,x=p['x']+1,y=p['y'],rotation=(p['rotation']+180)%360) for i,p in enumerate(ps[:2])],x=x+1,y=y,source_x=x,source_y=y,
               width=max(p['x']+p['width'] for p in ps[:2])-x,height=max(p['y']+p['height'] for p in ps[:2])-y,
               body_area=sum(p['width']*p['height'] for p in ps[:2]))
        obs=observe(e.state['current'],dict(d,actions=[a]),e.state,1)
        self.assertEqual(obs['member_nodes'].tolist(),[0,1])
        torch.manual_seed(11);model=FullFactoryPolicy(0)
        # Isolate the two new pooled operands; global/action features cannot explain this gradient.
        start=32+3*model.width
        with torch.no_grad():
            model.score[0].weight.zero_();model.score[0].weight[:,start:start+2*model.width]=.1
            model.score[0].bias.fill_(.1);model.score[2].weight.fill_(.1)
        obs['nodes']=torch.tensor(obs['nodes'],requires_grad=True)
        obs['member_pose']=torch.tensor(obs['member_pose'],requires_grad=True)
        raster=[]
        def capture(module,inputs,output):
            output.retain_grad();raster.append(output)
        handle=model.raster.register_forward_hook(capture)
        model(obs).sum().backward();handle.remove()
        for index in obs['member_nodes']:
            self.assertGreater(float(obs['nodes'].grad[index].abs().sum()),0)
        for row in obs['member_pose'].grad:
            self.assertTrue(bool((row.abs()>0).all()))
        for x,y in obs['member_xy']:
            self.assertGreater(float(raster[0].grad[0,:,y,x].abs().sum()),0)

    def test_member_encoding_preserves_orientation_and_source_destination_pairing(self):
        e=self.episode();d=e.describe();ps=e.state['current']['placements']
        poses=[dict(member=i,x=p['x'],y=p['y'],rotation=(p['rotation']+180)%360) for i,p in enumerate(ps[:2])]
        x,y=min(p['x'] for p in ps[:2]),min(p['y'] for p in ps[:2])
        action=dict(kind='pose',poses=poses,x=x,y=y,source_x=x,source_y=y,
                    width=max(p['x']+p['width'] for p in ps[:2])-x,height=max(p['y']+p['height'] for p in ps[:2])-y,
                    body_area=sum(p['width']*p['height'] for p in ps[:2]))
        obs=observe(e.state['current'],dict(d,actions=[action]),e.state,1)
        torch.manual_seed(67);model=FullFactoryPolicy(0)
        # Isolate pooled members; whole-scene means and action envelopes stay fixed.
        with torch.no_grad():
            model.score[0].weight[:,:32+3*model.width]=0
            model.score[0].weight[:,32+5*model.width:]=0
        base=model(obs)
        reversed_obs=deepcopy(obs)
        for field in ('member_nodes','member_actions','member_xy','member_pose'):
            reversed_obs[field]=reversed_obs[field][::-1].copy()
        torch.testing.assert_close(model(reversed_obs),base)
        swapped=deepcopy(obs);swapped['member_nodes']=swapped['member_nodes'][::-1].copy()
        self.assertGreater(float((model(swapped)-base).abs().detach()),1e-7)
        changed=deepcopy(obs);changed['member_pose'][0,6:10]*=-1
        self.assertGreater(float((model(changed)-base).abs().detach()),1e-7)

    def test_compound_rotation_is_offered_and_validates_a_complete_original_factory(self):
        source=ROOT/'artifacts/factory-edit-pilot/full-factory-inputs-verified/inputs.json'
        if not source.exists():self.skipTest('Optional retained original evidence absent')
        case=next(r for r in json.loads(source.read_text())['rows'] if r['id']=='closure-original')
        e=FactoryEpisode(self.runtime,case);before=deepcopy(e.state['current']);description=e.describe()
        compounds=[a for a in description['actions'] if a['kind']=='pose' and len(a['poses'])>1 and
                   any(q['rotation']!=before['placements'][q['member']]['rotation'] for q in a['poses'])]
        self.assertTrue(compounds)
        # Retained original-factory witness: this pair can exchange orientation and
        # relative positions while all other requirements remain in force.
        action=next(a for a in compounds if [q['member'] for q in a['poses']]==[35,41] and
                    all(q['rotation']==0 for q in a['poses']) and a['x']==a['source_x'] and a['y']==a['source_y'])
        row=e.step(action)
        self.assertTrue(row['acceptance']['candidate_validated'])
        self.assertFalse(row['unchanged'])
        for q in action['poses']:
            actual=e.state['current']['placements'][q['member']]
            self.assertEqual((actual['x'],actual['y'],actual['rotation']),(q['x'],q['y'],q['rotation']))
        self.assertLessEqual(row['route_expansions'],5000)


    def test_original_heavy_distant_and_connected_group_native_acceptance(self):
        source=ROOT/'artifacts/factory-edit-pilot/full-factory-inputs-verified/inputs.json'
        if not source.exists():self.skipTest('Optional retained original 59-facility evidence absent')
        case=next(r for r in json.loads(source.read_text())['rows'] if r['id']=='closure-original')
        for members,dx,dy in [([0,2],1,0),([0],-3,0)]:
            e=FactoryEpisode(self.runtime,case)
            a=next(a for a in e.describe()['actions'] if a['kind']=='pose' and
                   a['poses']==[dict(member=i,x=e.state['current']['placements'][i]['x']+dx,y=e.state['current']['placements'][i]['y']+dy,rotation=e.state['current']['placements'][i]['rotation']) for i in members])
            before=deepcopy(e.state['current'])
            result=e.step(a)
            self.assertTrue(result['acceptance']['candidate_validated'])
            self.assertFalse(result['unchanged'])
            self.assertLessEqual(result['route_expansions'],5000)
            for index in members:
                self.assertEqual(e.state['current']['placements'][index]['x'],before['placements'][index]['x']+dx)
                self.assertEqual(e.state['current']['placements'][index]['y'],before['placements'][index]['y']+dy)


    def test_dense_root_terminal_rejoins_are_offered_and_native_validated(self):
        source=ROOT/'artifacts/factory-edit-pilot/full-factory-relocation-hole-probe'
        inputs=ROOT/'artifacts/factory-edit-pilot/full-factory-inputs-verified/inputs.json'
        if not (source/'report.json').exists():self.skipTest('Optional retained dense-root evidence absent')
        case=next(c for c in json.loads(inputs.read_text())['rows'] if c['id']=='closure-original')
        records=json.loads((source/'report.json').read_text())['trace']
        for index in [121,127,154]:
            forward=next(t['action'] for t in records if t['candidate_index']==index)
            e=FactoryEpisode(self.runtime,dict(case,initial=str(source/f'candidate-{index}.json')))
            actions=e.describe()['actions']
            target=[dict(member=i,x=e.state['current']['placements'][i]['x']-forward['dx'],y=e.state['current']['placements'][i]['y']-forward['dy'],rotation=e.state['current']['placements'][i]['rotation']) for i in forward['members']]
            inverse=next(a for a in actions if a['kind']=='pose' and a['poses']==target)
            result=e.step(inverse)
            self.assertEqual(result['status'],'improved')
            self.assertTrue(result['acceptance']['candidate_validated'])
            self.assertEqual(result['best_score'],[2132,385,146])


    def test_runtime_close_drains_an_interrupted_large_response(self):
        runtime=object.__new__(FactoryRuntime)
        runtime.worker=subprocess.Popen(['node','-e',"process.stdin.resume();process.stdin.on('end',()=>process.stdout.write('x'.repeat(1000000)))"],
                                        stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
        runtime.close()
        self.assertEqual(runtime.worker.returncode,0)
