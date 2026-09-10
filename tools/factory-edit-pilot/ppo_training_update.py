"""One PPO update with frozen recorded behavior and bounded batched execution."""
from contextlib import nullcontext
from concurrent.futures import ThreadPoolExecutor
import math
import time

import torch

from full_factory_on_policy import parameter_hash
from ppo_area_model import SCHEMA, clipped_surrogate
from ppo_area_update import sha_bytes
from ppo_training_batch import EXECUTION_SCHEMA, compile_steps, distribution, critic_values, predictions, require
from ppo_actor_critic import optimizer_parameters
from random_factory_audit import optimizer_hash
from ppo_reward import SCHEMA as REWARD_SCHEMA, DEFAULT, validate_episode
from ppo_training_supply import prepare_cpu, prepared_batches
from ppo_training_transfer import move_prepared


def calculate_gae(rewards, values, lam):
    rewards, values = torch.tensor(rewards, dtype=torch.float64), torch.tensor(values, dtype=torch.float64)
    advantages = torch.zeros_like(rewards)
    next_value = next_advantage = 0.
    for index in reversed(range(len(rewards))):
        advantages[index] = rewards[index] + next_value - values[index] + lam * next_advantage
        next_value, next_advantage = values[index], advantages[index]
    return advantages.float(), (advantages + values).float()


def collect_steps(actor, critic, results, config, *, experiences):
    """Validate owned in-memory envelopes, then calculate credit without replay."""
    require(len(results) == len(experiences), 'one experience slot per attempted episode')
    accepted = [r for r in results if r['status'] == 'complete']
    require(len(accepted) >= 2, 'at least two admitted episodes')
    actor_hash, critic_hash = parameter_hash(actor), parameter_hash(critic)
    require(all(r['policy_sha256'] == actor_hash and r['critic_sha256'] == critic_hash for r in accepted),
            'single behavior checkpoint per batch')
    steps, advantages, targets, returns = [], [], [], []
    for result, payload in zip(results, experiences):
        if result['status'] != 'complete':
            require(payload is None, 'rejected generation must not carry experience')
            continue
        require(isinstance(payload, dict), 'complete training result requires an in-memory experience')
        require(payload['schema'] == SCHEMA, 'supported experience schema')
        require(all(payload[key] == result[key] for key in ('policy_sha256', 'critic_sha256', 'input_sha256')) and
                payload['input_contract'] == result['candidate'], 'experience behavior/task/input-contract envelope')
        require(payload.get('reward_schema') == REWARD_SCHEMA and
                payload.get('reward_config') == config.get('reward', DEFAULT), 'experience reward contract')
        trajectory = payload['steps']
        require(len(trajectory) == result['row']['selected'], 'complete experience envelope length')
        validate_episode(trajectory, result['row'], config.get('reward', DEFAULT))
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


def statistics(actor, critic, steps, advantages, targets, config, microbatch, device, guard, executor):
    log_ratios, values = [], []
    timing = dict(wait_seconds=0.0, worker_seconds=0.0, transfer_seconds=0.0)
    with torch.no_grad():
        for indices, chunks, wait, worker in prepared_batches(executor, steps, advantages, targets, config,
                actor.width, microbatch, list(range(len(steps))), microbatch, guard):
            timing['wait_seconds'] += wait
            timing['worker_seconds'] += worker
            sample = [steps[i] for i in indices]
            began = time.perf_counter()
            batch = move_prepared(chunks[0]['batch'], device)
            if device.type == 'cuda':
                torch.cuda.synchronize(device)
            timing['transfer_seconds'] += time.perf_counter() - began
            logp, _, _, predicted = predictions(actor, critic, batch, config)
            # Preserve the original diagnostic's float64 subtraction of recorded Python floats.
            log_ratios.extend(float(p) - s['old_log_probability'] for p, s in zip(logp.cpu(), sample))
            values.extend(predicted.cpu().tolist())
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
        explained_variance=float(1 - (value - targets).var(unbiased=False) / variance) if variance > 1e-12 else None), timing


def prepare_minibatch(steps, advantages, targets, config, width, microbatch, device):
    # Candidate filtering and shape construction happen on CPU, avoiding dynamic
    # GPU indexing synchronization before transferring complete prepared tensors.
    return move_chunks(prepare_cpu(steps, advantages, targets, config, width, microbatch), device)


