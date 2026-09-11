"""Capacity comparisons preserve paired cohorts and distinguish starting function from learning."""
from copy import deepcopy
import unittest

from compare_ppo_capacity import paired, workload_controls
from ppo_actor_critic import treatment
from ppo_spatial_model import specification, shared_capacity_specification


def cell(initial, final):
    return dict(jobs={str(seed): dict(summary=dict(area_reduction_pp=finish, training_actions=64000),
                                    evaluations=[dict(batch=batch, area_reduction_pp=area,
                                                      pairs=[dict(generation_seed=123)])
                                                 for batch, area in ((0, start), (32, finish))])
                      for seed, start, finish in zip((17, 29), initial, final)})


def protocol(root, model):
    python_files = ('ppo_spatial_model.py', 'ppo_harness_tracking.py', 'ppo_training_update.py')
    spec = dict(cli=root+'/target/release/aic-cli', validator=root+'/target/release/aic-witness-probe',
                native_library=root+'/target/release/libaic_pilot_native.so',
                catalogs=dict(items=root+'/data/items.json'), model=model)
    return dict(config=dict(model=model, ppo=dict(epochs=4), execution=dict(device='cuda')),
                environment=dict(platform='Linux', packages=dict(torch='test-version')), spec=spec,
                sources={**{spec[key]: key+'-digest' for key in ('cli', 'validator', 'native_library')},
                         **{root+'/tools/factory-edit-pilot/'+name: name+'-digest' for name in python_files},
                         root+'/data/items.json': 'item-catalog-digest'})


class CapacityAnalysisTests(unittest.TestCase):
    def test_paired_final_difference_and_learning_gain_are_distinct(self):
        control = cell((40, 50), (55, 60))
        wide = cell((42, 49), (56, 62))
        result = paired(wide, control)
        self.assertEqual(result['by_seed']['17']['area_reduction_pp'], 1)
        self.assertEqual(result['by_seed']['17']['area_learning_gain_pp'], -1)
        self.assertEqual(result['by_seed']['29']['area_learning_gain_pp'], 3)
        self.assertEqual(result['mean']['area_reduction_pp'], 1.5)
        self.assertEqual(result['mean']['area_learning_gain_pp'], 1)
        corrupted = deepcopy(wide)
        corrupted['jobs']['17']['evaluations'][-1]['pairs'][0]['generation_seed'] = 999
        with self.assertRaisesRegex(ValueError, 'admission differs'):
            paired(corrupted, control)
        corrupted = deepcopy(wide)
        corrupted['jobs']['17']['summary']['training_actions'] -= 32
        with self.assertRaisesRegex(ValueError, 'action budgets differ'):
            paired(corrupted, control)

    def test_only_declared_capacity_and_display_source_changes_are_allowed(self):
        old = protocol('/old', dict(specification('cnn2'), actor_critic=treatment(True, False, False)))
        new = protocol('/new', shared_capacity_specification())
        for name in ('ppo_spatial_model.py', 'ppo_harness_tracking.py'):
            new['sources']['/new/tools/factory-edit-pilot/'+name] = 'new-'+name
        result = workload_controls(old, new)
        self.assertTrue(result['native_binaries_identical'])
        self.assertEqual(len(result['changed_runtime_sources']), 2)
        for field in ('learning', 'binary', 'environment', 'runtime', 'catalog'):
            changed = deepcopy(new)
            if field == 'learning':
                changed['config']['ppo']['epochs'] = 5
            elif field == 'binary':
                changed['sources'][changed['spec']['native_library']] = 'other-binary'
            elif field == 'environment':
                changed['environment']['packages']['torch'] = 'other-version'
            elif field == 'catalog':
                changed['sources']['/new/data/items.json'] = 'changed-catalog'
            else:
                changed['sources']['/new/tools/factory-edit-pilot/ppo_training_update.py'] = 'different-update'
            with self.subTest(field=field), self.assertRaises(ValueError):
                workload_controls(old, changed)


if __name__ == '__main__':
    unittest.main()
