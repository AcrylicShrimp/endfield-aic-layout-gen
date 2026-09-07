"""Shared experiment file writing and source/runtime provenance, independent of rules."""

import hashlib
import json
import platform
from pathlib import Path
import subprocess

import numpy as np
import torch


def write(path, value):
    path = Path(path)
    temp = path.with_suffix(path.suffix+".tmp")
    temp.write_text(json.dumps(value, indent=2)+"\n")
    temp.replace(path)


def append(path, value):
    with Path(path).open("a") as stream:
        stream.write(json.dumps(value)+"\n")


def provenance(context, config_path):
    directory = Path(__file__).resolve().parent
    files = sorted(set([*directory.glob("*.py"), *directory.glob("reference/*.py"),
                        *directory.glob("*.toml"), *directory.glob("*.lock"),
                        context.path.resolve(), config_path.resolve(), *context.paths.values()]))
    native = getattr(context, "provenance", None)
    if native is not None:
        files = sorted(set(files + [context.root / path for path in native["build"]["files_sha256"]]))
    hashes, external = {}, {}
    for path in files:
        path = path.resolve()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if path.is_relative_to(context.root):
            hashes[str(path.relative_to(context.root))] = digest
        else:
            external[str(path)] = digest
    git = subprocess.run(["git", "rev-parse", "HEAD"], cwd=context.root, text=True, capture_output=True, check=True).stdout.strip()
    dirty = subprocess.run(["git", "status", "--porcelain"], cwd=context.root, text=True, capture_output=True, check=True).stdout
    if native is not None and any(hashes.get(key) != value for key, value in native["build"]["files_sha256"].items()):
        raise ValueError("native.stale_build: native source changed before provenance was recorded")
    return {"git_head": git, "git_worktree_clean": not bool(dirty), "files_sha256": hashes, "external_files_sha256": external,
            "native": native, "environment_backend": "rust" if native is not None else "python_reference",
            "python": platform.python_version(), "torch": torch.__version__, "numpy": np.__version__,
            "platform": platform.platform(), "machine": platform.machine()}
