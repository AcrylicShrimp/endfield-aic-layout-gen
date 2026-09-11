"""Audit grouped completion cases, candidate probabilities and native witnesses."""
import argparse
from collections import Counter
from copy import deepcopy
import json
from pathlib import Path
import tempfile

import torch

from audit_blueprint_ppo import WitnessAudit
from blueprint_edit_input import prepare
from blueprint_ppo import check_sources
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_environment import Runtime, semantic
from blueprint_ppo_store import ROOT, checked, load, read, restore_learners, sha, write
from blueprint_ppo_update import digest, predictions, require
from evaluate_blueprint_bc_completion import likelihood


def audit_completion(run):
    protocol, state = read(run / 'protocol.json'), read(run / 'state.json')
    report = read(run / 'completion/report.json')
    require(report['status'] == 'complete' and report['protocol_sha256'] == state['protocol_sha256'] == sha(run / 'protocol.json'), 'blueprint.bc-audit: completion identity')
    check_sources(protocol)
    for relative, expected in report['sources'].items():
        require(sha(ROOT / relative) == expected, f'blueprint.bc-audit: completion source {relative}')
    config = protocol['config']
    dataset = load(ROOT / config['bc']['dataset'])
    demonstrations = {d['id']: d for d in dataset['demonstrations']}
    runtime = Runtime(config, protocol['native_library'])
    counts, models = Counter(), {}
    actual_likelihood = []
    for seed in config['seeds']:
        for ordinal in config['evaluation']['at']:
            saved = load(run / f'seed-{seed}/checkpoints/{ordinal:04d}.pt.gz')
            model, _, _, _, _ = restore_learners(saved, config['ppo'], state['protocol_sha256'])
            models[seed, ordinal] = model
            actual_likelihood.extend(dict(seed=seed, update=ordinal, **p) for p in likelihood(model, dataset['demonstrations']))
    require(actual_likelihood == report['probabilities'], 'blueprint.bc-audit: all teacher likelihood values reproduce')
    keys, rows = set(), []
    with tempfile.TemporaryDirectory(prefix='blueprint-bc-completion-audit-') as temporary:
        checker = WitnessAudit(runtime, temporary)
        for reference in report['cases']:
            case = load(checked(reference, run))
            demo = demonstrations[case['demo_id']]
            require(demo['split'] == 'holdout', 'blueprint.bc-audit: held-out completion case')
            key = (case['seed'], case['update'], demo['id'], case['position'], case['sample'])
            require(key not in keys, 'blueprint.bc-audit: unique completion case')
            keys.add(key)
            model = models[case['seed'], case['update']]
            require(case['model_sha256'] == digest(model.state_dict()) and case['protocol_sha256'] == state['protocol_sha256'], 'blueprint.bc-audit: completion model/protocol')
            saved = deepcopy(demo['checkpoint'])
            saved.update(elapsed_ns=0, closed=False)
            saved['config'].update(max_actions=saved['attempts'] + config['bc']['completion_actions'] + (case['position'] == 'post'), elapsed_ms=config['bc']['completion_elapsed_ms'])
            require(saved == case['initial_checkpoint'], 'blueprint.bc-audit: diagnostic-only budget reset')
            session = runtime.restore(saved)
            require(len(case['prefix']) == (case['position'] == 'post'), 'blueprint.bc-audit: exact setup prefix')
            for entry in case['prefix']:
                require(entry['action'] == demo['after']['rows'][0]['action'], 'blueprint.bc-audit: source pose prefix')
                observation = json.loads(session.observation_json())
                outcome = runtime.step(session, observation, entry['action'])
                require(outcome['status'] == entry['outcome']['status'] == 'draft_accepted', 'blueprint.bc-audit: setup pose succeeds')
            actual = json.loads(session.observation_json())
            require(semantic(actual) == semantic(case['initial']), 'blueprint.bc-audit: actual native held-out restoration')
            actions = Counter()
            same_pose, committed = case['position'] == 'post', False
            samples = []
            sampler = torch.Generator().manual_seed(4_000_000 + case['seed'] * 100_000 + int(demo['id'][:8], 16) * 10 + case['sample'] * 2 + (case['position'] == 'post'))
            for i, entry in enumerate(case['trace']):
                prepared = prepare(entry['observation'])
                action, outcome = entry['action'], entry['outcome']
                require(prepared['actions'][entry['index']] == action, 'blueprint.bc-audit: completion candidate')
                if i == 0:
                    require(semantic(entry['observation']) == semantic(case['initial']), 'blueprint.bc-audit: initial policy observation')
                if i + 1 < len(case['trace']):
                    require(entry['after_semantic'] == semantic(case['trace'][i + 1]['observation']), 'blueprint.bc-audit: completion continuity')
                with torch.no_grad():
                    batch = pack([prepared])
                    logits, _ = model.forward_packed(batch)
                    logp, _ = distribution(logits, batch)
                    sampled = int(torch.multinomial(logp.exp(), 1, generator=sampler))
                require(sampled == entry['index'], 'blueprint.bc-audit: no teacher intervention in sampling')
                samples.append(dict(input=prepared, index=entry['index'], old_logp=entry['old_logp'], old_value=entry['old_value']))
                if outcome['charged']:
                    actions[action['kind']] += 1
                if action['kind'] in ('pose', 'restore_best') and outcome['status'] != 'rejected':
                    same_pose = False
                if action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved'):
                    require(entry['validated_witness'] is not None, 'blueprint.bc-audit: successful completion commit witness')
                    checker.check(case['initial']['blueprint'], entry['validated_witness'])
                    committed |= same_pose
                else:
                    require(entry['validated_witness'] is None, 'blueprint.bc-audit: only committed witnesses')
            with torch.no_grad():
                logp, _, values = predictions(model, samples)
                torch.testing.assert_close(logp, torch.tensor([s['old_logp'] for s in samples]), rtol=1e-5, atol=2e-6)
                torch.testing.assert_close(values, torch.tensor([s['old_value'] for s in samples]), rtol=1e-5, atol=2e-6)
            require(sum(actions.values()) <= config['bc']['completion_actions'], 'blueprint.bc-audit: action allowance')
            result, row = case['result'], case['row']
            checker.check(case['initial']['blueprint'], result['best'])
            require(result['factory_validated'] and row['best_score'] == result['best_metrics']['score'], 'blueprint.bc-audit: completion final score')
            require(row['counts'] == dict(actions) and row['policy_actions'] == sum(actions.values()), 'blueprint.bc-audit: charged action counts')
            require(row['same_pose_commit'] == committed and row['goal_reached'] == (row['best_score'][0] <= demo['after']['score'][0]), 'blueprint.bc-audit: completion classification')
            rows.append(dict(update=case['update'], **row))
            counts['cases'] += 1
            counts['policy_actions'] += sum(actions.values())
        require(rows == report['rows'], 'blueprint.bc-audit: completion row references')
        expected = {(seed, ordinal, d['id'], position, sample) for seed in config['seeds']
                    for ordinal in (0, config['training']['updates']) for d in dataset['demonstrations'] if d['split'] == 'holdout'
                    for position in ('pre', 'post') for sample in range(config['bc']['completion_samples'])}
        require(keys == expected, 'blueprint.bc-audit: exact registered completion coverage')
        counts.update(witness_references=checker.references, unique_witnesses=len(checker.unique))
    audit = dict(schema='blueprint-bc-completion-audit-v1', status='passed', report_sha256=sha(run / 'completion/report.json'),
                 counts=dict(counts), audit_source_sha256=sha(Path(__file__)))
    write(run / 'completion/audit.json', audit)
    print(json.dumps(audit), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    audit_completion(args.run.resolve())


if __name__ == '__main__':
    main()
