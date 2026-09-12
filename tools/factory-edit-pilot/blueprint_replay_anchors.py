"""Fixed training-label measurements and original-horizon autonomous branches."""
from collections import Counter
from copy import copy
import json
from pathlib import Path
import time

import torch

from blueprint_edit_input import prepare
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_store import save
from blueprint_replay_contract import check


def label_metrics(model, imports):
    rows = []
    with torch.no_grad():
        for case_index, sequence in enumerate(imports):
            for row in sequence['rows']:
                prepared = row['input']
                logits, values = model.forward_packed(pack([prepared]))
                logp, _ = distribution(logits, pack([prepared]))
                selected = row['index']
                kind = prepared['actions'][selected]['kind']
                eligible = torch.tensor([a['kind'] == kind for a in prepared['actions']])
                kind_mass = float(logp[eligible].exp().sum())
                probability = float(logp[selected].exp())
                value = float(values[0])
                rows.append(dict(case_index=case_index, sequence_id=sequence['id'], stage=row['stage'],
                    namespace=sequence['namespace'], reference=row['reference'], log_probability=float(logp[selected]),
                    probability=probability, rank=1 + int((logp > logp[selected]).sum()), action_kind=kind,
                    action_kind_mass=kind_mass, conditional_probability=probability / kind_mass,
                    value=value, return_to_terminal=row['return_to_terminal'],
                    positive_imitation_weight=max(0., row['return_to_terminal'] - value)))
    check(len(rows) == 15, 'source-invalid', 'anchor-labels', 'expected fifteen fixed labels')
    return rows


def continuation(runtime, model, command):
    native = command['state']['native']
    target = command['state']['demonstrated_score'][0]
    selected = copy(runtime)
    selected.original, selected.request = native['original'], native['request']
    began = time.monotonic()
    session = selected.restore(native)
    observation = json.loads(session.observation_json())
    initial = observation
    check(native['config']['max_actions'] == 64 and not native['closed']
        and observation['best_metrics']['score'][0] > target,
        'continuation-state-invalid', 'anchor-continuation', 'invalid original-horizon diagnostic start')
    check(observation['attempts'] == command['state']['observation']['attempts']
        and observation['revision'] == command['state']['observation']['revision'],
        'continuation-state-invalid', 'anchor-continuation', 'restored counters differ')
    sampler = torch.Generator().manual_seed(command['action_seed'])
    trace, counts = [], Counter()
    while not observation['done'] and time.monotonic() - began < command['branch_seconds']:
        check(time.monotonic() < command['deadline'], 'resource-exhausted', 'anchor-continuation', 'probe wall guard')
        prepared = prepare(observation)
        batch = pack([prepared])
        with torch.no_grad():
            logits, _ = model.forward_packed(batch)
            logp, _ = distribution(logits, batch)
            index = int(torch.multinomial(logp.exp(), 1, generator=sampler))
        action = prepared['actions'][index]
        outcome = selected.step(session, observation, action)
        after = json.loads(session.observation_json())
        if outcome['charged']:
            counts[action['kind']] += 1
            counts[outcome['status']] += 1
        if action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved'):
            selected.commit_witness(session, after)
            counts['valid_commit'] += 1
        trace.append(dict(action=action, outcome=outcome, attempts=after['attempts'], elapsed_ms=after['elapsed_ms'],
            best_score=after['best_metrics']['score'], pending=sum(e['status'] == 'pending' for e in after['edges']),
            conflicts=len(after['conflicts'])))
        observation = after
    result = selected.finish(session, observation)
    check(result['factory_validated'], 'source-invalid', 'anchor-continuation', 'branch did not retain a valid best')
    row = dict(case_index=command['case_index'], stage=command['state']['stage'], replicate=command['replicate'],
        action_seed=command['action_seed'], counts=dict(counts), start_area=initial['best_metrics']['score'][0],
        best_area=result['best_metrics']['score'][0], demonstrated_area=target,
        improved=result['best_metrics']['score'][0] < initial['best_metrics']['score'][0],
        demonstrated_target_attained=result['best_metrics']['score'][0] <= target,
        valid_commit=counts['valid_commit'] > 0, abandoned=counts['restore_best'] > 0,
        pending=sum(e['status'] == 'pending' for e in observation['edges']), conflicts=len(observation['conflicts']),
        start_attempts=initial['attempts'], end_attempts=observation['attempts'],
        recorded_elapsed_ms=command['state']['recorded_elapsed_ms'], restored_elapsed_ms=initial['elapsed_ms'],
        max_actions=native['config']['max_actions'], native_elapsed_allowance_ms=native['config']['elapsed_ms'],
        guard='native' if observation['done'] else 'branch-wall', seconds=time.monotonic() - began)
    path = Path(command['output']) / f'case-{command["case_index"]}-{row["stage"]}-{command["replicate"]}.pt.gz'
    save(path, dict(row=row, trace=trace, result=result))
    return row
