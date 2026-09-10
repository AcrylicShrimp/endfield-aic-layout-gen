"""History-conditioned sampling without speculative routing or candidate scores."""
from collections import Counter
import hashlib
import json
import math

import torch


def canonical(value):
    """Witness DTO arrays are sets of records, never an ordered path encoding."""
    if isinstance(value,dict):
        return {k:canonical(v) for k,v in sorted(value.items())}
    if isinstance(value,list):
        return sorted((canonical(v) for v in value),key=lambda v:json.dumps(v,sort_keys=True,separators=(',',':')))
    return value


def physical_key(witness):
    return hashlib.sha256(json.dumps(canonical(witness),sort_keys=True,separators=(',',':')).encode()).hexdigest()


def action_key(action,description,witness,best_key):
    value=dict(action)
    if value['kind']=='pose':
        value['poses']=sorted((dict(q,member=witness['placements'][q['member']]['instance']) for q in value['poses']),key=lambda q:q['member'])
    elif value['kind']=='reroute':
        chain=description['chains'][value.pop('chain')]
        value['network']=witness['transport_networks'][chain['network']]['id']
        # The direction of a chain's cell sequence matters; do not canonical-sort it.
        value['directed_cells']=chain['cells']
        value['rate']=chain['rate']
    elif value['kind']=='restore_best':
        value['best_key']=best_key
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()


class SelectionMemory:
    def __init__(self,initial):
        self.seen={physical_key(initial)}
        self.failed={}

    def mask(self,state_key,keys,actions,enabled):
        blocked=self.failed.get(state_key,set()) if enabled else set()
        allowed=[k not in blocked for k in keys]
        exhausted=not any(allowed)
        if exhausted:
            allowed[next(i for i,a in enumerate(actions) if a['kind']=='restore_best')]=True
        return allowed,exhausted

    def record(self,before,after,key):
        unchanged=before==after
        revisit=not unchanged and after in self.seen
        if unchanged or revisit:
            self.failed.setdefault(before,set()).add(key)
        self.seen.add(after)
        return unchanged,revisit


def distribution(logits,actions,allowed,balanced):
    return distribution_kinds(logits, [a['kind'] for a in actions], allowed, balanced)


def distribution_kinds(logits, kinds, allowed, balanced):
    counts = Counter(kind for kind, ok in zip(kinds, allowed) if ok)
    if len(kinds) != len(logits) or len(allowed) != len(kinds) or not counts:
        raise ValueError('factory.selection-shape: nonempty aligned kind/logit/mask arrays required')
    adjustment = [-math.log(counts[kind]) if ok and balanced else 0 for kind, ok in zip(kinds, allowed)]
    adjusted = logits + torch.tensor(adjustment, device=logits.device, dtype=logits.dtype)
    adjusted = adjusted.masked_fill(~torch.tensor(allowed, device=logits.device, dtype=torch.bool), -torch.inf)
    return torch.distributions.Categorical(logits=adjusted)


def geometry_choice(witness,actions,allowed):
    for i,a in enumerate(actions):
        if allowed[i] and a['kind']=='contract':return i
    centers=[sum(p[axis]+p[extent]/2 for p in witness['placements'])/len(witness['placements'])
             for axis,extent in [('x','width'),('y','height')]]
    scores=[]
    for i,a in enumerate(actions):
        if allowed[i] and a['kind']=='pose':
            gain=0
            for q in a['poses']:
                p=witness['placements'][q['member']]
                swap=abs(q['rotation']-p['rotation'])%180!=0
                pw,ph=(p['height'],p['width']) if swap else (p['width'],p['height'])
                gain+=sum(abs(p[axis]+p[extent]/2-c)-abs(q[axis]+size/2-c)
                          for axis,extent,size,c in [('x','width',pw,centers[0]),('y','height',ph,centers[1])])
            gain/=len(a['poses'])
            scores.append((gain,-i,i))
    if scores:return max(scores)[2]
    # Exhaustion is explicit; no uncharged candidate search or implicit stopping.
    return next((i for i,a in enumerate(actions) if allowed[i] and a['kind']=='restore_best'),next(i for i,ok in enumerate(allowed) if ok))
