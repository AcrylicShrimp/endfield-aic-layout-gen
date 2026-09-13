"""Bounded complete-state native audit of capped geometric success credit."""
import argparse
from copy import deepcopy
import gzip
import json
import math
from pathlib import Path
import signal
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from declarative_action_runtime import Runtime, assignments, bound, read, reference, sha
from blueprint_execution_resources import ResourceGuard
from declarative_success_features import bounds, features, score, segment_row, witness_bounds
from full_factory_selection import physical_key

SCHEMA='declarative-success-native-audit-v1'
FAMILIES={'pose_reversal','band_pose_contraction'}


def require(ok,code,message):
    if not ok:raise ValueError(f'success-native-audit.{code}: {message}')


def write(path,value):
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True)
    with (gzip.open(path,'wt',compresslevel=3) if str(path).endswith('.gz') else path.open('w')) as stream:
        json.dump(value,stream,allow_nan=False);stream.write('\n')


def canonical_witness(witness):
    """Translate all physical DTO coordinates, preserving ports and directed edges."""
    value=deepcopy(witness);x0,y0,_,_=witness_bounds(value)
    def move(point):point.update(x=point['x']-x0,y=point['y']-y0)
    for placement in value['placements']:move(placement)
    for network in value['transport_networks']:
        for cell in network['cells']:move(cell)
        for terminal in network.get('terminals',[]):move(terminal['position'])
        for segment in network.get('segments',[]):
            move(segment['from']);move(segment['to'])
    for component in value.get('logistics_components',[]):move(component['position'])
    return value


def physical(observation):
    return {key:observation[key] for key in ('blueprint_sha256','facilities','nodes','components','port_assignments','edges','best_metrics')}


def resource_limited(outcome):
    codes={d['code'] for d in outcome.get('diagnostics',[])}
    return bool(codes & {'declarative.elapsed','declarative.budget-exhausted'}) or any(
        'budget' in row['status'] or 'exhaust' in row['status'] for row in outcome.get('details',{}).get('routes',[]))


def outward(row):
    x0,y0,x1,y1=bounds(row['before']);center=((x0+x1)/2,(y0+y1)/2)
    def distance(p):return math.hypot(p[1]+p[3]/2-center[0],p[2]+p[4]/2-center[1])
    return any(distance(next(p for p in item['after'] if p[0]==item['instance']))>
        distance(next(p for p in item['before'] if p[0]==item['instance']))+1e-12 for item in row['places'])


def execute(runtime,checkpoint,actions,guard):
    session=runtime.restore(deepcopy(checkpoint));samples=[];outcomes=[];observations=[];status=None
    for step,action in enumerate(actions,1):
        guard();before=runtime.observe(session)
        result=runtime.step(session,before,action);after=runtime.observe(session)
        outcomes.append(result);observations.append(dict(before=physical(before),after=physical(after)))
        if resource_limited(result):status='resource_exhausted';break
        if result['status']=='rejected':status='invalid_action';break
        if after['revision']!=before['revision']:
            witness=json.loads(session.validated_draft_json()) if result['reward']>0 else None
            samples.append(dict(action=action,policy_action=step,reward=result['reward'],
                observation=before,validated_witness=witness))
    final_observation=runtime.observe(session);saved=json.loads(session.checkpoint_json())
    final=json.loads(session.finish_json())
    row=feature=None
    if status is None:
        if not samples or samples[-1]['reward']<=0 or not final_observation['draft_factory_validated']:
            status='inconclusive_attribution'
        elif any(s['reward']>0 for s in samples[:-1]):
            status='inconclusive_attribution'
        else:
            row=segment_row(samples,'native-audit');feature=features(row)
            feature['rarity']=0.
            feature['capped_exterior_bonus']=score(feature,dict(rare=0.,center=0.,exterior=1.,cap_geometry=True))-feature['gain']
            status='reproduced_success'
    canonical=canonical_witness(final['best'])
    return dict(status=status,actions=actions,outcomes=outcomes,observations=observations,
        final=final,final_canonical=canonical,final_physical_sha256=physical_key(dict(witness=canonical,
            ports_by_node={str(i):port for i,port in enumerate(saved['best_state']['ports'])})),
        features=feature,outward_move=outward(row) if row else False,
        reason='intermediate_positive_improvement_or_no_final_segment_success' if status=='inconclusive_attribution' else None)


