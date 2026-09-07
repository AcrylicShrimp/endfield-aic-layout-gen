"""Native engine setup and cold evidence exports; no Python environment rules."""

import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path


CONFIG_KEYS = ("horizon", "halo", "raster_cell_guard", "move_lengths", "segment_lengths")


def load_native(path):
    path = Path(path).resolve()
    if not path.is_file():
        raise ValueError(f"native.library_missing: {path}: build the release extension and pass --native-library")
    loader = importlib.machinery.ExtensionFileLoader("aic_pilot_native", str(path))
    spec = importlib.util.spec_from_file_location("aic_pilot_native", path, loader=loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class Runtime:
    """Execution inputs and native module identity, shared by train/evaluation orchestration."""

    def __init__(self, root, case_path, config, native_library):
        self.root = Path(root).resolve()
        self.path = Path(case_path).resolve()
        self.native_library = Path(native_library).resolve()
        self.module = load_native(self.native_library)
        if not hasattr(self.module.Engine, "build_info_json"):
            raise ValueError(f"native.incompatible_library: {self.native_library}: rebuild the current owned engine")
        self.build = json.loads(self.module.Engine.build_info_json())
        for relative, expected in self.build["files_sha256"].items():
            path = self.root / relative
            if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
                raise ValueError(f"native.stale_build: {self.native_library}: rebuild after source change in {path}")
        try:
            self.config_json = json.dumps({key: config[key] for key in CONFIG_KEYS})
        except KeyError as error:
            raise ValueError(f"native.config: missing environment field {error}") from error
        engine = self.new_engine()
        self.metadata = json.loads(engine.metadata_json())
        self.case = self.metadata["case"]
        self.paths = {key: self.root / value for key, value in self.case["catalogs"].items()}
        self.provenance = {"library": str(self.native_library),
                           "library_sha256": hashlib.sha256(self.native_library.read_bytes()).hexdigest(),
                           "build": self.build, "engine_version": self.metadata["engine_version"],
                           "snapshot_version": self.metadata["snapshot_version"],
                           "inputs": self.metadata["provenance"], "root_identity": self.metadata["root_identity"]}

    def new_engine(self):
        engine = self.module.Engine.open(str(self.root), str(self.path), self.config_json)
        previous = getattr(self, "metadata", None)
        if previous is not None:
            current = json.loads(engine.metadata_json())
            if (current["provenance"] != previous["provenance"] or current["root_identity"] != previous["root_identity"]):
                raise ValueError(f"native.inputs_changed: {self.path}: runtime inputs changed during this execution")
        return engine


def layout(engine, which):
    return json.loads(engine.export_layout_json(which))


def snapshot(engine):
    return json.loads(engine.export_state_json())


def check(engine, value):
    return json.loads(engine.check_layout_json(json.dumps(value)))


def trace(engine):
    return json.loads(engine.trace_json())


def performance(engine):
    return json.loads(engine.performance_json())


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Validate an exported layout with the owned Rust engine")
    parser.add_argument("command", choices=("check-layout",))
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--layout", type=Path, required=True)
    args = parser.parse_args()
    runtime = Runtime(args.root, args.case, json.loads(args.config.read_text()), args.native_library)
    result = json.loads(runtime.new_engine().check_layout_json(args.layout.read_text()))
    print(json.dumps({"layout": str(args.layout), **result}, indent=2))
    raise SystemExit(0 if result["valid"] else 1)


if __name__ == "__main__":
    main()
