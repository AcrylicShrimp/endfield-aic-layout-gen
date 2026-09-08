import unittest
from collections import Counter
import torch
from full_factory_ranking import training_pairs,ranking_loss


def row(name,root,kind,good,split='train',completed=True):
    return dict(id=name,root=root,family=root,action=dict(kind=kind),improving=good,split=split,completed=completed)


class RankingTests(unittest.TestCase):
    def test_pairs_exclude_development_and_incomplete_and_stay_on_root(self):
        rows=[row('a','one','pose',True),row('b','one','insert',False),
              row('late','one','contract',False,completed=False),row('late-good','one','port',True,completed=False),
              row('c','two','reroute',True),row('d','two','insert',False),
              row('e','dev','pose',True,'development'),row('f','dev','insert',False,'development')]
        pairs=training_pairs(rows);self.assertEqual(pairs,training_pairs(rows))
        self.assertEqual(Counter(p['kind'] for p in pairs),{'pose':128,'reroute':128})
        for p in pairs:self.assertIn((p['positive'],p['negative']),[('a','b'),('c','d')])

    def test_no_cross_root_negative_or_family_leakage(self):
        with self.assertRaisesRegex(ValueError,'no-observed-pairs'):
            training_pairs([row('a','one','pose',True),row('b','two','pose',False)])
        with self.assertRaisesRegex(ValueError,'family-leakage'):
            training_pairs([row('a','one','pose',True),row('b','one','pose',False,'development')])

    def test_update_prefers_positive_and_ignores_unobserved_action(self):
        logits=torch.tensor([-2.,2.,7.],requires_grad=True)
        loss=ranking_loss(logits,0,1);loss.backward()
        self.assertLess(float(logits.grad[0]),0);self.assertGreater(float(logits.grad[1]),0)
        self.assertEqual(float(logits.grad[2]),0)
        self.assertLess(float(ranking_loss(logits-.1*logits.grad,0,1).detach()),float(loss.detach()))


if __name__=='__main__':unittest.main()
