from concurrent.futures import ThreadPoolExecutor
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import torch

from train import rollout


class FakeWorker:
    def __init__(self, transitions, futile, performed):
        self.transitions = transitions
        self.futile = futile
        self.performed = performed
        self.index = 0

    def call(self, op, **kwargs):
        if op == "summary":
            return dict(waste=dict(futile=self.futile), work=dict(proposals=self.performed))
        if op in ("audit", "save"):
            return {}
        obs = dict(done=False, state=dict(state="initial"), best_score=[100, 10, 3])
        if op == "step":
            transition = self.transitions[self.index]
            self.index += 1
            obs.update(transition=transition, state=dict(state=transition.get("witness_sha256", "initial")))
        return obs


class RolloutAccountingTests(unittest.TestCase):
    def execute(self, worker, horizon, record):
        with ThreadPoolExecutor(max_workers=1) as executor, patch("train.decode", return_value=({"kind": "compact"}, [0], torch.tensor(0.), torch.tensor(0.))):
            pool = SimpleNamespace(workers=[worker], executor=executor)
            return rollout(pool, None, [dict(id="test")], horizon, dict(episode_ms=300000), 1, uniform=True, record=record)

    def test_futility_prefix_and_per_worker_inference_are_preserved(self):
        worker = FakeWorker([dict(accepted=True, witness_sha256="new"), dict(accepted=True, witness_sha256="new"), dict(accepted=False)], 2, 3)
        result = self.execute(worker, 3, True)
        self.assertEqual([h[0]["futile"] for h in result["histories"][0]], [0, 0, 1])
        self.assertEqual(result["inference_seconds"], sum(result["inference_seconds_by_worker"]))
        self.assertGreaterEqual(result["inference_seconds"], 0)

    def test_an_unperformed_proposal_is_not_an_executed_transition(self):
        worker = FakeWorker([dict(performed=False, status="episode-ended")], 0, 0)
        result = self.execute(worker, 1, False)
        self.assertEqual(result["histories"], [[]])
        with self.assertRaisesRegex(RuntimeError, "truncated"):
            self.execute(FakeWorker([dict(performed=False)], 0, 0), 1, True)

    def test_mismatched_worker_accounting_is_an_error(self):
        with self.assertRaisesRegex(RuntimeError, "accounting disagree"):
            self.execute(FakeWorker([dict(accepted=False)], 0, 1), 1, False)


if __name__ == "__main__":
    unittest.main()
