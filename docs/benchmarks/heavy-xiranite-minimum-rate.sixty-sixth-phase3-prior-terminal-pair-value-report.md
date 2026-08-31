# Phase 3 Prior-Terminal Pair-Value Report

## Result

The two same-network `item-xiranite-powder` demand terminals each have five compatible physical
ports, producing 25 exact pair cases. Exhaustively fixing the pair removes most, but not all, of the
Phase 3 five-second cliff:

- 17 cases prove infeasible in 49--252 ms;
- 8 cases remain unknown after 5 seconds;
- no case produces an incumbent; and
- the 25-case pair wave finishes in 12,683 ms with 12 workers.

The residual eight cases have one exact structural signature: exactly one terminal selects
`input-belt-4`, while the other selects one of `input-belt-0` through `input-belt-3`. Every pair that
does not use `input-belt-4` proves infeasible quickly. Selecting the same physical port for both
terminals also proves infeasible immediately.

This is meaningful progress over the monolithic no-port case. The cliff is not an undifferentiated
four-terminal problem. The two identified port choices gate a much smaller residual family, and the
fifth physical input port is the exact boundary between fast proof and unresolved routing search.

## Complete pair matrix

Rows select the first demand terminal and columns select the second. Values are search time and
outcome.

| First / second | belt-0 | belt-1 | belt-2 | belt-3 | belt-4 |
|---|---:|---:|---:|---:|---:|
| belt-0 | 92 ms infeasible | 200 ms infeasible | 212 ms infeasible | 206 ms infeasible | 5,007 ms unknown |
| belt-1 | 209 ms infeasible | 90 ms infeasible | 247 ms infeasible | 246 ms infeasible | 5,006 ms unknown |
| belt-2 | 209 ms infeasible | 244 ms infeasible | 96 ms infeasible | 251 ms infeasible | 5,007 ms unknown |
| belt-3 | 213 ms infeasible | 252 ms infeasible | 251 ms infeasible | 98 ms infeasible | 5,008 ms unknown |
| belt-4 | 5,006 ms unknown | 5,006 ms unknown | 5,007 ms unknown | 5,007 ms unknown | 49 ms infeasible |

The 17 completed proofs have a median search time of 209 ms.

## Residual unknown search

| Pair | Decisions | Backtracks | Conflicts | Learned clauses | Propagations |
|---|---:|---:|---:|---:|---:|
| belt-0 / belt-4 | 47,323 | 3,710 | 3,709 | 3,709 | 4,205,176 |
| belt-1 / belt-4 | 43,538 | 3,742 | 3,741 | 3,741 | 4,280,237 |
| belt-2 / belt-4 | 35,486 | 3,305 | 3,304 | 3,304 | 3,447,210 |
| belt-3 / belt-4 | 35,934 | 2,749 | 2,748 | 2,748 | 3,453,054 |
| belt-4 / belt-0 | 37,866 | 2,911 | 2,910 | 2,910 | 3,392,872 |
| belt-4 / belt-1 | 35,666 | 3,110 | 3,109 | 3,109 | 3,442,092 |
| belt-4 / belt-2 | 35,549 | 3,309 | 3,308 | 3,308 | 3,452,214 |
| belt-4 / belt-3 | 36,099 | 2,761 | 2,760 | 2,760 | 3,467,867 |

Every case has the same exact model scale:

| Variables | Constraints | Incidences | Placement-routing incidences |
|---:|---:|---:|---:|
| 64,471 | 163,827 | 626,029 | 244,622 |

Model construction ranges from 452 to 959 ms under parallel contention, with a median of 947 ms.

## What this says about branching

A port is only an endpoint choice, not a route. Fixing a port pair still leaves Pumpkin responsible
for the physical path, item state, direction, flow, topology, capacity, collision, and component
decisions. Independent pair workers therefore must each solve their own routing problem.

The 17 fast cases show that the generic monolithic search can spend substantial time before making
the decisive high-level port choices. Port-first branching or an exact external portfolio is now a
supported research direction. However, the eight `input-belt-4` cases show that branching order
alone is not a complete explanation: even with both port values fixed, a deeper routing proof still
times out.

No production brancher change is justified yet. The next diagnosis should retain the eight exact
residual pairs and decompose their remaining endpoint-to-routing state. A useful first cut is the
two remaining final-target terminals, followed by route reachability or topology state only if
those terminal choices do not separate the cliff.

## Exactness

The two domains are both:

```text
input-belt-0
input-belt-1
input-belt-2
input-belt-3
input-belt-4
```

All 25 Cartesian-product cases were executed, including equal-port pairs. The portfolio fixes only
the two selected terminal values. Preceding placements and the selected introduced-facility state
remain fixed exactly as in the prior diagnosis. Every other port, routing, flow, topology, capacity,
item, occupancy, and logistics-component variable remains a solver decision.

The union of the 25 cases is exactly the selected diagnostic state's feasible set. It is not a
heuristic reduction. Because eight cases are unknown, this run does not prove the selected state
infeasible.

## Artifacts

```text
/tmp/aic-phase3-prior-terminal-pairs.zH1rcs
```

The standalone diagnostic CLI generated `summary.json`, `summary.html`, and 25 per-pair HTML files.

## Reproduction

```text
target/release/aic-prior-terminal-pair \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --used-width 16 \
  --used-height 16 \
  --facility-x 8 \
  --facility-y 5 \
  --port-assignment-index 5 \
  --facility-rotation 0 \
  --prior-facility-bit 2 \
  --terminal-pair 2,3 \
  --worker-count 12 \
  --prefix-case-time-limit-ms 10000 \
  --pair-case-time-limit-ms 5000 \
  --output-dir /tmp/aic-phase3-prior-terminal-pairs.zH1rcs
```
