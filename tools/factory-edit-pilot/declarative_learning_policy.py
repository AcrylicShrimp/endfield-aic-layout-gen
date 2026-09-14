"""Packed root choices and conditional ports form one environment decision."""
import torch
from torch import nn

from blueprint_edit_input import BlueprintActorCritic
from blueprint_ppo_batch import distribution, pack as pack_graphs, to_device
from blueprint_ppo_update import require
from declarative_learning_input import PACKED, PORT_FEATURES, SCHEMA
from declarative_band_mask import ROOT_MASK_SCHEMA


def pack(inputs):
    result = pack_graphs(inputs, schema=SCHEMA, packed_schema=PACKED)
    if any('root_invalid' in row or 'root_mask_schema' in row for row in inputs):
        masks = []
        for row in inputs:
            if 'root_invalid' not in row and 'root_mask_schema' not in row:
                masks.append(torch.zeros(len(row['actions']),dtype=torch.bool))
                continue
            mask = row.get('root_invalid')
            require(row.get('root_mask_schema')==ROOT_MASK_SCHEMA and isinstance(mask,torch.Tensor) and
                mask.dtype==torch.bool and mask.shape==(len(row['actions']),) and not bool(mask.all()),
                'declarative.masking.input: bound nonempty boolean root domain required')
            masks.append(mask)
        result['root_invalid'] = torch.cat(masks)
    return result


def root_distribution(logits, batch, *, uniform=False):
    logits = torch.zeros_like(logits) if uniform else logits
    if 'root_invalid' in batch:
        # Mask after uniformization, retaining original per-kind count correction.
        # The finite sentinel keeps the existing entropy kernel finite.
        logits = logits.masked_fill(batch['root_invalid'], -1e9)
    return distribution(logits, batch)


class ActorCritic(BlueprintActorCritic):
    packed_schema = PACKED

    def __init__(self, width=40, message_rounds=2):
        super().__init__(width, message_rounds)
        self.port_actor = nn.Sequential(nn.Linear(width * 3 + 12 + PORT_FEATURES * 2, width),
                                        nn.Tanh(), nn.Linear(width, 1))

    def root(self, batch):
        nodes, pooled = self.encode_packed(batch)
        references = batch['action_references']
        context = pooled[batch['action_graph']]
        selected = torch.where((references >= 0).unsqueeze(-1), nodes[references.clamp_min(0)], context.unsqueeze(1))
        logits = self.actor(torch.cat((selected.flatten(1), context, batch['action_features']), -1)).squeeze(-1)
        return logits, self.critic(pooled).squeeze(-1), nodes, pooled

    def ports(self, nodes, pooled, branches):
        references = branches['references']
        return self.port_actor(torch.cat((nodes[references].flatten(1), pooled[branches['owner']],
            branches['root_features'], branches['features'], branches['history']), -1)).squeeze(-1)


def learners(seed, config, device='cpu'):
    torch.manual_seed(seed)
    model = ActorCritic(config['width'], config['message_rounds']).to(device)
    actor = [p for name, p in model.named_parameters() if not name.startswith('critic.')]
    return model, torch.optim.Adam(actor, lr=config['learning_rate']), torch.optim.Adam(model.critic.parameters(), lr=config['learning_rate'])


def branch(prepared, decision, depth):
    root = decision['root']
    require(0 <= root < len(prepared['actions']), 'declarative root choice range')
    action = prepared['actions'][root]
    require(action['kind'] == 'place', 'port choices require a facility edit')
    owner = prepared['root_owners'][root]
    groups = prepared['port_groups'][owner]
    require(0 <= depth < len(groups) and len(decision['ports']) >= depth, 'declarative port prefix')
    used, history = set(), torch.zeros(PORT_FEATURES)
    for group, index in zip(groups[:depth], decision['ports'][:depth], strict=True):
        require(0 <= index < len(group['ports']) and group['ports'][index] not in used, 'declarative duplicate port prefix')
        used.add(group['ports'][index])
        history += group['features'][action['rotation']][index]
    group = groups[depth]
    keep = [i for i, name in enumerate(group['ports']) if name not in used]
    require(bool(keep), 'declarative compatible port domain exhausted')
    return dict(features=group['features'][action['rotation']][keep], keep=keep,
        references=torch.tensor([[owner, group['graph_node']]] * len(keep), dtype=torch.long),
        root_features=prepared['action_features'][root].expand(len(keep), -1),
        history=(history / max(depth, 1)).expand(len(keep), -1))


