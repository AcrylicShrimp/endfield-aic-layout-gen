"""Frozen behavior-batch controls; no factory actions or trained checkpoint writes."""

import argparse
import itertools
import json
from pathlib import Path
import time

import torch

from replay import CAPSULE_BYTES, file_reference, restore_snapshot, snapshot, unpack_histories, verify
from update import apply_update, checked_decode, distribution_changes, fingerprint


def vector(model, *, gradient=False):
    return torch.cat([(torch.zeros_like(p) if p.grad is None else p.grad).detach().flatten().double()
                      if gradient else p.detach().flatten().double() for p in model.parameters()])


def cosine(a, b):
    a, b = a.double(), b.double()
    denominator = float(a.norm() * b.norm())
    return max(-1.0, min(1.0, float(torch.dot(a, b)) / denominator)) if denominator else None


def raw_gradient(model, histories, signals, entropy_weight):
    if not histories or len(histories) != len(signals) or any(len(h) != len(s) for h, s in zip(histories, signals)):
        raise ValueError("Gradient histories and credit must align")
    count = sum(map(len, histories))
    if not count:
        raise ValueError("Gradient histories must contain actions")
    model.zero_grad(set_to_none=True)
    for history, credit in zip(histories, signals):
        for (obs, decisions, action), signal in zip(history, credit):
            logp, entropy = checked_decode(model, obs, decisions, action)
            ((-signal * logp - entropy_weight * entropy) / count).backward()
    result = vector(model, gradient=True)
    if not torch.isfinite(result).all():
        raise ValueError("Nonfinite frozen gradient")
    digest = fingerprint({k: None if p.grad is None else p.grad.detach().tolist() for k, p in model.named_parameters()})
    return result, digest


def arm(before, histories, signals, entropy_weight, *, clear_moments=False):
    model, optimizer = restore_snapshot(before)
    if clear_moments:
        optimizer.state.clear()
    initial = vector(model)
    gradient, digest = raw_gradient(model, histories, signals, entropy_weight)
    update = apply_update(model, optimizer, histories, signals, entropy_weight)
    if digest != update["gradient_sha256"]:
        raise ValueError("Diagnostic raw gradient differs from actual update")
    delta = vector(model) - initial
    changes = distribution_changes(model, histories)
    changes.pop("elapsed_seconds")
    return dict(update=update, parameter_delta_norm=float(delta.norm()),
                parameter_delta_max=float(delta.abs().max()), distribution_changes=changes,
                after_sha256=fingerprint(snapshot(model, optimizer))), gradient, delta


def decomposition(combined, task, entropy):
    residual = float((combined - task - entropy).norm())
    relative = residual / max(float(combined.norm()), float(task.norm()), float(entropy.norm()), 1e-30)
    if relative > 1e-5:
        raise ValueError("Task/entropy gradient decomposition exceeds float32 tolerance")
    task_norm = float(task.norm())
    return dict(residual_norm=residual, relative_residual=relative,
                task_norm=task_norm, weighted_entropy_norm=float(entropy.norm()),
                entropy_to_task_norm=float(entropy.norm()) / task_norm if task_norm else None,
                task_entropy_cosine=cosine(task, entropy))


