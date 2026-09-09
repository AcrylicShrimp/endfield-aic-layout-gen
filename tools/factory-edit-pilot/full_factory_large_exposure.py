"""Admit original-large contraction targets and rebalance existing trajectory lessons."""
import argparse
from collections import Counter
import copy
from pathlib import Path
import random
import shutil
import time

from full_factory_curriculum import source_manifest
from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_large_learning import read, verify_sources
from full_factory_policy import observe
from full_factory_screen import sha, write, save_gzip


def role(lesson, large, original):
    if lesson['root'] == large:
        return 'initiating_pose' if lesson['ordinal'] == 0 else 'continuation'
    return 'original_contraction' if lesson['root'] == original else 'rehearsal'


def schedule(lessons, large, original, config, seed):
    groups = {}
    for lesson in lessons:
        if lesson['split'] != 'train':
            raise ValueError('large-exposure.nontraining-lesson')
        groups.setdefault(role(lesson, large, original), []).append(lesson)
    weights = config['role_weights']
    if set(groups) != set(weights) or any(type(v) is not int or v <= 0 for v in weights.values()):
        raise ValueError('large-exposure.role-membership')
    if len(groups['initiating_pose']) != 1 or len(groups['original_contraction']) != 1:
        raise ValueError('large-exposure.initial-lesson-count')
    if len({lesson['id'] for lesson in lessons}) != len(lessons):
        raise ValueError('large-exposure.duplicate-lesson')
    total = sum(weights.values())
    if config['updates'] % total:
        raise ValueError('large-exposure.incomplete-block')
    rng = random.Random(seed)
    groups['continuation'].sort(key=lambda lesson: lesson['ordinal'])
    rng.shuffle(groups['rehearsal'])
    visits = Counter()
    result = []
    for _ in range(config['updates']//total):
        block = [name for name, count in weights.items() for _ in range(count)]
        rng.shuffle(block)
        for name in block:
            item = groups[name][visits[name] % len(groups[name])]
            visits[name] += 1
            result.append(dict(update=len(result), lesson=item['id']))
    return result


def prepare(args):
    began = time.perf_counter()
    p, report, audit, previous_admission, previous_audit = (read(args.parent/name) for name in [
        'protocol.json', 'report.json', 'audit.json', 'admission.json', 'lessons-audit.json'])
    if report['status'] != 'complete' or audit['status'] != 'passed' or audit['report_sha256'] != sha(args.parent/'report.json') or report['protocol_sha256'] != sha(args.parent/'protocol.json'):
        raise ValueError('large-exposure.parent-evaluation-audit')
    if previous_admission['status'] != 'complete' or previous_audit['status'] != 'passed' or previous_audit['admission_sha256'] != sha(args.parent/'admission.json') or previous_admission['lessons_sha256'] != sha(args.parent/'lessons.json'):
        raise ValueError('large-exposure.parent-lesson-audit')
    verify_sources(p)
    config = read(args.config)
    if {k: v for k, v in config.items() if k != 'role_weights'} != p['config']:
        raise ValueError('large-exposure.only-curriculum-may-change')
    lessons = read(args.parent/'lessons.json')
    for lesson in lessons:
        if sha(lesson['initial']) != lesson['initial_sha256']:
            raise ValueError(f'large-exposure.lesson-hash: {lesson["initial"]}')
    case = next(c for c in p['cases'] if c['id'] == p['original_large'])
    runtime = FactoryRuntime(p['native_library'], p['catalogs'])
    rows = []
    args.output.mkdir()
    try:
        env = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
        description = env.describe()
        selected = [(i, action) for i, action in enumerate(description['actions']) if action['kind'] == 'contract']
        if not selected:
            raise ValueError('large-exposure.no-original-contraction')
        obs = observe(env.state['current'], description, env.state, 1)
        item = dict(id=f'{case["id"]}--lesson-0', root=case['id'], family=case['family'], split=case['split'], ordinal=0,
                    initial=case['initial'], initial_sha256=sha(case['initial']), current_sha256=digest(env.state['current']),
                    initial_metrics=env.state['current_metrics'], episode_initial_metrics=env.state['initial_metrics'],
                    episode_best_metrics=env.state['best_metrics'], observation_context=obs['context'].tolist())
        for index, action in selected:
            if time.perf_counter()-began >= config['screen_seconds']:
                raise TimeoutError('large-exposure.admission-budget')
            trial = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
            result = trial.step(action)
            result['current_sha256'] = digest(trial.state['current'])
            rows.append(dict(id=f'original-contraction-{len(rows)}', vocabulary_index=index, action=action,
                completed=result['status'] not in ['late', 'budget_exhausted'], status=result['status'],
                result=result, best_metrics=trial.state['best_metrics']))
        positives = [r for r in rows if r['completed'] and r['status'] == 'improved']
        if not positives:
            raise ValueError('large-exposure.no-positive-original-contraction')
        best = min(r['best_metrics']['score'] for r in positives)
        targets = [r for r in positives if r['best_metrics']['score'] == best]
        item['targets'] = [dict(id=r['id'], action=r['action'], vocabulary_index=r['vocabulary_index']) for r in targets]
        item['results'] = [r['result'] for r in targets]
        lessons.append(item)
    finally:
        runtime.close()
    protocol = copy.deepcopy(p)
    protocol.update(experiment='large-initiating-pose-exposure', prior_screen=str(args.parent.resolve()), config=config,
        schedules={str(seed): schedule(lessons, p['large'], p['original_large'], config, seed) for seed in config['seeds']},
        source_sha256=source_manifest(), lesson_roles={l['id']: role(l, p['large'], p['original_large']) for l in lessons})
    for name in ['protocol.json', 'report.json', 'audit.json', 'admission.json', 'lessons.json', 'lessons-audit.json', 'rehearsal-contrasts.json.gz']:
        protocol['sources'][str((args.parent/name).resolve())] = sha(args.parent/name)
    protocol['sources'][str(args.config.resolve())] = sha(args.config)
    shutil.copyfile(args.parent/'rehearsal-contrasts.json.gz', args.output/'rehearsal-contrasts.json.gz')
    save_gzip(args.output/'original-contractions.json.gz', rows)
    write(args.output/'lessons.json', lessons)
    write(args.output/'protocol.json', protocol)
    verify_sources(protocol)
    write(args.output/'admission.json', dict(schema_version=1, status='complete', seconds=time.perf_counter()-began,
        protocol_sha256=sha(args.output/'protocol.json'), lessons_sha256=sha(args.output/'lessons.json'),
        rehearsal_sha256=sha(args.output/'rehearsal-contrasts.json.gz'), original_contractions_sha256=sha(args.output/'original-contractions.json.gz'),
        lessons=len(lessons), targets=sum(len(l['targets']) for l in lessons), inherited_lessons=len(lessons)-1,
        prior_admission_audit_sha256=sha(args.parent/'lessons-audit.json'), original_contractions=len(rows),
        original_target_score=best, original_target_ids=[r['id'] for r in targets], training_performed=False))
    print(dict(status='admitted', lessons=len(lessons), original_contractions=len(rows), targets=[r['id'] for r in targets], score=best), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['parent', 'config', 'output']:
        parser.add_argument('--'+name, type=Path, required=True)
    prepare(parser.parse_args())
