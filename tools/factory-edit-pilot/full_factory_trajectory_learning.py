"""Replay-admitted original compression lessons and opened-family policy transfer."""
import argparse
from collections import OrderedDict
import gzip
import json
from pathlib import Path
import random
import time

import numpy as np
import torch

from full_factory_curriculum import source_manifest
from full_factory_environment import ROOT, FactoryRuntime, FactoryEpisode, digest
from full_factory_landscape import episode
from full_factory_policy import SCHEMA, FullFactoryPolicy, observe
from full_factory_screen import sha, write, save_gzip


def unpack(path):
    with gzip.open(path, 'rt') as stream:
        return json.load(stream)


def family_cases(admission, inputs):
    """Join predeclared family assignment; accepted denominators must match exactly."""
    accepted = {r['id']: r for r in admission['rows'] if r['factory_validated']}
    if len(accepted) != admission['accepted'] or len(inputs['rows']) != len(accepted):
        raise ValueError('trajectory.admission-count')
    cases = []
    for row in inputs['rows']:
        a = accepted.pop(row['id'])
        if a['split'] not in ('train', 'development') or not a['family']:
            raise ValueError('trajectory.family-split')
        cases.append(dict(row, family=a['family'], split=a['split']))
    if accepted or not any(c['split']=='train' for c in cases):
        raise ValueError('trajectory.admission-membership')
    family_splits = {}
    for c in cases:
        if c['family'] in family_splits and family_splits[c['family']] != c['split']:
            raise ValueError('trajectory.family-leakage')
        family_splits[c['family']] = c['split']
    return cases


def trajectory_state(admitted_state, lesson):
    """Admission of a stored witness must not reset its original trajectory context."""
    if admitted_state['current_metrics'] != lesson['initial_metrics']:
        raise ValueError('trajectory.lesson-current-metrics')
    return dict(admitted_state, initial_metrics=lesson['episode_initial_metrics'],
                best_metrics=lesson['episode_best_metrics'])