def validate_prepend(variant,observation):
    family=variant['family'];commands=variant['prepend']
    require(family in FAMILIES and len(commands)==2,'variant','unknown family or non-paired prefix')
    second=commands[1]
    require(second['kind']=='place' and 'ports' in second,'variant','prefix must end with an explicit original pose/ports')
    initial=next((p for p in observation['facilities'] if p['instance']==second['instance']),None)
    require(initial is not None and all(second[k]==initial[k] for k in ('x','y','rotation')) and
        sorted(second['ports'],key=lambda r:r['node'])==sorted(assignments(observation,second['instance']),key=lambda r:r['node']),
        'variant','contraction/reversal must restore the exact initial facility pose and ports')
    first=commands[0]
    if family=='pose_reversal':
        require(first['kind']=='place' and first['instance']==second['instance'] and first['ports']==second['ports'] and
            any(first[k]!=second[k] for k in ('x','y','rotation')),'variant','pure reversal needs one actual pose change')
    else:
        require(first['kind']=='insert_band' and first['axis'] in ('x','y') and first['count']>0 and
            first['at']<=initial[first['axis']],'variant','band must shift the contracted facility')


def comparison(baseline,variant,family):
    decisive=(baseline['status']==variant['status']=='reproduced_success' and
        baseline['final_physical_sha256']==variant['final_physical_sha256'] and
        baseline['final']['best_metrics']['score'][0]==variant['final']['best_metrics']['score'][0] and
        abs(baseline['features']['gain']-variant['features']['gain'])<1e-12)
    extra=variant['features']['capped_exterior_bonus']-baseline['features']['capped_exterior_bonus'] if decisive else None
    return dict(family=family,decisive=decisive,geometry_bonus_delta=extra,
        gratuitous_loop_bonus=decisive and extra>1e-12,
        reason=None if decisive else 'rejected_resource_limited_split_success_or_different_complete_endpoint')


def eligibility(cases,resource_stop=False):
    controls={case['role'] for case in cases if case.get('control_reproduced')}
    pairs=[pair for case in cases for pair in case.get('comparisons',[])]
    decisive={pair['family'] for pair in pairs if pair['decisive']}
    reasons=[]
    for role in ('outward_success','non_loop_compaction'):
        if role not in controls:reasons.append(f'missing_reproduced_control:{role}')
    for family in sorted(FAMILIES-decisive):reasons.append(f'missing_decisive_pair:{family}')
    if any(pair['gratuitous_loop_bonus'] for pair in pairs):reasons.append('verified_gratuitous_geometry_bonus')
    if resource_stop:reasons.append('resource_cap_reached')
    return dict(geometry_eligible=not reasons,reasons=reasons,decisive_families=sorted(decisive),reproduced_controls=sorted(controls))


def case_checkpoint(runtime,case,guard):
    if 'checkpoint' in case:
        saved=bound(case['checkpoint'])
        require(saved.get('schema')=='declarative-edit-checkpoint-v1','source','complete native checkpoint required')
        session=runtime.restore(saved)
    else:
        value=bound(case['input'])
        require('request' in value and 'original' in value and 'transport_networks' in value['original'],
            'source','complete request/start required; geometric exports are insufficient')
        session=runtime.open(value['request'],value['original'])
    for action in case.get('prefix',[]):
        guard();result=runtime.step(session,runtime.observe(session),action)
        if resource_limited(result):return None,None,'resource_exhausted'
        if result['status']=='rejected':return None,None,'invalid_prefix'
    if case.get('expected_start'):
        require(physical(runtime.observe(session))==bound(case['expected_start']),'prefix','actual physical start was not reproduced')
    return json.loads(session.checkpoint_json()),runtime.observe(session),None


