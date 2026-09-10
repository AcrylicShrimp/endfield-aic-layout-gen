"""Compare input delivery and exact CUDA updates against an explicit frozen runtime."""
import argparse
from collections import defaultdict
import gc
import importlib.util
import json
import os
from pathlib import Path
import resource
import statistics
import time
from unittest.mock import patch

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')

import torch

from ppo_area_update import load_experience
from ppo_harness_run import learners
from ppo_harness_state import read, sha
from ppo_training_batch import prepare, distribution, critic_values
from ppo_training_transfer import map_tensors
import ppo_training_update as current

EXACT_KEYS = ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
              'critic_optimizer_sha256', 'shuffle_rng_sha256', 'epochs')


def asynchronous(value, device):
    return map_tensors(value, lambda tensor: tensor.to(device, non_blocking=True))


def coalesced(value, device):
    device = torch.device(device)
    if device.type != 'cuda':
        return map_tensors(value, lambda tensor: tensor.to(device))
    leaves = []
    map_tensors(value, lambda tensor: leaves.append(tensor))
    groups = defaultdict(list)
    for index, tensor in enumerate(leaves):
        if tensor.device.type != 'cpu' or tensor.requires_grad:
            raise ValueError('ppo-training.transfer: expected immutable CPU preparation tensors')
        groups[tensor.dtype].append((index, tensor))
    moved = [None] * len(leaves)
    for dtype, group in groups.items():
        size = sum(t.numel() for _, t in group)
        host = torch.empty(size, dtype=dtype, pin_memory=True)
        torch.cat([t.reshape(-1) for _, t in group], out=host)
        # CUDA's pinned allocator tracks outstanding copies; views retain the
        # destination storage. Subsequent model work uses the same CUDA stream.
        target = host.to(device, non_blocking=True)
        offset = 0
        for index, tensor in group:
            count = tensor.numel()
            moved[index] = target[offset:offset + count].view(tensor.shape)
            offset += count
    ordered = iter(moved)
    return map_tensors(value, lambda _: next(ordered))