def run(args):
    began = time.perf_counter()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    plan = json.loads(args.study.read_text())
    resolve = lambda p: (args.study.resolve().parent/p).resolve()
    if plan.get('schema_version') != 1:
        raise ValueError('trajectory.study-schema')
    for key, maximum in [('updates',4096),('horizon',64),('episode_seconds',300),('route_work',100000),
                         ('screen_seconds',7200),('observation_cache_bytes',512*1024*1024)]:
        if type(plan.get(key)) is not int or not 1 <= plan[key] <= maximum:
            raise ValueError(f'trajectory.study-bound: {key}')
    if not isinstance(plan.get('banks'),list) or not plan['banks'] or len(set(plan['banks']))!=len(plan['banks']) or any(type(s) is not int or s<0 for s in plan['banks']):
        raise ValueError('trajectory.sampling-banks')
    input_path, admission_path, teacher = (resolve(plan[k]) for k in ('inputs','admission','teacher'))
    inputs, admission = (json.loads(p.read_text()) for p in (input_path,admission_path))
    cases = family_cases(admission,inputs)
    paths = json.loads(Path(inputs['catalogs']).read_text())
    sources = {str(args.study.resolve()):sha(args.study),str(input_path):sha(input_path),str(admission_path):sha(admission_path),
               str(args.native_library.resolve()):sha(args.native_library),str(Path(inputs['catalogs']).resolve()):sha(inputs['catalogs'])}
    sources.update({str(Path(p).resolve()):sha(p) for p in paths.values()})
    for extra in plan['additional_evaluation']:
        other_path = resolve(extra['inputs']); sources[str(other_path)] = sha(other_path)
        other = json.loads(other_path.read_text())
        if {k:sha(p) for k,p in json.loads(Path(other['catalogs']).read_text()).items()} != {k:sha(p) for k,p in paths.items()}:
            raise ValueError('trajectory.additional-catalogs')
        row = next(c for c in other['rows'] if c['id']==extra['case'])
        original = dict(row, id=extra['id'], family=extra['family'], split='development')
        if 'initial' in extra:
            original['initial'] = str(resolve(extra['initial']))
            original['files_sha256'] = dict(row['files_sha256'], **{'initial.json':sha(original['initial'])})
        cases.append(original)
    if len({c['id'] for c in cases})!=len(cases):
        raise ValueError('trajectory.duplicate-case')
    trained_families = {c['family'] for c in cases if c['split']=='train'}
    if trained_families & {c['family'] for c in cases if c['split']!='train'}:
        raise ValueError('trajectory.additional-family-leakage')
    for c in cases:
        for key,name in [('initial','initial.json'),('request','request.json'),('original','original-request.json')]:
            if sha(c[key])!=c['files_sha256'][name]:raise ValueError(f'trajectory.input-hash: {c[key]}')
            sources[str(Path(c[key]).resolve())] = sha(c[key])
    teacher_report = json.loads((teacher/'report.json').read_text())
    teacher_protocol = json.loads((teacher/'protocol.json').read_text())
    if teacher_report['status']!='complete' or sha(teacher/'protocol.json')!=teacher_report['protocol_sha256']:
        raise ValueError('trajectory.teacher-incomplete')
    if sha(teacher_protocol['inputs']) != sha(input_path):raise ValueError('trajectory.teacher-inputs')
    for file,expected in teacher_protocol['provenance'].items():
        if sha(file)!=expected:raise ValueError(f'trajectory.teacher-source: {file}')
    for name in ('report.json','protocol.json'):
        sources[str(teacher/name)] = sha(teacher/name)
    teacher_rows = [r for r in teacher_report['rows'] if r['arm']=='geometry']
    if sorted(r['case'] for r in teacher_rows)!=sorted(c['id'] for c in inputs['rows']):
        raise ValueError('trajectory.teacher-membership')
    for row in teacher_rows:
        for name in ('episode.json.gz','best.json.gz'):
            p=Path(row['directory'])/name;sources[str(p)]=sha(p)
    protocol = dict(schema_version=1,experiment='original-compression-family-transfer',observation_schema=SCHEMA,
                    plan=plan,cases=cases,catalogs=inputs['catalogs'],sources=sources,source_sha256=source_manifest(),
                    model_seed=17,depth=2,width=32,learning_rate=.0003,gradient_clip=.5,teacher=str(teacher))
    args.output.mkdir(); write(args.output/'protocol.json',protocol)
    runtime = FactoryRuntime(args.native_library,inputs['catalogs'])
    lessons, updates, rows, label_scores = [], [], [], []
    stage='admission'; entity=None
    def guard():
        if time.perf_counter()-began >= plan['screen_seconds']:
            raise TimeoutError('trajectory.screen-budget')
    cache=OrderedDict(); cache_bytes=0; peak_cache_bytes=0
    def observation(lesson):
        nonlocal cache_bytes,peak_cache_bytes
        if lesson['id'] in cache:
            cache.move_to_end(lesson['id']);return cache[lesson['id']][0]
        c=dict(next(c for c in cases if c['id']==lesson['root']),initial=lesson['initial'])
        env=FactoryEpisode(runtime,c,1,plan['episode_seconds'],plan['route_work']);d=env.describe()
        obs=observe(env.state['current'],d,trajectory_state(env.state,lesson),1-lesson['ordinal']/plan['horizon'])
        if obs['context'].tolist()!=lesson['observation_context']:
            raise ValueError('trajectory.lesson-context-replay')
        value=(obs,d['actions'].index(lesson['action']))
        size=sum(v.nbytes for v in obs.values() if isinstance(v,np.ndarray))
        if size<=plan['observation_cache_bytes']:
            while cache and cache_bytes+size>plan['observation_cache_bytes']:
                _,(_,used)=cache.popitem(last=False);cache_bytes-=used
            cache[lesson['id']]=(value,size);cache_bytes+=size;peak_cache_bytes=max(peak_cache_bytes,cache_bytes)
        return value
    def checkpoint(model,optimizer,step,label):
        torch.save(dict(schema=SCHEMA,depth=2,policy=model.state_dict(),optimizer=optimizer.state_dict() if optimizer else None,
                        rng=torch.get_rng_state(),step=step,protocol_sha256=sha(args.output/'protocol.json')),args.output/f'{label}.pt')
    try:
        for row in teacher_rows:
            guard(); entity=row['case']; c=next(c for c in cases if c['id']==entity)
            recorded=unpack(Path(row['directory'])/'episode.json.gz')
            env=FactoryEpisode(runtime,c,plan['horizon'],plan['episode_seconds'],plan['route_work'])
            for ordinal,t in enumerate(recorded['trace']):
                d=env.describe(); action=t['action']
                if action not in d['actions']:raise ValueError('trajectory.teacher-action-not-offered')
                before=env.state['current']; initial_metrics=env.state['current_metrics']
                episode_initial_metrics=env.state['initial_metrics'];episode_best_metrics=env.state['best_metrics']
                original_context=observe(before,d,env.state,1-ordinal/plan['horizon'])['context'].tolist()
                actual=env.step(action)
                for key in ('status','current_score','best_score','route_expansions','unchanged'):
                    if actual[key]!=t[key]:raise ValueError(f'trajectory.teacher-replay: {entity}:{ordinal}:{key}')
                if actual['status']!='improved':raise ValueError('trajectory.teacher-not-improving')
                name=f'{entity}-{ordinal}';p=args.output/(name+'.json');write(p,before)
                lessons.append(dict(id=name,root=entity,family=c['family'],split=c['split'],ordinal=ordinal,initial=str(p.resolve()),
                                    initial_sha256=sha(p),action=action,initial_metrics=initial_metrics,result_metrics=env.state['current_metrics'],
                                    episode_initial_metrics=episode_initial_metrics,episode_best_metrics=episode_best_metrics,
                                    observation_context=original_context))
            if env.state['best']!=unpack(Path(row['directory'])/'best.json.gz'):raise ValueError('trajectory.teacher-final-replay')
        write(args.output/'lessons.json',dict(schema_version=1,status='admitted',lessons=lessons,selected=len(lessons)))
        print(json.dumps(dict(stage='admission',lessons=len(lessons),training=sum(l['split']=='train' for l in lessons))),flush=True)
        torch.manual_seed(17);model=FullFactoryPolicy(2);checkpoint(model,None,0,'untrained')
        def labels(model,phase):
            for lesson in lessons:
                guard(); obs,target=observation(lesson)
                with torch.no_grad():logits=model(obs);probability=float(torch.softmax(logits,0)[target])
                if not torch.isfinite(logits).all():raise ValueError('trajectory.nonfinite-logits')
                label_scores.append(dict(phase=phase,lesson=lesson['id'],family=lesson['family'],split=lesson['split'],
                                         top1=int(logits.argmax())==target,probability=probability))
        labels(model,'untrained')
        stage='training';optimizer=torch.optim.Adam(model.parameters(),lr=.0003)
        order=[l for l in lessons if l['split']=='train'];random.Random(17).shuffle(order)
        if not order:raise ValueError('trajectory.no-training-lessons')
        for update in range(plan['updates']):
            guard();lesson=order[update%len(order)];entity=lesson['id'];obs,target=observation(lesson)
            logits=model(obs);loss=-torch.log_softmax(logits,0)[target];optimizer.zero_grad();loss.backward()
            grad=float(torch.nn.utils.clip_grad_norm_(model.parameters(),.5))
            if not torch.isfinite(loss) or not np.isfinite(grad):raise ValueError('trajectory.nonfinite-training')
            optimizer.step();updates.append(dict(update=update,lesson=entity,loss=float(loss.detach()),gradient_norm=grad))
            if (update+1)%64==0:
                checkpoint(model,optimizer,update+1,'progress');write(args.output/'updates.json',updates)
                print(json.dumps(dict(stage=stage,updates=update+1,seconds=time.perf_counter()-began)),flush=True)
        checkpoint(model,optimizer,plan['updates'],'learned');labels(model,'learned')
        write(args.output/'label-scores.json',label_scores);write(args.output/'updates.json',updates)
        stage='evaluation'
        bounded=argparse.Namespace(episode_seconds=plan['episode_seconds'],route_work=plan['route_work'])
        for method in ('untrained','learned','uniform','geometry'):
            policy=None
            if method in ('untrained','learned'):
                policy=FullFactoryPolicy(2);policy.load_state_dict(torch.load(args.output/f'{method}.pt',weights_only=False)['policy']);policy.eval()
            config=dict(horizon=plan['horizon'],balanced=False,memory=True,geometry=method=='geometry')
            for c in cases:
                entity=c['id']
                for bank in plan['banks']:
                    guard();summary,best,_,_=episode(runtime,c,policy,config,bank,bounded)
                    directory=args.output/f'{method}--{c["id"]}--{bank}';directory.mkdir()
                    save_gzip(directory/'episode.json.gz',summary);save_gzip(directory/'best.json.gz',best)
                    rows.append(dict(method=method,case=c['id'],family=c['family'],split=c['split'],bank=bank,
                        directory=str(directory.resolve()),initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],
                        selected=summary['selected'],seconds=summary['total_seconds'],statuses=summary['statuses'],kinds=summary['kinds'],
                        budget_complete=summary['selected']==plan['horizon'] and not any(t['status']=='late' for t in summary['trace'])))
            print(json.dumps(dict(stage=stage,method=method,episodes=len(rows),seconds=time.perf_counter()-began)),flush=True)
        for p,expected in sources.items():
            if sha(p)!=expected:raise ValueError(f'trajectory.changed-input: {p}')
        for p,expected in protocol['source_sha256'].items():
            if sha(ROOT/p)!=expected:raise ValueError(f'trajectory.changed-source: {p}')
        write(args.output/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
            seconds=time.perf_counter()-began,cache_array_bytes_peak=peak_cache_bytes,lessons=len(lessons),updates=len(updates),rows=rows))
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',code='trajectory.failed',severity='error',stage=stage,entity=entity,
            message=str(error),lessons=lessons,updates=updates,rows=rows))
        raise
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('study','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
