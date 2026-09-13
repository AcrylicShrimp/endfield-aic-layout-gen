"""Durable 80/10/10 starts from the preceding wave's strictly improved bests."""
from collections import Counter
from copy import deepcopy
from pathlib import Path
import random

from blueprint_ppo_store import checked, read, reference, write
from blueprint_ppo_update import digest, require
from ppo_start_curriculum import quotas

SCHEMA = 'declarative-best-starts-v1'


def validate(config):
    require(config == dict(schema=SCHEMA,percentages=[80,10,10],selection_seed=config.get('selection_seed'),
        donors='strict-area-improvement') and type(config['selection_seed']) is int and config['selection_seed']>=0,
        'declarative.curriculum-config: explicit 80/10/10 and strict-area-improvement required')


def choose(config, seed, wave, count, parents):
    """Fixed quotas and score-independent sampling among improving parents."""
    validate(config)
    requested=quotas(count,wave); pools={0:[],1:[]}; complete=Counter();improved=Counter()
    for parent in sorted(parents,key=lambda p:p['stream']):
        require(parent['wave']==wave-1 and parent['seed']==seed and parent['depth'] in (0,1,2),
            'declarative.curriculum-parent: wrong seed, wave or depth')
        require(parent['complete'] and parent['validated'], 'declarative.curriculum-parent: incomplete or invalid donor')
        complete[parent['depth']]+=1
        if parent['best_score'][0]<parent['start_score'][0]:
            improved[parent['depth']]+=1
            if parent['depth']<2:pools[parent['depth']].append(parent)
    slots=[dict(depth=0,requested_depth=0) for _ in range(requested[0])]
    diagnostics=[]
    for depth in (1,2):
        pool=pools[depth-1]
        selected=random.Random(f'{config["selection_seed"]}:{seed}:{wave}:donors:{depth}').sample(pool,min(requested[depth],len(pool)))
        slots.extend(dict(depth=depth,requested_depth=depth,parent=p) for p in selected)
        missing=requested[depth]-len(selected)
        if missing:
            code='declarative.curriculum-bootstrap' if wave<=depth else 'declarative.curriculum-success-shortage'
            diagnostics.append(dict(code=code,severity='info',stage='curriculum',entity=f'{seed}/{wave}/{depth}',
                message=f'{missing} slots use fresh starts; {len(pool)} strictly improving eligible parents available'))
            slots.extend(dict(depth=0,requested_depth=depth,fallback=code) for _ in range(missing))
    random.Random(f'{config["selection_seed"]}:{seed}:{wave}:slots').shuffle(slots)
    return dict(schema=SCHEMA,seed=seed,wave=wave,requested={str(k):v for k,v in requested.items()},
        complete_parents={str(k):v for k,v in complete.items()},improving_parents={str(k):v for k,v in improved.items()},
        actual=dict(Counter(str(s['depth']) for s in slots)),diagnostics=diagnostics,slots=slots)


def first_start(index):
    return dict(schema=SCHEMA,wave=1,stream=index,depth=0,requested_depth=0)


def annotate(metadata, start, initial_score):
    result=deepcopy(metadata);start=deepcopy(start)
    if start['depth']==0:
        root=dict(wave=start['wave'],generation_seed=metadata['generation_seed'],
            request_sha256=metadata['request_sha256'],geometry_sha256=metadata['geometry_sha256'],area=initial_score[0])
        require('root' not in start or start['root']==root, 'declarative.curriculum-root: fresh root identity changed')
        start['root']=root
    result['start_curriculum']=start
    return result


def next_plan(config, seed, directory, payload, chunk_ref, *, legacy=False):
    finished=payload['completed'];count=config['training']['streams']
    require(len(finished)==count and len({r['episode'] for r in finished})==1,
        'declarative.curriculum-wave: all streams must finish the same wave')
    wave=finished[0]['episode']+2;before={r['index']:r for r in payload['native_before']}
    parents=[]
    for row in finished:
        metadata=row['training_input'];start=metadata.get('start_curriculum')
        if start is None:
            require(legacy, 'declarative.curriculum-missing: completed input has no depth identity')
            start=annotate(metadata,dict(schema=SCHEMA,wave=wave-1,stream=row['stream'],depth=0,requested_depth=0),row['initial_score'])['start_curriculum']
        require(start['wave']==wave-1, 'declarative.curriculum-wave: input wave differs from completed episode')
        parents.append(dict(seed=seed,wave=wave-1,stream=row['stream'],depth=start['depth'],root=start['root'],
            complete=row['policy_actions']==config['training']['episode_actions'],validated=row['result']['factory_validated'],
            start_score=row['initial_score'],best_score=row['result']['best_metrics']['score'],
            metadata=metadata,best=row['result']['best'],request=before[row['stream']]['native']['request'],
            metrics=row['result']['best_metrics']))
    plan=choose(config['start_curriculum'],seed,wave,count,parents)
    folder=directory/'curriculum'/f'wave-{wave:06d}-{chunk_ref["sha256"][:16]}'
    folder.mkdir(parents=True,exist_ok=True)
    for index,slot in enumerate(plan['slots']):
        slot.update(schema=SCHEMA,wave=wave,stream=index)
        parent=slot.pop('parent',None)
        if parent is None:continue
        value=dict(schema='declarative-inherited-input-v1',request=parent['request'],original=parent['best'],
            draw=parent['metadata']['draw'],generation_seed=parent['metadata']['generation_seed'],
            expected_metrics=parent['metrics'],best_sha256=digest(parent['best']))
        path=folder/f'slot-{index}.json'
        if path.exists():require(read(path)==value, f'declarative.curriculum-collision: {path}')
        else:write(path,value)
        slot.update(root=parent['root'],expected_metrics=parent['metrics'],best_sha256=digest(parent['best']),
            input=dict(directory=str(directory.resolve()),**reference(path,directory)),
            parent=dict(seed=seed,wave=parent['wave'],stream=parent['stream'],depth=parent['depth'],
                chunk=chunk_ref,model_before=payload['model_before']))
    path=folder/'plan.json'
    if path.exists():require(read(path)==plan, f'declarative.curriculum-collision: {path}')
    else:write(path,plan)
    return plan,reference(path,directory)


def inherited(start, index, episode):
    require(start['schema']==SCHEMA and start['stream']==index and start['wave']==episode+1 and start['depth'] in (1,2),
        'declarative.curriculum-identity: inherited slot differs from its episode')
    parent=start['parent']
    require(parent['wave']==episode and parent['depth']==start['depth']-1 and start['root']['wave']==episode+1-start['depth'],
        'declarative.curriculum-lineage: parents must be consecutive waves')
    value=read(checked(start['input'],Path(start['input']['directory'])))
    require(value['schema']=='declarative-inherited-input-v1' and value['expected_metrics']==start['expected_metrics'] and
        digest(value['original'])==value['best_sha256']==start['best_sha256'] and
        digest(value['request'])==start['root']['request_sha256'],
        'declarative.curriculum-input: inherited obligations or expected state changed')
    payload={k:value[k] for k in ('request','original')}
    from random_factory_generation import geometry_identity
    metadata=dict(reference={k:start['input'][k] for k in ('path','sha256')},
        input_directory=start['input']['directory'],identity=dict(stream=index,episode=episode,slot=episode*8+index),
        generation_seed=value['generation_seed'],draw=value['draw'],input_sha256=digest(payload),
        request_sha256=digest(value['request']),geometry_sha256=geometry_identity(value['original']),
        blueprint_sha256=None,setup_seconds=0.,attempts=[],start_curriculum=deepcopy(start))
    return payload,metadata
