"""The single streaming Adam update, with exact behavior-policy replay checks."""

import hashlib
import json
import math
import time

import torch

from policy import decode


def fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, allow_nan=False, separators=(",", ":")).encode()).hexdigest()


def behavior(logp, entropy, trace):
    return dict(log_probability=float(logp), entropy=float(entropy), trace=trace)


def checked_decode(model, obs, decisions, original_action, timing=None):
    trace = []
    action, replay, logp, entropy = decode(model, obs, decisions=decisions, trace=trace, timing=timing)
    began = time.perf_counter()
    if action != original_action or replay != decisions:
        raise ValueError("Behavior replay changed the direct action or conditional decisions")
    actual = behavior(logp.detach(), entropy.detach(), trace)
    if actual != obs.get("behavior"):
        raise ValueError("Behavior replay changed log probability, entropy or conditional distribution")
    if timing is not None:
        timing["comparison_seconds"] = timing.get("comparison_seconds", 0.0) + time.perf_counter() - began
    return logp, entropy


def apply_update(model, optimizer, histories, signals, entropy_weight, *, timing=None):
    if len(histories) != len(signals) or not histories or any(not h or len(h) != len(s) for h, s in zip(histories, signals)):
        raise ValueError("Update requires complete aligned histories and credit")
    if any(not math.isfinite(v) for xs in signals for v in xs) or not math.isfinite(entropy_weight):
        raise ValueError("Nonfinite update credit or entropy weight")
    samples = sum(len(h) for h in histories)
    optimizer.zero_grad(set_to_none=True)
    task_loss, entropy_sum = 0.0, 0.0
    for credit, history in zip(signals, histories):
        for signal, (obs, decisions, action) in zip(credit, history):
            logp, entropy = checked_decode(model, obs, decisions, action, timing)
            # Preserve the original combined backward expression and accumulation order.
            loss = (-signal * logp - entropy_weight * entropy) / samples
            loss.backward()
            task_loss += float((-signal * logp).detach()) / samples
            entropy_sum += float(entropy.detach()) / samples
    if not all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()):
        raise ValueError("Nonfinite streaming gradient")
    began = time.perf_counter()
    gradient_sha256 = fingerprint({k: None if p.grad is None else p.grad.detach().tolist() for k, p in model.named_parameters()})
    if timing is not None:
        timing["gradient_fingerprint_seconds"] = time.perf_counter() - began
    norm = float(torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0))
    optimizer.step()
    if not all(torch.isfinite(p).all() for p in model.parameters()):
        raise ValueError("Nonfinite parameters after streaming update")
    return dict(task_loss=task_loss, entropy=entropy_sum, gradient_norm=norm,
                gradient_sha256=gradient_sha256, clipping_active=norm > 1.0)


def distribution_changes(model, histories):
    """Conditional KL on sampled old prefixes, not full trajectory KL or efficacy."""
    started = time.perf_counter()
    stages = {}
    with torch.no_grad():
        for history in histories:
            for obs, decisions, original_action in history:
                after = []
                action, _, _, _ = decode(model, obs, decisions=decisions, trace=after)
                before = obs["behavior"]["trace"]
                if action != original_action or len(before) != len(after):
                    raise ValueError("Updated policy changed fixed-prefix action decoding")
                for a, b in zip(before, after):
                    if any(a[k] != b[k] for k in ("stage", "chosen", "vocabulary", "legal")):
                        raise ValueError("Updated policy changed conditional legal support")
                    kl = sum(p * (x - y) for p, x, y, ok in zip(a["probabilities"], a["log_probabilities"], b["log_probabilities"], a["legal"]) if ok)
                    tv = sum(abs(x - y) for x, y in zip(a["probabilities"], b["probabilities"])) / 2
                    if not math.isfinite(kl) or kl < -1e-5 or not math.isfinite(tv):
                        raise ValueError("Nonfinite or inconsistent conditional divergence")
                    row = stages.setdefault(a["stage"], dict(choices=0, legal_min=sum(a["legal"]), legal_max=0,
                                                            kl_sum=0.0, tv_sum=0.0, entropy_before_sum=0.0, entropy_after_sum=0.0))
                    row["choices"] += 1
                    row["legal_min"] = min(row["legal_min"], sum(a["legal"]))
                    row["legal_max"] = max(row["legal_max"], sum(a["legal"]))
                    row["kl_sum"] += kl
                    row["tv_sum"] += tv
                    row["entropy_before_sum"] += a["entropy"]
                    row["entropy_after_sum"] += b["entropy"]
    return dict(scope="conditional-distributions-on-recorded-old-policy-prefixes", stages=stages,
                elapsed_seconds=time.perf_counter() - started)
