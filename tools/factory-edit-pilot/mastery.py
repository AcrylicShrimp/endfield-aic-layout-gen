"""Explicit curriculum state and isolated short-horizon performance assessments."""

from dataclasses import asdict, dataclass
import json

import torch

from curriculum import prefixes
from native import check, layout


@dataclass
class Mastery:
    level: int = 1
    streak: int = 0

    def observe(self, successes, protocol):
        assessment = protocol["assessment"]
        if (set(successes) != set(protocol["cases"]) or self.level not in (0, 1, 2, 3)
                or not 0 <= self.streak < assessment["consecutive_passes"]
                or any(type(n) is not int or not 0 <= n <= assessment["episodes"] for n in successes.values())):
            raise ValueError("mastery.state: invalid level, streak or case assessment counts")
        before = asdict(self)
        passed = all(n >= assessment["required_successes"] for n in successes.values())
        if self.level:
            self.streak = self.streak + 1 if passed else 0
            if self.streak >= assessment["consecutive_passes"]:
                self.level = self.level + 1 if self.level < 3 else 0
                self.streak = 0
        return {"before": before, "passed": passed, "after": asdict(self)}


def level_at(protocol, condition, commands, controller):
    if protocol["conditions"][condition]["advancement"] == "mastery":
        return controller.level
    for stage in protocol["fixed_curriculum"]:
        if commands < stage["until"]:
            return stage["remaining"]
    return 0


@torch.no_grad()
def assess(actor, runtimes, protocol, level):
    spec = protocol["assessment"]
    rows, successes = [], {}
    for key, runtime in runtimes.items():
        prefix = prefixes(runtime)[level]
        env = runtime.new_engine()
        wins = 0
        for index in range(spec["episodes"]):
            seed = spec["seed_start"] + index
            generator = torch.Generator(device="cpu").manual_seed(seed)
            env.reset_with_prefix_json(json.dumps(prefix))
            total = 0.0
            for _ in range(spec["horizon"]):
                observation, handle = env.observe()
                action, _, _ = actor.act(observation, generator)
                reward, _, _ = env.step(handle, action)
                total += reward
            accepted = check(env, layout(env, "best"))
            if not accepted["valid"] or abs(total-(env.initial_score[0]-accepted["score"][0])/env.initial_score[0]) > 1e-10:
                raise RuntimeError("mastery.assessment_reward: inconsistent accepted score or return")
            wins += total > 0
            rows.append({"case": key, "seed": seed, "level": level, "return": total, "score": accepted["score"],
                         "counts": json.loads(env.episode_json())["counts"]})
        successes[key] = wins
    return {"level": level, "successes": successes, "rows": rows}
