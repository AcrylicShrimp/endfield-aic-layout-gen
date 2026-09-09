from collections import Counter
import unittest

from full_factory_large_exposure import role, schedule


class ExposureScheduleTests(unittest.TestCase):
    def setUp(self):
        self.lessons = [dict(id=f'large-{i}', root='large', split='train', ordinal=i) for i in range(4)]
        self.lessons += [dict(id='original', root='original', split='train', ordinal=0)]
        self.lessons += [dict(id=f'small-{i}', root=f'small-{i}', split='train', ordinal=0) for i in range(6)]
        self.config = dict(updates=512, role_weights=dict(initiating_pose=8, continuation=4, original_contraction=2, rehearsal=2))

    def test_block_quotas_and_state_cycles_are_exact(self):
        steps = schedule(self.lessons, 'large', 'original', self.config, 17)
        self.assertEqual(steps, schedule(self.lessons, 'large', 'original', self.config, 17))
        self.assertNotEqual(steps, schedule(self.lessons, 'large', 'original', self.config, 29))
        lookup = {l['id']: l for l in self.lessons}
        for start in range(0, 512, 16):
            self.assertEqual(Counter(role(lookup[s['lesson']], 'large', 'original') for s in steps[start:start+16]), self.config['role_weights'])
        continuations = [lookup[s['lesson']]['ordinal'] for s in steps if role(lookup[s['lesson']], 'large', 'original') == 'continuation']
        self.assertEqual(continuations, [1+i % 3 for i in range(128)])
        small = Counter(s['lesson'] for s in steps if s['lesson'].startswith('small'))
        self.assertEqual(sum(small.values()), 64)
        self.assertEqual(max(small.values())-min(small.values()), 1)

    def test_incomplete_block_and_development_input_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'incomplete-block'):
            schedule(self.lessons, 'large', 'original', dict(self.config, updates=511), 17)
        self.lessons[0]['split'] = 'development'
        with self.assertRaisesRegex(ValueError, 'nontraining'):
            schedule(self.lessons, 'large', 'original', self.config, 17)


if __name__ == '__main__':
    unittest.main()
