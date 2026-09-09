"""Prepare varied complete factories and measure bounded dense-state continuations."""
import argparse
from collections import Counter
from pathlib import Path
import time

from full_factory_curriculum import source_manifest
from full_factory_dense_contrasts import alternatives
from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_large_learning import read, verify_sources
from full_factory_pose_teachers import displacement
from full_factory_screen import sha, write, save_gzip


VALID = {'improved', 'accepted_equal', 'accepted_exploration'}


def config_check(c):
    if c.get('schema_version') != 1:
        raise ValueError('varied-teachers.config-schema')
    bounds = dict(seed=2**31-1, preparation_horizon=256, preparation_seconds=300,
                  preparation_study_seconds=3600, horizon=64, episode_seconds=300,
                  route_work=100000, study_seconds=7200)
    for key, cap in bounds.items():
        if type(c.get(key)) is not int or not 0 < c[key] <= cap:
            raise ValueError(f'varied-teachers.config-bound: {key}')
    expected = dict(pose_quotas={'local_single', 'distant_single', 'group_translation', 'single_rotation', 'compound_rotation'},
                    other_quotas={'reroute', 'port', 'component', 'release_bridge'})
    for key, kinds in expected.items():
        if set(c.get(key, {})) != kinds or any(type(q) is not int or not 0 < q <= 1024 for q in c[key].values()):
            raise ValueError(f'varied-teachers.config-quotas: {key}')


def prepare(args):
    began = time.perf_counter()
    config, admission, inputs = read(args.config), read(args.admission/'report.json'), read(args.inputs)
    config_check(config)
    accepted = [r for r in admission['rows'] if r['status'] == 'accepted' and r['factory_validated']]
    if admission['status'] != 'measured' or len(admission['rows']) != admission['attempted'] or len(accepted) != admission['accepted']:
        raise ValueError('varied-teachers.incomplete-admission')
    if not accepted or [r['id'] for r in accepted] != [c['id'] for c in inputs['rows']] or any(r['split'] != 'train' for r in accepted):
        raise ValueError('varied-teachers.training-membership')
    sources = {str(Path(s['path']).resolve()): s['sha256'] for s in admission['provenance'] + inputs['provenance']}
    cases = []
    for row, case in zip(accepted, inputs['rows']):
        for key, name in [('initial', 'initial.json'), ('request', 'request.json'), ('original', 'original-request.json')]:
            if sha(case[key]) != case['files_sha256'][name]:
                raise ValueError(f'varied-teachers.case-hash: {case[key]}')
            sources[case[key]] = case['files_sha256'][name]
        cases.append(dict(case, family=row['family'], split=row['split'], size_stratum=row['size_stratum']))
    files = [args.config, args.admission/'report.json', args.inputs, args.native_library,
             args.future_evaluation, Path(inputs['catalogs'])]
    sources.update({str(f.resolve()): sha(f) for f in files})
    p = dict(schema_version=1, experiment='varied-original-contraction-preparation', config=config, cases=cases,
        admission=str(args.admission.resolve()), inputs=str(args.inputs.resolve()), future_evaluation=str(args.future_evaluation.resolve()),
        native_library=str(args.native_library.resolve()), catalogs=inputs['catalogs'], sources=sources, source_sha256=source_manifest(), training_performed=False)
    verify_sources(p)
    args.output.mkdir()
    write(args.output/'protocol.json', p)
    rows = []
    try:
        for case in cases:
            remaining = config['preparation_study_seconds']-(time.perf_counter()-began)
            if remaining <= 0:
                break
            root_started = time.perf_counter()
            deadline = min(config['preparation_seconds'], remaining)
            runtime = FactoryRuntime(args.native_library, p['catalogs'])
            try:
                env = FactoryEpisode(runtime, case, config['preparation_horizon'], max(.001, deadline-(time.perf_counter()-root_started)), config['route_work'])
                termination, count = 'budget_exhausted', None
                while not env.done:
                    description = env.describe()
                    if env.done:
                        break
                    choices = [(i,a) for i,a in enumerate(description['actions']) if a['kind'] == 'contract']
                    count = len(choices)
                    if not choices:
                        termination = 'contraction_exhausted'
                        break
                    index, action = choices[0]
                    step = env.step(action)
                    step.update(vocabulary_index=index, available_contractions=count, current_sha256=digest(env.state['current']))
                    if step['status'] != 'improved':
                        termination = 'candidate_failed' if step['status'] != 'late' else 'budget_exhausted'
                        break
                directory = args.output/case['id']
                directory.mkdir()
                summary = env.summary()
                save_gzip(directory/'preparation.json.gz', summary)
                write(directory/'best.json', env.state['best'])
                prepared = dict(case, id=case['id']+'--contraction-exhausted', root=case['id'],
                    initial=str((directory/'best.json').resolve()), files_sha256=dict(case['files_sha256'], **{'initial.json': sha(directory/'best.json')}))
                rows.append(dict(root=case['id'], status=termination, selected=env.selected, remaining_contractions=count,
                    initial_metrics=summary['initial_metrics'], best_metrics=summary['best_metrics'],
                    seconds=time.perf_counter()-root_started, episode_seconds=env.elapsed,
                    directory=str(directory.resolve()), prepared=prepared if termination == 'contraction_exhausted' else None,
                    trace_sha256=sha(directory/'preparation.json.gz'), best_sha256=sha(directory/'best.json'),
                    diagnostics=[] if termination == 'contraction_exhausted' else [dict(code='varied-teachers.preparation-stopped', severity='warning',
                        stage='contraction-preparation', entity=case['id'], message='Preparation did not establish ordinary contraction exhaustion within its declared bounds')]))
                write(args.output/'progress.json', dict(status='incomplete', rows=rows))
                print(dict(root=case['id'], status=termination, selected=env.selected, score=summary['best_metrics']['score'], seconds=time.perf_counter()-began), flush=True)
            finally:
                runtime.close()
        verify_sources(p)
        write(args.output/'report.json', dict(schema_version=1, status='complete' if len(rows)==len(cases) else 'study_budget',
            protocol_sha256=sha(args.output/'protocol.json'), seconds=time.perf_counter()-began, rows=rows,
            prepared=sum(r['status']=='contraction_exhausted' for r in rows), selected=sum(r['selected'] for r in rows), training_performed=False))
    except BaseException as error:
        write(args.output/'partial.json', dict(status='incomplete', code='varied-teachers.preparation-failed', severity='error', stage='preparation', message=str(error), rows=rows))
        raise


