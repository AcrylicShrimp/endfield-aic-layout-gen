"""Admit frozen parent replay with original namespaces and full terminal returns."""
from collections import OrderedDict
from concurrent.futures import ProcessPoolExecutor, as_completed
from copy import deepcopy
import json
import multiprocessing
import os
import time
from pathlib import Path
import tempfile
from types import SimpleNamespace

import torch

from blueprint_edit_audit import audit
from audit_blueprint_ppo import WitnessAudit
from blueprint_edit_input import prepare
from blueprint_ppo_environment import semantic
from blueprint_ppo_store import load, read, reference, restore_learners, save, sha, write
from blueprint_ppo_update import digest
from blueprint_replay_contract import SOURCE, Sources, check, classify
from blueprint_success import restore_buffer
from blueprint_success_buffer import physical, stage_labels
from post_pose_recovery import advance
from native import load_native


class ParentEvidence:
    def __init__(self, root, seed, sources):
        self.root, self.seed, self.sources = Path(root).resolve(), seed, sources
        self.protocol_path = sources.path(self.root / 'protocol.json')
        self.protocol = read(self.protocol_path)
        source_root = Path(self.protocol['source_root'])
        for path, expected in self.protocol['sources'].items():
            sources.path(source_root / path, expected)
        sources.path(self.protocol['native_library'], self.protocol['native_sha256'])
        state = read(sources.path(self.root / 'state.json'))
        check(state['status'] == 'complete' and state['protocol_sha256'] == sha(self.protocol_path),
              'parent-incompatible', 'admission', 'parent must be complete and source-bound', self.root)
        self.job = state['jobs'][str(seed)]
        self.checkpoint_path = sources.reference(self.root, self.job['latest_checkpoint'])
        self.envelope = load(self.checkpoint_path)
        self.learner, self.buffer = restore_buffer(self.envelope)
        check(self.learner['seed'] == seed and self.learner['update'] == self.job['update'] == 512,
              'parent-incompatible', 'admission', 'expected the completed update-512 parent', self.root)
        check(not self.buffer.state['active'] and len(self.learner['streams']) == 8
              and [s['index'] for s in self.learner['streams']] == list(range(8))
              and all(s['native'] is None and s['episode'] == 256 for s in self.learner['streams']),
              'parent-incompatible', 'admission', 'parent must be at the registered true episode boundary', self.root)
        config = self.protocol['config']
        check(config['training']['streams'] == 8 and config['training']['chunk_actions'] == 32
              and config['training']['episode_actions'] == 64 and config['ppo']['gamma'] == 1.,
              'parent-incompatible', 'admission', 'parent stream/horizon/return contract differs', self.root)
        # Authenticated CPU import preserves optimizer ordering before device migration.
        model, actor, critic, _, _ = restore_learners(deepcopy(self.learner), config['ppo'], sha(self.protocol_path))
        check(bool(actor.state) and bool(critic.state), 'parent-incompatible', 'admission', 'parent Adam states are empty', self.root)
        for optimizer in (actor, critic):
            for group in optimizer.param_groups:
                for parameter in group['params']:
                    state = optimizer.state.get(parameter, {})
                    check(all(state[k].shape == parameter.shape for k in ('exp_avg', 'exp_avg_sq') if k in state),
                          'parent-incompatible', 'admission', 'Adam parameter shape/order mismatch', self.root)
        check(digest(actor.state_dict()) == digest(self.learner['actor_optimizer'])
              and digest(critic.state_dict()) == digest(self.learner['critic_optimizer']),
              'parent-incompatible', 'admission', 'CPU Adam import changed saved state', self.root)
        self.cache, self.records, self.inputs = OrderedDict(), {}, {}
        self.namespace = self.root.name
        self.runtime = SimpleNamespace(module=load_native(self.protocol['native_library']),
            spec=read(source_root / config['probe_spec']), original=None, request=None)
        self.checked_witnesses = set()

    def chunk(self, ordinal):
        if ordinal not in self.cache:
            check(1 <= ordinal <= 512, 'source-invalid', 'admission', 'source update outside parent range', ordinal)
            if ordinal not in self.records:
                record = read(self.sources.reference(self.root, self.job['records'][ordinal - 1]))
                check(record['update'] == ordinal, 'source-invalid', 'admission', 'source update order mismatch', self.root)
                self.records[ordinal] = record
            self.cache[ordinal] = load(self.sources.reference(self.root, self.records[ordinal]['chunk']))
            if len(self.cache) > 4:
                self.cache.popitem(last=False)
        self.cache.move_to_end(ordinal)
        return self.cache[ordinal]

    def sample(self, ref):
        return self.chunk(ref[0])['samples'][ref[1]]

    def successor(self, ref):
        chunk = self.chunk(ref[0])
        sample = chunk['samples'][ref[1]]
        for row in chunk['samples'][ref[1] + 1:]:
            if row['stream'] == sample['stream']:
                return row['observation']
        return next(f['successor_observation'] for f in chunk['frontiers'] if f['stream'] == sample['stream'])

    def episode(self, stream, episode):
        rows = []
        for ordinal in (episode * 2 + 1, episode * 2 + 2):
            rows.extend(((ordinal, i), row) for i, row in enumerate(self.chunk(ordinal)['samples']) if row['stream'] == stream)
        check([r['policy_action'] for _, r in rows] == list(range(1, 65))
              and all(r['episode'] == episode for _, r in rows)
              and [r['terminal'] for _, r in rows] == [False] * 63 + [True],
              'source-invalid', 'admission', 'source is not one complete true 64-action episode', (self.namespace, stream, episode))
        return rows

    def input_metadata(self, stream, episode):
        key = (stream, episode)
        if key not in self.inputs:
            before = next(s for s in self.chunk(episode * 2 + 1)['native_before'] if s['index'] == stream)
            metadata = before['metadata']['training_input']
            directory = Path(self.protocol['config']['training_input']['directory'])
            if not directory.is_absolute():
                directory = Path(self.protocol['source_root']) / directory
            record = read(self.sources.reference(directory, metadata['reference']))
            check(record['identity'] == metadata['identity'] and record['identity']['slot'] == episode * 8 + stream,
                  'source-invalid', 'admission', 'source input stream/episode identity mismatch', key)
            for ref in record['files'].values():
                self.sources.reference(directory, ref)
            payload = read(self.sources.reference(directory, record['files']['input']))
            check(digest(payload) == record['input_sha256'] == metadata['input_sha256'],
                  'source-invalid', 'admission', 'source input payload identity mismatch', key)
            self.inputs[key] = (record, payload)
        return self.inputs[key]

    def sequence(self, entry, origin, strata):
        segment = entry['segment']
        stream, episode = segment['stream'], segment['episode']
        episode_rows = self.episode(stream, episode)
        record, payload = self.input_metadata(stream, episode)
        returned, returns = 0., {}
        for ref, sample in reversed(episode_rows):
            returned += sample['reward']
            returns[ref] = returned
        reconstructed = stage_labels(segment['references'], self)
        check(reconstructed == [{k: l[k] for k in ('reference', 'stage')} for l in entry['labels']],
              'source-invalid', 'admission', 'source stage labels differ from the recorded full segment', entry['id'])
        rows = []
        for label in entry['labels']:
            ref = tuple(label['reference'])
            sample = self.sample(ref)
            check(abs(returns[ref] - label['return_to_terminal']) < 1e-12,
                  'source-invalid', 'admission', 'label return differs from the complete true episode', ref)
            check(digest(prepare(sample['observation'])) == digest(sample['input']),
                  'source-invalid', 'admission', 'recorded graph/candidates differ from source observation', ref)
            rows.append(dict(input=sample['input'], index=sample['index'], observation=sample['observation'],
                stage=label['stage'], reference=label['reference'], namespace=self.namespace,
                return_to_terminal=label['return_to_terminal'], policy_action=sample['policy_action']))
        identity = digest([dict(state=physical(r['observation']), action=r['input']['actions'][r['index']],
            return_to_terminal=round(r['return_to_terminal'], 12)) for r in rows])
        check(identity == entry['id'], 'source-invalid', 'admission', 'physical/action/return identity differs', entry['id'])
        commit = self.sample(segment['commit'])
        check(commit['reward'] > 0 and commit['validated_witness'] is not None,
              'source-invalid', 'admission', 'positive source commit has no native witness', entry['id'])
        audit(payload['original'], commit['observation']['blueprint'], commit['validated_witness'])
        witness_key = digest((payload['request'], commit['observation']['blueprint'], commit['validated_witness']))
        if witness_key not in self.checked_witnesses:
            self.runtime.original, self.runtime.request = payload['original'], payload['request']
            with tempfile.TemporaryDirectory(prefix='blueprint-source-admission-') as directory:
                checker = WitnessAudit(self.runtime, directory)
                checker.check(commit['observation']['blueprint'], commit['validated_witness'])
            self.checked_witnesses.add(witness_key)
        return dict(id=entry['id'], origin=origin, child_admitted_update=0,
            source_admitted_update=entry['admitted_update'], source_uses=entry['uses'], source_occurrences=entry['occurrences'],
            origin_refs=[dict(origin=origin, namespace=self.namespace, id=entry['id'], source_admitted_update=entry['admitted_update'])],
            stratum=classify(record['draw']['product'], strata), product=record['draw']['product'],
            facilities=len(rows[0]['observation']['facilities']), input_identity=record['identity'],
            input_sha256=record['input_sha256'], blueprint_sha256=record['blueprint_sha256'], rows=rows,
            segment=deepcopy(segment), namespace=self.namespace,
            native_witness=commit['validated_witness'], demonstrated_score=self.successor(segment['commit'])['best_metrics']['score'])

    def anchor_states(self, sequence):
        stream, episode = sequence['segment']['stream'], sequence['segment']['episode']
        native = deepcopy(next(s for s in self.chunk(episode * 2 + 1)['native_before'] if s['index'] == stream)['native'])
        selected = {tuple(row['reference']): row for row in sequence['rows']}
        states = []
        for ref, sample in self.episode(stream, episode):
            if ref in selected:
                row = selected[ref]
                saved = deepcopy(native)
                saved['elapsed_ns'] = round(sample['observation']['elapsed_ms'] * 1_000_000)
                check(not saved['closed'] and saved['config']['max_actions'] == 64,
                      'continuation-state-invalid', 'admission', 'anchor has a closed or altered action horizon', ref)
                check(sample['observation']['best_metrics']['score'][0] > sequence['demonstrated_score'][0],
                      'continuation-state-invalid', 'admission', 'anchor already satisfies demonstrated target', ref)
                session = self.runtime.module.BlueprintEditSession.from_checkpoint(str(Path(__file__).resolve().parents[2]),
                    json.dumps(self.runtime.spec['catalogs']), json.dumps(saved))
                restored = json.loads(session.observation_json())
                check(semantic(restored) == semantic(sample['observation']), 'continuation-state-invalid',
                      'admission', 'native reconstructed anchor differs from the recorded state', ref)
                states.append(dict(stage=row['stage'], reference=ref, namespace=self.namespace, native=saved,
                    observation=sample['observation'], input=row['input'], index=row['index'],
                    recorded_elapsed_ms=sample['observation']['elapsed_ms'], demonstrated_score=sequence['demonstrated_score']))
            native = advance(native, sample, self.successor(ref))
        check(len(states) == len(sequence['rows']), 'source-invalid', 'admission', 'anchor state coverage differs', sequence['id'])
        return states


