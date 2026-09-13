"""Paired sampling and budget-prefix attribution must not bias the experiment."""
import unittest

from declarative_action_sampling import draw, port_choices
from declarative_action_compare import summaries
import random


class ExperimentTests(unittest.TestCase):
    def test_extra_decisions_do_not_change_common_spatial_draws_or_reuse_ports(self):
        observation=dict(facilities=[dict(instance='a',x=5,y=5,rotation=0),dict(instance='b',x=12,y=8,rotation=0)],
            allowed_rotations={'a':[0,90],'b':[0,90]},port_assignments=['p','q'],
            blueprint=dict(nodes=[dict(id=0,terminal=dict(endpoint=dict(instance='a'))),
                                  dict(id=1,terminal=dict(endpoint=dict(instance='a')))]),
            port_options=[dict(node=i,instance='a',ports=['p','q','r']) for i in range(2)])
        options=dict(other_facility_origin_probability=.25,translation_steps=[-2,-1,1,2])
        for index in range(64):
            common=draw(observation,observation,'automatic_common',17,index,options)
            rotated=draw(observation,observation,'automatic_rotation',17,index,options)
            scripted=draw(observation,observation,'legacy_scripted',17,index,options)
            for key in ('instance','x','y'):
                self.assertEqual(common[key],rotated[key])
                self.assertEqual(common[key],scripted[key])
            ports=port_choices(observation,'a',random.Random(index))
            self.assertEqual({r['node'] for r in ports},{0,1})
            self.assertEqual(len({r['port'] for r in ports}),2)

    def test_prefix_does_not_credit_an_improvement_completed_after_the_budget(self):
        work=dict(route_expansions=12,transition_seconds=.002)
        trace=[dict(decision=1,action=dict(kind='place'),outcomes=[dict(status='improved',diagnostics=[])],
            best_score=[90,10,2],cumulative=work)]
        row=dict(arm='automatic_common',improved=True,score_improved=True,decisions=1,initial_score=[100,10,2],
            final_score=[90,10,2],final_pending=0,final_conflicts=0,work=work,trace=trace,first_improvement=dict(decision=1))
        cases=[dict(case=dict(index=0,product='diagnostic'),sequential=[row],independent=[])]
        options=dict(arms=['automatic_common'],independent_arms=[],decision_prefixes=[1],routing_prefixes=[11,12],
                     time_prefixes_seconds=[.001,.002])
        result=summaries(cases,options)['sequential/automatic_common']['prefixes']
        self.assertEqual(result['route_expansions/11']['improved'],0)
        self.assertEqual(result['route_expansions/12']['improved'],1)
        self.assertEqual(result['transition_seconds/0.001']['improved'],0)
        self.assertEqual(result['transition_seconds/0.002']['improved'],1)


if __name__=='__main__':
    unittest.main()
