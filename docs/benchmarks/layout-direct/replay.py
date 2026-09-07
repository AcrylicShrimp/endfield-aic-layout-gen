"""Finite JSON update capsules; offline reproduction without factory search or pickle."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

import torch

from policy import checkpoint, restore
from update import apply_update, distribution_changes, fingerprint

DTYPES = {str(t): t for t in (torch.float32, torch.float64, torch.int64, torch.uint8, torch.bool)}
CAPSULE_BYTES = 512 * 1024 * 1024


def tensor_tree(value):
    if isinstance(value, torch.Tensor):
        if value.device.type != "cpu" or str(value.dtype) not in DTYPES or not torch.isfinite(value).all():
            raise ValueError("Unsupported or nonfinite CPU optimizer tensor")
        return dict(type="tensor", dtype=str(value.dtype), shape=list(value.shape), values=value.detach().tolist())
    if isinstance(value, dict):
        return dict(type="mapping", values=[[k, tensor_tree(v)] for k, v in value.items()])
    if isinstance(value, (list, tuple)):
        return dict(type="tuple" if isinstance(value, tuple) else "list", values=[tensor_tree(v) for v in value])
    if value is None or type(value) in (str, bool, int, float):
        return value
    raise ValueError(f"Unsupported optimizer value: {type(value).__name__}")


def from_tensor_tree(value):
    if not isinstance(value, dict):
        return value
    kind = value.get("type")
    if kind == "tensor":
        result = torch.tensor(value["values"], dtype=DTYPES[value["dtype"]])
        if list(result.shape) != value["shape"] or not torch.isfinite(result).all():
            raise ValueError("Malformed optimizer tensor shape or finite values")
        return result
    if kind == "mapping":
        keys = [k for k, _ in value["values"]]
        if len(set(keys)) != len(keys):
            raise ValueError("Duplicate optimizer mapping key")
        return {k: from_tensor_tree(v) for k, v in value["values"]}
    if kind in ("list", "tuple"):
        xs = [from_tensor_tree(v) for v in value["values"]]
        return tuple(xs) if kind == "tuple" else xs
    raise ValueError("Unsupported optimizer JSON node")


def runtime():
    return dict(torch=torch.__version__, python=sys.version, threads=torch.get_num_threads(),
                interop_threads=torch.get_num_interop_threads(), dtype=str(torch.get_default_dtype()), device="cpu",
                matmul_precision=torch.get_float32_matmul_precision(), mkldnn_enabled=torch.backends.mkldnn.enabled,
                mkldnn_deterministic=torch.backends.mkldnn.deterministic,
                deterministic_algorithms=torch.are_deterministic_algorithms_enabled(),
                deterministic_warn_only=torch.is_deterministic_algorithms_warn_only_enabled())


def file_reference(path):
    return dict(path=str(Path(path).resolve()), sha256=hashlib.sha256(Path(path).read_bytes()).hexdigest())


def referenced_json(reference):
    data = Path(reference["path"]).read_bytes()
    if hashlib.sha256(data).hexdigest() != reference["sha256"]:
        raise ValueError(f"Changed evidence reference: {reference['path']}")
    return json.loads(data)


def snapshot(model, optimizer):
    return dict(schema_version=1, runtime=runtime(), model=checkpoint(model), optimizer=tensor_tree(optimizer.state_dict()),
                cpu_rng=torch.get_rng_state().tolist())


def restore_snapshot(value):
    if value.get("schema_version") != 1 or value.get("runtime") != runtime():
        raise ValueError("Update snapshot schema or execution runtime mismatch")
    model = restore(value["model"])
    optimizer = torch.optim.Adam(model.parameters())
    optimizer.load_state_dict(from_tensor_tree(value["optimizer"]))
    torch.set_rng_state(torch.tensor(value["cpu_rng"], dtype=torch.uint8))
    if snapshot(model, optimizer) != value:
        raise ValueError("Optimizer/model snapshot failed exact round trip")
    return model, optimizer


def pack_histories(histories):
    states, packed = {}, []
    for history in histories:
        rows = []
        for obs, decisions, action in history:
            state = obs["state"]
            identity = fingerprint(state)
            states.setdefault(identity, state)
            rows.append(dict(observation={k: v for k, v in obs.items() if k != "state"}, state_ref=identity,
                             decisions=decisions, action=action))
        packed.append(rows)
    return dict(states=states, histories=packed)


def unpack_histories(value):
    states = value["states"]
    if any(fingerprint(state) != identity for identity, state in states.items()):
        raise ValueError("Recorded observation state fingerprint mismatch")
    histories = []
    used = set()
    for history in value["histories"]:
        rows = []
        for row in history:
            if "state" in row["observation"] or row["state_ref"] not in states:
                raise ValueError("Missing or conflicting observation state reference")
            used.add(row["state_ref"])
            rows.append((dict(row["observation"], state=states[row["state_ref"]]), row["decisions"], row["action"]))
        histories.append(rows)
    if used != set(states):
        raise ValueError("Unreferenced observation payload in capsule")
    return histories


def save_capsule(path, value):
    data = json.dumps(value, allow_nan=False, separators=(",", ":")).encode()
    if len(data) > CAPSULE_BYTES:
        raise ValueError(f"Update capsule exceeds {CAPSULE_BYTES} byte evidence guard: {path}")
    with Path(path).open("xb") as stream:
        stream.write(data)
    return dict(path=str(Path(path).resolve()), sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))


def verify(value):
    from train import credit_signals

    if value.get("schema_version") != 1 or value.get("scope") != "single-on-policy-adam-update":
        raise ValueError("Unsupported update capsule schema/scope; old records lack behavior evidence")
    required = {str(Path(__file__).with_name(name).resolve()) for name in ("train.py", "policy.py", "client.py", "update.py", "replay.py")}
    required |= {str(Path(__file__).parent.joinpath(name).resolve()) for name in ("environment.mjs", "../layout-rl/features.mjs")}
    if not required <= {s["path"] for s in value["sources"]}:
        raise ValueError("Capsule lacks required update implementation provenance")
    for source in value["sources"]:
        if hashlib.sha256(Path(source["path"]).read_bytes()).hexdigest() != source["sha256"]:
            raise ValueError(f"Update source changed: {source['path']}")
    root = referenced_json(value["root_artifact"])
    if root != value["root"] or not root.get("request") or not root.get("witness", {}).get("placements"):
        raise ValueError("Capsule does not preserve the original complete root artifact")
    if len(value["audits"]) != len(value["episodes"]) or any(not a["coverage"]["factory_validated"] or not a["validation"]["covered_geometry_validated"] for a in value["audits"]):
        raise ValueError("Capsule lacks successful original factory audits")
    histories = unpack_histories(value["rollouts"])
    if len(histories) != len(value["summaries"]) or len(histories) != len(value["episodes"]):
        raise ValueError("Capsule episode cardinality mismatch")
    for artifact, summary, history in zip(value["episodes"], value["summaries"], histories):
        data = Path(artifact["path"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != artifact["sha256"]:
            raise ValueError("Changed source episode artifact")
        episode = json.loads(data)
        if any(episode[k] != summary[k] for k in ("root", "initial_score", "best_score", "waste", "work")):
            raise ValueError("Terminal training summary differs from source episode")
        if len(history) != len(episode["trace"]) or any(row["action"] != h[2] for row, h in zip(episode["trace"], history)):
            raise ValueError("Recorded actions differ from executed episode")
        seen = {episode["trace"][0]["parent_sha256"]} if history else set()
        best, futile = summary["initial_score"]["tuple"], 0
        for i, ((obs, _, _), transition) in enumerate(zip(history, episode["trace"])):
            initial = summary["initial_score"]["tuple"]
            current = transition["before"]
            context = [(len(history) - i) / len(history), current[0] / initial[0], current[1] / max(initial[1], 1),
                       current[2] / max(initial[2], 1), best[0] / initial[0], best[1] / max(initial[1], 1),
                       best[2] / max(initial[2], 1), (current > best) - (current < best)]
            if (obs["root"] != value["root"]["id"] or obs["state"]["state"] != transition["parent_sha256"]
                    or obs["used"] != i or obs["remaining"] != len(history) - i
                    or obs["initial_score"] != initial or obs["context"] != context
                    or obs["best_score"] != best or obs["futile"] != futile or obs["current_score"] != current):
                raise ValueError("Pre-action history does not match the executed trajectory")
            if not transition["accepted"] or transition["witness_sha256"] in seen:
                futile += 1
            if transition["accepted"]:
                seen.add(transition["witness_sha256"])
            best = transition["best"]
        if futile != summary["waste"]["futile"]:
            raise ValueError("Futility ledger does not reproduce")
    credit = credit_signals(histories, value["summaries"], value["credit_mode"])
    if credit != value["credit"]:
        raise ValueError("Credit does not reproduce from saved pre-action observations")
    model, optimizer = restore_snapshot(value["before"])
    update = apply_update(model, optimizer, histories, credit["signals"], value["entropy_weight"])
    if update != value["update"] or snapshot(model, optimizer) != value["after"]:
        raise ValueError("Saved optimizer update does not reproduce exactly")
    changes = distribution_changes(model, histories)
    if changes["stages"] != value["distribution_changes"]["stages"]:
        raise ValueError("Updated conditional distribution diagnostics do not reproduce")
    return dict(schema_version=1, status="exact-update-reproduced", scope=value["scope"],
                trajectories=len(histories), actions=sum(map(len, histories)), gradient_sha256=update["gradient_sha256"],
                after_sha256=fingerprint(value["after"]), environment_reexecuted=False, efficacy_established=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--capsule", required=True)
    parser.add_argument("--sha256", required=True, help="Expected capsule hash from the completed group report")
    options = parser.parse_args()
    try:
        filename = Path(options.capsule)
        if filename.stat().st_size > CAPSULE_BYTES:
            raise ValueError("Update capsule exceeds evidence byte guard")
        data = filename.read_bytes()
        if hashlib.sha256(data).hexdigest() != options.sha256:
            raise ValueError("Capsule SHA-256 differs from completed group reference")
        value = json.loads(data)
        torch.set_num_threads(value["before"]["runtime"]["threads"])
        torch.set_num_interop_threads(value["before"]["runtime"]["interop_threads"])
        settings = value["before"]["runtime"]
        torch.set_float32_matmul_precision(settings["matmul_precision"])
        torch.backends.mkldnn.enabled = settings["mkldnn_enabled"]
        torch.backends.mkldnn.deterministic = settings["mkldnn_deterministic"]
        torch.use_deterministic_algorithms(settings["deterministic_algorithms"], warn_only=settings["deterministic_warn_only"])
        print(json.dumps(verify(value), allow_nan=False))
    except Exception as error:
        print(json.dumps(dict(code="update-replay-failed", severity="error", stage="update-replay",
                              message=str(error), entity=options.capsule)))
        raise SystemExit(1)
