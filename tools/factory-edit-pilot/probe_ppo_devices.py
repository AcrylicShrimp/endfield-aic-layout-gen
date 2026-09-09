"""Sequential device then AMP experiments on the optimized PPO execution path."""
import argparse
from contextlib import nullcontext
from copy import deepcopy
from pathlib import Path
import statistics
import time

import torch

from benchmark_ppo_optimization import fresh
from full_factory_screen import sha
from ppo_training_batch import distribution, critic_values, require
from ppo_training_update import prepare_minibatch, train_prepared
from probe_ppo_precision import reference_workload, vector, difference, synchronize
from random_factory_generation import read, write
from random_factory_pilot import configure_torch


def cell_models(workload, device):
    actor, critic, aopt, copt = fresh(workload)
    actor.to(device)
    critic.to(device)
    aopt.load_state_dict(deepcopy(workload['checkpoint']['actor_optimizer']))
    copt.load_state_dict(deepcopy(workload['checkpoint']['critic_optimizer']))
    return actor, critic, aopt, copt


def predictions(actor, critic, chunks, precision=None):
    logps, values = [], []
    with torch.no_grad():
        for chunk in chunks:
            device = next(actor.parameters()).device
            context = torch.autocast(device.type, dtype=precision) if precision is not None else nullcontext()
            with context:
                logps.append(distribution(actor, chunk['batch'], chunk['config'])[0].cpu())
                values.append(critic_values(critic, chunk['batch']).cpu())
    return dict(logps=torch.cat(logps), values=torch.cat(values))


def train(model_set, chunks, config, dtype, device, capture=False):
    scaler = torch.amp.GradScaler(device.type) if dtype == torch.float16 else None
    calls = [0, 0]
    def counted(index):
        def hook(optimizer, args, kwargs):
            calls[index] += 1
        return hook
    handles = [opt.register_step_post_hook(counted(i)) for i, opt in enumerate(model_set[2:])]
    try:
        result = train_prepared(*model_set, chunks, config, precision=dtype, scaler=scaler)
    finally:
        for handle in handles:
            handle.remove()
    require(calls == [1, 1], f'actual optimizer steps for {device}/{dtype}: {calls}')
    result.update(actual_optimizer_steps=calls, scale=scaler.get_scale() if scaler is not None else None)
    if capture:
        result['vectors'] = {kind: {name: vector(model, kind == 'gradients')
                                   for name, model in zip(('actor', 'critic'), model_set[:2])}
                             for kind in ('parameters', 'gradients')}
    return result


