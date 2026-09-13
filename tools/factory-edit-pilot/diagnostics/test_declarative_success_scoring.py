"""Failure cases for geometry heuristics, not native gameplay fixtures."""
from copy import deepcopy
import unittest

from declarative_success_scoring import features, score, witness_extent


def row(before, after, places, actions=None, before_extent=(12, 2), after_extent=(10, 2)):
    return dict(id='fixture', native_validated=True, before_area=before_extent[0]*before_extent[1],
        after_area=after_extent[0]*after_extent[1], before_extent=list(before_extent), after_extent=list(after_extent),
        before=before, final=after, places=places,
        actions=actions or [dict(action=dict(kind='place', instance=p['instance'])) for p in places],
        stratum='fixture', product='fixture', geometry='fixture', episode=[0, 0], depth=0,
        admitted_update=1, uses=0)


class GeometryTests(unittest.TestCase):
    def test_interior_center_move_does_not_receive_exterior_credit(self):
        before=[[0, 0, 0, 2, 2, 0], [1, 4, 1, 2, 2, 0], [2, 10, 8, 2, 2, 0]]
        after=deepcopy(before);after[1][2]=3
        value=features(row(before, after, [dict(label=0, instance=1, before=before, after=after)],
            before_extent=(14, 12), after_extent=(14, 11)))
        self.assertGreater(value['center_inward'], 0)
        self.assertEqual(value['exterior_release'], 0)

    def test_outside_facility_inward_releases_extent(self):
        before=[[0, 0, 0, 2, 2, 0], [1, 10, 0, 2, 2, 0]]
        after=[[0, 0, 0, 2, 2, 0], [1, 8, 0, 2, 2, 0]]
        value=features(row(before, after, [dict(label=0, instance=1, before=before, after=after)]))
        self.assertAlmostEqual(value['exterior_release'], 1/6)
        self.assertGreater(value['center_inward'], 0)

    def test_reversed_preparation_does_not_accumulate_bonus(self):
        before=[[0, 0, 0, 2, 2, 0], [1, 10, 0, 2, 2, 0]]
        middle=[[0, 0, 0, 2, 2, 0], [1, 4, 0, 2, 2, 0]]
        final=[[0, 0, 0, 2, 2, 0], [1, 8, 0, 2, 2, 0]]
        places=[dict(label=0, instance=1, before=before, after=middle),
                dict(label=1, instance=1, before=middle, after=before)]
        actions=[dict(action=dict(kind='place', instance=1)) for _ in range(2)]
        actions.append(dict(action=dict(kind='delete_band', axis='x', at=2, count=2)))
        value=features(row(before, final, places, actions))
        self.assertEqual(value['family'], 'place_preparation')
        self.assertTrue(value['has_place'])
        self.assertAlmostEqual(value['center_inward'], 0)
        self.assertAlmostEqual(value['exterior_release'], 0)

    def test_real_rotated_footprint_is_used(self):
        before=[[0, 4, 0, 8, 2, 0], [1, 0, 0, 2, 2, 0]]
        after=[[0, 4, 0, 2, 8, 90], [1, 0, 0, 2, 2, 0]]
        value=features(row(before, after, [dict(label=0, instance=0, before=before, after=after)],
            before_extent=(12, 10), after_extent=(10, 8)))
        # Its facility footprint grows from 12*2 to 6*8 despite the validated
        # combined transport/facility area improving. No exterior bonus.
        self.assertEqual(value['exterior_release'], 0)

    def test_transport_can_determine_actual_area(self):
        value=dict(placements=[dict(zip(('instance','x','y','width','height','rotation'),
            ('a',4,3,2,2,0)))], transport_networks=[dict(cells=[dict(x=12,y=9,z=0)])])
        self.assertEqual(witness_extent(value), (9,7))
        value['transport_networks'][0]['terminals']=[dict(position=dict(x=2,y=2))]
        self.assertEqual(witness_extent(value), (11,8))

    def test_translating_entire_geometry_cannot_change_spatial_score(self):
        before=[[0, 0, 0, 2, 2, 0], [1, 10, 0, 2, 2, 0]]
        after=[[0, 0, 0, 2, 2, 0], [1, 8, 0, 2, 2, 0]]
        value=row(before, after, [dict(label=0, instance=1, before=before, after=after)])
        original=features(value)
        shifted=deepcopy(value)
        # deepcopy preserves shared list aliases, so construct fresh shifted
        # layouts rather than accidentally translating shared objects twice.
        def shift(layout):return [[p[0],p[1]+17,p[2]+23,*p[3:]] for p in layout]
        shifted['before']=shift(before);shifted['final']=shift(after)
        shifted['places']=[dict(label=0,instance=1,before=shift(before),after=shift(after))]
        actual=features(shifted)
        for key in ('gain','center_inward','exterior_release'):
            self.assertAlmostEqual(original[key],actual[key])

    def test_unvalidated_and_nonimproving_results_are_rejected(self):
        before=[[0,0,0,2,2,0]]
        value=row(before,before,[],[dict(action=dict(kind='delete_band'))])
        for changes in [dict(native_validated=False),dict(after_area=24),dict(after_extent=[10,3])]:
            with self.assertRaises(ValueError):features(dict(value,**changes))

    def test_unrelated_facility_change_in_place_transition_is_rejected(self):
        before=[[0,0,0,2,2,0],[1,10,0,2,2,0]]
        after=[[0,1,0,2,2,0],[1,8,0,2,2,0]]
        with self.assertRaisesRegex(ValueError, 'place-transition'):
            features(row(before,after,[dict(label=0,instance=1,before=before,after=after)]))

    def test_ablation_does_not_relabel_center_as_exterior(self):
        value=dict(gain=.02,rarity=.9,center_inward=.1,exterior_release=0.)
        rare=dict(rare=.03,center=0.,exterior=0.)
        self.assertEqual(score(value,rare),score(value,dict(rare,exterior=1.)))
        self.assertGreater(score(value,dict(rare,center=.1)),score(value,rare))

    def test_geometry_bonus_can_be_capped_by_actual_validated_gain(self):
        value=dict(gain=.02,rarity=.9,center_inward=.1,exterior_release=.17)
        weights=dict(rare=.03,center=0.,exterior=1.)
        self.assertAlmostEqual(score(value,dict(weights,cap_geometry=True)),.067)
        self.assertAlmostEqual(score(value,weights),.217)


if __name__ == '__main__':
    unittest.main()
