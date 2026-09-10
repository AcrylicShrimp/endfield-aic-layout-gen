"""A new schedule must retain parent learning state without changing its evidence."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

import torch

from ppo_harness_continuation import PROTECTED_SOURCES, prepare_parent, restore_parent
from ppo_harness_state import SCHEMA, HarnessError, write, read, sha, save_checkpoint
from random_factory_audit import optimizer_hash


def learners():
    torch.manual_seed(17)
    actor, critic = torch.nn.Linear(3,2), torch.nn.Linear(3,1)
    aopt,copt=(torch.optim.Adam(m.parameters(),lr=.01) for m in (actor,critic))
    return actor,critic,aopt,copt,torch.Generator().manual_seed(29)


def update(values):
    actor,critic,aopt,copt,rng=values
    x=torch.randn(4,3)[torch.randperm(4,generator=rng)]
    for model,opt in ((actor,aopt),(critic,copt)):
        opt.zero_grad(); model(x).square().sum().backward(); opt.step()


class ContinuationTests(unittest.TestCase):
    def fixture(self, root):
        asset=root/'asset.json';write(asset,dict(data='unchanged'))
        cfg=dict(candidates=['current-history-16'],seeds=[0],ppo=dict(lr=.01),execution=dict(device='cpu'),
                 training=dict(batches=1,attempts=2,horizon=32,generation_seed_base=100,action_seed_base=200),
                 evaluation=dict(attempts=2,generation_seed_base=500,action_seed_base=600))
        spec=dict(policy=dict(depth=2,width=32),selection=dict(horizon=32),generation={},
                  products=[dict(id='fixture',source=str(asset))],native_library=str(asset),cli=str(asset),validator=str(asset),
                  catalogs=dict(fixture=str(asset)),frozen_starts=dict(evaluation_seeds=[500,501],entries={
                      str(seed):dict(status='rejected',files={}) for seed in [500,501]}))
        sources={str(Path(__file__).with_name(name)):sha(Path(__file__).with_name(name)) for name in PROTECTED_SOURCES}
        protocol=dict(schema=SCHEMA,backend='retained-full-factory-native',sources=sources,
                      config=cfg,spec=spec,environment={})
        write(root/'protocol.json',protocol)
        original=learners();update(original)
        checkpoint=save_checkpoint(root,root/'checkpoint.pt',original,sha(root/'protocol.json'),'job',1)
        write(root/'state.json',dict(schema=SCHEMA,protocol_sha256=sha(root/'protocol.json'),status='complete',jobs=[
              dict(id='job',candidate='current-history-16',seed=0,completed_batches=1,selected_training_actions=64,
                   checkpoint=checkpoint,records=[])]))
        following=deepcopy(cfg);following['input_manifest_scope']='evaluation'
        following['training'].update(generation_seed_base=102,action_seed_base=202)
        return following,deepcopy(spec),original

    def test_full_state_transfer_preserves_next_update_and_parent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);config,spec,first=self.fixture(root)
            identities={p:sha(p) for p in root.iterdir()}
            parent,paths=prepare_parent(root,spec,config,{})
            self.assertEqual(parent['total_batches'],1)
            self.assertEqual(parent['total_training_actions'],64)
            self.assertEqual(len(paths),3)
            update(first);expected_rng=torch.get_rng_state().clone()
            second=learners();restore_parent(parent,second);update(second)
            for a,b in zip(first[:2],second[:2]):
                for actual,expected in zip(a.parameters(),b.parameters()):
                    self.assertTrue(torch.equal(actual,expected))
            self.assertEqual(optimizer_hash(first[2]),optimizer_hash(second[2]))
            self.assertEqual(optimizer_hash(first[3]),optimizer_hash(second[3]))
            self.assertTrue(torch.equal(first[4].get_state(),second[4].get_state()))
            self.assertTrue(torch.equal(expected_rng,torch.get_rng_state()))
            self.assertTrue(all(sha(p)==h for p,h in identities.items()))

    def test_drift_contract_and_consumed_seed_overlap_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);config,spec,_=self.fixture(root)
            changed=deepcopy(config);changed['ppo']['lr']=.02
            with self.assertRaisesRegex(HarnessError,'continuation-contract'):
                prepare_parent(root,spec,changed,{})
            changed=deepcopy(config);changed['training']['generation_seed_base']=101
            with self.assertRaisesRegex(HarnessError,'continuation-training-overlap'):
                prepare_parent(root,spec,changed,{})
            (root/'job/training-0002/episode-0000').mkdir(parents=True)
            write(root/'job/training-0002/episode-0000/logical-task.json',dict(identity=dict(batch=2,index=0)))
            with self.assertRaisesRegex(HarnessError,'continuation-training-overlap'):
                prepare_parent(root,spec,config,{})
            config['training'].update(generation_seed_base=103,action_seed_base=203)
            parent,_=prepare_parent(root,spec,config,{})
            state=read(root/'state.json');state['changed']=True;write(root/'state.json',state)
            with self.assertRaisesRegex(HarnessError,'continuation-parent-drift'):
                restore_parent(parent,learners())


if __name__=='__main__':
    unittest.main()
