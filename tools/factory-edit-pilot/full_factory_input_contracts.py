"""Explicit observation contracts for an isolated full-factory policy screen."""
from copy import deepcopy

import numpy as np
import torch
from torch import nn

from full_factory_environment import digest
from full_factory_policy import observe, ACTION_WIDTH, NODE_WIDTH, EDGE_WIDTH, RASTER_WIDTH
from full_factory_selection import SelectionMemory, action_key, physical_key

SCHEMA = 'factory-input-contract-screen-v1'
HISTORY_WIDTH = 53
CONTEXT_WIDTH = 9
STATUSES = ('improved', 'accepted_exploration', 'accepted_equal', 'rejected', 'executor_rejected', 'late')
CONTRACTS = {
    'action-only': dict(current='none', history=0, outcomes=False, best='none', restore=True),
    'geometry-current': dict(current='geometry', history=0, outcomes=False, best='none', restore=True),
    'full-current': dict(current='full', history=0, outcomes=False, best='none', restore=True),
    'current-history-4': dict(current='full', history=4, outcomes=False, best='none', restore=True),
    'current-history-16': dict(current='full', history=16, outcomes=False, best='none', restore=True),
    'current-history-16-outcomes': dict(current='full', history=16, outcomes=True, best='none', restore=True),
    'current-best-2': dict(current='full', history=0, outcomes=False, best='scores-2', restore=True),
    'current-best-3': dict(current='full', history=0, outcomes=False, best='scores-3', restore=True),
    'current-best-layout': dict(current='full', history=0, outcomes=False, best='layout', restore=True),
    'current-best-layout-history': dict(current='full', history=16, outcomes=True, best='layout', restore=True),
    'current-no-restore': dict(current='full', history=0, outcomes=False, best='none', restore=False),
    'history-no-restore': dict(current='full', history=16, outcomes=True, best='none', restore=False),
}


def world_input(base, geometry=False):
    value = {k: base[k] for k in ('raster', 'nodes', 'node_xy', 'edges', 'edge_values')}
    value['messages'] = not geometry
    if geometry:
        value = deepcopy(value)
        value['raster'][3:19] = 0
        value['nodes'][:, 17:24] = 0
        value['nodes'][:, 31] = 0
        value['edges'] = np.empty((0, 2), np.int64)
        value['edge_values'] = np.empty((0, EDGE_WIDTH), np.float32)
    return value


def history_array(history, contract):
    length = contract['history']
    selected = history[-length:] if length else []
    result = np.zeros((len(selected), HISTORY_WIDTH), np.float32)
    for i, record in enumerate(selected):
        result[i, :42] = record['action']
        result[i, 42] = (len(selected) - i) / length
        if contract['outcomes']:
            result[i, 43:] = record['outcome']
    return result


def selected_action_token(obs, index):
    action = np.zeros(42, np.float32)
    action[:32] = obs['actions'][index]
    selected = obs['member_pose'][obs['member_actions'] == index]
    if len(selected):
        action[32:] = selected.mean(0)
    return action


def history_record(action_token, before_score, step, initial_score):
    if step['status'] not in STATUSES:
        raise ValueError(f'input-contract.history-status: {step["status"]}')
    outcome = np.zeros(10, np.float32)
    outcome[STATUSES.index(step['status'])] = 1
    outcome[6] = step['unchanged']
    outcome[7:] = [(after - before) / max(1, initial) for after, before, initial
                   in zip(step['current_score'], before_score, initial_score)]
    return dict(action=action_token.copy(), outcome=outcome)


def observation(state, description, remaining, history, name, runtime):
    contract = CONTRACTS[name]
    base = observe(state['current'], description, state, remaining)
    context = np.zeros(CONTEXT_WIDTH, np.float32)
    context[:4] = base['context'][:4]
    if contract['current'] != 'none':
        context[4:6] = base['context'][4:6]
    best_kind = contract['best']
    if best_kind != 'none':
        context[6:8] = base['context'][6:8]
    if best_kind in ('scores-3', 'layout'):
        context[8] = state['best_metrics']['score'][2] / max(1, state['initial_metrics']['score'][2])
    current = None if contract['current'] == 'none' else world_input(base, contract['current'] == 'geometry')
    best, best_is_current = None, False
    if best_kind == 'layout':
        best_is_current = digest(state['current']) == digest(state['best'])
        if best_is_current:
            best = current
        else:
            best_state = dict(state, current=state['best'], current_metrics=state['best_metrics'])
            best_description = runtime.rpc('describe', state['best'], options={})
            best = world_input(observe(state['best'], best_description, best_state, remaining))
    candidates = {k: base[k] for k in ('actions', 'operands', 'action_xy', 'member_nodes', 'member_actions', 'member_xy', 'member_pose')}
    # Omit references into a hidden world; the action-only model uses geometry in
    # action/member descriptors but cannot inspect node counts, ownership or maps.
    if current is None:
        candidates['operands'] = np.zeros_like(candidates['operands'])
        candidates['member_nodes'] = np.zeros_like(candidates['member_nodes'])
        candidates['action_xy'] = np.zeros_like(candidates['action_xy'])
        candidates['member_xy'] = np.zeros_like(candidates['member_xy'])
    return dict(schema=SCHEMA, **candidates, current=current, best=best, best_is_current=best_is_current,
                context=context, history=history_array(history, contract))


def observation_hash(obs):
    """Stable nested tensor/metadata identity, including explicit absent inputs."""
    import hashlib
    result = hashlib.sha256()

    def visit(value):
        if isinstance(value, dict):
            result.update(b'dict')
            for key in sorted(value):
                result.update(key.encode())
                visit(value[key])
        elif isinstance(value, np.ndarray):
            result.update(str((value.dtype.str, value.shape)).encode())
            result.update(np.ascontiguousarray(value).tobytes())
        else:
            result.update(repr(value).encode())
    visit(obs)
    return result.hexdigest()


