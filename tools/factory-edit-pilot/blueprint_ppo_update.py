"""Persistent-chunk PPO with frozen bootstrap targets and actor-safe critic tails."""
import hashlib
import json
import math

import torch

from blueprint_edit_input import BlueprintActorCritic
from blueprint_ppo_batch import distribution, pack, selected_indices


def require(ok, message):
    if not ok:
        raise ValueError(f'blueprint.ppo: {message}')


def digest(value):
    result = hashlib.sha256()
    def visit(x):
        if isinstance(x, torch.Tensor):
            result.update(str((x.dtype, tuple(x.shape))).encode())
            result.update(x.detach().cpu().contiguous().numpy().tobytes())
        elif isinstance(x, dict):
            for key in sorted(x, key=str):
                result.update(str(key).encode())
                visit(x[key])
        elif isinstance(x, (tuple, list)):
            for item in x:
                visit(item)
        else:
            result.update(json.dumps(x, sort_keys=True).encode())
    visit(value)
    return result.hexdigest()


def learners(seed, config):
    torch.manual_seed(seed)
    model = BlueprintActorCritic(config['width'], config.get('message_rounds', 2))
    actor = [p for name, p in model.named_parameters() if not name.startswith('critic.')]
    critic = list(model.critic.parameters())
    return model, torch.optim.Adam(actor, lr=config['learning_rate']), torch.optim.Adam(critic, lr=config['learning_rate'])


def gae(rewards, values, terminals, bootstrap, gamma, lam):
    require(len(rewards) == len(values) == len(terminals) and bool(rewards), 'nonempty aligned GAE inputs')
    require(all(math.isfinite(float(x)) for x in [*rewards, *values, bootstrap]), 'finite GAE inputs')
    advantages = torch.zeros(len(rewards), dtype=torch.float64)
    next_value, next_advantage = float(bootstrap), 0.
    for i in reversed(range(len(rewards))):
        continuation = 0. if terminals[i] else 1.
        delta = rewards[i] + gamma * continuation * next_value - values[i]
        advantages[i] = delta + gamma * lam * continuation * next_advantage
        next_value, next_advantage = values[i], advantages[i].item()
    targets = advantages + torch.tensor(values, dtype=torch.float64)
    return advantages.float(), targets.float()


def predictions(model, samples):
    batch = pack([s['input'] for s in samples])
    logits, values = model.forward_packed(batch)
    logp, entropy = distribution(logits, batch)
    selected = selected_indices(batch, [s['index'] for s in samples])
    return logp[selected], entropy, values


def credit(samples, frontiers, config):
    require(len({f['stream'] for f in frontiers}) == len(frontiers)
            and {f['stream'] for f in frontiers} == {s['stream'] for s in samples}, 'exact unique frontier coverage')
    raw = torch.zeros(len(samples))
    targets = torch.zeros(len(samples))
    for frontier in frontiers:
        indices = [i for i, s in enumerate(samples) if s['stream'] == frontier['stream']]
        require(bool(indices), 'every frontier has a sampled stream')
        rows = [samples[i] for i in indices]
        require(all(not s['terminal'] for s in rows[:-1]), 'no internal reset inside a chunk')
        require(rows[-1]['terminal'] == frontier['terminal'], 'frontier true termination agrees with last action')
        advantage, target = gae([s['reward'] for s in rows], [s['old_value'] for s in rows],
            [s['terminal'] for s in rows], frontier['bootstrap'], config['gamma'], config['gae_lambda'])
        raw[indices], targets[indices] = advantage, target
    normalized = (raw - raw.mean()) / raw.std(unbiased=False).clamp_min(1e-8)
    return raw, normalized, targets


def statistics(model, samples, targets, config):
    actual, values = [], []
    with torch.no_grad():
        for start in range(0, len(samples), config['minibatch_size']):
            logp, _, predicted = predictions(model, samples[start:start + config['minibatch_size']])
            actual.extend(logp.tolist())
            values.extend(predicted.tolist())
    log_ratio = torch.tensor(actual, dtype=torch.float64) - torch.tensor([s['old_logp'] for s in samples], dtype=torch.float64)
    ratio = log_ratio.exp()
    return dict(approximate_kl=float((ratio - 1 - log_ratio).mean()),
                outside_clip_fraction=float(((ratio - 1).abs() > config['clip_epsilon']).double().mean()),
                value_mae=float((torch.tensor(values) - targets).abs().mean()),
                value_loss=float(.5 * (torch.tensor(values) - targets).square().mean()))


