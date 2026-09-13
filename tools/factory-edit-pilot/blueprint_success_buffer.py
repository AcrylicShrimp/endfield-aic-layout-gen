"""Durable references to policy-discovered successful edit segments and returns."""
from collections import OrderedDict
from copy import deepcopy

from blueprint_ppo_store import checked, load, read
from blueprint_ppo_update import digest, require


def physical(observation):
    return dict(blueprint=observation['blueprint_sha256'], facilities=observation['facilities'],
                paths=[e['path'] for e in observation['edges']], best=observation['best_metrics'])


class Evidence:
    def __init__(self, output, job):
        self.output, self.job = output, job
        self.current = None
        self.cache = OrderedDict()

    def begin(self, ordinal, chunk):
        self.current = (ordinal, chunk)

    def chunk(self, ordinal):
        if self.current is not None and self.current[0] == ordinal:
            return self.current[1]
        if ordinal not in self.cache:
            require(0 < ordinal <= len(self.job['records']), f'blueprint.success-buffer: unknown source chunk {ordinal}')
            if 'replay_sources' in self.job:
                require(str(ordinal) in self.job['replay_sources'], f'blueprint.success-buffer: missing replay source {ordinal}')
                reference = self.job['replay_sources'][str(ordinal)]
            else:
                record = read(checked(self.job['records'][ordinal - 1], self.output))
                require(record['update'] == ordinal, 'blueprint.success-buffer: ordered source record')
                reference = record['chunk']
            self.cache[ordinal] = load(checked(reference, self.output))
            if len(self.cache) > 8:
                self.cache.popitem(last=False)
        self.cache.move_to_end(ordinal)
        return self.cache[ordinal]

    def sample(self, reference):
        ordinal, index = reference
        samples = self.chunk(ordinal)['samples']
        require(index in samples if isinstance(samples, dict) else 0 <= index < len(samples),
                f'blueprint.success-buffer: source sample {reference}')
        return samples[index]

    def published(self, ordinal, compact):
        """Keep the small immutable replay payload already in memory after commit."""
        self.cache[ordinal] = compact
        self.cache.move_to_end(ordinal)
        while len(self.cache) > 8:
            self.cache.popitem(last=False)
        if self.current is not None and self.current[0] == ordinal:
            self.current = None

    def successor(self, reference):
        ordinal, index = reference
        chunk = self.chunk(ordinal)
        sample = chunk['samples'][index]
        for row in chunk['samples'][index + 1:]:
            if row['stream'] == sample['stream']:
                return row['observation']
        return next(f['successor_observation'] for f in chunk['frontiers'] if f['stream'] == sample['stream'])


def stage_labels(references, evidence):
    labels = []
    for reference in references:
        sample = evidence.sample(reference)
        action = sample['input']['actions'][sample['index']]
        if sample['outcome']['status'] == 'rejected':
            continue
        kind = action['kind']
        if kind == 'pose':
            changed = sample['observation']['facilities'] != evidence.successor(reference)['facilities']
            stage = 'pose'
        elif kind in ('repair', 'repair_all'):
            changed = [e['path'] for e in sample['observation']['edges']] != [e['path'] for e in evidence.successor(reference)['edges']]
            stage = 'repair'
        elif kind == 'commit' and reference == references[-1]:
            changed, stage = True, 'commit'
        else:
            continue
        if changed:
            labels.append(dict(reference=reference, stage=stage))
    return labels


class SuccessBuffer:
    def __init__(self, saved=None):
        self.state = deepcopy(saved) if saved is not None else dict(schema='blueprint-success-buffer-v1',
            entries={}, active={}, positive_commits=0, admissions=0, duplicates=0)
        require(self.state['schema'] == 'blueprint-success-buffer-v1', 'blueprint.success-buffer: checkpoint schema')

    def snapshot(self):
        return deepcopy(self.state)

    def mark_used(self, selected_ids):
        for group in selected_ids:
            for identity in group:
                self.state['entries'][identity]['uses'] += 1

    def ingest(self, chunk, ordinal, evidence, config, gamma):
        events = []
        for index, sample in enumerate(chunk['samples']):
            reference = (ordinal, index)
            stream, episode = sample['stream'], sample['episode']
            active = self.state['active'].get(stream)
            if active is None:
                require(sample['policy_action'] == 1, 'blueprint.success-buffer: episode must begin at action one')
                active = dict(episode=episode, last_action=0, rows=[], segment=[], pending=[])
                self.state['active'][stream] = active
            require(active['episode'] == episode and sample['policy_action'] == active['last_action'] + 1,
                    'blueprint.success-buffer: exact stream/episode/action continuity')
            active['last_action'] = sample['policy_action']
            active['rows'].append((reference, sample['reward']))
            active['segment'].append(reference)
            action = sample['input']['actions'][sample['index']]
            outcome = sample['outcome']
            if action['kind'] == 'restore_best' and outcome['status'] != 'rejected':
                active['segment'] = []
            if action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved'):
                if sample['reward'] > 0:
                    require(sample['validated_witness'] is not None, 'blueprint.success-buffer: success requires native witness')
                    labels = stage_labels(active['segment'], evidence)
                    segment = dict(commit=reference, references=list(active['segment']), labels=labels,
                                   seed_chunk=ordinal, stream=stream, episode=episode, start=sample['start'])
                    active['pending'].append(segment)
                    self.state['positive_commits'] += 1
                    events.append(dict(kind='archived', **deepcopy(segment)))
                active['segment'] = []
            if not sample['terminal']:
                continue
            returns, value = {}, 0.
            for ref, reward in reversed(active['rows']):
                value = reward + gamma * value
                returns[ref] = value
            for segment in active['pending']:
                labels = [dict(label, return_to_terminal=returns[label['reference']]) for label in segment['labels']]
                identity = digest([dict(state=physical(evidence.sample(label['reference'])['observation']),
                    action=evidence.sample(label['reference'])['input']['actions'][evidence.sample(label['reference'])['index']],
                    return_to_terminal=round(label['return_to_terminal'], 12)) for label in labels])
                if identity in self.state['entries']:
                    self.state['entries'][identity]['occurrences'] += 1
                    self.state['duplicates'] += 1
                    events.append(dict(kind='duplicate', id=identity, commit=segment['commit']))
                else:
                    require(len(self.state['entries']) < config['max_sequences'], 'blueprint.success-buffer: unique sequence guard')
                    require(sum(len(e['labels']) for e in self.state['entries'].values()) + len(labels) <= config['max_labels'],
                            'blueprint.success-buffer: label reference guard')
                    self.state['entries'][identity] = dict(id=identity, admitted_update=ordinal, labels=labels,
                        segment=deepcopy(segment), uses=0, occurrences=1)
                    self.state['admissions'] += 1
                    events.append(dict(kind='admitted', id=identity, commit=segment['commit'], labels=len(labels)))
            del self.state['active'][stream]
        return events

    def sequences(self, evidence, ordinal):
        result = []
        for identity in sorted(self.state['entries']):
            entry = self.state['entries'][identity]
            require(entry['admitted_update'] < ordinal, 'blueprint.success-buffer: only previous updates may be replayed')
            rows = []
            for label in entry['labels']:
                require(label['reference'][0] < ordinal, 'blueprint.success-buffer: old training rows only')
                sample = evidence.sample(label['reference'])
                rows.append(dict(input=sample['input'], index=sample['index'], stage=label['stage'],
                                 reference=label['reference'], return_to_terminal=label['return_to_terminal']))
            result.append(dict(id=entry['id'], admitted_update=entry['admitted_update'], rows=rows))
        return result
