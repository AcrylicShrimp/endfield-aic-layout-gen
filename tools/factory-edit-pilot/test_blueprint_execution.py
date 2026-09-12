"""Real worker continuity, shared-array isolation and packed CUDA gradients."""
from copy import deepcopy
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest

import torch

from blueprint_edit_input import prepare
from blueprint_execution_pool import BlueprintPool
from blueprint_ppo import readiness
from blueprint_ppo_batch import distribution, pack, to_device
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_store import ROOT, read, reference, write
from blueprint_ppo_update import digest, learners, predictions


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires selected native library')
class DeviceTests(unittest.TestCase):
    @unittest.skipUnless(torch.cuda.is_available(), 'requires CUDA')
    def test_cuda_packed_probabilities_and_shared_encoder_gradients(self):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        config = readiness(read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo.json'))
        runtime = Runtime(config, os.environ['AIC_NATIVE_LIBRARY'])
        inputs = [prepare(Stream(runtime, i).observe()) for i in range(4)]
        model, _, _ = learners(17, config['ppo'])
        gpu = deepcopy(model).cuda()
        cpu_batch = pack(inputs)
        cpu_logits, cpu_value = model.forward_packed(cpu_batch)
        cpu_logp, cpu_entropy = distribution(cpu_logits, cpu_batch)
        gpu_batch = to_device(cpu_batch, 'cuda')
        gpu_logits, gpu_value = gpu.forward_packed(gpu_batch)
        gpu_logp, gpu_entropy = distribution(gpu_logits, gpu_batch)
        for actual, expected in [(gpu_logits, cpu_logits), (gpu_value, cpu_value),
                                 (gpu_logp, cpu_logp), (gpu_entropy, cpu_entropy)]:
            torch.testing.assert_close(actual.cpu(), expected, rtol=1e-4, atol=2e-5)
        cpu_value.square().mean().backward()
        gpu_value.square().mean().backward()
        self.assertGreater(float(model.nodes[0].weight.grad.abs().sum()), 0.)
        for cpu, device in zip(model.parameters(), gpu.parameters(), strict=True):
            self.assertEqual(cpu.grad is None, device.grad is None)
            if cpu.grad is not None:
                torch.testing.assert_close(device.grad.cpu(), cpu.grad, rtol=1e-4, atol=2e-5)


@unittest.skipUnless(sys.platform == 'linux' and os.environ.get('AIC_NATIVE_LIBRARY'),
                     'requires Linux and selected native library')
class PoolTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get('AIC_RANDOM_INPUT_TESTS'), 'requires explicit constructor tests')
    def test_parallel_fresh_inputs_publish_separate_slots_and_restore_same_payloads(self):
        torch.set_num_threads(1)
        cores = sorted(os.sched_getaffinity(0))[:2]
        if len(cores) < 2:
            self.skipTest('two allocated CPUs required')
        config = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-64-12h.json')
        config['training'].update(streams=2, chunk_actions=4, episode_actions=8)
        model, _, _ = learners(17, config['ppo'])
        sampler = torch.Generator().manual_seed(1000017)
        deadline = time.monotonic() + 90
        def guard():
            if time.monotonic() > deadline:
                raise TimeoutError('fresh worker test deadline')
        with tempfile.TemporaryDirectory(prefix='blueprint-fresh-pool-') as temporary:
            root = Path(temporary)
            config['training_input']['directory'] = str(root / 'inputs')
            with BlueprintPool(config, os.environ['AIC_NATIVE_LIBRARY'], cores, root / 'first', guard) as pool:
                pool.collect(model, sampler)
                saved = pool.snapshot()
            identities = [s['metadata']['training_input'] for s in saved]
            self.assertEqual([i['identity']['slot'] for i in identities], [0, 1])
            self.assertNotEqual(identities[0]['input_sha256'], identities[1]['input_sha256'])
            with BlueprintPool(config, os.environ['AIC_NATIVE_LIBRARY'], cores, root / 'restore', guard, saved) as pool:
                self.assertEqual([s['metadata']['training_input'] for s in pool.snapshot()], identities)
                second = pool.collect(model, sampler)
                self.assertTrue(all(c['result']['factory_validated'] for c in second['completed']))
                pool.collect(model, sampler)
                self.assertEqual([s['metadata']['training_input']['identity']['slot'] for s in pool.snapshot()], [2, 3])
            self.assertEqual(len(list((root / 'inputs').glob('episode-*/episode.json'))), 4)

    def test_eight_workers_preserve_drafts_and_true_terminal(self):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cores = sorted(os.sched_getaffinity(0))[:8]
        if len(cores) < 8:
            self.skipTest('eight distinct allocated CPUs required')
        config = readiness(read(ROOT / 'tools/factory-edit-pilot/blueprint-ppo.json'))
        config['training']['streams'] = 8
        model, _, _ = learners(17, config['ppo'])
        sampler = torch.Generator().manual_seed(1000017)
        deadline = time.monotonic() + 90
        def guard():
            if time.monotonic() > deadline:
                raise TimeoutError('worker test deadline')
        with tempfile.TemporaryDirectory(prefix='blueprint-pool-') as temporary:
            with BlueprintPool(config, os.environ['AIC_NATIVE_LIBRARY'], cores, Path(temporary) / 'first', guard) as pool:
                identities = pool.identities
                first = pool.collect(model, sampler)
                saved = pool.snapshot()
                self.assertEqual(len({r['pid'] for r in identities}), 8)
                self.assertEqual(len(first['samples']), 32)
                self.assertEqual([s['stream'] for s in first['samples']], list(range(8)) * 4)
                self.assertTrue(all(not f['terminal'] for f in first['frontiers']))
                self.assertTrue(all(s['policy_actions'] == 4 and s['native'] is not None for s in saved))
                # Shared mappings survive subsequent messages and pool shutdown.
                before = digest(first['samples'])
                pool.snapshot()
                self.assertEqual(before, digest(first['samples']))
                with torch.no_grad():
                    actual, _, values = predictions(model, first['samples'])
                torch.testing.assert_close(actual, torch.tensor([s['old_logp'] for s in first['samples']]), rtol=1e-5, atol=2e-6)
                torch.testing.assert_close(values, torch.tensor([s['old_value'] for s in first['samples']]), rtol=1e-5, atol=2e-6)
                runtime = Runtime(config, os.environ['AIC_NATIVE_LIBRARY'])
                directory = Path(temporary) / 'evaluation-input'
                write(directory / 'initial.json', runtime.original)
                write(directory / 'request.json', runtime.request)
                files = {k: reference(directory / f'{k}.json', directory) for k in ('initial', 'request')}
                cases = [dict(directory=str(directory), files=files, generation_seed=i, action_seed=500 + i,
                    initial_metrics=first['samples'][0]['observation']['initial_metrics']) for i in range(8)]
                sampling_before = digest(sampler.get_state())
                rows = pool.evaluate(model, cases, 17, 0, Path(temporary) / 'evaluation', deadline,
                    dict(max_actions=4, elapsed_ms=2000, action_milestones=[4]))
                self.assertEqual([r['generation_seed'] for r in rows], list(range(8)))
                self.assertTrue(all(r['policy_actions'] == 4 for r in rows))
                self.assertEqual(sampling_before, digest(sampler.get_state()))
                after_evaluation = pool.snapshot()
                self.assertEqual([s['expected'] for s in saved], [s['expected'] for s in after_evaluation])
                self.assertEqual([s['policy_actions'] for s in saved], [s['policy_actions'] for s in after_evaluation])
            self.assertEqual(before, digest(first['samples']))
            with BlueprintPool(config, os.environ['AIC_NATIVE_LIBRARY'], cores, Path(temporary) / 'restored', guard, saved) as pool:
                restored = pool.snapshot()
                self.assertEqual([s['expected'] for s in restored], [f['semantic'] for f in first['frontiers']])
                second = pool.collect(model, sampler)
                self.assertEqual(len(second['completed']), 8)
                self.assertTrue(all(f['terminal'] and f['bootstrap'] == 0 for f in second['frontiers']))
                self.assertTrue(all(c['result']['factory_validated'] for c in second['completed']))
                self.assertTrue(all(s['native'] is None and s['episode'] == 1 for s in pool.snapshot()))
                pool.workers[0].kill()
                pool.workers[0].wait()
                with self.assertRaises((ValueError, RuntimeError, BrokenPipeError)):
                    pool.snapshot()


if __name__ == '__main__':
    unittest.main()