def update(model, actor_optimizer, critic_optimizer, shuffle_rng, samples, frontiers, config):
    before = digest(model.state_dict())
    raw, advantages, targets = credit(samples, frontiers, config)
    # Recompute only from the recorded immutable inputs under the same behavior.
    with torch.no_grad():
        for start in range(0, len(samples), config['minibatch_size']):
            rows = samples[start:start + config['minibatch_size']]
            logp, _, values = predictions(model, rows)
            torch.testing.assert_close(logp, torch.tensor([s['old_logp'] for s in rows]), rtol=1e-5, atol=2e-6)
            torch.testing.assert_close(values, torch.tensor([s['old_value'] for s in rows]), rtol=1e-5, atol=2e-6)
    actor_parameters = [p for name, p in model.named_parameters() if not name.startswith('critic.')]
    critic_parameters = list(model.critic.parameters())
    epochs, stopped = [], False
    tail_actor = tail_optimizer = None
    for epoch in range(config['epochs']):
        order = torch.randperm(len(samples), generator=shuffle_rng).tolist()
        if stopped:
            for p in actor_parameters:
                p.requires_grad_(False)
        try:
            for start in range(0, len(order), config['minibatch_size']):
                indices = order[start:start + config['minibatch_size']]
                rows = [samples[i] for i in indices]
                actor_optimizer.zero_grad(set_to_none=True)
                critic_optimizer.zero_grad(set_to_none=True)
                logp, entropy, values = predictions(model, rows)
                value_loss = .5 * (values - targets[indices]).square().mean()
                if stopped:
                    loss = value_loss
                else:
                    ratio = (logp - torch.tensor([s['old_logp'] for s in rows])).exp()
                    clipped = ratio.clamp(1 - config['clip_epsilon'], 1 + config['clip_epsilon'])
                    surrogate = torch.minimum(ratio * advantages[indices], clipped * advantages[indices])
                    loss = -surrogate.mean() - config['entropy_coefficient'] * entropy.mean() + value_loss
                require(torch.isfinite(loss).item(), 'finite PPO loss')
                loss.backward()
                critic_norm = torch.nn.utils.clip_grad_norm_(critic_parameters, config['gradient_clip'])
                require(torch.isfinite(critic_norm).item(), 'finite critic gradient')
                if not stopped:
                    actor_norm = torch.nn.utils.clip_grad_norm_(actor_parameters, config['gradient_clip'])
                    require(torch.isfinite(actor_norm).item(), 'finite actor gradient')
                    actor_optimizer.step()
                critic_optimizer.step()
        finally:
            for p in actor_parameters:
                p.requires_grad_(True)
        measured = statistics(model, samples, targets, config)
        epochs.append(dict(epoch=epoch + 1, actor_updated=not stopped, **measured))
        if not stopped and measured['approximate_kl'] > config['target_kl']:
            stopped = True
            tail_actor = digest({k: v for k, v in model.state_dict().items() if not k.startswith('critic.')})
            tail_optimizer = digest(actor_optimizer.state_dict())
    if tail_actor is not None:
        require(tail_actor == digest({k: v for k, v in model.state_dict().items() if not k.startswith('critic.')}), 'critic tail preserves actor/encoder')
        require(tail_optimizer == digest(actor_optimizer.state_dict()), 'critic tail preserves actor optimizer')
    return dict(model_before=before, model_after=digest(model.state_dict()),
                actor_optimizer=digest(actor_optimizer.state_dict()), critic_optimizer=digest(critic_optimizer.state_dict()),
                shuffle_rng=digest(shuffle_rng.get_state()), raw_advantage_mean=float(raw.mean()),
                raw_advantage_std=float(raw.std(unbiased=False)), targets_sha256=digest(targets),
                advantages_sha256=digest(advantages), epochs=epochs, actor_epochs=sum(e['actor_updated'] for e in epochs),
                critic_epochs=len(epochs), behavior_recomputed=True, critic_tail_preserves_actor=True,
                nonterminal_bootstraps=sum(not f['terminal'] for f in frontiers), terminal_frontiers=sum(f['terminal'] for f in frontiers))
