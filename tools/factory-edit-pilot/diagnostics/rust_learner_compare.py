"""Bounded Python/Rust numerical and separate-process recovery gate.

Uses actual exported frozen inputs, the existing Python model/likelihood, and
Python's foreach/fused Adam as oracle. Not a rollout or training-quality test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import torch
from safetensors.torch import load_file, save_file
import declarative_learning_policy as policy
from rust_learner_export import state_tensors


def write(path, value):
    path.write_text(json.dumps(value, indent=2)+'\n')


def compare_tensors(left, right, atol=2e-6, rtol=2e-5):
    assert left.keys() == right.keys(), (left.keys()-right.keys(), right.keys()-left.keys())
    maximum, worst = 0., None
    for name in left:
        a, b = left[name], right[name]
        assert a.dtype == b.dtype and a.shape == b.shape, name
        if a.is_floating_point():
            difference = float((a-b).abs().max()) if a.numel() else 0.
            if difference > maximum:
                maximum, worst = difference, name
            torch.testing.assert_close(a, b, atol=atol, rtol=rtol, msg=lambda m: f'{name}: {m}')
        else:
            assert torch.equal(a, b), name
    return dict(fields=len(left), maximum_absolute_error=maximum, worst_field=worst, atol=atol, rtol=rtol)


def oracle(manifest_path, output, device, backend, steps):
    began = time.perf_counter()
    manifest = json.loads(manifest_path.read_text())
    tensors = load_file(manifest_path.parent/manifest['tensors'])
    config = manifest['update']
    model = policy.ActorCritic(manifest['model']['width'], manifest['model']['message_rounds']).to(device)
    model.load_state_dict({n.removeprefix('model.'): t for n, t in tensors.items() if n.startswith('model.')}, strict=True)
    groups = {label: [(n, p) for n, p in model.named_parameters() if n.startswith('critic.') == (label == 'critic')]
              for label in ('actor', 'critic')}
    optimizers = {}
    for label, parameters in groups.items():
        optimizer = torch.optim.Adam([p for _, p in parameters], lr=config['learning_rate'],
            betas=(config['beta1'], config['beta2']), eps=config['epsilon'],
            foreach=backend == 'foreach', fused=backend == 'fused')
        for name, p in parameters:
            prefix = f'adam.{label}.{name}'
            step = int(tensors[f'{prefix}.step'])
            if step:
                optimizer.state[p] = dict(step=torch.tensor(float(step), device=device if backend == 'fused' else 'cpu'),
                    exp_avg=tensors[f'{prefix}.mean'].to(device).clone(), exp_avg_sq=tensors[f'{prefix}.square'].to(device).clone())
        optimizers[label] = optimizer
    generators = {name: torch.Generator().set_state(tensors[f'rng.{name}'])
                  for name in ('sampler', 'shuffle', 'replay', 'replay_order')}
    def packed(prefix):
        value = {}
        for section, size in [('batch', manifest[f'{prefix}_graphs']), ('branches', manifest[f'{prefix}_factors'])]:
            key = f'{prefix}.{section}.'
            value[section] = {n.removeprefix(key): t.to(device) for n, t in tensors.items() if n.startswith(key)}
            value[section].update(schema=policy.PACKED, size=size)
            if not size:
                value[section] = None
        value.update({n: tensors[f'{prefix}.{n}'].to(device) for n in ('roots', 'selected')})
        return value
    ppo, sil = packed('ppo'), packed('sil')
    targets = {n: tensors[n].to(device) for n in ('ppo.targets', 'ppo.old_logp', 'ppo.advantages', 'sil.returns', 'sil.weights')}
    load_seconds = time.perf_counter()-began
    output.mkdir()
    def forward(path):
        with torch.no_grad():
            prediction = policy.predict(model, ppo)
        save_file({n: t.detach().cpu().contiguous() for n, t in zip(('logp', 'entropy', 'values'), prediction)}, path)
    forward(output/'forward.safetensors')
    records, update_seconds, step_seconds = [], 0., []
    for index in range(steps):
        actor_updated = index % 3 != 2
        started = time.perf_counter()
        permutation = torch.randperm(manifest['ppo_graphs'], generator=generators['shuffle'])
        order = permutation.to(device)
        for optimizer in optimizers.values():
            optimizer.zero_grad(set_to_none=True)
        for _, p in groups['actor']:
            p.requires_grad_(actor_updated)
        logits, values, nodes, pooled = model.root(ppo['batch'])
        roots, entropy = policy.root_distribution(logits, ppo['batch'])
        logp = roots[ppo['roots']]
        branches = ppo['branches']
        if branches is not None:
            conditional, port_entropy = policy.distribution(model.ports(nodes, pooled, branches), branches)
            logp = logp.index_add(0, branches['factor_owners'], conditional[ppo['selected']])
            entropy = entropy.index_add(0, branches['factor_owners'], port_entropy)
        roots = roots.detach()
        value_loss = .5*(values[order]-targets['ppo.targets'][order]).square().mean()
        loss, teacher, positive = value_loss, torch.zeros((), device=device), 0
        if actor_updated:
            ratio = (logp[order]-targets['ppo.old_logp'][order]).exp()
            clipped = ratio.clamp(1-config['clip_epsilon'], 1+config['clip_epsilon'])
            advantages = targets['ppo.advantages'][order]
            loss = loss-torch.minimum(ratio*advantages, clipped*advantages).mean()
            loss = loss-config['entropy_coefficient']*entropy[order].mean()
            if config['imitation_coefficient']:
                slp, _, sv = policy.predict(model, sil)
                advantage = (targets['sil.returns']-sv.detach()).clamp_min(0)
                positive = int((advantage > 0).sum())
                teacher = (-slp*advantage*targets['sil.weights']).sum()
                loss = loss+config['imitation_coefficient']*teacher
        loss_scalar = float(loss.detach())
        loss.backward()
        critic_norm = float(torch.nn.utils.clip_grad_norm_([p for _, p in groups['critic']], config['gradient_clip'], foreach=True))
        actor_norm = float(torch.nn.utils.clip_grad_norm_([p for _, p in groups['actor']], config['gradient_clip'], foreach=True)) if actor_updated else 0.
        if actor_updated:
            optimizers['actor'].step()
        optimizers['critic'].step()
        for _, p in groups['actor']:
            p.requires_grad_(True)
        count = int((ppo['batch']['action_graph'] == 0).sum())
        sample = int(torch.multinomial(roots[:count].exp().cpu(), 1, generator=generators['sampler']))
        draws = torch.randint(manifest['sil_graphs'], (8,), generator=generators['replay']).tolist()
        replay_order = torch.randperm(8, generator=generators['replay_order']).tolist()
        records.append(dict(step=index+1, actor_updated=actor_updated, loss=loss_scalar, value_loss=float(value_loss.detach()),
            imitation_loss=float(teacher.detach()), positive_replay_labels=positive, actor_gradient_norm=actor_norm,
            critic_gradient_norm=critic_norm, permutation=permutation.tolist(), sample_root=sample, replay_draws=draws, replay_order=replay_order))
        seconds = time.perf_counter()-started
        update_seconds += seconds
        step_seconds.append(seconds)
        if index == 0:
            save_file({n: p.grad.detach().cpu().contiguous() for n, p in model.named_parameters() if p.grad is not None}, output/'first-gradients.safetensors')
    started = time.perf_counter()
    save_file(state_tensors(model, optimizers['actor'], optimizers['critic'], generators), output/'state.safetensors')
    save_seconds = time.perf_counter()-started
    forward(output/'final-forward.safetensors')
    report = dict(records=records, step_seconds=step_seconds, load_seconds=load_seconds, update_seconds=update_seconds, tensor_save_seconds=save_seconds,
                  total_seconds=time.perf_counter()-began, timing_note='Same bounded kernel and diagnostic draws; save excludes Rust manifest/hash/fsync')
    write(output/'report.json', report)
    return report


def run(args):
    assert not args.output.exists(), args.output
    assert 4 <= args.steps <= 32
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    args.output.mkdir(parents=True)
    results = []
    env = dict(os.environ, CUBLAS_WORKSPACE_CONFIG=':4096:8')
    def native(*command):
        result = subprocess.run([str(args.binary.resolve()), 'learning', *map(str, command)], env=env, capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr or result.stdout)
        return json.loads(result.stdout)
    for device, backend in [('cpu', 'foreach'), ('cuda:0', 'foreach'), ('cuda:0', 'fused')]:
        label = device.replace(':', '-')+'-'+backend
        root = args.output/label
        root.mkdir()
        common = ('verify-kernel', '--input', args.input.resolve(), '--device', device, '--adam-backend', backend)
        continuous = native(*common, '--output', root/'continuous', '--steps', args.steps)
        native(*common, '--output', root/'prefix', '--steps', 2)
        resumed = native(*common, '--output', root/'resumed', '--steps', args.steps-2, '--resume', root/'prefix/checkpoint')
        recovery = native('compare-checkpoints', '--left', root/'continuous/checkpoint', '--right', root/'resumed/checkpoint')
        assert continuous['records'][2:] == resumed['records'], label
        expected = oracle(args.input, root/'python', device, backend, args.steps)
        comparisons = {}
        for filename in ('forward.safetensors', 'first-gradients.safetensors', 'final-forward.safetensors'):
            comparisons[filename] = compare_tensors(load_file(root/'continuous'/filename), load_file(root/'python'/filename))
        comparisons['state'] = compare_tensors(load_file(root/'continuous/checkpoint/state.safetensors'), load_file(root/'python/state.safetensors'))
        for actual, oracle_record in zip(continuous['records'], expected['records'], strict=True):
            for key in actual:
                if isinstance(actual[key], float):
                    torch.testing.assert_close(actual[key], oracle_record[key], atol=2e-6, rtol=2e-5, msg=key)
                else:
                    assert actual[key] == oracle_record[key], (label, key, actual[key], oracle_record[key])
        results.append(dict(device=device, adam_backend=backend, steps=args.steps, comparisons=comparisons, recovery=recovery,
            rust={k: continuous[k] for k in ('load_seconds', 'update_seconds', 'checkpoint_seconds', 'total_seconds')},
            python={k: expected[k] for k in ('load_seconds', 'update_seconds', 'tensor_save_seconds', 'total_seconds')},
            warmup_steps=4,
            rust_warm_mean_seconds=sum(continuous['step_seconds'][4:])/len(continuous['step_seconds'][4:]) if args.steps > 4 else None,
            python_warm_mean_seconds=sum(expected['step_seconds'][4:])/len(expected['step_seconds'][4:]) if args.steps > 4 else None,
            checkpoint_bytes=sum(p.stat().st_size for p in (root/'continuous/checkpoint').iterdir())))
        write(args.output/'summary.json', dict(schema='aic-rust-learner-comparison-v1', status='in-progress', results=results))
        print(f'{label}: numerical parity and exact process recovery passed', flush=True)
    rejections = rejection_checks(args, env)
    summary = dict(rejections=rejections, schema='aic-rust-learner-comparison-v1', status='passed', results=results, torch_version=torch.__version__,
        input_manifest_sha256=hashlib.sha256(args.input.read_bytes()).hexdigest(), production_cutover=False)
    write(args.output/'summary.json', summary)
    return summary


def rejection_checks(args, env):
    root = args.output/'rejections'
    root.mkdir()
    manifest = json.loads(args.input.read_text())
    original = load_file(args.input.parent/manifest['tensors'])
    cases = [('root-index', 'aic-learning.input-index'), ('factor-owner', 'aic-learning.input-owner'),
             ('missing-adam', 'aic-learning.tensor-missing'), ('negative-adam', 'aic-torch.backend'),
             ('rng-state', 'aic-torch.backend'), ('input-hash', 'aic-learning.input-hash')]
    results = []
    for case, code in cases:
        directory = root/case
        directory.mkdir()
        tensors = {n: t.clone() for n, t in original.items()}
        if case == 'root-index':
            tensors['ppo.roots'][0] = 2**31
        elif case == 'factor-owner':
            tensors['ppo.branches.factor_owners'][0] = (int(tensors['ppo.branches.factor_owners'][0])+1)%manifest['ppo_graphs']
        elif case == 'missing-adam':
            del tensors[next(n for n in tensors if n.startswith('adam.actor.') and n.endswith('.mean'))]
        elif case == 'negative-adam':
            tensors[next(n for n in tensors if n.startswith('adam.actor.') and n.endswith('.square'))].fill_(-1)
        elif case == 'rng-state':
            tensors['rng.sampler'] = torch.zeros(8, dtype=torch.uint8)
        archive = directory/manifest['tensors']
        save_file(tensors, archive)
        current = dict(manifest, tensors_sha256='0'*64 if case == 'input-hash' else hashlib.sha256(archive.read_bytes()).hexdigest())
        write(directory/'manifest.json', current)
        result = subprocess.run([str(args.binary.resolve()), 'learning', 'verify-kernel', '--input', str(directory/'manifest.json'),
            '--output', str(directory/'unexpected'), '--steps', '1'], env=env, capture_output=True, text=True)
        assert result.returncode != 0 and code in result.stderr, (case, result.stdout, result.stderr)
        assert not (directory/'unexpected/checkpoint').exists(), case
        results.append(dict(case=case, code=code, rejected=True))
        # Generated rejection inputs are disposable; retain concise outcomes only.
        archive.unlink()
        (directory/'manifest.json').unlink()
        directory.rmdir()
    return results


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--steps', type=int, default=4)
    run(parser.parse_args())
