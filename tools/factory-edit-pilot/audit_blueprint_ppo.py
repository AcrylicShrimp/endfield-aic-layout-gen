"""Audit retained blueprint PPO experience, all updates and physical witnesses."""
import argparse
from collections import Counter
import json
from pathlib import Path
import tempfile

import torch

from blueprint_edit_audit import audit
from blueprint_edit_input import prepare
from blueprint_ppo import check_sources
from blueprint_ppo_batch import pack
from blueprint_ppo_environment import Runtime, Stream, semantic
from blueprint_ppo_store import ROOT, checked, load, read, restore_learners, sha, write
from blueprint_ppo_update import digest, predictions, require, update


class WitnessAudit:
    def __init__(self, runtime, directory):
        self.runtime, self.directory = runtime, Path(directory)
        self.unique, self.references = set(), 0
        self.inputs = {}

    def register(self, runtime, blueprint):
        self.inputs[digest(blueprint)] = runtime

    def check(self, blueprint, witness):
        self.references += 1
        runtime = self.inputs.get(digest(blueprint), self.runtime)
        require(runtime.original is not None and runtime.request is not None,
                'blueprint.training-input.audit: no authenticated original for this blueprint')
        audit(runtime.original, blueprint, witness)
        identity = digest((runtime.request, blueprint, witness))
        if identity in self.unique:
            return
        write(self.directory / 'witness.json', witness)
        write(self.directory / 'request.json', runtime.request)
        verifier = self.runtime.module.FactoryEditSession.open(str(ROOT), str(self.directory / 'request.json'),
            str(self.directory / 'witness.json'), json.dumps(self.runtime.spec['catalogs']),
            json.dumps(dict(proposals=1, elapsed_ms=60000)))
        require(json.loads(verifier.snapshot_json())['factory_validated'], 'independent complete-factory admission')
        self.unique.add(identity)


