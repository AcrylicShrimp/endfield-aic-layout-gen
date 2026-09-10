"""Unchanged actor and an independent current/best-world critic for PPO area learning."""
import torch
from torch import nn

from full_factory_input_contracts import InputContractPolicy, WorldEncoder, ACTION_WIDTH, CONTEXT_WIDTH
from full_factory_selection import distribution

SCHEMA = 'ppo-critic-gae-direct-area-v2'


def require(condition, message):
    if not condition:
        raise ValueError(f'ppo-area.contract: {message}')


class AreaCritic(nn.Module):
    def __init__(self, depth=2, width=32):
        super().__init__()
        self.world = WorldEncoder(depth, width)
        self.head = nn.Sequential(nn.Linear(4 * width + 2 * ACTION_WIDTH + CONTEXT_WIDTH + 1, width),
                                  nn.SiLU(), nn.Linear(width, 1))
        nn.init.zeros_(self.head[-1].weight)
        nn.init.zeros_(self.head[-1].bias)

    def forward(self, obs, allowed):
        require(obs['current'] is not None and obs['best'] is not None, 'critic requires current and best state')
        device = next(self.parameters()).device
        current = self.world(obs['current'])[2]
        best = current if obs['best_is_current'] else self.world(obs['best'])[2]
        actions = torch.as_tensor(obs['actions'], dtype=torch.float32, device=device)
        mask = torch.as_tensor(allowed, dtype=torch.bool, device=device)
        require(mask.shape == (len(actions),) and mask.any(), 'critic candidate mask')
        available = actions[mask]
        context = torch.as_tensor(obs['context'], dtype=torch.float32, device=device)
        features = torch.cat((current, best, available.mean(0), available.max(0).values,
                              context, mask.float().mean().reshape(1)))
        return self.head(features).squeeze()


def models(spec, seed):
    torch.manual_seed(seed)
    if 'model' in spec:
        from ppo_spatial_model import validate_model
        model = spec['model']
        validate_model(model)
        actor = InputContractPolicy(model['graph_layers'], model['width'], model['cnn_dilations'])
        # Actor allocation must not change the critic's initial weights or RNG stream.
        with torch.random.fork_rng(devices=[]):
            torch.random.default_generator.manual_seed(seed + model['critic_seed_offset'])
            critic = AreaCritic(model['critic_graph_layers'], model['critic_width'])
        return actor, critic
    actor = InputContractPolicy(spec['policy']['depth'], spec['policy']['width'])
    critic = AreaCritic(spec['policy']['depth'], spec['policy']['width'])
    return actor, critic


def actor_distribution(actor, step, config):
    actions = [dict(kind=kind) for kind in step['kinds']]
    return distribution(actor(step['actor_observation']) / config['temperature'], actions,
                        step['allowed'], config['balanced'])


def gae(rewards, old_values, lam=.95):
    require(len(rewards) == len(old_values) and len(rewards) > 0 and 0 <= lam <= 1, 'GAE shape/lambda')
    rewards = torch.as_tensor(rewards, dtype=torch.float64)
    values = torch.as_tensor(old_values, dtype=torch.float64)
    require(torch.isfinite(rewards).all() and torch.isfinite(values).all(), 'finite GAE inputs')
    advantages = torch.zeros_like(rewards)
    following_value, following_advantage = 0.0, 0.0
    for index in reversed(range(len(rewards))):
        delta = rewards[index] + following_value - values[index]
        advantages[index] = delta + lam * following_advantage
        following_value, following_advantage = values[index], advantages[index]
    return advantages.float(), (advantages + values).float()


def clipped_surrogate(log_probability, old_log_probability, advantages, epsilon):
    ratio = torch.exp(log_probability - old_log_probability)
    return torch.minimum(ratio * advantages, ratio.clamp(1 - epsilon, 1 + epsilon) * advantages), ratio
