"""Frozen-trajectory PPO minibatches with value fitting, GAE and measured ratio diagnostics."""
import gzip
import io
import math
from pathlib import Path

import torch

from full_factory_input_contracts import observation_hash
from full_factory_on_policy import parameter_hash, probability_hash, gradient_norm
from full_factory_screen import sha
from ppo_area_model import SCHEMA, actor_distribution, clipped_surrogate, gae, require
from random_factory_audit import optimizer_hash


def save_experience(path, payload):
    temporary = Path(path).with_suffix('.tmp')
    with gzip.open(temporary, 'wb', compresslevel=1) as stream:
        torch.save(payload, stream)
    temporary.replace(path)


def load_experience(result):
    require(sha(result['experience']) == result['experience_sha256'], 'experience file hash')
    # Audit archives are read into a seekable buffer once. A Torch ZIP reader
    # seeking directly in GzipFile repeatedly inflates earlier archive bytes.
    payload = torch.load(io.BytesIO(gzip.decompress(Path(result['experience']).read_bytes())),
                         map_location='cpu', weights_only=False)
    require(payload['schema'] == SCHEMA and payload['policy_sha256'] == result['policy_sha256'] and
            payload['critic_sha256'] == result['critic_sha256'] and
            payload['input_sha256'] == result['input_sha256'], 'experience schema/checkpoint/input identity')
    require(len(payload['steps']) == result['row']['selected'], 'complete experience horizon')
    return payload


def collect_steps(actor, critic, results, config):
    accepted = [r for r in results if r['status'] == 'complete']
    require(len(accepted) >= 2, 'at least two admitted episodes')
    actor_hash, critic_hash = parameter_hash(actor), parameter_hash(critic)
    require(all(r['policy_sha256'] == actor_hash and r['critic_sha256'] == critic_hash for r in accepted), 'same actor/critic batch')
    steps, advantages, targets, episode_returns = [], [], [], []
    with torch.no_grad():
        for result in accepted:
            payload = load_experience(result)
            trajectory = payload['steps']
            rewards, values = [], []
            for step in trajectory:
                require(observation_hash(step['actor_observation']) == step['observation_sha256'] and
                        observation_hash(step['critic_observation']) == step['critic_observation_sha256'], 'observation hash')
                require(0 <= step['index'] < len(step['allowed']) and step['allowed'][step['index']], 'sampled action allowed')
                dist = actor_distribution(actor, step, config)
                logp = float(dist.log_prob(torch.tensor(step['index'])))
                value = float(critic(step['critic_observation'], step['allowed']))
                require(logp == step['old_log_probability'] and probability_hash(dist.probs) == step['probabilities_sha256'],
                        'behavior distribution recomputation')
                require(value == step['old_value'] and math.isfinite(value), 'behavior critic recomputation')
                require(math.isfinite(step['reward']) and 0 <= step['reward'] <= 1, 'finite direct reward')
                rewards.append(step['reward'])
                values.append(value)
            initial, best = (result['row'][k]['score'][0] for k in ('initial_metrics', 'best_metrics'))
            require(math.isclose(math.fsum(rewards), (initial - best) / initial, abs_tol=1e-12), 'experience reward sum')
            advantage, target = gae(rewards, values, config['gae_lambda'])
            steps.extend(trajectory)
            advantages.append(advantage)
            targets.append(target)
            episode_returns.append(math.fsum(rewards))
    raw = torch.cat(advantages)
    deviation = raw.std(unbiased=False)
    normalized = (raw - raw.mean()) / deviation.clamp_min(1e-8)
    require(torch.isfinite(normalized).all(), 'finite normalized advantages')
    joined_targets = torch.cat(targets)
    credit = [dict(raw_advantage=float(a), normalized_advantage=float(n), value_target=float(t))
              for a, n, t in zip(raw, normalized, joined_targets)]
    return steps, normalized, joined_targets, dict(credit=credit, raw_advantage_mean=float(raw.mean()),
        raw_advantage_std=float(deviation), returns=episode_returns, behavior_ratios_equal_one=True)