def main(args):
    if args.output.exists():
        raise ValueError(f'fresh output required: {args.output}')
    args.output.mkdir(parents=True)
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    torch.backends.cuda.matmul.fp32_precision = 'ieee'
    torch.backends.cudnn.conv.fp32_precision = 'ieee'
    torch.backends.cudnn.benchmark = False
    root = args.run.resolve()
    protocol, state = read(root / 'protocol.json'), read(root / 'state.json')
    if len(state['jobs']) != 1:
        raise ValueError('data-supply benchmark requires exactly one job')
    reference = next(r for r in state['jobs'][0]['records']
                     if r['phase'] == dict(kind='training', batch=args.batch))
    if sha(root / reference['path']) != reference['sha256']:
        raise ValueError(f'record identity differs: {root / reference["path"]}')
    record = read(root / reference['path'])
    checkpoint = root / record['behavior_checkpoint']['path']
    if sha(checkpoint) != record['behavior_checkpoint']['sha256']:
        raise ValueError(f'checkpoint identity differs: {checkpoint}')
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    if not saved['actor_optimizer']['state'] or not saved['critic_optimizer']['state']:
        raise ValueError('benchmark requires a nonempty Adam checkpoint')
    baseline_path = args.baseline_runtime.resolve() / 'tools/factory-edit-pilot/ppo_training_update.py'
    spec = importlib.util.spec_from_file_location('ppo_frozen_data_supply_baseline', baseline_path)
    baseline = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(baseline)
    baseline_batch_path = baseline_path.with_name('ppo_training_batch.py')
    batch_spec = importlib.util.spec_from_file_location('ppo_frozen_batch_baseline', baseline_batch_path)
    baseline_batch = importlib.util.module_from_spec(batch_spec)
    batch_spec.loader.exec_module(baseline_batch)
    if sha(baseline_path.with_name('ppo_training_world.py')) != sha(Path(__file__).with_name('ppo_training_world.py')):
        raise ValueError('baseline world batching changed; isolate its dependency before comparing')
    baseline.prepare = baseline_batch.prepare
    baseline.distribution = baseline_batch.distribution
    baseline.critic_values = baseline_batch.critic_values
    config = dict(protocol['spec']['selection'], **protocol['config']['ppo'], reward=protocol['config']['reward'])
    microbatch = protocol['config']['execution']['microbatch']
    experiences = [load_experience(r) if r['status'] == 'complete' else None for r in record['results']]
    began = time.monotonic()

    def guard():
        if time.monotonic() - began > args.seconds:
            raise TimeoutError(f'data-supply benchmark exceeded {args.seconds} seconds')

    def restored():
        # Adam's CPU step tensors may alias load_state_dict input storage.
        # Reload the frozen file for every cell instead of mutating a shared
        # in-memory reference across supposedly independent repetitions.
        initial = torch.load(checkpoint, map_location='cpu', weights_only=False)
        models = learners(protocol, state['jobs'][0]['seed'])
        for model, key in zip(models[:4], ('policy', 'critic', 'actor_optimizer', 'critic_optimizer')):
            model.load_state_dict(initial[key])
        models[4].set_state(initial['shuffle_rng'])
        torch.set_rng_state(initial['rng'])
        torch.cuda.set_rng_state_all(initial['cuda_rng'])
        return models

    report = dict(schema='ppo-data-supply-benchmark-v1', phase=args.phase, status='running',
                  source_sha256=sha(__file__), baseline_source=dict(path=str(baseline_path), sha256=sha(baseline_path)),
                  baseline_batch_source=dict(path=str(baseline_batch_path), sha256=sha(baseline_batch_path)),
                  current_sources={str(p): sha(p) for p in (Path(current.__file__), Path(__file__).with_name('ppo_training_transfer.py'),
                      Path(__file__).with_name('ppo_training_batch.py'), Path(__file__).with_name('ppo_training_supply.py'))},
                  protocol_sha256=sha(root / 'protocol.json'), record=reference, behavior_checkpoint=record['behavior_checkpoint'],
                  gpu=torch.cuda.get_device_name(), torch_version=torch.__version__, repetitions=args.repeats, cells={})

    def publish():
        (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    if args.phase == 'transfer':
        model_set = restored()
        steps, advantages, targets, _ = current.collect_steps(*model_set[:2], record['results'], config,
                                                             experiences=experiences)
        indices = torch.randperm(len(steps), generator=model_set[4])[:microbatch].tolist()
        prepared = prepare([steps[i] for i in indices], config, model_set[0].width, 'cpu')
        leaves = []
        map_tensors(prepared, lambda t: leaves.append(t))
        report['prepared'] = dict(tensors=len(leaves), bytes=sum(t.numel() * t.element_size() for t in leaves),
                                  dtypes=sorted({str(t.dtype) for t in leaves}))
        expected = None
        for name, move in [('individual-transfers', baseline.move_prepared), ('coalesced-transfers', coalesced),
                           ('asynchronous-transfers', asynchronous)]:
            timings = []
            for repetition in range(args.repeats + 1):
                guard()
                torch.cuda.synchronize()
                start = time.perf_counter()
                actual = move(prepared, 'cuda')
                torch.cuda.synchronize()
                elapsed = time.perf_counter() - start
                if repetition:
                    timings.append(elapsed)
                with torch.no_grad():
                    values = (distribution(model_set[0], actual, config)[0].cpu(), critic_values(model_set[1], actual).cpu())
                if expected is None:
                    expected = values
                else:
                    for a, b in zip(values, expected):
                        torch.testing.assert_close(a, b, rtol=0, atol=0)
                del actual
            report['cells'][name] = dict(seconds=timings, median_seconds=statistics.median(timings), probabilities_and_values_exact=True)
            publish()
    elif args.phase == 'kernels':
        model_set = restored()
        steps, advantages, targets, _ = current.collect_steps(*model_set[:2], record['results'], config,
                                                             experiences=experiences)
        indices = torch.randperm(len(steps), generator=model_set[4])[:microbatch].tolist()
        chunks = current.prepare_minibatch([steps[i] for i in indices], advantages[indices], targets[indices],
                                          config, model_set[0].width, microbatch, 'cuda')
        current.train_prepared(*model_set[:4], chunks, config)
        model_set = restored()
        torch.cuda.synchronize()
        start = time.perf_counter()
        current.train_prepared(*model_set[:4], chunks, config)
        torch.cuda.synchronize()
        report['unprofiled_resident_minibatch_seconds'] = time.perf_counter() - start
        model_set = restored()
        with torch.profiler.profile(activities=[torch.profiler.ProfilerActivity.CPU, torch.profiler.ProfilerActivity.CUDA]) as profile:
            current.train_prepared(*model_set[:4], chunks, config)
            torch.cuda.synchronize()
        events = profile.key_averages()
        rows = [dict(name=e.key, count=e.count, self_cpu_us=e.self_cpu_time_total,
                     self_device_us=e.self_device_time_total) for e in events]
        report['top_cpu_operators'] = sorted(rows, key=lambda r: r['self_cpu_us'], reverse=True)[:20]
        report['top_device_operators'] = sorted(rows, key=lambda r: r['self_device_us'], reverse=True)[:20]
        report['note'] = 'One warmed minibatch; instrumented operator timings do not establish full-update utilization.'
        profile.export_chrome_trace(str(args.output / 'cuda-trace.json'))
    else:
        timings = {name: [] for name in ('frozen-file-update', 'memory-update', 'asynchronous-memory-update')}
        for repetition in range(args.repeats + 1):
            order = list(timings)
            if repetition % 2:
                order.reverse()
            for name in order:
                guard()
                gc.collect()
                torch.cuda.empty_cache()
                model_set = restored()
                torch.cuda.reset_peak_memory_stats()
                torch.cuda.synchronize()
                start = time.perf_counter()
                if name == 'frozen-file-update':
                    actual = baseline.update(*model_set, record['results'], config, microbatch, guard)
                else:
                    move = asynchronous if name == 'asynchronous-memory-update' else baseline.move_prepared
                    with patch.object(current, 'move_prepared', move):
                        actual = current.update(*model_set, record['results'], config, microbatch, guard, experiences=experiences)
                torch.cuda.synchronize()
                elapsed = time.perf_counter() - start
                for key in EXACT_KEYS:
                    if actual[key] != record['ppo'][key]:
                        raise AssertionError(f'{name}: retained CUDA update differs: {key}')
                if repetition:
                    timings[name].append(elapsed)
                report['cells'][name] = dict(seconds=timings[name], median_seconds=statistics.median(timings[name]) if timings[name] else None,
                                            last_stages=actual['timing'], all_checkpoint_and_epoch_fields_exact=True,
                                            cuda_peak_allocated_bytes=torch.cuda.max_memory_allocated())
                publish()
                print(json.dumps(dict(cell=name, repetition=repetition, seconds=elapsed, exact=True)), flush=True)
                del model_set, actual
    report.update(status='complete', seconds=time.monotonic() - began,
                  host_peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
    publish()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--baseline-runtime', type=Path, required=True)
    parser.add_argument('--batch', type=int, default=3)
    parser.add_argument('--phase', choices=('transfer', 'update', 'kernels'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, choices=range(1, 6), default=3)
    parser.add_argument('--seconds', type=int, choices=range(30, 1801), default=900)
    main(parser.parse_args())
