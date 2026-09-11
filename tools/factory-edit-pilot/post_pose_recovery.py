"""Restore recorded post-action drafts and probe bounded fixed-pose recovery."""
from copy import deepcopy
import json
import time

from blueprint_ppo_environment import semantic
from blueprint_ppo_update import require


def draft(observation):
    return dict(placements=observation['facilities'], paths=[e['path'] for e in observation['edges']])


def advance(saved, sample, successor):
    require(semantic(successor) == sample['after_semantic'], 'authenticated successor identity')
    current = draft(successor)
    return dict(saved, draft=current,
        best_draft=current if sample['outcome']['status'] == 'improved' else saved['best_draft'],
        attempts=successor['attempts'], revision=successor['revision'],
        draft_validated=successor['draft_factory_validated'], elapsed_ns=round(successor['elapsed_ms'] * 1_000_000))


def restore_verified(runtime, saved, observation):
    session = runtime.restore(saved)
    actual = json.loads(session.observation_json())
    require(semantic(actual) == semantic(observation), 'native restoration equals the actual post-pose state')
    return session


def geometry(observation):
    blockers = []
    nodes = observation['nodes']
    for edge, state in zip(observation['blueprint']['edges'], observation['edges'], strict=True):
        if state['status'] != 'pending':
            continue
        for end in ('source', 'target'):
            point = nodes[edge[end]]['position']
            for facility in observation['facilities']:
                if (facility['x'] <= point['x'] < facility['x'] + facility['width'] and
                        facility['y'] <= point['y'] < facility['y'] + facility['height']):
                    blockers.append(dict(edge=edge['id'], end=end, position=point, facility=facility['instance']))
    return dict(conflicts=observation['conflicts'], body_blocked_endpoints=blockers,
                pending=[e['id'] for e in observation['edges'] if e['status'] == 'pending'])


def edge_order(observation, name):
    def key(edge):
        a, b = [observation['nodes'][edge[end]]['position'] for end in ('source', 'target')]
        distance = abs(a['x'] - b['x']) + abs(a['y'] - b['y'])
        rank = -edge['id'] if name == 'reverse_id' else -distance if name == 'longest_first' else distance
        return observation['blueprint']['networks'][edge['network']]['transport'] != 'pipe', rank, edge['id']
    pending = set(geometry(observation)['pending'])
    return [e['id'] for e in sorted(observation['blueprint']['edges'], key=key) if e['id'] in pending]


def branch(runtime, checkpoint, observation, name, work, padding, config, checker, guard):
    started = time.perf_counter()
    saved = deepcopy(checkpoint)
    saved.update(elapsed_ns=0, closed=False)
    saved['config'].update(max_actions=saved['attempts'] + config['branch_actions'],
                           elapsed_ms=config['branch_elapsed_ms'], route_expansions=work, search_padding=padding)
    session = runtime.restore(saved)
    current = json.loads(session.observation_json())
    for key in ('blueprint', 'facilities', 'nodes', 'edges', 'conflicts', 'best_metrics'):
        require(current[key] == observation[key], f'diagnostic fork preserves physical {key}')
    original_poses = current['facilities']
    original_paths = {e['id']: e['path'] for e in current['edges'] if e['path'] is not None}
    ordered = name in config['ordered_repairs']
    actions = [dict(kind='repair', edge=edge) for edge in edge_order(current, name)] if ordered else [dict(kind='repair_all')]
    actions.append(dict(kind='commit'))
    trace, expansions, witness, status = [], 0, None, 'unresolved'
    for action in actions:
        guard()
        if (time.perf_counter() - started) * 1000 >= config['branch_elapsed_ms']:
            status = 'diagnostic_elapsed'
            break
        if action['kind'] != 'commit':
            remaining = work - expansions
            if remaining <= 0:
                status = 'diagnostic_work_budget'
                break
            if ordered:
                # Preserve charged time and geometry while narrowing the next
                # single-edge call to the remaining shared branch allowance.
                saved = json.loads(session.checkpoint_json())
                saved['config']['route_expansions'] = remaining
                session = runtime.restore(saved)
                current = json.loads(session.observation_json())
        outcome = runtime.step(session, current, action)
        expansions += outcome['details'].get('route_expansions', 0)
        require(expansions <= work, 'routing branch respects its total expansion budget')
        current = json.loads(session.observation_json())
        trace.append(dict(action=action, outcome=outcome, pending=geometry(current)['pending'],
                          elapsed_ms=current['elapsed_ms']))
        require(current['facilities'] == original_poses, 'repair branch moved a facility')
        require(all(current['edges'][i]['path'] == cells for i, cells in original_paths.items()), 'repair branch changed a retained path')
        if outcome['status'] == 'rejected':
            break
        if action['kind'] == 'commit':
            require(outcome['status'] in ('validated', 'improved'), 'complete repair commit status')
            witness = runtime.commit_witness(session, current)
            checker.check(current['blueprint'], witness)
            status = 'validated'
    score = trace[-1]['outcome']['details']['native_acceptance']['metrics']['score'] if status == 'validated' else None
    return dict(name=name, status=status, expansions=expansions, work_allowance=work, search_padding=padding,
        actions=trace, seconds=time.perf_counter() - started, score=score, witness=witness,
        improves_score=score < observation['best_metrics']['score'] if score else False,
        improves_area=score[0] < observation['best_metrics']['score'][0] if score else False)


def probe(runtime, checkpoint, observation, config, checker, guard):
    restore_verified(runtime, checkpoint, observation)
    shape = geometry(observation)
    if shape['conflicts']:
        return dict(classification='overlap_requires_pose', geometry=shape, branches=[])
    if shape['body_blocked_endpoints']:
        return dict(classification='endpoint_requires_pose', geometry=shape, branches=[])
    original = observation['config']
    stages = [('original', original['route_expansions'], original['search_padding']),
              ('expanded_work', config['expanded_route_expansions'], original['search_padding']),
              ('expanded_window', config['expanded_route_expansions'], config['expanded_search_padding'])]
    stages += [(name, config['expanded_route_expansions'], config['expanded_search_padding']) for name in config['ordered_repairs']]
    results = []
    for name, work, padding in stages:
        result = branch(runtime, checkpoint, observation, name, work, padding, config, checker, guard)
        results.append(result)
        if result['status'] == 'validated':
            return dict(classification='fixed_pose_recoverable', geometry=shape, branches=results,
                        recovered_by=name, score=result['score'], improves_score=result['improves_score'],
                        improves_area=result['improves_area'])
    return dict(classification='bounded_routing_unresolved', geometry=shape, branches=results)
