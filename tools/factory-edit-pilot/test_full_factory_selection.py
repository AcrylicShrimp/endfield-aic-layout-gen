"""Verify balanced probability mass and history masks without routing outcomes."""
from copy import deepcopy
import unittest

import torch

from full_factory_selection import physical_key,action_key,SelectionMemory,distribution,geometry_choice


class SelectionTests(unittest.TestCase):
    def test_operation_mass_is_independent_of_candidate_count(self):
        actions=[{'kind':'relocate'}]*100+[{'kind':'release_bridge'}]
        logits=torch.zeros(101,requires_grad=True)
        d=distribution(logits,actions,[True]*101,True)
        self.assertAlmostEqual(float(d.probs[:100].sum().detach()),.5,places=6)
        self.assertAlmostEqual(float(d.probs[-1].detach()),.5,places=6)
        (-d.log_prob(torch.tensor(100))).backward()
        self.assertTrue(torch.isfinite(logits.grad).all())
        self.assertGreater(float(logits.grad.abs().sum()),0)
        allowed=[True]+[False]*99+[True]
        d=distribution(logits.detach(),actions,allowed,True)
        self.assertAlmostEqual(float(d.probs[0]),.5,places=6)
        self.assertTrue((d.probs[1:100]==0).all())

    def test_order_independent_state_preserves_geometry_and_identity(self):
        w={'placements':[{'instance':'a','x':0},{'instance':'b','x':2}],
           'networks':[{'id':'n','cells':[{'x':0,'y':1},{'x':1,'y':1}]}]}
        other=deepcopy(w);other['placements'].reverse();other['networks'][0]['cells'].reverse()
        self.assertEqual(physical_key(w),physical_key(other))
        other['placements'][0]['x']+=1
        self.assertNotEqual(physical_key(w),physical_key(other))

    def test_reroute_key_survives_enumeration_change(self):
        chain={'network':0,'cells':[{'x':0,'y':0},{'x':1,'y':0}],'rate':{'numerator':1,'denominator':2}}
        first={'kind':'reroute','chain':0,'network':0,'x':0,'y':0}
        second=dict(first,chain=1,network=1)
        a=action_key(first,{'chains':[chain]},{'transport_networks':[{'id':'n'}]},'best')
        b=action_key(second,{'chains':[None,dict(chain,network=1)]},{'transport_networks':[{'id':'other'},{'id':'n'}]},'best')
        self.assertEqual(a,b)
        changed=deepcopy(chain);changed['cells'].reverse()
        self.assertNotEqual(a,action_key(first,{'chains':[changed]},{'transport_networks':[{'id':'n'}]},'best'))

    def test_memory_only_masks_observed_edges_and_rollback_depends_on_best(self):
        m=SelectionMemory({'state':'initial'})
        actions=[{'kind':'relocate'},{'kind':'restore_best'}];keys=['move','restore']
        self.assertEqual(m.mask('a',keys,actions,True),([True,True],False))
        self.assertEqual(m.record('a','a','move'),(True,False))
        self.assertEqual(m.mask('a',keys,actions,True),([False,True],False))
        self.assertEqual(m.mask('b',keys,actions,True),([True,True],False))
        self.assertEqual(m.mask('a',keys,actions,False),([True,True],False))
        self.assertEqual(m.record('a','novel','restore'),(False,False))
        m.record('a','a','restore')
        self.assertEqual(m.mask('a',keys,actions,True),([False,True],True))
        action={'kind':'restore_best'}
        self.assertNotEqual(action_key(action,{},None,'old'),action_key(action,{},None,'new'))

    def test_geometry_control_moves_to_next_candidate_after_failure(self):
        w={'placements':[dict(instance='a',x=0,y=0,width=1,height=1),dict(instance='b',x=4,y=0,width=1,height=1)]}
        actions=[dict(kind='relocate',members=[0],dx=1,dy=0),dict(kind='relocate',members=[1],dx=-1,dy=0),dict(kind='restore_best')]
        self.assertEqual(geometry_choice(w,actions,[True]*3),0)
        self.assertEqual(geometry_choice(w,actions,[False,True,True]),1)
        self.assertEqual(geometry_choice(w,actions,[False,False,True]),2)


    def test_relocation_keys_resolve_member_identity(self):
        w={'placements':[{'instance':'a'},{'instance':'b'},{'instance':'c'}]}
        a=dict(kind='relocate',members=[0,1],dx=7,dy=-3)
        key=action_key(a,{},w,'')
        other={'placements':list(reversed(w['placements']))}
        self.assertEqual(key,action_key(dict(a,members=[2,1]),{},other,''))
        self.assertNotEqual(key,action_key(a,{},other,''))
