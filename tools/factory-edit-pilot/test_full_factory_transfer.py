import unittest
from full_factory_transfer import admitted_cases


class TransferTests(unittest.TestCase):
    def setUp(self):
        self.admission=dict(status='measured',attempted=2,accepted=1,rows=[
            dict(id='valid',family='shared',split='train',factory_validated=True,facilities=26),
            dict(id='partial',family='shared',split='train',factory_validated=False,facilities=85)])
        self.inputs=dict(rows=[dict(id='valid',instances=26)])

    def test_partial_factory_is_excluded_without_relabeling_trained_family(self):
        cases=admitted_cases(self.admission,self.inputs)
        self.assertEqual(len(cases),1);self.assertEqual(cases[0]['id'],'valid')
        self.assertEqual(cases[0]['family_split'],'train');self.assertEqual(cases[0]['evaluation_role'],'fresh-size-development')
        self.assertEqual(self.admission['attempted'],2)

    def test_import_count_and_facility_count_must_match_admission(self):
        with self.assertRaisesRegex(ValueError,'admission-count'):
            admitted_cases(self.admission,dict(rows=self.inputs['rows']+[dict(id='partial',instances=20)]))
        with self.assertRaisesRegex(ValueError,'admission-size'):
            admitted_cases(self.admission,dict(rows=[dict(id='valid',instances=25)]))


if __name__=='__main__':unittest.main()
