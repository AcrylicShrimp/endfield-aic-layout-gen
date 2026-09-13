"""Bounded preregistered fixed-arrival, frozen-model retention/sampling screen."""
import argparse
from collections import Counter
from copy import deepcopy
import hashlib
import json
import math
from pathlib import Path
import signal
import statistics
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import torch
from blueprint_execution_resources import ResourceGuard
from blueprint_ppo_store import read, save, sha, write
from blueprint_ppo_update import digest
from declarative_replay_retention import ReplacementIndex, observe
from declarative_replay_sampling import Distribution, PriorityBootstrap, observe_priorities
from declarative_retention_corpus import Corpus, frozen_cache, need

SCHEMA='declarative-retention-screen-v1'
REGISTERED=dict(schema=SCHEMA,seeds=[17,29,41],counts=[8,16,32],max_sequences=4096,max_labels=262144,
    seconds=600,max_output_bytes=2*1024**3,device='cpu',cpu_threads=1,category_fraction=.1,protection_updates=64,
    minimum_new_ids=4096,minimum_facility_episodes=10,horizon=32,minimum_exposure_delta=.05,
    minimum_geometry_delta=.02,minimum_deletion_coverage_ratio=.9,maximum_storage_ratio=1.25)


def profiles(geometry):
    result=[]
    for retention in ('v1','v2'):
        for sampling in ('uniform','score_mixture','positive_advantage'):
            for count in (8,16,32):result.append(profile(retention,sampling,count))
    result.append(profile('q0','uniform',8))
    if geometry:
        for retention in ('v1','v2'):
            for count in (8,16,32):result.append(profile(retention,'score_mixture',count,True))
    return result


def profile(retention,sampling,count,geometry=False):
    setting=dict(schema='declarative-success-sampling-v1',kind=sampling,sequences_per_minibatch=count)
    if sampling=='score_mixture':setting.update(mixture=.5,score_profile='rare_3pp_exterior_capped' if geometry else 'rare_3pp')
    if sampling=='positive_advantage':setting.update(alpha=.6,beta=.1,epsilon=1e-6)
    return dict(id=f'{retention}-{sampling}-{count}'+('-geometry' if geometry else ''),
        retention=retention,sampling=setting,geometry=geometry)


def policy_config(base,p):
    config=dict(base)
    if p['retention']!='v1':config.update(schema='declarative-replay-category-v2',
        category_fraction=.1 if p['retention']=='v2' else 0.,protection_updates=64,
        immature_order='area_capped_exterior' if p['geometry'] else 'area')
    return config


def horizon_counts(cohort,ordinal,draws,present,records,cache):
    for identity,c in cohort.items():
        if not c['anchor']<ordinal<=c['anchor']+32:continue
        r=records[identity];n=draws[identity]
        r['draws']+=n;r['retained']+=identity in present
        if n and cache[identity]['raw']>0:r['positive_draws']+=n


def aggregate(cohort,records,entries,cache):
    result={}
    for name in ('large','small'):
        ids=[i for i,c in cohort.items() if c['place'] and c['stratum']==name]
        deletion=[i for i,c in cohort.items() if c['deletion'] and c['stratum']==name and records[i]['draws']]
        drawn=[records[i]['draws'] for i in ids];total=sum(drawn)
        def mean(values):return statistics.mean(values) if values else None
        result[name]=dict(cohort_ids=len(ids),at_least_once=mean([records[i]['draws']>0 for i in ids]),
            positive_at_least_once=mean([records[i]['positive_draws']>0 for i in ids]),mean_draws=mean(drawn),
            retained_updates=mean([records[i]['retained'] for i in ids]),
            positive_draw_fraction=sum(records[i]['positive_draws'] for i in ids)/total if total else 0.,
            frozen_contribution=sum(records[i]['draws']*cache[i]['raw'] for i in ids),
            draw_concentration=sum(n*n for n in drawn)/(total*total) if total else 0.,
            deletion_geometries=len({entries[i]['screen_geometry'] for i in deletion}),
            deletion_episodes=len({tuple(cohort[i]['episode']) for i in deletion}),
            deletion_mean_gain=mean([entries[i]['retention_features_v1']['gain'] for i in deletion]))
    return result


