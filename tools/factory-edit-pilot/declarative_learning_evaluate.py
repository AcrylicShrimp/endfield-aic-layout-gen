"""Held-out continuing evaluation with retained valid best and charged work curves."""
from collections import Counter
import json
import time

import torch

from blueprint_ppo_store import save
from blueprint_ppo_update import require
from declarative_learning_policy import sample as choose


def evaluate(runtime, model, case, command):
    config = command['evaluation']
    retain_trace=config.get('retain_trace',True)
    sampler = torch.Generator().manual_seed(command['action_seed'])
    session, admission = runtime.open(config['max_actions'],config['elapsed_ms'])
    initial = json.loads(session.observation_json())
    require(initial['initial_metrics']['score']==case['initial_metrics']['score'], 'declarative evaluation original score')
    trace, counts, milestones = [], Counter(), []
    decisions = 0
    began = time.perf_counter()
    if retain_trace:session.begin_learning_chunk(config['max_actions'],True)
    observation,prepared=runtime.observe(session)
    while not observation['done']:
        require(time.monotonic()<command['deadline'], 'declarative evaluation command deadline')
        with torch.no_grad():
            selected = choose(model,[prepared],sampler,uniform=command.get('uniform',False))[0]
        result=runtime.step_index(session,prepared,selected['decision'])
        outcome=result['outcome'];selected['action']=result['action']
        after,next_input=runtime.observe(session)
        if outcome['charged']:
            decisions += 1
            counts[f'kind/{selected["action"]["kind"]}'] += 1
            counts[f'status/{outcome["status"]}'] += 1
            counts['actions_with_pending'] += after['pending']>0
            counts['actions_with_overlap'] += after['conflicts']>0
        entry = dict(policy_action=decisions,**selected,outcome=outcome,
            best_score=outcome['best_metrics']['score'],elapsed_ms=after['elapsed_ms'],work=result['work'],
            pending=after['pending'],conflicts=after['conflicts'])
        if retain_trace:trace.append(entry)
        if decisions in config['action_milestones'] and outcome['charged']:
            milestones.append({k:entry[k] for k in ('policy_action','elapsed_ms','best_score','work')})
        observation,prepared = after,next_input
    if retain_trace:
        observations=session.take_learning_chunk()
        require(len(observations)==len(trace),'declarative.native-evaluation: complete trace coverage')
        for row,native in zip(trace,observations,strict=True):row['observation']=native['observation']
    checkpoint = json.loads(session.checkpoint_json()) if retain_trace else None
    result = runtime.finish(session)
    output = command['output']
    from pathlib import Path
    name = f'factory-{case["generation_seed"]}-{command["sample"]}'
    if retain_trace:save(Path(output)/f'{name}.pt.gz',dict(initial=initial,trace=trace,result=result,native=checkpoint))
    area = initial['initial_metrics']['score'][0]
    score = result['best_metrics']['score']
    return dict(seed=command['seed'],update=command['update'],start=f'factory-{case["generation_seed"]}',
        sample=command['sample'],generation_seed=case['generation_seed'],action_seed=command['action_seed'],
        request_sha256=case['files']['request']['sha256'],initial_sha256=case['files']['initial']['sha256'],
        geometry_identity=case['geometry_identity'],product=case['product'],facilities=case['facilities'],
        policy_actions=decisions,admission_seconds=admission,inference_seconds=time.perf_counter()-began,
        elapsed_ms=result['elapsed_ms'],start_score=initial['initial_metrics']['score'],best_score=score,
        policy_area_reduction_percent=100*(area-score[0])/area,total_area_reduction_percent=100*(area-score[0])/area,
        counts=dict(counts),action_milestones=milestones,work=result['work'],unfinished_draft=result['unfinished_draft'],
        factory_validated=result['factory_validated'],uniform=command.get('uniform',False),
        guard='actions' if result['attempts']>=config['max_actions'] else 'resource')
