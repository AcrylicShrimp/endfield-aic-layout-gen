"""PPO plus positive-return-advantage weighted imitation of self-generated successes."""
import torch

from blueprint_ppo_update import credit, credit_metrics, digest, predictions, require, statistics, update


def imitation_loss(model, sequences):
    rows = [row for sequence in sequences for row in sequence['rows']]
    logp, _, values = predictions(model, rows)
    returns = torch.tensor([r['return_to_terminal'] for r in rows])
    advantages = (returns - values.detach()).clamp_min(0)
    losses, offset = [], 0
    for sequence in sequences:
        stages = []
        for stage in sorted({row['stage'] for row in sequence['rows']}):
            indices = [offset + i for i, row in enumerate(sequence['rows']) if row['stage'] == stage]
            stages.append((-logp[indices] * advantages[indices]).mean())
        losses.append(torch.stack(stages).mean())
        offset += len(sequence['rows'])
    loss = torch.stack(losses).mean()
    return loss, dict(labels=len(rows), positive=int((advantages > 0).sum()),
        mean_weight=float(advantages.mean()), max_weight=float(advantages.max()),
        mean_return=float(returns.mean()), mean_value=float(values.detach().mean()))


def update_success(model, actor_optimizer, critic_optimizer, shuffle_rng, samples, frontiers, config,
              config_success, sequences, seed, ordinal):
    coefficient = config_success['coefficient']
    require(coefficient >= 0, 'blueprint.success-update: nonnegative coefficient')
    require(all(s['admitted_update'] < ordinal for s in sequences), 'blueprint.success-update: prior episodes only')
    if coefficient == 0 or not sequences:
        result = update(model, actor_optimizer, critic_optimizer, shuffle_rng, samples, frontiers, config)
        result['success'] = dict(coefficient=coefficient, selected_ids=[], label_count=0,
            positive_labels=0, weighted_loss=None, reason='zero_coefficient' if coefficient == 0 else 'empty_buffer')
        return result
    generator = torch.Generator().manual_seed(5_000_000 + seed * 1000 + ordinal)
    selected_ids, losses, diagnostics = [], [], []
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
                    choices = torch.randint(len(sequences), (config_success['sequences_per_minibatch'],), generator=generator).tolist()
                    selected = [sequences[i] for i in choices]
                    selected_ids.append([d['id'] for d in selected])
                    teacher_loss, diagnostic = imitation_loss(model, selected)
                    diagnostics.append(diagnostic)
                    losses.append(float(teacher_loss.detach()))
                    loss = loss + coefficient * teacher_loss
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
    return dict(**credit_metrics(raw, advantages, samples, config), success=dict(coefficient=coefficient, selected_ids=selected_ids,
                    label_count=sum(d['labels'] for d in diagnostics), positive_labels=sum(d['positive'] for d in diagnostics),
                    weighted_loss=sum(losses) / len(losses), diagnostics=diagnostics,
                    rng_sha256=digest(generator.get_state())), model_before=before, model_after=digest(model.state_dict()),
                actor_optimizer=digest(actor_optimizer.state_dict()), critic_optimizer=digest(critic_optimizer.state_dict()),
                shuffle_rng=digest(shuffle_rng.get_state()), raw_advantage_mean=float(raw.mean()),
                raw_advantage_std=float(raw.std(unbiased=False)), targets_sha256=digest(targets),
                advantages_sha256=digest(advantages), epochs=epochs, actor_epochs=sum(e['actor_updated'] for e in epochs),
                critic_epochs=len(epochs), behavior_recomputed=True, critic_tail_preserves_actor=True,
                nonterminal_bootstraps=sum(not f['terminal'] for f in frontiers), terminal_frontiers=sum(f['terminal'] for f in frontiers))
