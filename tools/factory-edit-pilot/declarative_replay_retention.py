"""Bounded replay replacement using observed SIL contribution, without rescoring."""
from collections import Counter
import math

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


def victim(entries, incoming, ordinal, config, strata):
    """Protect scarce strata, then retire the least useful sufficiently observed row."""
    validate(config)
    require(bool(entries), 'declarative.replay-retention: no removable entry')
    incoming_stratum=stratum(incoming,strata)
    counts=Counter(stratum(e,strata) for e in entries.values());counts[incoming_stratum]+=1
    # Count the arriving experience when balancing capacity. This permits a
    # scarce stratum to grow instead of endlessly replacing its few examples.
    available={stratum(e,strata) for e in entries.values()}
    chosen=max(available,key=lambda s:(counts[s],s==incoming_stratum,s))
    candidates=[e for e in entries.values() if stratum(e,strata)==chosen]
    def mature(entry):
        value=entry.get('utility')
        return value is not None and value['draws']>=config['minimum_draws'] and value['updates']>=config['minimum_updates']
    experienced=[e for e in candidates if mature(e)]
    if experienced:
        def rank(entry):
            u=entry['utility'];age=max(0,ordinal-u['last_update'])
            return (u['ema']/(1+age/config['stale_updates']),entry['admitted_update'],entry['id'])
        selected=min(experienced,key=rank);reason='lowest_observed_contribution'
    else:
        # Capacity is still a hard bound when no candidate has enough evidence.
        # Give newer/unobserved arrivals a chance; report this fallback honestly.
        selected=min(candidates,key=lambda e:(e['admitted_update'],e['id']));reason='insufficient_observations_oldest'
    return selected['id'],dict(reason=reason,stratum=chosen,utility=selected.get('utility'),
        admitted_update=selected['admitted_update'],labels=len(selected['labels']))
