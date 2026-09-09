"""Read-only checks of identical-input target conflicts in audited policy lessons."""
import argparse
from collections import Counter, defaultdict
import hashlib
from pathlib import Path
import time

import numpy as np

from full_factory_curriculum import source_manifest
from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_large_learning import read, verify_sources
from full_factory_policy import observe
from full_factory_screen import sha, write
from full_factory_trajectory_learning import trajectory_state


def observation_key(obs):
    result = hashlib.sha256()
    for name in sorted(obs):
        value = obs[name]
        result.update(name.encode())
        if isinstance(value, np.ndarray):
            result.update(str((value.dtype.str, value.shape)).encode())
            result.update(np.ascontiguousarray(value).tobytes())
        else:
            result.update(str(value).encode())
    return result.hexdigest()


def action_aliases(obs):
    """Equal readout operands imply equal logits for every parameter setting.

    Use actual node indices and all member features: this is a sufficient equality
    test, not a complete characterization of GNN indistinguishability. Member
    aggregation is a mean, so repeated identical multisets also have equal output.
    Ordinary pose membership currently forbids duplicate members.
    """
    members = defaultdict(list)
    for action, node, xy, pose in zip(obs['member_actions'], obs['member_nodes'], obs['member_xy'], obs['member_pose']):
        members[int(action)].append((int(node), tuple(xy.tolist()), tuple(pose.tolist())))
    groups = defaultdict(list)
    for i in range(len(obs['actions'])):
        key = (tuple(obs['actions'][i].tolist()), tuple(obs['operands'][i].tolist()),
               tuple(obs['action_xy'][i].tolist()), tuple(sorted(members[i])))
        groups[key].append(i)
    return list(groups.values())


def target_mass_ceiling(groups, targets):
    """Maximum possible target mass if arbitrary logits are allowed per alias class."""
    target_set = set(targets)
    return max(len(target_set.intersection(group))/len(group) for group in groups)


def run(args):
    began = time.perf_counter()
    if not 0 < args.seconds <= 1800:
        raise ValueError('learnability.invalid-time-bound')
    p, admission, proof = (read(args.screen/n) for n in ['protocol.json','admission.json','lessons-audit.json'])
    if admission['status'] != 'complete' or proof['status'] != 'passed' or proof['admission_sha256'] != sha(args.screen/'admission.json') or admission['lessons_sha256'] != sha(args.screen/'lessons.json'):
        raise ValueError('learnability.lesson-audit')
    verify_sources(p)
    lessons = read(args.screen/'lessons.json')
    cases = {c['id']:c for c in p['cases']}
    rows, inputs = [], defaultdict(list)
    sources = dict(p['sources'])
    for name in ['protocol.json','admission.json','lessons-audit.json','lessons.json']:
        sources[str((args.screen/name).resolve())] = sha(args.screen/name)
    args.output.mkdir()
    protocol = dict(schema_version=1, experiment='policy-input-target-alias-audit', screen=str(args.screen.resolve()),
        seconds_bound=args.seconds, sources=sources, source_sha256=source_manifest(), training_performed=False,
        scope='Sufficient exact input/readout equality checks only; absence of collisions is not proof of representability, optimization, generalization or target reachability')
    write(args.output/'protocol.json',protocol)
    runtime = FactoryRuntime(p['native_library'],p['catalogs'])
    try:
        for lesson in lessons:
            if time.perf_counter()-began >= args.seconds:
                raise TimeoutError('learnability.audit-budget')
            if sha(lesson['initial']) != lesson['initial_sha256']:
                raise ValueError('learnability.lesson-witness-hash')
            env = FactoryEpisode(runtime,dict(cases[lesson['root']],initial=lesson['initial']),p['config']['horizon'],p['config']['episode_seconds'],p['config']['route_work'])
            description = env.describe()
            obs = observe(env.state['current'],description,trajectory_state(env.state,lesson),1-lesson['ordinal']/p['config']['horizon'])
            if obs['context'].tolist() != lesson['observation_context'] or digest(env.state['current']) != lesson['current_sha256']:
                raise ValueError('learnability.observation-context')
            targets = [description['actions'].index(t['action']) for t in lesson['targets']]
            if targets != [t['vocabulary_index'] for t in lesson['targets']]:
                raise ValueError('learnability.target-membership')
            groups = action_aliases(obs)
            key = observation_key(obs)
            inputs[key].append(dict(lesson=lesson['id'],targets=targets))
            rows.append(dict(lesson=lesson['id'],root=lesson['root'],ordinal=lesson['ordinal'],observation_sha256=key,
                actions=len(description['actions']),targets=targets,alias_groups=[g for g in groups if len(g)>1],
                target_alias_groups=[g for g in groups if len(g)>1 and set(g).intersection(targets)],
                target_mass_ceiling=target_mass_ceiling(groups,targets)))
    finally:
        runtime.close()
    duplicate_inputs=[]
    for key,group in inputs.items():
        if len(group)>1:
            common=set(group[0]['targets'])
            for g in group[1:]:common.intersection_update(g['targets'])
            duplicate_inputs.append(dict(observation_sha256=key,lessons=group,common_targets=sorted(common),conflicting=not common))
    verify_sources(protocol)
    write(args.output/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
        seconds=time.perf_counter()-began,lessons=len(rows),roots=len({r['root'] for r in rows}),
        targets=sum(len(r['targets']) for r in rows),rows=rows,duplicate_inputs=duplicate_inputs,
        conflicting_duplicate_inputs=sum(r['conflicting'] for r in duplicate_inputs),
        positive_loss_floor_lessons=sum(r['target_mass_ceiling']<1 for r in rows),training_performed=False,
        limitations=protocol['scope']))
    print(dict(status='complete',lessons=len(rows),duplicate_inputs=len(duplicate_inputs),
               positive_loss_floor_lessons=sum(r['target_mass_ceiling']<1 for r in rows),seconds=time.perf_counter()-began),flush=True)


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['screen','output']:parser.add_argument('--'+name,required=True,type=Path)
    parser.add_argument('--seconds',type=int,default=300)
    run(parser.parse_args())
