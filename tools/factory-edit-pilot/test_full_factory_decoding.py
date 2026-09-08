import unittest
import torch
from full_factory_decoding import TemperaturePolicy
from full_factory_selection import distribution


class DecodingTests(unittest.TestCase):
    def test_history_mask_can_exclude_highest_scaled_logit(self):
        actor=TemperaturePolicy(torch.nn.Identity(),.05)
        logits=actor(torch.tensor([3.,2.,1.]))
        actions=[{'kind':'contract'}]*3
        d=distribution(logits,actions,[False,True,True],False)
        self.assertEqual(float(d.probs[0]),0)
        self.assertGreater(float(d.probs[1]),.999)

    def test_equal_logits_remain_equal_and_parameters_do_not_change(self):
        model=torch.nn.Linear(2,2,bias=False)
        with torch.no_grad():model.weight.fill_(1)
        before=model.weight.clone()
        logits=TemperaturePolicy(model,.25)(torch.ones(2))
        self.assertTrue(torch.equal(logits[0],logits[1]))
        self.assertTrue(torch.equal(before,model.weight))

    def test_invalid_temperature_fails(self):
        for t in (0,-1,float('nan'),float('inf')):
            with self.assertRaises(ValueError):TemperaturePolicy(torch.nn.Identity(),t)


if __name__=='__main__':unittest.main()
