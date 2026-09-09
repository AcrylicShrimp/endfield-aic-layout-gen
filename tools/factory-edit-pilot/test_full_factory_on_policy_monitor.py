"""Training curves must compare fixed cases and completed policies consistently."""
from copy import deepcopy
import unittest

from full_factory_on_policy_monitor import aggregate


def fixture():
    protocol=dict(training_cases=['small','large'],cases=[dict(id='small',instances=2,transfer_group='raw'),
        dict(id='large',instances=20,transfer_group='prepared')],config=dict(group_episodes=2,horizon=3,initializations=['random','imitation-warm']))
    return protocol,dict(rows=[],groups=[])


def add(report,group,case,initial,finals,arm='random'):
    for attempt,score in enumerate(finals):
        report['rows'].append(dict(initialization=arm,group=group,case=case,attempt=attempt,budget_complete=True,selected=3,
            initial_metrics=dict(score=initial),best_metrics=dict(score=score,empty_xy_cells=1),improvements={}))
    report['groups'].append(dict(initialization=arm,group=group,case=case,scores=finals,seconds=12,
        reward_gradient_norm=.1,entropy_gradient_norm=.001))


class OutcomeMonitorTests(unittest.TestCase):
    def test_case_separation_balanced_cycles_and_pending_episode_exclusion(self):
        protocol,report=fixture()
        add(report,0,'small',[100,10,1],[[90,10,1],[90,10,1]])
        add(report,1,'large',[1000,20,2],[[500,20,2],[500,20,2]])
        add(report,2,'small',[100,10,1],[[80,10,1],[80,10,1]])
        report['rows'].append(dict(initialization='random',group=3))
        actual=aggregate(protocol,report)
        self.assertEqual(actual['pending_group_episodes'],1)
        self.assertEqual(len(actual['balanced_complete_cycles']),1)
        self.assertAlmostEqual(actual['balanced_complete_cycles'][0]['mean_area_reduction_percent'],30)
        small=next(c for c in actual['curves'] if c['case']=='small')
        self.assertEqual([p['updates_before_sampling'] for p in small['points']],[0,2])
        self.assertAlmostEqual(small['latest_minus_first_mean_area_reduction_pp'],10)
        self.assertEqual(small['points'][1]['completed_group_minutes_before_sampling'],.4)

    def test_transport_only_improvement_is_visible_without_an_area_gain(self):
        protocol,report=fixture()
        add(report,0,'small',[100,10,2],[[100,9,100],[100,10,1]])
        point=aggregate(protocol,report)['curves'][0]['points'][0]
        self.assertEqual(point['mean_area_reduction_percent'],0)
        self.assertEqual(point['exact_score_improved_episodes'],2)
        report['rows'][0]['best_metrics']['score']=[101,0,0]
        report['groups'][0]['scores'][0]=[101,0,0]
        with self.assertRaisesRegex(ValueError,'incumbent-regression'):
            aggregate(protocol,report)

    def test_completed_group_integrity_and_initialization_separation(self):
        protocol,report=fixture()
        add(report,0,'small',[100,10,1],[[90,10,1],[80,10,1]])
        add(report,0,'small',[100,10,1],[[70,10,1],[60,10,1]],arm='imitation-warm')
        actual=aggregate(protocol,report)
        self.assertEqual([c['initialization'] for c in actual['curves']],['random','imitation-warm'])
        self.assertEqual([c['points'][0]['completed_group_minutes_before_sampling'] for c in actual['curves']],[0,0])
        malformed=deepcopy(report)
        malformed['rows'][0]['selected']=2
        with self.assertRaisesRegex(ValueError,'group-episodes'):
            aggregate(protocol,malformed)
        malformed=deepcopy(report)
        malformed['groups'][0]['group']=1
        with self.assertRaisesRegex(ValueError,'group-membership'):
            aggregate(protocol,malformed)


if __name__=='__main__':
    unittest.main()