def ratio_statistics(actor, critic, steps, advantages, targets, config):
    log_ratios, values = [], []
    with torch.no_grad():
        for step in steps:
            distribution = actor_distribution(actor, step, config)
            log_ratios.append(float(distribution.log_prob(torch.tensor(step['index']))) - step['old_log_probability'])
            values.append(float(critic(step['critic_observation'], step['allowed'])))
    log_ratio = torch.tensor(log_ratios, dtype=torch.float64)
    ratio = log_ratio.exp()
    require(torch.isfinite(ratio).all(), 'finite probability ratios')
    value = torch.tensor(values)
    require(torch.isfinite(value).all(), 'finite updated critic values')
    epsilon = config['clip_epsilon']
    variance = targets.var(unbiased=False)
    return dict(ratio_mean=float(ratio.mean()), ratio_min=float(ratio.min()), ratio_max=float(ratio.max()),
        ratio_p05=float(ratio.quantile(.05)), ratio_p95=float(ratio.quantile(.95)),
        outside_clip_fraction=float(((ratio < 1 - epsilon) | (ratio > 1 + epsilon)).double().mean()),
        objective_clipped_fraction=float((((advantages > 0) & (ratio > 1 + epsilon)) |
                                          ((advantages < 0) & (ratio < 1 - epsilon))).double().mean()),
        approximate_kl=float((ratio - 1 - log_ratio).mean()),
        value_loss=float(.5 * ((value - targets) ** 2).mean()),
        explained_variance=float(1 - (value - targets).var(unbiased=False) / variance) if variance > 1e-12 else None)


def update(actor, critic, actor_optimizer, critic_optimizer, shuffle_rng, results, config, guard=lambda: None):
    before_actor, before_critic = parameter_hash(actor), parameter_hash(critic)
    steps, advantages, targets, accounting = collect_steps(actor, critic, results, config)
    require(len(steps) > 0 and config['minibatch_size'] > 0, 'positive minibatch size')
    epochs, optimizer_steps = [], 0
    for epoch in range(config['epochs']):
        order = torch.randperm(len(steps), generator=shuffle_rng).tolist()
        losses, actor_norms, critic_norms, entropies = [], [], [], []
        for offset in range(0, len(order), config['minibatch_size']):
            guard()
            indices = order[offset:offset + config['minibatch_size']]
            actor_optimizer.zero_grad(set_to_none=True)
            critic_optimizer.zero_grad(set_to_none=True)
            # Backward one observation at a time, but hold both networks fixed until
            # the whole minibatch gradient is assembled. This bounds graph memory.
            for index in indices:
                step = steps[index]
                dist = actor_distribution(actor, step, config)
                logp = dist.log_prob(torch.tensor(step['index']))
                surrogate, _ = clipped_surrogate(logp, step['old_log_probability'], advantages[index], config['clip_epsilon'])
                actor_loss = -surrogate - config['entropy_coefficient'] * dist.entropy()
                value = critic(step['critic_observation'], step['allowed'])
                value_loss = .5 * (value - targets[index]).square()
                (actor_loss / len(indices)).backward()
                (value_loss / len(indices)).backward()
                losses.append(float(actor_loss.detach()))
                entropies.append(float(dist.entropy().detach()))
            a_norm = torch.nn.utils.clip_grad_norm_(actor.parameters(), config['gradient_clip'])
            c_norm = torch.nn.utils.clip_grad_norm_(critic.parameters(), config['gradient_clip'])
            require(torch.isfinite(a_norm) and torch.isfinite(c_norm), 'finite actor/critic gradients')
            actor_optimizer.step()
            critic_optimizer.step()
            actor_norms.append(float(a_norm))
            critic_norms.append(float(c_norm))
            optimizer_steps += 1
        statistics = ratio_statistics(actor, critic, steps, advantages, targets, config)
        epochs.append(dict(epoch=epoch + 1, optimizer_steps=optimizer_steps, **statistics,
            actor_loss=sum(losses) / len(losses), entropy=sum(entropies) / len(entropies),
            actor_gradient_norm_max=max(actor_norms), critic_gradient_norm_max=max(critic_norms)))
        if statistics['approximate_kl'] > config['target_kl']:
            break
    return dict(**accounting, schema=SCHEMA, selected=len(steps), epochs=epochs, optimizer_steps=optimizer_steps,
        kl_stopped=len(epochs) < config['epochs'], policy_before_sha256=before_actor,
        policy_after_sha256=parameter_hash(actor), critic_before_sha256=before_critic,
        critic_after_sha256=parameter_hash(critic), actor_optimizer_sha256=optimizer_hash(actor_optimizer),
        critic_optimizer_sha256=optimizer_hash(critic_optimizer),
        shuffle_rng_sha256=sha_bytes(shuffle_rng.get_state()))


def sha_bytes(tensor):
    import hashlib
    return hashlib.sha256(tensor.numpy().tobytes()).hexdigest()
