"""Cold inputs and JSON boundaries for the isolated declarative action study."""
import hashlib
import json
from pathlib import Path

from native import load_native

ROOT = Path(__file__).resolve().parents[2]


def check(ok, code, message, entity='command'):
    if not ok:
        raise ValueError(json.dumps(dict(code=f'declarative-study.{code}', severity='error',
            stage='action-experiment', message=message, entity=str(entity))))


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path = Path(path)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


def reference(path):
    path = Path(path).resolve()
    return dict(path=str(path), sha256=sha(path))


def bound(ref):
    check(sha(ref['path']) == ref['sha256'], 'source-invalid', 'Selected input changed', ref['path'])
    return read(ref['path'])


class Runtime:
    def __init__(self, spec, library, verify_build=True):
        self.spec = spec
        self.library = str(Path(library).resolve())
        self.module = load_native(self.library)
        build = json.loads(self.module.DeclarativeEditSession.build_info_json())
        check(build['profile'] == 'release', 'build-profile', 'Use a release native extension', library)
        if verify_build:
            for name, digest in build['files_sha256'].items():
                check(sha(ROOT / name) == digest, 'stale-build', 'Rebuild after source change', name)
        probe = read(ROOT / spec['probe_spec'])
        self.catalogs = probe['catalogs']
        self.build = build
        self.inputs = {str(ROOT / name): sha(ROOT / name) for name in self.catalogs.values()}
        self.inputs.update({self.library: sha(self.library)})

    def open(self, request, original, old=False, decisions=None, limits=None):
        config = dict(self.spec['native'], **(limits or {}))
        if decisions is not None:
            config['max_actions'] = decisions
        cls = self.module.DeclarativeEditSession
        if old:
            cls = self.module.BlueprintEditSession
            config['schema'] = 'blueprint-edit-v1'
            config.pop('max_total_route_expansions')
        return cls.open(str(ROOT), json.dumps(self.catalogs), json.dumps(request), json.dumps(original), json.dumps(config))

    def restore(self, saved):
        return self.module.DeclarativeEditSession.from_checkpoint(str(ROOT), json.dumps(self.catalogs), json.dumps(saved))

    @staticmethod
    def observe(session):
        return json.loads(session.observation_json())

    @staticmethod
    def step(session, observation, action):
        return json.loads(session.step_json(json.dumps(dict(blueprint_sha256=observation['blueprint_sha256'],
            revision=observation['revision'], action=action))))


def assignments(observation, instance):
    return [dict(node=n['id'], port=observation['port_assignments'][n['id']])
        for n in observation['blueprint']['nodes']
        if n['terminal'] and n['terminal']['endpoint'].get('instance') == instance]


def place(observation, instance, x, y, rotation):
    return dict(kind='place', instance=instance, x=x, y=y, rotation=rotation, ports=assignments(observation, instance))
