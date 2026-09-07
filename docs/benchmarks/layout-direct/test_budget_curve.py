import copy
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import torch

from budget_curve import (Evidence, Stream, audit_episode, compare, evaluate, require_complete,
                          seed_for, validate_config, validate_evaluation)
from policy import DirectPolicy
from replay import save_capsule, snapshot
from update import fingerprint


def episode(root="dev", score=None, seed=101, horizon=4):
    score = score or [100, 20, 5]
    return dict(seed=seed, summary=dict(root=root, initial_score=dict(tuple=[100, 20, 5]),
        best_score=dict(tuple=score), waste=dict(futile_fraction=.5), work=dict(proposals=horizon)),
        audit=dict(coverage=dict(factory_validated=True), validation=dict(covered_geometry_validated=True)),
        artifact={})


class BudgetCurveTests(unittest.TestCase):
    def test_all_workers_handshake_before_provenance_is_read(self):
        ready, reads = set(), []
        metadata = dict(dynamic_generation=True, policy_type="conditional-direct-parameters")
        workers = [SimpleNamespace(output=Path(f"worker-{i}"),
                    call=lambda op, i=i: (ready.add(i), metadata)[1]) for i in range(4)]

        def read(path):
            index = int(path.parts[0].split("-")[1])
            if index not in ready:
                raise FileNotFoundError("Worker has not finished startup")
            reads.append(str(path))
            return b"[]"

        evidence = SimpleNamespace(sources=[], write=lambda *_: None)
        with patch.object(Path, "read_bytes", read):
            Evidence.attach(evidence, SimpleNamespace(metadata=metadata, workers=workers))
        self.assertEqual(ready, set(range(4)))
        self.assertEqual(len(reads), 8)

    def test_allocation_is_not_a_tunable_pilot(self):
        config = json.loads(Path(__file__).with_name("budget-curve.json").read_bytes())
        validate_config(config)
        for key, value in (("checkpoints", [64]), ("horizon", 1024), ("seeds", [1901]),
                           ("schema_version", True), ("credit", "terminal-score"), ("learning_rate", .01)):
            bad = dict(config, **{key: value})
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_config(bad)

    def test_seed_roles_and_groups_are_distinct(self):
        values = [seed_for(role, s, i) for role in ("training", "evaluation")
                  for s in (0, 1901, 2901, 3901) for i in range(1, 1025)]
        self.assertEqual(len(values), len(set(values)))
        self.assertEqual(seed_for("evaluation", 0, 101), seed_for("evaluation", 0, 101))

    def test_evaluation_rejects_truncation_wrong_roots_missing_audits(self):
        roots = [dict(id="dev"), dict(id="target")]
        config = dict(evaluation_seeds=[101, 211], evaluation_horizon=4)
        episodes = [episode(r["id"], seed=s) for s in config["evaluation_seeds"] for r in roots]
        validate_evaluation(episodes, roots, config)
        for mutation in (lambda es: es.pop(), lambda es: es.reverse(),
                         lambda es: es[0]["summary"]["work"].update(proposals=3),
                         lambda es: es[0]["audit"]["coverage"].update(factory_validated=False)):
            changed = copy.deepcopy(episodes)
            mutation(changed)
            with self.assertRaises(ValueError):
                validate_evaluation(changed, roots, config)

    def test_score_pairing_is_lexicographic_and_target_is_separate(self):
        candidate = [episode(score=[99, 99, 99]), episode("target", [100, 19, 8])]
        baseline = [episode(), episode("target")]
        paired = compare(candidate, baseline, "target")
        self.assertEqual(paired["development"]["wins"], 1)
        self.assertEqual(paired["target"]["wins"], 1)
        self.assertEqual(paired["development"]["mean_delta"], [-1, 79, 94])
        candidate[1]["seed"] = 999
        with self.assertRaises(ValueError):
            compare(candidate, baseline, "target")

    def test_evaluation_does_not_consume_training_rng_or_optimizer_state(self):
        torch.manual_seed(7)
        model = DirectPolicy(4)
        optimizer = torch.optim.Adam(model.parameters(), lr=.003)
        before = fingerprint(snapshot(model, optimizer))
        roots = [dict(id="dev"), dict(id="target")]
        saved, calls = {}, []
        evidence = SimpleNamespace(config=dict(evaluation_seeds=[101, 211], workers=2, evaluation_horizon=4),
            write=lambda key, value: saved.update({key: value}), guard=lambda: None)

        def fake_rollout(pool, current, batch, horizon, config, seed, **kwargs):
            torch.rand(5)
            calls.append(seed)
            episodes = [episode(r["id"], horizon=horizon) for r in batch]
            return dict(summaries=[e["summary"] for e in episodes], audits=[e["audit"] for e in episodes],
                        artifacts=[e["artifact"] for e in episodes], inference_seconds_by_worker=[0.] * len(batch))

        with patch("budget_curve.rollout", side_effect=fake_rollout):
            evaluate(None, model, optimizer, roots, evidence, 0, "initial")
            evaluate(None, model, optimizer, roots, evidence, 64, "trained")
        self.assertEqual(fingerprint(snapshot(model, optimizer)), before)
        self.assertEqual(calls[:2], calls[2:])
        self.assertEqual(len(saved), 2)

    def test_evaluation_detects_accidental_weight_mutation(self):
        model = DirectPolicy(4)
        optimizer = torch.optim.Adam(model.parameters())
        evidence = SimpleNamespace(config=dict(evaluation_seeds=[101], workers=1, evaluation_horizon=4),
                                   write=lambda *_: None, guard=lambda: None)

        def corrupt(*args, **kwargs):
            with torch.no_grad():
                next(model.parameters()).add_(1)
            e = episode()
            return dict(summaries=[e["summary"]], audits=[e["audit"]], artifacts=[{}], inference_seconds_by_worker=[0.])

        with patch("budget_curve.rollout", side_effect=corrupt), self.assertRaisesRegex(ValueError, "Evaluation changed"):
            evaluate(None, model, optimizer, [dict(id="dev")], evidence, 64, "trained")

    def test_stream_records_rejections_and_never_substitutes_root(self):
        saved = []
        evidence = SimpleNamespace(config=dict(generation_attempts_per_root=2, minimum_training_facilities=2),
                                   guard=lambda: None, write=lambda *args: saved.append(args))
        rejected = dict(root=None, row=dict(status="construction-stopped"))
        worker = SimpleNamespace(call=lambda *args, **kwargs: rejected)
        stream = Stream(worker, evidence, "train", "namespace")
        with self.assertRaisesRegex(ValueError, "no fallback"):
            stream.generate()
        self.assertEqual(len(saved), 2)
        self.assertEqual(stream.ordinal, 2)

    def test_validation_family_leak_is_an_error_not_a_retry(self):
        root = dict(family="held-out", witness=dict(placements=[{}, {}]))
        worker = SimpleNamespace(call=lambda *args, **kwargs: dict(root=root, row=dict(learning_support=dict(supported=True))))
        evidence = SimpleNamespace(config=dict(generation_attempts_per_root=2, minimum_training_facilities=2), guard=lambda: None)
        with self.assertRaisesRegex(ValueError, "overlaps"):
            Stream(worker, evidence, "train", "namespace", ["held-out"]).generate()

    def test_saved_episode_reproduces_summary_and_trace_effort(self):
        e = episode()
        with tempfile.TemporaryDirectory() as directory:
            ref = save_capsule(Path(directory) / "episode.json", dict(**e["summary"], trace=[{}] * 4))
            audit_episode(ref, e["summary"])
            wrong = copy.deepcopy(e["summary"])
            wrong["best_score"]["tuple"][0] = 1
            with self.assertRaises(ValueError):
                audit_episode(ref, wrong)


if __name__ == "__main__":
    unittest.main()