def move_chunks(chunks, device):
    return [dict(batch=move_prepared(chunk['batch'], device), advantages=chunk['advantages'].to(device),
                 targets=chunk['targets'].to(device)) for chunk in chunks]


def train_prepared(actor, critic, actor_optimizer, critic_optimizer, chunks, config,
                   precision=None, scaler=None, guard=lambda: None):
    actor_optimizer.zero_grad(set_to_none=True)
    critic_optimizer.zero_grad(set_to_none=True)
    count = sum(chunk['batch']['size'] for chunk in chunks)
    loss_sum = entropy_sum = 0.
    device = next(actor.parameters()).device
    for chunk in chunks:
        guard()
        context = torch.autocast(device.type, dtype=precision) if precision is not None else nullcontext()
        with context:
            logp, entropy, _, values = predictions(actor, critic, chunk['batch'], config)
        surrogate, _ = clipped_surrogate(logp, chunk['batch']['old_logp'], chunk['advantages'], config['clip_epsilon'])
        actor_loss = -surrogate - config['entropy_coefficient'] * entropy
        value_loss = .5 * (values - chunk['targets']).square()
        loss = (actor_loss.sum() + value_loss.sum()) / count
        if scaler is None:
            loss.backward()
        else:
            scaler.scale(loss).backward()
        loss_sum += float(actor_loss.detach().sum())
        entropy_sum += float(entropy.detach().sum())
    if scaler is not None:
        scaler.unscale_(actor_optimizer)
        scaler.unscale_(critic_optimizer)
    a_norm = torch.nn.utils.clip_grad_norm_(optimizer_parameters(actor_optimizer), config['gradient_clip'])
    c_norm = torch.nn.utils.clip_grad_norm_(optimizer_parameters(critic_optimizer), config['gradient_clip'])
    require(torch.isfinite(a_norm) & torch.isfinite(c_norm), 'finite actor/critic gradients')
    if scaler is None:
        actor_optimizer.step()
        critic_optimizer.step()
    else:
        scaler.step(actor_optimizer)
        scaler.step(critic_optimizer)
        scaler.update()
    return dict(actor_loss_sum=loss_sum, entropy_sum=entropy_sum, actor_gradient_norm=float(a_norm),
                critic_gradient_norm=float(c_norm), optimizer_steps=1)


def train_minibatch(actor, critic, actor_optimizer, critic_optimizer, steps, advantages, targets,
                    config, microbatch, device, guard=lambda: None):
    require(len(steps) > 0 and 0 < microbatch <= config['minibatch_size'], 'microbatch within minibatch')
    began = time.perf_counter()
    chunks = prepare_minibatch(steps, advantages, targets, config, actor.width, microbatch, device)
    if torch.device(device).type == 'cuda':
        torch.cuda.synchronize(device)
    preparation_seconds = time.perf_counter() - began
    began = time.perf_counter()
    result = train_prepared(actor, critic, actor_optimizer, critic_optimizer, chunks, config, guard=guard)
    if torch.device(device).type == 'cuda':
        torch.cuda.synchronize(device)
    return dict(**result, preparation_seconds=preparation_seconds, computation_seconds=time.perf_counter() - began)


