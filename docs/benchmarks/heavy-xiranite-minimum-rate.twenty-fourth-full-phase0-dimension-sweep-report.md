# Heavy Xiranite Full Phase-Zero Exact Dimension Sweep Report

## Outcome

The parallel exact dimension sweep moves the first-feasible cliff beyond the complete Heavy
Xiranite phase-zero slice.

With all three phase-zero commodity networks (`0,1,2`) enabled, the current free-dimension joint
model found no incumbent within 5,000 ms. The four-worker exact dimension sweep found validated
`6x7` and `7x6` witnesses, proved every legal smaller-area case infeasible, and therefore proved
that the minimum used bounding-box area is 42. The archived sweep completed in 801 ms; an isolated
repeat completed in 804 ms.

This result preserves every solver decision except the exact `(used_width, used_height)` value of
each independent case. Placement, rotation, directional port selection, external boundary
terminal selection, shared belt and pipe routing, item assignment, flow, topology, capacity,
facility collision, and transport collision all remain solver decisions. The portfolio enumerates
every legal dimension pair, so it does not exclude a legal layout or lower solution quality.

## Baby-Mode Explanation

The old solver had to answer all of these questions at once:

1. How wide is the finished factory?
2. How tall is it?
3. Where is that factory's edge?
4. Where do external connections touch that edge?
5. How do the belts and pipes reach those connections?

Those answers depend on each other. The edge depends on the routes, while the routes depend on the
edge. Pumpkin spent five seconds trying to settle that loop and never completed one witness.

The sweep keeps the exact same layout problem but asks separate questions such as "can a `6x7`
factory exist?" Once `6x7` is fixed, the edge is no longer waiting for the routes to define it. The
solver can propagate the remaining game rules strongly and either build a valid layout or prove
that size impossible.

## Controlled Comparison

Both rows use the current release binary, the same workload, the same `12x12` caller ceiling, and
all three networks. The fixed cases add only two equality constraints.

| Mode | Workers | Model variables | Constraints | Incidences | Result | Search / sweep wall | Peak RSS |
| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| Free dimensions | 1 | 19,582 | 57,843 | 186,379 | `unknown`, no incumbent | 5,000 ms | 132.32 MiB |
| Exact dimension sweep | 4 | 19,582 per case | 57,845 per case | 186,381 per case | area 42 proven | 801 ms | 487.16 MiB repeat |

The baseline did not finish, so this is not a measured 6.24x solve-time speedup claim. The truthful
statement is stronger and narrower: the free model produced no witness in five seconds, while the
portfolio produced a validated witness and a complete primary-area proof in about 0.8 seconds.

Four independent dense models increase memory use substantially. The measured repeat used about
3.68 times the peak RSS of the one-worker free model. Worker count must therefore remain an
explicit resource-policy choice.

## Exact Area Proof

The sound static lower bound is:

- minimum facility width: 5;
- minimum facility height: 5;
- facility area: 25 cells;
- at least one mandatory transport cell;
- area lower bound: 26.

The first legal dimension pairs begin at area 30 because both dimensions must be at least five.
The executed cases were:

| Dimensions | Area | Result | Search |
| --- | ---: | --- | ---: |
| `5x6` | 30 | proven infeasible | 50 ms |
| `6x5` | 30 | proven infeasible | 47 ms |
| `5x7` | 35 | proven infeasible | 58 ms |
| `7x5` | 35 | proven infeasible | 52 ms |
| `6x6` | 36 | proven infeasible | 68 ms |
| `5x8` | 40 | proven infeasible | 48 ms |
| `8x5` | 40 | proven infeasible | 49 ms |
| `6x7` | 42 | validated feasible | 183 ms |
| `7x6` | 42 | validated feasible | 119 ms |
| `5x9` | 45 | proven infeasible; already running | 72 ms |
| `9x5` | 45 | proven infeasible; already running | 52 ms |

