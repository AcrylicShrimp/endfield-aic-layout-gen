"""One bounded continuation diagnostic; reuse completed predecessor kernel proofs."""
from copy import deepcopy
import io
import os
from pathlib import Path
import time
import unittest

import torch

from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_ppo_store import checked, load, read, sha, write
from blueprint_ppo_update import digest, require
from blueprint_success_buffer import Evidence
from declarative_learning_continue import parent, initialize
from declarative_learning_launch import pool, sources
from declarative_learning_run import register_inputs
from declarative_learning_runtime import Learner, publish, retain


def verify(config, library, panel_path, output, parent_path, resume=False):
    require(parent_path is not None, 'declarative curriculum verification requires --parent')
    output=Path(output).resolve()
    require(resume or not output.exists(), f'declarative verification requires fresh output: {output}')
    output.mkdir(parents=True,exist_ok=resume)
    progress=read(output/'diagnostic-state.json') if resume else None
    require(progress is None or (progress['config_sha256']==digest(config) and progress['parent']==str(Path(parent_path).resolve())),
        'declarative diagnostic resume identity')
    os.environ['AIC_NATIVE_LIBRARY']=str(Path(library).resolve())
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    require(torch.cuda.is_available(), 'declarative curriculum verification requires CUDA')
    allocation=cpu_allocation(8)
    guard=ResourceGuard(output,600-(progress['seconds'] if progress else 0),config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    selected=deepcopy(config)
    value=parent(parent_path,config)
    proof_ref=value['launch']['verification']
    require(sha(proof_ref['path'])==proof_ref['sha256'], 'declarative predecessor proof identity')
    old_proof=read(proof_ref['path'])
    require(old_proof['status']=='passed' and old_proof['continuity_closed'] and
        sha(panel_path)==old_proof['panel_sha256'], 'declarative missing predecessor proof or changed panel')
    selected['training_input']['directory']=value['launch']['config']['training_input']['directory']
    # Existing immutable generated inputs remain bound. New diagnostic slots go
    # into a separate store, so they cannot pre-generate the main continuation.
    selected['training']['updates']=selected['training']['first_update']+6
    selected['retention']['checkpoints']=[selected['training']['first_update'],selected['training']['updates']]
    if progress is None:
        write(output/'launch.json',dict(config=selected))
        imported=initialize(value,dict(config=selected),output)
    else:imported=progress['imported']
    seed=config['seeds'][0];directory=output/f'seed-{seed}'
    job=read(directory/'state.json');saved=load(directory/f'initial/continued-{config["training"]["first_update"]:04d}.pt.gz')
    for key in ('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','buffer'):
        require(digest(saved[key])==digest(value['saved'][key]), f'declarative import changed {key}')
    del value
    # Avoid repeated old optimizer/profiler tests: only changed curriculum/cache
    # contracts are tested here. Their native fixture covers both inherited depths.
    import test_declarative_curriculum
    if progress is None:
        suite=unittest.defaultTestLoader.loadTestsFromModule(test_declarative_curriculum)
        log=io.StringIO();tests=unittest.TextTestRunner(stream=log,verbosity=2).run(suite)
        (output/'mechanical-tests.log').write_text(log.getvalue())
        require(tests.wasSuccessful() and not tests.skipped, 'declarative curriculum tests failed: mechanical-tests.log')
        progress=dict(config_sha256=digest(config),parent=str(Path(parent_path).resolve()),imported=imported,tests=tests.testsRun,seconds=0.,rows=[])
        write(output/'diagnostic-state.json',progress)
    print(dict(stage='curriculum-verify',tests=progress['tests'],imported='exact model/Adam/RNG/replay'),flush=True)
    selected['training_input']['directory']=str(output/'diagnostic-inputs')
    selected['training_input']['seed_base']=config['readiness']['seed_base']+200000
    if resume:saved=load(checked(job['latest_checkpoint'],directory))
    learner=Learner(seed,selected,sha(output/'launch.json'),saved)
    del saved
    evidence=Evidence(directory,job);workers=None;rows=progress['rows'];restored=False
    prior_seconds=progress['seconds']
    try:
        workers=pool(selected,library,allocation,directory/'workers',guard,saved=learner.streams)
        os.sched_setaffinity(0,allocation['coordinator'])
        if resume:
            before=learner.streams;after=workers.snapshot()
            compare_restore(before,after);restored=True
        for step in range(learner.ordinal-config['training']['first_update'],6):
            guard();payload=workers.collect(learner.behavior_copy(),learner.sampler)
            register_inputs(payload,job)
            record=learner.update(payload,evidence)
            publish(learner,payload,record,workers.snapshot(),directory,job,workers=workers)
            retain(learner,directory,job)
            rows.append(dict(update=learner.ordinal,collection_seconds=record['collection_seconds'],
                update_seconds=record['ppo']['execution']['update_seconds'],publication_seconds=record['publication_seconds'],
                completed=record['curriculum_completed'],max_step_concurrency=workers.max_step_concurrency))
            progress.update(rows=rows,seconds=prior_seconds+time.monotonic()-guard.began)
            write(output/'diagnostic-state.json',progress)
            print(dict(stage='curriculum-verify',update=learner.ordinal,seconds=round(progress['seconds'],2)),flush=True)
            if step==2:
                before=load(checked(job['latest_checkpoint'],directory))['streams']
                require(all(s['native'] is not None and s['policy_actions']==32 for s in before),
                    'declarative continuation live restore boundary')
                workers.close();workers=None
                restored_learner=Learner(seed,selected,sha(output/'launch.json'),load(checked(job['latest_checkpoint'],directory)))
                for key in ('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','buffer'):
                    require(digest(learner.checkpoint(before)[key])==digest(restored_learner.checkpoint(before)[key]),
                        f'declarative continuation restoration changed {key}')
                learner=restored_learner
                os.sched_setaffinity(0,allocation['allowed'])
                workers=pool(selected,library,allocation,directory/'workers-restored',guard,saved=learner.streams)
                os.sched_setaffinity(0,allocation['coordinator'])
                after=workers.snapshot()
                compare_restore(before,after)
                restored=True
        require(all(s['native'] is None for s in workers.snapshot()), 'declarative diagnostic incomplete episodes')
    finally:
        if workers:workers.close()
        os.sched_setaffinity(0,allocation['allowed'])
    typical=sum(r['collection_seconds']+r['update_seconds']+r['publication_seconds'] for r in rows)/len(rows)
    forecast=typical*(config['training']['updates']-config['training']['first_update'])*1.5+120
    require(forecast<config['limits']['arm_seconds'], 'declarative continuation forecast exceeds arm ceiling')
    result=dict(schema='declarative-learning-verification-v2',status='passed',continuity_closed=True,
        source=sources(config,library),config_sha256=digest(config),panel_sha256=sha(panel_path),allocation=allocation,
        imported=imported,mechanical_tests=progress['tests'],live_restore=restored,rows=rows,
        predecessor_proof=dict(path=str(parent_path),verification=read(Path(parent_path)/'launch.json')['verification']),
        forecast_seconds=forecast,seconds=prior_seconds+time.monotonic()-guard.began,diagnostics_enabled_for_main=False,
        diagnostic_scope='Six updates on separate generated inputs; no profiler or repeat of the completed 12-cycle benchmark.')
    write(output/'verification.json',result)
    return result


def compare_restore(before, after):
    for a,b in zip(before,after,strict=True):
        for key in ('next_start','metadata','expected'):
            require(a[key]==b[key], f'declarative restored {key} stream {a["index"]}')
        require(a['native'] is not None and b['native'] is not None and
            a['native']['elapsed_ns']<=b['native']['elapsed_ns'],
            f'declarative restored checkpoint allowance stream {a["index"]}')
