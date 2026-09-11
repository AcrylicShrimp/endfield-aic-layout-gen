"""Measure teacher likelihood and autonomous completion on grouped held-out drafts."""
import argparse
from collections import Counter
from copy import deepcopy
import json
from pathlib import Path
import tempfile
import time

import torch

from audit_blueprint_ppo import WitnessAudit
from blueprint_edit_input import prepare
from blueprint_ppo import check_sources
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_environment import Runtime, semantic
from blueprint_ppo_store import ROOT, checked, load, read, reference, restore_learners, save, sha, write
from blueprint_ppo_update import digest, predictions, require


def likelihood(model, demonstrations):
    result = []
    with torch.no_grad():
        for demo in demonstrations:
            logp, _, _ = predictions(model, demo['after']['rows'])
            for row, value in zip(demo['after']['rows'], logp, strict=True):
                result.append(dict(id=demo['id'], split=demo['split'], kind=row['action']['kind'],
                                   log_probability=float(value), probability=float(value.exp())))
    return result


def complete(runtime, model, seed, demo, position, sample, config, checker):
    saved = deepcopy(demo['checkpoint'])
    saved.update(elapsed_ns=0, closed=False)
    saved['config'].update(max_actions=saved['attempts'] + config['completion_actions'] + (position == 'post'),
                           elapsed_ms=config['completion_elapsed_ms'])
    session = runtime.restore(saved)
    prefix = []
    if position == 'post':
        observation = json.loads(session.observation_json())
        action = demo['after']['rows'][0]['action']
        outcome = runtime.step(session, observation, action)
        require(outcome['status'] == 'draft_accepted', 'blueprint.bc-eval: held-out pose prefix')
        prefix.append(dict(action=action, outcome=outcome))
    initial = json.loads(session.observation_json())
    observation = initial
    sampler = torch.Generator().manual_seed(4_000_000 + seed * 100_000 + int(demo['id'][:8], 16) * 10 + sample * 2 + (position == 'post'))
    trace, counts, actions = [], Counter(), 0
    same_pose = position == 'post'
    same_pose_commit = False
    while not observation['done']:
        prepared = prepare(observation)
        with torch.no_grad():
            batch = pack([prepared])
            logits, values = model.forward_packed(batch)
            logp, _ = distribution(logits, batch)
            index = int(torch.multinomial(logp.exp(), 1, generator=sampler))
        action = prepared['actions'][index]
        outcome = runtime.step(session, observation, action)
        after = json.loads(session.observation_json())
        witness = None
        if outcome['charged']:
            actions += 1
            counts[action['kind']] += 1
        if action['kind'] in ('pose', 'restore_best') and outcome['status'] != 'rejected':
            same_pose = False
        if action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved'):
            witness = runtime.commit_witness(session, after)
            checker.check(after['blueprint'], witness)
            same_pose_commit = same_pose_commit or same_pose
        trace.append(dict(observation=observation, index=index, action=action, outcome=outcome,
                          old_logp=float(logp[index]), old_value=float(values[0]),
                          after_semantic=semantic(after), validated_witness=witness))
        observation = after
    result = runtime.finish(session, observation)
    checker.check(initial['blueprint'], result['best'])
    require(result['factory_validated'], 'blueprint.bc-eval: final best valid')
    return dict(initial_checkpoint=saved, initial=initial, prefix=prefix, trace=trace, result=result,
                row=dict(seed=seed, id=demo['id'], position=position, sample=sample,
                    start_score=initial['best_metrics']['score'], best_score=result['best_metrics']['score'],
                    goal_area=demo['after']['score'][0], goal_reached=result['best_metrics']['score'][0] <= demo['after']['score'][0],
                    same_pose_commit=same_pose_commit, policy_actions=actions, counts=dict(counts), elapsed_ms=result['elapsed_ms']))


def evaluate(run):
    protocol, state, audit = [read(run / f'{name}.json') for name in ('protocol', 'state', 'audit')]
    require(audit['status'] == 'passed' and audit['protocol_sha256'] == state['protocol_sha256'] == sha(run / 'protocol.json'), 'blueprint.bc-eval: audited source run')
    check_sources(protocol)
    config = protocol['config']
    dataset = load(ROOT / config['bc']['dataset'])
    runtime = Runtime(config, protocol['native_library'])
    output = run / 'completion'
    require(not output.exists(), f'blueprint.bc-eval: fresh completion directory: {output}')
    output.mkdir()
    began = time.perf_counter()
    rows, probabilities, references = [], [], []
    with tempfile.TemporaryDirectory(prefix='blueprint-bc-completion-') as temporary:
        checker = WitnessAudit(runtime, temporary)
        for seed in config['seeds']:
            job = state['jobs'][str(seed)]
            for ordinal in config['evaluation']['at']:
                checkpoint_path = run / f'seed-{seed}/checkpoints/{ordinal:04d}.pt.gz'
                if ordinal:
                    record = read(checked(job['records'][ordinal - 1], run))
                    checked(record['checkpoint'], run)
                saved = load(checkpoint_path)
                model, _, _, _, _ = restore_learners(saved, config['ppo'], state['protocol_sha256'])
                before = digest(model.state_dict())
                probabilities.extend(dict(seed=seed, update=ordinal, **p) for p in likelihood(model, dataset['demonstrations']))
                if ordinal in (0, config['training']['updates']):
                    for demo in dataset['demonstrations']:
                        if demo['split'] != 'holdout':
                            continue
                        for position in ('pre', 'post'):
                            for sample in range(config['bc']['completion_samples']):
                                require(time.perf_counter() - began < 300, 'blueprint.bc-eval: completion elapsed guard')
                                result = complete(runtime, model, seed, demo, position, sample, config['bc'], checker)
                                result.update(seed=seed, update=ordinal, demo_id=demo['id'], model_sha256=before,
                                              position=position, sample=sample, protocol_sha256=state['protocol_sha256'])
                                path = output / f's{seed}-u{ordinal:04d}-{demo["id"][:12]}-{position}-{sample}.pt.gz'
                                save(path, result)
                                references.append(reference(path, run))
                                rows.append(dict(update=ordinal, **result['row']))
                require(digest(model.state_dict()) == before, 'blueprint.bc-eval: evaluation leaves model unchanged')
                print(json.dumps(dict(seed=seed, update=ordinal, completion_cases=len(rows))), flush=True)
        report = dict(schema='blueprint-bc-completion-v1', status='complete', protocol_sha256=state['protocol_sha256'],
            sources={str(Path(__file__).relative_to(ROOT)): sha(Path(__file__))}, rows=rows, probabilities=probabilities,
            cases=references, witness_references=checker.references, unique_witnesses=len(checker.unique),
            seconds=time.perf_counter() - began)
        write(output / 'report.json', report)
    check_sources(protocol)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    evaluate(args.run.resolve())


if __name__ == '__main__':
    main()