There are no unknown candidates below area 42. Therefore the primary area proof is complete.

## Upper-Bound Propagation

The sweep generated all 63 legal dimension pairs in increasing-area order. Four workers consumed
them concurrently. When worker 2 validated `6x7`, it atomically published area 42 before sending
its completion event. Every worker observed that upper bound before taking another case.

The final schedule was:

- 11 cases executed;
- 52 cases skipped because their area was greater than 42;
- 2 validated feasible cases;
- 9 proven infeasible cases;
- 0 unknown cases;
- 0 unresolved smaller cases.

The two area-45 cases had already started and were allowed to finish. This non-cancelling race is
expected and does not affect correctness. Equal-area `7x6` also remained eligible, as required.

## Observed Layout Quality

The selected `6x7` witness has objective vector:

```text
(used area 42, transport tiles 23, route turns 2, maximum side 7, components 0)
```

The observed `7x6` witness has `(42, 24, 3, 7, 0)`. The first witness is therefore the better
observed result. This is not a proof that 23 transport tiles or two turns are optimal. Each case
currently stops at its first validated witness, so only the primary area objective is proven.

## Root Cause Result

The experiment rejects raw three-network routing size as the immediate phase-zero first-feasible
blocker. A fixed-dimension case retains essentially the same 19,582 variables and 57,845
constraints, yet solves quickly.

The measured blocker was the top-level cyclic choice among used dimensions, the derived bounding
box boundary, boundary-terminal positions, and routing geometry. Exact dimension partition breaks
that propagation cycle outside Pumpkin while preserving the union of all legal solutions.

This does not prove that routing scale is cheap in later phases. It proves only that the complete
phase-zero routing problem is tractable once exact dimensions are supplied to each solver case.

## Improvements Reached Before This Result

This result builds on the exact-formulation work already measured in earlier checkpoints:

- shared belt and pipe layer state instead of a separate dense grid per logical network;
- factored placement and port decisions instead of flattened placement-times-port candidates;
- canonical physical occupancy coupling between facility footprints and both transport layers;
- external connections represented inside the shared routing model;
- exact boundary-terminal element constraints for stronger propagation;
- exact dimension partition with sound lower bounds;
- parallel independent Pumpkin workers;
- atomic propagation of validated feasible upper bounds;
- structured unknown, infeasible, feasible, and proof-boundary reporting;
- automatic JSON and self-contained HTML artifacts for every executed case.

No deterministic placement, port, or routing heuristic was added.

## Next Research Boundary

The previous phase-zero first-feasible cliff is closed. The next first-feasible cliff has not yet
been located.

The next controlled experiment should apply the same exact dimension partition to cumulative SCC
phase 1. That experiment will answer whether the next growth step remains tractable or whether
multiple facilities and internal inter-facility routing create the next cliff.

Secondary optimization should be studied separately inside the two minimum-area partitions. It is
the next objective-quality question, not the current first-feasible blocker. Mixing it into the
phase-growth experiment would make the next cliff harder to interpret.

Stop here for user review before either experiment.

## Verification And Artifacts

- optimized release execution, all networks `0,1,2`, `12x12` ceiling;
- 5,000 ms per fixed case, four workers;
- archived run: 801 ms sweep wall;
- isolated repeat: 804 ms sweep wall, 487.16 MiB peak RSS;
- same-formulation free-dimension baseline: no incumbent in 5,000 ms;
- all emitted witnesses passed independent validation;
- machine-readable comparison: `heavy-xiranite-full-phase0-parallel-dimension-sweep/comparison.json`;
- sweep summary and interactive history: `heavy-xiranite-full-phase0-parallel-dimension-sweep/summary.json`
  and `summary.html`;
- free-dimension failure evidence: `free-dimensions-baseline.json` and
  `free-dimensions-baseline.html` in the same directory;
- per-case JSON and self-contained HTML are stored in the sweep directory.
