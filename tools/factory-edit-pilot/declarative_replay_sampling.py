"""Opt-in bounded replay distributions; constructed once per optimizer update."""
import heapq
import math

import torch

from blueprint_ppo_update import require
from declarative_replay_retention import stratum
from declarative_success_features import validate_metadata

SCHEMA='declarative-success-sampling-v1'


def validate(config):
    if config is None:return
    require(isinstance(config,dict),'declarative.replay-sampling.config: object required')
    fields={'schema','kind','sequences_per_minibatch'}
    kind=config.get('kind')
    require(config.get('schema')==SCHEMA and kind in ('uniform','score_mixture','positive_advantage') and
        type(config.get('sequences_per_minibatch')) is int and config['sequences_per_minibatch'] in (8,16,32),
        'declarative.replay-sampling.config: unknown schema, kind or count')
    if kind=='score_mixture':
        fields|={'mixture','score_profile'}
        require(config.get('mixture')==.5 and config.get('score_profile') in ('rare_3pp','rare_3pp_exterior_capped'),
            'declarative.replay-sampling.profile: unsupported score mixture')
    if kind=='positive_advantage':
        fields|={'alpha','beta','epsilon'}
        require(config.get('alpha')==.6 and config.get('beta')==.1 and config.get('epsilon')==1e-6,
            'declarative.replay-sampling.priority-config: unsupported priority reference')
    require(set(config)==fields,'declarative.replay-sampling.fields: unknown or irrelevant settings')


def priority_enabled(config):
    return (config or {}).get('kind')=='positive_advantage'


def validate_priority(value,identity):
    require(isinstance(value,dict) and set(value)=={'value','observed_update'} and
        type(value['value']) in (int,float) and math.isfinite(value['value']) and value['value']>=0 and
        (value['observed_update'] is None or type(value['observed_update']) is int and value['observed_update']>=0),
        f'declarative.replay-sampling.priority-metadata: {identity}')


def observe_priorities(entries,measurements,ordinal):
    for row in measurements:
        entry=entries[row['id']];value=row['mean_positive_advantage'];old=entry['sampling_priority_v1']
        require(old['observed_update'] is None or old['observed_update']<ordinal,
            'declarative.replay-sampling.repeated-observation: priority already refreshed')
        new=dict(value=value,observed_update=ordinal);validate_priority(new,row['id'])
        entry['sampling_priority_v1']=new


class PriorityBootstrap:
    """Current maximum per stratum, including removals, without per-entry scans."""
    def __init__(self,entries,strata):
        self.strata,self.heaps,self.present,self.token=strata,{}, {},0
        for entry in entries.values():self.add(entry)

    def add(self,entry):
        value=entry['sampling_priority_v1'];validate_priority(value,entry['id'])
        self.present[entry['id']]=self.token
        heapq.heappush(self.heaps.setdefault(stratum(entry,self.strata),[]),(-value['value'],entry['id'],self.token))
        self.token+=1

    def remove(self,identity):
        del self.present[identity]

    def maximum(self,incoming):
        heap=self.heaps.setdefault(stratum(incoming,self.strata),[])
        while heap and self.present.get(heap[0][1])!=heap[0][2]:heapq.heappop(heap)
        return -heap[0][0] if heap else 1.


def probabilities(rows,config):
    n=len(rows);require(n>0,'declarative.replay-sampling.empty: distribution requires entries')
    kind=config['kind']
    if kind=='uniform':return [1./n]*n,[1.]*n
    if kind=='score_mixture':
        for row in rows:validate_metadata(row.get('retention_features_v1'),row['id'])
        fraction=sum(row['retention_features_v1']['has_place'] for row in rows)/n
        weights=[]
        for row in rows:
            feature=row['retention_features_v1']
            score=feature['gain']+.03*(1-fraction)*feature['has_place']
            if config['score_profile']=='rare_3pp_exterior_capped':score+=min(feature['exterior_release'],feature['gain'])
            weights.append(1+min(7,score/.03))
        total=math.fsum(weights);mix=config['mixture']
        values=[(1-mix)/n+mix*w/total for w in weights];corrections=[1.]*n
    else:
        for row in rows:validate_priority(row.get('sampling_priority_v1'),row['id'])
        masses=[(row['sampling_priority_v1']['value']+config['epsilon'])**config['alpha'] for row in rows]
        total=math.fsum(masses);values=[v/total for v in masses]
        minimum=min(values)
        # Algebraically identical to the normalized (1/(n*p))**beta formula,
        # with no large intermediate reciprocal for a tiny priority.
        corrections=[(minimum/p)**config['beta'] for p in values]
    require(all(math.isfinite(p) and p>0 for p in values) and abs(math.fsum(values)-1)<1e-12,
        'declarative.replay-sampling.probabilities: invalid normalized distribution')
    return values,corrections


class Distribution:
    def __init__(self,sequences,config,mode):
        validate(config);self.config=config;self.count=config['sequences_per_minibatch'];self.groups=[]
        require([s['id'] for s in sequences]==sorted({s['id'] for s in sequences}),
            'declarative.replay-sampling.identity-order: stable unique IDs required')
        if not sequences:
            require(mode in ('uniform','balanced_available'),'replay-balance.stratum-empty: balanced replay requires both strata')
            return
        if mode=='balanced_available':
            mode='balanced' if {s['stratum'] for s in sequences}=={'large','small'} else 'uniform'
        require(mode in ('uniform','balanced'),'declarative.replay-sampling.mode: unsupported sampler')
        self.balanced=mode=='balanced'
        names=('large','small') if self.balanced else ('all',)
        for name in names:
            rows=[s for s in sequences if s['stratum']==name] if self.balanced else sequences
            require(bool(rows),f'replay-balance.stratum-empty: {name}')
            values,corrections=probabilities(rows,config)
            cdf=None
            if config['kind']!='uniform':
                cdf=torch.tensor(values,dtype=torch.float64).cumsum(0);cdf[-1]=1.
            label=name if name!='all' or len({s['stratum'] for s in rows})!=1 else rows[0]['stratum']
            self.groups.append(dict(stratum=label,rows=rows,probabilities=values,corrections=corrections,cdf=cdf,
                draws=self.count//2 if self.balanced else self.count))

    def draw(self,generator,order_generator):
        selected=[]
        for group in self.groups:
            if group['cdf'] is None:
                indices=torch.randint(len(group['rows']),(group['draws'],),generator=generator).tolist()
            else:
                uniforms=torch.rand(group['draws'],generator=generator,dtype=torch.float64)
                indices=torch.searchsorted(group['cdf'],uniforms,right=True).tolist()
            for index in indices:
                row=group['rows'][index]
                selected.append(dict(row,sil_correction=group['corrections'][index]) if priority_enabled(self.config) else row)
        if selected and self.balanced:
            selected=[selected[i] for i in torch.randperm(self.count,generator=order_generator).tolist()]
        return selected

    def summary(self):
        return dict(settings=self.config,groups=[dict(stratum=g['stratum'],entries=len(g['rows']),
            min_probability=min(g['probabilities']),max_probability=max(g['probabilities']),
            effective_sample_size=1/math.fsum(p*p for p in g['probabilities'])) for g in self.groups])
