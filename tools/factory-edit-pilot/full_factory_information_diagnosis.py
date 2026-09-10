"""Audit hidden incumbent state and access to measured improving factory paths."""
import argparse
from collections import Counter, defaultdict
from itertools import combinations
from pathlib import Path
import time

import torch

from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_large_learning import read, unpack, verify_sources
from full_factory_learnability_audit import observation_key
from full_factory_on_policy import parameter_hash, relative_returns
from full_factory_on_policy_result import load_result
from full_factory_policy import FullFactoryPolicy, observe
from full_factory_screen import sha, write
from full_factory_selection import SelectionMemory, action_key, distribution, physical_key


def require(condition, detail):
    if not condition:
        raise ValueError(f'information-diagnosis.identity: {detail}')


def collect_credit_and_aliases(result, proof, sources, guard):
    groups = {(g['initialization'], g['group']): g for g in result['groups']}
    audited = {r['key']: r for r in proof['rows']}
    buckets, stats = defaultdict(list), defaultdict(Counter)
    for group in groups.values():
        require(relative_returns(group['scores']) == group['returns'], 'relative returns')
        role = 'prepared' if group['case'].endswith('--contraction-exhausted') else 'original'
        stat = stats[group['initialization'], role]
        stat['groups'] += 1
        stat['flat_groups'] += not any(group['returns'])
    for row in result['rows']:
        guard()
        group = groups.get((row['initialization'], row['group']))
        if group is None:
            continue
        path = Path(row['directory']) / 'episode.json.gz'
        expected = audited[f"{row['initialization']}:{row['group']}:{row['attempt']}"]['episode_sha256']
        require(sha(path) == expected, str(path))
        sources[str(path.resolve())] = expected
        episode = unpack(path)
        advantage = group['returns'][row['attempt']]
        role = 'prepared' if row['case'].endswith('--contraction-exhausted') else 'original'
        stat = stats[row['initialization'], role]
        stat['episodes'] += 1
        stat['nonzero_return_episodes'] += advantage != 0
        best = episode['initial_metrics']['score']
        best_key = episode['trace'][0]['physical_before']
        last = max((i for i, t in enumerate(episode['trace']) if t['status'] == 'improved'), default=-1)
        stat['episodes_without_improvement'] += last < 0
        for ordinal, step in enumerate(episode['trace']):
            buckets[step['observation_sha256'], step['allowed_sha256']].append(dict(
                case=row['case'], directory=row['directory'], ordinal=ordinal,
                initialization=row['initialization'], group=row['group'], attempt=row['attempt'],
                best_key=best_key, best_score=best, current_key=step['physical_before']))
            if advantage:
                stat['credited_actions'] += 1
                stat['credited_after_last_improvement'] += ordinal > last
                stat['credited_unchanged'] += step['unchanged']
                stat['credited_executor_rejected'] += step['status'] == 'executor_rejected'
            if step['status'] == 'improved':
                best_key = step['physical_after']
            best = step['best_score']
    aliases = [dict(observation_sha256=key[0], allowed_sha256=key[1], occurrences=rows)
               for key, rows in buckets.items()
               if len({(r['best_key'], tuple(r['best_score'])) for r in rows}) > 1]
    pairs = {}
    for bucket in aliases:
        for a, b in combinations(bucket['occurrences'], 2):
            if a['best_key'] == b['best_key'] or a['current_key'] != b['current_key']:
                continue
            kind = 'omitted_best_turns' if a['best_score'] != b['best_score'] else 'omitted_best_geometry'
            if kind not in pairs:
                pairs[kind] = dict(kind=kind, observation_sha256=bucket['observation_sha256'],
                                   allowed_sha256=bucket['allowed_sha256'], pair=[a, b])
    return dict(credit=[dict(initialization=arm, reset=role, **s) for (arm, role), s in stats.items()],
                alias_buckets=aliases, selected_pairs=list(pairs.values()))


def mask_for(env, description, memory, enabled):
    keys = [action_key(a, description, env.state['current'], physical_key(env.state['best']))
            for a in description['actions']]
    allowed, exhausted = memory.mask(physical_key(env.state['current']), keys, description['actions'], enabled)
    return keys, allowed, exhausted


