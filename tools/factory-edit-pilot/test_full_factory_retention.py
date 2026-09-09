from collections import Counter
import unittest

from full_factory_retention import choose_paths, role, schedule


class RetentionTests(unittest.TestCase):
    def test_completed_full_score_paths_and_stable_ties(self):
        def row(area, tiles, order, complete=True, split='train'):
            return dict(case='a', split=split, budget_complete=complete, best_metrics=dict(score=[area, tiles, 0]),
                        initial_metrics=dict(score=[100, 10, 0]), source_order=order, row_order=0)
        rows = [row(90, 1, 0), row(80, 50, 1), row(80, 50, 2), row(70, 1, 0, False), row(60, 1, 0, split='development')]
        self.assertEqual(choose_paths(rows, ['a']), [rows[1]])
        with self.assertRaisesRegex(ValueError, 'no-complete-teacher'):
            choose_paths(rows, ['missing'])

    def test_followups_balance_roots_and_cycle_each_path(self):
        lessons = [dict(id=f'large-{i}', root='large', split='train', ordinal=i) for i in range(4)]
        lessons += [dict(id='original', root='original', split='train', ordinal=0)]
        lessons += [dict(id=f'small-{root}-{i}', root=f'small-{root}', split='train', ordinal=i) for root in range(6) for i in range(8)]
        config = dict(updates=1024, role_weights=dict(initiating_pose=8, continuation=4, original_contraction=2, small_initial=6, small_followup=12))
        steps = schedule(lessons, 'large', 'original', config, 17)
        self.assertEqual(steps, schedule(lessons, 'large', 'original', config, 17))
        self.assertNotEqual(steps, schedule(lessons, 'large', 'original', config, 29))
        lookup = {l['id']: l for l in lessons}
        for start in range(0, 1024, 32):
            self.assertEqual(Counter(role(lookup[s['lesson']], 'large', 'original') for s in steps[start:start+32]), config['role_weights'])
        for root in range(6):
            selected = [lookup[s['lesson']]['ordinal'] for s in steps if lookup[s['lesson']]['root'] == f'small-{root}']
            self.assertEqual(selected.count(0), 32)
            self.assertEqual([i for i in selected if i], [1+i % 7 for i in range(64)])
        lessons[0]['split'] = 'development'
        with self.assertRaisesRegex(ValueError, 'nontraining'):
            schedule(lessons, 'large', 'original', config, 17)


if __name__ == '__main__':
    unittest.main()
