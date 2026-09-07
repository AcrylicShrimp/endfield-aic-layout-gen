"""Fit recorded improving direct choices on freshly reset complete factory states."""

import argparse
import copy
import hashlib
import json
import math
from pathlib import Path
import time

import torch

from client import Pool
from gradient_audit import configure_runtime, vector
from policy import decode
from replay import file_reference, referenced_json, restore_snapshot, save_capsule, snapshot
from update import apply_update, behavior, fingerprint

REPORT_SHA = "f1d8099372fd8214da9341355a60766938534479247be33420c2da1ff93ada29"
KINDS = ("relocate", "pose", "port", "reroute", "compact")
CHECKPOINTS = (0, 16, 32, 64, 128)


def select_cases(candidates):
    selected = []
    for root_index in (0, 1):
        items = [c for c in candidates if c["root_index"] == root_index]
        if root_index == 0:
            queues = {k: [c for c in items if c["action"]["kind"] == k] for k in KINDS}
            chosen = []
            while len(chosen) < 12:
                previous = len(chosen)
                for kind in KINDS:
                    if queues[kind] and len(chosen) < 12:
                        chosen.append(queues[kind].pop(0))
                if len(chosen) == previous:
                    raise ValueError("Fewer than twelve distinct small positive states")
        else:
            if len(items) != 2:
                raise ValueError("Target availability differs from the declared two positive states")
            chosen = items
        for i, case in enumerate(chosen):
            held_out = i % 3 == 2 if root_index == 0 else i == 1
            selected.append(dict(case, split="held-out-state" if held_out else "train"))
    return selected


def collect(run):
    report_ref = file_reference(run / "report.json")
    if report_ref["sha256"] != REPORT_SHA:
        raise ValueError("Source report differs from the declared frozen-batch pilot")
    report = referenced_json(report_ref)
    manifest = json.loads((run / "manifest.json").read_bytes())
    for ref in manifest["sources"]:
        if file_reference(ref["path"]) != ref:
            raise ValueError(f"Changed source implementation: {ref['path']}")
    if report["status"] != "complete" or len(report["groups"]) != 16:
        raise ValueError("Choice fit requires the complete frozen pilot")
    candidates, negatives, seen = [], {}, set()
    for group_ref in report["groups"]:
        group = referenced_json(group_ref)
        capsule = referenced_json(group["behavior_capsule"])
        for worker, (history, episode_ref) in enumerate(zip(capsule["rollouts"]["histories"], capsule["episodes"])):
            episode = referenced_json(episode_ref)
            for step, (entry, row) in enumerate(zip(history, episode["trace"])):
                key = (group["root_index"], row["parent_sha256"])
                source = dict(capsule=group["behavior_capsule"], episode=episode_ref, worker=worker, step=step)
                if not row["accepted"] or row["witness_sha256"] == row["parent_sha256"]:
                    negatives.setdefault(key, dict(action=row["action"], decisions=entry["decisions"], source=source))
                if (row["accepted"] and row.get("new_incumbent") and tuple(row["score"]) < tuple(row["before"])
                        and key not in seen):
                    seen.add(key)
                    state = capsule["rollouts"]["states"][entry["state_ref"]]
                    if state["state"] != row["parent_sha256"] or fingerprint(state) != entry["state_ref"]:
                        raise ValueError("Recorded positive state identity differs from source trajectory")
                    candidates.append(dict(root_index=group["root_index"], original_root=capsule["root"], state=state,
                        action=row["action"], decisions=entry["decisions"], source=source,
                        before=row["before"], after=row["score"], after_witness_sha256=row["witness_sha256"]))
    selected = select_cases(candidates)
    for case in selected:
        case["contrast"] = negatives.get((case["root_index"], case["state"]["state"]))
    return selected, manifest, report_ref, dict(distinct_positive_states=[sum(c["root_index"] == i for c in candidates) for i in (0, 1)])


def reconstructed_root(case, index):
    root = copy.deepcopy(case["original_root"])
    s = case["state"]
    witness = dict(root["witness"])
    witness.update(placements=[{k: v for k, v in p.items() if k not in ("ports", "rotations")} for p in s["facilities"]],
                   transport_networks=s["networks"], logistics_components=s["logistics_components"], utilizations=s["utilizations"])
    root.update(id=root["id"] + f"-choice-fit-{index}", generation="explicit-recorded-complete-state-diagnostic", witness=witness)
    return root


def finish_trial(worker, action):
    after = worker.call("step", action=action)
    summary, audit, artifact = worker.call("summary"), worker.call("audit"), worker.call("save")
    if summary["work"]["proposals"] != 1 or summary["termination"] != "proposal-budget":
        raise ValueError("Choice-fit single-action trial did not complete exactly one proposal")
    if not audit["coverage"]["factory_validated"] or not audit["validation"]["covered_geometry_validated"]:
        raise ValueError("Choice-fit trial failed complete factory audit")
    return after, dict(action=action, summary=summary, audit=audit, artifact=artifact)