def replay_alias(runtime, protocol, cases, pair, guard):
    config = protocol['config']
    records = []
    for occurrence in pair['pair']:
        guard()
        trace = unpack(Path(occurrence['directory']) / 'episode.json.gz')['trace']
        env = FactoryEpisode(runtime, cases[occurrence['case']], config['horizon'],
                             config['episode_seconds'], config['route_work'])
        memory = SelectionMemory(env.state['current'])
        for ordinal in range(occurrence['ordinal'] + 1):
            guard()
            before = physical_key(env.state['current'])
            description = env.describe()
            keys, allowed, exhausted = mask_for(env, description, memory, config['memory'])
            obs = observe(env.state['current'], description, env.state, 1 - ordinal / config['horizon'])
            old = trace[ordinal]
            require(observation_key(obs) == old['observation_sha256'], 'replayed alias observation')
            require(digest(allowed) == old['allowed_sha256'] and exhausted == old['memory_exhausted'], 'alias mask')
            if ordinal == occurrence['ordinal']:
                require(physical_key(env.state['best']) == occurrence['best_key'], 'alias incumbent geometry')
                require(env.state['best_metrics']['score'] == occurrence['best_score'], 'alias incumbent score')
                restore = next(i for i, a in enumerate(description['actions']) if a['kind'] == 'restore_best')
                require(allowed[restore], 'alias restore allowed')
                step = env.step(description['actions'][restore])
                require(step.get('acceptance', {}).get('candidate_validated'), 'restore native validation')
                require(physical_key(env.state['current']) == occurrence['best_key'], 'restore target')
                records.append(dict(**occurrence, observation_sha256=observation_key(obs),
                    allowed_sha256=digest(allowed), restore_index=restore, restore_action=description['actions'][restore],
                    restored_key=physical_key(env.state['current']), restored_score=step['current_score'],
                    native_validated=True, replayed_actions=ordinal + 1))
                break
            require(description['actions'][old['vocabulary_index']] == old['action'], 'alias action membership')
            step = env.step(old['action'])
            require(step['status'] == old['status'] and step['current_score'] == old['current_score']
                    and step['best_score'] == old['best_score'] and digest(env.state['current']) == old['current_sha256'],
                    'alias physical replay')
            memory.record(before, physical_key(env.state['current']), keys[old['vocabulary_index']])
    require(len(records) == 2, 'two alias records')
    require(records[0]['observation_sha256'] == records[1]['observation_sha256'] == pair['observation_sha256'], 'equal pair input')
    require(records[0]['allowed_sha256'] == records[1]['allowed_sha256'] == pair['allowed_sha256'], 'equal pair mask')
    require(records[0]['restore_action'] == records[1]['restore_action'] and
            records[0]['restore_index'] == records[1]['restore_index'], 'equal restore action')
    require(records[0]['restored_key'] != records[1]['restored_key'], 'different restored outcomes')
    return dict(kind=pair['kind'], records=records, same_input_different_transition=True)


def snapshots(protocol, result, sources):
    output = []
    for arm in result['arms']:
        name = arm['initialization']
        for update in sorted({0, 16, 32, 48, arm['groups']}):
            require(update <= arm['groups'], 'snapshot count')
            if update == 0:
                path, expected = Path(arm['initial']), arm['initial_sha256']
            else:
                row = next(g for g in result['groups'] if g['initialization'] == name and g['group'] == update - 1)
                path, expected = Path(row['checkpoint']), row['checkpoint_sha256']
            require(sha(path) == expected, str(path))
            sources[str(path.resolve())] = expected
            checkpoint = torch.load(path, map_location='cpu', weights_only=False)
            model = FullFactoryPolicy(protocol['depth'], protocol['width'])
            model.load_state_dict(checkpoint['policy'], strict=True)
            require(parameter_hash(model) == checkpoint['policy_sha256'], 'snapshot policy hash')
            output.append(dict(initialization=name, update=update, model=model, policy_sha256=parameter_hash(model)))
    return output