def pack_branches(rows, graph_owners, node_offsets):
    fields = {k: [] for k in ('features', 'references', 'root_features', 'history', 'owner', 'action_graph')}
    for factor, (row, owner) in enumerate(zip(rows, graph_owners, strict=True)):
        size = len(row['keep'])
        for field in ('features', 'root_features', 'history'):
            fields[field].append(row[field])
        fields['references'].append(row['references'] + node_offsets[owner])
        fields['owner'].append(torch.full((size,), owner, dtype=torch.long))
        fields['action_graph'].append(torch.full((size,), factor, dtype=torch.long))
    result = {key: torch.cat(values) for key, values in fields.items()}
    result.update(size=len(rows), kind_log_counts=torch.zeros(len(result['features'])),
        factor_owners=torch.tensor(graph_owners, dtype=torch.long), action_counts=[len(r['keep']) for r in rows])
    return result


def offsets(counts):
    result, total = [], 0
    for count in counts:
        result.append(total); total += count
    return result


def action(prepared, decision):
    require('recorded_root' not in prepared or decision['root']==prepared['recorded_root'],
        'declarative.replay-input: persisted input only describes its recorded conditional action')
    require(0 <= decision['root'] < len(prepared['actions']), 'declarative root choice range')
    require('root_invalid' not in prepared or not bool(prepared['root_invalid'][decision['root']]),
        'declarative.masking.action: selected root is masked')
    value = dict(prepared['actions'][decision['root']])
    groups = prepared['port_groups'][prepared['root_owners'][decision['root']]] if value['kind'] == 'place' else []
    require(len(decision['ports']) == len(groups), 'declarative joint action must assign every terminal')
    if value['kind'] == 'place':
        assignments, used = [], set()
        for group, index in zip(groups, decision['ports'], strict=True):
            require(0 <= index < len(group['ports']) and group['ports'][index] not in used,
                    'declarative selected incompatible/reused port')
            used.add(group['ports'][index])
            assignments.append(dict(node=group['node'], port=group['ports'][index]))
        value['ports'] = assignments
    return value


def sample(model, inputs, sampler, *, uniform=False):
    """One encoder pass, batched conditional heads, deterministic depth/stream order."""
    device = next(model.parameters()).device
    batch = to_device(pack(inputs), device)
    logits, predicted, nodes, pooled = model.root(batch)
    logp, entropy = root_distribution(logits, batch, uniform=uniform)
    # Keep the established CPU categorical RNG and stream/depth draw order.
    # Transfer each head's complete result once, not one GPU scalar per choice.
    logp, entropy, predicted = logp.cpu(), entropy.cpu(), predicted.cpu()
    decisions, total_logp, total_entropy = [], [], []
    for start, count, ent in zip(offsets(batch['action_counts']), batch['action_counts'], entropy, strict=True):
        selected = int(torch.multinomial(logp[start:start+count].exp(), 1, generator=sampler))
        decisions.append(dict(root=selected, ports=[]))
        total_logp.append(float(logp[start+selected])); total_entropy.append(float(ent))
    node_offsets = offsets([len(value['node_static']) for value in inputs])
    depth = 0
    while True:
        owners = [i for i, (value, decision) in enumerate(zip(inputs, decisions, strict=True))
            if value['actions'][decision['root']]['kind'] == 'place'
            and len(value['port_groups'][value['root_owners'][decision['root']]]) > depth]
        if not owners:
            break
        rows = [branch(inputs[i], decisions[i], depth) for i in owners]
        branches = to_device(pack_branches(rows, owners, node_offsets), device)
        logits = model.ports(nodes, pooled, branches)
        conditional, entropy = distribution(torch.zeros_like(logits) if uniform else logits, branches)
        conditional, entropy = conditional.cpu(), entropy.cpu()
        for owner, row, start, count, ent in zip(owners, rows, offsets(branches['action_counts']), branches['action_counts'], entropy, strict=True):
            index = int(torch.multinomial(conditional[start:start+count].exp(), 1, generator=sampler))
            decisions[owner]['ports'].append(row['keep'][index])
            total_logp[owner] += float(conditional[start+index]); total_entropy[owner] += float(ent)
        depth += 1
    return [dict(decision=d, action=action(value,d), old_logp=lp, old_value=float(v), entropy=ent)
        for value, d, lp, v, ent in zip(inputs, decisions, total_logp, predicted, total_entropy, strict=True)]


