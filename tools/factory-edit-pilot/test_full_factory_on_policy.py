"""Reward-dependent gradients, delayed credit and exact native-rollout recomputation."""
from copy import deepcopy
import itertools
import os
from pathlib import Path
import tempfile
import unittest

import torch

from full_factory_environment import ROOT
from full_factory_large_learning import read,unpack
from full_factory_on_policy import relative_returns,loss_components,on_policy_rollout,replay_gradients,parameter_hash
from full_factory_on_policy_learning import config_check,group_schedule
from full_factory_policy import observe,FullFactoryPolicy
from full_factory_selection import distribution
import test_full_factory_policy as fixtures


class ReturnTests(unittest.TestCase):
    def test_priority_and_flat_returns(self):
        self.assertEqual(relative_returns([[9,999999,999999],[10,0,0]]),[1,-1])
        self.assertEqual(relative_returns([[10,20,9],[10,20,8],[10,21,0]]),[0,1,-1])
        self.assertEqual(relative_returns([[10,20,9]]*4),[0,0,0,0])
        for invalid in [[],[[1,2,3]],[[1,2,True],[1,2,3]]]:
            with self.assertRaises(ValueError):relative_returns(invalid)

    def test_flat_return_has_no_policy_gradient_but_entropy_can_change(self):
        logits=torch.tensor([1.0,-1.0],requires_grad=True)
        dist=torch.distributions.Categorical(logits=logits)
        loss,regularizer=loss_components(dist.log_prob(torch.tensor(0)),dist.entropy(),0,4,32,.001)
        reward=torch.autograd.grad(loss,logits,retain_graph=True)[0]
        entropy=torch.autograd.grad(regularizer,logits)[0]
        self.assertEqual(float(reward.abs().sum()),0)
        self.assertGreater(float(entropy.abs().sum()),0)

    def test_group_gradient_matches_frozen_reference_objective_with_delayed_credit(self):
        # Abstract two-step MDP: action one first worsens current cost to 12 while
        # retaining best 10; a later action one unlocks terminal incumbent 7.
        actions=list(itertools.product([0.,1.],repeat=2))
        scores=[[10,1,1],[9,1,1],[10,1,1],[7,1,1]]
        parameters=torch.tensor([-.4,.2],dtype=torch.float64,requires_grad=True)
        def probabilities():
            logs=torch.stack([torch.distributions.Bernoulli(logits=parameters).log_prob(torch.tensor(a)).sum() for a in actions])
            return logs,logs.exp()
        logs,probs=probabilities()
        reference=probs.detach()
        comparator=lambda s,t:float((tuple(s)<tuple(t))-(tuple(s)>tuple(t)))
        rewards=torch.tensor([sum(float(reference[j])*comparator(s,t) for j,t in enumerate(scores)) for s in scores],dtype=torch.float64)
        exact=torch.autograd.grad(-(probs*rewards).sum()/2,parameters)[0]
        logs,probs=probabilities()
        sampled_loss=0
        for i,j in itertools.product(range(4),repeat=2):
            returns=relative_returns([scores[i],scores[j]])
            sampled_loss+=reference[i]*reference[j]*(-(logs[i]*returns[0]+logs[j]*returns[1])/(2*2))
        estimated=torch.autograd.grad(sampled_loss,parameters)[0]
        torch.testing.assert_close(estimated,exact,rtol=1e-10,atol=1e-10)
        self.assertLess(float(estimated[0]),0,'Later return must credit the initially worse first action')

    def test_config_and_root_schedule(self):
        c=read(Path(__file__).with_name('full-factory-on-policy-study.json'))
        config_check(c)
        order=group_schedule(['a','b','c'],c)
        self.assertEqual(order,group_schedule(['a','b','c'],c))
        self.assertEqual(len({s for g in order for s in g['sample_seeds']}),c['groups']*c['group_episodes'])
        self.assertEqual([g['case'] for g in order],[order[i%3]['case'] for i in range(len(order))])
        for name,value in [('groups',True),('group_episodes',1),('balanced',1),('temperature',float('nan')),('entropy_coefficient',-1)]:
            with self.assertRaises(ValueError):config_check(dict(c,**{name:value}))