def path_access(runtime, protocol, cases, teachers, models, sources, guard):
    config, rows = protocol['config'], []
    for root in teachers['rows']:
        guard()
        path = Path(root['directory']) / 'trajectories.json.gz'
        require(sha(path) == root['trajectories_sha256'], str(path))
        sources[str(path.resolve())] = sha(path)
        trials = unpack(path)
        successful = [t for t in trials if t['summary']['best_metrics']['score'][0] < root['initial_metrics']['score'][0]]
        require(len(successful) == root['area_improved'], 'successful trial count')
        teacher = next(t for t in trials if t['id'] == min(root['targets']))
        env = FactoryEpisode(runtime, cases[root['case']], config['horizon'],
                             config['episode_seconds'], config['route_work'])
        require(env.state['initial_metrics'] == root['initial_metrics'], 'teacher root metrics')
        memory = SelectionMemory(env.state['current'])
        probability_rows = [dict(initialization=m['initialization'], update=m['update'],
                                 selected_path_probabilities=[], contraction_mass_by_step=[]) for m in models]
        for ordinal, old in enumerate(teacher['summary']['trace']):
            guard()
            description = env.describe()
            keys, allowed, _ = mask_for(env, description, memory, config['memory'])
            obs = observe(env.state['current'], description, env.state, 1 - ordinal / config['horizon'])
            index = description['actions'].index(old['action'])
            require(allowed[index], 'teacher path allowed by training memory')
            successful_indices = sorted({description['actions'].index(t['action']) for t in successful}) if ordinal == 0 else []
            pose_indices = [i for i, a in enumerate(description['actions']) if a['kind'] == 'pose']
            contract_indices = [i for i, a in enumerate(description['actions']) if a['kind'] == 'contract']
            with torch.no_grad():
                for model, row in zip(models, probability_rows):
                    probs = distribution(model['model'](obs) / config['temperature'], description['actions'], allowed, config['balanced']).probs
                    row['selected_path_probabilities'].append(float(probs[index]))
                    row['contraction_mass_by_step'].append(float(probs[contract_indices].double().sum()))
                    if ordinal == 0:
                        mass = float(probs[successful_indices].double().sum())
                        pose_mass = float(probs[pose_indices].double().sum())
                        good_pose = [i for i in successful_indices if description['actions'][i]['kind'] == 'pose']
                        good_pose_mass = float(probs[good_pose].double().sum())
                        row.update(measured_improving_start_mass=mass, pose_mass=pose_mass,
                            measured_improving_pose_mass=good_pose_mass,
                            improving_conditional_pose_mass=good_pose_mass / pose_mass if pose_mass else None,
                            root_actions=len(description['actions']), root_poses=len(pose_indices))
            before = physical_key(env.state['current'])
            step = env.step(old['action'])
            require(step['status'] == old['status'] and step['current_score'] == old['current_score']
                    and step['best_score'] == old['best_score'] and digest(env.state['current']) == old['current_sha256'],
                    'teacher native replay')
            require(step.get('acceptance', {}).get('candidate_validated'), 'teacher native validation')
            memory.record(before, physical_key(env.state['current']), keys[index])
        require(env.state['best_metrics'] == root['teacher_metrics'], 'teacher final score')
        for row in probability_rows:
            probability = 1.0
            for value in row['selected_path_probabilities']:
                probability *= value
            row['exact_selected_path_probability'] = probability
        rows.append(dict(case=root['case'], measured_trials=len(trials), measured_improving_starts=len(successful),
            successful_start_kinds=dict(Counter(t['action']['kind'] for t in successful)),
            path_selected=len(env.trace), path_native_validated=True, teacher_score=env.state['best_metrics']['score'],
            teacher_actions=[t['action'] for t in env.trace], snapshots=probability_rows))
        print(dict(stage='path-access', case=root['case'], selections=len(env.trace)), flush=True)
    for m in models:
        require(parameter_hash(m['model']) == m['policy_sha256'], 'policy unchanged')
    return rows


