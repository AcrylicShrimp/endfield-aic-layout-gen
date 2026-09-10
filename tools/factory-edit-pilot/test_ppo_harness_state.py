"""Fault tests for durable PPO state, immutable schedules and secret-free config."""
from copy import deepcopy
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import torch

from ppo_harness_environment import read_environment, load_environment
from ppo_harness_protocol import validate, phases
from ppo_harness_schedule import phase_tasks
from ppo_harness_state import (SCHEMA, HarnessError, write, read, sha, manifest, writer_lock, check_sources,
                               save_checkpoint, restore_checkpoint, commit_phase)


def config():
    result = read(Path(__file__).with_name('ppo-harness-readiness.json'))
    result['execution']['device'] = 'cpu'
    result['tracking'] = dict(backend='none', mode='offline', entity='', project='readiness')
    return result


class StateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()

    def tearDown(self):
        self.temporary.cleanup()

    def test_partial_write_and_failed_replace_preserve_old_commit(self):
        path = self.root / 'state.json'
        original = dict(epoch=1, checkpoint='old')
        write(path, original)
        with patch('ppo_harness_state.os.replace', side_effect=OSError('disk fault')):
            with self.assertRaises(OSError):
                write(path, dict(epoch=2, checkpoint='new'))
        self.assertEqual(read(path), original)
        self.assertEqual(list(self.root.glob('*.tmp')), [])

    def test_orphan_checkpoint_and_record_do_not_advance_state(self):
        state = dict(jobs=[dict(checkpoint=dict(path='old.pt'), records=[], next_phase=0)])
        write(self.root / 'state.json', state)
        write(self.root / 'new.pt', {'weights': 2})
        write(self.root / 'record.json', {'phase': 1})
        record = manifest(self.root, self.root / 'record.json')
        checkpoint = manifest(self.root, self.root / 'new.pt')
        with patch('ppo_harness_state.os.replace', side_effect=OSError('crash before commit')):
            with self.assertRaises(OSError):
                commit_phase(self.root, deepcopy(state), 0, checkpoint, record)
        self.assertEqual(read(self.root / 'state.json')['jobs'][0]['next_phase'], 0)
        commit_phase(self.root, deepcopy(state), 0, checkpoint, record)
        durable = read(self.root / 'state.json')['jobs'][0]
        self.assertEqual(durable['next_phase'], 1)
        self.assertEqual(durable['checkpoint'], checkpoint)
        self.assertEqual(durable['records'], [record])

    def test_single_writer_lock_excludes_a_second_owner(self):
        with writer_lock(self.root):
            with self.assertRaisesRegex(HarnessError, 'writer-active'):
                with writer_lock(self.root):
                    self.fail('second lock acquired')
        with writer_lock(self.root):
            pass

    def models(self):
        torch.manual_seed(17)
        actor, critic = torch.nn.Linear(3, 2), torch.nn.Linear(3, 1)
        aopt, copt = (torch.optim.Adam(m.parameters(), lr=.01) for m in (actor, critic))
        return actor, critic, aopt, copt, torch.Generator().manual_seed(29)

    def step(self, model_set):
        actor, critic, aopt, copt, rng = model_set
        x = torch.randn(4, 3)[torch.randperm(4, generator=rng)]
        for model, optimizer in ((actor, aopt), (critic, copt)):
            optimizer.zero_grad()
            model(x).square().sum().backward()
            optimizer.step()

    def test_nonempty_adam_and_both_rngs_resume_exactly(self):
        first = self.models()
        self.step(first)
        record = save_checkpoint(self.root, self.root / 'checkpoint.pt', first, 'protocol', 'job', 1)
        self.step(first)
        expected_rng = torch.get_rng_state().clone()
        second = self.models()
        saved = restore_checkpoint(self.root, record, second, 'protocol', 'job', 1)
        self.assertTrue(saved['actor_optimizer']['state'])
        self.assertTrue(saved['critic_optimizer']['state'])
        self.step(second)
        from random_factory_audit import optimizer_hash
        for a, b in zip(first[:2], second[:2]):
            for actual, expected in zip(a.parameters(), b.parameters()):
                self.assertTrue(torch.equal(actual, expected))
        self.assertEqual(optimizer_hash(first[2]), optimizer_hash(second[2]))
        self.assertEqual(optimizer_hash(first[3]), optimizer_hash(second[3]))
        self.assertTrue(torch.equal(first[4].get_state(), second[4].get_state()))
        self.assertTrue(torch.equal(expected_rng, torch.get_rng_state()))

    def test_corrupt_checkpoint_and_source_are_rejected_before_loading(self):
        first = self.models()
        record = save_checkpoint(self.root, self.root / 'checkpoint.pt', first, 'protocol', 'job', 0)
        with open(self.root / 'checkpoint.pt', 'ab') as stream:
            stream.write(b'corruption')
        with patch.object(first[0], 'load_state_dict', side_effect=AssertionError('must not load')):
            with self.assertRaisesRegex(HarnessError, 'artifact-identity'):
                restore_checkpoint(self.root, record, first, 'protocol', 'job', 0)
        path = self.root / 'source.py'
        path.write_text('one')
        protocol = dict(schema=SCHEMA, backend='retained-full-factory-native', sources={str(path): sha(path)})
        path.write_text('two')
        with self.assertRaisesRegex(HarnessError, 'source-drift'):
            check_sources(protocol)

    def test_checkpoint_path_is_checked_before_writing(self):
        path = self.root.parent / (self.root.name + '-outside.pt')
        with self.assertRaisesRegex(HarnessError, 'foreign-path'):
            save_checkpoint(self.root, path, self.models(), 'protocol', 'job', 0)
        self.assertFalse(path.exists())

    def test_invalid_configuration_and_held_out_overlap_are_rejected(self):
        good = config()
        validate(good)
        variants = []
        for name, value in [('workers', 0), ('microbatch', 65), ('device', 'automatic')]:
            bad = deepcopy(good)
            bad['execution'][name] = value
            variants.append(bad)
        bad = deepcopy(good)
        bad['evaluation']['generation_seed_base'] = bad['training']['generation_seed_base']
        variants.append(bad)
        bad = deepcopy(good)
        bad['candidates'] = ['action-only', 'action-only']
        variants.append(bad)
        bad = deepcopy(good)
        bad['surprise'] = True
        variants.append(bad)
        for bad in variants:
            with self.subTest(config=bad):
                with self.assertRaises(HarnessError):
                    validate(bad)

    def test_schedule_is_fresh_in_training_and_paired_across_candidates(self):
        cfg = config()
        protocol = dict(config=cfg)
        version = dict(path='checkpoint.pt', sha256='file', policy_sha256='actor', critic_sha256='critic')
        jobs = [dict(id=c, candidate=c, seed=17, checkpoint=version) for c in cfg['candidates']]
        schedules = {}
        for job in jobs:
            rows = []
            for phase in phases(cfg):
                _, tasks, _, _ = phase_tasks(self.root, protocol, 'protocol', job, phase)
                rows.append((phase, [read(path)['generation_seed'] for _, path in tasks]))
            schedules[job['id']] = rows
        self.assertEqual(*schedules.values())
        rows = next(iter(schedules.values()))
        training = [s for p, seeds in rows if p['kind'] == 'training' for s in seeds]
        evaluation = [seeds for p, seeds in rows if p['kind'] == 'evaluation']
        self.assertEqual(len(training), len(set(training)))
        self.assertTrue(all(seeds == evaluation[0] for seeds in evaluation))
        self.assertFalse(set(training) & set(evaluation[0]))

    def test_interrupted_attempts_are_preserved_and_a_published_result_is_reused(self):
        cfg = config()
        job = dict(id='job', candidate='history-no-restore', seed=17,
                   checkpoint=dict(path='checkpoint.pt', sha256='file', policy_sha256='actor', critic_sha256='critic'))
        phase = dict(kind='training', batch=1)
        directory, tasks, _, _ = phase_tasks(self.root, dict(config=cfg), 'protocol', job, phase)
        index, path = tasks[0]
        (path.parent / 'irreplaceable-partial.pt').write_bytes(b'actual interrupted state')
        task = read(tasks[1][1])
        result_path = tasks[1][1].with_name('result.json')
        write(result_path, dict(schema=SCHEMA, status='generation-rejected', input_sha256=sha(tasks[1][1]),
                               **{k: task[k] for k in ('identity', 'candidate', 'protocol_sha256', 'policy_sha256', 'critic_sha256')}))
        write(result_path.with_name('result-ready.json'), dict(kind='result', path=str(result_path),
              sha256=sha(result_path), task=str(tasks[1][1])))
        _, retried, cached, attempts = phase_tasks(self.root, dict(config=cfg), 'protocol', job, phase)
        self.assertIn(1, cached)
        self.assertNotIn(1, [i for i, _ in retried])
        self.assertEqual((path.parent / 'irreplaceable-partial.pt').read_bytes(), b'actual interrupted state')
        self.assertEqual(retried[0][1].parent.name, 'attempt-0001')
        self.assertEqual(attempts[0]['interrupted_attempts_retained'], 1)

    def test_concurrent_prepare_cannot_overwrite_claimed_root(self):
        import ppo_harness_protocol as protocol
        from ppo_harness_state import mkdir
        root = self.root / 'run'
        path = self.root / 'config.json'
        write(path, config())
        spec = read(Path(__file__).with_name('random-factory-server-pilot.json'))
        def competing_claim(parent):
            mkdir(parent)
            root.mkdir()
            write(root / 'protocol.json', dict(owner='other-prepare'))
        with patch.object(protocol, 'mkdir', side_effect=competing_claim), patch.object(protocol, 'resolve_spec', return_value=spec), patch.object(protocol, 'sources', return_value=set()), patch.object(protocol, 'sha', return_value='hash'):
            with self.assertRaisesRegex(HarnessError, 'existing-run'):
                protocol.prepare(path, root)
        self.assertEqual(read(root / 'protocol.json'), dict(owner='other-prepare'))
        self.assertFalse((root / 'state.json').exists())

    def test_env_is_not_a_shell_and_environment_takes_precedence(self):
        path = self.root / '.env'
        path.write_text('WANDB_API_KEY="$(touch unwanted)"\nWANDB_ENTITY=team\nWANDB_PROJECT="project-name" # comment\n')
        self.assertEqual(read_environment(path)['WANDB_API_KEY'], '$(touch unwanted)')
        with patch.dict(os.environ, {'WANDB_API_KEY': 'existing'}):
            load_environment(path)
            self.assertEqual(os.environ['WANDB_API_KEY'], 'existing')
        path.write_text('WANDB_API_KEY=very-secret\nUNRELATED=unexpected\n')
        with self.assertRaises(HarnessError) as caught:
            read_environment(path)
        self.assertNotIn('very-secret', str(caught.exception))


if __name__ == '__main__':
    unittest.main()
