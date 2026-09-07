import json
import copy
import hashlib
from pathlib import Path
import tempfile
import unittest

import torch

from client import Pool
from policy import DirectPolicy, decode, checkpoint, restore, location_mask
from train import terminal_advantages, validate, credit_signals, rollout
from update import apply_update, distribution_changes
from replay import snapshot, restore_snapshot, pack_histories, verify, file_reference


class DirectPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(2)
        cls.temp = tempfile.TemporaryDirectory(prefix="aic-direct-test-")
        cls.pool = Pool(Path(__file__).with_name("sampling.json").resolve(), Path(cls.temp.name), 1)
        cls.root = cls.pool.workers[0].call("generate", seed="streaming-train-01", ordinal=22, split="train")["root"]
        cls.obs = cls.pool.workers[0].call("reset", root=cls.root, budget=dict(steps=16, elapsed_ms=300000))

    @classmethod
    def tearDownClass(cls):
        cls.pool.close()
        cls.temp.cleanup()

    def test_no_candidate_list_and_exact_direct_replay_with_task_gradient(self):
        self.assertNotIn("actions", self.obs["state"])
        self.assertNotIn("action_mask", self.obs["state"])
        torch.manual_seed(123)
        model = DirectPolicy(16)
        action, choices, logp, _ = decode(model, self.obs)
        replay = decode(model, self.obs, decisions=choices)
        self.assertEqual(action, replay[0])
        self.assertTrue(torch.equal(logp, replay[2]))
        (-logp).backward()
        self.assertTrue(all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()))
        self.assertTrue(any(p.grad is not None and torch.count_nonzero(p.grad) for p in model.parameters()))
        rebuilt = restore(checkpoint(model))
        self.assertTrue(torch.equal(replay[2], decode(rebuilt, self.obs, decisions=choices)[2]))

    def test_location_mask_matches_exhaustive_body_collision_without_ranked_pose_filtering(self):
        state = self.obs["state"]
        for member in range(len(state["facilities"])):
            p = state["facilities"][member]
            location = location_mask(state, [member])
            mask, b = location["legal"], state["bounds"]
            for y in range(mask.shape[0]):
                for x in range(mask.shape[1]):
                    px, py = x + b["minX"], y + b["minY"]
                    blocked = any(i != member and px < q["x"] + q["width"] and q["x"] < px + p["width"]
                                  and py < q["y"] + q["height"] and q["y"] < py + p["height"] for i, q in enumerate(state["facilities"]))
                    self.assertEqual(bool(mask[y, x]), not blocked)

    def test_direct_decisions_execute_and_keep_an_audited_incumbent(self):
        worker = self.pool.workers[0]
        obs = worker.call("reset", root=self.root, budget=dict(steps=8, elapsed_ms=300000))
        generator = torch.Generator().manual_seed(193)
        for _ in range(8):
            action, _, _, _ = decode(None, obs, generator=generator, uniform=True)
            obs = worker.call("step", action=action)
            self.assertLessEqual(tuple(obs["best_score"]), tuple(obs["initial_score"]))
        self.assertTrue(obs["done"])
        self.assertTrue(worker.call("audit")["coverage"]["factory_validated"])

    def test_terminal_priority_and_configuration(self):
        self.assertEqual(terminal_advantages([[100, 99, 5], [101, 0, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 5], [100, 9, 5]]), [0, 0])
        validate(json.loads(Path(__file__).with_name("train.json").read_text()))
        validate(json.loads(Path(__file__).with_name("credit.json").read_text()))
        validate(json.loads(Path(__file__).with_name("breadth.json").read_text()))
        with self.assertRaises(ValueError):
            validate(dict(json.loads(Path(__file__).with_name("breadth.json").read_text()), training_size_schedule={"small": 1}))
        with self.assertRaises(ValueError):
            validate(dict(schema_version=1))

    def test_trace_is_observational_and_covers_conditional_stages(self):
        model = DirectPolicy(16)
        stages, kinds = set(), set()
        for seed in range(48):
            a, b = torch.Generator().manual_seed(seed), torch.Generator().manual_seed(seed)
            trace = []
            with torch.no_grad():
                plain = decode(model, self.obs, generator=a)
                traced = decode(model, self.obs, generator=b, trace=trace)
            self.assertEqual(plain[:2], traced[:2])
            self.assertTrue(torch.equal(plain[2], traced[2]) and torch.equal(plain[3], traced[3]))
            self.assertTrue(torch.equal(a.get_state(), b.get_state()))
            kinds.add(plain[0]["kind"])
            for row in trace:
                stages.add(row["stage"])
                self.assertAlmostEqual(sum(row["probabilities"]), 1, places=6)
                self.assertTrue(row["legal"][row["chosen"]])
                self.assertTrue(all(p == 0 and lp is None for p, lp, ok in zip(row["probabilities"], row["log_probabilities"], row["legal"]) if not ok))
        self.assertEqual(kinds, {"relocate", "pose", "port", "reroute", "compact"})
        self.assertEqual(stages, {"kind", "member", "rotation", "x", "y", "terminal", "port", "flag"})
        for trace in (None, []):
            model.zero_grad(set_to_none=True)
            _, _, logp, entropy = decode(model, self.obs, decisions=plain[1], trace=trace)
            (-logp - .0001 * entropy).backward()
            gradients = [None if p.grad is None else p.grad.clone() for p in model.parameters()]
            if trace is None:
                expected = gradients
            else:
                self.assertTrue(all(a is None and b is None or a is not None and b is not None and torch.equal(a, b) for a, b in zip(expected, gradients)))

    def test_capsule_reproduces_nonempty_adam_and_rejects_corruption(self):
        model = DirectPolicy(16)
        optimizer = torch.optim.Adam(model.parameters(), lr=.003)
        sources = [dict(path=str(Path(__file__).with_name(name).resolve()),
                        sha256=hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest())
                   for name in ("train.py", "policy.py", "client.py", "update.py", "replay.py")]
        worker_output = self.pool.workers[0].output
        sources += json.loads((worker_output / "implementation.json").read_text()) + json.loads((worker_output / "generation/provenance.json").read_text())
        for group in range(2):
            episodes = [rollout(self.pool, model, [self.root], 4, dict(episode_ms=300000), 900 + group * 2 + i, record=True) for i in range(2)]
            histories = [e["histories"][0] for e in episodes]
            summaries = [e["summaries"][0] for e in episodes]
            credit = credit_signals(histories, summaries, "quality-futility-prefix")
            before = snapshot(model, optimizer)
            if group:
                self.assertTrue(optimizer.state)
                self.assertTrue(any(torch.count_nonzero(s["exp_avg"]) for s in optimizer.state.values()))
            # Independent legacy arithmetic: checks the refactor, not just self-replay.
            reference, reference_optimizer = restore_snapshot(before)
            samples = sum(map(len, histories))
            reference_optimizer.zero_grad(set_to_none=True)
            for signals, history in zip(credit["signals"], histories):
                for signal, (obs, decisions, _) in zip(signals, history):
                    _, _, logp, entropy = decode(reference, obs, decisions=decisions)
                    ((-signal * logp - .0001 * entropy) / samples).backward()
            torch.nn.utils.clip_grad_norm_(reference.parameters(), 1.0)
            reference_optimizer.step()
            expected = snapshot(reference, reference_optimizer)
            update = apply_update(model, optimizer, histories, credit["signals"], .0001)
            after = snapshot(model, optimizer)
            self.assertEqual(after, expected)
            root_path = Path(self.temp.name) / f"capsule-root-{group}.json"
            with root_path.open("x") as stream:
                json.dump(self.root, stream)
            capsule = dict(schema_version=1, scope="single-on-policy-adam-update", sources=sources, root=self.root,
                           root_artifact=file_reference(root_path), audits=[e["audits"][0] for e in episodes],
                           episodes=[e["artifacts"][0] for e in episodes], rollouts=pack_histories(histories), summaries=summaries,
                           credit_mode="quality-futility-prefix", entropy_weight=.0001, credit=credit, before=before,
                           after=after, update=update, distribution_changes=distribution_changes(model, histories))
            capsule = json.loads(json.dumps(capsule, allow_nan=False))
            self.assertEqual(verify(capsule)["status"], "exact-update-reproduced")
        for field in ("behavior", "credit", "state", "checkpoint", "root", "initial", "context", "source", "precision"):
            bad = copy.deepcopy(capsule)
            if field == "behavior":
                bad["rollouts"]["histories"][0][0]["observation"]["behavior"]["log_probability"] += .1
            elif field == "credit":
                bad["credit"]["signals"][0][0] += .1
            elif field == "state":
                bad["rollouts"]["histories"][0][0]["state_ref"] = "absent"
            elif field == "checkpoint":
                bad["after"]["model"]["weights"]["head.2.bias"][0] += 1
            elif field == "root":
                bad["root"] = dict(id=bad["root"]["id"])
            elif field == "initial":
                bad["rollouts"]["histories"][0][0]["observation"]["initial_score"] = [1, 1, 1]
            elif field == "context":
                bad["rollouts"]["histories"][0][0]["observation"]["context"][0] = 0
            elif field == "source":
                bad["sources"] = [s for s in bad["sources"] if not s["path"].endswith("environment.mjs")]
            else:
                bad["before"]["runtime"]["matmul_precision"] = "changed"
            with self.assertRaises((ValueError, KeyError), msg=field):
                verify(bad)

    def test_futility_tiebreak_cannot_outvote_any_quality_field(self):
        self.assertEqual(terminal_advantages([[100, 99, 5, 999], [101, 0, 0, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 99, 999], [100, 10, 0, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 5, 999], [100, 9, 6, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 5, 1], [100, 9, 5, 2]]), [1, -1])

    def test_temporal_baseline_uses_only_pre_action_history(self):
        def entry(score, futile=0):
            return (dict(best_score=score, futile=futile), [], {})
        histories = [[entry([100, 9, 5]), entry([90, 9, 5])], [entry([100, 9, 5]), entry([100, 9, 5])]]
        summaries = [dict(best_score=dict(tuple=q), waste=dict(futile=0)) for q in ([90, 9, 5], [100, 9, 5])]
        credit = credit_signals(histories, summaries, "quality-futility-prefix")
        self.assertEqual(credit["signals"], [[1, 0], [-1, 0]])
        self.assertEqual(credit_signals(histories, summaries, "terminal-score")["signals"], [[1, 1], [-1, -1]])
        # Delayed success is credited before its later improvement; no valid-worse penalty.
        histories[0][1] = entry([100, 9, 5])
        self.assertEqual(credit_signals(histories, summaries, "quality-futility-prefix")["signals"], [[1, 1], [-1, -1]])
        with self.assertRaises(ValueError):
            credit_signals([histories[0], []], summaries, "quality-futility-prefix")


if __name__ == "__main__":
    unittest.main()
