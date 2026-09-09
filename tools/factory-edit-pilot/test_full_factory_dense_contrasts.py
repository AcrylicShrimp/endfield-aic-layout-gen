import unittest
from full_factory_dense_contrasts import alternatives


class DenseContrastSelectionTests(unittest.TestCase):
    def test_diverse_reroutes_alternate_long_short_and_do_not_borrow_quota(self):
        witness=dict(placements=[dict(instance='a',x=0,y=0,width=2,height=2,rotation=0),
                                 dict(instance='b',x=10,y=0,width=2,height=2,rotation=0)],
                     logistics_components=[],transport_networks=[])
        lengths=[10,2,6,9,1,5]
        description=dict(chains=[dict(cells=[None]*n) for n in lengths],
            actions=[dict(kind='reroute',chain=i,network=i//3) for i in range(6)]+[dict(kind='restore_best')])
        config=dict(seed=17,pose_quotas=dict(local_single=2,distant_single=2,group_translation=2,single_rotation=2,compound_rotation=2),
                    other_quotas=dict(reroute=4,port=4,restore_best=1))
        selected,populations=alternatives(description,witness,config)
        self.assertEqual(selected,alternatives(description,witness,config)[0]);self.assertEqual(len(selected),5)
        self.assertEqual(len({row['vocabulary_index'] for row in selected}),5)
        route_rows=[row for row in selected if row['stratum']=='reroute']
        self.assertEqual({lengths[row['action']['chain']] for row in route_rows[:2]},{10,9})
        self.assertEqual({lengths[row['action']['chain']] for row in route_rows[2:]},{2,1})
        self.assertEqual(populations['port']['selected'],0);self.assertEqual(populations['local_single']['selected'],0)

    def test_port_selection_prefers_near_options_for_distinct_terminals(self):
        witness=dict(placements=[dict(instance='a',x=0,y=0,width=2,height=2,rotation=0)],logistics_components=[],
            transport_networks=[dict(terminals=[dict(id='a',position=dict(x=0,y=0)),dict(id='b',position=dict(x=10,y=0))])])
        description=dict(chains=[],actions=[dict(kind='port',terminal=t,x=x+dx,y=0) for t,x in [('a',0),('b',10)] for dx in [4,1]])
        config=dict(seed=9,pose_quotas=dict(local_single=1,distant_single=1,group_translation=1,single_rotation=1,compound_rotation=1),other_quotas=dict(port=2))
        selected,_=alternatives(description,witness,config)
        self.assertEqual({row['action']['terminal'] for row in selected},{'a','b'})
        self.assertEqual({row['action']['x'] for row in selected},{1,11})


if __name__=='__main__':unittest.main()
