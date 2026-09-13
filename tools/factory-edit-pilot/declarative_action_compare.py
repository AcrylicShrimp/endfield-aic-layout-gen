"""Paired random decision traces and independent one-decision opportunity samples."""
from collections import Counter
import json
import random
import time

from declarative_action_runtime import Runtime, bound, check, reference, write
from declarative_action_sampling import draw
from declarative_edit_audit import audit
from blueprint_edit_audit import audit as old_audit


def geometry_key(observation):
    # Used only for the explicit cold paired-control comparison after execution.
    return {k:observation[k] for k in ('facilities','edges','conflicts','best_metrics')}


def one_episode(runtime, request, original, base, arm, seed, count, options, output, independent=False):
    old = arm.startswith('legacy_')
    admitted=time.perf_counter()
    session=runtime.open(request,original,old=old,decisions=count*(3 if arm=='legacy_scripted' else 1))
    admission_seconds=time.perf_counter()-admitted
    trace=[]
    cumulative=dict(native_seconds=0.,transition_seconds=0.,cpu_seconds=0.,route_expansions=0,
        preflight_expansions=0,route_calls=0,validations=0,native_attempts=0)
    initial_score=base['initial_metrics']['score']
    first_improvement=None
    for index in range(count):
        step_began=time.perf_counter()
        cpu_began=time.process_time()
        observation=runtime.observe(session)
        if observation['done']:
            break
        action=draw(observation,base,arm,seed,index,options)
        selected_action=action
        actions=[action]
        if arm=='legacy_scripted':
            actions += [dict(kind='repair_all'),dict(kind='commit')]
        outcomes=[]
        for action in actions:
            if observation['done']:
                break
            outcome=runtime.step(session,observation,action)
            outcomes.append(outcome)
            cumulative['native_seconds']+=outcome['seconds']
            cumulative['native_attempts']+=outcome['charged']
            details=outcome.get('details',{})
            cumulative['route_expansions']+=details.get('route_expansions',0)
            if old:
                routes=details.get('routes',[])
                cumulative['route_calls']+=len(routes)
                cumulative['preflight_expansions']+=sum(r['result'].get('preflight_expanded',0) for r in routes)
                cumulative['validations']+='native_acceptance' in details
            else:
                cumulative['route_calls']+=details['route_calls']
                cumulative['preflight_expansions']+=details['preflight_expansions']
                cumulative['validations']+=details['validations']
            observation=runtime.observe(session)
        cumulative['transition_seconds']+=time.perf_counter()-step_began
        cumulative['cpu_seconds']+=time.process_time()-cpu_began
        score=observation['best_metrics']['score']
        if first_improvement is None and score[0]<initial_score[0]:
            first_improvement=dict(decision=index+1,**cumulative)
        pending=sum(e['status']=='pending' for e in observation['edges'])
        def compact(outcome):
            details=dict(outcome['details'])
            if old and 'routes' in details:
                details['routes']=[dict(edge=r['edge'],**{k:r['result'][k] for k in
                    ('status','expanded','preflight_expanded','preflight_status')}) for r in details['routes']]
            return dict(status=outcome['status'],reward=outcome['reward'],diagnostics=outcome['diagnostics'],details=details)
        row=dict(decision=index+1,action=selected_action,outcomes=[compact(o) for o in outcomes],
            pending=pending,conflicts=len(observation['conflicts']),best_score=score,
            draft_validated=observation['draft_factory_validated'],cumulative=dict(cumulative))
        trace.append(row)
    final_observation=runtime.observe(session)
    # Exports, topology audits and checkpoint hashing happen after timed actions.
    checkpoint=json.loads(session.checkpoint_json()) if output is not None else None
    final=json.loads(session.finish_json())
    audit_result=(old_audit if old else audit)(original,base['blueprint'],final['best']) if not independent else None
    row=dict(arm=arm,seed=seed,independent=independent,initial_score=initial_score,
        final_score=final['best_metrics']['score'],improved=final['best_metrics']['score'][0]<initial_score[0],
        score_improved=final['best_metrics']['score']<initial_score,first_improvement=first_improvement,
        decisions=len(trace),admission_seconds=admission_seconds,work=cumulative,trace=trace,
        final_pending=sum(e['status']=='pending' for e in final_observation['edges']),
        final_conflicts=len(final_observation['conflicts']),unfinished_draft=final['unfinished_draft'],
        factory_validated=final['factory_validated'],audit=audit_result)
    if output is not None:
        write(output.with_suffix('.checkpoint.json'),checkpoint)
        write(output.with_suffix('.best.json'),final['best'])
        write(output.with_suffix('.json'),row)
        row['source']=reference(output.with_suffix('.json'))
    return row,geometry_key(final_observation),final['best']


