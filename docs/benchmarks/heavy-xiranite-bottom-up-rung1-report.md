# Heavy Xiranite Bottom-Up Rung 1 Report

## Question

Rung 0 proved that geometry-only facility placement is easy after rotations with identical occupied
rectangles are projected together. Rung 1 asks what happens when full directional rotation becomes
observable again through facility ports, while routing, flow, external terminals, and optimization
remain absent.

The experiment is feasibility-only. It cannot be slowed by a rotation preference or any other
objective because no objective is posted.

## Exact Contract

For every facility, the solver chooses:

- an `(x, y)` origin;
- one occupied-rectangle class;
- one full directional rotation consistent with that class.

For every facility-owned logical terminal, the solver chooses one direction- and
transport-compatible port. An exact support relation links full rotation, port choice, and the
rotated outside-adjacent connection offset. Guarded equalities place the connection cell relative
to the facility origin. Every selected connection cell must remain in the request grid and outside
every facility footprint.

Rung 1 does not contain external terminal coordinates, routing cells, flow, item-on-grid state,
logistics components, objectives, hints, or prior learned state.

## Search-Space Profile

The 59-facility full graph has the following independent-choice upper bound before non-overlap and
selected-port clearance:

```text
directional placement and rotation: 772.36 bits  ~= 10^232.50
compatible port choices:             400.97 bits  ~= 10^120.71
combined semantic upper bound:       1173.33 bits ~= 10^353.21
```

These are not counts of feasible layouts. They are a stable brute-force upper-bound notation for
comparing rungs. Pumpkin's declared model-domain volume is larger because reification auxiliaries
are included and must not be interpreted as independent layouts.

## Cumulative Growth

Each row is one fresh release-mode process with a `50 x 50` request ceiling and a five-second search
budget. `Terminals` counts facility-owned logical terminals, not physical transport tiles.

| Phase | Facilities | Terminals | Variables | Constraints | Outcome | Search | Decisions | Conflicts | Propagations |
|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|
| 0 | 1 | 4 | 76 | 124 | feasible | <1 ms | 14 | 0 | 335 |
| 3 | 4 | 16 | 496 | 718 | feasible | 3 ms | 342 | 33 | 9,589 |
| 10 | 11 | 33 | 2,096 | 2,806 | feasible | 30 ms | 2,159 | 209 | 128,634 |
| 15 | 16 | 43 | 3,776 | 4,956 | feasible | 79 ms | 4,491 | 410 | 335,112 |
| 20 | 21 | 61 | 8,168 | 10,685 | feasible | 314 ms | 10,810 | 974 | 1,122,173 |
| 21 | 22 | 63 | 8,744 | 11,415 | feasible | 4,710 ms | 147,366 | 11,877 | 21,826,004 |
| 22 | 23 | 67 | 9,576 | 12,472 | feasible | 605 ms | 19,856 | 2,010 | 2,233,144 |
| 23 | 24 | 71 | 10,444 | 13,574 | unknown | 5,000 ms | 162,537 | 10,110 | 23,460,407 |
| 24 | 25 | 75 | 11,348 | 14,721 | feasible | 372 ms | 15,111 | 907 | 1,438,691 |
| 25 | 26 | 79 | 12,288 | 15,913 | unknown | 5,000 ms | 174,219 | 9,404 | 21,883,357 |
| 26 | 27 | 83 | 13,264 | 17,150 | feasible | 1,858 ms | 69,132 | 3,674 | 7,843,472 |
| 27 | 28 | 87 | 14,276 | 18,432 | feasible | 170 ms | 7,147 | 217 | 754,187 |
| 31 | 36 | 115 | 24,462 | 31,493 | feasible | 1,184 ms | 43,190 | 1,132 | 4,477,113 |
| 32 | 39 | 125 | 28,354 | 36,465 | unknown | 5,001 ms | 165,513 | 5,877 | 17,071,832 |
| 40 | 59 | 197 | 62,738 | 80,135 | unknown | 5,003 ms | 121,526 | 3,639 | 12,281,188 |

The first robust five-second cliff is Phase 23. Four additional fresh runs all returned `unknown`
at five seconds. One predeclared thirty-second confirmation found a validated witness after
7,700 ms. Therefore this case is neither infeasible nor a persistent thirty-second cliff; it is a
large first-witness cost increase relative to adjacent phases.

