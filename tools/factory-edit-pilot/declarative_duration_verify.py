"""Explicit isolated server readiness; never called by normal duration training."""
from copy import deepcopy
import io
import os
import signal
import subprocess
import sys
from pathlib import Path
import time
import unittest

from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_ppo_store import checked, load, read, sha, write
from blueprint_ppo_update import digest, require
from declarative_duration import build
from declarative_duration_import import parent
from declarative_learning_launch import pool, sources
from declarative_learning_runtime import Learner


def verify(config, library, panel, parent_path, output, environment):
    output=Path(output).resolve();require(not output.exists(),f'declarative.duration-verify: fresh output required: {output}')
    output.mkdir(parents=True)
    allocation=cpu_allocation(8);guard=ResourceGuard(output,600,config['limits']['matrix_rss_bytes'],config['limits']['min_free_bytes'])
    os.environ['AIC_NATIVE_LIBRARY']=str(Path(library).resolve())
    value=parent(parent_path,config);binding=value['binding'];proof=value['launch']['verification']
    require(sha(proof['path'])==proof['sha256'], 'declarative.duration-verify: changed predecessor receipt')
    prior=read(proof['path']);require(prior['status']=='passed' and prior['continuity_closed'], 'declarative.duration-verify: predecessor continuity missing')
    del value
    import test_declarative_replay_retention, test_declarative_duration
    suite=unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromModule(m) for m in
        (test_declarative_replay_retention,test_declarative_duration))
    log=io.StringIO();tests=unittest.TextTestRunner(stream=log,verbosity=2).run(suite)
    (output/'tests.log').write_text(log.getvalue())
    require(tests.wasSuccessful() and not tests.skipped, 'declarative.duration-verify: targeted tests failed or skipped')
    guard();selected=deepcopy(config)
    selected.update(purpose='declarative-duration-readiness')
    selected['duration'].update(seconds=.001,finish_grace_seconds=450.)
    selected['limits'].update(arm_seconds=450.001,matrix_seconds=480.)
    selected['training']['updates']=selected['training']['first_update']+2
    selected['tracking']['mode']='disabled'
    run_root=output/'integrated'
    build(selected,library,panel,parent_path,proof['path'],run_root,diagnostic=True)
    # The normal command's time stop, publication, evaluation and final reporting
    # are exercised once. No profiler or redundant optimizer replay is attached.
    with (output/'normal-run.log').open('w') as log:
        process=subprocess.Popen([sys.executable,str(Path(__file__).with_name('declarative_duration.py')),'run',
            '--output',str(run_root),'--environment',str(Path(environment).resolve())],stdout=log,stderr=log,start_new_session=True)
        try:
            while process.poll() is None:guard();time.sleep(.25)
            require(process.returncode==0, 'declarative.duration-verify: normal command failed; inspect normal-run.log')
        finally:
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGTERM)
                try:process.wait(timeout=5)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
    state=read(run_root/'state.json')
    guard();require(state['status']=='complete', 'declarative.duration-verify: normal duration path did not complete')
    seed=config['seeds'][0];owner=run_root/f'seed-{seed}';job=read(owner/'state.json')
    require(job['stop_reason']=='duration' and job['update']==selected['training']['updates'], 'declarative.duration-verify: wrong duration boundary')
    result=read(run_root/'report.json')
    require(result['seeds'][0]['actions']==2048 and result['seeds'][0]['completed_episodes']==32,
        'declarative.duration-verify: full logical wave coverage')
    first=config['training']['first_update']+1
    saved=load(checked(read(checked(job['records'][first-1],owner))['checkpoint'],owner))
    learner=Learner(seed,read(run_root/'launch.json')['config'],sha(run_root/'launch.json'),saved)
    require(all(s['native'] is not None and s['policy_actions']==32 for s in learner.streams),
        'declarative.duration-verify: unfinished checkpoint missing')
    with pool(read(run_root/'launch.json')['config'],library,allocation,output/'restored-workers',guard,saved=learner.streams) as workers:
        restored=workers.snapshot()
        from declarative_learning_verify_curriculum import compare_restore
        compare_restore(saved['streams'],restored)
    guard()
    source=sources(config,library)
    receipt=dict(schema='declarative-duration-verification-v1',status='passed',continuity_closed=True,
        config_sha256=digest(config),source_sha256=source['code_sha256'],parent=binding,panel_sha256=sha(panel),allocation=allocation,
        tests=tests.testsRun,normal_completed_updates=2,normal_completed_actions=2048,live_restored_streams=len(restored),
        normal_phase_seconds=result['seeds'][0]['timings'],normal_rss=job['resource'],
        seconds=time.monotonic()-guard.began,diagnostics_enabled_for_main=False,
        scope='Two actual normal-path updates and one unfinished-checkpoint restoration; no profiler, full-buffer inference or repeated benchmark matrix.')
    write(output/'verification.json',receipt);return receipt
