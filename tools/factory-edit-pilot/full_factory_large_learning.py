"""Admit and learn a large compression trajectory with original-root rehearsal."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import random
import time

import numpy as np
import torch

from full_factory_best_learning import best_mass_loss
from full_factory_curriculum import source_manifest
from full_factory_decoding import TemperaturePolicy
from full_factory_environment import ROOT, FactoryRuntime, FactoryEpisode, digest
from full_factory_landscape import episode
from full_factory_policy import SCHEMA, FullFactoryPolicy, observe
from full_factory_screen import sha, write, save_gzip
from full_factory_trajectory_learning import trajectory_state


def read(path):
    return json.loads(Path(path).read_text())


def unpack(path):
    return json.loads(gzip.decompress(Path(path).read_bytes()))


def schedule(lessons, updates, seed):
    roots = {}
    for lesson in lessons:
        if lesson['split'] != 'train':
            raise ValueError('large-learning.nontraining-lesson')
        roots.setdefault(lesson['root'], []).append(lesson)
    if not roots or len({lesson['id'] for lesson in lessons}) != len(lessons):
        raise ValueError('large-learning.lesson-membership')
    order = list(roots)
    random.Random(seed).shuffle(order)
    for group in roots.values():
        group.sort(key=lambda lesson: lesson['ordinal'])
    return [dict(update=i, lesson=roots[order[i % len(order)]][
        (i // len(order)) % len(roots[order[i % len(order)]])]['id']) for i in range(updates)]


def verify_sources(protocol):
    for name, expected in protocol['sources'].items():
        if sha(name) != expected:
            raise ValueError(f'large-learning.input-hash: {name}')
    for name, expected in protocol['source_sha256'].items():
        if sha(ROOT/name) != expected:
            raise ValueError(f'large-learning.source-hash: {name}')


def prepare(args):
    began = time.perf_counter()
    config = read(args.config)
    if config.get('schema_version') != 1:
        raise ValueError(f'large-learning.study-schema: {args.config}')
    for key, limit in [('updates', 4096), ('horizon', 64), ('episode_seconds', 300),
                       ('route_work', 100000), ('screen_seconds', 7200),
                       ('observation_cache_bytes', 512*1024*1024)]:
        if type(config.get(key)) is not int or not 0 < config[key] <= limit:
            raise ValueError(f'large-learning.study-bound: {key}')
    for key in ['seeds', 'banks']:
        values = config.get(key)
        if not isinstance(values, list) or not values or len(values) != len(set(values)) or any(type(v) is not int or v < 0 for v in values):
            raise ValueError(f'large-learning.study-seeds: {key}')
    for key in ['learning_rate', 'gradient_clip', 'temperature']:
        if not isinstance(config.get(key), (int, float)) or not 0 < config[key] <= 1:
            raise ValueError(f'large-learning.study-scalar: {key}')
    p, report, audit = (read(args.continuations/name) for name in ['protocol.json', 'report.json', 'audit.json'])
    if report['status'] != 'complete' or audit['status'] != 'passed' or audit['report_sha256'] != sha(args.continuations/'report.json') or report['protocol_sha256'] != sha(args.continuations/'protocol.json'):
        raise ValueError('large-learning.continuation-audit')
    verify_sources(p)
    paths = json.loads(Path(p['catalogs']).read_text())
    old, old_report, old_audit = (read(args.parent/name) for name in ['protocol.json', 'report.json', 'audit-capabilities.json'])
    checkpoint_path = args.parent/'best-mass-final.pt'
    if old_report['status'] != 'complete' or old_report['protocol_sha256'] != sha(args.parent/'protocol.json') or old_audit['status'] != 'passed' or old_audit['report_sha256'] != sha(args.parent/'report.json') or old_audit['checkpoints']['best-mass'] != sha(checkpoint_path):
        raise ValueError('large-learning.parent-checkpoint-audit')
    checkpoint = torch.load(checkpoint_path, map_location='cpu', weights_only=False)
    if checkpoint['schema'] != SCHEMA or checkpoint['protocol_sha256'] != sha(args.parent/'protocol.json'):
        raise ValueError('large-learning.parent-checkpoint-provenance')
    if {k: sha(v) for k, v in read(old['catalogs']).items()} != {k: sha(v) for k, v in paths.items()}:
        raise ValueError('large-learning.rehearsal-catalogs')
    collection = Path(old['collection'])
    labels_data, collection_audit = (read(collection/name) for name in ['contrasts.json', 'audit.json'])
    if collection_audit['status'] != 'passed' or collection_audit['contrasts_sha256'] != sha(collection/'contrasts.json') or labels_data['protocol_sha256'] != sha(collection/'protocol.json'):
        raise ValueError('large-learning.rehearsal-label-audit')
    labels = {row['id']: row for row in labels_data['rows']}
    traces = unpack(args.continuations/'trajectories.json.gz')
    if report['trajectories_sha256'] != sha(args.continuations/'trajectories.json.gz') or audit['replayed_actions'] != sum(t['summary']['selected'] for t in traces):
        raise ValueError('large-learning.continuation-traces')
    winning = [t for t in traces if t['completed'] and t['summary']['best_metrics']['score'] == report['best_metrics']['score']]
    if [t['id'] for t in winning] != report['targets'] or len(winning) != 1:
        raise ValueError('large-learning.expected-unique-trajectory')
    target = winning[0]
    if target['summary']['selected'] > config['horizon'] or target['summary']['best_metrics']['score'][0] >= report['initial_metrics']['score'][0]:
        raise ValueError('large-learning.noncompression-teacher')
    large = dict(p['case'], split=p['case']['family_split'])
    original_inputs = read(args.original_inputs)
    original = next(c for c in original_inputs['rows'] if c['id'] == large['id'].removesuffix('--contraction-exhausted'))
    original = dict(original, family=large['family'], split=large['split'])
    if sha(original['original']) != sha(large['original']) or sha(original['request']) != sha(large['request']):
        raise ValueError('large-learning.large-original-identity')
    cases = old['cases'] + [large, original]
    if len({c['id'] for c in cases}) != len(cases):
        raise ValueError('large-learning.duplicate-case')
    families = {}
    for case in cases:
        families.setdefault(case['family'], set()).add(case['split'])
    if any(len(splits) != 1 for splits in families.values()):
        raise ValueError('large-learning.family-leakage')
    sources = dict(p['sources'])
    files = [args.config, args.original_inputs, args.native_library, checkpoint_path]
    files += [args.continuations/n for n in ['protocol.json', 'report.json', 'audit.json', 'trajectories.json.gz']]
    files += [args.parent/n for n in ['protocol.json', 'report.json', 'audit-capabilities.json']]
    files += [collection/n for n in ['protocol.json', 'contrasts.json', 'audit.json']]
    for case in cases:
        for key, name in [('initial', 'initial.json'), ('request', 'request.json'), ('original', 'original-request.json')]:
            if sha(case[key]) != case['files_sha256'][name]:
                raise ValueError(f'large-learning.case-hash: {case[key]}')
            files.append(Path(case[key]))
    sources.update({str(f.resolve()): sha(f) for f in files})
    args.output.mkdir()
    lessons, rehearsal = [], []
    runtime = FactoryRuntime(args.native_library, p['catalogs'])

    def lesson(case, env, ordinal, actions, ids, initial):
        description = env.describe()
        obs = observe(env.state['current'], description, env.state, 1-ordinal/config['horizon'])
        return dict(id=f'{case["id"]}--lesson-{ordinal}', root=case['id'], family=case['family'], split=case['split'],
                    ordinal=ordinal, initial=str(Path(initial).resolve()), initial_sha256=sha(initial),
                    current_sha256=digest(env.state['current']), initial_metrics=env.state['current_metrics'],
                    episode_initial_metrics=env.state['initial_metrics'], episode_best_metrics=env.state['best_metrics'],
                    observation_context=obs['context'].tolist(), targets=[dict(id=i, action=a,
                    vocabulary_index=description['actions'].index(a)) for i, a in zip(ids, actions)], results=[])
    try:
        for teacher in old['teachers']:
            if teacher['split'] != 'train':
                continue
            case = next(c for c in cases if c['id'] == teacher['root'])
            env = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
            actions = env.describe()['actions']
            group = []
            for label in labels_data['rows']:
                if label['root'] != case['id']:
                    continue
                if time.perf_counter()-began >= config['screen_seconds']:
                    raise TimeoutError('large-learning.admission-budget')
                record = dict(id=label['id'], root=case['id'], action=label['action'], old_best_metrics=label['best_metrics'])
                if label['action'] not in actions:
                    record.update(vocabulary_index=None, status='not_offered', completed=False)
                    rehearsal.append(record)
                    continue
                trial = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
                step = trial.step(label['action'])
                result = dict(step, current_sha256=digest(trial.state['current']))
                record.update(vocabulary_index=actions.index(label['action']), status=step['status'],
                    completed=step['status'] not in ['late', 'budget_exhausted'], result=result,
                    best_metrics=trial.state['best_metrics'])
                rehearsal.append(record)
                if record['completed'] and record['status'] == 'improved':
                    group.append(record)
            if not group:
                raise ValueError(f'large-learning.rehearsal-no-teacher: {case["id"]}')
            minimum = min(row['best_metrics']['score'] for row in group)
            targets = [row for row in group if row['best_metrics']['score'] == minimum]
            item = lesson(case, env, 0, [r['action'] for r in targets], [r['id'] for r in targets], case['initial'])
            item['results'] = [r['result'] for r in targets]
            lessons.append(item)
        env = FactoryEpisode(runtime, large, config['horizon'], config['episode_seconds'], config['route_work'])
        for ordinal, step in enumerate(target['summary']['trace']):
            initial = Path(large['initial'])
            if ordinal:
                initial = args.output/f'large-lesson-{ordinal}.json'
                write(initial, env.state['current'])
            item = lesson(large, env, ordinal, [step['action']], [f'trajectory-{target["id"]}-step-{ordinal}'], initial)
            result = env.step(step['action'])
            if result['status'] != step['status'] or digest(env.state['current']) != step['current_sha256'] or result['route_expansions'] != step['route_expansions']:
                raise ValueError(f'large-learning.trajectory-readmission: {ordinal}')
            item['results'].append(dict(result, current_sha256=digest(env.state['current'])))
            lessons.append(item)
        if env.state['best_metrics'] != report['best_metrics']:
            raise ValueError('large-learning.trajectory-score')
    finally:
        runtime.close()
    protocol = dict(schema_version=1, experiment='large-trajectory-original-rehearsal', config=config,
        parent=str(args.parent.resolve()), checkpoint=str(checkpoint_path.resolve()), continuations=str(args.continuations.resolve()),
        cases=cases, catalogs=p['catalogs'], native_library=str(args.native_library.resolve()), schema=SCHEMA,
        depth=old['depth'], width=old['width'], large=large['id'], original_large=original['id'], sources=sources,
        source_sha256=source_manifest(), schedules={str(seed): schedule(lessons, config['updates'], seed) for seed in config['seeds']},
        methods=['parent']+[f'learned-{seed}' for seed in config['seeds']]+['geometry'],
        legacy_label_readmission='Remeasure every historical training-root contrast under the current runtime and recompute full-score target ties; independent original-graph audit required before optimization')
    save_gzip(args.output/'rehearsal-contrasts.json.gz', rehearsal)
    write(args.output/'lessons.json', lessons)
    write(args.output/'protocol.json', protocol)
    verify_sources(protocol)
    write(args.output/'admission.json', dict(schema_version=1, status='complete', seconds=time.perf_counter()-began,
        protocol_sha256=sha(args.output/'protocol.json'), lessons_sha256=sha(args.output/'lessons.json'),
        rehearsal_sha256=sha(args.output/'rehearsal-contrasts.json.gz'), rehearsal_contrasts=len(rehearsal),
        lessons=len(lessons), targets=sum(len(l['targets']) for l in lessons), training_performed=False))
    print(json.dumps(dict(status='admitted', lessons=len(lessons))), flush=True)


def run(args):
    began = time.perf_counter()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    p, admitted, audit = (read(args.screen/n) for n in ['protocol.json', 'admission.json', 'lessons-audit.json'])
    config = p['config']
    lessons = read(args.screen/'lessons.json')
    if admitted['status'] != 'complete' or admitted['protocol_sha256'] != sha(args.screen/'protocol.json') or admitted['lessons_sha256'] != sha(args.screen/'lessons.json') or audit['status'] != 'passed' or audit['admission_sha256'] != sha(args.screen/'admission.json'):
        raise ValueError('large-learning.admission-audit')
    if (args.screen/'report.json').exists() or (args.screen/'updates.json').exists():
        raise ValueError('large-learning.existing-training: use a fresh screen')
    verify_sources(p)
    cases = {c['id']: c for c in p['cases']}
    cache, cache_bytes, scores, updates, rows = {}, 0, [], [], []
    stage = 'observations'

    def guard():
        if time.perf_counter()-began >= config['screen_seconds']:
            raise TimeoutError(f'large-learning.study-budget: {stage}')

    runtime = FactoryRuntime(p['native_library'], p['catalogs'])
    try:
        for item in lessons:
            guard()
            if sha(item['initial']) != item['initial_sha256']:
                raise ValueError(f'large-learning.lesson-hash: {item["initial"]}')
            env = FactoryEpisode(runtime, dict(cases[item['root']], initial=item['initial']), config['horizon'], config['episode_seconds'], config['route_work'])
            description = env.describe()
            obs = observe(env.state['current'], description, trajectory_state(env.state, item), 1-item['ordinal']/config['horizon'])
            if obs['context'].tolist() != item['observation_context'] or digest(env.state['current']) != item['current_sha256']:
                raise ValueError('large-learning.lesson-context')
            indices = [description['actions'].index(t['action']) for t in item['targets']]
            if indices != [t['vocabulary_index'] for t in item['targets']]:
                raise ValueError('large-learning.lesson-membership')
            cache[item['id']] = obs, indices, description['actions']
            cache_bytes += sum(v.nbytes for v in obs.values() if isinstance(v, np.ndarray))
            if cache_bytes > config['observation_cache_bytes']:
                raise ValueError('large-learning.observation-cache-bound')
    finally:
        runtime.close()

    def label_scores(model, method):
        for item in lessons:
            guard()
            obs, targets, actions = cache[item['id']]
            with torch.no_grad():
                logits = model(obs)
            if not torch.isfinite(logits).all():
                raise ValueError('large-learning.nonfinite-label-logits')
            scores.append(dict(method=method, lesson=item['id'], root=item['root'], ordinal=item['ordinal'],
                top1=int(logits.argmax()) in targets, rank=1+int((logits > logits[targets].max()).sum()),
                target_mass=float(torch.softmax(logits, 0)[targets].sum()), top_action=actions[int(logits.argmax())]))

    parent = torch.load(p['checkpoint'], map_location='cpu', weights_only=False)
    try:
        stage = 'training'
        for seed in config['seeds']:
            torch.manual_seed(seed)
            model = FullFactoryPolicy(p['depth'], p['width'])
            model.load_state_dict(parent['policy'])
            if seed == config['seeds'][0]:
                label_scores(model, 'parent')
            optimizer = torch.optim.Adam(model.parameters(), lr=config['learning_rate'])
            for step in p['schedules'][str(seed)]:
                guard()
                obs, targets, _ = cache[step['lesson']]
                loss = best_mass_loss(model(obs), targets)
                optimizer.zero_grad()
                loss.backward()
                grad = float(torch.nn.utils.clip_grad_norm_(model.parameters(), config['gradient_clip']))
                if not torch.isfinite(loss) or not np.isfinite(grad):
                    raise ValueError('large-learning.nonfinite-update')
                optimizer.step()
                updates.append(dict(step, seed=seed, loss=float(loss.detach()), gradient_norm=grad))
                if (step['update']+1) % 128 == 0 or step['update']+1 == config['updates']:
                    torch.save(dict(schema=SCHEMA, depth=p['depth'], width=p['width'], policy=model.state_dict(),
                        optimizer=optimizer.state_dict(), rng=torch.get_rng_state(), step=step['update']+1, seed=seed,
                        protocol_sha256=sha(args.screen/'protocol.json')), args.screen/f'learned-{seed}.pt')
                    write(args.screen/'updates.json', updates)
                    print(json.dumps(dict(stage=stage, seed=seed, updates=step['update']+1, seconds=time.perf_counter()-began)), flush=True)
            model.eval()
            label_scores(model, f'learned-{seed}')
            write(args.screen/'label-scores.json', scores)
        cache.clear()
        stage = 'evaluation'
        bounded = argparse.Namespace(episode_seconds=config['episode_seconds'], route_work=config['route_work'])
        for method in p['methods']:
            model = None
            if method != 'geometry':
                checkpoint = parent if method == 'parent' else torch.load(args.screen/f'{method}.pt', map_location='cpu', weights_only=False)
                model = FullFactoryPolicy(p['depth'], p['width'])
                model.load_state_dict(checkpoint['policy'])
                model.eval()
                frozen = {k: v.clone() for k, v in model.state_dict().items()}
            for case in p['cases']:
                for bank in config['banks']:
                    guard()
                    runtime = FactoryRuntime(p['native_library'], p['catalogs'])
                    try:
                        if runtime.description_cache:
                            raise ValueError('large-learning.nonempty-evaluation-cache')
                        arm = dict(horizon=config['horizon'], balanced=False, memory=True, geometry=method == 'geometry')
                        actor = TemperaturePolicy(model, config['temperature']) if model is not None else None
                        summary, best, _, _ = episode(runtime, case, actor, arm, bank, bounded)
                        directory = args.screen/f'{method}--{case["id"]}--{bank}'
                        directory.mkdir()
                        save_gzip(directory/'episode.json.gz', summary)
                        save_gzip(directory/'best.json.gz', best)
                        rows.append(dict(method=method, case=case['id'], family=case['family'], split=case['split'], bank=bank,
                            directory=str(directory.resolve()), initial_metrics=summary['initial_metrics'], best_metrics=summary['best_metrics'],
                            selected=summary['selected'], seconds=summary['total_seconds'], observation_seconds=summary['observation_seconds'],
                            kinds=summary['kinds'], statuses=summary['statuses'], route_expansions=summary['route_expansions'],
                            improvements=dict(Counter(t['action']['kind'] for t in summary['trace'] if t['status'] == 'improved')),
                            budget_complete=summary['selected'] == config['horizon'] and not any(t['status'] == 'late' for t in summary['trace'])))
                        write(args.screen/'progress.json', dict(status='incomplete', rows=rows))
                    finally:
                        runtime.close()
                print(json.dumps(dict(stage=stage, method=method, case=case['id'], episodes=len(rows), seconds=time.perf_counter()-began)), flush=True)
            if model is not None and any(not torch.equal(v, frozen[k]) for k, v in model.state_dict().items()):
                raise ValueError('large-learning.policy-mutated')
        verify_sources(p)
        write(args.screen/'report.json', dict(schema_version=1, status='complete', protocol_sha256=sha(args.screen/'protocol.json'),
            lessons_audit_sha256=sha(args.screen/'lessons-audit.json'), seconds=time.perf_counter()-began,
            admission_seconds=admitted['seconds'], updates=len(updates), cache_array_bytes=cache_bytes, policy_immutable=True, rows=rows))
    except BaseException as error:
        write(args.screen/'partial.json', dict(status='incomplete', code='large-learning.failed', severity='error', stage=stage,
            message=str(error), updates=len(updates), rows=rows))
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    admission = sub.add_parser('prepare')
    for name in ['continuations', 'parent', 'original-inputs', 'native-library', 'config', 'output']:
        admission.add_argument('--'+name, type=Path, required=True)
    training = sub.add_parser('run')
    training.add_argument('--screen', type=Path, required=True)
    args = parser.parse_args()
    (prepare if args.command == 'prepare' else run)(args)