def prepare_source_arm(matrix, arm, output, specification, choices, core):
    os.sched_setaffinity(0, {core})
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    began = time.monotonic()
    sources = Sources()
    sources.reference(matrix, arm['protocol'])
    seed, name = arm['seed'], arm['name']
    evidence = ParentEvidence(Path(matrix) / arm['directory'], seed, sources)
    if name.startswith('center-'):
        check(evidence.protocol['config']['ppo'] == specification['ppo'],
              'parent-incompatible', 'admission', 'parent PPO contract differs from the registered continuation', name)
        entries = sorted(evidence.buffer.state['entries'].values(), key=lambda e: (e['segment']['episode'], e['segment']['stream']))
        sequences = []
        for entry in entries:
            check(time.monotonic() - began < 1800, 'resource-exhausted', 'admission', 'source admission deadline', name)
            sequences.append(evidence.sequence(entry, 'parent-own', specification['strata']))
        sequences.sort(key=lambda e: e['id'])
        path = Path(output) / f'parent-{seed}.pt.gz'
        save(path, dict(schema=SOURCE, seed=seed, parent_root=str(evidence.root), parent_protocol=evidence.protocol,
            parent_protocol_sha256=sha(evidence.protocol_path), parent_checkpoint_sha256=sha(evidence.checkpoint_path),
            learner=evidence.learner, parent_buffer=evidence.buffer.snapshot(), sequences=sequences))
        result = dict(kind='parent', seed=seed, file=reference(path, output), entries=len(sequences),
                      labels=sum(len(s['rows']) for s in sequences))
    else:
        sequences = []
        for choice in choices:
            check(time.monotonic() - began < 1800, 'resource-exhausted', 'admission', 'source admission deadline', name)
            sequence = evidence.sequence(evidence.buffer.state['entries'][choice['entry_id']], 'imported', specification['strata'])
            check(sequence['input_identity']['slot'] == choice['slot'] and sequence['product'] == choice['product'],
                  'source-invalid', 'admission', 'selected import identity/product mismatch', choice['entry_id'])
            sequence['anchor_states'] = evidence.anchor_states(sequence)
            sequences.append(sequence)
        path = Path(output) / f'import-part-{seed}.pt.gz'
        save(path, dict(schema=SOURCE, sequences=sequences))
        result = dict(kind='import', seed=seed, file=reference(path, output), entries=len(sequences))
    result.update(seconds=time.monotonic() - began, pid=os.getpid(), cpu=core, sources=sources.bindings)
    write(Path(output) / f'{name}-admission.json', result)
    return result


