import unittest
from full_factory_pose_teachers import selected_poses, stratum, displacement


class PoseTeacherSelectionTests(unittest.TestCase):
    def setUp(self):
        self.witness=dict(placements=[dict(instance=str(i),x=i*10,y=0,width=2,height=2,rotation=0) for i in range(4)])

    def pose(self,members,dx=1,rotation=0):
        return dict(kind='pose',poses=[dict(member=i,x=self.witness['placements'][i]['x']+dx,y=0,rotation=rotation) for i in members])

    def test_strata_and_round_robin_keep_groups_before_second_choices(self):
        groups=[self.pose([0]),self.pose([0],2),self.pose([1]),self.pose([1],2)]
        actions=groups+[self.pose([2],5),self.pose([0,1]),self.pose([0],0,90),self.pose([0,1],0,90),dict(kind='contract')]
        quotas=dict(local_single=2,distant_single=1,group_translation=1,single_rotation=1,compound_rotation=1)
        selected,report=selected_poses(dict(actions=actions),self.witness,quotas,123)
        self.assertEqual(selected,selected_poses(dict(actions=actions),self.witness,quotas,123)[0])
        self.assertEqual(len(selected),6);self.assertEqual(len({r['vocabulary_index'] for r in selected}),6)
        local=[r['action']['poses'][0] for r in selected if r['stratum']=='local_single']
        self.assertEqual({q['member'] for q in local},{0,1})
        self.assertTrue(all(q['x']-self.witness['placements'][q['member']]['x']==1 for q in local))
        self.assertEqual(report['local_single']['population'],4)
        for r in selected:self.assertEqual(r['stratum'],stratum(r['action'],self.witness))

    def test_canonical_frame_shift_is_not_a_facility_translation(self):
        after=dict(placements=[dict(p,x=p['x']+7,y=p['y']+4) for p in self.witness['placements']])
        self.assertEqual(displacement(self.witness,after,self.pose([0])),dict(translated=0,rotated=0,frame_shift=[7,4]))
        after['placements'][0]['x']+=1;after['placements'][0]['rotation']=90
        self.assertEqual(displacement(self.witness,after,self.pose([0])),dict(translated=1,rotated=1,frame_shift=[7,4]))
        after['placements'][2]['x']+=1
        with self.assertRaisesRegex(ValueError,'unselected-facility-changed'):displacement(self.witness,after,self.pose([0]))


if __name__=='__main__':unittest.main()
