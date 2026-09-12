"""Live native streams; optimizer updates never reset an unfinished draft."""
from collections import Counter
from copy import copy
import hashlib
import json
import time

from blueprint_edit_audit import audit
from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest, require
from native import load_native
from probe_blueprint_edit import request_from_initial
from blueprint_training_input import FreshInputs, validate_input


def semantic(observation):
    value = {k: observation[k] for k in ('blueprint_sha256', 'revision', 'attempts', 'facilities', 'nodes',
        'edges', 'conflicts', 'best_metrics', 'initial_metrics', 'draft_factory_validated', 'config')}
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


class Runtime:
    def __init__(self, config, library):
        validate_input(config)
        self.config = config
        self.spec = read(ROOT / config['probe_spec'])
        self.module = load_native(library)
        self.original = self.request = None
        self.inputs = None
        if config['training_input']['kind'] == 'fresh-random':
            self.inputs = FreshInputs(config, library, self.module, self.spec)
        else:
            self.original = read(ROOT / self.spec['initial'])
            self.request = request_from_initial(self.original)

    def episode_runtime(self, index, episode, saved_input=None):
        result = copy(self)
        if self.inputs is None:
            require(saved_input is None, 'blueprint.training-input.restore: generated input in a fixture diagnostic')
            return result, None
        if saved_input is None:
            payload, metadata = self.inputs.acquire(index, episode)
        else:
            payload, metadata = self.inputs.load(saved_input['reference'], index, episode)
            require(metadata == saved_input, 'blueprint.training-input.restore: episode provenance changed')
        result.original, result.request = payload['original'], payload['request']
        return result, metadata

    def open(self, start, actions, elapsed_ms):
        require(self.original is not None and self.request is not None,
                'blueprint.training-input.missing: select an episode or explicit evaluation input before opening')
        began = time.perf_counter()
        environment = dict(self.spec['config'], max_actions=actions + len(start['prefix']), elapsed_ms=elapsed_ms)
        session = self.module.BlueprintEditSession.open(str(ROOT), json.dumps(self.spec['catalogs']), json.dumps(self.request),
            json.dumps(self.original), json.dumps(environment))
        admission = time.perf_counter() - began
        prefix = []
        for action in start['prefix']:
            observation = json.loads(session.observation_json())
            outcome = self.step(session, observation, action)
            require(outcome['status'] != 'rejected', f'warm-start prefix failed: {start["name"]}: {outcome}')
            prefix.append(dict(action=action, outcome=outcome))
        return session, prefix, admission

    def restore(self, saved):
        return self.module.BlueprintEditSession.from_checkpoint(str(ROOT), json.dumps(self.spec['catalogs']), json.dumps(saved))

    def step(self, session, observation, action):
        return json.loads(session.step_json(json.dumps(dict(blueprint_sha256=observation['blueprint_sha256'],
            revision=observation['revision'], action=action))))

    def commit_witness(self, session, observation):
        witness = json.loads(session.validated_draft_json())
        audit(self.original, observation['blueprint'], witness)
        return witness

    def finish(self, session, observation):
        result = json.loads(session.finish_json())
        audit(self.original, observation['blueprint'], result['best'])
        return result


class Stream:
    def __init__(self, runtime, index):
        self.base_runtime = self.runtime = runtime
        self.index = index
        self.session = None
        self.episode = 0
        self.policy_actions = 0
        self.expected = None
        self.metadata = {}
        self.counts = Counter()
        self.boundaries = 0

    def observe(self):
        if self.session is None:
            input_began = time.perf_counter()
            self.runtime, input_metadata = self.base_runtime.episode_runtime(self.index, self.episode)
            input_access_seconds = time.perf_counter() - input_began
            starts = self.runtime.config['starts']
            start = starts[(self.index + self.episode) % len(starts)]
            training = self.runtime.config['training']
            self.session, prefix, admission = self.runtime.open(start, training['episode_actions'], training['elapsed_ms'])
            observation = json.loads(self.session.observation_json())
            self.metadata = dict(start=start['name'], prefix=prefix, admission_seconds=admission, training_input=input_metadata,
                                 input_access_seconds=input_access_seconds,
                                 initial_score=observation['initial_metrics']['score'], start_best=observation['best_metrics']['score'])
            if input_metadata is not None:
                require(observation['blueprint_sha256'] == input_metadata['blueprint_sha256'],
                        'blueprint.training-input.admission: episode blueprint differs from input record')
            self.policy_actions = 0
            self.counts = Counter()
            self.expected = None
        observation = json.loads(self.session.observation_json())
        require(not observation['done'], 'training native elapsed guard exhausted before the registered action count')
        if self.expected is not None:
            require(semantic(observation) == self.expected, 'draft/best continuity across optimizer update')
            self.boundaries += 1
            self.expected = None
        return observation

    def snapshot(self):
        return dict(index=self.index, episode=self.episode, policy_actions=self.policy_actions, expected=self.expected,
                    metadata=self.metadata, counts=dict(self.counts), boundaries=self.boundaries,
                    native=json.loads(self.session.checkpoint_json()) if self.session is not None else None)

    @classmethod
    def restore(cls, runtime, saved):
        result = cls(runtime, saved['index'])
        for key in ('episode', 'policy_actions', 'expected', 'metadata', 'boundaries'):
            setattr(result, key, saved[key])
        result.counts = Counter(saved['counts'])
        if saved['native'] is not None:
            if runtime.inputs is not None:
                require(result.metadata.get('training_input') is not None,
                        'blueprint.training-input.restore: missing episode provenance')
            result.runtime, _ = runtime.episode_runtime(result.index, result.episode, result.metadata.get('training_input'))
            if runtime.inputs is not None:
                require(digest(saved['native']['original']) == digest(result.runtime.original)
                        and digest(saved['native']['request']) == digest(result.runtime.request),
                        'blueprint.training-input.restore: native checkpoint belongs to a different original factory')
            result.session = result.runtime.restore(saved['native'])
            observation = json.loads(result.session.observation_json())
            if result.expected is not None:
                require(semantic(observation) == result.expected, 'restored draft/best continuity')
        return result
