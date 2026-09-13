"""Bounded one-off continuity, complete-cycle timing and CUDA execution verification."""
from copy import deepcopy
import io
import json
import os
from pathlib import Path
import statistics
import time
import unittest

import torch

from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_execution_update import update_packed
from blueprint_ppo_store import checked, load, read, save, sha, write
from blueprint_ppo_update import digest, require
from blueprint_success_buffer import Evidence
from declarative_learning_environment import semantic
from declarative_learning_launch import evaluation_cases, pool, sources
from declarative_learning_run import evaluate as evaluate_cycle
from declarative_learning_runtime import Learner, publish, retain
import declarative_learning_policy as policy


def mechanical(config, library, directory):
    os.environ['AIC_NATIVE_LIBRARY']=str(Path(library).resolve())
    import test_declarative_learning
    import test_blueprint_execution_update
    suite=unittest.TestSuite()
    for module in (test_declarative_learning,test_blueprint_execution_update):
        suite.addTests(unittest.defaultTestLoader.loadTestsFromModule(module))
    stream=io.StringIO()
    result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    (directory/'mechanical-tests.log').write_text(stream.getvalue())
    require(result.wasSuccessful() and not result.skipped, 'declarative mechanical/native/CUDA tests failed; see mechanical-tests.log')
    return dict(tests=result.testsRun,passed=True)


def continuity(config, library, allocation, output, guard):
    selected=deepcopy(config)
    selected['training_input'].update(directory=str(output/'inputs'),seed_base=config['readiness']['seed_base'])
    selected['training']['updates']=4
    learner=Learner(17,selected,'declarative-continuity')
    job=dict(records=[],update=0)
    evidence=Evidence(output,job)
    workers=None;rows=[];replayed=False;restored=False
    try:
        workers=pool(selected,library,allocation,output/'workers',guard)
        os.sched_setaffinity(0,allocation['coordinator'])
        identities=workers.identities
        for ordinal in range(1,5):
            before=deepcopy(learner.checkpoint(workers.snapshot()))
            payload=workers.collect(learner.behavior_copy(),learner.sampler)
            actual_config=learner.config
            if ordinal==2:
                learner.config=deepcopy(actual_config);learner.config['ppo']['target_kl']=1e-12
            record=learner.update(payload,evidence)
            snapshots=workers.snapshot()
            publish(learner,payload,record,snapshots,output,job,workers=workers)
            if ordinal==2:
                repeated=Learner(17,learner.config,'declarative-continuity',before)
                repeat_evidence=Evidence(output,dict(records=job['records'][:1]))
                again=repeated.update(deepcopy(payload),repeat_evidence)
                for key in ('model_after','actor_optimizer','critic_optimizer','shuffle_rng','targets_sha256','advantages_sha256'):
                    require(record['ppo'][key]==again['ppo'][key], f'declarative CUDA exact update replay: {key}')
                require(learner.buffer.snapshot()==repeated.buffer.snapshot(), 'declarative replay buffer restoration')
                require(record['ppo']['actor_epochs']<4 and record['ppo']['critic_epochs']==4,
                    'declarative forced KL did not exercise critic-only tail')
                replayed=True;del repeated
            learner.config=actual_config
            if ordinal==3:
                require(all(s['native'] is not None and s['policy_actions']==32 for s in snapshots), 'declarative eight live drafts at restore boundary')
                before_geometry=[(s['metadata']['training_input'],s['expected'],s['native']['elapsed_ns']) for s in snapshots]
                workers.close();workers=None
                learner=Learner(17,selected,'declarative-continuity',load(output/'checkpoints/0003.pt.gz'))
                os.sched_setaffinity(0,allocation['allowed'])
                workers=pool(selected,library,allocation,output/'workers-restored',guard,saved=learner.streams)
                os.sched_setaffinity(0,allocation['coordinator'])
                after=workers.snapshot()
                for prior,now in zip(before_geometry,after,strict=True):
                    require(prior[0]==now['metadata']['training_input'] and prior[1]==now['expected']
                        and prior[2]<=now['native']['elapsed_ns'], 'declarative restored input/draft/time allowance changed')
                restored=True
            rows.append(dict(update=ordinal,actor_epochs=record['ppo']['actor_epochs'],
                terminal_frontiers=record['ppo']['terminal_frontiers'],nonterminal_bootstraps=record['ppo']['nonterminal_bootstraps'],
                retained_successes=record['buffer']['own_entries'],max_step_concurrency=workers.max_step_concurrency))
        require(all(s['native'] is None for s in workers.snapshot()), 'declarative continuity ended with live episodes')
    finally:
        if workers:workers.close()
        os.sched_setaffinity(0,allocation['allowed'])
    result=dict(passed=True,exact_cuda_update_replay=replayed,eight_live_episode_restore=restored,rows=rows,workers=identities)
    write(output/'continuity.json',result)
    return result