def selection_mask(state, description, memory, name, enabled=True):
    before, best = physical_key(state['current']), physical_key(state['best'])
    keys = [action_key(a, description, state['current'], best) for a in description['actions']]
    allowed, exhausted = memory.mask(before, keys, description['actions'], enabled)
    if not CONTRACTS[name]['restore']:
        allowed = [ok and a['kind'] != 'restore_best' for ok, a in zip(allowed, description['actions'])]
    if not any(allowed):
        raise ValueError(f'input-contract.action-exhausted: {name}, state {before}')
    return keys, allowed, exhausted


class WorldEncoder(nn.Module):
    def __init__(self, depth, width):
        super().__init__()
        self.width = width
        self.raster = nn.Sequential(nn.Conv2d(RASTER_WIDTH, width, 3, padding=1), nn.GroupNorm(4, width), nn.SiLU(),
                                    nn.Conv2d(width, width, 3, padding=1), nn.SiLU())
        self.node = nn.Linear(NODE_WIDTH + width, width)
        self.edge = nn.Linear(EDGE_WIDTH, width)
        self.messages = nn.ModuleList(nn.Linear(width, width) for _ in range(depth))
        self.updates = nn.ModuleList(nn.Linear(2 * width, width) for _ in range(depth))
        self.norms = nn.ModuleList(nn.LayerNorm(width) for _ in range(depth))

    def forward(self, world):
        device = next(self.parameters()).device
        def tensor(key, dtype=torch.float32):
            return torch.as_tensor(world[key], dtype=dtype, device=device)
        raster = self.raster(tensor('raster').unsqueeze(0))[0]
        xy = tensor('node_xy', torch.long)
        h = torch.nn.functional.silu(self.node(torch.cat((tensor('nodes'), raster[:, xy[:, 1], xy[:, 0]].T), dim=1)))
        if world['messages']:
            edges = tensor('edges', torch.long)
            edge_h = self.edge(tensor('edge_values'))
            degree = torch.zeros(len(h), 1, device=device).index_add_(0, edges[:, 1], torch.ones(len(edges), 1, device=device)).clamp_min(1)
            for message, update, norm in zip(self.messages, self.updates, self.norms):
                aggregate = torch.zeros_like(h).index_add_(0, edges[:, 1], torch.nn.functional.silu(message(h[edges[:, 0]]) + edge_h)) / degree
                h = norm(h + torch.nn.functional.silu(update(torch.cat((h, aggregate), dim=1))))
        return h, raster, torch.cat((h.mean(0), raster.mean((1, 2))))


class InputContractPolicy(nn.Module):
    """Identical parameter allocation for all contracts; one shared world encoder."""
    def __init__(self, depth=2, width=32):
        super().__init__()
        self.depth, self.width = depth, width
        self.world = WorldEncoder(depth, width)
        self.member = nn.Sequential(nn.Linear(2 * width + 10, 2 * width), nn.SiLU())
        self.history_input = nn.Sequential(nn.Linear(HISTORY_WIDTH, width), nn.LayerNorm(width), nn.SiLU())
        self.history = nn.GRU(width, width, batch_first=True)
        self.score = nn.Sequential(nn.Linear(ACTION_WIDTH + 10 * width + CONTEXT_WIDTH, width), nn.SiLU(), nn.Linear(width, 1))

    def forward(self, obs):
        if obs['schema'] != SCHEMA:
            raise ValueError('input-contract.observation-schema')
        device = next(self.parameters()).device
        def tensor(name, dtype=torch.float32):
            return torch.as_tensor(obs[name], dtype=dtype, device=device)
        actions = tensor('actions')
        count = len(actions)
        member_actions = tensor('member_actions', torch.long)
        member_count = len(member_actions)
        if obs['current'] is None:
            operands = torch.zeros(count, 2 * self.width, device=device)
            local = torch.zeros(count, self.width, device=device)
            member_context = torch.zeros(member_count, 2 * self.width, device=device)
            current_global = torch.zeros(2 * self.width, device=device)
        else:
            h, raster, current_global = self.world(obs['current'])
            operands = torch.cat((torch.zeros(1, self.width, device=device), h))[tensor('operands', torch.long)].flatten(1)
            xy = tensor('action_xy', torch.long)
            local = raster[:, xy[:, 1], xy[:, 0]].T
            mxy = tensor('member_xy', torch.long)
            member_context = torch.cat((h[tensor('member_nodes', torch.long)], raster[:, mxy[:, 1], mxy[:, 0]].T), dim=1)
        members = self.member(torch.cat((member_context, tensor('member_pose')), dim=1))
        counts = torch.zeros(count, 1, device=device).index_add_(0, member_actions, torch.ones(member_count, 1, device=device)).clamp_min(1)
        pooled = torch.zeros(count, 2 * self.width, device=device).index_add_(0, member_actions, members) / counts
        best_global = torch.zeros(2 * self.width, device=device)
        if obs['best'] is not None:
            best_global = current_global if obs['best_is_current'] else self.world(obs['best'])[2]
        history_h = torch.zeros(self.width, device=device)
        if len(obs['history']):
            _, hidden = self.history(self.history_input(tensor('history')).unsqueeze(0))
            history_h = hidden[-1, 0]
        global_h = torch.cat((current_global, best_global, history_h, tensor('context'))).expand(count, -1)
        return self.score(torch.cat((actions, operands, local, pooled, global_h), dim=1)).squeeze(1)
