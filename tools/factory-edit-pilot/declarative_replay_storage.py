"""Persist selected replay inputs and live credit, not complete PPO trajectories."""
from collections import defaultdict

from blueprint_ppo_update import require
from declarative_learning_buffer import physical

SCHEMA = 'declarative-replay-chunk-v2'


def replay_input(prepared, decision):
    """Keep the full root likelihood domain and only the recorded port branch.

    Numerical tensors, kind counts, ordering and structural masks are unchanged.
    SIL never executes alternative roots or samples a new conditional branch.
    Shared kind dictionaries avoid thousands of unused action dictionaries; the
    selected action retains its complete description for explicit inspection.
    """
    if 'recorded_root' in prepared:
        require(prepared['recorded_root']==decision['root'],'declarative.replay-input: recorded conditional root changed')
        return prepared
    result=dict(prepared)
    kinds={kind:dict(kind=kind) for kind in ('place','insert_band','delete_band')}
    actions=[kinds[action['kind']] for action in prepared['actions']]
    root=decision['root'];selected=prepared['actions'][root]
    actions[root]=selected
    result.update(actions=actions,recorded_root=root)
    groups=[[] for _ in prepared['port_groups']]
    if selected['kind']=='place':
        owner=prepared['root_owners'][root];rotation=selected['rotation']
        groups[owner]=[dict(group,features={rotation:group['features'][rotation]})
            for group in prepared['port_groups'][owner]]
    result['port_groups']=groups
    return result


def sample_input(sample):
    if '_replay_input' not in sample:
        sample['_replay_input']=replay_input(sample['input'],sample['decision'])
    return sample['_replay_input']


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
        rows[index]['input']=sample_input(sample)
        rows[index]['observation'] = physical(sample['observation'])
    return dict(schema=SCHEMA,model_before=payload['model_before'],sample_count=payload.get('sample_count',len(payload['samples'])),
        samples=rows,completed=[])
