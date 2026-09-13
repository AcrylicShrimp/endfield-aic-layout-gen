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
    sampler = torch.Generator().manual_seed(command['action_seed'])
    session, admission = runtime.open(config['max_actions'],config['elapsed_ms'])
    observation = json.loads(session.observation_json())
    initial = observation
    require(initial['initial_metrics']['score']==case['initial_metrics']['score'], 'declarative evaluation original score')
    trace, counts, milestones = [], Counter(), []
    decisions = 0
    began = time.perf_counter()
    while not observation['done']:
        require(time.monotonic()<command['deadline'], 'declarative evaluation command deadline')
        prepared = runtime.prepare(observation)
        with torch.no_grad():
            selected = choose(model,[prepared],sampler,uniform=command.get('uniform',False))[0]
        outcome = runtime.step(session,observation,selected['action'])
        after = json.loads(session.observation_json())
        if outcome['charged']:
            decisions += 1
            counts[f'kind/{selected["action"]["kind"]}'] += 1
            counts[f'status/{outcome["status"]}'] += 1
            counts['actions_with_pending'] += any(e['status']=='pending' for e in after['edges'])
            counts['actions_with_overlap'] += bool(after['conflicts'])
        entry = dict(policy_action=decisions,**selected,outcome=outcome,observation=observation,
            best_score=after['best_metrics']['score'],elapsed_ms=after['elapsed_ms'],work=after['work'],
            pending=sum(e['status']=='pending' for e in after['edges']),conflicts=len(after['conflicts']))
        trace.append(entry)
        if decisions in config['action_milestones'] and outcome['charged']:
            milestones.append({k:entry[k] for k in ('policy_action','elapsed_ms','best_score','work')})
        observation = after
    checkpoint = json.loads(session.checkpoint_json())
    result = runtime.finish(session)
    output = command['output']
    from pathlib import Path
    name = f'factory-{case["generation_seed"]}-{command["sample"]}'
    save(Path(output)/f'{name}.pt.gz',dict(initial=initial,trace=trace,result=result,native=checkpoint))
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