def run(args):
    began = time.perf_counter()
    if not 0 < args.seconds <= 1800:
        raise ValueError('information-diagnosis.bound: seconds must be in 1..1800')
    args.output.mkdir()
    stage, entity, runtime = 'inputs', None, None
    report, sources = {}, {}

    def guard():
        if time.perf_counter() - began >= args.seconds:
            raise TimeoutError('information-diagnosis.time-bound')

    try:
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        protocol, result = load_result(args.training)
        proof = read(args.training / 'training-audit.json')
        checkpoint_proof = read(args.training / 'checkpoint-audit.json')
        teacher_protocol, teachers, teacher_proof = (read(args.teachers / n) for n in ('protocol.json', 'report.json', 'audit.json'))
        verify_sources(teacher_protocol)
        require(proof['status'] == checkpoint_proof['status'] == teacher_proof['status'] == 'passed', 'audits passed')
        require(proof['report_sha256'] == checkpoint_proof['report_sha256'] == sha(args.training / 'training-result.json'), 'training audit identity')
        require(proof['checkpoint_audit_sha256'] == sha(args.training / 'checkpoint-audit.json'), 'checkpoint audit binding')
        require(teachers['status'] == 'complete' and teacher_proof['report_sha256'] == sha(args.teachers / 'report.json')
                and teachers['protocol_sha256'] == sha(args.teachers / 'protocol.json'), 'teacher audit identity')
        require(protocol['native_library'] == teacher_protocol['native_library'] and
                read(protocol['catalogs']) == read(teacher_protocol['catalogs']), 'shared native and catalogs')
        for directory, names in [(args.training, ['protocol.json', 'training-result.json', 'training-audit.json', 'checkpoint-audit.json']),
                                  (args.teachers, ['protocol.json', 'report.json', 'audit.json'])]:
            sources.update({str((directory / n).resolve()): sha(directory / n) for n in names})
        sources[str(Path(__file__).resolve())] = sha(__file__)
        write(args.output / 'protocol.json', dict(schema_version=1, experiment='policy-information-and-credit-diagnosis',
            training=str(args.training.resolve()), teachers=str(args.teachers.resolve()), seconds_bound=args.seconds,
            snapshot_updates=[0, 16, 32, 48, 'each_endpoint'], training_performed=False,
            contract='docs/designs/2026-09-09.37-policy-information-and-credit-diagnosis.md'))
        cases = {c['id']: c for c in protocol['cases']}
        stage = 'credit-and-aliases'
        report.update(collect_credit_and_aliases(result, proof, sources, guard))
        require({p['kind'] for p in report['selected_pairs']} == {'omitted_best_turns', 'omitted_best_geometry'}, 'both alias kinds available')
        runtime = FactoryRuntime(protocol['native_library'], protocol['catalogs'])
        stage = 'alias-replay'
        report['alias_replays'] = []
        for pair in report['selected_pairs']:
            entity = pair['kind']
            report['alias_replays'].append(replay_alias(runtime, protocol, cases, pair, guard))
            print(dict(stage=stage, kind=entity, status='verified'), flush=True)
        stage, entity = 'path-access', None
        models = snapshots(protocol, result, sources)
        report['path_access'] = path_access(runtime, protocol, cases, teachers, models, sources, guard)
        guard()
        verify_sources(protocol)
        verify_sources(teacher_protocol)
        for path, expected in sources.items():
            require(sha(path) == expected, f'final source identity: {path}')
        write(args.output / 'sources.json', sources)
        report.update(schema_version=1, status='complete', seconds=time.perf_counter() - began,
            protocol_sha256=sha(args.output / 'protocol.json'), sources_sha256=sha(args.output / 'sources.json'),
            training_performed=False,
            scope='Actual observation aliasing and finite measured-path access; no claim of conflicting optimal actions, exhaustive success coverage, gradient variance or causal endpoint improvement.')
        write(args.output / 'report.json', report)
        print(dict(status='complete', aliases=len(report['alias_buckets']), paths=len(report['path_access']), seconds=report['seconds']), flush=True)
    except BaseException as error:
        write(args.output / 'partial.json', dict(schema_version=1, status='incomplete', code='information-diagnosis.failed',
            severity='error', stage=stage, entity=entity, message=str(error), findings=report, sources=sources))
        raise
    finally:
        if runtime is not None:
            runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('training', 'teachers', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--seconds', type=int, default=900)
    run(parser.parse_args())
