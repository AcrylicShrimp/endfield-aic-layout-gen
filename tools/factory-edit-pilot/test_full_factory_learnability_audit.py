"""Detect sufficient action/input equality obstructions without claiming completeness."""
import unittest
import numpy as np
from full_factory_learnability_audit import action_aliases,target_mass_ceiling,observation_key


class AliasTests(unittest.TestCase):
    def test_shared_readout_inputs_bound_target_probability(self):
        obs=dict(actions=np.zeros((3,32),np.float32),operands=np.zeros((3,2),np.int64),action_xy=np.zeros((3,2),np.int64),
                 member_actions=np.array([],np.int64),member_nodes=np.array([],np.int64),member_xy=np.zeros((0,2),np.int64),member_pose=np.zeros((0,10),np.float32))
        groups=action_aliases(obs)
        self.assertEqual(groups,[[0,1,2]])
        self.assertEqual(target_mass_ceiling(groups,[1]),1/3)
        self.assertEqual(target_mass_ceiling(groups,[0,1,2]),1)
        obs['action_xy'][2,0]=1
        self.assertEqual(action_aliases(obs),[[0,1],[2]])
        self.assertEqual(target_mass_ceiling(action_aliases(obs),[2]),1)

    def test_member_order_is_invariant_but_correspondence_matters(self):
        obs=dict(actions=np.zeros((2,32),np.float32),operands=np.zeros((2,2),np.int64),action_xy=np.zeros((2,2),np.int64),
                 member_actions=np.array([0,0,1,1]),member_nodes=np.array([4,5,5,4]),member_xy=np.array([[1,2],[3,4],[3,4],[1,2]]),member_pose=np.zeros((4,10),np.float32))
        self.assertEqual(action_aliases(obs),[[0,1]])
        obs['member_nodes'][2:]=[4,5]
        self.assertEqual(action_aliases(obs),[[0],[1]])

    def test_observation_key_includes_context_shape_and_dtype(self):
        obs=dict(schema='test',nodes=np.zeros((2,2),np.float32),context=np.array([1],np.float32))
        key=observation_key(obs)
        self.assertEqual(key,observation_key(dict(reversed(list(obs.items())))))
        for other in [dict(obs,context=np.array([.5],np.float32)),dict(obs,nodes=obs['nodes'].reshape(1,4)),dict(obs,nodes=obs['nodes'].astype(np.float64))]:
            self.assertNotEqual(key,observation_key(other))
