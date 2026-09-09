"""Validated direct area rewards and constant-memory causal policy gradients."""
import math

import torch

from full_factory_on_policy import parameter_hash, gradient_norm
from full_factory_screen import sha
from random_factory_collection import tensor_hash

SCHEMA = 'direct-best-area-reward-to-go-v1'


def require(condition, message):
    if not condition:
        raise ValueError(f'direct-reward.contract: {message}')


def reward_sequence(initial_area, best_areas):
    require(type(initial_area) is int and initial_area > 0, 'positive integer initial area')
    require(len(best_areas) >= 2 and all(type(a) is int and 0 < a <= initial_area for a in best_areas),
            'best areas within initial area')
    require(all(a >= b for a, b in zip(best_areas, best_areas[1:])), 'best area cannot increase')
    return [(a - b) / initial_area for a, b in zip(best_areas, best_areas[1:])]


class PrefixCredit:
    """Sum r_u * prefix-score-gradient_u; exactly reward-to-go in real arithmetic."""
    def __init__(self, model, horizon):
        require(type(horizon) is int and horizon > 0, 'positive horizon')
        self.horizon = horizon
        self.prefix, self.weighted, self.entropy = ([torch.zeros_like(p) for p in model.parameters()] for _ in range(3))
        self.mean_entropy, self.steps, self.rewards = 0.0, 0, []

    def select(self, gradients):
        require(self.steps == len(self.rewards) and self.steps < self.horizon, 'one selection per reward')
        require(len(gradients) == len(self.prefix), 'gradient shape')
        for target, value in zip(self.prefix, gradients):
            if value is not None:
                target.add_(value.detach(), alpha=1 / self.horizon)
        self.steps += 1

    def reward(self, value):
        require(self.steps == len(self.rewards) + 1 and math.isfinite(value) and 0 <= value <= 1,
                'finite reward after selection')
        for target, prefix in zip(self.weighted, self.prefix):
            target.add_(prefix, alpha=value)
        self.rewards.append(value)

    def payload(self):
        require(self.steps == len(self.rewards) == self.horizon, 'complete reward horizon')
        return dict(schema=SCHEMA, reward_gradients=self.weighted, entropy_gradients=self.entropy,
                    mean_entropy=self.mean_entropy, rewards=self.rewards, horizon=self.horizon,
                    reward_sum=math.fsum(self.rewards))


def payload_hash(payload):
    require(payload.get('schema') == SCHEMA, 'payload schema')
    return tensor_hash(payload['reward_gradients'] + payload['entropy_gradients'])


def checked_payload(result, model=None):
    require(sha(result['gradients']) == result['gradients_sha256'], 'gradient file identity')
    saved = torch.load(result['gradients'], map_location='cpu', weights_only=False)
    require(saved['policy_sha256'] == result['policy_sha256'] and
            saved['input_sha256'] == result['input_sha256'], 'payload policy/input identity')
    payload = saved['gradients']
    require(payload_hash(payload) == result['gradient_values_sha256'], 'gradient tensor identity')
    require(payload['horizon'] == result['row']['selected'] == len(payload['rewards']), 'complete payload horizon')
    require(all(math.isfinite(r) and 0 <= r <= 1 for r in payload['rewards']), 'finite rewards')
    area, best = (result['row'][k]['score'][0] for k in ('initial_metrics', 'best_metrics'))
    require(math.isclose(math.fsum(payload['rewards']), (area - best) / area, abs_tol=1e-12), 'telescoping reward')
    require(math.isclose(payload['reward_sum'], result['row']['reward_sum'], abs_tol=1e-12)
            and math.isfinite(payload['mean_entropy']), 'payload reward/entropy metadata')
    for key in ('reward_gradients', 'entropy_gradients'):
        values = payload[key]
        require(all(torch.isfinite(v).all() for v in values), 'finite gradient tensors')
        if model is not None:
            parameters = list(model.parameters())
            require(len(values) == len(parameters) and all(v.shape == p.shape and v.dtype == p.dtype
                    for v, p in zip(values, parameters)), 'gradient parameter shape/dtype')
    return payload


def update(model, optimizer, results, config):
    accepted = [r for r in results if r['status'] == 'complete']
    require(len(accepted) >= 2, 'at least two admitted episodes')
    before = parameter_hash(model)
    require(all(r['policy_sha256'] == before for r in accepted), 'same-policy batch')
    # Validate the entire batch before changing any gradients or optimizer state.
    payloads = [checked_payload(r, model) for r in accepted]
    optimizer.zero_grad(set_to_none=True)
    reward, entropy = ([torch.zeros_like(p) for p in model.parameters()] for _ in range(2))
    for payload in payloads:
        for target, value in zip(reward, payload['reward_gradients']):
            target.add_(value, alpha=-1 / len(payloads))
        for target, value in zip(entropy, payload['entropy_gradients']):
            target.add_(value, alpha=-config['entropy_coefficient'] / len(payloads))
    for parameter, a, b in zip(model.parameters(), reward, entropy):
        parameter.grad = a + b
    norm = torch.nn.utils.clip_grad_norm_(model.parameters(), config['gradient_clip'])
    require(torch.isfinite(norm), 'finite update norm')
    gradient_hash = tensor_hash([p.grad for p in model.parameters()])
    optimizer.step()
    returns = [p['reward_sum'] for p in payloads]
    return dict(reward_schema=SCHEMA, returns=returns, zero_reward_episodes=sum(r == 0 for r in returns),
        reward_gradient_norm=gradient_norm(reward), entropy_gradient_norm=gradient_norm(entropy),
        mean_entropy=sum(p['mean_entropy'] for p in payloads) / len(payloads),
        valid_episodes=len(payloads), policy_before_sha256=before, policy_after_sha256=parameter_hash(model),
        applied_gradient_sha256=gradient_hash, pre_clip_gradient_norm=float(norm))
