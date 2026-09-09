"""Bounded reference minibatch utilities for profiling and optimized-execution probes."""
from contextlib import nullcontext
from collections import Counter
from copy import deepcopy
import math
import os
from pathlib import Path

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')

import numpy as np
import torch
from torch import nn

from direct_area_study import checked
from finish_ppo_confirmation import load_confirmation
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from ppo_area_model import clipped_surrogate
from ppo_area_study import learners
from ppo_area_update import collect_steps
from random_factory_collection import check_sources
from random_factory_generation import read


def require(condition, code, message):
    if not condition:
        raise ValueError(f'ppo-precision.{code}: {message}')


def resident(value, device):
    if isinstance(value, np.ndarray):
        return torch.as_tensor(value, device=device)
    if isinstance(value, dict):
        return {key: resident(item, device) for key, item in value.items()}
    return value


def fp32_output(module, inputs, output):
    return output.float()


def selection_tensors(step, config, device):
    counts = Counter(kind for kind, allowed in zip(step['kinds'], step['allowed']) if allowed)
    adjustment = [-math.log(counts[kind]) if allowed and config['balanced'] else 0
                  for kind, allowed in zip(step['kinds'], step['allowed'])]
    return dict(mask=torch.tensor(step['allowed'], dtype=torch.bool, device=device),
                adjustment=torch.tensor(adjustment, dtype=torch.float32, device=device),
                index=torch.tensor(step['index'], device=device))


def vector(model, gradients=False):
    return torch.cat([(parameter.grad if parameter.grad is not None else torch.zeros_like(parameter)).detach().float().cpu().flatten()
                      if gradients else parameter.detach().float().cpu().flatten()
                      for parameter in model.parameters()])


def difference(actual, reference):
    delta = actual.double() - reference.double()
    norm = reference.double().norm()
    denominator = actual.double().norm() * norm
    return dict(max_absolute=float(delta.abs().max()), rms=float(delta.square().mean().sqrt()),
                relative_l2=float(delta.norm() / norm) if norm > 0 else None,
                cosine=float(torch.dot(actual.double(), reference.double()) / denominator) if denominator > 0 else None)


def synchronize(device):
    if device.type == 'cuda':
        torch.cuda.synchronize(device)


def make_cell(spec, seed, config, checkpoint, device):
    actor, critic, aopt, copt, _ = learners(spec, seed, config)
    actor.load_state_dict(checkpoint['policy'])
    critic.load_state_dict(checkpoint['critic'])
    actor.to(device)
    critic.to(device)
    aopt.load_state_dict(deepcopy(checkpoint['actor_optimizer']))
    copt.load_state_dict(deepcopy(checkpoint['critic_optimizer']))
    for model in (actor, critic):
        for module in model.modules():
            if isinstance(module, (nn.Linear, nn.Conv2d)):
                module.register_forward_hook(fp32_output)
    return actor, critic, aopt, copt


