"""Scientific ranking, pairing, isolation and additive storage boundary checks."""
from copy import deepcopy
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path[:0]=[str(Path(__file__).resolve().parents[1]),str(Path(__file__).resolve().parent)]
from declarative_quality_analysis import aggregate,geometry_outcomes,paired,ranked,select
from declarative_quality_branch import PROFILES,read,sha,write
from declarative_quality_storage import Budget,inode,sync_fresh
from declarative_quality_study import bound,check_bound,disjoint,install_initial

STRATA={'small':['s'],'large':['l']}


def fixture():
    panel={'cases':[dict(geometry_identity=k,generation_seed=i,product=s[0],stratum=s,action_seed=100+i*100)
        for i,(k,s) in enumerate((('a','small'),('b','small'),('c','large')))]}
    rows=[dict(geometry_identity=c['geometry_identity'],generation_seed=c['generation_seed'],product=c['product'],
        sample=i,action_seed=c['action_seed']+i,factory_validated=True,start_score=[100],best_score=[100-gain],
        total_area_reduction_percent=gain) for c in panel['cases'] for i,gain in enumerate({'a':[0,100],'b':[0,0],'c':[100,100]}[c['geometry_identity']])]
    return panel,rows


class StudyTests(unittest.TestCase):
    def test_geometry_then_equal_strata_then_training_replicates(self):
        panel,rows=fixture();one=geometry_outcomes(rows,panel,STRATA);two=deepcopy(one)
        for r in two.values():r.update(mean=0,action_replicates=[0,0])
        result=aggregate([one,two],STRATA)
        self.assertEqual(result['by_stratum'],{'small':12.5,'large':50.})
        self.assertEqual(result['mean'],31.25)
    def test_saturation_does_not_prevent_selection(self):
        scores={p:100. for p in PROFILES};self.assertEqual(select(scores),['gain-8','geometry-8'])
    def test_select_two_even_below_baseline(self):
        scores={p:-1. for p in PROFILES};scores.update({'baseline-8':10.,'uniform-32':2.,'uniform-16':1.})
        self.assertEqual(select(scores),['uniform-32','uniform-16'])
    def test_quantity_only_breaks_actual_ties(self):
        self.assertEqual(ranked({'uniform-16':1.,'uniform-32':1.000001}),['uniform-32','uniform-16'])
        self.assertEqual(ranked({'uniform-16':1.,'uniform-32':1.}),['uniform-16','uniform-32'])
        self.assertEqual(ranked({'score-8':1.,'protected-score-8':1.}),['score-8','protected-score-8'])
    def test_all_arms_and_finite_scores_required(self):
        with self.assertRaises(ValueError):select({'baseline-8':100})
        with self.assertRaises(ValueError):ranked({'score-8':float('nan')})
    def test_duplicate_missing_rng_invalid_or_fabricated_gain_rejected(self):
        panel,rows=fixture()
        for change in ('duplicate','missing','seed','gain','invalid'):
            bad=deepcopy(rows)
            if change=='duplicate':bad[-1]=bad[-2]
            elif change=='missing':bad.pop()
            elif change=='seed':bad[0]['action_seed']+=1
            elif change=='gain':bad[0]['total_area_reduction_percent']=10
            else:bad[0]['factory_validated']=False
            with self.subTest(change=change),self.assertRaises(ValueError):geometry_outcomes(bad,panel,STRATA)
    def test_stratified_bootstrap_keeps_geometry_repeats_paired(self):
        panel,rows=fixture();base=geometry_outcomes(rows,panel,STRATA);candidate=deepcopy(base)
        for r in candidate.values():r['mean']+=2
        a=aggregate([base,base],STRATA);b=aggregate([candidate,candidate],STRATA)
        result=paired(b,a,STRATA,resamples=100)
        self.assertEqual(result['interval_95_pp'],[2.,2.]);self.assertEqual(result['mean_delta_pp'],2.)
        self.assertEqual(result['geometry_outcomes'],{'win':3})
        with self.assertRaises(ValueError):paired(b,aggregate([base],STRATA),STRATA,resamples=100)
    def test_panel_training_and_panel_panel_leakage_rejected(self):
        panel,_=fixture();disjoint([panel],{})
        with self.assertRaises(ValueError):disjoint([panel],{'0':{'geometry_sha256':'a'}})
        with self.assertRaises(ValueError):disjoint([panel,panel],{})
    def test_changed_selection_receipt_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp)/'selection.json';write(p,{'selected':['score-8','gain-8']});ref=bound(p)
            write(p,{'selected':['priority-8','gain-8']})
            with self.assertRaises(ValueError):check_bound(ref)
    def test_old_and_new_hardlinks_charged_once_new_versions_conservative(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);old=root/'old';old.write_bytes(b'x'*20);new=root/'new'
            budget=Budget(root/'budget.json',100,[inode(old)[0]])
            os.link(old,root/'old-link');budget.observe(root/'old-link');self.assertEqual(budget.value['bytes'],0)
            new.write_bytes(b'y'*30);budget.observe(new);os.link(new,root/'new-link');budget.observe(root/'new-link')
            self.assertEqual(budget.value['bytes'],30)
            new.unlink();new.write_bytes(b'z'*40);budget.observe(new);self.assertEqual(budget.value['bytes'],70)
            budget.save();self.assertEqual(Budget(budget.path).value['bytes'],70)
            with self.assertRaises(ValueError):budget.observe(self.extra(root))
    def extra(self,root):
        p=root/'extra';p.write_bytes(b'x'*31);return p
    def test_initial_evaluation_reuse_requires_bound_model_and_panel(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);write(root/'launch.json',{'panel':{'path':'p','sha256':'h'}})
            write(root/'quality-branch.json',{'preserved':{'model':'m'},'first_update':8})
            cache=dict(panel={'path':'other','sha256':'h'},model_sha256='m',update=8)
            with self.assertRaises(ValueError):install_initial(root,cache)
    def test_fresh_slot_conflict_rejected_and_inherited_stays_private(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);branch=root/'branch';bank=branch/'inputs';shared=root/'shared'
            episode=bank/'episode-00000032/episode.json';write(episode,{'actual':1})
            job={'inputs':{'32':dict(reference={'path':'episode-00000032/episode.json','sha256':sha(episode)},
                start_curriculum={'depth':0},input_sha256='i',geometry_sha256='g'),
                '33':dict(start_curriculum={'depth':1})}}
            write(branch/'launch.json',{'config':{'seeds':[29],'training_input':{'directory':str(bank)}}})
            write(branch/'seed-29/state.json',job)
            rows=sync_fresh(branch,shared,32,lambda:None);self.assertEqual([r['slot'] for r in rows],[32])
            self.assertEqual(inode(episode),inode(shared/'episode-00000032/episode.json'))
            write(shared/'episode-00000032/episode.json',{'actual':2})
            with self.assertRaises(ValueError):sync_fresh(branch,shared,32,lambda:None)


if __name__=='__main__':unittest.main()
