"""Registered encoder/history/critic-schedule treatments for the PPO screen."""
from copy import deepcopy

import torch
from torch import nn

from full_factory_input_contracts import ACTION_WIDTH, CONTEXT_WIDTH

SCHEMA = 'ppo-actor-critic-combinations-v1'
FIELDS = ('shared_backbone', 'critic_history', 'critic_after_kl')


def require(condition, message):
    if not condition:
        raise ValueError(f'ppo-combinations.contract: {message}')


def treatment(shared_backbone, critic_history, critic_after_kl):
    result = dict(schema=SCHEMA, shared_backbone=shared_backbone,
                  critic_history=critic_history, critic_after_kl=critic_after_kl)
    validate(result)
    return result


def validate(config):
    require(isinstance(config, dict) and set(config) == {'schema', *FIELDS}
            and config['schema'] == SCHEMA and all(type(config[k]) is bool for k in FIELDS),
            'expected the versioned three-boolean treatment')


def label(config):
    validate(config)
    return '-'.join(('shared' if config['shared_backbone'] else 'separate',
                     'history' if config['critic_history'] else 'nohistory',
                     'tail' if config['critic_after_kl'] else 'stop'))


class CombinationCritic(nn.Module):
    """Identical allocation across history treatments; explicit shared ownership."""
    def __init__(self, actor, config):
        super().__init__()
        validate(config)
        self.treatment = dict(config)
        self.width = actor.width
        for name in ('world', 'history_input', 'history'):
            module = getattr(actor, name)
            setattr(self, name, module if config['shared_backbone'] else deepcopy(module))
        self.head = nn.Sequential(nn.Linear(5 * actor.width + 2 * ACTION_WIDTH + CONTEXT_WIDTH + 1, actor.width),
                                  nn.SiLU(), nn.Linear(actor.width, 1))
        nn.init.zeros_(self.head[-1].weight)
        nn.init.zeros_(self.head[-1].bias)

    def history_features(self, obs):
        device = next(self.parameters()).device
        if not self.treatment['critic_history'] or not len(obs['history']):
            return torch.zeros(self.width, device=device)
        tokens = torch.as_tensor(obs['history'], dtype=torch.float32, device=device)
        _, hidden = self.history(self.history_input(tokens).unsqueeze(0))
        return hidden[-1, 0]

    def forward(self, obs, allowed, *, current_features=None, history_features=None):
        require(obs['current'] is not None and obs['best'] is not None, 'critic needs current and best worlds')
        device = next(self.parameters()).device
        current = self.world(obs['current'])[2] if current_features is None else current_features[2]
        best = current if obs['best_is_current'] else self.world(obs['best'])[2]
        actions = torch.as_tensor(obs['actions'], dtype=torch.float32, device=device)
        mask = torch.as_tensor(allowed, dtype=torch.bool, device=device)
        require(mask.shape == (len(actions),) and mask.any(), 'critic candidate mask')
        available = actions[mask]
        history = self.history_features(obs) if history_features is None else history_features
        if not self.treatment['critic_history']:
            history = torch.zeros_like(history)
        context = torch.as_tensor(obs['context'], dtype=torch.float32, device=device)
        return self.head(torch.cat((current, best, available.mean(0), available.max(0).values,
                                    context, mask.float().mean().reshape(1), history))).squeeze()


def parameter_groups(actor, critic):
    actor_parameters = list(actor.parameters())
    actor_ids = {id(p) for p in actor_parameters}
    critic_parameters = [p for p in critic.parameters() if id(p) not in actor_ids]
    require(actor_parameters and critic_parameters, 'nonempty disjoint actor/critic optimizer groups')
    return actor_parameters, critic_parameters


def optimizers(actor, critic, learning_rate):
    return tuple(torch.optim.Adam(parameters, lr=learning_rate)
                 for parameters in parameter_groups(actor, critic))


def optimizer_parameters(optimizer):
    return [p for group in optimizer.param_groups for p in group['params']]


def observation(critic, actor_obs, critic_obs):
    if not hasattr(critic, 'treatment'):
        return critic_obs
    history = actor_obs['history'] if critic.treatment['critic_history'] else actor_obs['history'][:0]
    return dict(critic_obs, history=history)


def predictions(actor, critic, actor_obs, critic_obs, allowed):
    """Scalar collection oracle; shared features never expose hidden best inputs."""
    if not getattr(critic, 'treatment', {}).get('shared_backbone'):
        return actor(actor_obs), critic(critic_obs, allowed)
    require(actor.world is critic.world and actor.history is critic.history,
            'shared treatment requires actual module identity')
    current = actor.world(actor_obs['current'])
    device = next(actor.parameters()).device
    history = torch.zeros(actor.width, device=device)
    if len(actor_obs['history']):
        tokens = torch.as_tensor(actor_obs['history'], dtype=torch.float32, device=device)
        _, hidden = actor.history(actor.history_input(tokens).unsqueeze(0))
        history = hidden[-1, 0]
    return (actor(actor_obs, current_features=current, history_features=history),
            critic(critic_obs, allowed, current_features=current, history_features=history))
