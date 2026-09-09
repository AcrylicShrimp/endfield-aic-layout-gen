"""Shared immutable PPO observation views and bounded witness identity reuse."""
import numpy as np

from full_factory_environment import digest
from full_factory_input_contracts import observation
from full_factory_selection import action_key, physical_key


def observation_pair(state, description, remaining, runtime, training):
    if not training:
        return observation(state, description, remaining, [], 'action-only', runtime), None
    critic = observation(state, description, remaining, [], 'current-best-layout', runtime)
    actor = dict(critic, current=None, best=None, best_is_current=False)
    actor['context'] = critic['context'].copy()
    actor['context'][4:] = 0
    for name in ('operands', 'member_nodes', 'action_xy', 'member_xy'):
        actor[name] = np.zeros_like(critic[name])
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

    def mask(self, state, description, memory, enabled):
        current = self.get(state['current'])[0]
        best = self.get(state['best'])[0]
        keys = [action_key(action, description, state['current'], best) for action in description['actions']]
        allowed, exhausted = memory.mask(current, keys, description['actions'], enabled)
        return keys, allowed, exhausted
