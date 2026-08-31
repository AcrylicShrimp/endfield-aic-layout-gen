# Heavy Xiranite Phase-3 Bound-Sensitivity Report

## Outcome

Increasing the request ceiling from 12 by 12 to 13 by 13 or 14 by 14 does not help the current v2
cumulative solver reach Heavy Xiranite phase 3 within the controlled 5,000 ms per-phase budget.
Both larger-bound runs stop earlier, at phase 1, without a first incumbent.

This result does not show that phase 3 is physically impossible at 12 by 12. It shows that modestly
more legal space expands the current exact candidate and connector model enough to make search
harder before the cumulative harness reaches phase 3.

## Controlled Matrix

- workload: Heavy Xiranite minimum rate;
- cumulative target: phase 3;
- formulation: `joint-shared-transport-layer-external-connectors-v2`;
- request ceilings: 12 by 12, 13 by 13, and 14 by 14;
- search budget: 5,000 ms independently for every cumulative phase;
- release build and one fresh process per target bound;
- complete legal placement, rotation, port, connector, component, and route domains;
- placement-only non-binding hints between successful cumulative phases.

The 12 by 12 case is the preceding cumulative-growth artifact. The two larger cases are new runs.
Only the caller-supplied ceiling changes.

## Result

| Bound | Last attempted phase | Facilities | First incumbent | Search | Result | Completed prior phases |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| 12 by 12 | 3 | 4 | none | 5,003 ms | unknown | 3 |
| 13 by 13 | 1 | 2 | none | 5,002 ms | unknown | 1 |
| 14 by 14 | 1 | 2 | none | 5,003 ms | unknown | 1 |

The larger cases do not contain a phase-3 solve result because the accepted cumulative harness
stops at the first phase without a complete incumbent. Continuing past a missing phase would also
remove its intended prior-solution hint and change the orchestration experiment.

Phase 0 remains tractable and proves the identical optimum `(42 area, 4 transport tiles, 0 turns)`:

| Bound | First incumbent | Complete proof | Variables | Constraints | Terms |
| --- | ---: | ---: | ---: | ---: | ---: |
| 12 by 12 | 78 ms | 1,882 ms | 15,770 | 70,522 | 201,137 |
| 13 by 13 | 59 ms | 2,737 ms | 20,769 | 93,732 | 266,318 |
| 14 by 14 | 102 ms | 3,285 ms | 26,678 | 121,322 | 343,469 |

The proof gets steadily more expensive even when the final used geometry and objective are
unchanged.

## Phase-1 Structural Comparison

The 12 by 12 phase-1 model finds its first incumbent in 1,179 ms. The two larger models find none in
5,000 ms.

| Metric | 12 by 12 | 13 by 13 | Delta vs 12 | 14 by 14 | Delta vs 12 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Grid cells | 144 | 169 | +17.4% | 196 | +36.1% |
| Placement variables | 514 | 650 | +26.5% | 802 | +56.0% |
| External-connector variables | 23,339 | 30,771 | +31.8% | 39,559 | +69.5% |
| Total variables | 32,622 | 41,777 | +28.1% | 52,438 | +60.7% |
| Constraints | 134,604 | 175,024 | +30.0% | 222,520 | +65.3% |
| Terms | 414,263 | 535,657 | +29.3% | 677,315 | +63.5% |
| Placement-routing incidences | 69,232 | 86,800 | +25.4% | 106,368 | +53.6% |
| Build time | 159 ms | 210 ms | +32.1% | 267 ms | +67.9% |
| First incumbent | 1,179 ms | none | cliff | none | cliff |

The number of physical shared route cells grows only with grid area, from 144 to 169 and 196. The
larger growth is in placement candidates, external connector state, and their cross-family
incidences. This is consistent with the current representation enumerating every legal
`(rotation, x, y)` placement and deriving connector geometry over that combined candidate domain.

## Interpretation

The experiment separates two questions:

1. **Does 12 by 12 make phase 3 physically infeasible?** Unknown. Pumpkin has not proved
   infeasibility, and the larger cumulative runs did not reach phase 3.
2. **Does merely giving the current solver more room help it find a solution?** No under the tested
   five-second budget. It makes an earlier, otherwise tractable phase lose its incumbent.

The observed phase-3 cliff should therefore not be treated primarily as evidence of insufficient
canvas size. The exact model is highly sensitive to the maximum bound because unused legal space
still creates placement and connector alternatives. Search complexity grows even though unused
space would not count toward the objective blueprint footprint.

This is a formulation/search issue, not a blueprint-footprint semantics issue. The ceiling remains
only a hard upper bound, but the current eager candidate encoding pays for most positions below that
upper bound.

## What The Evidence Rules Out

- Increasing the ceiling by one or two cells is not a practical workaround for the current solver.
- The 12 by 12 phase-3 timeout is not an infeasibility proof.
- Larger request bounds do not change the optimal phase-0 footprint or objective.
- Model construction remains sub-second; search is the immediate failure.
- The earlier failure is not caused by adding another commodity network or routing layer. The
  compared phase-1 logical graph is identical at every bound.

## Recommended Next Experiment

The next controlled comparison should be the already proposed exact coordinate-factored placement
encoding. It should test 12 by 12 and 13 by 13 phase 1 first, because this matrix provides a small,
clear boundary: the same logical graph changes from a 1,179 ms incumbent to no incumbent solely by
adding one row and one column of legal positions.

If explicit bidirectional `x`, `y`, and `rotation` channeling restores the 13 by 13 incumbent without
changing legal placements or objective semantics, repeat cumulative phases 2 and 3. If it does not,
the next diagnosis should isolate the external-connector geometry family, which grows faster than
grid area and dominates the added variables.

Do not bypass the cumulative stop rule or increase the budget in the same checkpoint. A direct
phase-3 solve without prior hints or a longer-budget run would answer a different orchestration
question and should be separately contracted if requested.

## Verification

- request JSON parsed with `jq`;
- optimized release binary;
- independent 13 by 13 and 14 by 14 processes;
- fixed 5,000 ms per-phase search budget;
- machine-readable JSON and self-contained HTML for both larger-bound failures;
- `/usr/bin/time -l` process elapsed time and peak RSS.

## Artifacts

- Experiment contract:
  `docs/designs/v2-phase3-bound-sensitivity-experiment.md`
- 12 by 12 baseline:
  `docs/benchmarks/heavy-xiranite-v2-cumulative-scc-growth/phase3.json`
- 13 by 13 and 14 by 14 JSON, HTML, and process-time records:
  `docs/benchmarks/heavy-xiranite-v2-phase3-bound-sensitivity/`

## Decision Boundary

The bound-only comparison is complete. No solver reformulation, domain reduction, heuristic, or
budget change was applied. Review this result before selecting the next experiment.