def compare_case(spec, library, case, output):
    output.mkdir(parents=True)
    runtime=Runtime(spec,library)
    request,original=bound(case['files']['request']),bound(case['files']['initial'])
    session=runtime.open(request,original)
    base=runtime.observe(session)
    session.finish_json()
    options=spec['comparison']
    index=case['index']
    sequential=[]
    controls={}
    jobs=[(stream,arm) for stream in range(options['streams']) for arm in options['arms']]
    random.Random(options['order_seed']+index).shuffle(jobs)
    began=time.monotonic()
    for stream,arm in jobs:
        seed=options['action_seed_base']+index*10000+stream*100
        path=output/f'{arm}-stream-{stream}'
        row,geometry,_=one_episode(runtime,request,original,base,arm,seed,options['decisions'],options,path)
        row['stream']=stream
        sequential.append(row)
        if arm in ('legacy_scripted','automatic_common'):
            controls[stream,arm]=(row,geometry)
    equivalence=[]
    for stream in range(options['streams']):
        old,a=controls[stream,'legacy_scripted']
        new,b=controls[stream,'automatic_common']
        common=min(len(old['trace']),len(new['trace']))
        decisions_equal=True
        scores_equal=True
        for first,second in zip(old['trace'][:common],new['trace'][:common],strict=True):
            pose=dict(second['action']); pose.pop('ports'); pose['kind']='pose'
            decisions_equal &= first['action']==pose
            scores_equal &= first['best_score']==second['best_score']
        evidence=dict(stream=stream,common_decisions=common,decisions_equal=decisions_equal,
            scores_equal=scores_equal,final_geometry_equal=a==b,old_decisions=len(old['trace']),new_decisions=len(new['trace']))
        equivalence.append(evidence)
        check(decisions_equal and scores_equal and (a==b or len(old['trace'])!=len(new['trace'])),
              'common-control-divergence','Scripted common and automatic common diverged',evidence)
    independent=[]
    selected={}
    jobs=[(proposal,arm) for proposal in range(options['independent_proposals']) for arm in options['independent_arms']]
    random.Random(options['order_seed']+index+100000).shuffle(jobs)
    for proposal,arm in jobs:
        seed=options['independent_seed_base']+index*100000+proposal*1009
        row,_,best=one_episode(runtime,request,original,base,arm,seed,1,options,None,independent=True)
        row['proposal']=proposal
        independent.append(row)
        if row['improved'] and arm not in selected:
            path=output/f'independent-selected-{arm}.json'
            write(path,best)
            selected[arm]=dict(**reference(path),audit=audit(original,base['blueprint'],best))
    result=dict(schema='declarative-action-case-v1',status='complete',case=case,seconds=time.monotonic()-began,
        controls=equivalence,sequential=sequential,independent=independent,selected=selected,
        runtime_inputs=runtime.inputs)
    write(output/'report.json',result)
    return dict(status='complete',case_index=index,report=reference(output/'report.json'),
        sequential=len(sequential),independent=len(independent),seconds=result['seconds'])


def summaries(cases, options):
    result={}
    for population in ('sequential','independent'):
        arms=options['arms'] if population=='sequential' else options['independent_arms']
        for arm in arms:
            rows=[r for c in cases for r in c[population] if r['arm']==arm]
            outcomes=[o for r in rows for t in r['trace'] for o in t['outcomes']]
            action_outcomes=[(t['action']['kind'],t['outcomes'][-1]['status']) for r in rows for t in r['trace'] if t['outcomes']]
            result[f'{population}/{arm}']=dict(episodes=len(rows),improved=sum(r['improved'] for r in rows),
                score_improved=sum(r['score_improved'] for r in rows),decisions=sum(r['decisions'] for r in rows),
                mean_relative_area_gain=sum(1-r['final_score'][0]/r['initial_score'][0] for r in rows)/max(1,len(rows)),
                area_gains=[r['initial_score'][0]-r['final_score'][0] for r in rows],
                failures_with_pending=sum(not r['improved'] and r['final_pending']>0 for r in rows),
                failures_with_conflicts=sum(not r['improved'] and r['final_conflicts']>0 for r in rows),
                statuses=dict(Counter(o['status'] for o in outcomes)),
                diagnostics=dict(Counter(d['code'] for o in outcomes for d in o['diagnostics'])),
                action_outcomes=dict(Counter('/'.join(pair) for pair in action_outcomes)),
                work={key:sum(r['work'][key] for r in rows) for key in rows[0]['work']},
                first_improvements=[r['first_improvement'] for r in rows if r['first_improvement']],
                by_case=[dict(case=c['case']['index'],product=c['case']['product'],
                    episodes=sum(r['arm']==arm for r in c[population]),
                    improved=sum(r['arm']==arm and r['improved'] for r in c[population]),
                    relative_area_gains=[1-r['final_score'][0]/r['initial_score'][0] for r in c[population] if r['arm']==arm]) for c in cases])
            if population=='sequential':
                prefixes={}
                for metric,limits in [('decision',options['decision_prefixes']),('route_expansions',options['routing_prefixes']),
                    ('transition_seconds',options['time_prefixes_seconds'])]:
                    for limit in limits:
                        scores=[]
                        for row in rows:
                            eligible=[t for t in row['trace'] if (t['decision'] if metric=='decision' else t['cumulative'][metric])<=limit]
                            scores.append(eligible[-1]['best_score'] if eligible else row['initial_score'])
                        prefixes[f'{metric}/{limit}']=dict(improved=sum(score[0]<row['initial_score'][0] for score,row in zip(scores,rows)),
                            mean_relative_area_gain=sum(1-score[0]/row['initial_score'][0] for score,row in zip(scores,rows))/len(rows))
                result[f'{population}/{arm}']['prefixes']=prefixes
    return result
