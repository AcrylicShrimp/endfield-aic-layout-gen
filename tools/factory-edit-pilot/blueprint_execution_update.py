"""Packed device PPO/SIL with one-batch-ahead CPU preparation and isolated replay RNG."""
from concurrent.futures import ThreadPoolExecutor
import time

import torch

from blueprint_ppo_batch import distribution, pack, selected_indices, to_device
from blueprint_ppo_update import credit, credit_metrics, digest, require


def draw_sequences(sequences, mode, generator, order_generator):
    if not sequences:
        require(mode == 'uniform', 'replay-balance.stratum-empty: balanced replay requires both strata')
        return []
    if mode == 'uniform':
        return [sequences[i] for i in torch.randint(len(sequences), (8,), generator=generator).tolist()]
    require(mode == 'balanced', 'unknown replay sampler')
    selected = []
    for stratum in ('large', 'small'):
        eligible = [s for s in sequences if s['stratum'] == stratum]
        require(bool(eligible), f'replay-balance.stratum-empty: {stratum}')
        selected.extend(eligible[i] for i in torch.randint(len(eligible), (4,), generator=generator).tolist())
    return [selected[i] for i in torch.randperm(8, generator=order_generator).tolist()]


def prepared_rows(rows):
    batch = pack([r['input'] for r in rows])
    return dict(batch=batch, selected=selected_indices(batch, [r['index'] for r in rows]))


def prepare_minibatch(samples, indices, advantages, targets, sequences):
    began = time.perf_counter()
    rows = [samples[i] for i in indices]
    result = dict(ppo=prepared_rows(rows), advantages=advantages[indices], targets=targets[indices],
                  old_logp=torch.tensor([r['old_logp'] for r in rows]), sil=None)
    if sequences:
        labels, weights, groups = [], [], []
        for sequence in sequences:
            stages = {r['stage'] for r in sequence['rows']}
            require(bool(stages), 'replay-balance.source-invalid: empty replay sequence')
            for row in sequence['rows']:
                labels.append(row)
                weights.append(1. / (len(sequences) * len(stages) * sum(r['stage'] == row['stage'] for r in sequence['rows'])))
                groups.append((sequence['origin'], sequence['stratum'], row['stage']))
        result['sil'] = dict(**prepared_rows(labels), returns=torch.tensor([r['return_to_terminal'] for r in labels]),
                             weights=torch.tensor(weights), groups=groups)
    result['prepare_seconds'] = time.perf_counter() - began
    return result


def device_rows(value, device):
    return dict(batch=to_device(value['batch'], device), selected=value['selected'].to(device))


def predict(model, value):
    logits, values = model.forward_packed(value['batch'])
    logp, entropy = distribution(logits, value['batch'])
    return logp[value['selected']], entropy, values


def imitation(model, prepared, device):
    logp, _, values = predict(model, device_rows(prepared, device))
    returns = prepared['returns'].to(device)
    positive = (returns - values.detach()).clamp_min(0)
    loss = (-logp * positive * prepared['weights'].to(device)).sum()
    # Transfer once after the update. No additional diagnostic forward/backward.
    measurements = torch.stack((positive, values.detach(), returns, -logp.detach() * positive), dim=1)
    return loss, measurements


def epoch_statistics(model, batches, samples, targets, config, device):
    actual, predicted = [], []
    with torch.no_grad():
        for batch in batches:
            logp, _, values = predict(model, device_rows(batch, device))
            actual.append(logp)
            predicted.append(values)
        actual = torch.cat(actual).double()
        predicted = torch.cat(predicted)
        log_ratio = actual - torch.tensor([s['old_logp'] for s in samples], device=device, dtype=torch.float64)
        ratio = log_ratio.exp()
        residual = predicted - targets.to(device)
        result = torch.stack(((ratio - 1 - log_ratio).mean(),
            ((ratio - 1).abs() > config['clip_epsilon']).double().mean(),
            residual.abs().mean(), .5 * residual.square().mean())).cpu().tolist()
    return dict(zip(('approximate_kl', 'outside_clip_fraction', 'value_mae', 'value_loss'), result, strict=True))


def summarize_imitation(measurements, groups):
    if not measurements:
        return dict(label_count=0, positive_labels=0, zero_labels=0, groups=[])
    values = torch.cat(measurements).cpu().tolist()
    require(len(values) == len(groups), 'replay diagnostic label coverage')
    totals = {}
    for key, (weight, value, returned, weighted_nll) in zip(groups, values, strict=True):
        row = totals.setdefault(key, dict(origin=key[0], stratum=key[1], stage=key[2], labels=0,
            positive=0, zero=0, weight_sum=0., value_sum=0., return_sum=0., weighted_nll_sum=0.))
        row['labels'] += 1
        row['positive'] += weight > 0
        row['zero'] += weight == 0
        row['weight_sum'] += weight
        row['value_sum'] += value
        row['return_sum'] += returned
        row['weighted_nll_sum'] += weighted_nll
    return dict(label_count=len(values), positive_labels=sum(r['positive'] for r in totals.values()),
                zero_labels=sum(r['zero'] for r in totals.values()), groups=[totals[k] for k in sorted(totals)])


