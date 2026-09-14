"""Geometry-paired native outcomes; allocation and coverage never select arms."""
from collections import Counter,defaultdict
from functools import cmp_to_key
import math
import random
import statistics

from declarative_quality_branch import PROFILES,require


def tie_key(profile):
    mechanisms={'baseline-8':0,'protected-score-8':2}
    return int(profile.rsplit('-',1)[1]),mechanisms.get(profile,1),profile


def ranked(scores):
    require(scores and all(p in PROFILES and math.isfinite(v) for p,v in scores.items()),
        'quality-study.rank: missing or nonfinite outcome')
    def compare(a,b):
        if abs(scores[a]-scores[b])<=1e-12:return (tie_key(a)>tie_key(b))-(tie_key(a)<tie_key(b))
        return -1 if scores[a]>scores[b] else 1
    return sorted(scores,key=cmp_to_key(compare))


def select(scores):
    require(set(scores)==set(PROFILES),'quality-study.selection: all eight trained outcomes required')
    return ranked({p:v for p,v in scores.items() if p!='baseline-8'})[:2]


def geometry_outcomes(rows,panel,strata,replicates=2):
    cases={c['geometry_identity']:c for c in panel['cases']}
    require(len(cases)==len(panel['cases']),'quality-study.panel: duplicate geometry')
    grouped=defaultdict(dict)
    for row in rows:
        identity=row['geometry_identity'];case=cases.get(identity)
        require(case is not None and row['generation_seed']==case['generation_seed'] and row['product']==case['product'],
            'quality-study.evaluation: unexpected identity')
        require(case['stratum'] in strata and case['product'] in strata[case['stratum']],
            'quality-study.evaluation: invalid declared stratum')
        sample=row['sample']
        require(type(sample) is int and 0<=sample<replicates and sample not in grouped[identity],
            'quality-study.evaluation: missing/duplicate action replicate')
        require(row['factory_validated'] and row['start_score'][0]>0 and 0<=row['best_score'][0]<=row['start_score'][0],
            'quality-study.evaluation: invalid native retained output')
        actual=100*(row['start_score'][0]-row['best_score'][0])/row['start_score'][0]
        require(math.isclose(actual,row['total_area_reduction_percent'],abs_tol=1e-10),
            'quality-study.evaluation: physical area and reported gain differ')
        require(row['action_seed']==case['action_seed']+sample and not row.get('uniform',False),
            'quality-study.evaluation: changed action RNG or uniform policy')
        grouped[identity][sample]=actual
    require(set(grouped)==set(cases) and all(len(v)==replicates for v in grouped.values()),
        'quality-study.evaluation: incomplete geometry cohort')
    return {identity:dict(stratum=case['stratum'],action_replicates=[grouped[identity][i] for i in range(replicates)],
        mean=statistics.mean(grouped[identity].values())) for identity,case in cases.items()}


def aggregate(replicates,strata):
    require(replicates and all(set(r)==set(replicates[0]) for r in replicates),'quality-study.aggregate: unpaired geometries')
    cases={}
    for identity,first in replicates[0].items():
        require(all(r[identity]['stratum']==first['stratum'] for r in replicates),'quality-study.aggregate: changed stratum')
        cases[identity]=dict(stratum=first['stratum'],mean=statistics.mean(r[identity]['mean'] for r in replicates),
            training_replicates=[r[identity] for r in replicates])
    by={s:statistics.mean(c['mean'] for c in cases.values() if c['stratum']==s) for s in strata}
    return dict(mean=statistics.mean(by.values()),by_stratum=by,geometries=cases)


def paired(candidate,baseline,strata,seed=714600000,resamples=10000):
    left,right=candidate['geometries'],baseline['geometries']
    require(left.keys()==right.keys(),'quality-study.bootstrap: unpaired geometry cohort')
    groups={s:[] for s in strata};outcomes=Counter()
    for identity in sorted(left):
        a,b=left[identity],right[identity]
        require(a['stratum']==b['stratum'] and len(a['training_replicates'])==len(b['training_replicates']),
            'quality-study.bootstrap: unpaired stratum/training replicates')
        delta=a['mean']-b['mean'];groups[a['stratum']].append(delta)
        outcomes['tie' if abs(delta)<=1e-12 else 'win' if delta>0 else 'loss']+=1
    rng=random.Random(seed);draws=sorted(statistics.mean(statistics.mean(rng.choices(v,k=len(v))) for v in groups.values())
        for _ in range(resamples))
    return dict(mean_delta_pp=candidate['mean']-baseline['mean'],by_stratum_delta_pp={
        s:statistics.mean(v) for s,v in groups.items()},geometry_outcomes=dict(outcomes),
        interval_95_pp=[draws[int(.025*resamples)],draws[min(resamples-1,int(.975*resamples))]],
        resamples=resamples,seed=seed,scope='Whole geometries stratified by size; both action/training replicates attached. Conditional on the two observed random initializations.')


def allocation(records):
    groups={};attempts=defaultdict(list);pools=defaultdict(list);grid=set()
    additive=('draws','native_gain_mass','labels','positive_labels','zero_labels','stage_weight_mass',
        'positive_stage_weight','zero_stage_weight','corrected_positive_stage_weight','corrected_zero_stage_weight',
        'raw_sil_contribution','corrected_sil_contribution','positive_advantage_mass')
    for record in records:
        observed=record['quality_allocation']
        grid.update((g,b) for g,v in observed.get('quality_partition',{}).items() for b in v.get('labels',[]))
        for row in observed['allocation']:
            key=(row['group'],row['quality_bin']);total=groups.setdefault(key,{k:0 for k in additive})
            for k in additive:total[k]+=row[k]
        for row in observed['eligible_pool']:pools[(row['group'],row['quality_bin'])].append(row)
        for row in observed['terminal_attempts']:attempts[(row['group'],row['quality_bin'])].append(row)
    result=[]
    for key in sorted(grid|set(groups)|set(pools)|set(attempts)):
        r=groups.get(key,{k:0 for k in additive});pool=pools[key];terminal=attempts[key]
        entries=sum(x['entries'] for x in pool)
        result.append(dict(group=key[0],quality_bin=key[1],**r,
            mean_native_gain_per_draw=r['native_gain_mass']/r['draws'] if r['draws'] else None,
            mean_sequence_labels=r['labels']/r['draws'] if r['draws'] else None,
            eligible_entry_updates=entries,
            mean_eligible_native_gain=sum(x['entries']*x['mean_native_gain'] for x in pool)/entries if entries else None,
            terminal_attempts=len(terminal),terminal_outcomes=dict(Counter(x['outcome'] for x in terminal)),
            terminal_native_gain=[x['native_gain'] for x in terminal]))
    return dict(updates=len(records),groups=result,interpretation='Physical gain and learning allocation; loss magnitude is not a policy-quality ranking.')