def cuda_imitation(config, library, directory):
    # The explicit fixture verifies positive SIL gradients on the new joint
    # likelihood. It never initializes or populates a main learner/buffer.
    from test_declarative_learning import NativeLearningTests, MemoryEvidence
    from declarative_learning_buffer import Buffer
    helper=NativeLearningTests();helper.setUp()
    first,second=helper.preparation_chunks()
    evidence,buffer=MemoryEvidence(),Buffer()
    for ordinal,payload in enumerate((first,second),1):
        evidence.begin(ordinal,payload);buffer.ingest(payload,ordinal,evidence,helper.config['success'],1.)
    sequences=buffer.eligible(evidence,3,helper.config['strata'])
    model,actor,critic=policy.learners(17,helper.config['ppo'],'cuda:0')
    with torch.no_grad():model.critic[-1].bias.sub_(1.)
    shuffled=torch.Generator().manual_seed(9)
    with torch.profiler.profile(activities=[torch.profiler.ProfilerActivity.CPU,torch.profiler.ProfilerActivity.CUDA]) as profile:
        result=update_packed(model,actor,critic,shuffled,second['samples'],second['frontiers'],helper.config['ppo'],
            sequences,.05,'balanced_available',17,3,3,'cuda:0',likelihood=policy)
        torch.cuda.synchronize()
    events=[dict(name=e.key,count=e.count,device_us=e.device_time_total) for e in profile.key_averages() if e.device_time_total>0]
    require(events and result['success']['positive_labels']>0, 'declarative actual CUDA/SIL work missing')
    proof=dict(passed=True,positive_labels=result['success']['positive_labels'],events=events,
        whole_joint_likelihood=True,diagnostic_critic_bias_shift=-1.)
    write(directory/'cuda-imitation.json',proof)
    return proof