def audit(manifest_path,library,output):
    manifest=read(manifest_path);output=Path(output).resolve()
    require(manifest['schema']=='declarative-success-native-cases-v1' and len(manifest['cases'])<=8,'manifest','at most eight bound cases required')
    require(len({c['id'] for c in manifest['cases']})==len(manifest['cases']),'manifest','duplicate case ID')
    require(all(len(c.get('variants',[]))<=2 for c in manifest['cases']),'manifest','at most two variants per case')
    launch=bound(manifest['limits_from_launch'])
    require(manifest['spec']['native']==launch['config']['native'] and manifest['spec']['probe_spec']==launch['config']['probe_spec'],
        'limits','native limits and catalogs must match the bound existing launch')
    require(not output.exists(),'output','fresh output required');output.mkdir(parents=True)
    preparation=manifest.get('provenance',{}).get('seconds',0.)
    require(0<=preparation<300,'limits','case preparation exhausted the total audit budget')
    began=time.monotonic();seconds=300-preparation;cases=[];replays=0;resource_stop=False
    resources=ResourceGuard(output,seconds,launch['config']['limits']['matrix_rss_bytes'],launch['config']['limits']['min_free_bytes'])
    def guard():
        try:resources()
        except ValueError as error:raise TimeoutError(str(error)) from error
    runtime=Runtime(manifest['spec'],library)
    def alarm(*_):raise TimeoutError('success-native-audit.wall: 300 second cap')
    previous=signal.signal(signal.SIGALRM,alarm);signal.setitimer(signal.ITIMER_REAL,max(.001,seconds-(time.monotonic()-began)))
    try:
        for case in manifest['cases']:
            guard();row=dict(id=case['id'],role=case['role'],comparisons=[],control_reproduced=False)
            cases.append(row)
            if 'unavailable' in case:
                row.update(status='unavailable_source',reason=case['unavailable']);continue
            try:
                checkpoint,observation,failure=case_checkpoint(runtime,case,guard)
                if failure:
                    row['status']=failure
                    if failure=='resource_exhausted':resource_stop=True;break
                    continue
                write(output/f'{case["id"]}/start-checkpoint.json.gz',checkpoint)
                for action in case['actions']:
                    require(action['kind']!='place' or 'ports' in action,'ports','record complete actual port assignments')
                baseline=execute(runtime,checkpoint,case['actions'],guard);replays+=1
                row.update(status=baseline['status'],baseline={k:baseline[k] for k in ('status','features','final_physical_sha256','outward_move')})
                write(output/f'{case["id"]}/baseline.json.gz',baseline)
                if baseline['status']=='resource_exhausted':resource_stop=True;break
                reproduced=baseline['status']=='reproduced_success'
                if case.get('expected_final'):
                    reproduced=reproduced and physical_key(canonical_witness(bound(case['expected_final'])))==physical_key(baseline['final_canonical'])
                if case.get('expected_area') is not None:
                    reproduced=reproduced and case['expected_area']==baseline['final']['best_metrics']['score'][0]
                row['control_reproduced']=reproduced and (
                    case['role']=='outward_success' and baseline['outward_move'] or
                    case['role']=='non_loop_compaction' and len(case['actions'])==1 and baseline['features']['exterior_release']>0)
                for variant in case.get('variants',[]):
                    guard();require(replays<24,'replay-limit','at most 24 native replays')
                    validate_prepend(variant,observation)
                    actual=execute(runtime,checkpoint,variant['prepend']+case['actions'],guard);replays+=1
                    write(output/f'{case["id"]}/{variant["id"]}.json.gz',actual)
                    pair=comparison(baseline,actual,variant['family'])
                    if not reproduced:pair.update(decisive=False,geometry_bonus_delta=None,gratuitous_loop_bonus=False,reason='baseline_expected_result_not_reproduced')
                    row['comparisons'].append(dict(variant=variant['id'],status=actual['status'],**pair))
                    if actual['status']=='resource_exhausted':resource_stop=True;break
                if resource_stop:break
            except (OSError,KeyError,ValueError) as error:
                row.update(status='unavailable_source' if isinstance(error,OSError) else 'invalid_case_input',reason=str(error))
            write(output/'progress.json',dict(schema=SCHEMA,status='running',cases=cases,replays=replays))
    except TimeoutError as error:
        resource_stop=True
        if cases:cases[-1].update(status='resource_exhausted',reason=str(error))
    finally:
        signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,previous)
    result=dict(schema=SCHEMA,status='complete' if not resource_stop else 'resource_limited',cases=cases,
        **eligibility(cases,resource_stop),replays=replays,seconds=time.monotonic()-began,
        preparation_seconds=preparation,total_seconds=preparation+time.monotonic()-began,
        resource=resources.last,
        limits=dict(cases=8,variants_per_case=2,replays=24,seconds=seconds,native=manifest['spec']['native']),
        provenance=dict(manifest=reference(manifest_path),library=reference(library),runtime_inputs=runtime.inputs,
            script=reference(__file__)),scope='Local complete-state attribution check; category rarity is separate and no learning benefit is claimed.')
    write(output/'report.json',result);return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    run=commands.add_parser('audit');run.add_argument('--manifest',type=Path,required=True)
    run.add_argument('--library',type=Path,required=True);run.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    try:
        result=audit(args.manifest,args.library,args.output)
        print(json.dumps({k:result[k] for k in ('status','geometry_eligible','reasons','replays','seconds')}));return 0
    except (OSError,KeyError,ValueError) as error:
        print(json.dumps(dict(code='success-native-audit.input',severity='error',stage='audit',message=str(error),entity=str(args.manifest))),file=sys.stderr)
        return 2


if __name__=='__main__':raise SystemExit(main())