def update(actor, critic, actor_optimizer, critic_optimizer, shuffle_rng, results, config,
           microbatch=8, guard=lambda: None, *, experiences):
    treatment = getattr(critic, 'treatment', None)
    if treatment is not None:
        config = dict(config, actor_critic=treatment)
    require(0 < microbatch <= config['minibatch_size'] and config['epochs'] > 0, 'positive execution bounds')
    device = next(actor.parameters()).device
    require(next(critic.parameters()).device == device, 'actor and critic on one learner device')
    before_actor, before_critic = parameter_hash(actor), parameter_hash(critic)
    began = time.perf_counter()
    steps, advantages, targets, accounting = collect_steps(actor, critic, results, config, experiences=experiences)
    credit_seconds = time.perf_counter() - began
    began = time.perf_counter()
    steps = compile_steps(steps, config)
    static_preparation_seconds = time.perf_counter() - began
    epochs, optimizer_steps = [], 0
    tail = dict(epochs=[], optimizer_steps=0, seconds=0.)
    preparation_seconds = computation_seconds = statistics_seconds = 0.
    supply_timing = dict(training_input_wait_seconds=0.0, training_cpu_preparation_seconds=0.0,
                        training_transfer_seconds=0.0, statistics_wait_seconds=0.0,
                        statistics_worker_seconds=0.0, statistics_transfer_seconds=0.0)
    with ThreadPoolExecutor(max_workers=1, thread_name_prefix='ppo-input') as executor:
        for epoch in range(config['epochs']):
            order = torch.randperm(len(steps), generator=shuffle_rng).tolist()
            losses = entropies = 0.
            a_norms, c_norms = [], []
            for _, cpu_chunks, wait, worker in prepared_batches(executor, steps, advantages, targets, config,
                    actor.width, microbatch, order, config['minibatch_size'], guard):
                supply_timing['training_input_wait_seconds'] += wait
                supply_timing['training_cpu_preparation_seconds'] += worker
                began = time.perf_counter()
                chunks = move_chunks(cpu_chunks, device)
                if device.type == 'cuda':
                    torch.cuda.synchronize(device)
                transfer = time.perf_counter() - began
                supply_timing['training_transfer_seconds'] += transfer
                preparation_seconds += wait + transfer
                began = time.perf_counter()
                result = train_prepared(actor, critic, actor_optimizer, critic_optimizer, chunks, config, guard=guard)
                if device.type == 'cuda':
                    torch.cuda.synchronize(device)
                computation_seconds += time.perf_counter() - began
                losses += result['actor_loss_sum']
                entropies += result['entropy_sum']
                a_norms.append(result['actor_gradient_norm'])
                c_norms.append(result['critic_gradient_norm'])
                optimizer_steps += result['optimizer_steps']
            began = time.perf_counter()
            metrics, stats_timing = statistics(actor, critic, steps, advantages, targets, config,
                                              microbatch, device, guard, executor)
            statistics_seconds += time.perf_counter() - began
            for key, value in stats_timing.items():
                supply_timing['statistics_' + key] += value
            epochs.append(dict(epoch=epoch + 1, optimizer_steps=optimizer_steps, **metrics,
                actor_loss=losses / len(steps), entropy=entropies / len(steps),
                actor_gradient_norm_max=max(a_norms), critic_gradient_norm_max=max(c_norms)))
            if metrics['approximate_kl'] > config['target_kl']:
                break
        if treatment is not None and treatment['critic_after_kl'] and len(epochs) < config['epochs']:
            from ppo_critic_tail import finish
            tail = finish(actor, critic, actor_optimizer, critic_optimizer, shuffle_rng, steps, advantages,
                          targets, config, len(epochs), microbatch, device, guard, executor, statistics)
    result = dict(**accounting, schema=EXECUTION_SCHEMA, selected=len(steps), epochs=epochs,
        optimizer_steps=optimizer_steps, kl_stopped=len(epochs) < config['epochs'],
        policy_before_sha256=before_actor, policy_after_sha256=parameter_hash(actor),
        critic_before_sha256=before_critic, critic_after_sha256=parameter_hash(critic),
        actor_optimizer_sha256=optimizer_hash(actor_optimizer), critic_optimizer_sha256=optimizer_hash(critic_optimizer),
        shuffle_rng_sha256=sha_bytes(shuffle_rng.get_state()),
        timing=dict(experience_validation_and_credit_seconds=credit_seconds, static_preparation_seconds=static_preparation_seconds,
                    preparation_seconds=preparation_seconds,
                    computation_seconds=computation_seconds, statistics_seconds=statistics_seconds, **supply_timing),
        execution=dict(microbatch=microbatch, device=str(device), precision='float32', full_update_executions=1))
    if treatment is not None:
        result.update(actor_critic=treatment, actor_epochs=len(epochs),
                      critic_epochs=len(epochs) + len(tail['epochs']), actor_optimizer_steps=optimizer_steps,
                      critic_optimizer_steps=optimizer_steps + tail['optimizer_steps'], critic_tail=tail)
        result['timing']['critic_tail_seconds'] = tail['seconds']
    return result
