"""Persist selected replay inputs and live credit, not complete PPO trajectories."""
from collections import defaultdict

from blueprint_ppo_update import require
from declarative_learning_buffer import physical

SCHEMA = 'declarative-replay-chunk-v1'


def references(state):
    needed = defaultdict(set)
    for entry in state['entries'].values():
        for label in entry['labels']:
            ordinal, index = label['reference']
            needed[ordinal].add(index)
    for active in state['active'].values():
        refs = list(active['segment'])
        refs.extend(ref for pending in active['pending'] for ref in pending['references'])
        for ordinal, index in refs:
            needed[ordinal].add(index)
    return needed


def compact(payload, indices):
    rows = {}
    for index in sorted(indices):
        available = payload['samples']
        require(index in available if isinstance(available,dict) else 0 <= index < len(available),
            'declarative.replay-storage: sample reference outside source chunk')
        sample = payload['samples'][index]
        rows[index] = {key:sample[key] for key in ('stream','episode','policy_action','input','decision',
            'action','reward','terminal','validated_witness')}
        rows[index]['observation'] = physical(sample['observation'])
    return dict(schema=SCHEMA,model_before=payload['model_before'],sample_count=payload.get('sample_count',len(payload['samples'])),
        samples=rows,completed=payload['completed'])