def minibatch(models, steps, advantages, targets, config, device, dtype, scaler, capture):
    actor, critic, aopt, copt = models
    aopt.zero_grad(set_to_none=True)
    copt.zero_grad(set_to_none=True)
    losses, logps, values = [], [], []
    for index, step in enumerate(steps):
        selection = step.get('_selection')
        if selection is None:
            selection = selection_tensors(step, config, device)
        context = torch.autocast('cuda', dtype=dtype) if dtype is not None else nullcontext()
        with context:
            logits = actor(step['actor_observation']).float() / config['temperature']
            value = critic(step['critic_observation'], selection['mask']).float()
        adjusted = (logits + selection['adjustment']).masked_fill(~selection['mask'], -torch.inf)
        dist = torch.distributions.Categorical(logits=adjusted)
        logp = dist.log_prob(selection['index'])
        surrogate, _ = clipped_surrogate(logp, step['old_log_probability'], advantages[index], config['clip_epsilon'])
        # Categorical.entropy clamps masked -inf to finfo.min. Scaling its
        # gradient can overflow that finite sentinel even in FP32. Zero masked
        # log terms before multiplication; their probability is exactly zero.
        entropy = -(dist.probs * dist.logits.masked_fill(~selection['mask'], 0)).sum()
        actor_loss = -surrogate - config['entropy_coefficient'] * entropy
        value_loss = .5 * (value - targets[index]).square()
        a_loss, c_loss = actor_loss / len(steps), value_loss / len(steps)
        if scaler is None:
            a_loss.backward()
            c_loss.backward()
        else:
            scaler.scale(a_loss).backward()
            scaler.scale(c_loss).backward()
        if capture:
            losses.append(torch.stack((actor_loss.detach(), value_loss.detach())))
            logps.append(logp.detach())
            values.append(value.detach())
    if scaler is not None:
        scaler.unscale_(aopt)
        scaler.unscale_(copt)
    gradients = {name: vector(model, True) for name, model in (('actor', actor), ('critic', critic))} if capture else None
    a_norm = torch.nn.utils.clip_grad_norm_(actor.parameters(), config['gradient_clip'])
    c_norm = torch.nn.utils.clip_grad_norm_(critic.parameters(), config['gradient_clip'])
    require(bool(torch.isfinite(a_norm) & torch.isfinite(c_norm)), 'nonfinite-gradient', str(device))
    calls = [0, 0]
    def counted(index):
        def hook(optimizer, args, kwargs):
            calls[index] += 1
        return hook
    handles = [optimizer.register_step_post_hook(counted(index)) for index, optimizer in enumerate((aopt, copt))]
    scale_before = scaler.get_scale() if scaler is not None else None
    try:
        if scaler is None:
            aopt.step()
            copt.step()
        else:
            scaler.step(aopt)
            scaler.step(copt)
            scaler.update()
    finally:
        for handle in handles:
            handle.remove()
    require(calls == [1, 1], 'skipped-update', f'{device}: optimizer step calls {calls}')
    details = dict(actual_optimizer_steps=calls, scale_before=scale_before,
                   scale_after=scaler.get_scale() if scaler is not None else None)
    if capture:
        details.update(actor_gradient_norm=float(a_norm), critic_gradient_norm=float(c_norm),
                       mean_losses=torch.stack(losses).mean(0).cpu().tolist())
        details['_vectors'] = dict(gradients=gradients, logps=torch.stack(logps).cpu(), values=torch.stack(values).cpu(),
                                  parameters={name: vector(model) for name, model in (('actor', actor), ('critic', critic))})
    return details


def reference_workload(screen):
    protocol_path = screen / 'protocol.json'
    protocol = read(protocol_path)
    check_sources(protocol)
    report_path = screen / 'confirmation.json'
    report = load_confirmation(screen)
    require(not protocol['smoke'], 'prerequisite', str(report_path))
    state = next(s for s in report['seeds'] if s['seed'] == 17)
    group = checked(state['groups'][7])
    previous = checked(state['groups'][6])['checkpoint']
    require(sha(previous['path']) == previous['sha256'], 'checkpoint-hash', previous['path'])
    checkpoint = torch.load(previous['path'], map_location='cpu', weights_only=False)
    spec, config = protocol['spec'], dict(protocol['spec']['selection'], **protocol['controls']['ppo'])
    actor, critic, _, _, rng = learners(spec, 17, config)
    actor.load_state_dict(checkpoint['policy'])
    critic.load_state_dict(checkpoint['critic'])
    require(parameter_hash(actor) == group['policy_before_sha256'] and parameter_hash(critic) == group['critic_before_sha256'],
            'behavior-identity', previous['path'])
    steps, advantages, targets, accounting = collect_steps(actor, critic, group['results'], config)
    require(all(accounting[key] == group[key] for key in accounting), 'credit-identity', str(state['groups'][7]['path']))
    rng.set_state(checkpoint['shuffle_rng'])
    indices = torch.randperm(len(steps), generator=rng)[:config['minibatch_size']].tolist()
    chosen = [steps[index] for index in indices]
    advantages, targets = advantages[indices], targets[indices]
    return dict(spec=spec, config=config, checkpoint=checkpoint, steps=chosen, advantages=advantages, targets=targets,
                protocol=protocol, group_record=group,
                identity=dict(protocol_sha256=sha(protocol_path), confirmation_sha256=sha(report_path),
                              checkpoint=previous, group=state['groups'][7], indices=indices))