def run(args):
    configure_torch()
    began = time.monotonic()
    def guard():
        require(time.monotonic() - began < args.seconds, 'device probe time budget')
    require(not args.output.exists(), f'fresh device probe output: {args.output}')
    prerequisite = read(args.native_check)
    require(prerequisite['status'] == 'complete' and prerequisite['native_checkpoint_continuation_exact'],
            'completed optimized native continuation check')
    workload = reference_workload(args.screen)
    config = workload['config']
    cuda = torch.cuda.is_available()
    if cuda:
        torch.backends.cuda.matmul.fp32_precision = 'ieee'
        torch.backends.cudnn.conv.fp32_precision = 'ieee'
        torch.backends.cudnn.benchmark = False
    if args.phase == 'device':
        cells = [('cpu-fp32-8', 'cpu', 8, None), ('cpu-fp32-64', 'cpu', 64, None)]
        cells += [(f'cuda-fp32-{size}', 'cuda', size, None) for size in (8, 32, 64)]
    else:
        require(args.device_report is not None, 'precision phase requires the completed device report')
        prior = read(args.device_report)
        require(prior['phase'] == 'device' and prior['status'] == 'complete', 'completed FP32 device comparison')
        size = prior['selected_cuda_microbatch']
        require(size is not None, 'successful CUDA FP32 cell before AMP')
        cells = [('cpu-fp32-8', 'cpu', 8, None), (f'cuda-fp32-{size}', 'cuda', size, None),
                 (f'cuda-bf16-{size}', 'cuda', size, torch.bfloat16), (f'cuda-fp16-{size}', 'cuda', size, torch.float16)]
    args.output.mkdir(parents=True)
    report = dict(schema='ppo-optimized-device-probe-v1', phase=args.phase, status='running', workload=workload['identity'],
                  sources={name: sha(Path(__file__).with_name(name)) for name in
                           ['probe_ppo_devices.py', 'ppo_training_batch.py', 'ppo_training_update.py', 'probe_ppo_precision.py']},
                  native_check=dict(path=str(args.native_check), sha256=sha(args.native_check)),
                  device_report=dict(path=str(args.device_report), sha256=sha(args.device_report)) if args.device_report else None,
                  torch_version=torch.__version__, cuda_version=torch.version.cuda, cuda_available=cuda,
                  gpu=torch.cuda.get_device_name() if cuda else None, threads=torch.get_num_threads(),
                  deterministic_algorithms=torch.are_deterministic_algorithms_enabled(), fp32='ieee',
                  scope='Same complete 64-selection minibatch and nonempty Adam state; one warmup plus three reset timings, synchronized GPU boundaries; no rollout or continued learning.',
                  numerical_limits=dict(fp32_logp_absolute=1e-4, fp32_value_absolute=1e-5,
                                        amp_logp_absolute=.01, amp_value_absolute=.01,
                                        amp_gradient_relative_l2=.1, amp_gradient_cosine=.995), cells=[])
    write(args.output / 'report.json', report)
    reference = None
    for name, device_name, size, dtype in cells:
        guard()
        cell = dict(name=name, device=device_name, microbatch=size, precision=str(dtype or torch.float32), status='running')
        report['cells'].append(cell)
        if device_name == 'cuda' and not cuda:
            cell.update(status='unsupported', diagnostic='ppo-device.cuda-unavailable')
            continue
        device = torch.device(device_name)
        try:
            if dtype == torch.bfloat16:
                require(torch.cuda.is_bf16_supported(), 'CUDA BF16 support')
            def prepared():
                chunks = prepare_minibatch(workload['steps'], workload['advantages'], workload['targets'], config,
                                          workload['spec']['policy']['width'], size, device)
                for chunk in chunks:
                    chunk['config'] = config
                return chunks
            preparation, combined, resident = [], [], []
            if device_name == 'cuda':
                torch.cuda.reset_peak_memory_stats(device)
            for repetition in range(4):
                guard()
                model_set = cell_models(workload, device)
                synchronize(device)
                started = time.perf_counter()
                chunks = prepared()
                synchronize(device)
                middle = time.perf_counter()
                details = train(model_set, chunks, config, dtype, device)
                synchronize(device)
                ended = time.perf_counter()
                if repetition:
                    preparation.append(middle - started)
                    combined.append(ended - started)
                model_set = cell_models(workload, device)
                synchronize(device)
                started = time.perf_counter()
                train(model_set, chunks, config, dtype, device)
                synchronize(device)
                if repetition:
                    resident.append(time.perf_counter() - started)
            model_set = cell_models(workload, device)
            before = predictions(*model_set[:2], chunks, dtype)
            details = train(model_set, chunks, config, dtype, device, capture=True)
            after = predictions(*model_set[:2], chunks)
            vectors = dict(**details.pop('vectors'), before=before, after=after)
            torch.save(vectors, args.output / f'{name}-vectors.pt')
            cell.update(status='complete', timings=dict(preparation_transfer=preparation, combined=combined, resident=resident),
                        medians=dict(preparation_transfer=statistics.median(preparation), combined=statistics.median(combined),
                                     resident=statistics.median(resident)), update=details,
                        peak_cuda_allocated_bytes=torch.cuda.max_memory_allocated(device) if device_name == 'cuda' else None)
            if reference is None:
                reference = vectors
                cell['numerical_acceptance'] = True
            else:
                cell['drift'] = {kind: {key: difference(value, reference[kind][key]) for key, value in vectors[kind].items()}
                                 for kind in ('parameters', 'gradients', 'before', 'after')}
                limits = report['numerical_limits']
                if dtype is None:
                    accepted = all(cell['drift'][when][key]['max_absolute'] <= limits[f'fp32_{"logp" if key == "logps" else "value"}_absolute']
                                   for when in ('before', 'after') for key in ('logps', 'values'))
                else:
                    accepted = (cell['drift']['before']['logps']['max_absolute'] <= limits['amp_logp_absolute'] and
                                cell['drift']['before']['values']['max_absolute'] <= limits['amp_value_absolute'] and
                                all(d['relative_l2'] is not None and d['relative_l2'] <= limits['amp_gradient_relative_l2'] and
                                    d['cosine'] is not None and d['cosine'] >= limits['amp_gradient_cosine']
                                    for d in cell['drift']['gradients'].values()))
                cell['numerical_acceptance'] = accepted
            print(dict(cell=name, medians=cell['medians'], numerical_acceptance=cell['numerical_acceptance'],
                       drift=cell.get('drift', {}).get('before')), flush=True)
        except Exception as error:
            cell.update(status='failed', diagnostic=dict(code='ppo-device.cell-failed', severity='error',
                                                         stage=args.phase, entity=name, message=str(error)))
            print(dict(cell=name, status='failed', error=str(error)), flush=True)
            if device_name == 'cpu':
                write(args.output / 'report.json', report)
                raise
        finally:
            if device_name == 'cuda':
                torch.cuda.empty_cache()
        write(args.output / 'report.json', report)
    usable = [c for c in report['cells'] if c['status'] == 'complete' and c['numerical_acceptance']]
    gpu = [c for c in usable if c['device'] == 'cuda']
    report.update(status='complete', seconds=time.monotonic() - began,
                  selected_cuda_microbatch=min(gpu, key=lambda c: c['medians']['combined'])['microbatch'] if gpu else None,
                  fastest_accepted_cell=min(usable, key=lambda c: c['medians']['combined'])['name'])
    write(args.output / 'report.json', report)
    print(dict(status='complete', phase=args.phase, fastest_accepted_cell=report['fastest_accepted_cell']), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--phase', choices=['device', 'precision'], required=True)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--native-check', type=Path, required=True)
    parser.add_argument('--device-report', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=600)
    args = parser.parse_args()
    if not 0 < args.seconds <= 600:
        parser.error('seconds must be in (0, 600]')
    existed = args.output.exists()
    try:
        run(args)
    except BaseException as error:
        path = args.output / 'report.json'
        if not existed and path.exists():
            report = read(path)
            report.update(status='failed', diagnostic=dict(code='ppo-device.probe-failed', severity='error',
                                                           stage=args.phase, entity=str(args.screen), message=str(error)))
            write(path, report)
        raise
