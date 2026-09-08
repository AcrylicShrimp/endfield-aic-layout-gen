"""Generate charged, validated inverse tasks without expert-only runtime actions."""
import argparse
from collections import Counter
import json
from pathlib import Path
import random
import re
import time

from full_factory_environment import ROOT,FactoryRuntime,FactoryEpisode
from full_factory_policy import SCHEMA
from full_factory_screen import TRAIN_ROOTS,sha,write
from full_factory_selection import physical_key


def source_manifest():
    pending=[Path(__file__).with_name('full_factory_executor.mjs')];sources=set()
    while pending:
        p=pending.pop().resolve()
        if p in sources:continue
        sources.add(p)
        pending.extend((p.parent/name).resolve() for name in re.findall(r"from\s+['\"](\.[^'\"]+)['\"]",p.read_text()))
    sources.update(p.resolve() for p in Path(__file__).parent.glob('full_factory*.py'))
    return {str(p.relative_to(ROOT)):sha(p) for p in sorted(sources)}


def inverse_action(action,before,after,description):
    kind=action['kind']
    if kind=='pose':
        # Native acceptance may canonically translate the whole witness. The actual
        # selected pose minus its requested pose identifies that frame shift even
        # when a small factory has no unselected facility.
        ids={before['placements'][q['member']]['instance'] for q in action['poses']}
        previous={p['instance']:p for p in before['placements']}
        requested=action['poses'][0];id=before['placements'][requested['member']]['instance']
        actual=next(p for p in after['placements'] if p['instance']==id)
        dx,dy=(actual[k]-requested[k] for k in ('x','y'))
        target={id:(previous[id]['x']+dx,previous[id]['y']+dy,previous[id]['rotation']) for id in ids}
        return next((a for a in description['actions'] if a['kind']=='pose' and
                     {after['placements'][q['member']]['instance']:(q['x'],q['y'],q['rotation']) for q in a['poses']}==target),None)
    if kind=='port':
        old=next(t['endpoint']['port'] for n in before['transport_networks'] for t in n['terminals'] if t['id']==action['terminal'])
        return next((a for a in description['actions'] if a['kind']==kind and a['terminal']==action['terminal'] and a['port']==old),None)
    if kind=='component':
        return next((a for a in description['actions'] if a['kind']==kind and a['component']==action['component'] and
                     a['dx']==-action['dx'] and a['dy']==-action['dy'] and a['rotation']==action['rotation']),None)
    if kind=='insert':
        return next((a for a in description['actions'] if a['kind']=='contract' and a['axis']==action['axis'] and a['position']==action['position']),None)
    return None


def action_family(action,witness):
    if action['kind']!='pose':return action['kind']
    rotated=any(q['rotation']!=witness['placements'][q['member']]['rotation'] for q in action['poses'])
    return ('single_rotation' if len(action['poses'])==1 else 'compound_pose') if rotated else ('single_translation' if len(action['poses'])==1 else 'group_translation')


