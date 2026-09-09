import unittest

from full_factory_fresh_transfer import validate_config


class FreshTransferTests(unittest.TestCase):
    def test_invalid_budget_or_sampling_bank_is_rejected_before_evaluation(self):
        valid = dict(schema_version=1, horizon=8, episode_seconds=30, route_work=5000,
                     screen_seconds=3600, banks=[1700000, 1710000], temperature=.05)
        validate_config(valid)
        for key, value in [('horizon', True), ('horizon', 0), ('episode_seconds', 301),
                           ('route_work', -1), ('screen_seconds', 7201), ('banks', []),
                           ('banks', [1, 1]), ('banks', [False]), ('temperature', float('nan'))]:
            with self.subTest(key=key, value=value), self.assertRaisesRegex(ValueError, 'fresh-transfer.config-'):
                validate_config(dict(valid, **{key: value}))


if __name__ == '__main__':
    unittest.main()
