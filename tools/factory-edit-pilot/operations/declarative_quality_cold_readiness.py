"""Bounded cold 0→4 execution and identical-origin live 1→2 recovery."""
from collections import defaultdict
from copy import deepcopy
from pathlib import Path
import time

from declarative_quality_cold import (BASE,ROOT,FIXED,FIELDS,bound,check_bound,config_for,endpoint,monitor,
    new_bank,prepare,protocol,source_check)
from declarative_quality_branch import PROFILES,checked,digest,load,read,require,source_binding,write
from declarative_quality_storage import Budget
from blueprint_execution_resources import ResourceGuard,cpu_allocation


def verify(args):
    from declarative_quality_study import panel,child
    began=time.monotonic();f=protocol();output=args.output.resolve()
    require(not output.exists(),'quality-cold.readiness: fresh diagnostic directory required')
    main=monitor(args.monitor_main);config=config_for(71,'score-8','readiness',output/'uninterrupted',600)
    source=source_binding(config,args.library);cuda=read(args.cuda_proof)
    require(cuda['schema']=='declarative-quality-cuda-proof-v1' and cuda['status']=='passed' and
        [r['profile'] for r in cuda['rows']]==list(PROFILES) and all(r['device']=='cuda:0' and r['nonempty_adam'] for r in cuda['rows']),
        'quality-cold.readiness: complete identical-input CUDA proof required')
    core=('blueprint_execution_update.py','declarative_learning_runtime.py','declarative_learning_policy.py',
        'declarative_learning_buffer.py','declarative_replay_sampling.py','declarative_success_features.py',
        'declarative_replay_retention.py','operations/declarative_quality_observer.py',
        'operations/declarative_quality_cuda_verify.py')
    from blueprint_ppo_store import sha
    for name in core:
        require(cuda['source']['files_sha256']['tools/factory-edit-pilot/'+name]==sha(BASE/name),
            f'quality-cold.readiness: CUDA-verified execution core changed: {name}')
    require(cuda['source']['native_sha256']==source['native_sha256'],'quality-cold.readiness: CUDA native changed')
    output.mkdir();guard=ResourceGuard(output,600,config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    guard.began=began;budget=Budget(output/'storage-budget.json',f['max_added_bytes'])
    try:
        diagnostic_panel=panel(config,args.library,output/'panel','readiness',f,set(),guard)
        shared=output/'fresh-bank';new_bank(shared,config,args.library)
        proof=prepare(output/'uninterrupted',seed=71,profile='score-8',stage='readiness',panel=diagnostic_panel['path'],
            library=args.library,shared=shared,seconds=600-(time.monotonic()-began))
        budget.scan(output);budget.save();child(output/'uninterrupted',args.environment,budget,guard)
        left=endpoint(output/'uninterrupted');right_before=endpoint(output/'restored',complete=False)
        recovery=read(output/'restored/diagnostic-recovery.json')
        require(left['saved']['update']==4 and right_before['saved']['update']==1 and
            digest(right_before['saved'])==recovery['saved_state_sha256']==digest(load(check_bound(recovery['initial_checkpoint']))),
            'quality-cold.readiness: exact saved checkpoint identity differs')
        from declarative_learning_environment import Runtime,Stream
        from declarative_learning_verify_curriculum import compare_restore
        runtime=Runtime(config,args.library);snapshots=[]
        for state in right_before['saved']['streams']:
            guard();require(state['native'] is not None and state['policy_actions']==32,'quality-cold.readiness: expected live checkpoint')
            restored=Stream.restore(runtime,state);snapshot=restored.snapshot();a,b=deepcopy(state),deepcopy(snapshot)
            require(a['native']['elapsed_ns']<=b['native']['elapsed_ns'],'quality-cold.readiness: native time allowance reset')
            b['native']['elapsed_ns']=a['native']['elapsed_ns']
            require(digest(a)==digest(b),'quality-cold.readiness: native/input state changed beyond monotonic elapsed')
            snapshots.append(snapshot);del restored
        compare_restore(right_before['saved']['streams'],snapshots)
        child(output/'restored',args.environment,budget,guard);right=endpoint(output/'restored',complete=False)
        require(right['job']['status']=='paused' and right['saved']['update']==2,'quality-cold.readiness: exact followup allocation')
        initial=load(left['owner']/'checkpoints/0000.pt.gz')
        require({k:digest(initial[k]) for k in FIELDS}==proof['preserved'] and initial['update']==0 and
            all(s['policy_actions']==0 and s['episode']==0 for s in initial['streams']),
            'quality-cold.readiness: nonzero initial stream/history or changed initialization')
        rows=[];coverage=[]
        from declarative_replay_storage import references
        for v,ordinals,states in ((left,range(1,5),[left['saved'],right_before['saved']]),(right,[2],[right['saved'],right_before['saved']])):
            for n in ordinals:
                guard();record=read(checked(v['job']['records'][n-1],v['owner']))
                chunk=load(checked(v['job']['replay_sources'][str(n)],v['owner']))
                require(record['update']==n and record['actions']==1024 and not record['full_trace_retained'],
                    'quality-cold.readiness: normal compact publication mismatch')
                require(len(chunk['completed'])==(32 if n%2==0 else 0) and
                    all(r['result']['factory_validated'] for r in chunk['completed']),
                    'quality-cold.readiness: incomplete or invalid native outputs')
                rows.append(dict(branch=v['directory'].name,update=n,record=bound(v['owner']/v['job']['records'][n-1]['path']),
                    collection_seconds=record['collection_seconds'],learner_seconds=record['ppo']['execution']['update_seconds'],
                    publication_seconds=record['publication_seconds'],buffer_entries=record['buffer']['own_entries'],
                    evictions=sum(e['kind']=='evicted' for e in record['events']),completed=len(chunk['completed']),native_validated=True))
            needed=defaultdict(set)
            for saved in states:
                for n,indices in references(saved['buffer']).items():needed[n].update(indices)
            for n,indices in needed.items():
                guard();chunk=load(checked(v['job']['replay_sources'][str(n)],v['owner']))
                require(indices<=chunk['samples'].keys(),'quality-cold.readiness: missing required replay/live rows')
            coverage.append(dict(branch=v['directory'].name,sources=len(needed),rows=sum(map(len,needed.values()))))
        require(monitor(args.monitor_main)==main,'quality-cold.main: paused main changed')
        budget.scan(output);budget.save();guard()
        result=dict(schema='declarative-cold-quality-readiness-v1',status='passed',continuity_closed=True,
            protocol=bound(FIXED),numerical_base_sha256=digest(f['numerical_base']),source=source,
            allocation=cpu_allocation(config['execution_options']['workers']),main=main,cuda_proof=bound(args.cuda_proof),
            executed_updates=5,seconds=time.monotonic()-began,rows=rows,required_sources=coverage,
            initial_proof=bound(output/'uninterrupted/quality-branch.json'),live_streams_restored=len(snapshots),
            exact_saved_state_sha256=recovery['saved_state_sha256'],unchanged_native_limits=True,
            future_trajectory_equality_required=False,resource=guard.last,actual_added=budget.actual(output),
            scope='Registered cold family only. Diagnostic 0→4 plus exact same-origin live 1→2; no inherited scientific state.')
        write(output/'readiness.json',result);return result
    except BaseException as error:
        write(output/'failure.json',dict(status='failed',seconds=time.monotonic()-began,message=str(error)));raise
