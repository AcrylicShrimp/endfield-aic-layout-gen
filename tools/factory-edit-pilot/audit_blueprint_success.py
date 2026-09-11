"""Reconstruct every learner/buffer update and verify purely self-generated training."""
import argparse
from collections import Counter
import json
from pathlib import Path
import tempfile

import torch

from audit_blueprint_ppo import WitnessAudit, check_sample
from blueprint_edit_input import prepare
from blueprint_ppo import check_sources
from blueprint_ppo_batch import pack
from blueprint_ppo_environment import Runtime, Stream, semantic
from blueprint_ppo_store import ROOT, checked, load, read, restore_learners, sha, write
from blueprint_ppo_update import digest, predictions, require
from blueprint_success import restore_buffer
from blueprint_success_buffer import Evidence, SuccessBuffer
from blueprint_success_update import update_success
from blueprint_ppo_batch import distribution


def audit_sampling(model, sampler, samples, config):
    if config['purpose'] == 'success-mechanical-readiness':
        for sample in samples:
            require(sample['input']['actions'][sample['index']] == config['success']['mechanical_actions'][sample['policy_action'] - 1],
                    'blueprint.success-audit: declared mechanical fixture only')
        return
    size = config['training']['streams']
    with torch.no_grad():
        for offset in range(0, len(samples), size):
            rows = samples[offset:offset + size]
            batch = pack([row['input'] for row in rows])
            logits, _ = model.forward_packed(batch)
            logp, _ = distribution(logits, batch)
            for row, probabilities in zip(rows, logp.exp().split(batch['action_counts']), strict=True):
                index = int(torch.multinomial(probabilities, 1, generator=sampler))
                require(index == row['index'], 'blueprint.success-audit: actual on-policy sampling, no scripted teacher')


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
            saved, buffer = restore_buffer(load(directory / 'checkpoints/0000.pt.gz'))
            require(buffer.snapshot() == SuccessBuffer().snapshot(), 'blueprint.success-audit: cold empty buffer')
            evidence = Evidence(output, job)
            model, actor, critic, sampler, shuffle = restore_learners(saved, config['ppo'], state['protocol_sha256'])
            prior_frontiers = {}
            for ordinal, record_ref in enumerate(job['records'], 1):
                record = read(checked(record_ref, output))
                chunk = load(checked(record['chunk'], output))
                require(record['update'] == chunk['update'] == ordinal and chunk['seed'] == seed, 'ordered chunk identity')
                require(chunk['protocol_sha256'] == state['protocol_sha256'], 'chunk protocol identity')
                require(chunk['model_before'] == digest(model.state_dict()), 'behavior checkpoint identity')
                require(len(chunk['samples']) == config['training']['streams'] * config['training']['chunk_actions'], 'registered chunk size')
                for before in chunk['native_before']:
                    restored = Stream.restore(runtime, before)
                    observation = json.loads(restored.session.observation_json())
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
                audit_sampling(model, sampler, chunk['samples'], config)
                evidence.begin(ordinal, chunk)
                require(digest(buffer.state) == digest(chunk['buffer']['before']) == record['buffer']['before_sha256'],
                        'blueprint.success-audit: buffer before learner')
                sequences = buffer.sequences(evidence, ordinal)
                actual = update_success(model, actor, critic, shuffle, chunk['samples'], chunk['frontiers'],
                                        config['ppo'], config['success'], sequences, seed, ordinal)
                buffer.mark_used(actual['success']['selected_ids'])
                events = buffer.ingest(chunk, ordinal, evidence, config['success'], config['ppo']['gamma'])
                require(events == chunk['buffer']['events'], 'blueprint.success-audit: complete archive/admission events')
                require(digest(buffer.state) == digest(chunk['buffer']['after']) == record['buffer']['after_sha256'],
                        'blueprint.success-audit: exact buffer transition')
                counts['archived_successes'] += sum(e['kind'] == 'archived' for e in events)
                counts['admitted_sequences'] += sum(e['kind'] == 'admitted' for e in events)
                counts['duplicate_sequences'] += sum(e['kind'] == 'duplicate' for e in events)
                counts['sampled_success_labels'] += actual['success']['label_count']
                counts['positive_weight_labels'] += actual['success']['positive_labels']
                counts['updates_with_positive_imitation_weight'] += actual['success']['positive_labels'] > 0
                expected = {k: v for k, v in chunk['ppo'].items() if k != 'seconds'}
                require(actual == expected, f'exact learner/Adam/RNG/credit replay seed={seed} update={ordinal}')
                counts['exact_update_replays'] += 1
                checkpoint_path = directory / f'checkpoints/{ordinal:04d}.pt.gz'
                if checkpoint_path.exists():
                    checkpoint, checkpoint_buffer = restore_buffer(load(checked(record['checkpoint'], output)))
                    require(digest(checkpoint_buffer.state) == digest(buffer.state), 'blueprint.success-audit: checkpoint open segments and bank')
                    require(digest(checkpoint['sampler_rng']) == digest(sampler.get_state()), 'blueprint.success-audit: replayed sampling RNG')
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
                checkpoint, _ = restore_buffer(load(directory / f'checkpoints/{eval_ref["update"]:04d}.pt.gz'))
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
    result = dict(schema='blueprint-success-audit-v1', status='passed', protocol_sha256=state['protocol_sha256'],
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
