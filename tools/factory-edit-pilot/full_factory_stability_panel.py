"""Immutable first-action panel for observing, never selecting, policy updates."""
from collections import Counter
from pathlib import Path

import numpy as np
import torch

from full_factory_environment import FactoryRuntime, digest
from full_factory_large_learning import unpack
from full_factory_learnability_audit import observation_key
from full_factory_policy import observe
from full_factory_selection import SelectionMemory, action_key, physical_key, distribution
from full_factory_stability_update import require


def load_panel(protocol, guard=lambda: None):
    panel = []
    retained_bytes = 0
    runtime = FactoryRuntime(protocol['native_library'], protocol['catalogs'])
    try:
        for source in protocol['panel']:
            guard()
            record = unpack(Path(source['directory'])/'states.json.gz')[0]
            episode = unpack(Path(source['directory'])/'episode.json.gz')
            state = record['state']
            require(digest(state['current']) == episode['trace'][0]['current_before_sha256'], f'panel current: {source["case"]}')
            description = runtime.rpc('describe', state['current'], options={})
            observation = observe(state['current'], description, state, 1)
            require(observation_key(observation) == record['observation_sha256'], f'panel observation: {source["case"]}')
            physical = physical_key(state['current'])
            keys = [action_key(a, description, state['current'], physical) for a in description['actions']]
            allowed, exhausted = SelectionMemory(state['current']).mask(physical, keys, description['actions'], protocol['config']['memory'])
            require(allowed == record['allowed'] and exhausted == episode['trace'][0]['memory_exhausted'], f'panel mask: {source["case"]}')
            retained_bytes += sum(v.nbytes for v in observation.values() if isinstance(v, np.ndarray))
            require(retained_bytes <= protocol['study']['panel_observation_bytes'], 'panel observation array bound')
            panel.append(dict(case=source['case'], observation=observation, actions=description['actions'], allowed=allowed,
                              observation_sha256=record['observation_sha256'], allowed_sha256=digest(allowed)))
    finally:
        runtime.close()
    guard()
    return panel


def probabilities(protocol, model, panel, guard=lambda: None):
    result = []
    for row in panel:
        guard()
        with torch.no_grad():
            dist = distribution(model(row['observation'])/protocol['config']['temperature'],
                                row['actions'], row['allowed'], protocol['config']['balanced'])
            require(torch.isfinite(dist.probs).all(), f'panel probabilities: {row["case"]}')
            result.append(dict(probs=dist.probs.clone(), entropy=float(dist.entropy())))
    return result


def summarize(panel, current, initial, previous):
    require(len(panel) == len(current) == len(initial) == len(previous), 'panel comparison size')
    rows = []
    for source, now, start, before in zip(panel, current, initial, previous):
        mass = Counter()
        for action, probability in zip(source['actions'], now['probs'].tolist()):
            mass[action['kind']] += probability
        rows.append(dict(case=source['case'], entropy=now['entropy'], kind_probability=dict(mass),
                         total_variation_from_initial=float((now['probs']-start['probs']).abs().sum()/2),
                         total_variation_from_previous=float((now['probs']-before['probs']).abs().sum()/2),
                         observation_sha256=source['observation_sha256'], allowed_sha256=source['allowed_sha256']))
    return rows
