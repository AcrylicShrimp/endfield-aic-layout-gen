"""Checks for false exclusion and stale physical-cache identity in the diagnostic."""
from copy import deepcopy
import unittest

from diagnose_blueprint_commit import draft_key, footprint_floor


class GeometryTests(unittest.TestCase):
    def test_body_floor_is_translation_invariant_and_below_routed_extent(self):
        for shift in (0, 100):
            observation = dict(facilities=[dict(x=shift+3, y=shift+5, width=2, height=4),
                dict(x=shift+8, y=shift+6, width=3, height=2)])
            cells = {(f['x']+x, f['y']+y) for f in observation['facilities']
                for x in range(f['width']) for y in range(f['height'])}
            body_area = (max(x for x,y in cells)-min(x for x,y in cells)+1) * \
                (max(y for x,y in cells)-min(y for x,y in cells)+1)
            self.assertEqual(footprint_floor(observation), body_area)
            cells.add((shift, shift+20))
            full_area = (max(x for x,y in cells)-min(x for x,y in cells)+1) * \
                (max(y for x,y in cells)-min(y for x,y in cells)+1)
            self.assertLess(footprint_floor(observation), full_area)

    def test_physical_cache_preserves_blueprint_pose_and_path_identity(self):
        observation = dict(blueprint_sha256='original', facilities=[dict(x=1, y=2, width=2, height=3)],
            edges=[dict(path=[dict(x=0,y=0),dict(x=1,y=0)])], attempts=3, best_metrics=dict(score=[100]))
        same = deepcopy(observation)
        same.update(attempts=30, best_metrics=dict(score=[90]))
        self.assertEqual(draft_key(observation), draft_key(same))
        for changed in ('blueprint', 'pose', 'path'):
            other = deepcopy(observation)
            if changed == 'blueprint':
                other['blueprint_sha256'] = 'other'
            elif changed == 'pose':
                other['facilities'][0]['x'] += 1
            else:
                other['edges'][0]['path'][1]['y'] += 1
            self.assertNotEqual(draft_key(observation), draft_key(other))


if __name__ == '__main__':
    unittest.main()