def run_arm(corpus,cache,p,seed,guard):
    began=time.monotonic();entries={e['id']:deepcopy(e) for e in corpus.initial['entries']}
    for e in entries.values():e['stratum']=corpus.entries[e['id']]['stratum']
    config=policy_config(corpus.config['success']['retention'],p);priority=p['sampling']['kind']=='positive_advantage'
    if priority:
        for i,e in entries.items():e['sampling_priority_v1']=dict(value=cache[i]['positive'],observed_update=corpus.manifest['initial_update'])
    labels=sum(len(e['labels']) for e in entries.values());reasons=Counter();boundaries=[]
    records={i:dict(draws=0,positive_draws=0,retained=0) for i in corpus.cohort};spells={i:dict(start=None,draws=0) for i in entries}
    evicted=[];trace=hashlib.sha256();all_draws=positive_draws=0;loss=effective_loss=0.;replacement_seconds=sampling_seconds=0.
    for row,storage in zip(corpus.updates,corpus.storage,strict=True):
        guard();ordinal=row['update'];started=time.monotonic()
        sequence=[dict(e,stratum=corpus.entries[i]['stratum']) for i,e in sorted(entries.items())]
        need(all(e['admitted_update']<ordinal for e in sequence),'future-observation','same-update replay')
        sampler=Distribution(sequence,p['sampling'],corpus.config['success']['sampler']);draws=Counter()
        rng=torch.Generator().manual_seed(5_000_000+seed*1000+ordinal)
        order=torch.Generator().manual_seed(6_000_000+seed*1000+ordinal)
        for _ in range(row['sil_calls']):
            selected=sampler.draw(rng,order)
            trace.update(('|'.join(e['id'] for e in selected)+'\n').encode())
            for e in selected:
                i=e['id'];draws[i]+=1;all_draws+=1;positive_draws+=cache[i]['raw']>0
                loss+=cache[i]['raw']/p['sampling']['sequences_per_minibatch']
                effective_loss+=cache[i]['raw']*e.get('sil_correction',1.)/p['sampling']['sequences_per_minibatch']
        sampling_seconds+=time.monotonic()-started
        measurements=[dict(id=i,draws=n,mean_weighted_nll=cache[i]['raw'],mean_positive_advantage=cache[i]['positive']) for i,n in sorted(draws.items())]
        observe(entries,measurements,ordinal,config)
        if priority:observe_priorities(entries,measurements,ordinal)
        for i,n in draws.items():entries[i]['uses']+=n;spells[i]['draws']+=n
        # Production observes preexisting entries, then admits terminal attempts.
        started=time.monotonic();index=None;bootstrap=None
        for a in row['attempts']:
            incoming=a['entry'];i=incoming['id']
            if i in entries:
                need(entries[i]['retention_features_v1']==incoming['retention_features_v1'],'identity','duplicate metadata mismatch')
                entries[i]['occurrences']+=1;reasons['duplicate']+=1;continue
            need(len(incoming['labels'])<=262144,'capacity','unrepresentable sequence')
            if priority and bootstrap is None:bootstrap=PriorityBootstrap(entries,corpus.strata)
            while len(entries)>=4096 or labels+len(incoming['labels'])>262144:
                if index is None:index=ReplacementIndex(entries,ordinal,config,corpus.strata)
                removed,detail=index.choose(incoming);reasons[detail['reason']]+=1
                reasons['category_protection']+=bool(detail.get('category_protection'))
                reasons['capacity_override']+=bool(detail.get('capacity_override'))
                reasons['label_limit_eviction']+=labels+len(incoming['labels'])>262144
                if bootstrap:bootstrap.remove(removed)
                old=entries.pop(removed);labels-=len(old['labels']);spell=spells.pop(removed)
                evicted.append(dict(id=removed,**spell,end=ordinal,left_truncated=spell['start'] is None))
                trace.update(f'evict:{removed}\n'.encode())
            entry=deepcopy(incoming);entry['stratum']=corpus.entries[i]['stratum']
            if bootstrap:
                entry['sampling_priority_v1']=dict(value=bootstrap.maximum(entry),observed_update=None);bootstrap.add(entry)
            entries[i]=entry;labels+=len(entry['labels']);spells[i]=dict(start=ordinal,draws=0)
            if index:index.add(entry)
            reasons['admitted']+=1
        replacement_seconds+=time.monotonic()-started
        need(len(entries)<=4096 and labels<=262144,'capacity','hard capacity violated')
        horizon_counts(corpus.cohort,ordinal,draws,entries,records,cache)
        active={l['reference'][0] for e in entries.values() for l in e['labels']}
        boundaries.append(dict(update=ordinal,sequences=len(entries),labels=labels,
            facility_entries=sum(e['retention_features_v1']['has_place'] for e in entries.values()),
            by_stratum=dict(Counter(e['stratum'] for e in entries.values())),
            active_only_bytes=corpus.price(active),mandatory_bytes=storage['mandatory_bytes'],
            total_bytes=corpus.price(active|storage['mandatory']),
            original_baseline_published_bytes=storage['original_baseline_published_bytes'],
            effective_sample_size=sampler.summary()['groups']))
    new_evicted=[e for e in evicted if not e['left_truncated']]
    result=dict(profile=p,seed=seed,metrics=aggregate(corpus.cohort,records,corpus.entries,cache),
        peak_source_bytes=max(b['total_bytes'] for b in boundaries),capacities_valid=True,source_valid=True,
        reasons=dict(reasons),draws=all_draws,positive_draw_fraction=positive_draws/all_draws if all_draws else 0.,
        frozen_loss_sum=loss,effective_frozen_loss_sum=effective_loss,trace_sha256=trace.hexdigest(),
        restored_initial_uses=sum(e['uses'] for e in corpus.initial['entries']),
        initial_survivors=sum(s['start'] is None for s in spells.values()),
        spell_summary=dict(evicted=len(new_evicted),unused_evicted=sum(s['draws']==0 for s in new_evicted),
            right_censored=sum(s['start'] is not None for s in spells.values()),
            initial_evicted=sum(s['left_truncated'] for s in evicted)),
        seconds=time.monotonic()-began,sampling_seconds=sampling_seconds,replacement_seconds=replacement_seconds)
    return result,dict(cohort=records,spells=evicted,censored=[dict(id=i,**s,right_censored=True) for i,s in spells.items()],boundaries=boundaries)


