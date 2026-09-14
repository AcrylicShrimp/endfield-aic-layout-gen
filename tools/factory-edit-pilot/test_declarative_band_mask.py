"""Recorded masks must survive PPO/SIL likelihood, storage and live restoration."""
from copy import deepcopy
import os
from pathlib import Path
import tempfile
import unittest

import torch
from blueprint_ppo_batch import distribution
from blueprint_ppo_store import load, save
from declarative_band_mask import MODE, ROOT_MASK_SCHEMA
from declarative_learning_environment import Runtime
import declarative_learning_policy as policy
import test_declarative_learning as existing


class MaskDistributionTests(unittest.TestCase):
    def test_zero_invalid_gradient_finite_entropy_and_uniform_mask(self):
        logits = torch.tensor([1., 3., 2.], requires_grad=True)
        batch = dict(size=1, action_graph=torch.zeros(3,dtype=torch.long),
            kind_log_counts=torch.zeros(3), root_invalid=torch.tensor([False,True,False]))
        lp, entropy = policy.root_distribution(logits,batch)
        (-lp[0] - .001*entropy.sum()).backward()
        self.assertEqual(float(logits.grad[1]),0.)
        self.assertTrue(bool(torch.isfinite(logits.grad).all()))
        uniform, entropy = policy.root_distribution(logits,batch,uniform=True)
        torch.testing.assert_close(uniform.exp(),torch.tensor([.5,0.,.5]))
        self.assertTrue(bool(torch.isfinite(entropy).all()))

    def test_disabled_path_is_exact_original_distribution(self):
        logits = torch.tensor([1.,3.,2.])
        batch = dict(size=1,action_graph=torch.zeros(3,dtype=torch.long),kind_log_counts=torch.tensor([0.,1.,1.]))
        for left,right in zip(distribution(logits,batch),policy.root_distribution(logits,batch),strict=True):
            self.assertTrue(torch.equal(left,right))


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'),'selected release native library required')
class MaskedNativeLearningTests(existing.NativeLearningTests):
    # Reuse the existing preparation-credit, live stream restore, imitation,
    # nonempty-Adam replay and actor-safe-KL tests under the masked input contract.
    def setUp(self):
        super().setUp()
        self.config['actions']['masking'] = MODE
        self.runtime = Runtime(self.config,os.environ['AIC_NATIVE_LIBRARY'])

    def test_recorded_domain_storage_and_invalid_teacher_rejection(self):
        _,obs = self.observation(); prepared = self.runtime.prepare(obs)
        self.assertEqual(prepared['root_mask_schema'],ROOT_MASK_SCHEMA)
        self.assertTrue(bool(prepared['root_invalid'].any()))
        with torch.no_grad():
            row = dict(input=prepared,**policy.sample(self.model,[prepared],torch.Generator().manual_seed(711))[0])
            before = policy.predict(self.model,policy.prepare_rows([row]))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'record.pt.gz';save(path,row);restored = load(path)
        with torch.no_grad():
            after = policy.predict(self.model,policy.prepare_rows([restored]))
        for a,b in zip(before,after,strict=True):
            self.assertTrue(torch.equal(a,b))
        invalid = int(prepared['root_invalid'].nonzero()[0])
        forged = dict(input=prepared,decision=dict(root=invalid,ports=[]))
        with self.assertRaisesRegex(ValueError,'masking.label'):
            policy.prepare_rows([forged])
        with self.assertRaisesRegex(ValueError,'masking.action'):
            policy.action(prepared,forged['decision'])
        malformed = deepcopy(prepared);malformed['root_mask_schema']='unknown'
        with self.assertRaisesRegex(ValueError,'masking.input'):
            policy.pack([malformed])
        malformed = deepcopy(prepared);malformed['root_invalid'] = torch.ones_like(prepared['root_invalid'])
        with self.assertRaisesRegex(ValueError,'masking.input'):
            policy.pack([malformed])

    @unittest.skipUnless(torch.cuda.is_available(),'CUDA likelihood and imitation check')
    def test_cuda_masked_sampling_teacher_and_imitation(self):
        _,obs = self.observation(); prepared = self.runtime.prepare(obs)
        model = deepcopy(self.model).to('cuda:0')
        with torch.no_grad():
            chosen = policy.sample(model,[prepared]*8,torch.Generator().manual_seed(719))
            rows = [dict(input=prepared,**row) for row in chosen]
            packed = policy.device_rows(policy.prepare_rows(rows),'cuda:0')
            lp,entropy,_ = policy.predict(model,packed)
            torch.testing.assert_close(lp.cpu(),torch.tensor([r['old_logp'] for r in chosen]),rtol=1e-5,atol=2e-6)
            self.assertTrue(bool(torch.isfinite(entropy).all()))
        before = lp.detach().mean().item()
        optimizer = torch.optim.Adam([p for n,p in model.named_parameters() if not n.startswith('critic.')],lr=.001)
        for _ in range(3):
            optimizer.zero_grad(set_to_none=True)
            loss = -policy.predict(model,packed)[0].mean();loss.backward();optimizer.step()
        with torch.no_grad():
            self.assertGreater(policy.predict(model,packed)[0].mean().item(),before)


if __name__=='__main__':
    unittest.main()
