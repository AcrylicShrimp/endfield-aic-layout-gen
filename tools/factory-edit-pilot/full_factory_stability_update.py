"""Explicit collection/update counters and transactional stability-study updates."""
from copy import deepcopy
from pathlib import Path

import torch

from full_factory_large_learning import unpack
from full_factory_on_policy import parameter_hash, relative_returns, replay_gradients
from full_factory_policy import FullFactoryPolicy, SCHEMA


CONTRACT = 'on-policy-update-stability-v1'


def require(ok, message):
    if not ok:
        raise ValueError(f'on-policy.stability-contract: {message}')


def checkpoint_value(protocol, protocol_hash, arm, model, optimizer, groups, updates):
    require(type(groups) is int and type(updates) is int and 0 <= updates <= groups, 'invalid counters')
    require(isinstance(model, FullFactoryPolicy) and (model.depth, model.width) == (protocol['depth'], protocol['width']),
            'policy architecture')
    require(type(optimizer) is torch.optim.Adam, 'expected Adam optimizer')
    parameters = list(model.parameters())
    require(all(torch.isfinite(v).all() for v in parameters), 'nonfinite parameters')
    require(len(optimizer.param_groups) == 1 and optimizer.param_groups[0]['lr'] == arm['learning_rate'], 'optimizer learning rate')
    require(len(optimizer.state) == (len(parameters) if updates else 0), 'optimizer state count')
    require(not updates or set(optimizer.state) == set(parameters), 'optimizer state membership')
    require([id(v) for v in optimizer.param_groups[0]['params']] == [id(v) for v in parameters], 'optimizer parameter order')
    for parameter in parameters:
        if not updates:
            continue
        state = optimizer.state[parameter]
        require(float(state['step']) == updates, 'optimizer step disagrees with applied-update count')
        for name in ('exp_avg', 'exp_avg_sq'):
            require(state[name].shape == parameter.shape and torch.isfinite(state[name]).all(), f'invalid {name}')
        require((state['exp_avg_sq'] >= 0).all(), 'negative second moment')
    return dict(schema=SCHEMA, learning_contract=CONTRACT, depth=protocol['depth'], width=protocol['width'],
                arm=arm['name'], groups_collected=groups, optimizer_updates=updates,
                policy=deepcopy(model.state_dict()), optimizer=deepcopy(optimizer.state_dict()),
                rng=torch.get_rng_state().clone(), policy_sha256=parameter_hash(model), protocol_sha256=protocol_hash)


def update_group(protocol, case, arm, model, optimizer, rows, guard=lambda: None):
    """Return one candidate update, rolling back all model/Adam/RNG state on failure."""
    config = protocol['config']
    require(type(optimizer) is torch.optim.Adam, 'expected Adam optimizer')
    require(type(arm['skip_flat']) is bool, 'skip_flat must be Boolean')
    require(len(rows) == config['group_episodes'], 'incomplete episode group')
    require(len({r['bank'] for r in rows}) == len(rows) and len({r['directory'] for r in rows}) == len(rows),
            'duplicate episode seed or directory')
    require(all(r['budget_complete'] is True and r['selected'] == config['horizon'] and r['case'] == case['id'] for r in rows),
            'incomplete or mismatched episode')
    require(len(optimizer.param_groups) == 1 and optimizer.param_groups[0]['lr'] == arm['learning_rate'], 'optimizer learning rate')
    require([id(v) for v in optimizer.param_groups[0]['params']] == [id(v) for v in model.parameters()], 'optimizer parameter order')
    require(all(torch.isfinite(v).all() for v in model.parameters()), 'nonfinite behavior parameters')
    before_hash = parameter_hash(model)
    for row in rows:
        guard()
        episode = unpack(Path(row['directory'])/'episode.json.gz')
        require(episode['policy_sha256'] == before_hash and episode['sample_seed'] == row['bank'],
                f'behavior policy or seed mismatch: {row["directory"]}')
        require(len(episode['trace']) == row['selected'] and episode['best_metrics'] == row['best_metrics'],
                f'episode summary mismatch: {row["directory"]}')
    returns = relative_returns([r['best_metrics']['score'] for r in rows])
    flat = not any(returns)
    if arm['skip_flat'] and flat:
        guard()
        return dict(returns=returns, flat_returns=True, optimizer_step_applied=False, update_reason='flat-group-skipped',
                    replayed_actions=0, reward_gradient_norm=0.0, entropy_gradient_norm=None,
                    combined_gradient_norm=None, pre_clip_gradient_norm=None, reward_loss=None, entropy_loss=None,
                    max_log_probability_error=None, behavior_policy_sha256=before_hash,
                    policy_before_sha256=before_hash, policy_after_sha256=before_hash, parameter_changed=False)
    before_model = deepcopy(model.state_dict())
    before_optimizer = deepcopy(optimizer.state_dict())
    before_rng = torch.get_rng_state().clone()
    before_gradients = [None if v.grad is None else v.grad.detach().clone() for v in model.parameters()]
    try:
        optimizer.zero_grad(set_to_none=True)
        gradient = replay_gradients(protocol, case, model, rows, returns, guard, unpack)
        guard()
        clipped = float(torch.nn.utils.clip_grad_norm_(model.parameters(), config['gradient_clip']))
        require(torch.isfinite(torch.tensor(clipped)), 'nonfinite gradient clipping')
        optimizer.step()
        require(all(torch.isfinite(v).all() for v in model.parameters()), 'nonfinite updated parameters')
        guard()
        after_hash = parameter_hash(model)
        return dict(**gradient, returns=returns, optimizer_step_applied=True, update_reason='ordinary-group-update',
                    pre_clip_gradient_norm=clipped, policy_before_sha256=before_hash,
                    policy_after_sha256=after_hash, parameter_changed=after_hash != before_hash)
    except BaseException:
        model.load_state_dict(before_model, strict=True)
        optimizer.load_state_dict(before_optimizer)
        torch.set_rng_state(before_rng)
        for parameter, gradient in zip(model.parameters(), before_gradients):
            parameter.grad = gradient
        raise