def run(args):
    began=time.perf_counter();inputs=json.loads(args.inputs.read_text())
    for c in inputs['rows']:
        for field,name in [('initial','initial.json'),('request','request.json'),('original','original-request.json')]:
            if sha(c[field])!=c['files_sha256'][name]:raise ValueError(f'factory.input-hash: {c[field]}')
    cases=[dict(c,initial_sha256=sha(c['initial'])) for c in inputs['rows']]
    original=next(c for c in cases if c['id']=='closure-original')
    cases.extend([dict(original,id='closure-strong',initial=str(args.strong.resolve()),initial_sha256=sha(args.strong)),
                  dict(original,id='closure-incumbent',initial=str(args.incumbent.resolve()),initial_sha256=sha(args.incumbent))])
    roots=[next(c for c in cases if c['id']==name) for name in (*TRAIN_ROOTS,'closure-incumbent')]
    args.output.mkdir();scratch=args.output/'candidate.json';pairs=[];traces=[];summaries=[];seen=set()
    protocol=dict(schema_version=1,observation_schema=SCHEMA,executor_revision='compound-facility-poses-v3',
                  inputs_sha256=sha(args.inputs),native_sha256=sha(args.native_library),source_sha256=source_manifest(),
                  cases=cases,training_roots=[c['id'] for c in roots],pairs_per_root=args.pairs,forward_limit=args.forward_limit,
                  validation_pairs=8,compound_pairs_per_root=4,seed=90117,seconds=args.seconds,route_work=5000,catalogs=inputs['catalogs'],
                  catalog_sha256={k:sha(v) for k,v in json.loads(Path(inputs['catalogs']).read_text()).items()})
    write(args.output/'protocol.json',protocol);runtime=FactoryRuntime(args.native_library,inputs['catalogs'])
    def episode(case):
        remaining=args.seconds-(time.perf_counter()-began)
        if remaining<=0:raise TimeoutError('factory.curriculum-generation-budget')
        return FactoryEpisode(runtime,case,horizon=1,elapsed=remaining,work=5000)
    try:
        for ri,case in enumerate(roots):
            root=episode(case);desc=root.describe();rng=random.Random(90117+ri)
            lists=[[a for a in desc['actions'] if action_family(a,root.state['current'])==kind] for kind in
                   ('single_translation','group_translation','single_rotation','compound_pose','port','component','insert')]
            for choices in lists:rng.shuffle(choices)
            actions=[choices[i] for i in range(max(map(len,lists))) for choices in lists if i<len(choices)][:args.forward_limit]
            local=[];counts=Counter();best=root.state['best_metrics']['score'];discovery=None
            for ordinal,action in enumerate(actions):
                env=episode(case);forward=env.step(action);counts['forward']+=1
                record=dict(root=case['id'],ordinal=ordinal,forward=forward);traces.append(record)
                score=env.state['current_metrics']['score']
                if score<best:
                    best=score;discovery=args.output/f'root-{ri}-discovery.json';write(discovery,env.state['current'])
                if forward['unchanged'] or score<=root.state['initial_metrics']['score']:
                    counts['not_worse_changed']+=1;continue
                if len(local)>=args.pairs and action_family(action,env.state['initial'])!='compound_pose':
                    counts['supplement_noncompound']+=1;continue
                state_key=physical_key(env.state['current'])
                if state_key in seen:counts['duplicate']+=1;continue
                write(scratch,env.state['current']);draft_case=dict(case,initial=str(scratch.resolve()))
                reverse=episode(draft_case);description=reverse.describe()
                inverse=inverse_action(action,env.state['initial'],reverse.state['current'],description)
                if inverse is None:counts['inverse_not_offered']+=1;continue
                result=reverse.step(inverse);counts['inverse_selected']+=1;record['inverse']=result
                if result['status']!='improved':counts['inverse_not_improved']+=1;continue
                if reverse.state['current_metrics']['score']<best:
                    best=reverse.state['current_metrics']['score'];discovery=args.output/f'root-{ri}-discovery.json';write(discovery,reverse.state['current'])
                name=f'root-{ri}-pair-{len(local)}';path=args.output/(name+'.json');write(path,env.state['current']);seen.add(state_key)
                pair=dict(id=name,root=case['id'],case=dict(case,id=name,initial=str(path.resolve()),initial_sha256=sha(path)),
                          forward=action,inverse=inverse,family=action_family(inverse,reverse.state['initial']),physical_key=state_key,root_metrics=root.state['initial_metrics'],
                          initial_metrics=reverse.state['initial_metrics'],inverse_metrics=reverse.state['best_metrics'],
                          restores_root=reverse.state['best_metrics']['score']<=root.state['initial_metrics']['score'])
                pairs.append(pair);local.append(pair);counts['admitted']+=1
                if len(local)>=args.pairs and sum(p['family']=='compound_pose' for p in local)>=4:break
            families={name:[p for p in reversed(local) if p['family']==name] for name in sorted({p['family'] for p in local})}
            validation=[pool[i]['id'] for i in range(max((len(pool) for pool in families.values()),default=0)) for pool in families.values() if i<len(pool)][:8]
            for pair in local:pair['split']='validation' if pair['id'] in validation else 'training'
            summary=dict(root=case['id'],counts=dict(counts),initial=root.state['initial_metrics'],best=best,families=dict(Counter(p['family'] for p in local)),
                         discovery=str(discovery.resolve()) if discovery else None)
            summaries.append(summary);print(json.dumps(summary),flush=True)
        report=dict(schema_version=1,status='complete' if all(s['counts'].get('admitted',0)>=16 for s in summaries) else 'admission_shortfall',
                    protocol_sha256=sha(args.output/'protocol.json'),pairs=pairs,generation=summaries,trace=traces,seconds=time.perf_counter()-began)
        write(args.output/'curriculum.json',report);scratch.unlink(missing_ok=True)
    except BaseException as error:
        write(args.output/'partial.json',dict(status='incomplete',error=str(error),pairs=pairs,generation=summaries,trace=traces))
        raise
    finally:runtime.close()
    print(json.dumps(dict(status=report['status'],pairs=len(pairs),seconds=report['seconds'])))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('inputs','strong','incumbent','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--pairs',type=int,default=24);p.add_argument('--forward-limit',type=int,default=384);p.add_argument('--seconds',type=float,default=180)
    args=p.parse_args()
    if args.pairs<16 or args.forward_limit<args.pairs or args.seconds<=0:p.error('At least 16 pairs, enough forward selections, and positive time required')
    run(args)
