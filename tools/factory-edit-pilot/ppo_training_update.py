"""One PPO update with frozen recorded behavior and bounded batched execution."""
import gzip
import math
import time

import torch

from full_factory_on_policy import parameter_hash
from ppo_area_model import SCHEMA, clipped_surrogate
from ppo_area_update import sha_bytes
from ppo_training_batch import EXECUTION_SCHEMA, prepare, distribution, critic_values, require
from random_factory_audit import optimizer_hash


def calculate_gae(rewards, values, lam):
    rewards, values = torch.tensor(rewards, dtype=torch.float64), torch.tensor(values, dtype=torch.float64)
    advantages = torch.zeros_like(rewards)
    next_value = next_advantage = 0.
    for index in reversed(range(len(rewards))):
        advantages[index] = rewards[index] + next_value - values[index] + lam * next_advantage
        next_value, next_advantage = values[index], advantages[index]
    return advantages.float(), (advantages + values).float()


def collect_steps(actor, critic, results, config):
    """Load trusted collector output and calculate credit without replay validation."""
    accepted = [r for r in results if r['status'] == 'complete']
    require(len(accepted) >= 2, 'at least two admitted episodes')
    actor_hash, critic_hash = parameter_hash(actor), parameter_hash(critic)
    require(all(r['policy_sha256'] == actor_hash and r['critic_sha256'] == critic_hash for r in accepted),
            'single behavior checkpoint per batch')
    steps, advantages, targets, returns = [], [], [], []
    for result in accepted:
        with gzip.open(result['experience'], 'rb') as stream:
            payload = torch.load(stream, map_location='cpu', weights_only=False)
        require(payload['schema'] == SCHEMA, 'supported experience schema')
        trajectory = payload['steps']
        rewards = [step['reward'] for step in trajectory]
        values = [step['old_value'] for step in trajectory]
        advantage, target = calculate_gae(rewards, values, config['gae_lambda'])
        steps.extend(trajectory)
        advantages.append(advantage)
        targets.append(target)
        returns.append(math.fsum(rewards))
    raw, targets = torch.cat(advantages), torch.cat(targets)
    deviation = raw.std(unbiased=False)
    normalized = (raw - raw.mean()) / deviation.clamp_min(1e-8)
    credit = [dict(raw_advantage=float(a), normalized_advantage=float(n), value_target=float(t))
              for a, n, t in zip(raw, normalized, targets)]
    return steps, normalized, targets, dict(credit=credit, raw_advantage_mean=float(raw.mean()),
        raw_advantage_std=float(deviation), returns=returns, behavior_source='recorded-immutable-trajectory')


def statistics(actor, critic, steps, advantages, targets, config, microbatch, device, guard):
    log_ratios, values = [], []
    with torch.no_grad():
        for offset in range(0, len(steps), microbatch):
            guard()
            sample = steps[offset:offset + microbatch]
            batch = prepare(sample, config, actor.width, device)
            logp, _, _ = distribution(actor, batch, config)
            # Preserve the original diagnostic's float64 subtraction of recorded Python floats.
            log_ratios.extend(float(p) - s['old_log_probability'] for p, s in zip(logp.cpu(), sample))
            values.extend(critic_values(critic, batch).cpu().tolist())
    log_ratio = torch.tensor(log_ratios, dtype=torch.float64)
    ratio, value = log_ratio.exp(), torch.tensor(values)
    epsilon, variance = config['clip_epsilon'], targets.var(unbiased=False)
    return dict(ratio_mean=float(ratio.mean()), ratio_min=float(ratio.min()), ratio_max=float(ratio.max()),
        ratio_p05=float(ratio.quantile(.05)), ratio_p95=float(ratio.quantile(.95)),
        outside_clip_fraction=float(((ratio < 1 - epsilon) | (ratio > 1 + epsilon)).double().mean()),
        objective_clipped_fraction=float((((advantages > 0) & (ratio > 1 + epsilon)) |
                                          ((advantages < 0) & (ratio < 1 - epsilon))).double().mean()),
        approximate_kl=float((ratio - 1 - log_ratio).mean()),
        value_loss=float(.5 * ((value - targets) ** 2).mean()),
        explained_variance=float(1 - (value - targets).var(unbiased=False) / variance) if variance > 1e-12 else None)