def prepare_sources(matrix, output, specification):
    matrix, output = Path(matrix).resolve(), Path(output).resolve()
    check(not output.exists(), 'source-invalid', 'admission', 'source bundle output already exists', output)
    output.mkdir(parents=True)
    sources = Sources()
    matrix_manifest = read(sources.path(matrix / 'matrix.json'))
    selected_path = Path(specification['import_selection'])
    choices = read(sources.path(selected_path, specification['import_selection_sha256']))
    check({(c['arm'], c['slot']) for c in choices} == {tuple(c) for c in specification['import_cases']}
          and len(choices) == 5, 'source-invalid', 'admission', 'registered five imported successes differ', selected_path)
    names = [f'center-seed-{seed}' for seed in specification['seeds']] + sorted({c['arm'] for c in choices})
    cores = sorted(os.sched_getaffinity(0))
    check(len(cores) >= len(names), 'execution-gate', 'admission', 'source validation needs five independent CPU allocations')
    results = []
    with ProcessPoolExecutor(max_workers=len(names), mp_context=multiprocessing.get_context('spawn')) as pool:
        futures = [pool.submit(prepare_source_arm, matrix,
            next(a for a in matrix_manifest['arms'] if a['name'] == name), output, specification,
            [c for c in choices if c['arm'] == name], cores[i]) for i, name in enumerate(names)]
        for future in as_completed(futures):
            result = future.result()
            results.append(result)
            for path, expected in result['sources'].items():
                check(path not in sources.bindings or sources.bindings[path] == expected,
                      'source-invalid', 'admission', 'source workers disagree on immutable identity', path)
                sources.bindings[path] = expected
            print(json.dumps(dict(stage='source-arm-admitted', **{k: v for k, v in result.items() if k != 'sources'})), flush=True)
    parents = [{k: v for k, v in r.items() if k not in ('sources', 'kind')} for r in results if r['kind'] == 'parent']
    parents.sort(key=lambda r: r['seed'])
    imports = []
    for result in results:
        if result['kind'] == 'import':
            imports.extend(load(sources.reference(output, result['file']))['sequences'])
    imports.sort(key=lambda s: (s['namespace'], s['input_identity']['slot']))
    check(sum(len(s['rows']) for s in imports) == 15 and all(s['stratum'] == 'large' for s in imports),
          'source-invalid', 'admission', 'expected fifteen large imported training labels', output)
    save(output / 'imports.pt.gz', dict(schema=SOURCE, sequences=imports))
    manifest = dict(schema=SOURCE, parents=parents, imports=reference(output / 'imports.pt.gz', output),
        strata=specification['strata'], imported_ids=[s['id'] for s in imports], sources=sources.bindings)
    write(output / 'manifest.json', manifest)
    # Part files are referenced by the admission receipts, so retain them until
    # preparation publishes a compact replacement receipt for the merged file.
    return manifest