def nll_update(model, optimizer, examples):
    if not examples:
        raise ValueError("Teacher fit requires training examples")
    optimizer.zero_grad(set_to_none=True)
    loss_sum = 0.0
    for example in examples:
        action, _, logp, entropy = decode(model, example["observation"], decisions=example["decisions"])
        if action != example["action"]:
            raise ValueError("Teacher-forced decoder changed the recorded action")
        loss = (-1.0 * logp - 0.0 * entropy) / len(examples)
        loss.backward()
        loss_sum += float(loss.detach())
    if not all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()):
        raise ValueError("Nonfinite teacher gradient")
    digest = fingerprint({k: None if p.grad is None else p.grad.detach().tolist() for k, p in model.named_parameters()})
    norm = float(torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0))
    optimizer.step()
    if not all(torch.isfinite(p).all() for p in model.parameters()):
        raise ValueError("Nonfinite teacher parameters")
    return dict(mean_nll_before_update=loss_sum, gradient_norm=norm, gradient_sha256=digest)


def likelihoods(model, examples):
    rows = []
    with torch.no_grad():
        for example in examples:
            trace = []
            action, _, logp, _ = decode(model, example["observation"], decisions=example["decisions"], trace=trace)
            if action != example["action"]:
                raise ValueError("Likelihood probe changed the teacher action")
            greedy = all(row["chosen"] == max(range(row["vocabulary"]), key=lambda i: row["probabilities"][i]) for row in trace)
            contrast = example.get("contrast")
            negative_nll = None
            if contrast:
                other, _, lp, _ = decode(model, example["observation"], decisions=contrast["decisions"])
                if other != contrast["action"]:
                    raise ValueError("Contrast decoder changed the source action")
                negative_nll = -float(lp)
            rows.append(dict(index=example["index"], root_index=example["root_index"], split=example["split"],
                nll=-float(logp), joint_probability=math.exp(float(logp)), greedy_teacher_match=greedy,
                contrast_nll=negative_nll, trace=trace))
    return rows