def decide(results,coverage,complete):
    by={(r['profile']['id'],r['seed']):r for r in results};base='v1-uniform-8';seeds=(17,29,41)
    if not complete or not coverage['sufficient']:return dict(verdict='insufficient_evidence',reason='resource_limit_or_common_cohort_coverage',candidate=None)
    baselines=[by[base,s] for s in seeds]
    if any(b['metrics'][t]['deletion_geometries']==0 for b in baselines for t in ('large','small')):
        return dict(verdict='insufficient_evidence',reason='zero_baseline_deletion_coverage_denominator',candidate=None)
    passing=[];gates={}
    for p in {r['profile']['id']:r['profile'] for r in results}.values():
        if p['id']==base:continue
        arms=[by[p['id'],s] for s in seeds];fail=[]
        if not all(a['capacities_valid'] and a['source_valid'] for a in arms):fail.append('capacity_or_source')
        if any(a['peak_source_bytes']>1.25*b['peak_source_bytes'] for a,b in zip(arms,baselines,strict=True)):fail.append('storage')
        for t in ('large','small'):
            av=[a['metrics'][t] for a in arms];bv=[b['metrics'][t] for b in baselines]
            if statistics.median(a['at_least_once'] for a in av)-statistics.median(b['at_least_once'] for b in bv)<.05-1e-12 or any(a['at_least_once']<b['at_least_once'] for a,b in zip(av,bv,strict=True)):fail.append(t+'.exposure')
            if any(a['deletion_geometries']<.9*b['deletion_geometries'] for a,b in zip(av,bv,strict=True)):fail.append(t+'.deletion')
            if statistics.median(a['positive_at_least_once'] for a in av)<statistics.median(b['positive_at_least_once'] for b in bv):fail.append(t+'.positive')
        if p['geometry']:
            matched=[by[p['id'].removesuffix('-geometry'),s] for s in seeds]
            deltas=[statistics.median(a['metrics'][t]['at_least_once'] for a in arms)-statistics.median(b['metrics'][t]['at_least_once'] for b in matched) for t in ('large','small')]
            if min(deltas)<0 or max(deltas)<.02-1e-12:fail.append('geometry_increment')
        gates[p['id']]=fail
        if not fail:passing.append(p)
    passing.sort(key=lambda p:(p['sampling']['sequences_per_minibatch'],p['geometry'],
        int(p['retention']!='v1')+int(p['sampling']['kind']!='uniform'),max(by[p['id'],s]['peak_source_bytes'] for s in seeds),p['id']))
    return dict(verdict='candidate' if passing else 'no_candidate',candidate=passing[0] if passing else None,gates=gates,
        ceiling_strata=[t for t in ('large','small') if statistics.median(b['metrics'][t]['at_least_once'] for b in baselines)>.95])


