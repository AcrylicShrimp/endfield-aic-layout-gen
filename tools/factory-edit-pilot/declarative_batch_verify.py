"""Bounded logical-worker equivalence and actual 32-stream recovery verification."""
from copy import deepcopy
import io
import os
from pathlib import Path
import statistics
import time
import unittest

import torch

from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_ppo_store import checked, load, read, save, sha, write
from blueprint_ppo_update import digest, require
from blueprint_success_buffer import Evidence
from declarative_batch_experiment import configuration
from declarative_batch_pool import MultiplexPool
from declarative_learning_curriculum import initial_plan
from declarative_learning_launch import pool, sources
from declarative_learning_run import register_inputs
from declarative_learning_runtime import Learner, publish, retain
from declarative_learning_verify_curriculum import compare_restore
import declarative_learning_policy as policy


def equivalent(library, allocation, directory, guard):
    from test_declarative_learning import diagnostic_config
    config=diagnostic_config();config['training']['streams']=8
    recorded=[]
    for kind in ('original','logical'):
        learner=Learner(17,config,'batch-equivalence')
        if kind=='original':workers=BlueprintPool(config,library,allocation['workers'],directory/kind,guard,
            worker_entry=Path(__file__).with_name('declarative_learning_worker.py'),policy=policy)
        else:workers=MultiplexPool(config,library,allocation['workers'],directory/kind,guard)
        # Real clocks differ between executions. Fix only the policy's elapsed
        # feature for this explicit equivalence fixture; native deadlines stay
        # active. Normal collection and all actual-input verification use time.
        class FixedClock:
            @staticmethod
            def inputs(values):
                selected=deepcopy(values)
                for value in selected:value['global_features'][-2]=0.
                return selected
            @classmethod
            def sample(cls,model,inputs,sampler):return policy.sample(model,cls.inputs(inputs),sampler)
            @classmethod
            def values(cls,model,inputs):return policy.values(model,cls.inputs(inputs))
        workers.policy=FixedClock
        try:
            rows=[]
            for _ in range(2):
                payload=workers.collect(learner.behavior_copy(),learner.sampler)
                rows.extend([dict(stream=s['stream'],action=s['action'],decision=s['decision'],reward=s['reward'],
                    old_logp=s['old_logp'],old_value=s['old_value'],terminal=s['terminal']) for s in payload['samples']])
            recorded.append(rows)
        finally:workers.close()
    require(recorded[0]==recorded[1],'declarative.batch-equivalence: eight-stream ordered sampling or native outcome changed')
    return dict(passed=True,actions=len(recorded[0]),rows_sha256=digest(recorded[0]),
        diagnostic_elapsed_feature=0,actual_native_deadlines_preserved=True)


def exercise(protocol, batch, library, allocation, output, guard):
    directory=output/f'batch-{batch}';directory.mkdir()
    config=configuration(protocol,17,batch,output,diagnostic=True)
    identity=f'batch-readiness-{batch}';learner=Learner(17,config,identity)
    job=dict(records=[],update=0,inputs={});evidence=Evidence(directory,job)
    workers=None;rows=[];restored=False
    try:
        workers=pool(config,library,allocation,directory/'workers',guard)
        plan,ref=initial_plan(config,17,directory);workers.assign_starts(plan['slots']);job['curriculum_plans']={'1':ref}
        identities=workers.identities
        for ordinal in range(1,7):
            guard();began=time.perf_counter()
            payload=workers.collect(learner.behavior_copy(),learner.sampler)
            require(len(payload['samples'])==batch and len(payload['frontiers'])==batch//32,
                'declarative.batch-count: one full frozen logical batch')
            register_inputs(payload,job);record=learner.update(payload,evidence)
            publish(learner,payload,record,workers.snapshot(),directory,job,workers=workers)
            retain(learner,directory,job)
            rows.append(dict(update=ordinal,seconds=time.perf_counter()-began,actions=batch,
                completed=len(payload['completed']),curriculum=record['curriculum_completed'],
                max_step_concurrency=workers.max_step_concurrency))
            print(dict(stage='batch-verify',batch=batch,update=ordinal,seconds=round(rows[-1]['seconds'],3)),flush=True)
            if ordinal==3:
                saved=load(checked(job['latest_checkpoint'],directory));before=saved['streams']
                require(len(before)==batch//32 and all(s['native'] is not None and s['policy_actions']==32 for s in before),
                    'declarative.batch-restore: complete unfinished logical-stream coverage')
                expected=learner.checkpoint(before)
                workers.close();workers=None
                learner=Learner(17,config,identity,saved)
                actual=learner.checkpoint(before)
                for key in ('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','buffer'):
                    require(digest(expected[key])==digest(actual[key]),f'declarative.batch-restore: {key}')
                workers=pool(config,library,allocation,directory/'workers-restored',guard,saved=learner.streams)
                compare_restore(before,workers.snapshot());restored=True
        require(all(s['native'] is None for s in workers.snapshot()),'declarative.batch-verify: unfinished final episodes')
    finally:
        if workers:workers.close()
    result=dict(batch=batch,rows=rows,restored=restored,workers=identities,input_slots=len(job['inputs']),
        actual_inputs=job['inputs'],normal_settings=True)
    write(directory/'verification.json',result);return result


def verify(protocol, library, panel_path, output):
    output=Path(output).resolve();require(not output.exists(),f'declarative.batch-verify: fresh directory required: {output}');output.mkdir(parents=True)
    os.environ['AIC_NATIVE_LIBRARY']=str(Path(library).resolve());os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    require(torch.cuda.is_available(),'declarative.batch-verify: CUDA required')
    allocation=cpu_allocation(8);guard=ResourceGuard(output,protocol['readiness_seconds'],32*1024**3,16*1024**3)
    import test_declarative_batch
    import test_declarative_curriculum
    suite=unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromModule(m) for m in (test_declarative_batch,test_declarative_curriculum))
    log=io.StringIO();tested=unittest.TextTestRunner(stream=log,verbosity=2).run(suite)
    (output/'tests.log').write_text(log.getvalue());require(tested.wasSuccessful() and not tested.skipped,'declarative.batch-verify: see tests.log')
    same=equivalent(library,allocation,output/'equivalence',guard)
    results=[exercise(protocol,b,library,allocation,output,guard) for b in (256,1024)]
    a,b=[r['actual_inputs'] for r in results]
    paired=[slot for slot in a.keys()&b.keys() if a[slot]['start_curriculum']['depth']==b[slot]['start_curriculum']['depth']==0]
    require(paired and all(a[k]['input_sha256']==b[k]['input_sha256'] and a[k]['reference']==b[k]['reference'] for k in paired),
        'declarative.batch-input: shared actual fresh inputs differ')
    # A descriptive forecast, not a new arbitrary speedup requirement.
    forecast={str(r['batch']):statistics.median(x['seconds'] for x in r['rows'])*(protocol['actions_per_arm']//r['batch'])*2+60 for r in results}
    result=dict(schema='declarative-batch-verification-v1',status='passed',continuity_closed=True,protocol_sha256=digest(protocol),
        source=sources(configuration(protocol,17,256,output),library),panel_sha256=sha(panel_path),allocation=allocation,
        tests=tested.testsRun,equivalence=same,arms=[{k:v for k,v in r.items() if k!='actual_inputs'} for r in results],
        paired_actual_fresh_slots=len(paired),forecast_by_batch=forecast,seconds=time.monotonic()-guard.began,
        diagnostics_enabled_for_main=False,resource=guard.last)
    write(output/'verification.json',result);return result
