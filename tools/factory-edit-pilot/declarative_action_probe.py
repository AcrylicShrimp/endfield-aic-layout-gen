"""One-off integrated contract gate, including a bounded joint pose/port example search."""
from copy import deepcopy
import json
import random
import time

from declarative_action_runtime import ROOT, Runtime, check, place, read, reference, write
from declarative_action_sampling import draw
from declarative_edit_audit import audit
from probe_blueprint_edit import request_from_initial


def probe(spec, library, output):
    output.mkdir(parents=True)
    runtime = Runtime(spec, library)
    probe_spec = read(ROOT / spec['probe_spec'])
    original = read(ROOT / probe_spec['initial'])
    request = request_from_initial(original)
    write(output / 'request.json', request)
    rows = []
    began = time.monotonic()

    def open_case():
        return runtime.open(request, original, decisions=256, limits=dict(elapsed_ms=60000,
            route_expansions=100000, max_total_route_expansions=10000000))

    def advance(session, action, expected):
        before = runtime.observe(session)
        outcome = runtime.step(session,before,action)
        after = runtime.observe(session)
        check(outcome['status'] in expected, 'probe-status', f'Expected {expected}; got {outcome}', action)
        check(before['blueprint'] == after['blueprint'], 'probe-topology', 'Original blueprint changed')
        return dict(action=action, outcome=outcome, pending=sum(e['status']=='pending' for e in after['edges']),
            conflicts=len(after['conflicts']))

    def save(name, session, trace, draft=False):
        observation = runtime.observe(session)
        if draft:
            witness = json.loads(session.validated_draft_json())
        result = json.loads(session.finish_json())
        if not draft:
            witness = result['best']
        proof = audit(original, observation['blueprint'], witness)
        path = output / f'{name}-witness.json'
        write(path, witness)
        # One explicit cold, independently admitted witness, outside execution timing.
        verifier = runtime.module.FactoryEditSession.open(str(ROOT), str(output/'request.json'), str(path),
            json.dumps(runtime.catalogs), json.dumps(dict(proposals=1,elapsed_ms=60000)))
        check(json.loads(verifier.snapshot_json())['factory_validated'], 'probe-validation', 'Export fails independent admission',name)
        row = dict(name=name, trace=trace, result=result, audit=proof, witness=reference(path))
        write(output/f'{name}.json',row)
        rows.append(dict(name=name, actions=len(trace), initial_score=observation['initial_metrics']['score'],
            best_score=result['best_metrics']['score'], audit=proof, source=reference(output/f'{name}.json')))
        return row

    session = open_case()
    observation = runtime.observe(session)
    trace = [advance(session,place(observation,'C1',27,13,90),('improved',))]
    save('automatic-known-improvement',session,trace)

    session = open_case()
    trace = [advance(session,place(runtime.observe(session),'A0',2,7,90),('draft_accepted',))]
    try:
        session.validated_draft_json()
    except ValueError:
        pass
    else:
        check(False,'probe-export','Unvalidated draft was exported')
    saved = json.loads(session.checkpoint_json())
    write(output/'unfinished-checkpoint.json',saved)
    restored = runtime.restore(saved)
    before, after = runtime.observe(session), runtime.observe(restored)
    for key in ('blueprint_sha256','revision','attempts','facilities','port_assignments','nodes','components','edges','best_metrics','work','config'):
        check(before[key]==after[key],'probe-restore',f'Restoration changed {key}')
    check(after['elapsed_ms']>=saved['elapsed_ns']/1000000,'probe-restore','Restoration reset elapsed allowance')
    for instance,x,y in [('A1',2,11),('A0',3,2)]:
        trace.append(advance(restored,place(runtime.observe(restored),instance,x,y,90),('draft_accepted','validated','improved')))
    check(runtime.observe(restored)['draft_factory_validated'],'probe-recovery','Cleared draft did not recover')
    save('overlap-restore-recovery',restored,trace,draft=True)
    stopped=save('unfinished-stop',session,trace[:1])
    check(stopped['result']['unfinished_draft'],'probe-stop','Unfinished draft was misclassified')

    session = open_case()
    trace=[]
    for axis in ('x','y'):
        trace.append(advance(session,dict(kind='insert_band',axis=axis,at=0,count=3),('validated',)))
        trace.append(advance(session,dict(kind='delete_band',axis=axis,at=0,count=3),('validated',)))
    trace.append(advance(session,dict(kind='delete_band',axis='x',at=24,count=1),('improved',)))
    saved=json.loads(session.checkpoint_json())
    session=runtime.restore(saved)
    save('grid-component-translation-and-compression',session,trace,draft=True)

    session=open_case()
    trace=[]
    for action in [dict(kind='delete_band',axis='x',at=3,count=1),dict(kind='insert_band',axis='x',at=3,count=1),
                   dict(kind='delete_band',axis='x',at=10,count=1),dict(kind='delete_band',axis='x',at=0,count=1),
                   dict(kind='repair'),dict(kind='repair_all'),dict(kind='commit'),dict(kind='restore_best')]:
        before=runtime.observe(session)
        trace.append(advance(session,action,('rejected',)))
        after=runtime.observe(session)
        for key in ('revision','facilities','port_assignments','nodes','edges','best_metrics'):
            check(before[key]==after[key],'probe-atomic',f'Rejected action changed {key}')
    action=place(runtime.observe(session),'C1',27,13,90)
    action['ports'][1]['port']=action['ports'][0]['port']
    trace.append(advance(session,action,('rejected',)))
    envelope=dict(blueprint_sha256=before['blueprint_sha256'],revision=before['revision']+1,action=action)
    stale=json.loads(session.step_json(json.dumps(envelope)))
    check(stale['diagnostics'][0]['code']=='declarative.stale-envelope','probe-stale','Stale revision was accepted')
    save('reject-removal-rewiring-and-port-reuse',session,trace)

    # The same retained port-turn fixture that exposed the old boundary-normal bug.
    port_turn=read(ROOT/'crates/aic-pilot-native/tests/fixtures/blueprint-port-turn.json')
    turn_request=request_from_initial(port_turn)
    turn_session=runtime.open(turn_request,port_turn)
    turn_before=runtime.observe(turn_session)
    def physical_networks(witness):
        networks=deepcopy(witness['transport_networks'])
        for network in networks:
            network['cells'].sort(key=lambda p:(p['x'],p['y']))
            network['segments'].sort(key=lambda s:json.dumps(s,sort_keys=True))
        return networks
    check(physical_networks(json.loads(turn_session.validated_draft_json()))==physical_networks(port_turn),
          'probe-port-turn','Admission changed original immediate port turns')
    turn_result=json.loads(turn_session.finish_json())
    audit(port_turn,turn_before['blueprint'],turn_result['best'])
    rows.append(dict(name='retained-immediate-port-turn',source=reference(ROOT/'crates/aic-pilot-native/tests/fixtures/blueprint-port-turn.json')))

    # A separate bounded contract diagnostic; this selected example never enters
    # the fresh panel and its hit rate is not a random-policy result.
    search=[]
    for index in range(128):
        check(time.monotonic()-began<120,'probe-budget','Contract gate exceeded 120 seconds')
        session=open_case()
        observation=runtime.observe(session)
        action=draw(observation,observation,'automatic_ports',601900000,index,spec['comparison'])
        original_pose=next(p for p in observation['facilities'] if p['instance']==action['instance'])
        changed_port=any(p['port']!=observation['port_assignments'][p['node']] for p in action['ports'])
        if action['rotation']==original_pose['rotation'] or not changed_port:
            continue
        outcome=runtime.step(session,observation,action)
        search.append(dict(index=index,action=action,outcome=outcome))
        if outcome['status'] not in ('validated','improved'):
            continue
        after=runtime.observe(session)
        witness=json.loads(session.validated_draft_json())
        proof=audit(original,observation['blueprint'],witness)
        check(proof['changed_ports']>0,'probe-port','Selected example did not change a physical port')
        saved=json.loads(session.checkpoint_json())
        restored=runtime.restore(saved)
        check(json.loads(restored.validated_draft_json())==witness,'probe-port-restore','Ported draft changed during restoration')
        save('joint-pose-rotation-port',restored,[dict(action=action,outcome=outcome)],draft=True)
        break
    else:
        write(output/'joint-search.json',search)
        check(False,'probe-joint-domain','No complete joint rotation/port example within registered 128 proposals')
    write(output/'joint-search.json',search)
    result=dict(schema='declarative-action-probe-v1',status='passed',seconds=time.monotonic()-began,
        scenarios=rows,joint_search_attempts=len(search),runtime_inputs=runtime.inputs,
        native_build=runtime.build,trained=False)
    write(output/'report.json',result)
    return result