def contrasts(results):
    by={}
    for r in results:by.setdefault(r['profile']['id'],{})[r['seed']]=r
    summary={};comparisons=[]
    for name,arms in by.items():
        summary[name]={t:{metric:dict(median=statistics.median(values),minimum=min(values),maximum=max(values))
            for metric in ('at_least_once','mean_draws','retained_updates','positive_at_least_once','deletion_geometries')
            if (values:=[a['metrics'][t][metric] for a in arms.values() if a['metrics'][t][metric] is not None])} for t in ('large','small')}
        p=next(iter(arms.values()))['profile'];count=p['sampling']['sequences_per_minibatch'];kind=p['sampling']['kind']
        references=[('combined','v1-uniform-8')]
        if count!=8:references.append(('quantity',profile(p['retention'],kind,8,p['geometry'])['id']))
        if p['retention']=='v2':references.append(('retention',profile('v1',kind,count,p['geometry'])['id']))
        if kind!='uniform':references.append(('sampling',profile(p['retention'],'uniform',count)['id']))
        if p['geometry']:references.append(('geometry_package',name.removesuffix('-geometry')))
        for mechanism,reference in references:
            if reference==name or reference not in by:continue
            paired=sorted(set(arms)&set(by[reference]))
            if not paired:continue
            comparisons.append(dict(profile=name,reference=reference,mechanism=mechanism,paired_seeds=paired,
                at_least_once_delta={t:[arms[s]['metrics'][t]['at_least_once']-by[reference][s]['metrics'][t]['at_least_once']
                    if arms[s]['metrics'][t]['at_least_once'] is not None and by[reference][s]['metrics'][t]['at_least_once'] is not None else None for s in paired] for t in ('large','small')}))
    return dict(summary=summary,contrasts=comparisons)


