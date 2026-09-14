"""Fresh native declarative episodes, persistent chunks and lightweight collection."""
from collections import Counter
from copy import copy
import json
import time

from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest, require
from blueprint_training_input import FreshInputs, validate_input, reserved_geometries
from declarative_learning_input import Catalog, prepare
from declarative_band_mask import BandMask, CODES, ROOT_MASK_SCHEMA, validate_mode
from native import load_native
import declarative_learning_curriculum as curriculum


def semantic(observation):
    # Boundary/restoration check only, never an extra per-action replay or audit.
    return digest({key:observation[key] for key in ('blueprint_sha256','revision','attempts','facilities',
        'nodes','components','port_assignments','edges','conflicts','best_metrics','initial_metrics',
        'draft_factory_validated','config','work')})


class Runtime:
    def __init__(self, config, library):
        validate_input(config)
        self.config = config
        self.reserved = reserved_geometries(config['training_input'])
        self.module = load_native(library)
        self.catalogs = read(ROOT/config['probe_spec'])['catalogs']
        self.catalog = Catalog(read(ROOT/self.catalogs['facilities']))
        validate_mode(config['actions'])
        self.mask_catalog = read(ROOT/self.catalogs['components']) if config['actions'].get('masking') else None
        self.band_mask = BandMask(self.mask_catalog) if self.mask_catalog is not None else None
        self.original = self.request = None
        self.inputs = None
        if config['training_input']['kind'] == 'fresh-random':
            if config['training_input'].get('canonical_streams') is not None:
                from declarative_batch_input import CanonicalInputs
                self.inputs=CanonicalInputs(config,library,self.module,dict(catalogs=self.catalogs,config=config['native']),self.module.DeclarativeEditSession)
            else:
                self.inputs = FreshInputs(config, library, self.module,
                    dict(catalogs=self.catalogs, config=config['native']), editor=self.module.DeclarativeEditSession)
        else:
            from probe_blueprint_edit import request_from_initial
            self.original = read(ROOT/read(ROOT/config['probe_spec'])['initial'])
            self.request = request_from_initial(self.original)

    def episode_runtime(self, index, episode, saved=None, start=None):
        result = copy(self)
        if self.mask_catalog is not None:
            result.band_mask = BandMask(self.mask_catalog)
        if self.inputs is None:
            require(saved is None, 'declarative fixture diagnostic cannot restore generated inputs')
            return result, None
        start = saved.get('start_curriculum') if saved is not None else start
        if self.config.get('start_curriculum'):
            require(start is not None and start['stream']==index and start['wave']==episode+1,
                'declarative.curriculum-plan: episode requires its assigned start')
        if start is not None and start['depth']>0:
            payload,metadata=curriculum.inherited(start,index,episode,streams=self.config['training']['streams'])
            if saved is not None:metadata['blueprint_sha256']=saved['blueprint_sha256']
        elif saved is None:
            payload, metadata = self.inputs.acquire(index,episode)
        else:
            payload, metadata = self.inputs.load(saved['reference'],index,episode)
            if start is not None:metadata=curriculum.annotate(metadata,start,[start['root']['area']])
        require(metadata['geometry_sha256'] not in self.reserved,
                'declarative.input-reservation: actual episode geometry is reserved for evaluation')
        if saved is not None:
            require(metadata == saved, 'declarative restored input provenance differs')
        result.original, result.request = payload['original'], payload['request']
        return result, metadata

    def open(self, actions, elapsed_ms):
        require(self.original is not None and self.request is not None, 'declarative episode requires actual request and start')
        began = time.perf_counter()
        native = dict(self.config['native'], max_actions=actions, elapsed_ms=elapsed_ms)
        session = self.module.DeclarativeEditSession.open(str(ROOT),json.dumps(self.catalogs),json.dumps(self.request),
            json.dumps(self.original),json.dumps(native))
        return session, time.perf_counter()-began

    def restore(self, saved):
        return self.module.DeclarativeEditSession.from_checkpoint(str(ROOT),json.dumps(self.catalogs),json.dumps(saved))

    def prepare(self, observation):
        result = prepare(observation,self.catalog,self.config['actions'])
        if self.band_mask is not None:
            result.update(root_mask_schema=ROOT_MASK_SCHEMA, root_invalid=self.band_mask(observation,result['actions']))
        return result

    def step(self, session, observation, action):
        result = json.loads(session.step_json(json.dumps(dict(blueprint_sha256=observation['blueprint_sha256'],
            revision=observation['revision'],action=action))))
        require(self.band_mask is None or not CODES.intersection(d['code'] for d in result['diagnostics']),
            'declarative.masking.rejection: selected a structurally impossible band')
        return result

    def finish(self, session):
        result = json.loads(session.finish_json())
        require(result['factory_validated'], 'declarative retained result must be native validated')
        return result


