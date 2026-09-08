"""Guard the independent diagnostic's crossing and comparison semantics."""
import unittest
from constructor_attachment_paths import route, same_item_options
from constructor_attachment_trace import stable


class AttachmentDiagnosticTests(unittest.TestCase):
    def test_crossing_axis_is_straight_and_cannot_be_endpoint(self):
        wall = {(1, y) for y in range(3)}
        crossing = {(1, 1): True}
        self.assertIsNone(route(3, 3, wall, {}, (0, 1), (2, 1)))
        self.assertEqual(route(3, 3, wall, crossing, (0, 1), (2, 1)), [[0, 1], [1, 1], [2, 1]])
        self.assertIsNone(route(3, 3, wall, {(1, 1): False}, (0, 1), (2, 1)))
        self.assertIsNone(route(3, 3, wall, crossing, (1, 1), (2, 1)))
        self.assertIsNone(route(2, 2, {(1, 1)}, {(1, 0): True}, (0, 0), (1, 1)))

    def test_same_item_eligibility_uses_independent_axis_capacity(self):
        p = lambda x, y: dict(x=x, y=y)
        r = dict(numerator=2, denominator=5)
        network = dict(transport='belt', item='fixture-item', terminals=[], cells=[p(1, 1)],
            segments=[dict(**{'from':p(0, 1)}, to=p(1, 1), rate=r),
                dict(**{'from':p(1, 1)}, to=p(2, 1), rate=r)])
        snapshot = dict(edge=dict(item='fixture-item', rate=r),
            trial=dict(source=dict(attachment=dict(port=dict(transport='belt')))),
            node=dict(logistics_components=[], transport_networks=[network]))
        catalog = dict(components=[dict(kind='bridge', id='fixture-bridge', transport='belt',
            footprint=dict(width=1, height=1), input_directions=list('NESW'), output_directions=list('NESW'),
            capacity=dict(quantity=1, duration_ms=2000))])
        self.assertEqual(same_item_options(snapshot, catalog)[0], {(1, 1):False})
        snapshot['edge']['rate'] = dict(numerator=3, denominator=5)
        self.assertEqual(same_item_options(snapshot, catalog)[0], {})
        snapshot['edge']['rate'] = r
        network['terminals'] = [dict(position=p(1, 1))]
        self.assertEqual(same_item_options(snapshot, catalog)[0], {})
        network['terminals'] = []
        network['segments'][1]['to'] = p(1, 2)
        self.assertEqual(same_item_options(snapshot, catalog)[0], {})

    def test_crossing_cannot_turn_even_when_escape_is_open(self):
        # The sole route would enter horizontally and turn north on the crossing.
        self.assertIsNone(route(3, 2, {(0, 0), (2, 0), (2, 1)},
            {(1, 1): True}, (0, 1), (1, 0)))

    def test_replay_ignores_only_elapsed_time(self):
        before = dict(elapsed_ms=1, nested=[dict(elapsed_ms=2, attempts=3)])
        after = dict(elapsed_ms=7, nested=[dict(elapsed_ms=8, attempts=3)])
        self.assertEqual(stable(before), stable(after))
        after['nested'][0]['attempts'] = 4
        self.assertNotEqual(stable(before), stable(after))


if __name__ == '__main__':
    unittest.main()