def analyze(value):
    histories = unpack_histories(value["rollouts"])
    if len(histories) != 4 or sum(map(len, histories)) > 512:
        raise ValueError("Frozen audit requires four trajectories and at most 512 total actions")
    verified = verify(value)
    credit = value["credit"]["signals"]
    zero = [[0.0] * len(h) for h in histories]
    terminal = [[v] * len(h) for v, h in zip(value["credit"]["terminal"], histories)]
    quality = [[v] * len(h) for v, h in zip(value["credit"]["quality_terminal"], histories)]
    controls = [
        ("recorded_combined", credit, value["entropy_weight"], False),
        ("task_only", credit, 0.0, False),
        ("entropy_only", zero, value["entropy_weight"], False),
        ("zero_signal", zero, 0.0, False),
        ("zero_signal_fresh_moments", zero, 0.0, True),
        ("terminal_same_outcomes", terminal, 0.0, False),
        ("terminal_quality_only", quality, 0.0, False),
    ]
    arms, gradients, deltas = {}, {}, {}
    for name, signals, weight, clear in controls:
        arms[name], gradients[name], deltas[name] = arm(value["before"], histories, signals, weight, clear_moments=clear)
    if (arms["recorded_combined"]["update"] != value["update"]
            or arms["recorded_combined"]["after_sha256"] != fingerprint(value["after"])):
        raise ValueError("Combined control does not reproduce saved update")
    if arms["zero_signal_fresh_moments"]["parameter_delta_norm"] != 0:
        raise ValueError("Zero-gradient fresh Adam changed parameters")
    if torch.count_nonzero(gradients["zero_signal"]):
        raise ValueError("Zero-signal gradient is not zero")
    components = decomposition(gradients["recorded_combined"], gradients["task_only"], gradients["entropy_only"])
    for name in arms:
        arms[name]["gradient_cosine_to_task"] = cosine(gradients[name], gradients["task_only"])
        arms[name]["delta_cosine_to_task_arm"] = cosine(deltas[name], deltas["task_only"])
        arms[name]["delta_distance_from_zero_signal"] = float((deltas[name] - deltas["zero_signal"]).norm())
    contributions = []
    for i in range(len(histories)):
        model, _ = restore_snapshot(value["before"])
        signals = [row if j == i else zero[j] for j, row in enumerate(credit)]
        contributions.append(raw_gradient(model, histories, signals, 0)[0])
    residual = float((sum(contributions) - gradients["task_only"]).norm())
    if residual > 1e-5 * max(sum(float(g.norm()) for g in contributions), 1e-30):
        raise ValueError("Per-trajectory contribution decomposition failed")
    permutations = []
    for assignment in itertools.permutations(range(len(histories))):
        report, gradient, delta = arm(value["before"], histories, [credit[i] for i in assignment], 0.0)
        report.update(assignment=list(assignment), gradient_cosine_to_task=cosine(gradient, gradients["task_only"]),
                      delta_cosine_to_task_arm=cosine(delta, deltas["task_only"]))
        permutations.append(report)
    if permutations[0]["update"] != arms["task_only"]["update"]:
        raise ValueError("Identity credit permutation differs from task control")
    return dict(schema_version=1, status="frozen-update-controls-passed", stage="frozen-update-gradient-audit",
                scope="single-fixed-behavior-batch-not-gradient-snr-or-efficacy", replay=verified,
                root=value["root"]["id"], facilities=len(value["root"]["witness"]["placements"]),
                before_sha256=fingerprint(value["before"]), arms=arms, decomposition=components,
                trajectory_contributions=dict(norms=[float(g.norm()) for g in contributions],
                    pairwise_cosines=[[cosine(a, b) for b in contributions] for a in contributions],
                    sum_residual_norm=residual,
                    group_norm_over_sum_norms=float(gradients["task_only"].norm()) / max(sum(float(g.norm()) for g in contributions), 1e-30)),
                credit=dict(recorded=credit, terminal=value["credit"]["terminal"],
                            quality_terminal=value["credit"]["quality_terminal"]), permutations=permutations,
                environment_actions=0, efficacy_established=False)


def configure_runtime(settings):
    torch.set_num_threads(settings["threads"])
    torch.set_num_interop_threads(settings["interop_threads"])
    torch.set_float32_matmul_precision(settings["matmul_precision"])
    torch.backends.mkldnn.enabled = settings["mkldnn_enabled"]
    torch.backends.mkldnn.deterministic = settings["mkldnn_deterministic"]
    torch.use_deterministic_algorithms(settings["deterministic_algorithms"], warn_only=settings["deterministic_warn_only"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--capsule", required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--output", required=True)
    options = parser.parse_args()
    try:
        began = time.perf_counter()
        path = Path(options.capsule)
        if Path(options.output).exists():
            raise ValueError(f"Output already exists: {options.output}")
        if path.stat().st_size > CAPSULE_BYTES:
            raise ValueError("Capsule exceeds evidence byte guard")
        reference = file_reference(path)
        if reference["sha256"] != options.sha256:
            raise ValueError("Capsule SHA-256 differs from expected group reference")
        value = json.loads(path.read_bytes())
        configure_runtime(value["before"]["runtime"])
        report = analyze(value)
        report.update(capsule=reference, sources=[file_reference(Path(__file__)),
                      file_reference(Path(__file__).with_name("gradient-audit-contract.md"))],
                      elapsed_seconds=time.perf_counter() - began)
        encoded = json.dumps(report, allow_nan=False)
        with Path(options.output).open("x") as stream:
            stream.write(encoded)
        print(json.dumps(dict(status=report["status"], output=file_reference(options.output),
                              decomposition=report["decomposition"], elapsed_seconds=report["elapsed_seconds"])))
    except Exception as error:
        print(json.dumps(dict(code="gradient-audit-failed", severity="error", stage="frozen-update-gradient-audit",
                              message=str(error), entity=options.capsule)))
        raise SystemExit(1)
