import unittest

from full_factory_large_learning import schedule


class LargeLearningScheduleTests(unittest.TestCase):
    def test_root_balance_does_not_overweight_intermediate_large_states(self):
        lessons = [dict(id=f'large-{i}', root='large', split='train', ordinal=i) for i in range(4)]
        lessons += [dict(id=f'small-{i}', root=f'small-{i}', split='train', ordinal=0) for i in range(6)]
        order = schedule(lessons, 56, 17)
        self.assertEqual(order, schedule(lessons, 56, 17))
        self.assertNotEqual(order, schedule(lessons, 56, 29))
        large = [s['lesson'] for s in order if s['lesson'].startswith('large')]
        self.assertEqual(large, [f'large-{i}' for i in range(4)]*2)
        for i in range(6):
            self.assertEqual(sum(s['lesson'] == f'small-{i}' for s in order), 8)

    def test_development_lesson_and_duplicate_ids_are_rejected(self):
        item = dict(id='a', root='a', split='development', ordinal=0)
        with self.assertRaisesRegex(ValueError, 'nontraining'):
            schedule([item], 8, 17)
        item['split'] = 'train'
        with self.assertRaisesRegex(ValueError, 'membership'):
            schedule([item, item], 8, 17)


if __name__ == '__main__':
    unittest.main()
