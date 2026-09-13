"""Explicit execution cutover verification, isolated from normal duration training."""
import importlib
import io
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import unittest

from blueprint_execution_resources import ResourceGuard,cpu_allocation
from blueprint_ppo_store import checked,load,read,sha,write
from blueprint_ppo_update import digest,require
from declarative_execution_migrate import diagnostic,parent
from declarative_learning_launch import pool,sources
from declarative_learning_runtime import Learner


def verify(config, directory, compact_directory, policy_proof, replay_proof, inputs, output, environment):
    output=Path(output).resolve();require(not output.exists(),'declarative.execution-verify: fresh output required');output.mkdir(parents=True)
    value=parent(directory);binding=value['binding'];library=value['launch']['source']['native_library']
    policy=read(policy_proof);replay=read(Path(replay_proof)/'report.json');compacted=read(Path(compact_directory)/'manifest.json')
    require(all(p['status']=='passed' and p['parent']==binding for p in (policy,replay,compacted)),
            'declarative.execution-verify: matching policy/replay/compact proofs required')
    require(policy['sampled_actions_equal'] and policy['rows']==1024 and len(replay['rows'])==2 and replay['process_restoration'] and
        all(all(r['exact_equal'].values()) and r['events_equal'] for r in replay['rows']), 'declarative.execution-verify: incomplete equivalence proof')
    prior_ref=value['launch']['verification']
    require(sha(prior_ref['path'])==prior_ref['sha256'],'declarative.execution-verify: predecessor receipt changed')
    prior=read(prior_ref['path'])
    require(prior['status']=='passed' and prior['continuity_closed'],'declarative.execution-verify: predecessor continuity missing')
    allocation=cpu_allocation(config['execution_options']['workers'])
    guard=ResourceGuard(output,600,config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    os.environ['AIC_NATIVE_LIBRARY']=library;os.environ['AIC_RANDOM_INPUT_TESTS']='1'
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    names=('test_declarative_execution_efficiency','test_declarative_learning','test_declarative_replay_retention',
           'test_declarative_duration','test_declarative_curriculum','test_declarative_batch',
           'test_blueprint_execution','test_blueprint_execution_update','test_ppo_data_supply')
    suite=unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromModule(importlib.import_module(name)) for name in names)
    log=io.StringIO();tests=unittest.TextTestRunner(stream=log,verbosity=2).run(suite);(output/'tests.log').write_text(log.getvalue())
    require(tests.wasSuccessful() and not tests.skipped,'declarative.execution-verify: targeted tests failed or skipped; inspect tests.log')
    print(dict(stage='tests',passed=tests.testsRun),flush=True);guard()
    run_root=output/'normal'
    diagnostic(config,library,directory,compact_directory,inputs,run_root)
    process=monitor=None
    try:
        with (output/'gpu.csv').open('w') as gpu,(output/'normal.log').open('w') as log:
            monitor=subprocess.Popen(['nvidia-smi','--query-gpu=timestamp,utilization.gpu,utilization.memory,memory.used,power.draw',
                '--format=csv,noheader','-lms','500'],stdout=gpu,stderr=subprocess.DEVNULL)
            process=subprocess.Popen([sys.executable,str(Path(__file__).with_name('declarative_duration.py')),'run',
                '--output',str(run_root),'--environment',str(Path(environment).resolve())],stdout=log,stderr=log,start_new_session=True)
            while process.poll() is None:guard();time.sleep(.25)
            require(process.returncode==0,'declarative.execution-verify: normal command failed; inspect normal.log')
    finally:
        if process is not None and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        if monitor is not None:
            monitor.terminate()
            try:monitor.wait(timeout=3)
            except subprocess.TimeoutExpired:monitor.kill();monitor.wait()
    guard();state=read(run_root/'state.json');result=read(run_root/'report.json');job=read(run_root/f'seed-{config["seeds"][0]}/state.json')
    require(state['status']=='complete' and job['stop_reason']=='duration' and result['seeds'][0]['actions']==2048 and
        result['seeds'][0]['completed_episodes']==32,'declarative.execution-verify: incomplete normal wave/time/report coverage')
    owner=run_root/f'seed-{config["seeds"][0]}';first=value['saved']['update']+1
    saved=load(checked(read(checked(job['records'][first-1],owner))['checkpoint'],owner))
    selected=read(run_root/'launch.json')['config'];learner=Learner(config['seeds'][0],selected,sha(run_root/'launch.json'),saved)
    require(all(s['native'] is not None and s['policy_actions']==32 for s in saved['streams']), 'declarative.execution-verify: incomplete live checkpoint')
    with pool(selected,library,allocation,output/'restored-workers',guard,saved=learner.streams) as workers:
        from declarative_learning_verify_curriculum import compare_restore
        restored=workers.snapshot();compare_restore(saved['streams'],restored)
    guard();source=sources(config,library)
    records=[read(checked(ref,owner)) for ref in job['records'][first-1:]]
    require(all(not r['full_trace_retained'] and '/replay/' in str(owner/r['chunk']['path']) for r in records),
            'declarative.execution-verify: normal execution retained full trajectories')
    receipt=dict(schema='declarative-execution-verification-v1',status='passed',continuity_closed=True,parent=binding,
        config_sha256=digest(config),source_sha256=source['code_sha256'],compact_manifest_sha256=sha(Path(compact_directory)/'manifest.json'),
        allocation=allocation,tests=tests.testsRun,policy_proof=dict(path=str(Path(policy_proof).resolve()),sha256=sha(policy_proof)),
        replay_proof=dict(path=str((Path(replay_proof)/'report.json').resolve()),sha256=sha(Path(replay_proof)/'report.json')),
        normal=dict(directory=str(run_root),actions=2048,episodes=32,restored_streams=len(restored),
            timings=result['seeds'][0]['timings'],seconds=job['seconds'],resource=job['resource'],
            records=[dict(update=r['update'],collection=r['collection_seconds'],learner=r['ppo']['execution']['update_seconds'],
                publication=r['publication_seconds'],replay_samples=r['replay_samples_retained'],replay_bytes=(owner/r['chunk']['path']).stat().st_size)
                for r in records]),seconds=time.monotonic()-guard.began,diagnostics_enabled_for_main=False,
        scope='Retained exact learner transitions, unchanged sampled CPU/GPU actions, targeted failure/recovery tests, one real normal-command wave and one 32-stream live native restoration. No diagnostics enter normal training.')
    write(output/'verification.json',receipt);return receipt