def compare(corpus_path,config_path,audit_path,output):
    began=time.monotonic();output=Path(output).resolve();need(not output.exists(),'output','fresh output directory required')
    config=read(config_path);need(config==REGISTERED,'config','configuration differs from preregistered matrix')
    audit=read(audit_path);need(audit.get('schema')=='declarative-success-native-audit-v1' and type(audit.get('geometry_eligible')) is bool,'geometry','versioned native eligibility required')
    output.mkdir(parents=True);torch.set_num_threads(1)
    # These are the unchanged main-run resource ceilings, also checked against
    # its bound launch below. The alarm covers long tensor operations between checks.
    parent_config=read(Path(read(corpus_path)['parent']['directory'])/'launch.json')['config']
    limits=parent_config['limits']
    guard=ResourceGuard(output,600,limits['matrix_rss_bytes'],limits['min_free_bytes'])
    results=[];coverage=dict(sufficient=False);cache_info={};error=None;complete=False;stage='corpus'
    def progress(value):write(output/'progress.json',value)
    def timeout(*_):raise TimeoutError('retention-screen.resource: 600 second wall-clock cap')
    previous=signal.signal(signal.SIGALRM,timeout);signal.setitimer(signal.ITIMER_REAL,max(.001,600-(time.monotonic()-began)))
    try:
        corpus=Corpus(corpus_path,guard);coverage=corpus.coverage;progress(dict(stage=stage,coverage=coverage))
        stage='frozen-inference';cache,cache_info=frozen_cache(corpus,guard,progress,config['device'])
        save(output/'frozen-cache.pt.gz',dict(schema=SCHEMA,cache=cache,provenance=cache_info))
        stage='screen';matrix=profiles(audit['geometry_eligible'])
        for p in matrix:
            for seed in config['seeds']:
                guard();result,raw=run_arm(corpus,cache,p,seed,guard);results.append(result)
                write(output/f'arms/{p["id"]}-{seed}.json',dict(result=result,**raw))
                progress(dict(stage=stage,completed=len(results),total=len(matrix)*3,latest=result['profile']['id'],seconds=time.monotonic()-began))
                need(sum(f.stat().st_size for f in output.rglob('*') if f.is_file())<=config['max_output_bytes'],'resource','output byte cap')
        complete=True
    except (TimeoutError,RuntimeError,ValueError,OSError,KeyError) as exc:
        error=dict(code='retention-screen.resource' if isinstance(exc,TimeoutError) or 'resource' in str(exc) else 'retention-screen.input',
            severity='error',stage=stage,message=str(exc),entity=str(corpus_path))
    finally:
        signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,previous)
    receipt=dict(schema=SCHEMA,complete=complete,coverage=coverage,decision=decide(results,coverage,complete),
        results=results,cache=cache_info,error=error,seconds=time.monotonic()-began,resource=guard.last,**contrasts(results),
        provenance=dict(corpus=dict(path=str(corpus_path),sha256=sha(corpus_path)),config=dict(path=str(config_path),sha256=sha(config_path)),
            native_audit=dict(path=str(audit_path),sha256=sha(audit_path)),source_sha256={str(p):sha(p) for p in
                [Path(__file__),Path(__file__).with_name('declarative_retention_corpus.py')]+[Path(__file__).resolve().parents[1]/n for n in
                    ('declarative_replay_retention.py','declarative_replay_sampling.py','declarative_learning_policy.py','blueprint_execution_update.py')]}),
        scope='Fixed actual baseline arrivals, frozen parent model and recorded KL-dependent minibatch schedule; no optimizer or policy-learning claim.')
    write(output/'report.json',receipt);print(json.dumps(dict(decision=receipt['decision'],coverage=coverage,error=error,seconds=receipt['seconds'])))
    return receipt


def main():
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    command=commands.add_parser('compare')
    for name in ('corpus','config','native-audit','output'):command.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    try:
        result=compare(args.corpus,args.config,args.native_audit,args.output)
        return 2 if result['error'] and result['error']['code']=='retention-screen.input' else 0
    except (ValueError,OSError,KeyError) as error:
        print(json.dumps(dict(code='retention-screen.input',severity='error',stage='validation',message=str(error),entity=str(args.corpus))),file=sys.stderr);return 2


if __name__=='__main__':raise SystemExit(main())
