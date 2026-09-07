"""Validate and restore an explicitly supplied completed CPU training stream."""

import hashlib
import json
from pathlib import Path
import random
import shutil

import numpy as np
import torch

from native import check


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_parent(path, runtime, config, current_provenance):
    path = Path(path).resolve()

    def require(condition, reason):
        if not condition:
            raise ValueError(f"resume.invalid: {path}: {reason}")

    parent = json.loads((path.parent/"metadata.json").read_text())
    checkpoint = torch.load(path, map_location="cpu", weights_only=False)
    require(checkpoint.get("schema_version") == parent.get("schema_version") == 2
            and checkpoint.get("checkpoint_kind") == "complete_rollout"
            and parent.get("environment_backend") == "rust",
            "resume.legacy_checkpoint: only native schema-2 completed-rollout checkpoints are supported")
    require(parent["status"] == "complete", "parent run is not complete")
    require(config["device"] == parent["device"] == "cpu", "continuation currently requires CPU")
    require(not config.get("compile_actor", False), "exact CPU continuation currently requires the eager actor")
    require(checkpoint["commands"] == parent["completed_commands"] == parent["config"]["commands"],
            "only the completed parent's final checkpoint is supported")
    require(checkpoint["optimizer_steps"] == parent["optimizer_steps"] and checkpoint["episode"] == parent["episodes"],
            "parent counters disagree")
    require(checkpoint["config"] == parent["config"], "checkpoint and parent configurations disagree")
    comparable = lambda value: {k: v for k, v in value.items() if k not in ("commands", "checkpoints")}
    require(comparable(config) == comparable(parent["config"]), "configuration changed beyond allowance/evaluation checkpoints")
    require(config["commands"] > checkpoint["commands"], "new cumulative allowance must exceed parent commands")
    require(config["checkpoints"][0] == checkpoint["commands"] and config["checkpoints"][-1] == config["commands"]
            and config["checkpoints"] == sorted(set(config["checkpoints"])), "evaluation schedule must span the continuation exactly")
    require(parent["case"] == runtime.case["id"], "case identifier differs")
    previous = parent["provenance"]
    require(checkpoint["metadata"]["provenance"] == previous, "checkpoint and parent provenance disagree")
    for key in ("python", "torch", "numpy", "platform", "machine"):
        require(previous[key] == current_provenance[key], f"runtime field changed: {key}")
    source = Path(__file__).resolve().parent
    semantic = [source/name for name in ("policy.py", "native.py", "run.py", "resume.py")]
    native_previous, native_current = previous.get("native"), current_provenance.get("native")
    require(native_previous is not None and native_current is not None, "native build provenance is missing")
    for field in ("library_sha256", "build", "engine_version", "snapshot_version", "inputs", "root_identity"):
        require(native_previous[field] == native_current[field], f"native build/input provenance changed: {field}")
    semantic += [runtime.root/name for name in native_current["build"]["files_sha256"]]
    for file in [*semantic, runtime.path.resolve(), *runtime.paths.values()]:
        key = str(file.relative_to(runtime.root)) if file.is_relative_to(runtime.root) else str(file)
        hash_field = "files_sha256" if file.is_relative_to(runtime.root) else "external_files_sha256"
        require(previous[hash_field].get(key) == current_provenance[hash_field].get(key),
                f"semantic/catalog hash changed: {key}")
    champion = json.loads((path.parent/"training-best.json").read_text())
    engine = runtime.new_engine()
    accepted = check(engine, champion)
    require(accepted["valid"] and accepted["score"] == checkpoint["champion_score"] == parent["training_champion_score"],
            "retained champion does not match the independently validated parent score")
    require(accepted["score"] == parent["verification"]["initial"]["score"] or (path.parent/"training-best-trace.json").is_file(),
            "improved champion has no retained trace")
    require(checkpoint.get("environment", {}).get("steps") == 0, "completed-rollout environment is not at an episode boundary")
    engine.restore_state_json(json.dumps(checkpoint["environment"]))
    lineage = {"checkpoint": str(path), "checkpoint_sha256": digest(path),
               "parent_metadata_sha256": digest(path.parent/"metadata.json"),
               "resumed_commands": checkpoint["commands"],
               "inherited_training_seconds": parent.get("lineage", {}).get("inherited_training_seconds", 0)+parent["training_seconds"]}
    return checkpoint, lineage


def restore(checkpoint, model, optimizer, rng, environment):
    model.load_state_dict(checkpoint["policy"])
    optimizer.load_state_dict(checkpoint["optimizer"])
    torch.set_rng_state(checkpoint["torch_rng"])
    rng.bit_generator.state = checkpoint["numpy_rng"]
    np.random.set_state(checkpoint["numpy_legacy_rng"])
    random.setstate(checkpoint["python_rng"])
    environment.restore_state_json(json.dumps(checkpoint["environment"]))
    environment.restart_measurements()


def copy_champion(parent_checkpoint, output):
    for name in ("training-best.json", "training-best-trace.json"):
        source = Path(parent_checkpoint).resolve().parent/name
        if source.exists():
            shutil.copyfile(source, output/name)
