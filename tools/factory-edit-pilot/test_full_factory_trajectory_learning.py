import unittest

from full_factory_trajectory_learning import family_cases, trajectory_state


class FamilyAssignmentTests(unittest.TestCase):
    def inputs(self):
        return {'rows': [{'id':'a'}, {'id':'b'}]}

    def admission(self):
        return {'accepted':2,'rows':[
            {'id':'a','family':'one','split':'train','factory_validated':True},
            {'id':'b','family':'two','split':'development','factory_validated':True},
            {'id':'failed','family':'two','split':'development','factory_validated':False},
        ]}

    def test_failed_admission_is_not_a_training_example(self):
        rows=family_cases(self.admission(),self.inputs())
        self.assertEqual([(r['id'],r['split']) for r in rows],[('a','train'),('b','development')])

    def test_missing_accepted_case_fails_instead_of_changing_denominator(self):
        with self.assertRaises(ValueError):
            family_cases(self.admission(),{'rows':[{'id':'a'}]})

    def test_related_family_cannot_cross_training_and_development(self):
        a=self.admission();a['rows'][1]['family']='one'
        with self.assertRaisesRegex(ValueError,'family-leakage'):
            family_cases(a,self.inputs())

    def test_sealed_split_is_not_opened_by_this_development_screen(self):
        a=self.admission();a['rows'][1]['split']='test'
        with self.assertRaisesRegex(ValueError,'family-split'):
            family_cases(a,self.inputs())

    def test_restored_lesson_keeps_original_episode_baseline(self):
        current={'score':[80,30,4]};original={'score':[100,40,4]}
        state={'current_metrics':current,'initial_metrics':current,'best_metrics':current}
        lesson={'initial_metrics':current,'episode_initial_metrics':original,'episode_best_metrics':current}
        restored=trajectory_state(state,lesson)
        self.assertEqual(restored['initial_metrics']['score'][0],100)
        self.assertEqual(restored['current_metrics']['score'][0]/restored['initial_metrics']['score'][0],.8)
        self.assertEqual(state['initial_metrics'],current)
        with self.assertRaisesRegex(ValueError,'lesson-current-metrics'):
            trajectory_state(state,dict(lesson,initial_metrics=original))


if __name__=='__main__':
    unittest.main()
