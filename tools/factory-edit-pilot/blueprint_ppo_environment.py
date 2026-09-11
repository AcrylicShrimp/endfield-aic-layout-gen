"""Live native streams; optimizer updates never reset an unfinished draft."""
from collections import Counter
import hashlib
import json
import time

from blueprint_edit_audit import audit
from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest, require
from native import load_native
from probe_blueprint_edit import request_from_initial


def semantic(observation):
    value = {k: observation[k] for k in ('blueprint_sha256', 'revision', 'attempts', 'facilities', 'nodes',
        'edges', 'conflicts', 'best_metrics', 'initial_metrics', 'draft_factory_validated', 'config')}
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


class Runtime:
    def __init__(self, config, library):
        self.config = config
        self.spec = read(ROOT / config['probe_spec'])
        self.original = read(ROOT / self.spec['initial'])
        self.request = request_from_initial(self.original)
        self.module = load_native(library)

    def open(self, start, actions, elapsed_ms):
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
        self.runtime, self.index = runtime, index
        self.session = None
        self.episode = 0
        self.policy_actions = 0
        self.expected = None
        self.metadata = {}
        self.counts = Counter()
        self.boundaries = 0

    def observe(self):
        if self.session is None:
            starts = self.runtime.config['starts']
            start = starts[(self.index + self.episode) % len(starts)]
            training = self.runtime.config['training']
            self.session, prefix, admission = self.runtime.open(start, training['episode_actions'], training['elapsed_ms'])
            observation = json.loads(self.session.observation_json())
            self.metadata = dict(start=start['name'], prefix=prefix, admission_seconds=admission,
                                 initial_score=observation['initial_metrics']['score'], start_best=observation['best_metrics']['score'])
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
            result.session = runtime.restore(saved['native'])
            observation = json.loads(result.session.observation_json())
            if result.expected is not None:
                require(semantic(observation) == result.expected, 'restored draft/best continuity')
        return result
