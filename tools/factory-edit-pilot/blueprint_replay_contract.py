"""Explicit replay-balance protocol identities and structured research failures."""
from pathlib import Path

from blueprint_ppo_store import sha

SCHEMA = 'blueprint-replay-balance-experiment-v1'
CHECKPOINT = 'blueprint-replay-balance-checkpoint-v1'
SOURCE = 'blueprint-replay-source-v1'


class Failure(ValueError):
    def __init__(self, code, stage, message, entity=None):
        self.diagnostic = dict(code=f'replay-balance.{code}', severity='error', stage=stage,
                               message=message, entity=None if entity is None else str(entity))
        super().__init__(f'{self.diagnostic["code"]}: {message}' + (f': {entity}' if entity is not None else ''))


def check(ok, code, stage, message, entity=None):
    if not ok:
        raise Failure(code, stage, message, entity)


def diagnostic(error, stage, entity=None):
    if isinstance(error, Failure):
        return error.diagnostic
    message = str(error)
    codes = ('parent-incompatible', 'source-invalid', 'stratum-empty', 'input-namespace',
        'worker-lost', 'nonfinite', 'resource-exhausted', 'evaluation-contaminated',
        'continuation-state-invalid', 'execution-gate')
    code = next((code for code in codes if f'replay-balance.{code}' in message), 'source-invalid')
    return Failure(code, stage, message, entity).diagnostic


class Sources:
    """Authenticate immutable files once during an explicit admission boundary."""
    def __init__(self):
        self.bindings = {}

    def path(self, path, expected=None):
        path = Path(path).resolve()
        key = str(path)
        if key not in self.bindings:
            check(path.is_file(), 'source-invalid', 'admission', 'required source file is missing', path)
            self.bindings[key] = sha(path)
        check(expected is None or self.bindings[key] == expected, 'source-invalid', 'admission', 'source SHA-256 mismatch', path)
        return path

    def reference(self, root, ref):
        root = Path(root).resolve()
        path = (root / ref['path']).resolve()
        check(path.is_relative_to(root), 'source-invalid', 'admission', 'source reference escapes its namespace', path)
        return self.path(path, ref['sha256'])


def classify(product, strata):
    matching = [key for key, values in strata.items() if product in values]
    check(len(matching) == 1 and matching[0] in ('large', 'small'), 'source-invalid', 'admission',
          'product has no unique registered replay stratum', product)
    return matching[0]
