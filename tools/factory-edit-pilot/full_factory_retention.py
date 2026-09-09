"""Admit complete smaller-factory improvement paths for trajectory rehearsal."""
import argparse
from collections import Counter
import copy
from pathlib import Path
import random
import shutil
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_large_learning import read, unpack, verify_sources
from full_factory_policy import observe
from full_factory_screen import sha, write, save_gzip
from full_factory_selection import physical_key


def choose_paths(rows, roots):
    selected = []
    for root in roots:
        candidates = [r for r in rows if r['case'] == root and r['split'] == 'train' and r['budget_complete']
                      and r['best_metrics']['score'] < r['initial_metrics']['score']]
        if not candidates:
            raise ValueError(f'retention.no-complete-teacher: {root}')
        selected.append(min(candidates, key=lambda r: (r['best_metrics']['score'], r['source_order'], r['row_order'])))
    return selected


def role(lesson, large, original):
    if lesson['root'] == large:
        return 'initiating_pose' if lesson['ordinal'] == 0 else 'continuation'
    if lesson['root'] == original:
        return 'original_contraction'
    return 'small_initial' if lesson['ordinal'] == 0 else 'small_followup'


def schedule(lessons, large, original, config, seed):
    groups = {}
    for lesson in lessons:
        if lesson['split'] != 'train':
            raise ValueError('retention.nontraining-lesson')
        groups.setdefault(role(lesson, large, original), []).append(lesson)
    weights = config['role_weights']
    if set(groups) != set(weights) or any(type(v) is not int or v <= 0 for v in weights.values()):
        raise ValueError('retention.role-membership')
    if len({l['id'] for l in lessons}) != len(lessons):
        raise ValueError('retention.duplicate-lesson')
    total = sum(weights.values())
    if config['updates'] % total:
        raise ValueError('retention.incomplete-block')
    rng = random.Random(seed)
    groups['continuation'].sort(key=lambda l: l['ordinal'])
    rng.shuffle(groups['small_initial'])
    roots = [l['root'] for l in groups['small_initial']]
    if len(set(roots)) != len(roots):
        raise ValueError('retention.initial-root-membership')
    followups = {root: sorted([l for l in groups['small_followup'] if l['root'] == root], key=lambda l: l['ordinal']) for root in roots}
    if sum(map(len, followups.values())) != len(groups['small_followup']) or any(not v for v in followups.values()):
        raise ValueError('retention.followup-root-membership')
    rng.shuffle(roots)
    visits = Counter()
    root_visits = Counter()
    result = []
    for _ in range(config['updates']//total):
        block = [name for name, count in weights.items() for _ in range(count)]
        rng.shuffle(block)
        for name in block:
            if name == 'small_followup':
                root = roots[visits[name] % len(roots)]
                item = followups[root][root_visits[root] % len(followups[root])]
                root_visits[root] += 1
            else:
                item = groups[name][visits[name] % len(groups[name])]
            visits[name] += 1
            result.append(dict(update=len(result), lesson=item['id']))
    return result


def prepare(args):
    began = time.perf_counter()
    p, report, audit, admission, lesson_audit = (read(args.parent/name) for name in [
        'protocol.json', 'report.json', 'audit.json', 'admission.json', 'lessons-audit.json'])
    if report['status'] != 'complete' or audit['status'] != 'passed' or audit['report_sha256'] != sha(args.parent/'report.json') or report['protocol_sha256'] != sha(args.parent/'protocol.json'):
        raise ValueError('retention.parent-evaluation-audit')
    if admission['status'] != 'complete' or lesson_audit['status'] != 'passed' or lesson_audit['admission_sha256'] != sha(args.parent/'admission.json') or admission['lessons_sha256'] != sha(args.parent/'lessons.json'):
        raise ValueError('retention.parent-lesson-audit')
    verify_sources(p)
    config = read(args.config)
    permitted = {'role_weights', 'updates', 'observation_cache_bytes'}
    if {k: v for k, v in config.items() if k not in permitted} != {k: v for k, v in p['config'].items() if k not in permitted}:
        raise ValueError('retention.undeclared-config-change')
    if type(config['updates']) is not int or not 1 <= config['updates'] <= 4096 or type(config['observation_cache_bytes']) is not int or not 1 <= config['observation_cache_bytes'] <= 512*1024*1024:
        raise ValueError('retention.training-bound')
    inherited = read(args.parent/'lessons.json')
    for lesson in inherited:
        if sha(lesson['initial']) != lesson['initial_sha256']:
            raise ValueError(f'retention.lesson-hash: {lesson["initial"]}')
    small = [c for c in p['cases'] if c['split'] == 'train' and c['id'] not in [p['large'], p['original_large']]]
    screens = [Path(p['prior_screen']), args.parent]
    rows, sources = [], dict(p['sources'])
    for source_order, screen in enumerate(screens):
        protocol, result, proof = (read(screen/name) for name in ['protocol.json', 'report.json', 'audit.json'])
        if result['status'] != 'complete' or proof['status'] != 'passed' or proof['report_sha256'] != sha(screen/'report.json') or result['protocol_sha256'] != sha(screen/'protocol.json') or protocol['cases'] != p['cases']:
            raise ValueError(f'retention.teacher-screen-audit: {screen}')
        for name in ['protocol.json', 'report.json', 'audit.json']:
            sources[str((screen/name).resolve())] = sha(screen/name)
        for row_order, row in enumerate(result['rows']):
            rows.append(dict(row, source_order=source_order, row_order=row_order, source_screen=str(screen.resolve())))
    chosen = choose_paths(rows, [c['id'] for c in small])
    for row in chosen:
        proof = read(Path(row['source_screen'])/'audit.json')
        record = next(r for r in proof['rows'] if r['key'] == f'{row["method"]}:{row["case"]}:{row["bank"]}')
        for name, key in [('episode.json.gz', 'episode_sha256'), ('best.json.gz', 'best_sha256')]:
            file = Path(row['directory'])/name
            if sha(file) != record[key]:
                raise ValueError(f'retention.teacher-file-hash: {file}')
            sources[str(file.resolve())] = sha(file)
    args.output.mkdir()
    lessons, traces, selections = copy.deepcopy(inherited), [], []
    runtime = FactoryRuntime(p['native_library'], p['catalogs'])
    try:
        for row in chosen:
            case = next(c for c in small if c['id'] == row['case'])
            source = unpack(Path(row['directory'])/'episode.json.gz')
            end = next(i+1 for i, step in enumerate(source['trace']) if step['best_score'] == row['best_metrics']['score'])
            prefix = source['trace'][:end]
            if not prefix or any(t['status'] != 'improved' or t['unchanged'] for t in prefix):
                raise ValueError(f'retention.expected-improving-prefix: {case["id"]}')
            env = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
            path_trace = []
            first = next(l for l in inherited if l['root'] == case['id'] and l['ordinal'] == 0)
            for ordinal, recorded in enumerate(prefix):
                if time.perf_counter()-began >= config['screen_seconds']:
                    raise TimeoutError('retention.admission-budget')
                description = env.describe()
                index = description['actions'].index(recorded['action'])
                item = None
                if ordinal == 0:
                    if recorded['action'] not in [t['action'] for t in first['targets']]:
                        raise ValueError(f'retention.first-target-drift: {case["id"]}')
                else:
                    destination = args.output/f'{case["id"]}--state-{ordinal}.json'
                    write(destination, env.state['current'])
                    obs = observe(env.state['current'], description, env.state, 1-ordinal/config['horizon'])
                    item = dict(id=f'{case["id"]}--lesson-{ordinal}', root=case['id'], family=case['family'], split=case['split'], ordinal=ordinal,
                        initial=str(destination.resolve()), initial_sha256=sha(destination), current_sha256=digest(env.state['current']),
                        initial_metrics=env.state['current_metrics'], episode_initial_metrics=env.state['initial_metrics'], episode_best_metrics=env.state['best_metrics'],
                        observation_context=obs['context'].tolist(), targets=[dict(id=f'{case["id"]}--path-step-{ordinal}', action=recorded['action'], vocabulary_index=index)])
                result = env.step(recorded['action'])
                for key in ['status', 'executor_status', 'route_expansions', 'current_score', 'best_score']:
                    if result[key] != recorded[key]:
                        raise ValueError(f'retention.source-transition: {case["id"]}:{ordinal}:{key}')
                if physical_key(env.state['current']) != recorded['physical_after']:
                    raise ValueError('retention.source-physical-state')
                result = dict(result, current_sha256=digest(env.state['current']))
                path_trace.append(result)
                if item is not None:
                    item['results'] = [result]
                    lessons.append(item)
            if digest(env.state['best']) != source['best_sha256'] or env.state['best_metrics'] != row['best_metrics']:
                raise ValueError('retention.source-best')
            selections.append(dict(root=case['id'], source_screen=row['source_screen'], source_order=row['source_order'], row_order=row['row_order'],
                method=row['method'], bank=row['bank'], directory=row['directory'], score=row['best_metrics']['score'], prefix=end))
            traces.append(dict(root=case['id'], trace=path_trace, best_sha256=digest(env.state['best'])))
    finally:
        runtime.close()
    protocol = copy.deepcopy(p)
    protocol.update(experiment='full-smaller-trajectory-retention', prior_screen=str(args.parent.resolve()), teacher_screens=[str(s.resolve()) for s in screens],
        config=config, selections=selections, source_sha256=source_manifest(), sources=sources,
        schedules={str(seed): schedule(lessons, p['large'], p['original_large'], config, seed) for seed in config['seeds']},
        lesson_roles={l['id']: role(l, p['large'], p['original_large']) for l in lessons})
    for name in ['admission.json', 'lessons.json', 'lessons-audit.json', 'original-contractions.json.gz', 'rehearsal-contrasts.json.gz']:
        protocol['sources'][str((args.parent/name).resolve())] = sha(args.parent/name)
    protocol['sources'][str(args.config.resolve())] = sha(args.config)
    shutil.copyfile(args.parent/'rehearsal-contrasts.json.gz', args.output/'rehearsal-contrasts.json.gz')
    save_gzip(args.output/'teacher-traces.json.gz', traces)
    write(args.output/'lessons.json', lessons)
    write(args.output/'protocol.json', protocol)
    verify_sources(protocol)
    write(args.output/'admission.json', dict(schema_version=1, status='complete', seconds=time.perf_counter()-began,
        protocol_sha256=sha(args.output/'protocol.json'), lessons_sha256=sha(args.output/'lessons.json'), rehearsal_sha256=sha(args.output/'rehearsal-contrasts.json.gz'),
        teacher_traces_sha256=sha(args.output/'teacher-traces.json.gz'), prior_admission_audit_sha256=sha(args.parent/'lessons-audit.json'),
        inherited_lessons=len(inherited), new_lessons=len(lessons)-len(inherited), lessons=len(lessons), targets=sum(len(l['targets']) for l in lessons),
        teacher_actions=sum(len(t['trace']) for t in traces), training_performed=False))
    print(dict(status='admitted', lessons=len(lessons), paths=len(selections), actions=sum(len(t['trace']) for t in traces)), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['parent', 'config', 'output']:
        parser.add_argument('--'+name, type=Path, required=True)
    prepare(parser.parse_args())