def check_sample(sample, checker):
    observation, outcome = sample['observation'], sample['outcome']
    prepared = prepare(observation)
    require(digest(prepared) == digest(sample['input']), 'recorded model input matches native observation')
    action = prepared['actions'][sample['index']]
    require(outcome['blueprint_sha256'] == observation['blueprint_sha256'], 'sample blueprint identity')
    reward = (observation['best_metrics']['score'][0] - outcome['best_metrics']['score'][0]) / observation['initial_metrics']['score'][0]
    require(abs(sample['reward'] - reward) < 1e-12 and sample['reward'] == outcome['reward'], 'native best-area reward only')
    require(sample['terminal'] == outcome['done'], 'true native termination')
    if sample['validated_witness'] is not None:
        require(action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved'), 'successful commit witness')
        checker.check(observation['blueprint'], sample['validated_witness'])
    else:
        require(not (action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved')), 'successful commit witness retained')
    return action


def audit_run(output):
    protocol, state = read(output / 'protocol.json'), read(output / 'state.json')
    require(state['status'] == 'complete' and state['protocol_sha256'] == sha(output / 'protocol.json'), 'complete source-bound run')
    check_sources(protocol)
    config = protocol['config']
    runtime = Runtime(config, protocol['native_library'])
    counts, by_start, replayed, evaluations = Counter(), {}, [], []
    with tempfile.TemporaryDirectory(prefix='blueprint-ppo-audit-') as temporary:
        checker = WitnessAudit(runtime, temporary)
        for seed in config['seeds']:
            job = state['jobs'][str(seed)]
            require(job['status'] == 'complete' and job['update'] == config['training']['updates'], 'complete registered seed')
            directory = output / f'seed-{seed}'
            saved = load(directory / 'checkpoints/0000.pt.gz')
            model, actor, critic, _, shuffle = restore_learners(saved, config['ppo'], state['protocol_sha256'])
            prior_frontiers = {}
            for ordinal, record_ref in enumerate(job['records'], 1):
                record = read(checked(record_ref, output))
                chunk = load(checked(record['chunk'], output))
                require(record['update'] == chunk['update'] == ordinal and chunk['seed'] == seed, 'ordered chunk identity')
                require(chunk['protocol_sha256'] == state['protocol_sha256'], 'chunk protocol identity')
                require(chunk['model_before'] == digest(model.state_dict()), 'behavior checkpoint identity')
                require(len(chunk['samples']) == config['training']['streams'] * config['training']['chunk_actions'], 'registered chunk size')
                checker.inputs.clear()
                for before in chunk['native_before']:
                    restored = Stream.restore(runtime, before)
                    observation = json.loads(restored.session.observation_json())
                    checker.register(restored.runtime, observation['blueprint'])
                    first = next(s for s in chunk['samples'] if s['stream'] == before['index'])
                    require(semantic(observation) == semantic(first['observation']), 'actual native snapshot restoration')
                    if before['policy_actions']:
                        require(semantic(observation) == prior_frontiers[before['index']]['semantic'], 'live physical draft crosses optimizer boundary')
                        counts['continuity_checks'] += 1
                    counts['native_restorations'] += 1
                for sample in chunk['samples']:
                    action = check_sample(sample, checker)
                    counts['training_actions'] += 1
                    local = by_start.setdefault(sample['start'], Counter())
                    for counter in (counts, local):
                        counter[f'kind/{action["kind"]}'] += 1
                        counter[f'status/{sample["outcome"]["status"]}'] += 1
                        counter['area_reward'] += sample['reward']
                        counter['actions_with_pending'] += sample['after_pending'] > 0
                        counter['actions_with_overlap'] += sample['after_conflicts'] > 0
                        if sample['outcome']['status'] == 'improved':
                            for boundary in (8, 32, 64):
                                counter[f'improvements_after_{boundary}'] += sample['policy_action'] > boundary
                    local['actions'] += 1
                for frontier in chunk['frontiers']:
                    rows = [s for s in chunk['samples'] if s['stream'] == frontier['stream']]
                    for earlier, later in zip(rows, rows[1:]):
                        require(earlier['after_semantic'] == semantic(later['observation']), 'within-chunk physical continuity')
                        require(earlier['policy_action'] + 1 == later['policy_action'] and earlier['episode'] == later['episode'], 'within-episode action order')
                    require(rows[-1]['after_semantic'] == semantic(frontier['successor_observation']) == frontier['semantic'], 'successor physical identity')
                    if frontier['terminal']:
                        require(frontier['bootstrap'] == 0. and rows[-1]['policy_action'] == config['training']['episode_actions'], 'true terminal zero bootstrap')
                        counts['terminal_frontiers'] += 1
                    else:
                        require(digest(prepare(frontier['successor_observation'])) == digest(frontier['successor_input']), 'successor input identity')
                        with torch.no_grad():
                            _, value = model.forward_packed(pack([frontier['successor_input']]))
                        require(abs(float(value[0]) - frontier['bootstrap']) < 2e-6, 'frozen behavior successor bootstrap')
                        counts['nonterminal_bootstraps'] += 1
                prior_frontiers = {f['stream']: f for f in chunk['frontiers']}
                for episode in chunk['completed']:
                    blueprint = prior_frontiers[episode['stream']]['successor_observation']['blueprint']
                    checker.check(blueprint, episode['result']['best'])
                    require(episode['result']['factory_validated'], 'training retained best validated')
                    counts['completed_episodes'] += 1
                # Reconstruct every update, including nonempty Adam state, from
                # retained behavior observations. Future wall-clock sampling is
                # deliberately not reconstructed from a seed.
                actual = update(model, actor, critic, shuffle, chunk['samples'], chunk['frontiers'], config['ppo'])
                expected = {k: v for k, v in chunk['ppo'].items() if k != 'seconds'}
                require(actual == expected, f'exact learner/Adam/RNG/credit replay seed={seed} update={ordinal}')
                counts['exact_update_replays'] += 1
                checkpoint_path = directory / f'checkpoints/{ordinal:04d}.pt.gz'
                if checkpoint_path.exists():
                    checkpoint = load(checked(record['checkpoint'], output))
                    require(checkpoint['model_sha256'] == digest(model.state_dict()), 'retained checkpoint model')
                    require(digest(checkpoint['actor_optimizer']) == actual['actor_optimizer'] and
                            digest(checkpoint['critic_optimizer']) == actual['critic_optimizer'], 'retained checkpoint both Adam states')
                    require(digest(checkpoint['shuffle_rng']) == actual['shuffle_rng'], 'retained checkpoint shuffle RNG')
                    counts['retained_checkpoints'] += 1
                if ordinal % 16 == 0:
                    print(json.dumps(dict(audited_seed=seed, update=ordinal)), flush=True)
            replayed.append(dict(seed=seed, updates=job['update'], final_model_sha256=digest(model.state_dict())))
            for eval_ref in job['evaluations']:
                report = read(checked(eval_ref, output))
                checkpoint = load(directory / f'checkpoints/{eval_ref["update"]:04d}.pt.gz')
                eval_model, _, _, _, _ = restore_learners(checkpoint, config['ppo'], state['protocol_sha256'])
                for case_ref in report['cases']:
                    case = load(checked(case_ref, output))
                    checker.check(case['initial']['blueprint'], case['result']['best'])
                    require(case['result']['factory_validated'], 'timed evaluation final best validated')
                    rows = []
                    for entry in case['trace']:
                        prepared = prepare(entry['observation'])
                        require(prepared['actions'][entry['index']] == entry['action'], 'evaluation candidate identity')
                        rows.append(dict(input=prepared, index=entry['index'], old_logp=entry['old_logp'], old_value=entry['old_value']))
                        if entry['validated_witness'] is not None:
                            checker.check(case['initial']['blueprint'], entry['validated_witness'])
                    with torch.no_grad():
                        for start in range(0, len(rows), 64):
                            batch = rows[start:start + 64]
                            logp, _, values = predictions(eval_model, batch)
                            torch.testing.assert_close(logp, torch.tensor([s['old_logp'] for s in batch]), rtol=1e-5, atol=2e-6)
                            torch.testing.assert_close(values, torch.tensor([s['old_value'] for s in batch]), rtol=1e-5, atol=2e-6)
                    counts['evaluation_cases'] += 1
                    counts['evaluation_actions'] += sum(e['outcome']['charged'] for e in case['trace'])
                evaluations.extend(report['rows'])
        counts.update(witness_references=checker.references, unique_physically_validated_witnesses=len(checker.unique))
    require(counts['exact_update_replays'] == len(config['seeds']) * config['training']['updates'], 'all registered updates audited')
    result = dict(schema='blueprint-ppo-audit-v1', status='passed', protocol_sha256=state['protocol_sha256'],
        counts=dict(counts), training_by_start={k: dict(v) for k, v in by_start.items()}, replayed=replayed,
        evaluation=evaluations, audit_source_sha256=sha(Path(__file__)))
    write(output / 'audit.json', result)
    print(json.dumps(dict(status='passed', counts=result['counts'])), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    audit_run(args.run.resolve())


if __name__ == '__main__':
    main()
