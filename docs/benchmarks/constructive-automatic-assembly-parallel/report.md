# Parallel Automatic Constructive Assembly

## Purpose

This slice tests whether automatic process-module discovery can continue beyond the initial two-module result without changing candidate semantics or constructive quality. Module construction remains deterministic. Independent whole-node composition candidates are evaluated concurrently, then merged and ranked by the same global score and stable tie-break tuple as before.

No candidate is pruned, no placement window is narrowed, and no module is selected before its complete composition result is available.

## Sequential Cliff

The previous implementation constructed and composed every discovered candidate sequentially. A release-mode run with a step budget greater than two did not complete after 139.62 seconds and was interrupted. The third discovery frontier contained four independent composition candidates.

## Parallel Result

- Status: six automatic module attachments constructed
- Complete factory: no; the explicit six-step experiment limit was reached
- Release wall time: 61.49 seconds
- Final facilities: 15
- Final routed internal requirements: 14
- Final bounds: `53x11` (area 583)
- Unique belt tiles: 90
- Unique pipe tiles: 12
- Remaining facility-supplied requirements: 8

| Step | Frontier | Candidates | Workers | Failed compositions | Composable | Step time |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 3 | 3 | 3 | 0 | 3 | 0.667 s |
| 2 | 3 | 3 | 3 | 0 | 3 | 1.852 s |
| 3 | 4 | 4 | 4 | 1 | 3 | 8.168 s |
| 4 | 5 | 5 | 5 | 1 | 4 | 14.943 s |
| 5 | 6 | 6 | 6 | 1 | 5 | 15.940 s |
| 6 | 7 | 7 | 7 | 1 | 6 | 19.029 s |

The same first three discovery steps now finish in approximately 10.7 seconds of measured planner time. This is a strict wall-time improvement over the interrupted sequential run while preserving the candidate set and ranking contract.

## Finding

Automatic discovery did not reach an unsupported leaf or cyclic frontier in six steps. It continued to find valid small process modules and at least three composable candidates at every step.

The next observed growth cost is whole-block composition. Every frontier candidate rotates and translates an immutable source module around the entire growing target node, enumerates compatible boundary-port pairs, and runs A* for each viable pair. Both the frontier candidate count and the target canvas grow with every attachment. Parallel candidate evaluation removes the accidental serial bottleneck, but it does not change that per-candidate work.

This result does not justify guessing a new module type yet. The next constructive research slice should profile or reformulate whole-node composition while preserving its complete candidate set. Suitable exact-equivalent targets include eliminating duplicate translated collision/routing work and sharing immutable target indexes across candidate workers.

## Artifacts

- `report.json`: schema version 2 automatic discovery report with per-step elapsed time and composition worker count.
- `heavy-xiranite.html`: localized six-page constructive growth history.
