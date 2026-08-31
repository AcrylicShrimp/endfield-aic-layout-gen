# Shared Boundary Terminal Phase-0 Cliff Diagnosis

## Status

Accepted diagnostic contract for the first faithful v4 performance breakdown on 2026-08-31.

This experiment diagnoses the new pre-incumbent cliff without changing production solver semantics
or selecting a replacement formulation.

## Falsifiable Question

Which smallest exact commodity-network composition changes Heavy Xiranite phase 0 from finding a
complete incumbent within 5,000 ms to finding none?

The candidate boundary is one of:

- a single commodity network already fails;
- a specific pair fails while both members succeed independently;
- only the full three-network composition fails;
- all compositions have the same outcome, in which case the sharpest measured cost jump is the
  boundary for the next diagnosis.

## Fixed Inputs

- Workload: `heavy-xiranite-minimum-rate`.
- Logical scope: cumulative SCC phase 0.
- Request ceiling: 12 by 12 for this diagnostic only.
- Formulation: `joint-shared-boundary-terminals-canonical-occupancy-v4`.
- Endpoint encoding: factored placement and port selection.
- Search budget: 5,000 ms independently for every case.
- Build: optimized release binary.

The production solver, objective, placement domains, rotations, port domains, boundary-terminal
domains, route domains, and validation contract remain unchanged.

## Matrix Axis

The only independent axis is the set of logical commodity networks present in a freshly rebuilt
model:

| Case | Exact logical networks |
| --- | --- |
| `single-0` | enriched Xiranite powder belt |
| `single-1` | Xiranite powder belt |
| `single-2` | liquid Xiranite polymer pipe |
| `pair-0-1` | both belt networks |
| `pair-0-2` | enriched powder belt and liquid polymer pipe |
| `pair-1-2` | powder belt and liquid polymer pipe |
| `full` | all three networks |

Each case is reconstructed from only the selected logical edges. Excluded networks do not remain as
zero-flow variables, endpoint candidates, or auxiliary topology state. The facility is retained
because it is part of the selected phase graph; only routing composition changes.

## Measurements

Every case records:

- stable network and logical-requirement IDs;
- total, Boolean, and integer variables;
- variable counts by family and log-domain volume;
- constraints, terms, arity, factor incidences, and family coupling;
- placement, endpoint, boundary-terminal, cell, arc, flow, branch, bridge, and objective variables;
- model construction and search time;
- first incumbent, incumbent count, objective, bound, gap, termination, proof, and validation;
- self-contained HTML for success or failure;
- isolated process peak RSS in bytes using `/usr/bin/time -l` on macOS.

## Interpretation Rules

1. A case with no incumbent at the time limit is `unknown`, never infeasible.
2. The first single-to-pair or pair-to-full transition to no incumbent is the primary cliff.
3. Structural deltas are computed between the closest successful and unsuccessful compositions,
   not only against the smallest case.
4. Raw variable count alone is not accepted as a cause. The report must identify which variable,
   domain, constraint, incidence, and placement-routing coupling families account for the jump.
5. If all cases fail, the largest build/search or structural jump becomes the next controlled
   boundary. If all succeed, the largest first-incumbent jump becomes the boundary.

## Research-Only Harness Change

The existing network-decomposition command already builds the complete seven-case matrix. It may
gain an optional case selector so each case can run in a fresh process for valid RSS measurement.
The selector changes only research orchestration. It must not fix a solver decision, alter a model,
or enter the production solve path.

## Prohibited Changes

- no fixed placement, rotation, port, boundary side, boundary coordinate, or route;
- no straight-ray restoration, corridor, crop, active window, or candidate template;
- no constructive seed, greedy ordering, or fallback;
- no constraint removal in the faithful matrix;
- no production solver reformulation during this diagnostic.

## Outputs

- one JSON and one HTML file per case;
- a machine-readable matrix summary with isolated RSS observations;
- a Markdown report containing absolute values, adjacent deltas, the first cliff, what the evidence
  rules out, and exactly one next discriminating experiment.

## Stopping Point

Stop after the network-composition cliff is identified and the diagnostic slice is committed. Do
not implement the next reformulation or diagnostic ablation until the user reviews the evidence.
