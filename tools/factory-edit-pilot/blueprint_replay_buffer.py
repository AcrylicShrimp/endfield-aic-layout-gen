"""Mixed-origin immutable replay plus a separate child discovery clock."""
from collections import Counter
from copy import deepcopy

from blueprint_replay_contract import check, classify
from blueprint_success_buffer import SuccessBuffer


class ReplayBuffer:
    def __init__(self, parent, imported, strata, saved=None):
        self.strata = strata
        self.fixed = {}
        # The first physical/action/return identity keeps its recorded tensors,
        # matching the historical dedup rule. Every additional origin is retained.
        for sequence in [*parent, *imported]:
            identity = sequence['id']
            if identity in self.fixed:
                current = self.fixed[identity]
                check(current['stratum'] == sequence['stratum'], 'source-invalid', 'admission',
                      'duplicate replay identity has conflicting strata', identity)
                current['origin_refs'].extend(deepcopy(sequence['origin_refs']))
            else:
                self.fixed[identity] = dict(sequence, origin_refs=deepcopy(sequence['origin_refs']))
        saved = saved or dict(schema='blueprint-mixed-replay-buffer-v1', own=None, selected_uses={})
        check(saved['schema'] == 'blueprint-mixed-replay-buffer-v1', 'parent-incompatible', 'restore', 'mixed buffer schema differs')
        self.own = SuccessBuffer(saved['own'])
        self.selected_uses = Counter(saved['selected_uses'])
        self.cache = {}

    def snapshot(self):
        return dict(schema='blueprint-mixed-replay-buffer-v1', own=self.own.snapshot(), selected_uses=dict(self.selected_uses))

    def eligible(self, evidence, child_ordinal):
        result = {key: dict(value, origin_refs=deepcopy(value['origin_refs'])) for key, value in self.fixed.items()}
        for identity in sorted(self.own.state['entries']):
            entry = self.own.state['entries'][identity]
            check(entry['admitted_update'] < child_ordinal, 'source-invalid', 'sampling',
                  'current child experience cannot enter replay', identity)
            if identity not in self.cache:
                rows = []
                for label in entry['labels']:
                    sample = evidence.sample(label['reference'])
                    rows.append(dict(input=sample['input'], index=sample['index'], stage=label['stage'],
                        reference=label['reference'], namespace='child-own', return_to_terminal=label['return_to_terminal']))
                chunk = evidence.chunk(entry['labels'][0]['reference'][0])
                sample = evidence.sample(entry['labels'][0]['reference'])
                before = next(s for s in chunk['native_before'] if s['index'] == sample['stream'])
                metadata = before['metadata']['training_input']
                product = metadata['draw']['product']
                self.cache[identity] = dict(id=identity, origin='child-own', child_admitted_update=entry['admitted_update'],
                    rows=rows, stratum=classify(product, self.strata), product=product,
                    facilities=len(sample['observation']['facilities']), input_identity=metadata['identity'],
                    origin_refs=[dict(origin='child-own', namespace='child-own', id=identity,
                                      child_admitted_update=entry['admitted_update'])])
            sequence = self.cache[identity]
            if identity in result:
                check(result[identity]['stratum'] == sequence['stratum'], 'source-invalid', 'sampling',
                      'child duplicate has conflicting strata', identity)
                result[identity]['origin_refs'].extend(sequence['origin_refs'])
            else:
                result[identity] = sequence
        return [result[key] for key in sorted(result)]

    def mark_used(self, selections):
        for ids in selections:
            for identity in ids:
                self.selected_uses[identity] += 1
                if identity not in self.fixed and identity in self.own.state['entries']:
                    self.own.state['entries'][identity]['uses'] += 1

    def ingest(self, chunk, child_ordinal, evidence, config, gamma):
        return self.own.ingest(chunk, child_ordinal, evidence, config, gamma)

    def summary(self, sequences):
        return dict(eligible=len(sequences), by_stratum=dict(Counter(s['stratum'] for s in sequences)),
            by_primary_origin=dict(Counter(s['origin'] for s in sequences)),
            own_positive_commits=self.own.state['positive_commits'], own_admissions=self.own.state['admissions'],
            own_duplicates=self.own.state['duplicates'], own_entries=len(self.own.state['entries']),
            child_selected_uses=sum(self.selected_uses.values()),
            fixed_entries=len(self.fixed), imported_entries=sum(s['origin'] == 'imported' for s in self.fixed.values()))
