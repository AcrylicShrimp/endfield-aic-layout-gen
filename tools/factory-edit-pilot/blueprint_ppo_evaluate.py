"""Continuing inference with a real native elapsed-time budget."""
from collections import Counter
import json
import time

import torch

from blueprint_edit_input import prepare
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_store import save
from blueprint_ppo_update import digest, require


def evaluate(runtime, model, seed, update, output, guard):
    config = runtime.config['evaluation']
    model_hash = digest(model.state_dict())
    rows = []
    for start_name in config['starts']:
        start = next(s for s in runtime.config['starts'] if s['name'] == start_name)
        for sample in range(config['samples']):
            guard()
            # Each checkpoint uses the same sampling stream for this start.
            start_index = config['starts'].index(start_name)
            sampler = torch.Generator().manual_seed(seed * 10000 + start_index * 100 + sample)
            session, prefix, admission = runtime.open(start, config['max_actions'], config['elapsed_ms'])
            observation = json.loads(session.observation_json())
            initial = observation
            trace, counts, milestones = [], Counter(), []
            policy_actions = 0
            began = time.perf_counter()
            while not observation['done']:
                guard()
                prepared = prepare(observation)
                batch = pack([prepared])
                with torch.no_grad():
                    logits, values = model.forward_packed(batch)
                    logp, _ = distribution(logits, batch)
                    index = int(torch.multinomial(logp.exp(), 1, generator=sampler))
                action = prepared['actions'][index]
                outcome = runtime.step(session, observation, action)
                after = json.loads(session.observation_json())
                if outcome['charged']:
                    policy_actions += 1
                    counts[f'kind/{action["kind"]}'] += 1
                    counts[f'status/{outcome["status"]}'] += 1
                    counts['actions_with_pending'] += any(e['status'] == 'pending' for e in after['edges'])
                    counts['actions_with_overlap'] += bool(after['conflicts'])
                    if outcome['status'] == 'improved':
                        for boundary in (8, 32, 64):
                            counts[f'improvements_after_{boundary}'] += policy_actions > boundary
                witness = None
                if action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved'):
                    witness = runtime.commit_witness(session, after)
                entry = dict(policy_action=policy_actions, action=action, outcome=outcome,
                             observation=observation, index=index, old_logp=float(logp[index]), old_value=float(values[0]),
                             elapsed_ms=after['elapsed_ms'], best_score=after['best_metrics']['score'],
                             pending=sum(e['status'] == 'pending' for e in after['edges']),
                             conflicts=len(after['conflicts']), validated_witness=witness)
                trace.append(entry)
                if policy_actions in config['action_milestones'] and outcome['charged']:
                    milestones.append({k: entry[k] for k in ('policy_action', 'elapsed_ms', 'best_score')})
                observation = after
            inference_seconds = time.perf_counter() - began
            result = runtime.finish(session, observation)
            require(result['factory_validated'], 'timed inference retains a validated best')
            name = f'{start_name}-{sample}'
            save(output / f'{name}.pt.gz', dict(initial=initial, prefix=prefix, trace=trace, result=result))
            start_area = initial['best_metrics']['score'][0]
            final_area = result['best_metrics']['score'][0]
            time_curve = []
            for fraction in (.125, .25, .5, 1.):
                limit = config['elapsed_ms'] * fraction
                eligible = [t for t in trace if t['elapsed_ms'] <= limit and t['outcome']['status'] != 'rejected']
                score = result['best_metrics']['score'] if fraction == 1. else eligible[-1]['best_score'] if eligible else initial['best_metrics']['score']
                time_curve.append(dict(elapsed_ms=limit, score=score))
            rows.append(dict(seed=seed, update=update, start=start_name, sample=sample, model_sha256=model_hash,
                policy_actions=policy_actions, admission_seconds=admission, inference_seconds=inference_seconds,
                elapsed_ms=result['elapsed_ms'], start_score=initial['best_metrics']['score'], best_score=result['best_metrics']['score'],
                policy_area_reduction_percent=100 * (start_area - final_area) / start_area,
                total_area_reduction_percent=100 * (initial['initial_metrics']['score'][0] - final_area) / initial['initial_metrics']['score'][0],
                action_milestones=milestones, time_curve=time_curve, counts=dict(counts), unfinished_draft=result['unfinished_draft'],
                guard='actions' if result['attempts'] >= config['max_actions'] + len(prefix) else 'elapsed'))
    require(digest(model.state_dict()) == model_hash, 'evaluation changed parameters')
    return rows
