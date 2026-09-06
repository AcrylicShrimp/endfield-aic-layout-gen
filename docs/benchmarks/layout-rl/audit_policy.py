"""Replay sampled final-checkpoint decisions on freshly reconstructed factory states."""

import argparse
import hashlib
import json
from pathlib import Path

import torch

from policy import distribution, restore


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(options):
    audit = read(options.audit)
    evaluation = read(audit["evaluation"])
    if audit["status"] != "complete" or sha(audit["observations"]["path"]) != audit["observations"]["sha256"]:
        raise ValueError("Incomplete or changed reconstruction audit")
    sources = [Path(options.audit), Path(audit["evaluation"]), Path(audit["observations"]["path"]),
               Path(__file__), Path(__file__).with_name("policy.py"), Path(__file__).parent.parent / "layout-learning" / "model.py"]
    provenance = [dict(path=str(p.resolve()), sha256=sha(p)) for p in sources]
    for item in audit["provenance"]:
        if sha(item["path"]) != item["sha256"]:
            raise ValueError(f'Changed reconstruction source {item["path"]}')
    torch.set_num_threads(evaluation["config"].get("threads", 1))
    torch.set_num_interop_threads(1)
    models = {}
    current_row, expected_step, count = None, 0, 0
    with Path(audit["observations"]["path"]).open() as stream:
        for line in stream:
            observation = json.loads(line)
            row = evaluation["rows"][observation["row"]]
            checkpoint = row["policy"]["checkpoint"]
            key = checkpoint["path"]
            if key not in models:
                if sha(key) != checkpoint["sha256"]:
                    raise ValueError(f"Changed checkpoint {key}")
                models[key] = restore(read(key))
                models[key].eval()
            if observation["row"] != current_row:
                current_row = observation["row"]
                expected_step = 0
                torch.manual_seed(row["rollout_seed"])
            if observation["step"] != expected_step:
                raise ValueError("Noncontiguous trajectory observations")
            with torch.no_grad():
                dist = distribution(models[key], [observation])
                chosen = int(dist.sample()[0] if row["policy"]["decoder"] == "sample" else dist.logits.argmax(-1)[0])
            if chosen != observation["action_index"]:
                raise ValueError(f"Policy decision mismatch in row {current_row}, step {expected_step}")
            expected_step += 1
            count += 1
    if count != audit["model_observations"]:
        raise ValueError("Missing policy observations")
    for item in provenance:
        if sha(item["path"]) != item["sha256"]:
            raise ValueError(f'Changed policy audit input/source {item["path"]}')
    result = dict(schema_version=1, status="complete", exact_replayed_decisions=count, models=len(models),
                  audit=dict(path=str(Path(options.audit).resolve()), sha256=sha(options.audit)),
                  evaluation=dict(path=audit["evaluation"], sha256=sha(audit["evaluation"])), provenance=provenance)
    with Path(options.report_output).open("x") as stream:
        json.dump(result, stream)
    print(json.dumps(result), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--audit", required=True)
    parser.add_argument("--report-output", required=True)
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="rl-policy-audit-failed", severity="error", stage="policy-audit", message=str(error), entity=None)))
        raise SystemExit(1)