def train_minibatch(actor, critic, actor_optimizer, critic_optimizer, steps, advantages, targets,
                    config, microbatch, device, guard=lambda: None):
    require(len(steps) > 0 and 0 < microbatch <= config['minibatch_size'], 'microbatch within minibatch')
    actor_optimizer.zero_grad(set_to_none=True)
    critic_optimizer.zero_grad(set_to_none=True)
    loss_sum = entropy_sum = 0.
    preparation_seconds = computation_seconds = 0.
    for offset in range(0, len(steps), microbatch):
        guard()
        stop = min(len(steps), offset + microbatch)
        began = time.perf_counter()
        batch = prepare(steps[offset:stop], config, actor.width, device)
        advantage = advantages[offset:stop].to(device)
        target = targets[offset:stop].to(device)
        preparation_seconds += time.perf_counter() - began
        began = time.perf_counter()
        logp, entropy, _ = distribution(actor, batch, config)
        surrogate, _ = clipped_surrogate(logp, batch['old_logp'], advantage, config['clip_epsilon'])
        actor_loss = -surrogate - config['entropy_coefficient'] * entropy
        value_loss = .5 * (critic_values(critic, batch) - target).square()
        ((actor_loss.sum() + value_loss.sum()) / len(steps)).backward()
        loss_sum += float(actor_loss.detach().sum())
        entropy_sum += float(entropy.detach().sum())
        computation_seconds += time.perf_counter() - began
    a_norm = torch.nn.utils.clip_grad_norm_(actor.parameters(), config['gradient_clip'])
    c_norm = torch.nn.utils.clip_grad_norm_(critic.parameters(), config['gradient_clip'])
    require(torch.isfinite(a_norm) and torch.isfinite(c_norm), 'finite actor/critic gradients')
    actor_optimizer.step()
    critic_optimizer.step()
    return dict(actor_loss_sum=loss_sum, entropy_sum=entropy_sum, actor_gradient_norm=float(a_norm),
                critic_gradient_norm=float(c_norm), preparation_seconds=preparation_seconds,
                computation_seconds=computation_seconds, optimizer_steps=1)


def update(actor, critic, actor_optimizer, critic_optimizer, shuffle_rng, results, config,
           microbatch=8, guard=lambda: None):
    require(0 < microbatch <= config['minibatch_size'] and config['epochs'] > 0, 'positive execution bounds')
    device = next(actor.parameters()).device
    require(next(critic.parameters()).device == device, 'actor and critic on one learner device')
    before_actor, before_critic = parameter_hash(actor), parameter_hash(critic)
    began = time.perf_counter()
    steps, advantages, targets, accounting = collect_steps(actor, critic, results, config)
    loading_seconds = time.perf_counter() - began
    epochs, optimizer_steps = [], 0
    preparation_seconds = computation_seconds = statistics_seconds = 0.
    for epoch in range(config['epochs']):
        order = torch.randperm(len(steps), generator=shuffle_rng).tolist()
        losses = entropies = 0.
        a_norms, c_norms = [], []
        for offset in range(0, len(order), config['minibatch_size']):
            indices = order[offset:offset + config['minibatch_size']]
            result = train_minibatch(actor, critic, actor_optimizer, critic_optimizer, [steps[i] for i in indices],
                                     advantages[indices], targets[indices], config, microbatch, device, guard)
            losses += result['actor_loss_sum']
            entropies += result['entropy_sum']
            a_norms.append(result['actor_gradient_norm'])
            c_norms.append(result['critic_gradient_norm'])
            preparation_seconds += result['preparation_seconds']
            computation_seconds += result['computation_seconds']
            optimizer_steps += result['optimizer_steps']
        began = time.perf_counter()
        metrics = statistics(actor, critic, steps, advantages, targets, config, microbatch, device, guard)
        statistics_seconds += time.perf_counter() - began
        epochs.append(dict(epoch=epoch + 1, optimizer_steps=optimizer_steps, **metrics,
            actor_loss=losses / len(steps), entropy=entropies / len(steps),
            actor_gradient_norm_max=max(a_norms), critic_gradient_norm_max=max(c_norms)))
        if metrics['approximate_kl'] > config['target_kl']:
            break
    return dict(**accounting, schema=EXECUTION_SCHEMA, selected=len(steps), epochs=epochs,
        optimizer_steps=optimizer_steps, kl_stopped=len(epochs) < config['epochs'],
        policy_before_sha256=before_actor, policy_after_sha256=parameter_hash(actor),
        critic_before_sha256=before_critic, critic_after_sha256=parameter_hash(critic),
        actor_optimizer_sha256=optimizer_hash(actor_optimizer), critic_optimizer_sha256=optimizer_hash(critic_optimizer),
        shuffle_rng_sha256=sha_bytes(shuffle_rng.get_state()),
        timing=dict(experience_loading_and_credit_seconds=loading_seconds, preparation_seconds=preparation_seconds,
                    computation_seconds=computation_seconds, statistics_seconds=statistics_seconds),
        execution=dict(microbatch=microbatch, device=str(device), precision='float32', full_update_executions=1))
