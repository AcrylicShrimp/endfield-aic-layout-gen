"""Score policy-target transforms against forced first-action continuations."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics

from hierarchical_candidate_width import ARMS, STREAMS, action_key, label
from ppo_harness_state import read, sha, write


VARIANTS = ("current", "q2", "q4", "half-visited")


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-target-contrast: {message}")


def entropy(weights):
    return -sum(value * math.log(value) for value in weights if value > 0)


def logsumexp(values):
    maximum = max(values)
    return maximum + math.log(sum(math.exp(value - maximum) for value in values))


def targets(actions):
    current = [action["policy_target"] for action in actions]
    prior = [0.5 * (action["prior"] + action["anchor_prior"]) for action in actions]
    visited = [action["visits"] > 0 for action in actions]
    require(all(value is not None and value >= 0 for value in current)
            and abs(sum(current) - 1) < 1e-6 and sum(prior) > 0
            and any(visited), "invalid recorded target or visits")
    prior_sum = sum(prior)
    prior = [value / prior_sum for value in prior]
    out = {"current": current}
    for exponent in (2, 4):
        logits = [math.log(max(p, 1e-300))
                  + exponent * (math.log(max(t, 1e-300)) - math.log(max(p, 1e-300)))
                  for p, t in zip(prior, current)]
        denominator = logsumexp(logits)
        out[f"q{exponent}"] = [math.exp(value - denominator) for value in logits]
    visited_mass = sum(value for value, was_visited in zip(current, visited)
                       if was_visited)
    out["half-visited"] = [
        0.5 * p + 0.5 * (t / visited_mass if was_visited else 0)
        for p, t, was_visited in zip(prior, current, visited)
    ]
    require(all(abs(sum(out[name]) - 1) < 1e-6
                and all(math.isfinite(value) and value >= 0 for value in out[name])
                for name in VARIANTS), "target transform is not normalized")
    return prior, visited, out


def pair_loss(log_odds):
    return math.log1p(math.exp(-abs(log_odds))) + max(0.0, -log_odds)


def conditional_budget_scores(candidate, arm, visited_mass):
    evidence = candidate["search_evidence"][arm]
    prior = 0.5 * (candidate["baseline_prior"] + candidate["baseline_anchor_prior"])
    target = evidence["policy_target"]
    require(prior > 0 and target > 0 and visited_mass > 0,
            f"invalid conditional budget score: {arm}")
    ratio = math.log(target) - math.log(prior)
    return {
        "current": math.log(target),
        "q2": math.log(prior) + 2 * ratio,
        "q4": math.log(prior) + 4 * ratio,
        "half-visited": 0.5 * prior
        + (0.5 * target / visited_mass if evidence["visits"] > 0 else 0),
    }


def analyze(root):
    width = read(root / "progress.json")
    continuation = read(root / "continuation-summary.json")
    continuation_progress = read(root / "continuation-progress.json")
    replication = read(root / "replication-summary.json")
    replication_progress = read(root / "replication-progress.json")
    require(width["status"] == "complete"
            and continuation["progress_sha256"] == sha(root / "continuation-progress.json")
            and continuation_progress["status"] == "complete"
            and replication["progress_sha256"] == sha(root / "replication-progress.json")
            and replication_progress["status"] == "complete"
            and read(root / "replication-protocol.json")["primary_summary_sha256"]
            == sha(root / "continuation-summary.json"),
            "prerequisite searches are incomplete")
    outcomes = {case["case_id"]: case for case in continuation["cases"]}
    replicated = {case["case_id"]: case for case in replication["cases"]}
    width_summary = read(root / "summary.json")
    manifest = read(root / "continuation-actions.json")
    require(width_summary["progress_sha256"] == sha(root / "progress.json")
            and manifest["candidate_width_summary_sha256"] == sha(root / "summary.json")
            and read(root / "continuation-protocol.json")["actions_sha256"]
            == sha(root / "continuation-actions.json"),
            "candidate-width or action evidence changed")
    width_cases = {case["case_id"]: case for case in width_summary["cases"]}
    action_cases = {case["case_id"]: case for case in manifest["cases"]}
    require(len(outcomes) == 48, "missing continuation cases")
    case_evidence = []
    root_metrics = {name: {"first": [], "later": []} for name in VARIANTS}
    arm_labels = [label(candidates, simulations) for candidates, simulations in ARMS]
    budget_agreement = {name: {arm: [] for arm in arm_labels if arm != "c16-n128"}
                        for name in VARIANTS}
    for stream in STREAMS:
        path = root / "raw" / stream / "c16-n128.json"
        record = next(row for row in width["records"]
                      if row["stream"] == stream and row["arm"] == "c16-n128")
        require(sha(path) == record["report_sha256"],
                f"baseline collection changed: {stream}")
        report = read(path)
        for case in report["cases"]:
            case_id = case["id"]
            require(case_id in outcomes, f"no continuation: {case_id}")
            first_targets = None
            first_actions = None
            for move, root_record in enumerate(case["mcts"]["root_records"]):
                actions = root_record["actions"]
                prior, visited, alternatives = targets(actions)
                cohort = "first" if move == 0 else "later"
                for name in VARIANTS:
                    dist = alternatives[name]
                    root_metrics[name][cohort].append({
                        "actions": len(actions), "visited": sum(visited),
                        "unvisited_mass": sum(value for value, flag in zip(dist, visited)
                                              if not flag),
                        "effective_fraction": math.exp(entropy(dist)) / len(actions),
                        "max_probability": max(dist),
                        "target_prior_kl_nats": sum(
                            target * math.log(target / max(base, 1e-300))
                            for target, base in zip(dist, prior) if target > 0),
                    })
                if move == 0:
                    first_targets, first_actions = alternatives, actions
            require(first_targets is not None, f"no first root: {case_id}")
            by_hash = {hashlib.sha256(action_key(action).encode()).hexdigest(): position
                       for position, action in enumerate(first_actions)}
            require(len(by_hash) == len(first_actions),
                    f"duplicate baseline action: {case_id}")
            variants = outcomes[case_id]["variants"]
            indices = [by_hash[variant["action_sha256"]] for variant in variants]
            action_case = action_cases[case_id]
            width_case = width_cases[case_id]
            require([row["action_sha256"] for row in action_case["candidates"]]
                    == [row["action_sha256"] for row in variants],
                    f"candidate order changed: {case_id}")
            conditional = {name: {} for name in VARIANTS}
            for arm in arm_labels:
                visited_mass = 1 - width_case["arms"][arm]["unvisited_target_mass"]
                arm_scores = [conditional_budget_scores(candidate, arm, visited_mass)
                              for candidate in action_case["candidates"]]
                for name in VARIANTS:
                    conditional[name][arm] = [row[name] for row in arm_scores]
            for name in VARIANTS:
                original_scores = conditional[name]["c16-n128"]
                for arm in budget_agreement[name]:
                    other_scores = conditional[name][arm]
                    concordant = discordant = tied = 0
                    for left in range(len(variants)):
                        for right in range(left + 1, len(variants)):
                            first = ((original_scores[left] > original_scores[right])
                                     - (original_scores[left] < original_scores[right]))
                            second = ((other_scores[left] > other_scores[right])
                                      - (other_scores[left] < other_scores[right]))
                            if first == 0 or second == 0:
                                tied += 1
                            elif first == second:
                                concordant += 1
                            else:
                                discordant += 1
                    budget_agreement[name][arm].append({
                        "concordant": concordant, "discordant": discordant,
                        "tied": tied})
            primary_clear_pairs = []
            for left in range(len(variants)):
                for right in range(left + 1, len(variants)):
                    difference = (variants[left]["completion_count"]
                                  - variants[right]["completion_count"])
                    if abs(difference) < 2:
                        continue
                    winner, loser = (left, right) if difference > 0 else (right, left)
                    primary_clear_pairs.append((winner, loser))
            replication_case = replicated.get(case_id)
            require((replication_case is not None) == bool(primary_clear_pairs),
                    f"replication case selection differs: {case_id}")
            clear_pairs = [tuple(pair) for pair in replication_case["confirmed_pairs"]] \
                if replication_case else []
            require(all(pair in primary_clear_pairs for pair in clear_pairs),
                    f"confirmed pair was not primary clear: {case_id}")
            scores = {}
            for name in VARIANTS:
                dist = first_targets[name]
                ranks = [0] * len(dist)
                for rank, position in enumerate(sorted(range(len(dist)),
                                                       key=lambda i: (-dist[i], i))):
                    ranks[position] = rank
                scores[name] = {
                    "mean_pair_loss": statistics.mean(pair_loss(
                        math.log(max(dist[indices[winner]], 1e-300))
                        - math.log(max(dist[indices[loser]], 1e-300)))
                        for winner, loser in clear_pairs) if clear_pairs else None,
                    "pair_concordance": statistics.mean(
                        dist[indices[winner]] > dist[indices[loser]]
                        for winner, loser in clear_pairs) if clear_pairs else None,
                    "tested_action_mass": sum(dist[index] for index in indices),
                    "mean_confirmed_winner_mass": statistics.mean(
                        dist[indices[winner]] for winner, _ in clear_pairs)
                        if clear_pairs else None,
                    "mean_confirmed_winner_rank_fraction": statistics.mean(
                        ranks[indices[winner]] / max(1, len(dist) - 1)
                        for winner, _ in clear_pairs) if clear_pairs else None,
                    "mean_confirmed_loser_rank_fraction": statistics.mean(
                        ranks[indices[loser]] / max(1, len(dist) - 1)
                        for _, loser in clear_pairs) if clear_pairs else None,
                }
            case_evidence.append({
                "stream": stream, "case_id": case_id,
                "primary_clear_pairs": len(primary_clear_pairs),
                "clear_pairs": len(clear_pairs),
                "clear_pairs_winner_visited": sum(
                    first_actions[indices[winner]]["visits"] > 0
                    for winner, _ in clear_pairs),
                "clear_pairs_loser_visited": sum(
                    first_actions[indices[loser]]["visits"] > 0
                    for _, loser in clear_pairs),
                "scores": scores,
            })
    require(len(case_evidence) == 48, "wrong target-analysis case count")
    summary = {}
    for name in VARIANTS:
        streams = {}
        for stream in STREAMS:
            rows = [row for row in case_evidence
                    if row["stream"] == stream and row["clear_pairs"]]
            streams[stream] = {
                "cases_with_clear_pairs": len(rows),
                "clear_pairs": sum(row["clear_pairs"] for row in rows),
                "case_mean_pair_loss": statistics.mean(
                    row["scores"][name]["mean_pair_loss"] for row in rows) if rows else None,
                "case_mean_pair_concordance": statistics.mean(
                    row["scores"][name]["pair_concordance"] for row in rows) if rows else None,
            }
        all_rows = [row for row in case_evidence if row["clear_pairs"]]
        summary[name] = {
            "streams": streams,
            "cases_with_clear_pairs": len(all_rows),
            "clear_pairs": sum(row["clear_pairs"] for row in all_rows),
            "case_mean_pair_loss": statistics.mean(
                row["scores"][name]["mean_pair_loss"] for row in all_rows)
            if all_rows else None,
            "case_mean_confirmed_winner_mass": statistics.mean(
                row["scores"][name]["mean_confirmed_winner_mass"]
                for row in all_rows) if all_rows else None,
            "case_mean_confirmed_winner_rank_fraction": statistics.mean(
                row["scores"][name]["mean_confirmed_winner_rank_fraction"]
                for row in all_rows) if all_rows else None,
            "case_mean_confirmed_loser_rank_fraction": statistics.mean(
                row["scores"][name]["mean_confirmed_loser_rank_fraction"]
                for row in all_rows) if all_rows else None,
            "first_roots": {
                field: statistics.mean(row[field] for row in root_metrics[name]["first"])
                for field in ("actions", "visited", "unvisited_mass", "effective_fraction",
                              "max_probability", "target_prior_kl_nats")
            },
            "later_roots": {
                field: statistics.mean(row[field] for row in root_metrics[name]["later"])
                for field in ("actions", "visited", "unvisited_mass", "effective_fraction",
                              "max_probability", "target_prior_kl_nats")
            },
            "later_root_count": len(root_metrics[name]["later"]),
            "budget_conditional_rank_agreement": {
                arm: {"tested_action_pairs": sum(
                          row["concordant"] + row["discordant"] + row["tied"]
                          for row in budget_agreement[name][arm]),
                      "concordant": sum(row["concordant"] for row in budget_agreement[name][arm]),
                      "discordant": sum(row["discordant"] for row in budget_agreement[name][arm]),
                      "tied": sum(row["tied"] for row in budget_agreement[name][arm])}
                for arm in budget_agreement[name]},
        }
    current = summary["current"]
    eligible = []
    if current["clear_pairs"] >= 20:
        for name in VARIANTS[1:]:
            candidate = summary[name]
            improvement = (current["case_mean_pair_loss"]
                           - candidate["case_mean_pair_loss"])
            better_streams = sum(
                current["streams"][stream]["case_mean_pair_loss"] is not None
                and candidate["streams"][stream]["case_mean_pair_loss"]
                < current["streams"][stream]["case_mean_pair_loss"]
                for stream in STREAMS)
            if improvement >= 0.05 and better_streams >= 2:
                eligible.append((candidate["case_mean_pair_loss"], name))
    choice = min(eligible, key=lambda item: (item[0], VARIANTS.index(item[1])))[1] \
        if eligible else "half-visited"
    choice_reason = "replicated-outcome-aligned" if eligible else "stress-contrast-only"
    output = {
        "schema": "aic-hierarchical-target-contrast-analysis-v1",
        "width_progress_sha256": sha(root / "progress.json"),
        "continuation_summary_sha256": sha(root / "continuation-summary.json"),
        "replication_summary_sha256": sha(root / "replication-summary.json"),
        "cases": case_evidence, "variants": summary,
        "selected_contrast": choice,
        "selection_reason": choice_reason,
        "promotion_eligible_from_this_analysis": False,
    }
    write(root / "target-contrast-analysis.json", output)
    print(json.dumps({"variants": summary, "selected_contrast": choice,
                      "selection_reason": choice_reason}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    args = parser.parse_args()
    analyze(args.output)


if __name__ == "__main__":
    main()