def update_packed(model, actor_optimizer, critic_optimizer, shuffle_rng, samples, frontiers, config,
                  sequences, coefficient, mode, seed, global_ordinal, child_ordinal, device):
    device = torch.device(device)
    require(next(model.parameters()).device == device, 'replay-balance.execution-gate: learner device mismatch')
    require(coefficient >= 0 and all(s['child_admitted_update'] < child_ordinal for s in sequences),
            'replay-balance.source-invalid: current child experience cannot enter imitation')
    require([s['id'] for s in sequences] == sorted({s['id'] for s in sequences}), 'replay identity order and uniqueness')
    generator = torch.Generator().manual_seed(5_000_000 + seed * 1000 + global_ordinal)
    order_generator = torch.Generator().manual_seed(6_000_000 + seed * 1000 + global_ordinal)
    began, before = time.perf_counter(), digest(model.state_dict())
    raw, advantages, targets = credit(samples, frontiers, config)
    actor_parameters = [p for name, p in model.named_parameters() if not name.startswith('critic.')]
    critic_parameters = list(model.critic.parameters())
    statistics_batches = [prepared_rows(samples[i:i + config['minibatch_size']])
                          for i in range(0, len(samples), config['minibatch_size'])]
    epochs, selected_ids, diagnostics, groups, losses = [], [], [], [], []
    stopped, tail_actor, tail_optimizer = False, None, None
    prepared_seconds = 0.
    with ThreadPoolExecutor(max_workers=1, thread_name_prefix='blueprint-prepare') as supply:
        for epoch in range(config['epochs']):
            order = torch.randperm(len(samples), generator=shuffle_rng).tolist()
            tasks = []
            for start in range(0, len(order), config['minibatch_size']):
                selected = draw_sequences(sequences, mode, generator, order_generator) if not stopped and coefficient else []
                if selected:
                    selected_ids.append([s['id'] for s in selected])
                tasks.append((samples, order[start:start + config['minibatch_size']], advantages, targets, selected))
            if stopped:
                for p in actor_parameters:
                    p.requires_grad_(False)
            try:
                pending = supply.submit(prepare_minibatch, *tasks[0])
                for task_index in range(len(tasks)):
                    prepared = pending.result()
                    if task_index + 1 < len(tasks):
                        pending = supply.submit(prepare_minibatch, *tasks[task_index + 1])
                    prepared_seconds += prepared['prepare_seconds']
                    actor_optimizer.zero_grad(set_to_none=True)
                    critic_optimizer.zero_grad(set_to_none=True)
                    logp, entropy, values = predict(model, device_rows(prepared['ppo'], device))
                    value_loss = .5 * (values - prepared['targets'].to(device)).square().mean()
                    loss = value_loss
                    if not stopped:
                        ratio = (logp - prepared['old_logp'].to(device)).exp()
                        clipped = ratio.clamp(1 - config['clip_epsilon'], 1 + config['clip_epsilon'])
                        advantage = prepared['advantages'].to(device)
                        loss = loss - torch.minimum(ratio * advantage, clipped * advantage).mean()
                        loss = loss - config['entropy_coefficient'] * entropy.mean()
                        if prepared['sil'] is not None:
                            teacher, measurement = imitation(model, prepared['sil'], device)
                            loss = loss + coefficient * teacher
                            losses.append(teacher.detach())
                            diagnostics.append(measurement.detach())
                            groups.extend(prepared['sil']['groups'])
                    require(torch.isfinite(loss).item(), 'replay-balance.nonfinite: learner loss')
                    loss.backward()
                    norm = torch.nn.utils.clip_grad_norm_(critic_parameters, config['gradient_clip'])
                    require(torch.isfinite(norm).item(), 'replay-balance.nonfinite: critic gradient')
                    if not stopped:
                        norm = torch.nn.utils.clip_grad_norm_(actor_parameters, config['gradient_clip'])
                        require(torch.isfinite(norm).item(), 'replay-balance.nonfinite: actor gradient')
                        actor_optimizer.step()
                    critic_optimizer.step()
            finally:
                for p in actor_parameters:
                    p.requires_grad_(True)
            measured = epoch_statistics(model, statistics_batches, samples, targets, config, device)
            require(all(torch.isfinite(torch.tensor(v)).item() for v in measured.values()), 'replay-balance.nonfinite: epoch statistics')
            epochs.append(dict(epoch=epoch + 1, actor_updated=not stopped, **measured))
            if not stopped and measured['approximate_kl'] > config['target_kl']:
                stopped = True
                tail_actor = digest({k: v for k, v in model.state_dict().items() if not k.startswith('critic.')})
                tail_optimizer = digest(actor_optimizer.state_dict())
    if tail_actor is not None:
        require(tail_actor == digest({k: v for k, v in model.state_dict().items() if not k.startswith('critic.')})
                and tail_optimizer == digest(actor_optimizer.state_dict()), 'critic tail changed actor parameters or Adam')
    success = dict(coefficient=coefficient, sampler=mode, selected_ids=selected_ids,
        weighted_loss=float(torch.stack(losses).mean()) if losses else None,
        rng_sha256=digest(generator.get_state()), order_rng_sha256=digest(order_generator.get_state()),
        **summarize_imitation(diagnostics, groups))
    return dict(**credit_metrics(raw, advantages, samples, config), success=success,
        model_before=before, model_after=digest(model.state_dict()), actor_optimizer=digest(actor_optimizer.state_dict()),
        critic_optimizer=digest(critic_optimizer.state_dict()), shuffle_rng=digest(shuffle_rng.get_state()),
        raw_advantage_mean=float(raw.mean()), raw_advantage_std=float(raw.std(unbiased=False)),
        targets_sha256=digest(targets), advantages_sha256=digest(advantages), epochs=epochs,
        actor_epochs=sum(e['actor_updated'] for e in epochs), critic_epochs=len(epochs),
        behavior_recomputed=False, critic_tail_preserves_actor=True,
        nonterminal_bootstraps=sum(not f['terminal'] for f in frontiers), terminal_frontiers=sum(f['terminal'] for f in frontiers),
        execution=dict(device=str(device), update_seconds=time.perf_counter() - began, prepare_worker_seconds=prepared_seconds))
