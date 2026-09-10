"""Cold runtime configuration for the retained Rust complete-factory environment."""
import hashlib
import json
from pathlib import Path

from native import load_native

ROOT = Path(__file__).resolve().parents[2]
BACKEND = 'retained-full-factory-native'


class FactoryRuntime:
    def __init__(self, library, catalogs):
        module = load_native(Path(library))
        self.build = json.loads(module.FullFactoryRuntime.build_info_json())
        for name, expected in self.build['files_sha256'].items():
            if hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != expected:
                raise ValueError(f'factory.stale-build: {name}')
        self.native = module.FullFactoryRuntime.open(str(ROOT), Path(catalogs).read_text())
        self.metadata = json.loads(self.native.metadata_json())
        self.kinds = self.metadata['kinds']

    def open_episode(self, case, horizon, elapsed, work, candidate='current-history-16', memory=True, vocabulary=None, repair=None):
        config = dict(input_contract=candidate, horizon=horizon, elapsed_ms=round(elapsed * 1000),
                      route_work=work, memory=memory)
        if vocabulary is not None:
            config['vocabulary'] = vocabulary
        if repair is not None:
            config['repair'] = repair
        return self.native.open_episode(case['request'], case['initial'], json.dumps(config))

    def close(self):
        self.native = None
