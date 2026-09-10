# Routing cost and bounded directed reachability

The measured routing budget can increase from 5,000 to 50,000 without a tenfold
increase in elapsed cost. On the frozen 17-failure cohort, the original router's
summed per-case median executor time rises from 36.05 to 119.44 ms; full probe
time rises from 613.46 to 723.47 ms. The improved router at 50k takes 96.38 ms
executor / 663.82 ms full probe and validates nine candidates instead of zero at
the old 5k setting. Two recovered edits improve preceding best area.

This selected failure cohort is not a representative training latency benchmark.
Full probe time includes fresh episode admission, observation/domain generation,
execution and summary; it excludes subsequent independent candidate validation,
policy inference and PPO updates. Five measured repetitions per case/budget,
alternating budget order, follow one warmup. The reference ran during training;
later probes ran after its user-requested stop. Do not attribute every wall-time
difference to code. Expansion counts and validated outcomes are deterministic.

## Final change and contract

Before A*, a bounded necessary reachability check first detects a sealed start
and otherwise searches backward from the goal through `(cell, arrival direction)`
states. It retains endpoint directions, no immediate reversal and straight
crossing traversal. It drops the crossing-count cap in the reverse graph and
permits repeated cells. Every legal forward route is included in this relaxation.
Only complete reverse-queue exhaustion proves **that frozen path query** has no
route. A capped reverse search returns `unknown` and continues to the real A*.
A positive reverse result also requires A*, which retains actual crossing limits.

The check consumes at most 256 state expansions and one quarter of the path's
remaining allowance. Its work, including the departure check, is charged to the
same transaction budget. It returns blocker frontier cells for existing local
repair; these come from the reverse boundary rather than the previous forward
flood, so downstream blocker ordering can change. A*'s route costs, heuristic,
state labels and validated-witness acceptance otherwise remain unchanged.

Regional repair now has an explicit `attempt_work` ceiling, default 25,000. At
50k total work, the first attempt cannot consume the second half of the allowance.
Unused work remains available to existing port/order/ownership retries. This is
an operational search limit, not a game-size constraint or infeasibility proof.
Four attempts and the existing ownership/crossing limits remain unchanged.

The diagnostic contract is `factory-route-region-v2`, schema version 2. Each
attempt reports its actual `work_budget`; each route reports `preflight_expanded`
and `preflight_status`. `expanded` still counts total charged work, so callers
can reconcile preflight + A* against per-attempt and per-transaction limits.
Older saved traces remain tied to their frozen runtime. Projection and uncapped
route-quality oracle checks replace obsolete equality of complete search traces.

## Measured results

All rows use the predeclared first budget failure in each affected factory of
evaluation 440, the same action and frozen pre-edit geometry. Every candidate is
independently admitted against its original production request. See the
[census report](2026-09-10-routing-search-efficiency.md) for population coverage.

| Router | Work | Valid / 17 | Better best area | Total expansions | Executor ms | Full probe ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Original | 5k | 0 | 0 | 85,000 | 36.05 | 613.46 |
| Original | 10k | 4 | 1 | 153,954 | 54.38 | 643.53 |
| Original | 25k | 7 | 2 | 297,830 | 96.52 | 695.23 |
| Original | 50k | 9 | 2 | 377,655 | 119.44 | 723.47 |
| Improved | 5k | 0 | 0 | 75,109 | 34.07 | 579.32 |
| Improved | 10k | 4 | 1 | 140,993 | 50.87 | 605.38 |
| Improved | 25k | 7 | 2 | 249,621 | 81.04 | 643.58 |
| Improved | 50k | 9 | 2 | 310,011 | 96.38 | 663.82 |

Times are sums of per-case medians. At equal 50k allowance, expansions decrease
17.91% and measured executor time 19.31%, with the same candidate yield. Median
executor time over the 85 measured action executions is 4.15 ms; p95 is 11.63 ms.
Eight cases remain rejected under bounded repair. This is not a claim that all
routing failures have been resolved, or that a valid exploration state must
improve learning.

The original motivating facility 15/16 action now validates at **2,384 total
expansions**, within 5k, compared with the prior 9,341-expansion witness. The
unreachable chain-58 query ends after 38 charged checks instead of 8,531 A*
expansions. Attempt zero costs 791 and the existing useful retry costs 1,593.
The extra reverse checks on reachable queries are included in these totals.
The candidate remains an exploration state with area 1,394.

A tested alternative divided remaining work evenly across remaining attempts.
It reduced 25k candidate yield from seven to three and 50k yield from nine to
seven, because viable long first routes lost necessary work. It was rejected.
The retained implementation uses the measured explicit 25k per-attempt ceiling.
A 64-state preliminary reverse probe already saved work; the final 256-state
cap remains bounded and includes an immediate sealed-departure check. Cross-count
label dominance and a full reverse-distance heuristic were not required for this
slice and are not silently assumed sound under repeated-cell path semantics.

## Validation and reproduction

The reverse check was compared with an independent forward exploration of the
finite crossing-count walk graph: 128 obstacle/crossing patterns, 25 endpoint
orientation combinations, three crossing caps and five preflight caps. All
48,000 comparisons preserved every forward-reachable walk. Separate tests cover
sealed goals, capped preprocessing followed by a valid directed route, exact
transaction work accounting and preservation of work for a subsequent attempt.
This is supporting evidence for the stated relaxation proof, not exhaustive
validation of arbitrary factory requests.

Workspace checks/tests and Python PPO tests pass. The independent native path
quality oracle passes with sufficient work. Final frozen replays preserve the
original requests and validate all candidates. No global infeasibility claims,
factory-rule changes or planner fallbacks are introduced.

Run from the chosen frozen native runtime; `--native-library` selects a separately
built variant. Omit it to enforce the original matrix's outcomes and counts:

```sh
PY=tools/factory-edit-pilot/.venv/bin/python
PARENT=/home/ashrimp/Srv/aic-ppo-cnn2-12h-20260910/artifacts/factory-edit-pilot
$PY tools/factory-edit-pilot/measure_ppo_routing_cost.py \
  --run "$PARENT/cnn2-twelve-hour" \
  --sample "$PARENT/routing-census-batch-444/sample.json" \
  --matrix "$PARENT/routing-census-batch-444/work-matrix" \
  --native-library target/release/libaic_pilot_native.so \
  --repetitions 5 --output artifacts/routing-cost-replay
```

Use a fresh output directory. The `--preflight-states` flag only controls the
environment hook in the preliminary isolated prototypes; the final native
implementation has its own bounded check and needs no environment switch.
Retained manifests bind original and improved reports plus the motivating
witness. Raw action traces, timing samples and witnesses remain under ignored
`artifacts/`. The old twelve-hour run stopped normally at 526 added batches;
its checkpoint and partial batch are preserved. New training initialization,
reward configuration and launch are recorded separately.
