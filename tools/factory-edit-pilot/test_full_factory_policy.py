"""Physical edits, graph fidelity, learning signal and incumbent accounting."""
from copy import deepcopy
from fractions import Fraction
import json
import os
from pathlib import Path
import tempfile
import unittest

import numpy as np
import torch

from full_factory_environment import ROOT, FactoryRuntime, FactoryEpisode
from full_factory_policy import observe, FullFactoryPolicy
from full_factory_screen import advantages


class RankTests(unittest.TestCase):
    def test_exact_lexicographic_terminal_rank(self):
        self.assertEqual(advantages([(9,1000,1000),(10,1,1),(10,1,1),(11,0,0)]), [1,0,0,-1])
        self.assertEqual(advantages([(10,1,1)]*4), [0]*4)


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