## Non-Monotonic Search Work

Phase 23 has 24 facilities and two grinders. Phase 24 has 25 facilities and three grinders. Four
fresh Phase 24 runs all found the same search witness after 15,111 decisions and about 0.37-0.40
seconds, while every Phase 23 five-second run exhausted its budget after approximately 160,000 to
164,000 decisions.

This does not mean that adding a facility reduces the semantic search space. The independent-choice
upper bound increases by 19.51 bits, or approximately 740,000 times. It shows that generic
first-witness search work is not monotonic in model size.

Pumpkin's initial default search selects among the complete variable pool using a random selector
with a fixed seed before conflict activity becomes informative. Adding one facility changes the
variable pool and IDs, so the same fixed random sequence names different variables and creates a
different initial conflict history. The non-monotonic pattern continues: Phase 25 is unknown at five
seconds, Phase 26 is feasible in 1.859 seconds, and Phase 27 is feasible in 0.170 seconds. This is
strong evidence of search-path sensitivity, but a same-model declaration-order or explicit
branch-family comparison is still required to isolate it from stronger propagation.

## Full-Graph Model Cost

At Phase 40, selected-port clearance dominates the representation:

| Family | Variables or constraints |
|---|---:|
| All model variables | 62,738 |
| Endpoint-geometry variables | 54,003 |
| Endpoint-clearance inequalities | 50,336 |
| Endpoint-clearance disjunctions | 12,584 |
| All model constraints | 80,135 |

Rung 0 used 8,479 variables and 10,565 constraints and found a full-graph witness in 26 ms. Rung 1
adds 54,259 variables and 69,570 constraints. Most of that delta comes from reifying four possible
point-versus-rectangle separations for every selected connection cell against every non-owner
facility geometry class.

The endpoint-support propagator is active rather than inert. In the full five-second run it executed
213,559 times, removed 363,715 values across backtracking search, and accounted for 6,226,905
support checks. This is evidence of substantial port/rotation interaction, but it does not prove
that endpoint support is the primary cliff. Selected-port clearance is the largest static cost and
must be isolated separately.

## Current Conclusion

The experiment disproves the simple claim that reintroducing rotation merely multiplies search by
four. Directional rotation becomes a shared parent decision for every terminal on a facility, and
each terminal's physical connection must remain clear of every other facility. The added semantic
block creates a dense rotation-port-coordinate-clearance coupling.

It is not yet valid to say that rotation itself is the blocker because Rung 1 introduced full
rotation, port choice, connection geometry, and selected-port clearance together. The next exact
diagnostic must decompose this new block without fixing placement, rotation, or port decisions. It
should distinguish:

1. cost inside the exact `(rotation, port, local connection)` support relation;
2. cost of connection-coordinate channeling;
3. cost of selected connection cell versus every other facility footprint;
4. default branch-order sensitivity exposed by the Phase 23/24 reversal.

The smallest semantic split is:

- Rung 1A: full rotation, compatible port choice, local connection, and in-grid connection
  coordinates;
- Rung 1B: Rung 1A plus selected connection cell versus every other facility footprint.

Both are exact partial-rung formulations. If 1A is fast and 1B is slow, selected-port clearance is
the first semantic cliff. If 1A is already slow, the rotation/port support boundary must be split
further. Separately, recording branch decisions by variable family and comparing an exact
primary-decision-first brancher will test whether derived clearance Booleans are being searched as
if they were independent semantic choices.

## Evidence Status

This is an exploratory growth report, not the final aggregate `FormulationLadderReport` classified
by every gate in the ladder design. The artifacts now include workload identity, workload-manifest
hash, introduced facility IDs, stable facility-terminal IDs, port-domain histogram, explicit
termination reason, witness count, model-family metrics, and search counters. Root-domain snapshots,
branch-decision family traces, normalized model fingerprints, and adjacent-rung structural deltas
remain next-harness work. The report therefore identifies a measured target but does not yet claim a
fully classified causal cliff.

## Artifacts

- `heavy-xiranite-bottom-up-rung1-growth/`: all cumulative phases, each with JSON and self-contained
  HTML evidence.
- `heavy-xiranite-bottom-up-rung1-repeat/`: four fresh five-second Phase 23 and Phase 24 runs plus the
  Phase 23 thirty-second confirmation.
