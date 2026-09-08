"""Case-specific full-prefix advancement and fresh on-policy rehearsal scheduling."""

from copy import deepcopy
import json

import torch

from curriculum import prefixes
from native import check, layout, trace
from evaluation_batch import batch_size, episodes as evaluation_episodes
from native_training_comparison import policy_hash


class CaseCurriculum:
    def __init__(self, starts, state=None):
        self.maximum = {key: max(table) for key, table in starts.items()}
        self.state = deepcopy(state) if state is not None else {
            key: {"level": 1, "streak": 0} for key in starts}
        if (set(self.state) != set(starts) or any(
                not 1 <= value["level"] <= self.maximum[key] or value["streak"] < 0
                for key, value in self.state.items())):
            raise ValueError("case_curriculum.state: invalid case/level/streak state")

    def schedule(self, keys, previous_episodes, rehearsal_every):
        if rehearsal_every < 2 or previous_episodes % len(self.state):
            raise ValueError("case_curriculum.state: invalid rehearsal cadence or episode boundary")
        ordinal = {key: previous_episodes // len(self.state) for key in self.state}
        levels, reasons = [], []
        for key in keys:
            current = self.state[key]["level"]
            index = ordinal[key]
            rehearsal = current > 1 and (index + 1) % rehearsal_every == 0
            level = 1 + (index // rehearsal_every) % (current - 1) if rehearsal else current
            levels.append(level)
            reasons.append("rehearsal" if rehearsal else "current")
            ordinal[key] += 1
        return levels, reasons

    def observe(self, successes, protocol):
        spec = protocol["assessment"]
        if (set(successes) != set(self.state)
                or any(type(n) is not int or not 0 <= n <= spec["episodes"] for n in successes.values())):
            raise ValueError("case_curriculum.state: invalid per-case assessment counts")
        before = deepcopy(self.state)
        passed = {}
        for key, state in self.state.items():
            passed[key] = successes[key] >= spec["required_successes"]
            state["streak"] = min(state["streak"] + 1, spec["consecutive_passes"]) if passed[key] else 0
            if state["streak"] == spec["consecutive_passes"] and state["level"] < self.maximum[key]:
                state["level"] += 1
                state["streak"] = 0
        return {"before": before, "passed": passed, "after": deepcopy(self.state)}


@torch.no_grad()
def assess_cases(actor, runtimes, protocol, levels, config):
    spec = protocol["assessment"]
    rows, successes = [], {}
    before = policy_hash(actor.state_dict())
    for key, runtime in runtimes.items():
        prefix = prefixes(runtime)[levels[key]]
        wins = 0
        for seed, env, total in evaluation_episodes(actor, runtime, spec["episodes"], spec["horizon"],
                                                   spec["seed_start"], batch_size(config), prefix):
            accepted = check(env, layout(env, "best"))
            if not accepted["valid"] or abs(total-(env.initial_score[0]-accepted["score"][0])/env.initial_score[0]) > 1e-10:
                raise RuntimeError("case_curriculum.accounting: invalid assessment layout or reward")
            wins += total > 0
            rows.append({"case": key, "seed": seed, "level": levels[key], "return": total,
                         "score": accepted["score"], "counts": json.loads(env.episode_json())["counts"],
                         "actions": [row["command"] for row in trace(env)]})
        successes[key] = wins
    if policy_hash(actor.state_dict()) != before:
        raise RuntimeError("case_curriculum.evaluation_mutation: assessment changed policy or BN buffers")
    return {"schema_version": 2, "policy_sha256": before, "level": dict(levels), "successes": successes, "rows": rows}