def values(model, inputs):
    batch = to_device(pack(inputs), next(model.parameters()).device)
    _, pooled = model.encode_packed(batch)
    return model.critic(pooled).squeeze(-1).cpu()


def recorded_branches(row):
    prepared,decision=row['input'],row['decision']
    key=(id(prepared),decision['root'],tuple(decision['ports']))
    cached=row.get('_recorded_branches')
    if cached is not None and cached[0]==key:return cached[1]
    expected=len(prepared['port_groups'][prepared['root_owners'][decision['root']]]) if prepared['actions'][decision['root']]['kind']=='place' else 0
    require(len(decision['ports'])==expected,'declarative recorded port coverage')
    result=[]
    for depth,index in enumerate(decision['ports']):
        value=branch(prepared,decision,depth)
        require(index in value['keep'],'declarative recorded port violates prefix mask')
        result.append((value,value['keep'].index(index)))
    # These tensors depend only on this immutable input and recorded decision.
    # Reuse them across PPO epochs and successful replay draws. Compact storage
    # never writes this disposable cache; buffer eviction releases its owner.
    row['_recorded_branches']=(key,result)
    return result


def prepare_rows(rows):
    inputs = [r['input'] for r in rows]
    batch = pack(inputs)
    decisions = [r['decision'] for r in rows]
    require(all(0 <= d['root'] < len(p['actions']) for p,d in zip(inputs,decisions,strict=True)),
            'declarative recorded root choice range')
    roots = torch.tensor([start+d['root'] for start,d in zip(offsets(batch['action_counts']), decisions, strict=True)], dtype=torch.long)
    require('root_invalid' not in batch or not bool(batch['root_invalid'][roots].any()),
        'declarative.masking.label: recorded action was masked in its source state')
    branch_rows, owners, selected = [], [], []
    total = 0
    for owner, recorded in enumerate(rows):
        # Full recorded choices are the conditioning context; no sampling or
        # candidate regeneration is allowed while recomputing PPO likelihoods.
        for row,index in recorded_branches(recorded):
            branch_rows.append(row); owners.append(owner)
            selected.append(total+index); total += len(row['keep'])
    branches = pack_branches(branch_rows, owners, offsets([len(i['node_static']) for i in inputs])) if branch_rows else None
    return dict(batch=batch, roots=roots, branches=branches, selected=torch.tensor(selected,dtype=torch.long))


def device_rows(prepared, device):
    return dict(batch=to_device(prepared['batch'],device), roots=prepared['roots'].to(device),
        branches=None if prepared['branches'] is None else to_device(prepared['branches'],device),
        selected=prepared['selected'].to(device))


def predict(model, prepared):
    logits, values, nodes, pooled = model.root(prepared['batch'])
    root_logp, entropy = root_distribution(logits, prepared['batch'])
    logp = root_logp[prepared['roots']]
    branches = prepared['branches']
    if branches is not None:
        conditional, port_entropy = distribution(model.ports(nodes,pooled,branches),branches)
        logp = logp.index_add(0,branches['factor_owners'],conditional[prepared['selected']])
        # Conditional categorical entropy on recorded prefixes. This retains
        # entropy regularization, but is not exact enumeration of joint entropy.
        entropy = entropy.index_add(0,branches['factor_owners'],port_entropy)
    return logp, entropy, values
