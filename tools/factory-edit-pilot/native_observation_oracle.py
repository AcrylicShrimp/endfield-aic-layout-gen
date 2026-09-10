"""Explicit migration oracle. Never imported by generation or training."""
import json
import sys

import numpy as np

from full_factory_policy import observe
from full_factory_input_contracts import CONTRACTS, history_array, selected_action_token, world_input


def compare(name, actual, expected):
    actual = np.asarray(actual, dtype=expected.dtype).reshape(expected.shape)
    if actual.tobytes() != expected.tobytes():
        unequal = np.flatnonzero(actual.view(np.uint8).ravel() != expected.view(np.uint8).ravel())
        first = int(unequal[0]) // expected.dtype.itemsize
        raise AssertionError(f'{name}: {len(unequal)} unequal bytes; first element {first}: '
                             f'{actual.ravel()[first]!r} != {expected.ravel()[first]!r}')


def main():
    request = json.load(sys.stdin)
    for ci, case in enumerate(request['cases']):
        state = case['state']
        expected = observe(state['current'], case['description'], state, .625)
        actual = dict(case['world'], **case['candidates'])
        for name in expected:
            if name not in ('schema', 'context'):
                compare(f'{ci}/{name}', actual[name], expected[name])
        for i, token in enumerate(case['tokens']):
            compare(f'{ci}/token/{i}', token, selected_action_token(expected, i))
        for name, contract in CONTRACTS.items():
            context = np.zeros(9, np.float32)
            context[:4] = expected['context'][:4]
            if contract['current'] != 'none':
                context[4:6] = expected['context'][4:6]
            if contract['best'] != 'none':
                context[6:8] = expected['context'][6:8]
            if contract['best'] in ('scores-3', 'layout'):
                context[8] = state['best_metrics']['score'][2] / max(1, state['initial_metrics']['score'][2])
            compare(f'{ci}/{name}/context', case['contexts'][name], context)
            compare(f'{ci}/{name}/history', case['histories'][name], history_array(case['records'], contract))
        geometry = world_input(expected, True)
        for name in ('raster', 'nodes', 'edges', 'edge_values'):
            compare(f'{ci}/geometry/{name}', case['geometry'][name], geometry[name])
    print(json.dumps(dict(status='passed', cases=len(request['cases']))))


if __name__ == '__main__':
    main()
