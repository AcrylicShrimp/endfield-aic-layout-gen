import unittest
import torch
from full_factory_best_learning import teachers,schedule,best_mass_loss


def row(name,score,kind='pose',root='one',split='train',completed=True,good=True):
    return dict(id=name,root=root,family=root,split=split,action=dict(kind=kind),completed=completed,improving=good,
                best_metrics=dict(score=score))


class BestLearningTests(unittest.TestCase):
    def test_full_score_prefers_area_despite_more_wiring_and_preserves_exact_ties(self):
        rows=[row('area',[9,100,20]),row('tie',[9,100,20]),row('wire',[10,1,0],'port'),
              row('less-turns',[9,100,19]),row('late',[8,1,0],completed=False)]
        result=teachers(rows)[0]
        self.assertEqual(result['targets'],['less-turns'])
        self.assertEqual(set(result['worse']),{'area','tie','wire'})
        self.assertEqual(teachers(rows[:3])[0]['targets'],['area','tie'])

    def test_updates_exclude_development_missing_and_same_score_alternatives(self):
        rows=[row('a',[9,9,9]),row('b',[10,1,0],'port'),row('same',[9,9,9]),
              row('dev',[1,1,1],root='dev',split='development'),row('missing',[10,1,0],root='empty',good=False)]
        lessons=teachers(rows);steps=schedule(lessons)
        self.assertEqual(steps,schedule(lessons));self.assertEqual(len(steps),256)
        for s in steps:
            self.assertEqual(s['root'],'one');self.assertIn(s['positive'],['a','same']);self.assertEqual(s['negative'],'b')
        with self.assertRaisesRegex(ValueError,'family-leakage'):
            teachers(rows+[row('leak',[8,1,0],split='development')])

    def test_mass_loss_accepts_any_tied_best_and_reduces_competitor_probability(self):
        logits=torch.tensor([1.,1.,4.],requires_grad=True)
        loss=best_mass_loss(logits,[0,1]);loss.backward()
        self.assertLess(float(logits.grad[0]),0);self.assertLess(float(logits.grad[1]),0);self.assertGreater(float(logits.grad[2]),0)
        self.assertAlmostEqual(float(best_mass_loss(logits,[1,0]).detach()),float(loss.detach()))
        self.assertEqual(float(best_mass_loss(logits,[0,1,2]).detach()),0)
        for targets in ([],[0,0]):
            with self.assertRaises(ValueError):best_mass_loss(logits,targets)


if __name__=='__main__':unittest.main()
