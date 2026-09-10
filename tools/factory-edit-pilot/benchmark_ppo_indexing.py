"""Compare masked padded indexing against an explicitly isolated frozen learner."""
import argparse
import gc
import importlib.util
import json
import os
from pathlib import Path
import statistics
import time

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')
import torch

from ppo_area_update import load_experience
from ppo_harness_run import learners
from ppo_harness_state import read, write, sha, checked_path
import ppo_training_batch as current_batch
import ppo_training_update as current

EXACT = ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
         'critic_optimizer_sha256', 'shuffle_rng_sha256', 'epochs')


def frozen(directory):
    def module(name):
        spec = importlib.util.spec_from_file_location('indexing_baseline_' + name, directory / (name + '.py'))
        value = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(value)
        return value
    world, batch, supply, update = [module('ppo_training_' + name) for name in ('world', 'batch', 'supply', 'update')]
    batch.prepare_worlds, batch.encode_worlds = world.prepare_worlds, world.encode_worlds
    supply.prepare = batch.prepare
    update.compile_steps, update.distribution, update.critic_values = batch.compile_steps, batch.distribution, batch.critic_values
    update.prepare_cpu, update.prepared_batches = supply.prepare_cpu, supply.prepared_batches
    update.EXECUTION_SCHEMA = batch.EXECUTION_SCHEMA
    return batch, update


def difference(actual, expected):
    assert len(actual) == len(expected)
    if not actual:
        return dict(exact=True, max_absolute=0.0, max_scaled=0.0)
    return dict(exact=all(torch.equal(a, b) for a, b in zip(actual, expected)),
                max_absolute=max(float((a - b).abs().max()) for a, b in zip(actual, expected)),
                max_scaled=max(float(((a - b).abs() / (1e-6 + b.abs())).max()) for a, b in zip(actual, expected)))


def tensors(value):
    if isinstance(value, torch.Tensor):
        return [value.detach().cpu().clone()]
    if isinstance(value, dict):
        return [t for child in value.values() for t in tensors(child)]
    if isinstance(value, (tuple, list)):
        return [t for child in value for t in tensors(child)]
    return []


def close_report(actual, expected, rtol, atol):
    result = dict(**difference(actual, expected), rtol=rtol, atol=atol)
    result['close'] = all(torch.allclose(a, b, rtol=rtol, atol=atol) for a, b in zip(actual, expected))
    return result