def measure(config, library, panel, allocation, output, guard):
    from declarative_learning_serial import Serial
    modes=[('serial_cpu','cpu'),('parallel_cpu','cpu'),('parallel_cuda','cuda:0')]
    results=[]
    case_config=deepcopy(config);case_config['evaluation']['replicates']=1
    cases=evaluation_cases(panel,case_config)
    # Every mode/repetition sees the same sixteen actual generated inputs.
    shared_inputs=output/'inputs'
    for mode,device in modes:
        selected=deepcopy(config)
        selected['training_input'].update(directory=str(shared_inputs),seed_base=config['readiness']['seed_base']+100000)
        selected['device']=device;selected['training']['updates']=config['readiness']['cycle_updates']
        selected['retention']['checkpoints']=[0,4]
        times=[];cycles=[]
        for repetition in range(config['readiness']['warmups']+config['readiness']['repetitions']):
            directory=output/f'{mode}-{repetition}';directory.mkdir(parents=True)
            learner=Learner(17,selected,'declarative-complete-cycle',device=device)
            job=dict(records=[],update=0)
            evidence=Evidence(directory,job)
            os.sched_setaffinity(0,allocation['allowed'])
            if mode=='serial_cpu':
                os.sched_setaffinity(0,{allocation['workers'][0]})
                workers=Serial(selected,library,guard)
            else:
                workers=pool(selected,library,allocation,directory/'workers',guard)
                os.sched_setaffinity(0,allocation['coordinator'])
            cycle_began=time.perf_counter();records=[];input_hashes=[]
            try:
                for _ in range(config['readiness']['cycle_updates']):
                    guard()
                    payload=workers.collect(learner.behavior_copy(),learner.sampler)
                    input_hashes.extend(s['metadata']['training_input']['input_sha256'] for s in payload['native_before'] if s['policy_actions']==0)
                    record=learner.update(payload,evidence);records.append(record)
                    publish(learner,payload,record,workers.snapshot(),directory,job,workers=workers)
                    retain(learner,directory,job)
                measured=evaluate_cycle(learner,workers,cases,directory,job,guard)
                elapsed=time.perf_counter()-cycle_began
                require(all(row['policy_actions']==config['evaluation']['max_actions'] for row in measured['rows']),
                    'declarative complete-cycle evaluation did not execute full action budget')
                row=dict(repetition=repetition,warmup=repetition<config['readiness']['warmups'],seconds=elapsed,
                    collection_seconds=sum(r['collection_seconds'] for r in records),
                    learner_seconds=sum(r['ppo']['execution']['update_seconds'] for r in records),
                    publication_seconds=sum(r['publication_seconds'] for r in records),evaluation_seconds=measured['seconds'],
                    actions=sum(r['actions'] for r in records),completed_episodes=sum(r['quality'].get('accepted',0) for r in records),
                    max_step_concurrency=workers.max_step_concurrency,input_hashes=input_hashes,
                    workers=workers.identities)
                cycles.append(row)
                if not row['warmup']:times.append(elapsed)
            finally:
                workers.close();os.sched_setaffinity(0,allocation['allowed'])
            write(directory/'cycle.json',row)
            print(dict(stage='measure',mode=mode,repetition=repetition,seconds=round(elapsed,3)),flush=True)
        results.append(dict(mode=mode,device=device,median_seconds=statistics.median(times),range_seconds=[min(times),max(times)],cycles=cycles))
    inputs=results[0]['cycles'][0]['input_hashes']
    require(len(inputs)==16 and all(c['input_hashes']==inputs for r in results for c in r['cycles']), 'declarative complete-cycle paired input identity')
    ratio=results[0]['median_seconds']/results[-1]['median_seconds']
    measured=[c for c in results[-1]['cycles'] if not c['warmup']]
    update_seconds=statistics.median(c['collection_seconds']+c['learner_seconds']+c['publication_seconds'] for c in measured)/config['readiness']['cycle_updates']
    evaluation_seconds=statistics.median(c['evaluation_seconds'] for c in measured)*config['evaluation']['replicates']
    forecast=(len(config['seeds'])*(update_seconds*config['training']['updates']+evaluation_seconds*len(config['evaluation']['at']))+evaluation_seconds)*1.25+60
    # A predecessor benchmark ratio is a comparison, not a new-workload gate.
    require(forecast<config['limits']['matrix_seconds'], 'declarative measured short-run forecast exceeds registered wall ceiling')
    require(all(c['max_step_concurrency']>=2 for c in measured), 'declarative native workers did not overlap')
    result=dict(passed=True,modes=results,speedup=ratio,forecast_seconds=forecast,
        predecessor_speedup_reference=1.5,meets_predecessor_reference=ratio>=1.5)
    write(output/'measurement.json',result)
    return result


def verify(config, library, panel_path, output):
    output=Path(output).resolve()
    require(not output.exists(), f'declarative verification needs a fresh output: {output}')
    output.mkdir(parents=True)
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    require(torch.cuda.is_available(), 'declarative CUDA verification requires CUDA')
    receipt=sources(config,library);allocation=cpu_allocation(8)
    guard=ResourceGuard(output,config['readiness']['seconds'],config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    tests=mechanical(config,library,output);guard()
    print(dict(stage='verify',mechanical_tests=tests['tests']),flush=True)
    continued=continuity(config,library,allocation,output/'continuity',guard);guard()
    print(dict(stage='verify',continuity='passed'),flush=True)
    cuda=cuda_imitation(config,library,output);guard()
    measured=measure(config,library,read(panel_path),allocation,output/'measurement',guard);guard()
    os.sched_setaffinity(0,allocation['allowed'])
    result=dict(schema='declarative-learning-verification-v1',status='passed',continuity_closed=True,
        source=receipt,config_sha256=digest(config),panel_sha256=sha(panel_path),allocation=allocation,
        mechanical=tests,continuity=continued,cuda_imitation_positive_labels=cuda['positive_labels'],
        measurement=measured,forecast_seconds=measured['forecast_seconds'],seconds=time.monotonic()-guard.began,
        resource=guard.last,diagnostics_enabled_for_main=False)
    write(output/'verification.json',result)
    return result