class KindPolicy(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.preferences=torch.nn.Parameter(torch.tensor([-40.,-40.,-40.,-40.,-40.,0.,1.,-40.]))

    def forward(self,obs):
        return torch.as_tensor(obs['actions'][:,:8])@self.preferences


class NativeReplayTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixtures.FactoryPolicyTests.setUpClass.__func__(cls)

    @classmethod
    def tearDownClass(cls):
        fixtures.FactoryPolicyTests.tearDownClass.__func__(cls)

    def test_recomputed_group_gradient_matches_retained_graph_and_detects_drift(self):
        c=read(Path(__file__).with_name('full-factory-on-policy-study.json'))
        c.update(group_episodes=2,horizon=4,episode_seconds=30)
        p=dict(config=c,catalogs=str(Path(self.temp.name)/'catalogs.json'),
               native_library=os.environ.get('AIC_NATIVE_LIBRARY',str(ROOT/'target/release/libaic_pilot_native.dylib')))
        case=dict(self.case,instances=len(self.witness['placements']),family='test',split='train',transfer_group='mechanical-test')
        model=KindPolicy()
        with tempfile.TemporaryDirectory() as temporary:
            rows=[]
            for index,seed in enumerate([31,47]):
                row,_=on_policy_rollout(p,case,model,seed,Path(temporary)/str(index))
                self.assertTrue(row['budget_complete'])
                rows.append(row)
            returns=relative_returns([r['best_metrics']['score'] for r in rows])
            self.assertTrue(any(returns),'Test episodes must actually differ in terminal factory score')
            before=parameter_hash(model)
            losses=[]
            for row,advantage in zip(rows,returns):
                for ordinal,record in enumerate(unpack(Path(row['directory'])/'states.json.gz')):
                    state=record['state']
                    d=self.runtime.rpc('describe',state['current'],options={})
                    obs=observe(state['current'],d,state,1-ordinal/c['horizon'])
                    dist=distribution(model(obs),d['actions'],record['allowed'],c['balanced'])
                    loss,regularizer=loss_components(dist.log_prob(torch.tensor(record['index'])),dist.entropy(),advantage,2,4,c['entropy_coefficient'])
                    losses.append(loss+regularizer)
            expected=torch.autograd.grad(torch.stack(losses).sum(),list(model.parameters()))
            report=replay_gradients(p,case,model,rows,returns,lambda:None,unpack)
            self.assertEqual(report['replayed_actions'],8)
            self.assertEqual(report['max_log_probability_error'],0)
            self.assertGreater(report['reward_gradient_norm'],0)
            self.assertEqual(parameter_hash(model),before)
            for parameter,reference in zip(model.parameters(),expected):
                torch.testing.assert_close(parameter.grad,reference)
            with torch.no_grad():model.preferences[6]+=.5
            with self.assertRaisesRegex(ValueError,'behavior-identity'):
                replay_gradients(p,case,model,rows,returns,lambda:None,unpack)

    def test_partial_rollout_keeps_valid_incumbent_on_state_bound(self):
        c=read(Path(__file__).with_name('full-factory-on-policy-study.json'))
        c.update(episode_state_bytes=1,horizon=4,episode_seconds=30)
        p=dict(config=c,catalogs=str(Path(self.temp.name)/'catalogs.json'),
               native_library=os.environ.get('AIC_NATIVE_LIBRARY',str(ROOT/'target/release/libaic_pilot_native.dylib')))
        case=dict(self.case,instances=len(self.witness['placements']),family='test',split='train',transfer_group='mechanical-test')
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary)/'failed'
            with self.assertRaisesRegex(ValueError,'episode-state-bound'):
                on_policy_rollout(p,case,KindPolicy(),1,directory)
            self.assertEqual(read(directory/'partial.json')['code'],'on-policy.rollout-failed')
            self.assertTrue((directory/'best.json.gz').is_file())
            self.assertEqual(unpack(directory/'episode-partial.json.gz')['selected'],0)

    def test_full_raster_graph_model_replays_exact_sampling_probabilities(self):
        c=read(Path(__file__).with_name('full-factory-on-policy-study.json'))
        c.update(group_episodes=2,horizon=3,episode_seconds=30)
        p=dict(config=c,catalogs=str(Path(self.temp.name)/'catalogs.json'),
               native_library=os.environ.get('AIC_NATIVE_LIBRARY',str(ROOT/'target/release/libaic_pilot_native.dylib')))
        case=dict(self.case,instances=len(self.witness['placements']),family='test',split='train',transfer_group='mechanical-test')
        torch.manual_seed(17)
        model=FullFactoryPolicy(2)
        with tempfile.TemporaryDirectory() as temporary:
            rows=[on_policy_rollout(p,case,model,seed,Path(temporary)/str(seed))[0] for seed in [13,17]]
            self.assertTrue(all(r['budget_complete'] for r in rows))
            returns=relative_returns([r['best_metrics']['score'] for r in rows])
            result=replay_gradients(p,case,model,rows,returns,lambda:None,unpack)
            self.assertEqual(result['replayed_actions'],6)
            self.assertEqual(result['max_log_probability_error'],0)
            self.assertTrue(all(torch.isfinite(v.grad).all() for v in model.parameters()))
