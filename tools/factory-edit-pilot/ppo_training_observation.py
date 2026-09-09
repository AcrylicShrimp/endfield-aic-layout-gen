"""Shared immutable PPO observation views and bounded witness identity reuse."""
import numpy as np

from full_factory_environment import digest
from full_factory_input_contracts import CONTRACTS, observation, history_array, world_input
from full_factory_selection import action_key, physical_key


def observation_pair(state, description, remaining, history, candidate, runtime, training):
    if not training:
        return observation(state, description, remaining, history, candidate, runtime), None
    critic = observation(state, description, remaining, [], 'current-best-layout', runtime)
    contract = CONTRACTS[candidate]
    current = critic['current']
    if contract['current'] == 'none':
        current = None
    elif contract['current'] == 'geometry':
        current = world_input(current, geometry=True)
    layout = contract['best'] == 'layout'
    actor = dict(critic, current=current, best=critic['best'] if layout else None,
                 best_is_current=critic['best_is_current'] if layout else False,
                 history=history_array(history, contract))
    actor['context'] = critic['context'].copy()
    if current is None:
        actor['context'][4:6] = 0
        for name in ('operands', 'member_nodes', 'action_xy', 'member_xy'):
            actor[name] = np.zeros_like(critic[name])
    if contract['best'] == 'none':
        actor['context'][6:8] = 0
    if contract['best'] not in ('scores-3', 'layout'):
        actor['context'][8] = 0
    return actor, critic


class WitnessIdentities:
    """Episode snapshots are replaced, never mutated; retain at most four objects."""
    def __init__(self):
        self.entries = []

    def get(self, witness):
        for previous, physical, serialized in self.entries:
            if witness is previous:
                return physical, serialized
        result = physical_key(witness), digest(witness)
        self.entries = [(witness, *result), *self.entries[:3]]
        return result

    def mask(self, state, description, memory, enabled, candidate):
        current = self.get(state['current'])[0]
        best = self.get(state['best'])[0]
        keys = [action_key(action, description, state['current'], best) for action in description['actions']]
        allowed, exhausted = memory.mask(current, keys, description['actions'], enabled)
        if not CONTRACTS[candidate]['restore']:
            allowed = [ok and action['kind'] != 'restore_best' for ok, action in zip(allowed, description['actions'])]
            if not any(allowed):
                raise ValueError(f'input-contract.action-exhausted: {candidate}, state {current}')
        return keys, allowed, exhausted
