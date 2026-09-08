"""Independent progression, rehearsal exposure and mixed native prefix collection."""

from copy import deepcopy
import json
import os
from pathlib import Path
import unittest

import torch

from case_curriculum import CaseCurriculum, assess_cases
from curriculum import Collector, prefixes
from native import Runtime, snapshot
from policy import Policy

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class CaseControllerTests(unittest.TestCase):
    def setUp(self):
        self.starts = {key: {i: [] for i in range(n+1)} for key, n in (("A", 6), ("B", 3))}
        self.protocol = json.loads((DIRECTORY / "case-curriculum-study.json").read_text())

    def test_independent_full_progression_root_streak_and_restore(self):
        controller = CaseCurriculum(self.starts)
        for _ in range(10):
            controller.observe({"A": 64, "B": 0}, self.protocol)
        self.assertEqual(controller.state, {"A": {"level": 6, "streak": 0}, "B": {"level": 1, "streak": 0}})
        controller = CaseCurriculum(self.starts, controller.state)
        for _ in range(8):
            controller.observe({"A": 64, "B": 64}, self.protocol)
        self.assertEqual(controller.state, {"A": {"level": 6, "streak": 2}, "B": {"level": 3, "streak": 2}})
        controller.observe({"A": 51, "B": 64}, self.protocol)
        self.assertEqual(controller.state["A"]["streak"], 0)
        self.assertEqual(controller.state["B"]["streak"], 2)
        with self.assertRaisesRegex(ValueError, "case_curriculum.state"):
            controller.observe({"A": 65, "B": 0}, self.protocol)

    def test_rehearsal_is_one_quarter_and_never_uses_future_levels(self):
        controller = CaseCurriculum(self.starts, {"A": {"level": 4, "streak": 0}, "B": {"level": 1, "streak": 0}})
        keys = ["A", "B"] * 4
        recorded = []
        for before in range(0, 24, 8):
            levels, reasons = controller.schedule(keys, before, 4)
            recorded.extend((key, level, reason) for key, level, reason in zip(keys, levels, reasons))
        a = [(n, why) for key, n, why in recorded if key == "A"]
        self.assertEqual([n for n, why in a if why == "rehearsal"], [1, 2, 3])
        self.assertEqual(sum(why == "current" for _, why in a), 9)
        self.assertTrue(all(n == 1 and why == "current" for key, n, why in recorded if key == "B"))


@unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build required")
class CaseCollectionTests(unittest.TestCase):
    def test_full_root_tables_mixed_streams_and_isolated_assessment(self):
        torch.set_num_threads(1)
        protocol = json.loads((DIRECTORY / "case-curriculum-study.json").read_text())
        protocol["assessment"].update(episodes=4, required_successes=3)
        config = json.loads((DIRECTORY / "run-server-cuda-batchnorm.json").read_text())
        config.update(device="cpu", threads=1, horizon=8)
        runtimes = {key: Runtime(ROOT, ROOT/path, config, Path(os.environ["AIC_NATIVE_LIBRARY"]))
                    for key, path in protocol["cases"].items()}
        starts = {key: prefixes(value) for key, value in runtimes.items()}
        self.assertEqual({key: max(value) for key, value in starts.items()}, {"A":6,"B":3})
        self.assertEqual(starts["A"][6], [])
        self.assertEqual(starts["B"][3], [])
        collector = Collector(runtimes, config, 8, 17, True)
        try:
            before = [snapshot(env) for env in collector.envs]
            with self.assertRaisesRegex(ValueError, "collection.levels"):
                collector.collect(None, [6, 4]*4)
            self.assertEqual([snapshot(env) for env in collector.envs], before)
            actor = Policy(collector.envs[0].observe()[0], normalization="batch")
            buffers = deepcopy(actor.state_dict())
            levels = [6,3,6,3,6,3,1,1]
            rollout, rows = collector.collect(actor, levels)
            self.assertEqual(len(rollout["actions"]), 64)
            self.assertEqual(collector.partial["levels"], levels)
            self.assertTrue(all(row["dones"] == [False]*7+[True] for row in rows))
            rng = torch.get_rng_state().clone()
            result = assess_cases(actor, runtimes, protocol, {"A":6,"B":3}, config)
            self.assertEqual(len(result["rows"]), 8)
            self.assertEqual({row["case"]:row["level"] for row in result["rows"]}, {"A":6,"B":3})
            torch.testing.assert_close(torch.get_rng_state(), rng)
            for key, value in actor.state_dict().items():
                torch.testing.assert_close(value, buffers[key])
        finally:
            collector.close()


if __name__ == "__main__":
    unittest.main()
