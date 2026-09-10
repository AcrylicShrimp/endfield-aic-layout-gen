"""Real pipe, timeout, detached descendant and supervisor fault checks."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import uuid

from ppo_harness_pool import Pool
from ppo_harness_schedule import cleanup_phase, phase_tasks
from ppo_harness_state import SCHEMA, HarnessError, read, write, sha, load_state
import ppo_harness_supervisor as supervisor
from test_ppo_harness_state import config


class ProcessTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        self.cfg = config()
        self.cfg['execution']['workers'] = 2
        self.cfg['limits'].update(startup_seconds=.4, task_seconds=.3, batch_seconds=2, cleanup_seconds=.1)

    def tearDown(self):
        self.temporary.cleanup()

    def script(self, text, name='worker.py'):
        path = self.root / name
        path.write_text(text)
        return path

    def pool(self, script):
        return Pool(self.root, self.root / 'pool', self.cfg, lambda **kw: None, script)

    def test_startup_eof_and_hung_task_fail_with_bounded_cleanup(self):
        for name, source, expected, collect in [
            ('startup', 'import time; time.sleep(20)', 'startup-timeout', False),
            ('eof', '', 'worker-eof', False),
            ('task', 'import time; print(\'{"kind":"ready"}\',flush=True); time.sleep(20)', 'worker-timeout', True),
            ('oversize', 'print("x" * (1024 * 1024 + 1),flush=True)', 'worker-message-size', False)]:
            with self.subTest(fault=name):
                script = self.script(source)
                began = time.monotonic()
                with self.assertRaisesRegex(HarnessError, expected):
                    with self.pool(script) as pool:
                        if collect:
                            pool.collect([self.root / 'task.json'])
                self.assertLess(time.monotonic() - began, 5)
                import shutil
                shutil.rmtree(self.root / 'pool')

    def test_completion_order_is_separate_from_training_order_and_credentials_removed(self):
        worker = self.script('''import json,os,sys,time
assert 'WANDB_API_KEY' not in os.environ
print(json.dumps(dict(kind='ready')),flush=True)
for line in sys.stdin:
    command=json.loads(line)
    if command['op']=='stop':break
    index=int(command['task'])
    time.sleep(.15 if index==0 else .01)
    print(json.dumps(dict(kind='result',index=index)),flush=True)
''')
        self.cfg['limits']['task_seconds'] = 2
        with patch.dict(os.environ, WANDB_API_KEY='fake-private-key'), patch('ppo_harness_pool.accept_result', side_effect=lambda root,path,msg: msg):
            with self.pool(worker) as pool:
                results, timing = pool.collect([0, 1, 2])
                self.assertEqual([r['index'] for r in results], [0, 1, 2])
                self.assertEqual(timing['completion_order'], [1, 2, 0])
            self.assertTrue(all(child.poll() is not None for child in pool.children))

    def test_orphaned_group_and_detached_unregistered_service_are_owned(self):
        session = uuid.uuid4().hex
        environment = dict(os.environ, **{supervisor.SESSION_KEY: session})
        # The leader exits; the remaining descendant ignores TERM. This also
        # exercises Darwin's environment ownership path without /proc.
        worker = self.script('''import os,signal,time
pid=os.fork()
if pid:os._exit(0)
signal.signal(signal.SIGTERM,signal.SIG_IGN)
while True:time.sleep(.05)
''')
        leader = subprocess.Popen([sys.executable, str(worker)], env=environment, start_new_session=True)
        record = dict(pid=leader.pid, identity=supervisor.identity(leader.pid))
        leader.wait(timeout=3)
        unrelated = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(20)'], start_new_session=True)
        # A separately detached SDK-like service was launched before registration.
        detached = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(20)'], env=environment, start_new_session=True)
        owned = [record]
        try:
            time.sleep(.1)
            self.assertTrue(supervisor.group_is_owned(record, session))
            found = supervisor.discover_owned(owned, session, full_scan=True)
            self.assertIn(detached.pid, [p['pid'] for p in found])
            self.assertNotIn(unrelated.pid, [p['pid'] for p in found])
            supervisor.cleanup(owned, session, .1)
            detached.wait(timeout=3)
            self.assertFalse(supervisor.members(leader.pid))
            self.assertIsNone(unrelated.poll())
        finally:
            supervisor.cleanup(owned, session, .1)
            if detached.poll() is None:
                detached.kill()
            detached.wait(timeout=3)
            unrelated.kill()
            unrelated.wait(timeout=3)

    def test_nested_directory_links_and_cleanup_scope(self):
        synced = []
        job = dict(id='job', candidate='history-no-restore', seed=17,
                   checkpoint=dict(path='checkpoint.pt', sha256='file', policy_sha256='actor', critic_sha256='critic'))
        with patch('ppo_harness_state.sync_directory', side_effect=lambda path: synced.append(Path(path))):
            phase, tasks, _, _ = phase_tasks(self.root, dict(config=self.cfg), 'protocol', job, dict(kind='training', batch=1))
        self.assertIn(self.root, synced)
        self.assertIn(self.root / 'job', synced)
        self.assertIn(phase, synced)
        attempt = tasks[0][1].parent
        other = self.root / 'other-job'
        other.mkdir()
        (other / 'checkpoint.pt').write_bytes(b'keep')
        (attempt / 'generated').symlink_to(other, target_is_directory=True)
        with self.assertRaisesRegex(HarnessError, 'cleanup-symlink'):
            cleanup_phase(self.root, phase, [dict(result_path=str(attempt / 'result.json'))], False)
        self.assertEqual((other / 'checkpoint.pt').read_bytes(), b'keep')

    def test_resume_reads_commit_after_old_coordinator_shutdown(self):
        protocol = dict(schema=SCHEMA, backend='retained-full-factory-native', sources={}, environment={}, jobs=[], phases=[],
                        config=dict(limits=dict(cleanup_seconds=.1, run_seconds=600, disk_reserve_bytes=0)))
        write(self.root / 'protocol.json', protocol)
        write(self.root / 'state.json', dict(schema=SCHEMA, protocol_sha256=sha(self.root / 'protocol.json'),
              status='running', jobs=[], cursor=0, elapsed_seconds=0, sessions=1))
        write(self.root / 'supervisor.json', dict(status='running', session='old', processes=[], prior_seconds=0,
                                                 started_epoch=time.time()))
        def late_commit(*args):
            state = load_state(self.root)
            state['late_committed_marker'] = 'must survive resume'
            write(self.root / 'state.json', state)
        class Captured(Exception):
            pass
        def capture(path, value):
            if path.name == 'state.json':
                self.assertEqual(value['late_committed_marker'], 'must survive resume')
                raise Captured
            write(path, value)
        with patch('ppo_harness_protocol.environment', return_value={}), patch.object(supervisor, 'cleanup', side_effect=late_commit), patch.object(supervisor, 'write', side_effect=capture):
            with self.assertRaises(Captured):
                supervisor.supervise(self.root, resume=True)

    def test_supervisor_progress_and_rss_watchdogs_kill_real_children(self):
        for reason in ('progress-timeout', 'rss-limit', 'run-timeout'):
            with self.subTest(reason=reason):
                limits = dict(self.cfg['limits'], progress_seconds=.1 if reason == 'progress-timeout' else 30,
                              run_seconds=.1 if reason == 'run-timeout' else 30,
                              rss_bytes=1 if reason == 'rss-limit' else 1024**4, disk_reserve_bytes=0)
                protocol = dict(schema=SCHEMA, backend='retained-full-factory-native', sources={}, environment={}, jobs=[], phases=[], config=dict(limits=limits))
                write(self.root / 'protocol.json', protocol)
                write(self.root / 'state.json', dict(schema=SCHEMA, protocol_sha256=sha(self.root / 'protocol.json'),
                      status='prepared', jobs=[], cursor=0, elapsed_seconds=0, sessions=0))
                (self.root / 'supervisor.json').unlink(missing_ok=True)
                real_popen = subprocess.Popen
                spawned = []
                def popen(args, **kwargs):
                    if any('ppo_harness_' in str(arg) for arg in args):
                        args = [sys.executable, '-c', 'import time; time.sleep(20)']
                        child = real_popen(args, **kwargs)
                        spawned.append(child)
                        return child
                    return real_popen(args, **kwargs)
                with patch('ppo_harness_protocol.environment', return_value={}), patch.object(supervisor, 'gpu_sample', return_value=None), patch.object(supervisor.subprocess, 'Popen', side_effect=popen):
                    result = supervisor.supervise(self.root)
                self.assertEqual(result['diagnostic']['code'], 'ppo-harness.' + reason)
                self.assertEqual(len(spawned), 2)
                self.assertTrue(all(child.poll() is not None for child in spawned))
                self.assertTrue(all(not supervisor.members(child.pid) for child in spawned))


if __name__ == '__main__':
    unittest.main()