class Stream:
    def __init__(self, runtime, index):
        self.base_runtime = self.runtime = runtime
        self.index, self.episode, self.policy_actions, self.boundaries = index, 0, 0, 0
        self.session = self.expected = None
        self.next_start = curriculum.first_start(index) if runtime.config.get('start_curriculum') else None
        self.metadata, self.counts = {}, Counter()
        self.immutable_observation = None

    def observation(self):
        value=json.loads(self.session.observation_json())
        if self.immutable_observation is None:
            self.immutable_observation={k:value[k] for k in ('blueprint_sha256','blueprint','allowed_rotations','port_options','config')}
        require(value['blueprint_sha256']==self.immutable_observation['blueprint_sha256'], 'declarative immutable blueprint changed')
        value.update(self.immutable_observation)
        return value

    def observe(self):
        if self.session is None:
            began = time.perf_counter()
            self.runtime, metadata = self.base_runtime.episode_runtime(self.index,self.episode,start=self.next_start)
            access = time.perf_counter()-began
            config = self.runtime.config['training']
            self.session, admission = self.runtime.open(config['episode_actions'],config['elapsed_ms'])
            self.immutable_observation = None
            observation = self.observation()
            if metadata and self.next_start is not None:
                metadata=curriculum.annotate(metadata,self.next_start,observation['initial_metrics']['score'])
                if self.next_start['depth']>0:
                    require(observation['initial_metrics']==self.next_start['expected_metrics'] and
                        digest(self.runtime.original)==self.next_start['best_sha256'],
                        'declarative.curriculum-admission: actual start differs from validated parent best')
                    metadata['blueprint_sha256']=observation['blueprint_sha256']
            self.metadata = dict(start=('fresh-random' if not self.next_start or self.next_start['depth']==0 else
                f'inherited-depth-{self.next_start["depth"]}') if metadata else 'fixture-diagnostic',prefix=[],
                training_input=metadata,input_access_seconds=access,admission_seconds=admission,
                initial_score=observation['initial_metrics']['score'],start_best=observation['best_metrics']['score'])
            if metadata:
                require(metadata['blueprint_sha256']==observation['blueprint_sha256'], 'declarative episode blueprint/input binding')
            self.policy_actions, self.counts, self.expected = 0, Counter(), None
        observation = self.observation()
        require(not observation['done'], 'declarative training session exhausted before its action horizon')
        if self.expected is not None:
            require(semantic(observation)==self.expected, 'declarative draft changed across optimizer boundary')
            self.expected = None; self.boundaries += 1
        return observation

    def snapshot(self):
        return dict(index=self.index,episode=self.episode,policy_actions=self.policy_actions,expected=self.expected,
            metadata=self.metadata,counts=dict(self.counts),boundaries=self.boundaries,next_start=self.next_start,
            native=json.loads(self.session.checkpoint_json()) if self.session is not None else None)

    @classmethod
    def restore(cls, runtime, saved):
        result = cls(runtime,saved['index'])
        for key in ('episode','policy_actions','expected','metadata','boundaries','next_start'):
            setattr(result,key,saved[key])
        result.counts = Counter(saved['counts'])
        if saved['native'] is not None:
            require(runtime.inputs is None or result.metadata.get('training_input') is not None, 'declarative missing restored input binding')
            result.runtime, _ = runtime.episode_runtime(result.index,result.episode,result.metadata.get('training_input'))
            require(digest(saved['native']['original'])==digest(result.runtime.original)
                and digest(saved['native']['request'])==digest(result.runtime.request), 'declarative restored native input differs')
            result.session = result.runtime.restore(saved['native'])
            if result.expected is not None:
                require(semantic(json.loads(result.session.observation_json()))==result.expected, 'declarative restored draft differs')
        return result


def step_stream(stream, observation, prepared, command):
    from declarative_learning_policy import action as decode
    require(decode(prepared,command['decision'])==command['action'], 'declarative sampled/native action identity')
    outcome = stream.runtime.step(stream.session,observation,command['action'])
    stream.policy_actions += 1
    after = stream.observation()
    terminal = stream.policy_actions == stream.runtime.config['training']['episode_actions']
    require(after['done']==terminal and outcome['charged'], 'declarative native guard interrupted training horizon')
    require(not any(d['code'] in ('declarative.elapsed','declarative.budget-exhausted') for d in outcome['diagnostics']),
        'declarative training step exceeded resource allowance')
    stream.counts[f'kind/{command["action"]["kind"]}'] += 1
    stream.counts[f'status/{outcome["status"]}'] += 1
    stream.counts['area_reward'] += outcome['reward']
    stream.counts['actions_with_pending'] += any(e['status']=='pending' for e in after['edges'])
    stream.counts['actions_with_overlap'] += bool(after['conflicts'])
    witness = json.loads(stream.session.validated_draft_json()) if outcome['reward']>0 else None
    sample = dict(stream=stream.index,episode=stream.episode,start=stream.metadata['start'],policy_action=stream.policy_actions,
        input=prepared,decision=command['decision'],action=command['action'],old_logp=command['old_logp'],
        old_value=command['old_value'],entropy=command['entropy'],reward=outcome['reward'],terminal=terminal,
        observation=observation,outcome=outcome,state_changed=after['revision']!=observation['revision'],
        after_pending=sum(e['status']=='pending' for e in after['edges']),after_conflicts=len(after['conflicts']),
        validated_witness=witness)
    return sample, after


def finish_stream(stream, observation, prepared, value):
    terminal = stream.policy_actions==stream.runtime.config['training']['episode_actions']
    identity = semantic(observation)
    frontier = dict(stream=stream.index,terminal=terminal,bootstrap=0. if terminal else float(value),
        successor_input=None if terminal else prepared,successor_observation=observation,semantic=identity)
    completed = None
    if terminal:
        result = stream.runtime.finish(stream.session)
        completed = dict(stream=stream.index,episode=stream.episode,**stream.metadata,
            policy_actions=stream.policy_actions,counts=dict(stream.counts),result=result)
        stream.session = None; stream.episode += 1; stream.expected = None; stream.next_start = None
    else:
        stream.expected = identity
    return frontier, completed
