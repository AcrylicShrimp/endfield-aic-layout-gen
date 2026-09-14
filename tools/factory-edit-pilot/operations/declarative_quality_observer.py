"""Study-only native-quality allocation, using existing SIL measurement transfers."""
from bisect import bisect_right
from collections import defaultdict
from copy import deepcopy
import statistics

from blueprint_ppo_update import require
from declarative_replay_retention import stratum
from declarative_success_features import extract, validate_metadata


def category(entry):
    if entry['retention_features_v1']['has_place']:return 'facility'
    return 'deletion' if all(l['stage']=='delete_band' for l in entry['labels']) else 'other'


def quartiles(entries,strata):
    groups=defaultdict(list)
    for e in entries.values():groups[stratum(e,strata)+'/'+category(e)].append(e['retention_features_v1']['gain'])
    return {k:dict(entries=len(v),cuts=[sorted(v)[min(len(v)-1,int(len(v)*q))] for q in (.25,.5,.75)],
        distinct_values=len(set(v))) for k,v in sorted(groups.items())}


class Observer:
    def __init__(self,learner,initial_quartiles):
        self.learner,self.quartiles=learner,initial_quartiles
        self.groups={};self.attempts=[];self.pool=[]
        learner.replay_observer=self.measure
        learner.buffer.admission_observer=self.admit

    def key(self,e):
        name=stratum(e,self.learner.config['strata'])+'/'+category(e)
        cuts=self.quartiles.get(name,{}).get('cuts')
        return name,('q'+str(bisect_right(cuts,e['retention_features_v1']['gain'])+1) if cuts else 'no_initial_cohort')

    def begin(self):
        self.groups={};self.attempts=[];groups=defaultdict(list)
        for e in self.learner.buffer.state['entries'].values():
            validate_metadata(e.get('retention_features_v1'),e['id'])
            groups[self.key(e)].append(e['retention_features_v1']['gain'])
        self.pool=[dict(group=k[0],quality_bin=k[1],entries=len(v),mean_native_gain=statistics.mean(v),
            min_native_gain=min(v),max_native_gain=max(v)) for k,v in sorted(groups.items())]

    def measure(self,values,utility_groups,allocation_groups):
        require(len(values)==len(utility_groups)==len(allocation_groups),'quality-study.measurement: label coverage')
        for value,(identity,stage_weight,first),(correction,count) in zip(values,utility_groups,allocation_groups,strict=True):
            e=self.learner.buffer.state['entries'][identity];key=self.key(e)
            row=self.groups.setdefault(key,dict(group=key[0],quality_bin=key[1],draws=0,native_gain_mass=0.,
                labels=0,positive_labels=0,zero_labels=0,stage_weight_mass=0.,positive_stage_weight=0.,
                zero_stage_weight=0.,corrected_positive_stage_weight=0.,corrected_zero_stage_weight=0.,
                raw_sil_contribution=0.,corrected_sil_contribution=0.,positive_advantage_mass=0.))
            positive,_,_,raw=value;factor=stage_weight*correction/count
            row['draws']+=int(first);row['native_gain_mass']+=int(first)*e['retention_features_v1']['gain']
            row['labels']+=1;row['positive_labels']+=int(positive>0);row['zero_labels']+=int(positive==0)
            row['stage_weight_mass']+=stage_weight
            row['positive_stage_weight']+=stage_weight*(positive>0);row['zero_stage_weight']+=stage_weight*(positive==0)
            row['corrected_positive_stage_weight']+=factor*(positive>0);row['corrected_zero_stage_weight']+=factor*(positive==0)
            row['raw_sil_contribution']+=stage_weight*raw;row['corrected_sil_contribution']+=factor*raw
            row['positive_advantage_mass']+=stage_weight*positive

    def admit(self,key,e,evidence):
        if 'retention_features_v1' not in e:
            e['retention_features_v1']=extract([evidence.sample(l['reference']) for l in e['labels']],e['id'])
        validate_metadata(e['retention_features_v1'],e['id'])
        group,quality_bin=self.key(e)
        self.attempts.append(dict(key=list(key),id=e['id'],group=group,quality_bin=quality_bin,
            native_gain=e['retention_features_v1']['gain'],labels=len(e['labels']),
            stream=e['stream'],episode=e['episode']))

    def finish(self,record):
        events=[e for e in record['events'] if e['kind'] in ('admitted','duplicate')]
        require([e['id'] for e in events]==[e['id'] for e in self.attempts],
            'quality-study.admission: missing ordered terminal attempts')
        for event,attempt in zip(events,self.attempts,strict=True):attempt['outcome']=event['kind']
        rows=[]
        for _,r in sorted(self.groups.items()):
            rows.append(dict(r,mean_native_gain=r['native_gain_mass']/r['draws'],mean_sequence_labels=r['labels']/r['draws']))
        return dict(schema='declarative-quality-allocation-v1',update=record['update'],eligible_pool=self.pool,
            allocation=rows,terminal_attempts=self.attempts,sil_minibatches=len(record['ppo']['success']['selected_ids']),
            sil_coefficient=self.learner.config['success']['coefficient'])
