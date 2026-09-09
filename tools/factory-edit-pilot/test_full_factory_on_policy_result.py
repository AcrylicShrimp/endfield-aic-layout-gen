"""Terminal accounting preserves unapplied episodes and rejects false completion."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

from full_factory_on_policy_result import build_result
from full_factory_screen import sha,write


class TerminalResultTests(unittest.TestCase):
    def fixture(self,screen,failed=None,count=1,pending=1):
        config=dict(initializations=['random','imitation-warm'],groups=3,group_episodes=2,horizon=2,arm_seconds=10)
        schedule=[dict(group=g,case='factory',sample_seeds=[100+2*g+i for i in range(2)]) for g in range(3)]
        p=dict(schema_version=1,config=config,schedule=schedule)
        names=config['initializations'] if failed is None else config['initializations'][:config['initializations'].index(failed)+1]
        rows=[];groups=[];arms=[]
        def file(path):
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_bytes(str(path).encode())
            return str(path.resolve())
        def episode(name,g,i):
            directory=screen/name/f'group-{g:04d}'/f'episode-{i}'
            for f in ['episode.json.gz','states.json.gz','best.json.gz']:
                file(directory/f)
            return dict(initialization=name,group=g,attempt=i,phase='training',case='factory',bank=100+2*g+i,
                        budget_complete=True,selected=2,directory=str(directory.resolve()),best_metrics=dict(score=[100-g,i,1]))
        for name in names:
            initial=Path(file(screen/name/'initial.pt'))
            n=count if name==failed else 3
            for g in range(n):
                episodes=[episode(name,g,i) for i in range(2)]
                rows+=episodes
                checkpoint=Path(file(screen/name/f'group-{g:04d}'/'checkpoint.pt'))
                group=dict(initialization=name,group=g,case='factory',sample_seeds=schedule[g]['sample_seeds'],
                           scores=[r['best_metrics']['score'] for r in episodes],checkpoint=str(checkpoint),checkpoint_sha256=sha(checkpoint),
                           flat_returns=False,reward_gradient_norm=1,seconds=.1)
                write(checkpoint.with_name('group.json'),group)
                groups.append(group)
            if name!=failed:
                final=Path(file(screen/name/'final.pt'))
                arms.append(dict(initialization=name,groups=3,initial=str(initial),initial_sha256=sha(initial),final=str(final),
                                 final_sha256=sha(final),seconds=1,parameters=10,nonflat_groups=3,reward_gradient_groups=3))
            else:
                rows += [episode(name,n,i) for i in range(pending)]
        r=dict(schema_version=1,status='incomplete' if failed else 'complete',rows=rows,groups=groups,arms=arms)
        if failed:
            r.update(code='on-policy.training-failed',stage='training',initialization=failed,group=count,message=f'on-policy.arm-budget: {failed}')
        else:
            r.update(protocol_sha256='protocol',teacher_labels_used=False,future_evaluation_opened=False,seconds=2)
        source=screen/('partial.json' if failed else 'training-report.json')
        write(source,r)
        return p,r,source

    def build(self,p,r,screen,source):
        write(source,r)
        return build_result(p,r,screen,source,'protocol')

    def test_complete_endpoints_and_full_accounting(self):
        with tempfile.TemporaryDirectory() as d:
            screen=Path(d)
            p,r,source=self.fixture(screen)
            result=self.build(p,r,screen,source)
            self.assertEqual(result['status'],'complete')
            self.assertTrue(result['training_complete'])
            self.assertEqual((result['completed_groups'],result['completed_group_episodes'],result['unapplied_episodes']),(6,12,0))
            self.assertEqual([a['terminal_role'] for a in result['arms']],['planned_final']*2)
            self.assertEqual(result['sampled_selected_actions'],result['planned_selected_actions'])

    def test_budget_endpoint_excludes_unapplied_episodes_without_dropping_them(self):
        with tempfile.TemporaryDirectory() as d:
            screen=Path(d)
            p,r,source=self.fixture(screen,failed='imitation-warm',count=1,pending=1)
            original=deepcopy(r)
            result=self.build(p,r,screen,source)
            self.assertEqual(r,original)
            self.assertEqual(result['status'],'budget_exhausted')
            self.assertFalse(result['training_complete'])
            self.assertEqual((result['completed_groups'],result['completed_group_episodes'],result['unapplied_episodes']),(4,8,1))
            self.assertEqual((result['sampled_selected_actions'],result['gradient_replayed_selected_actions'],result['planned_selected_actions']),(18,16,24))
            arm=result['arms'][1]
            self.assertEqual(arm['terminal'],r['groups'][-1]['checkpoint'])
            self.assertEqual(arm['terminal_role'],'last_completed_update')
            self.assertIsNone(arm['seconds'])
            self.assertEqual(arm['elapsed_seconds_lower_bound'],10)

    def test_zero_update_endpoint_and_unstarted_arm(self):
        with tempfile.TemporaryDirectory() as d:
            screen=Path(d)
            p,r,source=self.fixture(screen,failed='random',count=0,pending=0)
            result=self.build(p,r,screen,source)
            self.assertEqual(result['unstarted_initializations'],['imitation-warm'])
            arm=result['arms'][0]
            self.assertEqual(arm['terminal'],arm['initial'])
            self.assertEqual(arm['terminal_role'],'initial_no_update')
            self.assertEqual(result['completed_groups'],0)

    def test_full_sampled_group_can_remain_unapplied(self):
        with tempfile.TemporaryDirectory() as d:
            screen=Path(d)
            p,r,source=self.fixture(screen,failed='imitation-warm',count=1,pending=2)
            result=self.build(p,r,screen,source)
            self.assertEqual(result['unapplied_episodes'],2)
            self.assertEqual(result['arms'][1]['groups'],1)

    def test_rejects_unsupported_failure_and_inconsistent_prefixes(self):
        with tempfile.TemporaryDirectory() as d:
            screen=Path(d)
            p,r,source=self.fixture(screen,failed='imitation-warm',count=1,pending=1)
            variants=[]
            bad=deepcopy(r);bad['message']='on-policy.nonfinite-clipping';variants.append(bad)
            bad=deepcopy(r);bad['groups'].pop(0);variants.append(bad)
            bad=deepcopy(r);bad['rows'].append(deepcopy(bad['rows'][-1]));variants.append(bad)
            bad=deepcopy(r);bad['rows'].pop(1);variants.append(bad)
            bad=deepcopy(r);bad['rows'][-1]['budget_complete']=False;variants.append(bad)
            bad=deepcopy(r);bad['rows'][-1]['bank']+=1;variants.append(bad)
            bad=deepcopy(r);bad['group']=2;variants.append(bad)
            for bad in variants:
                with self.subTest(report=bad.get('message')),self.assertRaises(ValueError):
                    self.build(p,bad,screen,source)

    def test_rejects_missing_or_unexpected_checkpoint_and_episode_files(self):
        for kind in ['missing-checkpoint','extra-checkpoint','missing-episode','extra-episode']:
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as d:
                screen=Path(d)
                p,r,source=self.fixture(screen,failed='imitation-warm',count=1,pending=1)
                if kind=='missing-checkpoint':Path(r['groups'][-1]['checkpoint']).unlink()
                if kind=='extra-checkpoint':(screen/'imitation-warm/final.pt').write_bytes(b'unexpected')
                if kind=='missing-episode':(Path(r['rows'][-1]['directory'])/'states.json.gz').unlink()
                if kind=='extra-episode':(screen/'imitation-warm/group-0001/episode-1').mkdir()
                with self.assertRaises(ValueError):self.build(p,r,screen,source)


if __name__=='__main__':
    unittest.main()