def main(args):
    args.output = args.output.resolve()
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
    reference = next(r for r in state['jobs'][0]['records'] if r['phase'] == dict(kind='training', batch=args.batch))
    record = read(checked_path(root, reference))
    checkpoint = checked_path(root, record['behavior_checkpoint'])
    config = dict(protocol['spec']['selection'], **protocol['config']['ppo'], reward=protocol['config']['reward'])
    microbatch = protocol['config']['execution']['microbatch']
    baseline_dir = args.baseline_runtime.resolve() / 'tools/factory-edit-pilot'
    baseline_batch, baseline = frozen(baseline_dir)
    experiences = [load_experience(r) if r['status'] == 'complete' else None for r in record['results']]
    report = dict(schema='ppo-batched-indexing-benchmark-v1', status='running', phase=args.phase,
                  source_sha256=sha(__file__), protocol_sha256=sha(root / 'protocol.json'), record=reference,
                  behavior_checkpoint=record['behavior_checkpoint'], gpu=torch.cuda.get_device_name(),
                  torch_version=torch.__version__, repetitions=args.repeats, cells={},
                  allow_roundoff=args.allow_roundoff,
                  sources={str(p): sha(p) for directory in (baseline_dir, Path(__file__).parent)
                           for p in directory.glob('ppo_training_*.py')})
    began = time.monotonic()

    def guard():
        if time.monotonic() - began > args.seconds:
            raise TimeoutError(f'indexing benchmark exceeded {args.seconds} seconds: {args.output}')

    def publish():
        write(args.output / 'report.json', report)

    def restored():
        initial = torch.load(checkpoint, map_location='cpu', weights_only=False)
        values = learners(protocol, state['jobs'][0]['seed'])
        for model, key in zip(values[:4], ('policy', 'critic', 'actor_optimizer', 'critic_optimizer')):
            model.load_state_dict(initial[key])
        values[4].set_state(initial['shuffle_rng'])
        torch.set_rng_state(initial['rng'])
        torch.cuda.set_rng_state_all(initial['cuda_rng'])
        return values

    variants = {'frozen': (baseline_batch, baseline), 'padded': (current_batch, current)}
    if args.phase in ('verify', 'kernels'):
        models = restored()
        steps, advantages, targets, _ = current.collect_steps(*models[:2], record['results'], config, experiences=experiences)
        indices = torch.randperm(len(steps), generator=models[4])[:microbatch].tolist()
        sample = [steps[i] for i in indices]
        expected = None
        for name, (batch_module, update) in variants.items():
            guard()
            models = restored()
            start = time.perf_counter()
            chunks = update.prepare_minibatch(sample, advantages[indices], targets[indices], config, models[0].width, microbatch, 'cuda')
            torch.cuda.synchronize()
            preparation = time.perf_counter() - start
            batch = chunks[0]['batch']
            with torch.no_grad():
                predictions = [t.cpu() for t in (*batch_module.distribution(models[0], batch, config)[:2],
                                                 batch_module.critic_values(models[1], batch))]
            update.train_prepared(*models[:4], chunks, config)
            gradients = [p.grad.detach().cpu().clone() for model in models[:2] for p in model.parameters() if p.grad is not None]
            if expected is None:
                expected = (predictions, gradients)
            for a, b in zip(predictions, expected[0]):
                torch.testing.assert_close(a, b, rtol=1e-5, atol=1e-6)
            for a, b in zip(gradients, expected[1]):
                torch.testing.assert_close(a, b, rtol=5e-5, atol=5e-6)
            timings = []
            for repetition in range(args.repeats):
                guard()
                models = restored()
                torch.cuda.reset_peak_memory_stats()
                torch.cuda.synchronize()
                start = time.perf_counter()
                update.train_prepared(*models[:4], chunks, config)
                torch.cuda.synchronize()
                timings.append(time.perf_counter() - start)
            world_batches = [batch['actor']['worlds'], batch['critic_worlds']]
            padding = [dict(worlds=w['count'], pixels=sum(b['raster'].shape[0] * b['raster'].shape[2] * b['raster'].shape[3] for b in w['buckets']),
                            valid_pixels=sum(float(b['mask'].sum()) if 'mask' in b else b['raster'].shape[0] * b['raster'].shape[2] * b['raster'].shape[3] for b in w['buckets'])) for w in world_batches if w]
            report['cells'][name] = dict(preparation_seconds=preparation, padding=padding, resident_seconds=timings,
                median_seconds=statistics.median(timings), predictions=difference(predictions, expected[0]),
                gradients=difference(gradients, expected[1]), peak_cuda_bytes=torch.cuda.max_memory_allocated())
            publish()
            if args.phase == 'kernels':
                models = restored()
                with torch.profiler.profile(activities=[torch.profiler.ProfilerActivity.CPU, torch.profiler.ProfilerActivity.CUDA]) as profile:
                    update.train_prepared(*models[:4], chunks, config)
                    torch.cuda.synchronize()
                events = [dict(name=e.key, count=e.count, self_cpu_us=e.self_cpu_time_total,
                               self_device_us=e.self_device_time_total) for e in profile.key_averages()]
                report['cells'][name]['kernel_launches'] = sum(e['count'] for e in events if e['name'] == 'cudaLaunchKernel')
                report['cells'][name]['top_cpu'] = sorted(events, key=lambda e: e['self_cpu_us'], reverse=True)[:15]
                report['cells'][name]['top_device'] = sorted(events, key=lambda e: e['self_device_us'], reverse=True)[:15]
                profile.export_chrome_trace(str(args.output / f'{name}-trace.json'))
                publish()
            print(json.dumps(dict(cell=name, **report['cells'][name])), flush=True)
            del chunks, batch, models
    else:
        timings = {name: [] for name in variants}
        frozen_state = None
        frozen_parameters = None
        frozen_predictions = None
        repeated = {}
        for repetition in range(args.repeats + 1):
            order = list(variants)
            if repetition % 2:
                order.reverse()
            for name in order:
                guard()
                gc.collect()
                torch.cuda.empty_cache()
                models = restored()
                torch.cuda.reset_peak_memory_stats()
                torch.cuda.synchronize()
                start = time.perf_counter()
                actual = variants[name][1].update(*models, record['results'], config, microbatch, guard, experiences=experiences)
                torch.cuda.synchronize()
                elapsed = time.perf_counter() - start
                peak_cuda_bytes = torch.cuda.max_memory_allocated()
                exact = {key: actual[key] == record['ppo'][key] for key in EXACT}
                state_tensors = {key: tensors(model.state_dict()) for key, model in
                                 zip(('actor', 'critic', 'actor_adam', 'critic_adam'), models[:4])}
                if frozen_state is None:
                    assert name == 'frozen'
                    frozen_state = state_tensors
                    frozen_parameters = {f'{kind}.{key}': value.detach().cpu().clone()
                                         for kind, model in zip(('actor', 'critic'), models[:2])
                                         for key, value in model.named_parameters()}
                numerical = {key: close_report(value, frozen_state[key], 1e-4, 1e-5 if 'adam' not in key else 1e-6)
                             for key, value in state_tensors.items()}
                parameter_differences = {f'{kind}.{key}': difference([value.detach().cpu()], [frozen_parameters[f'{kind}.{key}']])
                                         for kind, model in zip(('actor', 'critic'), models[:2])
                                         for key, value in model.named_parameters()}
                before_epochs, after_epochs = record['ppo']['epochs'], actual['epochs']
                same_control = all(actual[key] == record['ppo'][key] for key in
                                   ('selected', 'optimizer_steps', 'kl_stopped', 'shuffle_rng_sha256'))
                same_control = same_control and len(before_epochs) == len(after_epochs)
                if same_control:
                    metric_keys = [key for key, value in before_epochs[0].items() if value is not None]
                    numerical['epochs'] = close_report(
                        [torch.tensor([[e[k] for k in metric_keys] for e in after_epochs], dtype=torch.float64)],
                        [torch.tensor([[e[k] for k in metric_keys] for e in before_epochs], dtype=torch.float64)], 1e-4, 1e-5)
                hashes = {key: actual[key] for key in EXACT}
                deterministic = name not in repeated or repeated[name] == hashes
                repeated[name] = hashes
                predictions = None
                if not repetition:
                    initial_models = restored()
                    steps, advantages, targets, _ = current.collect_steps(*initial_models[:2], record['results'], config, experiences=experiences)
                    del initial_models
                    predictions = [[], [], [], []]
                    with torch.no_grad():
                        for start_index in range(0, len(steps), microbatch):
                            guard()
                            end = start_index + microbatch
                            chunks = variants[name][1].prepare_minibatch(steps[start_index:end], advantages[start_index:end],
                                targets[start_index:end], config, models[0].width, microbatch, 'cuda')
                            batch = chunks[0]['batch']
                            values = (*variants[name][0].distribution(models[0], batch, config),
                                      variants[name][0].critic_values(models[1], batch))
                            for group, value in zip(predictions, values):
                                group.append(value.detach().cpu().flatten())
                    predictions = [torch.cat(group) for group in predictions]
                    if frozen_predictions is None:
                        frozen_predictions = predictions
                    prediction_report = {key: difference([a], [b]) for key, a, b in
                                         zip(('selected_logp', 'entropy', 'all_probabilities', 'values'), predictions, frozen_predictions)}
                    del chunks, batch, steps
                else:
                    prediction_report = report['cells'][name]['final_predictions']
                if repetition:
                    timings[name].append(elapsed)
                report['cells'][name] = dict(seconds=timings[name], median_seconds=statistics.median(timings[name]) if timings[name] else None,
                    exact=exact, numerical=numerical, same_control=same_control, deterministic=deterministic,
                    largest_parameter_differences=sorted(parameter_differences.items(), key=lambda row: row[1]['max_absolute'], reverse=True)[:10],
                    epoch_metrics=actual['epochs'],
                    final_predictions=prediction_report,
                    last_stages=actual['timing'], peak_cuda_bytes=peak_cuda_bytes)
                publish()
                print(json.dumps(dict(cell=name, repetition=repetition, seconds=elapsed, exact=exact)), flush=True)
                if ((name == 'frozen' and not all(exact.values())) or not same_control or not deterministic or
                        (not args.allow_roundoff and not all(v['close'] for v in numerical.values()))):
                    raise AssertionError(f'full continuation validation failed: {name}: {report["cells"][name]}')
                del models, actual
    report.update(status='complete', seconds=time.monotonic() - began)
    publish()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--baseline-runtime', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--batch', type=int, default=3)
    parser.add_argument('--phase', choices=('verify', 'kernels', 'update'), default='verify')
    parser.add_argument('--repeats', type=int, choices=range(1, 6), default=3)
    parser.add_argument('--seconds', type=int, choices=range(30, 1801), default=600)
    parser.add_argument('--allow-roundoff', action='store_true',
                        help='Measure full-update numerical differences without requiring old parameter/metric tolerances; control flow and repeated hashes must remain exact')
    main(parser.parse_args())
