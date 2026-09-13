"""Bounded replay replacement using observed SIL contribution, without rescoring."""
from collections import Counter
import math
import heapq

from blueprint_ppo_update import require

SCHEMA = 'declarative-replay-utility-v1'


def validate(config):
    require(config['schema']==SCHEMA and 0<config['ema_alpha']<=1 and
        config['minimum_draws']>=1 and config['minimum_updates']>=1 and
        config['stale_updates']>0, 'declarative.replay-retention: invalid utility policy')


def stratum(entry, strata):
    product=entry['input']['draw']['product'] if entry['input'] else 'fixture-diagnostic'
    matches=[name for name,products in strata.items() if product in products]
    require(len(matches)==1, 'declarative.replay-retention: ambiguous product stratum')
    return matches[0]


def observe(entries, measurements, ordinal, config):
    validate(config)
    for row in measurements:
        entry=entries[row['id']]
        require(row['draws']>0 and math.isfinite(row['mean_weighted_nll']) and row['mean_weighted_nll']>=0,
            'declarative.replay-retention: invalid observed contribution')
        old=entry.get('utility')
        require(old is None or old['last_update']<ordinal, 'declarative.replay-retention: repeated update measurement')
        alpha=config['ema_alpha']
        entry['utility']=dict(ema=row['mean_weighted_nll'] if old is None else
            alpha*row['mean_weighted_nll']+(1-alpha)*old['ema'],
            draws=row['draws']+(old['draws'] if old else 0),updates=1+(old['updates'] if old else 0),last_update=ordinal)


class ReplacementIndex:
    """One fixed-update ordering, with logarithmic removal and admission."""
    def __init__(self, entries, ordinal, config, strata):
        validate(config)
        self.entries, self.ordinal, self.config, self.strata = entries, ordinal, config, strata
        self.counts, self.present, self.next_token = Counter(), {}, 0
        self.mature, self.oldest = {}, {}
        for entry in entries.values():
            self.add(entry)

    def add(self, entry):
        name = stratum(entry,self.strata)
        token = self.next_token; self.next_token += 1
        self.present[entry['id']] = token
        self.counts[name] += 1
        tail = (entry['admitted_update'],entry['id'],token)
        heapq.heappush(self.oldest.setdefault(name,[]),tail)
        value = entry.get('utility')
        if value is not None and value['draws']>=self.config['minimum_draws'] and value['updates']>=self.config['minimum_updates']:
            age = max(0,self.ordinal-value['last_update'])
            score = value['ema']/(1+age/self.config['stale_updates'])
            heapq.heappush(self.mature.setdefault(name,[]),(score,*tail))

    def current(self, heap):
        while heap and self.present.get(heap[0][-2]) != heap[0][-1]:
            heapq.heappop(heap)
        return bool(heap)

    def choose(self, incoming):
        require(bool(self.present), 'declarative.replay-retention: no removable entry')
        name = stratum(incoming,self.strata)
        chosen = max((s for s,n in self.counts.items() if n),
            key=lambda s:(self.counts[s]+(s==name),s==name,s))
        experienced = self.mature.setdefault(chosen,[])
        if self.current(experienced):
            selected = heapq.heappop(experienced)[-2]
            reason = 'lowest_observed_contribution'
        else:
            oldest = self.oldest[chosen]
            require(self.current(oldest), 'declarative.replay-retention: empty indexed stratum')
            selected = heapq.heappop(oldest)[-2]
            reason = 'insufficient_observations_oldest'
        entry = self.entries[selected]
        del self.present[selected];self.counts[chosen] -= 1
        return selected,dict(reason=reason,stratum=chosen,utility=entry.get('utility'),
            admitted_update=entry['admitted_update'],labels=len(entry['labels']))


def victim(entries, incoming, ordinal, config, strata):
    return ReplacementIndex(entries,ordinal,config,strata).choose(incoming)
