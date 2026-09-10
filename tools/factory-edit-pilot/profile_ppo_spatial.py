"""Profile predeclared spatial actors on measured retained states before training."""
import argparse
import gc
import os
from pathlib import Path
import resource
import time
os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')
import torch
from full_factory_on_policy import parameter_hash
from ppo_area_model import models
from ppo_area_update import load_experience
from ppo_harness_state import read, write, checked_path, sha
from ppo_spatial_model import VARIANTS, specification
from ppo_training_batch import prepare, distribution, critic_values
from ppo_training_transfer import move_prepared


def profile(root, output):
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    torch.backends.cuda.matmul.fp32_precision = 'ieee'
    torch.backends.cudnn.conv.fp32_precision = 'ieee'
    torch.backends.cudnn.benchmark = False
    state, protocol = read(root / 'state.json'), read(root / 'protocol.json')
    reference = next(r for r in state['jobs'][0]['records'] if r['phase'] == dict(kind='training', batch=32))
    record = read(checked_path(root, reference))
    steps = [s for r in record['results'] if r['status'] == 'complete' for s in load_experience(r)['steps']]
    measures = dict(cells=lambda s: s['actor_observation']['current']['raster'].shape[1] * s['actor_observation']['current']['raster'].shape[2],
                    nodes=lambda s: len(s['actor_observation']['current']['nodes']),
                    actions=lambda s: len(s['actor_observation']['actions']))
    selected, sizes = {}, {}
    for metric, measure in measures.items():
        ordered = sorted(range(len(steps)), key=lambda i: measure(steps[i]))
        sizes[metric] = dict(min=measure(steps[ordered[0]]), median=measure(steps[ordered[len(ordered)//2]]),
                             p95=measure(steps[ordered[int(.95*(len(ordered)-1))]]), max=measure(steps[ordered[-1]]))
        for label, index in [('median', len(ordered)//2), ('p95', int(.95*(len(ordered)-1))), ('max', len(ordered)-1)]:
            selected[f'{metric}-{label}'] = ordered[index]
    config = dict(protocol['spec']['selection'], **protocol['config']['ppo'])
    report = dict(schema='ppo-spatial-profile-v1', source_protocol_sha256=sha(root/'protocol.json'),
                  source_record=reference, retained_states=len(steps), sizes=sizes, variants={})
    for name in VARIANTS:
        actor, critic = models(dict(model=specification(name)), 17)
        item = dict(model=specification(name), actor_parameters=sum(p.numel() for p in actor.parameters()),
                    critic_parameters=sum(p.numel() for p in critic.parameters()),
                    initial_actor_sha256=parameter_hash(actor), initial_critic_sha256=parameter_hash(critic), cases=[])
        for label, index in selected.items():
            sample = steps[index]
            began = time.perf_counter()
            prepared = prepare([sample], config, 32, 'cpu')
            prep = time.perf_counter() - began
            with torch.no_grad():
                began = time.perf_counter()
                expected = actor(sample['actor_observation'])
                cpu_seconds = time.perf_counter() - began
            actor, critic = actor.cuda(), critic.cuda()
            batch = move_prepared(prepared, 'cuda')
            torch.cuda.reset_peak_memory_stats()
            torch.cuda.synchronize()
            began = time.perf_counter()
            logp, entropy, probs = distribution(actor, batch, config)
            values = critic_values(critic, batch)
            torch.cuda.synchronize()
            forward = time.perf_counter() - began
            from ppo_training_batch import actor_logits
            with torch.no_grad():
                torch.testing.assert_close(actor_logits(actor, batch).cpu(), expected, rtol=5e-5, atol=5e-6)
            began = time.perf_counter()
            (-logp.mean() - .001*entropy.mean() + values.square().mean()).backward()
            torch.cuda.synchronize()
            backward = time.perf_counter() - began
            assert all(torch.isfinite(p.grad).all() for m in (actor, critic) for p in m.parameters() if p.grad is not None)
            torch.testing.assert_close(probs.sum(1), torch.ones(1, device='cuda'))
            item['cases'].append(dict(label=label, state_index=index, sizes={k: f(sample) for k, f in measures.items()},
                                      preparation_seconds=prep, cpu_actor_seconds=cpu_seconds,
                                      cuda_actor_critic_forward_seconds=forward, cuda_backward_seconds=backward,
                                      cuda_peak_allocated_bytes=torch.cuda.max_memory_allocated()))
            actor.zero_grad(set_to_none=True)
            critic.zero_grad(set_to_none=True)
            actor, critic = actor.cpu(), critic.cpu()
            del batch, logp, entropy, probs, values
            gc.collect()
        report['variants'][name] = item
        write(output, report)
        print(name, item['actor_parameters'], max(c['cuda_peak_allocated_bytes'] for c in item['cases']), flush=True)
    assert len({v['initial_critic_sha256'] for v in report['variants'].values()}) == 1
    report.update(status='complete', host_peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                  timing_note='Single-state first-pass diagnostics, not warmed throughput benchmarks.')
    write(output, report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    profile(args.run.resolve(), args.output.resolve())