def run(source, output):
    began = time.perf_counter()
    source, output = Path(source).resolve(), Path(output).resolve()
    selected, manifest, report_ref, availability = collect(source)
    original = referenced_json(manifest["capsules"][0])
    before = original["before"]
    configure_runtime(before["runtime"])
    model, optimizer = restore_snapshot(before)
    if optimizer.state:
        raise ValueError("Choice fit requires empty original Adam moments")
    output.mkdir()
    write = lambda name, value: save_capsule(output / name, value)
    sources = manifest["sources"] + [file_reference(Path(__file__)), file_reference(Path(__file__).with_name("choice-fit-contract.md"))]
    provenance = dict(schema_version=1, sources=sources, source_report=report_ref, availability=availability,
                      initial_snapshot_sha256=fingerprint(before), objective="diagnostic-teacher-forced-action-nll-not-production-rl")
    write("provenance.json", provenance)
    plan = Path(__file__).with_name("sampling.json").resolve()
    examples, curves, updates, samples = [], [], [], []
    with Pool(plan, output, 1) as pool:
        worker = pool.workers[0]
        for index, case in enumerate(selected):
            root = reconstructed_root(case, index)
            root_ref = write(f"root-{index:02}.json", root)
            obs = worker.call("reset", root=root, budget=dict(steps=1, elapsed_ms=300000))
            if obs["state"]["state"] != case["state"]["state"] or obs["current_score"] != case["before"]:
                raise ValueError(f"Fresh reset changed recorded complete geometry: {index}")
            trace = []
            with torch.no_grad():
                action, _, logp, entropy = decode(model, obs, decisions=case["decisions"], trace=trace)
            if action != case["action"]:
                raise ValueError(f"Fresh context changed teacher decoding: {index}")
            obs = dict(obs, futile=0, behavior=behavior(logp, entropy, trace))
            after, teacher = finish_trial(worker, action)
            if (not after["transition"]["accepted"] or after["state"]["state"] != case["after_witness_sha256"]
                    or after["current_score"] != case["after"]):
                raise ValueError(f"Recorded teacher did not reproduce its valid improvement: {index}")
            contrast = case["contrast"]
            if contrast:
                worker.call("reset", root=root, budget=dict(steps=1, elapsed_ms=300000))
                after_negative, trial = finish_trial(worker, contrast["action"])
                if after_negative["state"]["state"] != obs["state"]["state"]:
                    raise ValueError(f"Recorded rejected/unchanged contrast changed state: {index}")
                contrast = dict(contrast, trial=trial)
            examples.append(dict(index=index, root_index=case["root_index"], split=case["split"], root=root_ref,
                source=case["source"], observation=obs, decisions=case["decisions"], action=action,
                teacher=teacher, contrast=contrast))
        dataset_ref = write("dataset.json", dict(schema_version=1, examples=examples))
        training = [e for e in examples if e["split"] == "train"]
        if len(training) != 9 or len(examples) != 14:
            raise ValueError("Choice-fit allocation differs from nine training and five held-out states")
        initial_parameters = vector(model)
        for step in range(129):
            if step in CHECKPOINTS:
                state = snapshot(model, optimizer)
                curve = dict(step=step, examples=likelihoods(model, examples), snapshot=write(f"snapshot-{step:03}.json", state),
                             parameter_delta_norm=float((vector(model) - initial_parameters).norm()))
                curves.append(curve)
                print(json.dumps(dict(stage="choice-fit-checkpoint", step=step,
                    mean_train_nll=sum(e["nll"] for e in curve["examples"] if e["split"] == "train") / len(training),
                    train_greedy=sum(e["greedy_teacher_match"] for e in curve["examples"] if e["split"] == "train"))), flush=True)
            if step == 128:
                break
            if step == 0:
                reference, reference_optimizer = restore_snapshot(before)
                reference_update = apply_update(reference, reference_optimizer,
                    [[(e["observation"], e["decisions"], e["action"])] for e in training], [[1.0]] * len(training), 0.0)
                expected = snapshot(reference, reference_optimizer)
            update = nll_update(model, optimizer, training)
            if step == 0 and (snapshot(model, optimizer) != expected or update["gradient_sha256"] != reference_update["gradient_sha256"]):
                raise ValueError("First teacher update differs from constant-positive existing update")
            updates.append(update)
        final = snapshot(model, optimizer)
        for name, state in (("initial", before), ("fitted", final)):
            candidate, _ = restore_snapshot(state)
            for example in examples:
                root = referenced_json(example["root"])
                for sample_index in range(8):
                    action_seed = int.from_bytes(hashlib.sha256(json.dumps(["choice-fit-v1", example["index"], sample_index]).encode()).digest()[:8], "little")
                    obs = worker.call("reset", root=root, budget=dict(steps=1, elapsed_ms=300000))
                    if obs["state"] != example["observation"]["state"] or obs["context"] != example["observation"]["context"]:
                        raise ValueError("Sampled choice did not start from identical fitted observation")
                    with torch.no_grad():
                        action, _, _, _ = decode(candidate, obs, generator=torch.Generator().manual_seed(action_seed))
                    after, trial = finish_trial(worker, action)
                    samples.append(dict(model=name, index=example["index"], root_index=example["root_index"], split=example["split"],
                        sample_index=sample_index, seed=action_seed, teacher_match=action == example["action"],
                        accepted=after["transition"]["accepted"], unchanged=after["state"]["state"] == obs["state"]["state"],
                        improved=tuple(after["best_score"]) < tuple(obs["initial_score"]), **trial))
                print(json.dumps(dict(stage="choice-fit-samples-complete", model=name, index=example["index"])), flush=True)
    for ref in sources:
        if file_reference(ref["path"]) != ref:
            raise ValueError(f"Source changed during choice fit: {ref['path']}")
    train_final = [e for e in curves[-1]["examples"] if e["split"] == "train"]
    mean_nll = sum(e["nll"] for e in train_final) / len(train_final)
    greedy = sum(e["greedy_teacher_match"] for e in train_final)
    result = dict(schema_version=1, status="complete", scope="complete-state-choice-memorization-and-execution-diagnostic",
                  dataset=dataset_ref, curves=curves, updates=updates, samples=samples,
                  training_fit_gate=dict(passed=mean_nll <= 1 and greedy >= 8, mean_nll=mean_nll, greedy_matches=greedy, examples=9),
                  sampled_proposals=len(samples), teacher_proposals=len(examples), contrast_proposals=sum(bool(e["contrast"]) for e in examples),
                  elapsed_seconds=time.perf_counter() - began, production_ready=False)
    print(json.dumps(dict(status="complete", report=write("report.json", result), training_fit_gate=result["training_fit_gate"])), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--run", required=True)
    parser.add_argument("--output-directory", required=True)
    options = parser.parse_args()
    try:
        run(options.run, options.output_directory)
    except Exception as error:
        print(json.dumps(dict(code="choice-fit-failed", severity="error", stage="complete-state-choice-fit",
                              message=str(error), entity=options.run)), flush=True)
        raise SystemExit(1)
