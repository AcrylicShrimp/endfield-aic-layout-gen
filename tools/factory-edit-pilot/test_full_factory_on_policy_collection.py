"""Exercise process ordering, identity rejection and failure cleanup without routing."""
import json
import gzip
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from full_factory_on_policy_collection import (
    CollectionError, ProcessTreeMemory, _run_jobs, collect_group, processes, read, sha, write,
)


STUB = r'''
import argparse, hashlib, json, os
from pathlib import Path
import signal, subprocess, sys, time
p=argparse.ArgumentParser();p.add_argument('--job');a=p.parse_args()
job=Path(a.job);d=json.loads(job.read_text());i=d['identity'];mode=d.get('mode','complete')
def cancelled(signum,frame):
    Path(d['result']+'.cancelled').write_text(str(os.getpid()))
    raise SystemExit(128+signum)
signal.signal(signal.SIGTERM,cancelled)
directory=Path(d['directory']);directory.mkdir()
if mode=='hold-child':
    child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])
    Path(d['marker']).write_text(json.dumps([os.getpid(),child.pid]))
    while True: time.sleep(.01)
if mode=='fail-after-child':
    while not Path(d['marker']).exists(): time.sleep(.01)
    raise SystemExit(7)
if mode=='wait':
    while not Path(d['marker']).exists(): time.sleep(.01)
    time.sleep(.1)
artifacts={}
for name in ('episode.json.gz','states.json.gz','best.json.gz'):
    f=directory/name;f.write_bytes(b'retained');artifacts[name]=hashlib.sha256(f.read_bytes()).hexdigest()
row=dict(case=i['case'],bank=i['seed'],directory=d['directory'],selected=d['horizon'],budget_complete=mode!='incomplete')
identity=dict(i)
if mode=='wrong-identity': identity['seed']+=1
result=dict(status='complete',identity=identity,input_sha256=hashlib.sha256(job.read_bytes()).hexdigest(),
            policy_sha256=i['policy_sha256'],row=row,artifacts_sha256=artifacts)
Path(d['result']).write_text(json.dumps(result))
if mode=='release': Path(d['marker']).write_text('ready')
'''


class CollectorProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.stub = self.directory / 'worker.py'
        self.stub.write_text(STUB)

    def jobs(self, modes):
        jobs = []
        for attempt, mode in enumerate(modes):
            stem = self.directory / f'worker-{attempt}'
            identity = dict(group=3, case='case', attempt=attempt, seed=100+attempt, policy_sha256='a'*64)
            data = dict(identity=identity, mode=mode, result=str(stem.with_suffix('.json')),
                        directory=str(self.directory / f'episode-{attempt}'), episode_seconds=5,
                        horizon=4, marker=str(self.directory / 'marker'))
            jobs.append(dict(identity=identity, input=data, path=stem.with_suffix('.input.json'),
                             result=Path(data['result']), log=stem.with_suffix('.log'), horizon=4))
        return jobs

    def run_jobs(self, jobs, workers=2, seconds=10, rss_limit_bytes=2**63):
        with ProcessTreeMemory(interval=.05) as memory:
            result = _run_jobs(jobs, workers, time.monotonic()+seconds,
                               command=[sys.executable, str(self.stub)], memory=memory,
                               rss_limit_bytes=rss_limit_bytes)
        self.assertFalse(memory.errors)
        self.assertGreater(memory.peak_bytes, 0)
        return result

    def test_order_is_attempt_order_even_when_completion_order_differs(self):
        results, order = self.run_jobs(self.jobs(['wait', 'release', 'complete', 'complete']))
        self.assertEqual([r['identity']['attempt'] for r in results], [0, 1, 2, 3])
        self.assertLess(order.index(1), order.index(0))

    def test_wrong_identity_and_incomplete_episode_cannot_return_group(self):
        for mode, code in [('wrong-identity', 'on-policy.worker-identity'),
                           ('incomplete', 'on-policy.group-incomplete')]:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as sub:
                previous = self.directory
                self.directory = Path(sub)
                try:
                    with self.assertRaises(CollectionError) as caught:
                        self.run_jobs(self.jobs([mode]))
                    self.assertEqual(caught.exception.diagnostic['code'], code)
                finally:
                    self.directory = previous

    def test_worker_failure_reaps_owned_runtime_child_and_retains_evidence(self):
        jobs = self.jobs(['hold-child', 'fail-after-child'])
        with self.assertRaises(CollectionError) as caught:
            self.run_jobs(jobs)
        self.assertEqual(caught.exception.diagnostic['code'], 'on-policy.worker-failed')
        identities = json.loads((self.directory/'marker').read_text())
        current = {pid for pid, _, _, _ in processes()}
        self.assertFalse(set(identities) & current)
        self.assertTrue(Path(str(jobs[0]['result'])+'.cancelled').exists())
        self.assertTrue(jobs[0]['path'].exists())

    def test_deadline_reaps_owned_runtime_child(self):
        jobs = self.jobs(['hold-child'])
        with self.assertRaises(CollectionError) as caught:
            self.run_jobs(jobs, seconds=.3)
        self.assertEqual(caught.exception.diagnostic['code'], 'on-policy.parallel-resource-bound')
        identities = json.loads((self.directory/'marker').read_text())
        self.assertFalse(set(identities) & {pid for pid, _, _, _ in processes()})

    def test_group_signal_denial_uses_checked_owned_pid_cleanup(self):
        jobs = self.jobs(['hold-child', 'fail-after-child'])
        with patch('full_factory_on_policy_collection.os.killpg', side_effect=PermissionError):
            with self.assertRaises(CollectionError) as caught:
                self.run_jobs(jobs)
        self.assertEqual(caught.exception.diagnostic['code'], 'on-policy.worker-failed')
        identities = json.loads((self.directory/'marker').read_text())
        self.assertFalse(set(identities) & {pid for pid, _, _, _ in processes()})

    def test_sampled_rss_guard_stops_before_dispatch(self):
        with self.assertRaises(CollectionError) as caught:
            self.run_jobs(self.jobs(['complete']), rss_limit_bytes=1)
        self.assertEqual(caught.exception.diagnostic['code'], 'on-policy.parallel-resource-bound')
        self.assertFalse((self.directory/'episode-0').exists())

    def test_missing_memory_measurement_stops_before_dispatch(self):
        jobs = self.jobs(['complete'])
        memory = ProcessTreeMemory()
        memory.errors.append('measurement unavailable')
        with self.assertRaises(CollectionError) as caught:
            _run_jobs(jobs,1,time.monotonic()+10,command=[sys.executable,str(self.stub)],
                      memory=memory,rss_limit_bytes=2**63)
        self.assertEqual(caught.exception.diagnostic['code'],'on-policy.collection-memory-measurement')
        self.assertFalse((self.directory/'episode-0').exists())

    def test_semantic_identity_keeps_acceptance_and_probability_changes(self):
        from full_factory_on_policy_collection_study import semantic_episode
        episode = dict(elapsed_seconds=1,observation_seconds=1,selection_seconds=1,recording_seconds=1,
                       config=dict(episode_seconds=30),trace=[dict(action=dict(kind='pose'),old_log_probability=-.2,
                       acceptance=dict(candidate_validated=True,elapsed_ms=1,validation_ms=1))])
        def save():
            for name,value in [('episode',episode),('states',[]),('best',{})]:
                with gzip.open(self.directory/f'{name}.json.gz','wt') as stream:json.dump(value,stream)
            return semantic_episode(self.directory)
        expected = save()
        episode['trace'][0]['acceptance']['validation_ms'] = 9
        self.assertEqual(expected,save())
        episode['trace'][0]['acceptance']['candidate_validated'] = False
        self.assertNotEqual(expected,save())
        episode['trace'][0]['acceptance']['candidate_validated'] = True
        episode['trace'][0]['old_log_probability'] = -.3
        self.assertNotEqual(expected,save())

    def test_duplicate_attempts_rejected_before_dispatch(self):
        protocol = self.directory/'protocol.json'
        write(protocol, dict(config=dict(group_episodes=4), cases=[dict(id='case')]))
        attempts = [dict(attempt=a, seed=100+a) for a in [0, 1, 1, 3]]
        with self.assertRaises(CollectionError) as caught:
            collect_group(protocol, self.directory/'unused.pt', 'case', attempts, self.directory/'out',
                          group=0, workers=2, group_seconds=10, policy_sha256='a'*64, rss_limit_bytes=2**30)
        self.assertEqual(caught.exception.diagnostic['code'], 'on-policy.worker-identity')
        self.assertFalse((self.directory/'out').exists())


class NativeCollectorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import test_full_factory_policy as fixtures
        fixtures.FactoryPolicyTests.setUpClass.__func__(cls)

    @classmethod
    def tearDownClass(cls):
        import test_full_factory_policy as fixtures
        fixtures.FactoryPolicyTests.tearDownClass.__func__(cls)

    def setup_protocol(self, directory, **overrides):
        import torch
        from full_factory_environment import ROOT
        from full_factory_on_policy import parameter_hash
        from full_factory_policy import FullFactoryPolicy, SCHEMA
        config = read(Path(__file__).with_name('full-factory-on-policy-study.json'))
        config.update(group_episodes=4, horizon=4, episode_seconds=30, **overrides)
        case = dict(self.case, instances=len(self.witness['placements']), family='test',
                    split='train', transfer_group='mechanical-test')
        protocol = dict(config=config, depth=2, width=32, cases=[case], sources={}, source_sha256={},
                        catalogs=str(Path(self.temp.name)/'catalogs.json'),
                        native_library=os.environ.get('AIC_NATIVE_LIBRARY',str(ROOT/'target/release/libaic_pilot_native.dylib')))
        write(directory/'protocol.json', protocol)
        torch.manual_seed(17)
        model = FullFactoryPolicy(2,32)
        optimizer = torch.optim.Adam(model.parameters(), lr=config['learning_rate'])
        checkpoint = dict(schema=SCHEMA, depth=2, width=32, next_group=0, policy=model.state_dict(),
                          optimizer=optimizer.state_dict(), rng=torch.get_rng_state(), policy_sha256=parameter_hash(model))
        torch.save(checkpoint, directory/'before.pt')
        return protocol, case, checkpoint

    def test_native_1_2_4_workers_preserve_actions_states_and_optimizer(self):
        from full_factory_on_policy import on_policy_rollout
        from full_factory_on_policy_collection_study import load_policy_optimizer, semantic_episode, tree_hash, update_group
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            protocol, case, checkpoint = self.setup_protocol(directory)
            model, _, _ = load_policy_optimizer(protocol, directory/'before.pt')
            attempts = [dict(attempt=i, seed=seed) for i,seed in enumerate([13,17,31,47])]
            original = [on_policy_rollout(protocol,case,model,a['seed'],directory/f'serial-{a["attempt"]}')[0] for a in attempts]
            semantics = [semantic_episode(r['directory']) for r in original]
            expected_returns, _, expected_after = update_group(protocol,case,directory/'before.pt',original,lambda:None)
            for workers in (1,2,4):
                rows, record = collect_group(directory/'protocol.json',directory/'before.pt',case['id'],attempts,
                    directory/f'workers-{workers}',group=0,workers=workers,group_seconds=60,
                    policy_sha256=checkpoint['policy_sha256'],rss_limit_bytes=12*1024**3)
                self.assertEqual(record['optimizer_updates'],0)
                self.assertEqual(semantics,[semantic_episode(r['directory']) for r in rows])
                returns, gradient, after = update_group(protocol,case,directory/'before.pt',rows,lambda:None)
                self.assertEqual(returns,expected_returns)
                self.assertEqual(gradient['max_log_probability_error'],0)
                self.assertEqual(tree_hash(after),tree_hash(expected_after))

    def test_native_worker_failure_preserves_incumbent_and_checkpoint(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            protocol, case, checkpoint = self.setup_protocol(directory,episode_state_bytes=1)
            before = sha(directory/'before.pt')
            attempts = [dict(attempt=i,seed=i+100) for i in range(4)]
            with self.assertRaises(CollectionError):
                collect_group(directory/'protocol.json',directory/'before.pt',case['id'],attempts,
                              directory/'failed',group=0,workers=2,group_seconds=30,
                              policy_sha256=checkpoint['policy_sha256'],rss_limit_bytes=12*1024**3)
            result = read(directory/'failed/partial.json')
            self.assertEqual(result['optimizer_updates'],0)
            self.assertEqual(sha(directory/'before.pt'),before)
            self.assertTrue(list((directory/'failed').glob('episode-*/best.json.gz')))
            self.assertFalse(set(result['owned_process_groups']) & {pgid for _,_,pgid,_ in processes()})

    def test_incomplete_group_rejected_before_loading_optimizer(self):
        from full_factory_on_policy_collection_study import update_group
        with patch('full_factory_on_policy_collection_study.load_policy_optimizer') as load:
            with self.assertRaises(CollectionError):
                update_group(dict(config=dict(horizon=4)),{},Path('unused'),[dict(budget_complete=False,selected=3)]*4,lambda:None)
            load.assert_not_called()


if __name__ == '__main__':
    unittest.main()
