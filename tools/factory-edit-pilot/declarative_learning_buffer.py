"""Complete-episode successful edits, reusable packed inputs and causal admission."""
from collections import Counter
from copy import deepcopy

from blueprint_ppo_update import digest, require
import declarative_success_features as success_features

SCHEMA = 'declarative-success-buffer-v1'


def physical(observation):
    return {key:observation[key] for key in ('blueprint_sha256','facilities','nodes','components',
                                            'port_assignments','edges','best_metrics')}


class Buffer:
    def __init__(self, saved=None):
        self.state = deepcopy(saved) if saved is not None else dict(schema=SCHEMA,entries={},active={},
            positive_improvements=0,admissions=0,duplicates=0,selected_uses={})
        require(self.state['schema']==SCHEMA, 'declarative replay checkpoint schema')
        self.cache = {}
        self.admission_observer = None
        self.label_count=sum(len(e['labels']) for e in self.state['entries'].values())

    def snapshot(self):
        return deepcopy(self.state)

    def eligible(self, evidence, ordinal_update, strata):
        self.hydrate(evidence, strata)
        result=[]
        for identity,entry in sorted(self.state['entries'].items()):
            require(entry['admitted_update']<ordinal_update, 'declarative current update cannot enter replay')
            if 'sampling_priority_v1' in entry:
                # Priorities change after updates; the cached policy rows do not.
                self.cache[identity]['sampling_priority_v1']=entry['sampling_priority_v1']
            result.append(self.cache[identity])
        return result

    def hydrate(self, evidence, strata):
        # On restoration many uncached sequences refer to the same source chunk.
        # Load that chunk once, instead of repeatedly inflating it in ID order.
        pending={identity:entry for identity,entry in self.state['entries'].items() if identity not in self.cache}
        rows={identity:[None]*len(entry['labels']) for identity,entry in pending.items()}
        by_chunk={}
        for identity,entry in pending.items():
            for index,label in enumerate(entry['labels']):
                by_chunk.setdefault(label['reference'][0],[]).append((identity,index,label))
        for ordinal,labels in sorted(by_chunk.items()):
            for identity,index,label in labels:
                sample=evidence.sample(label['reference'])
                rows[identity][index]=dict(input=sample['input'],decision=sample['decision'],stage=label['stage'],
                    reference=label['reference'],return_to_terminal=label['return_to_terminal'])
        for identity,entry in pending.items():
            product=entry['input']['draw']['product'] if entry['input'] else 'fixture-diagnostic'
            matches=[name for name,products in strata.items() if product in products]
            require(len(matches)==1, 'declarative replay product must identify one configured stratum')
            self.cache[identity]=dict(id=identity,child_admitted_update=entry['admitted_update'],origin='own',
                stratum=matches[0],product=product,rows=rows[identity])
            if 'retention_features_v1' in entry:
                self.cache[identity]['retention_features_v1']=entry['retention_features_v1']

    def mark_used(self, groups):
        for group in groups:
            for identity in group:
                require(identity in self.state['entries'], 'declarative selected replay identity missing')
                self.state['entries'][identity]['uses'] += 1
                self.state['selected_uses'][identity] = self.state['selected_uses'].get(identity,0)+1
                if 'selected_uses_total' in self.state:self.state['selected_uses_total']+=1

    def ingest(self, chunk, ordinal, evidence, config, gamma, strata=None):
        events = []
        replacement = None
        priority_index = None
        priority_enabled = (config.get('sampling') or {}).get('kind')=='positive_advantage'
        terminal_order = 0
        before = {s['index']:s for s in chunk['native_before']}
        for index, sample in enumerate(chunk['samples']):
            reference = (ordinal,index)
            stream, episode = sample['stream'],sample['episode']
            active = self.state['active'].get(stream)
            if active is None:
                require(sample['policy_action']==1, 'declarative replay episode starts at action one')
                metadata = before[stream]['metadata']['training_input']
                require(metadata is not None or config.get('fixture_diagnostic',False), 'declarative replay needs generated input provenance')
                active = dict(episode=episode,last_action=0,rows=[],segment=[],pending=[],input=metadata)
                self.state['active'][stream] = active
            require(active['episode']==episode and sample['policy_action']==active['last_action']+1,
                    'declarative replay stream continuity')
            active['last_action'] = sample['policy_action']
            active['rows'].append((reference,sample['reward']))
            if sample['state_changed']:
                require(sample['outcome']['status'] not in ('rejected','unchanged'), 'declarative changed action status')
                active['segment'].append(reference)
            if sample['reward']>0:
                require(sample['validated_witness'] is not None and sample['outcome']['status']=='improved',
                    'declarative success requires native-validated improvement')
                require(bool(active['segment']), 'declarative improvement has no edit labels')
                active['pending'].append(dict(improvement=reference,references=list(active['segment'])))
                self.state['positive_improvements'] += 1
                events.append(dict(kind='archived',improvement=reference))
                active['segment'] = []
            # A valid worse insertion is still a preparation action. Automatic
            # validation does not discard its credit before a later improvement.
            if not sample['terminal']:
                continue
            returns, value = {},0.
            for ref,reward in reversed(active['rows']):
                value = reward+gamma*value; returns[ref] = value
            for segment_order,segment in enumerate(active['pending']):
                labels = [dict(reference=ref,stage=evidence.sample(ref)['action']['kind'],return_to_terminal=returns[ref])
                          for ref in segment['references']]
                identity = digest([dict(state=physical(evidence.sample(label['reference'])['observation']),
                    action=evidence.sample(label['reference'])['action'],return_to_terminal=round(label['return_to_terminal'],12))
                    for label in labels])
                features = success_features.extract([evidence.sample(label['reference']) for label in labels], identity) \
                    if success_features.required(config) else None
                candidate = None
                if self.admission_observer is not None or identity not in self.state['entries']:
                    candidate = dict(id=identity,admitted_update=ordinal,labels=labels,
                        segment=segment,input=active['input'],episode=episode,stream=stream,
                        episode_chunks=sorted({ref[0] for ref,_ in active['rows']}),uses=0,occurrences=1)
                    if features is not None:
                        candidate['retention_features_v1'] = features
                if self.admission_observer is not None:
                    self.admission_observer((ordinal,terminal_order,segment_order),candidate,evidence)
                if identity in self.state['entries']:
                    if features is not None:
                        require(self.state['entries'][identity].get('retention_features_v1') == features,
                            'declarative.replay-retention.duplicate-features: conflicting semantic identity')
                    self.state['entries'][identity]['occurrences'] += 1; self.state['duplicates'] += 1
                    events.append(dict(kind='duplicate',id=identity))
                else:
                    if priority_enabled and priority_index is None:
                        from declarative_replay_sampling import PriorityBootstrap
                        require(strata is not None,'declarative.replay-sampling.strata: priority initialization requires strata')
                        priority_index=PriorityBootstrap(self.state['entries'],strata)
                    if config.get('retention'):
                        require(strata is not None and len(labels)<=config['max_labels'],
                            'declarative.replay-retention: incoming sequence exceeds capacity or strata missing')
                        incoming=dict(input=active['input'])
                        while len(self.state['entries'])>=config['max_sequences'] or self.label_count+len(labels)>config['max_labels']:
                            if replacement is None:
                                from declarative_replay_retention import ReplacementIndex
                                replacement=ReplacementIndex(self.state['entries'],ordinal,config['retention'],strata)
                            removed,detail=replacement.choose(incoming)
                            if priority_index is not None:priority_index.remove(removed)
                            if 'selected_uses_total' not in self.state:self.state['selected_uses_total']=sum(self.state['selected_uses'].values())
                            self.state['selected_uses'].pop(removed,None)
                            entry=self.state['entries'].pop(removed);self.cache.pop(removed,None)
                            self.label_count-=len(entry['labels'])
                            self.state['evictions']=self.state.get('evictions',0)+1
                            events.append(dict(kind='evicted',id=removed,**detail))
                    require(len(self.state['entries'])<config['max_sequences'], 'declarative replay sequence limit')
                    require(self.label_count+len(labels)<=config['max_labels'],
                        'declarative replay label limit')
                    self.state['entries'][identity] = candidate
                    if priority_index is not None:
                        candidate['sampling_priority_v1']=dict(value=priority_index.maximum(candidate),observed_update=None)
                        priority_index.add(candidate)
                    if replacement is not None:replacement.add(self.state['entries'][identity])
                    self.label_count+=len(labels)
                    self.state['admissions'] += 1
                    events.append(dict(kind='admitted',id=identity,labels=len(labels)))
            del self.state['active'][stream]
            terminal_order += 1
        return events

    def retained_chunks(self):
        from declarative_replay_storage import references
        return set(references(self.state))

    def summary(self, eligible):
        return dict(eligible=len(eligible),by_stratum=dict(Counter(s['stratum'] for s in eligible)),
            own_entries=len(self.state['entries']),positive_improvements=self.state['positive_improvements'],
            admissions=self.state['admissions'],duplicates=self.state['duplicates'],
            selected_uses=self.state.get('selected_uses_total',sum(self.state['selected_uses'].values())),cached_sequences=len(self.cache),
            evictions=self.state.get('evictions',0),retained_labels=self.label_count)
