"""Stateful continuation and narrowly scoped constructor-artifact cleanup."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest
import subprocess
import sys

import torch

from continue_fresh_factory_learning import restore, cleanup_construction
from random_factory_audit import optimizer_hash
from full_factory_on_policy import parameter_hash
from random_factory_pilot import new_model, checkpoint
from fresh_factory_process_supervisor import capture_tree, stop_tree


class ContinuationTests(unittest.TestCase):
    def test_restored_adam_and_rng_preserve_the_next_update(self):
        spec = dict(policy=dict(depth=2, width=32), learning=dict(learning_rate=.0003))
        model = new_model(spec, 17)
        optimizer = torch.optim.Adam(model.parameters(), lr=.0003)
        generator = torch.Generator().manual_seed(123)
        def step(target, opt, gradients):
            for parameter, gradient in zip(target.parameters(), gradients):
                parameter.grad = gradient.clone()
            opt.step()
        for _ in range(3):
            step(model, optimizer, [torch.randn(p.shape, generator=generator) for p in model.parameters()])
        rng = torch.get_rng_state().clone()
        with tempfile.TemporaryDirectory() as directory:
            manifest = checkpoint(Path(directory) / 'state.pt', model, optimizer, seed=17,
                                  next_group=32, next_attempt=1024)
            restored, restored_optimizer, saved = restore(spec, manifest)
            self.assertEqual(parameter_hash(model), parameter_hash(restored))
            self.assertEqual(optimizer_hash(optimizer), optimizer_hash(restored_optimizer))
            self.assertTrue(torch.equal(torch.get_rng_state(), rng))
            self.assertEqual((saved['next_group'], saved['next_attempt']), (32, 1024))
            reset_model = deepcopy(model)
            reset_optimizer = torch.optim.Adam(reset_model.parameters(), lr=.0003)
            gradients = [torch.randn(p.shape, generator=generator) for p in model.parameters()]
            for target, opt in [(model, optimizer), (restored, restored_optimizer), (reset_model, reset_optimizer)]:
                step(target, opt, gradients)
            self.assertEqual(parameter_hash(model), parameter_hash(restored))
            self.assertEqual(optimizer_hash(optimizer), optimizer_hash(restored_optimizer))
            self.assertNotEqual(parameter_hash(model), parameter_hash(reset_model))

    def test_cleanup_keeps_inputs_roots_and_failed_construction_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            results = []
            retained = []
            for index, status in enumerate(['complete', 'generation-rejected']):
                admission = root / str(index) / 'admission'
                location = admission / 'factory'
                location.mkdir(parents=True)
                for name in ['construction.html', 'construction.json', 'root.json']:
                    (location / name).write_text('{}')
                (admission / 'roots.json').write_text('{}')
                case = {}
                for name in ['request', 'initial', 'original']:
                    path = root / str(index) / f'{name}.json'
                    path.write_text('{}')
                    case[name] = str(path)
                    retained.append(path)
                retained += [location / 'root.json', admission / 'roots.json']
                if status == 'generation-rejected':
                    retained.append(location / 'construction.json')
                results.append(dict(status=status, generation=dict(case=case,
                    original_attempt=dict(directory=str(location)))))
            result = cleanup_construction(results, root)
            self.assertEqual(result['files'], 3)
            self.assertTrue(all(path.exists() for path in retained))
            self.assertTrue((root / 'retention.json').exists())

    def test_cleanup_rejects_paths_outside_its_batch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            batch, unrelated = root / 'batch', root / 'unrelated'
            batch.mkdir()
            unrelated.mkdir()
            protected = unrelated / 'construction.html'
            protected.write_text('preserve')
            result = dict(status='generation-rejected', generation=dict(original_attempt=dict(directory=str(unrelated))))
            with self.assertRaisesRegex(ValueError, 'cleanup owned'):
                cleanup_construction([result], batch)
            self.assertEqual(protected.read_text(), 'preserve')

    @unittest.skipUnless(Path('/proc/self/stat').exists(), 'Linux server process supervisor')
    def test_supervisor_stops_detached_workers_and_preserves_unrelated_process(self):
        program = ('import signal,subprocess,sys,time; '
                   'signal.signal(signal.SIGTERM,signal.SIG_IGN); '
                   'p=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"],start_new_session=True); '
                   'print(p.pid,flush=True); time.sleep(60)')
        child = subprocess.Popen([sys.executable, '-c', program], stdout=subprocess.PIPE,
                                 text=True, start_new_session=True)
        unrelated = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'], start_new_session=True)
        identities = {}
        try:
            worker_pid = int(child.stdout.readline())
            capture_tree(child.pid, identities)
            self.assertIn(worker_pid, identities)
            self.assertNotIn(unrelated.pid, identities)
            stop_tree(child, identities, grace=.05)
            self.assertIsNotNone(child.poll())
            self.assertIsNone(unrelated.poll())
        finally:
            stop_tree(child, identities, grace=.05)
            child.stdout.close()
            unrelated.terminate()
            unrelated.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
