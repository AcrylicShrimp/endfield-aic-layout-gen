"""Explicit preparation and fresh held-out panel, without learner execution."""
from collections import Counter
from copy import deepcopy
from pathlib import Path
import os
import time

from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_ppo_store import ROOT, read, reference, sha, write
from blueprint_ppo_update import digest, require
from blueprint_training_input import source_paths, validate_input
import declarative_learning_policy as policy
from native import load_native


def validate(config):
    require(config['schema']=='declarative-learning-v2' and config['chunk_schema']=='declarative-learning-chunk-v2',
        'declarative training schema')
    validate_input(config)
    from declarative_learning_curriculum import validate as validate_curriculum
    validate_curriculum(config.get('start_curriculum',{}))
    require(config['training_input']['kind']=='fresh-random' and 'fixture_diagnostic' not in config['success'],
        'declarative main training must use fresh inputs only')
    training=config['training']
    require(training['streams']==8 and 0<training['chunk_actions']<training['episode_actions'] and
        training['episode_actions']%training['chunk_actions']==0 and
        training['updates']%(training['episode_actions']//training['chunk_actions'])==0, 'declarative complete persistent horizon')
    require(config['native']['schema']=='declarative-edit-v1' and
        config['native']['max_total_route_expansions']>=training['episode_actions']*config['native']['route_expansions'],
        'declarative equal per-action allowance across full episode')
    require(config['device']=='cuda:0' and config['success']['sampler'] in ('uniform','balanced','balanced_available'),
        'declarative explicit CUDA/replay configuration')
    require(config['ppo']['advantage_normalization']=='center' and config['ppo']['gamma']==1. and
        config['ppo']['width']==40 and config['ppo']['message_rounds']==2, 'declarative retained baseline learning settings')
    require(config['evaluation']['at'][0]==training.get('first_update',0) and config['evaluation']['at'][-1]==training['updates'],
        'declarative initial/final evaluation')
    require(config['seeds'] and len(config['seeds'])==len(set(config['seeds'])), 'declarative unique learner seeds')


def sources(config, library):
    module=load_native(str(library))
    import json
    build=json.loads(module.DeclarativeEditSession.build_info_json())
    require(build['profile']=='release', 'declarative training requires a release native library')
    for path,expected in build['files_sha256'].items():
        require(sha(ROOT/path)==expected, f'declarative stale native source: {path}')
    paths=list(Path(__file__).parent.glob('*.py'))+source_paths(config)
    catalogs=read(ROOT/config['probe_spec'])['catalogs']
    paths += [ROOT/config['probe_spec'],*(ROOT/p for p in catalogs.values())]
    bindings=dict(build['files_sha256'])
    bindings.update({str(p.resolve().relative_to(ROOT)):sha(p) for p in paths})
    return dict(native_library=str(Path(library).resolve()),native_sha256=sha(library),native_build=build,
        files_sha256=bindings,code_sha256=digest(bindings))


def pool(config, library, allocation, output, guard, saved=None):
    return BlueprintPool(config,library,allocation['workers'],output,guard,saved=saved,
        worker_entry=Path(__file__).with_name('declarative_learning_worker.py'),policy=policy)


def panel(config, library, output):
    output=Path(output).resolve()
    require(not output.exists(), f'declarative panel requires a fresh directory: {output}')
    output.mkdir(parents=True)
    selected=deepcopy(config)
    selected['training_input'].update(directory=str(output/'inputs'),seed_base=config['panel']['seed_base'])
    receipt=sources(selected,library)
    allocation=cpu_allocation(8)
    guard=ResourceGuard(output,config['readiness']['seconds'],config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    cases,attempts,identities=[],[],set()
    counts=Counter()
    with pool(selected,library,allocation,output/'workers',guard) as workers:
        for episode in range((config['panel']['max_generated']+7)//8):
            rows=workers.exchange([dict(operation='panel',episode=episode) for _ in range(8)])
            for row in rows:
                case=row['case']
                matches=[name for name,products in config['strata'].items() if case['product'] in products]
                require(len(matches)==1, 'declarative panel product stratum')
                stratum=matches[0]
                status='accepted'
                if case['geometry_identity'] in identities:
                    status='duplicate-geometry'
                elif counts[stratum]>=config['panel']['strata'][stratum]:
                    status='surplus-admitted'
                else:
                    case.update(index=len(cases),stratum=stratum,action_seed=config['panel']['action_seed_base']+len(cases)*100)
                    cases.append(case);identities.add(case['geometry_identity']);counts[stratum]+=1
                attempts.append(dict(status=status,generation_seed=case['generation_seed'],input=case['input']))
            write(output/'progress.json',dict(cases=cases,attempts=attempts))
            print(dict(stage='panel',selected=len(cases),generated=len(attempts),strata=dict(counts)),flush=True)
            if dict(counts)==config['panel']['strata']:
                break
    require(dict(counts)==config['panel']['strata'], 'declarative panel exhausted registered generation allowance')
    result=dict(schema='declarative-learning-panel-v1',status='complete',cases=cases,attempts=attempts,
        selection='first distinct admitted geometries per registered product stratum; no editing outcome selection',
        source=receipt,allocation=allocation,seconds=time.monotonic()-guard.began,resource=guard.last,
        coverage=dict(products=dict(Counter(c['product'] for c in cases)),facilities=[c['facilities'] for c in cases],
            statuses=dict(Counter(r['status'] for r in attempts)),generation_attempts=sum(len(r['input']['attempts']) for r in attempts)))
    write(output/'panel.json',result)
    return result


def evaluation_cases(panel, config):
    return [dict(case,sample=sample,action_seed=case['action_seed']+sample)
            for case in panel['cases'] for sample in range(config['evaluation']['replicates'])]


def prepare(config, library, output, panel_path, verification, parent_path=None):
    output=Path(output).resolve()
    require(not output.exists(), f'declarative main requires a fresh directory: {output}')
    verified=read(verification)
    require(verified['status']=='passed' and verified['continuity_closed'], 'declarative integrated continuity verification required')
    selected=deepcopy(config)
    imported=None
    if config['training'].get('first_update',0):
        from declarative_learning_continue import parent
        require(parent_path is not None, 'declarative.continuation: --parent is required')
        imported=parent(parent_path,config)
        selected['training_input']['directory']=str(output/'training-inputs')
    else:selected['training_input']['directory']=str(output/'training-inputs')
    receipt=sources(selected,library)
    if imported:
        require(receipt['native_sha256']==imported['launch']['source']['native_sha256'], 'declarative.continuation: changed native binary')
        for name in ('declarative_learning_policy.py','declarative_learning_input.py','blueprint_edit_input.py',
                     'blueprint_ppo_batch.py','blueprint_execution_update.py'):
            path='tools/factory-edit-pilot/'+name
            require(receipt['files_sha256'][path]==imported['launch']['source']['files_sha256'][path],
                f'declarative.continuation: changed model or optimizer semantics {name}')
    require(receipt['code_sha256']==verified['source']['code_sha256'] and receipt['native_sha256']==verified['source']['native_sha256'],
        'declarative source changed after integrated verification')
    require(digest(config)==verified['config_sha256'], 'declarative numerical configuration changed after verification')
    panel_value=read(panel_path)
    require(panel_value['schema']=='declarative-learning-panel-v1' and panel_value['status']=='complete', 'declarative frozen panel schema')
    require(sha(panel_path)==verified['panel_sha256'], 'declarative evaluation panel differs from verification')
    allocation=cpu_allocation(8)
    require(allocation['workers']==verified['allocation']['workers'], 'declarative verified CPU allocation changed')
    output.mkdir(parents=True)
    launch=dict(schema='declarative-learning-launch-v1',config=selected,source=receipt,allocation=allocation,
        panel=dict(path=str(Path(panel_path).resolve()),sha256=sha(panel_path)),
        verification=dict(path=str(Path(verification).resolve()),sha256=sha(verification)),
        forecast_seconds=verified['forecast_seconds'],created_unix_seconds=time.time())
    if imported:launch['continuation']=imported['binding']
    write(output/'launch.json',launch)
    if imported:
        from declarative_learning_continue import initialize
        write(output/'continuation.json',initialize(imported,launch,output))
    write(output/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,
        launch_sha256=sha(output/'launch.json'),jobs={}))
    return dict(status='ready',output=str(output),forecast_seconds=launch['forecast_seconds'])