def collect(args):
    began = time.perf_counter()
    parent, report, audit = (read(args.preparation/n) for n in ['protocol.json','report.json','audit.json'])
    if report['status'] != 'complete' or audit['status'] != 'passed' or audit['report_sha256'] != sha(args.preparation/'report.json') or report['protocol_sha256'] != sha(args.preparation/'protocol.json'):
        raise ValueError('varied-teachers.preparation-audit')
    verify_sources(parent)
    config = parent['config']
    cases = [row['prepared'] for row in report['rows'] if row['status'] == 'contraction_exhausted']
    if not cases:
        raise ValueError('varied-teachers.no-exhausted-original')
    sources = dict(parent['sources'])
    for f in [args.preparation/n for n in ['protocol.json','report.json','audit.json']] + [Path(c['initial']) for c in cases]:
        sources[str(f.resolve())] = sha(f)
    p = dict(schema_version=1, experiment='varied-full-score-continuation-teachers', preparation=str(args.preparation.resolve()), config=config,
        cases=cases, catalogs=parent['catalogs'], native_library=parent['native_library'], sources=sources, source_sha256=source_manifest(), training_performed=False)
    args.output.mkdir()
    write(args.output/'protocol.json', p)
    rows = []
    try:
        for ordinal, case in enumerate(cases):
            if time.perf_counter()-began >= config['study_seconds']:
                break
            directory = args.output/case['root']
            directory.mkdir()
            runtime = FactoryRuntime(p['native_library'], p['catalogs'])
            trajectories = []
            try:
                initial_env = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
                initial, initial_metrics = initial_env.state['current'], initial_env.state['initial_metrics']
                description = initial_env.describe()
                if any(a['kind'] == 'contract' for a in description['actions']):
                    raise ValueError('varied-teachers.prepared-contraction-drift')
                selections, populations = alternatives(description, initial, dict(config, seed=config['seed']+ordinal))
                write(directory/'selection.json', dict(case=case, seed=config['seed']+ordinal, selections=selections, populations=populations,
                    initial_metrics=initial_metrics, description_seconds=description['seconds']))
                best, best_metrics = initial, initial_metrics
                teacher_best, teacher_metrics = initial, initial_metrics
                for selection in selections:
                    remaining = config['study_seconds']-(time.perf_counter()-began)
                    if remaining <= 0:
                        break
                    env = FactoryEpisode(runtime, case, config['horizon'], min(config['episode_seconds'], remaining), config['route_work'])
                    first = env.step(selection['action'])
                    first['current_sha256'] = digest(env.state['current'])
                    valid = first['status'] in VALID
                    moved = displacement(initial, env.state['current'], first['action']) if valid and first['action']['kind']=='pose' else None
                    termination = 'first_rejected' if not valid else 'time_budget'
                    if first['status'] == 'late':
                        termination = 'time_budget'
                    while valid and not env.done:
                        d = env.describe()
                        if env.done:
                            break
                        choices = [(i,a) for i,a in enumerate(d['actions']) if a['kind']=='contract']
                        if not choices:
                            termination = 'contraction_exhausted'
                            break
                        index, action = choices[0]
                        before = env.state['current_metrics']['score']
                        step = env.step(action)
                        step.update(vocabulary_index=index, available_contractions=len(choices), current_sha256=digest(env.state['current']))
                        # A contraction can improve the current exploratory layout before it beats the incumbent.
                        if step['status'] not in VALID or not step['current_score'] < before:
                            termination = 'time_budget' if step['status']=='late' else 'continuation_failed'
                            break
                    else:
                        if valid and env.selected == config['horizon'] and env.trace[-1]['status'] != 'late':
                            termination = 'horizon'
                    summary = env.summary()
                    trajectories.append(dict(id=len(trajectories), **selection, termination=termination,
                        completed=termination in ['first_rejected','contraction_exhausted','horizon','continuation_failed'],
                        first_valid=valid, displacement=moved, episode_seconds=env.elapsed, summary=summary))
                    if summary['best_metrics']['score'] < best_metrics['score']:
                        best, best_metrics = env.state['best'], summary['best_metrics']
                        save_gzip(directory/'best.json.gz', best)
                    if trajectories[-1]['completed'] and summary['best_metrics']['score'] < teacher_metrics['score']:
                        teacher_best, teacher_metrics = env.state['best'], summary['best_metrics']
                    if len(trajectories)%32 == 0:
                        save_gzip(directory/'trajectories.json.gz', trajectories)
                        print(dict(root=case['root'], trials=len(trajectories), best=best_metrics['score'], seconds=time.perf_counter()-began), flush=True)
                save_gzip(directory/'trajectories.json.gz', trajectories)
                save_gzip(directory/'best.json.gz', best)
                teacher_path = directory/'best.json.gz'
                if digest(teacher_best) != digest(best):
                    teacher_path = directory/'teacher-best.json.gz'
                    save_gzip(teacher_path, teacher_best)
                targets = [t['id'] for t in trajectories if t['completed'] and t['summary']['best_metrics']['score']==teacher_metrics['score'] and teacher_metrics['score']<initial_metrics['score']]
                rows.append(dict(root=case['root'], case=case['id'], directory=str(directory.resolve()), planned=len(selections), trials=len(trajectories),
                    selected=sum(t['summary']['selected'] for t in trajectories), initial_metrics=initial_metrics, best_metrics=best_metrics,
                    teacher_metrics=teacher_metrics, teacher_path=str(teacher_path.resolve()), teacher_sha256=sha(teacher_path),
                    targets=targets, first_valid=sum(t['first_valid'] for t in trajectories),
                    area_improved=sum(t['summary']['best_metrics']['score'][0]<initial_metrics['score'][0] for t in trajectories),
                    terminations=dict(Counter(t['termination'] for t in trajectories)),
                    selection_sha256=sha(directory/'selection.json'), trajectories_sha256=sha(directory/'trajectories.json.gz'), best_sha256=sha(directory/'best.json.gz')))
                write(args.output/'progress.json', dict(status='incomplete', rows=rows))
                print(dict(root=case['root'], targets=targets, best=best_metrics['score'], area_improved=rows[-1]['area_improved']), flush=True)
            finally:
                runtime.close()
        verify_sources(p)
        complete = len(rows)==len(cases) and all(r['trials']==r['planned'] and not r['terminations'].get('time_budget') for r in rows)
        write(args.output/'report.json', dict(schema_version=1, status='complete' if complete else 'budget_exhausted', protocol_sha256=sha(args.output/'protocol.json'),
            seconds=time.perf_counter()-began, rows=rows, trials=sum(r['trials'] for r in rows), selected=sum(r['selected'] for r in rows), training_performed=False))
    except BaseException as error:
        write(args.output/'partial.json', dict(status='incomplete', code='varied-teachers.collection-failed', severity='error', stage='collection', message=str(error), rows=rows))
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    preparation = sub.add_parser('prepare')
    for name in ['admission','inputs','native-library','config','future-evaluation','output']:
        preparation.add_argument('--'+name, required=True, type=Path)
    collection = sub.add_parser('collect')
    for name in ['preparation','output']:
        collection.add_argument('--'+name, required=True, type=Path)
    args = parser.parse_args()
    (prepare if args.command=='prepare' else collect)(args)
