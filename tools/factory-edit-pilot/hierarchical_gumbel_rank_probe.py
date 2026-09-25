"""Compare rank-seeded and action-keyed Gumbel root candidate overlap."""

import argparse
import hashlib
import math
import statistics
import sys

from hierarchical_recovery_search import action_digest
from ppo_harness_state import read, sha, write


MASK = (1 << 64) - 1


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-gumbel-rank-probe: {message}")


def splitmix64(state):
    state = (state + 0x9E3779B97F4A7C15) & MASK
    value = state
    value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & MASK
    return state, value ^ (value >> 31)


def gumbel(bits):
    uniform = ((bits >> 11) + 0.5) / (1 << 53)
    return -math.log(-math.log(uniform))


def prefix(seed, case_id, state_identity):
    return (seed.to_bytes(8, "little") + b"\0" + case_id.encode()
            + b"\0" + state_identity.encode())


def by_rank(seed, case_id, record):
    digest = hashlib.sha256(prefix(seed, case_id, record["state_sha256"])).digest()
    state = int.from_bytes(digest[:8], "little")
    values = []
    for _ in record["actions"]:
        state, bits = splitmix64(state)
        values.append(gumbel(bits))
    return values


def by_action(seed, case_id, record):
    head = prefix(seed, case_id, record["state_sha256"]) + b"\0"
    return [gumbel(int.from_bytes(hashlib.sha256(
        head + action_digest(action).encode()).digest()[:8], "little"))
        for action in record["actions"]]


def considered(record, gumbels, candidates, anchors, anchor_mass):
    actions = record["actions"]
    anchor_indices = sorted(range(len(actions)),
                            key=lambda index: (-actions[index]["anchor_prior"], index))[
                                :min(anchors, candidates, len(actions))]
    anchor_set = set(anchor_indices)
    remaining = sorted((index for index in range(len(actions))
                        if index not in anchor_set),
                       key=lambda index: (-gumbels[index] - math.log(max(
                           actions[index]["prior"] * (1 - anchor_mass)
                           + actions[index]["anchor_prior"] * anchor_mass,
                           sys.float_info.min)), index))
    chosen = anchor_indices + remaining[:min(candidates, len(actions))
                                         - len(anchor_indices)]
    return {action_digest(actions[index]) for index in chosen}


def probe(current, candidate, config):
    controls = config["root_selection"]
    require(controls["algorithm"] == "gumbel-sequential-halving",
            "wrong root algorithm")
    require(len(current["cases"]) == len(candidate["cases"]),
            "case counts differ")
    rows = []
    for left, right in zip(current["cases"], candidate["cases"]):
        require(left["id"] == right["id"], "case order differs")
        first = left["mcts"]["root_records"][0]
        second = right["mcts"]["root_records"][0]
        require(first["state_sha256"] == second["state_sha256"],
                "first-root states differ")
        left_actions = first["actions"]
        right_actions = second["actions"]
        left_keys = [action_digest(action) for action in left_actions]
        right_keys = [action_digest(action) for action in right_actions]
        require(len(left_keys) == len(set(left_keys))
                and len(right_keys) == len(set(right_keys))
                and set(left_keys) == set(right_keys),
                "physical action sets differ")
        positions = {key: index for index, key in enumerate(right_keys)}
        shifts = [index - positions[key] for index, key in enumerate(left_keys)]
        count = len(shifts)
        rank_sets = []
        action_sets = []
        verified = []
        for case, record in ((left, first), (right, second)):
            args = (controls["candidates"], controls["anchor_candidates"],
                    controls["anchor_policy_mass"])
            rank_set = considered(record, by_rank(controls["seed"], case["id"],
                                                   record), *args)
            action_set = considered(record, by_action(controls["seed"], case["id"],
                                                       record), *args)
            visited = {action_digest(action) for action in record["actions"]
                       if action["visits"] > 0}
            rank_sets.append(rank_set)
            action_sets.append(action_set)
            verified.append(rank_set == visited)
        rows.append({
            "case_id": left["id"], "action_count": count,
            "exact_position_fraction": sum(shift == 0 for shift in shifts) / count,
            "spearman_rank_correlation": (1 - 6 * sum(shift * shift for shift in shifts)
                                          / (count * (count * count - 1))),
            "median_absolute_rank_shift": statistics.median(map(abs, shifts)),
            "first_action_equal": (left_keys[first["selected_action"]]
                                   == right_keys[second["selected_action"]]),
            "rank_candidate_overlap": len(rank_sets[0] & rank_sets[1]),
            "action_key_candidate_overlap": len(action_sets[0] & action_sets[1]),
            "rank_reconstruction_matches_visits": verified,
        })
    return {
        "schema": "aic-hierarchical-gumbel-rank-probe-v1",
        "case_count": len(rows),
        "mean_action_count": statistics.mean(row["action_count"] for row in rows),
        "mean_exact_position_fraction": statistics.mean(
            row["exact_position_fraction"] for row in rows),
        "mean_spearman_rank_correlation": statistics.mean(
            row["spearman_rank_correlation"] for row in rows),
        "mean_median_absolute_rank_shift": statistics.mean(
            row["median_absolute_rank_shift"] for row in rows),
        "equal_first_actions": sum(row["first_action_equal"] for row in rows),
        "reconstructed_roots": sum(sum(row["rank_reconstruction_matches_visits"])
                                   for row in rows),
        "mean_rank_candidate_overlap": statistics.mean(
            row["rank_candidate_overlap"] for row in rows),
        "mean_action_key_candidate_overlap": statistics.mean(
            row["action_key_candidate_overlap"] for row in rows),
        "rows": rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--current", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--search-config", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    result = probe(read(args.current), read(args.candidate),
                   read(args.search_config))
    result.update(current_sha256=sha(args.current),
                  candidate_sha256=sha(args.candidate),
                  search_config_sha256=sha(args.search_config))
    write(args.output, result)
    print(f"cases={result['case_count']} reconstructed="
          f"{result['reconstructed_roots']} "
          f"rank_overlap={result['mean_rank_candidate_overlap']} "
          f"action_key_overlap={result['mean_action_key_candidate_overlap']}")


if __name__ == "__main__":
    main()
