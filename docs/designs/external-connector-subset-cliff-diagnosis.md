# External Connector Subset Cliff Diagnosis

## Status

Accepted diagnostic contract for the exact external-boundary-connector formulation. This research
command is separate from production solving and does not change solver semantics.

## Falsifiable Question

For Heavy Xiranite cumulative SCC phase zero under a 12 by 12 hard ceiling, what is the smallest
clean subset of the four external logical requirements for which Pumpkin cannot prove the primary
used-area optimum within an independent five-second release-mode budget?

If every non-empty subset has the same proof outcome, the diagnostic instead identifies the
sharpest increase in primary-stage search cost and the model families that account for it.

## Controlled Formulation

Every case uses `joint-shared-transport-layer-external-connectors-v1` unchanged. The solver remains
free to choose facility placement, facility rotation, compatible directional ports, and one of the
three legal external connector templates. Used geometry and the complete lexicographic objective
remain unchanged. No placement, port, template, coordinate, or objective value is fixed.

The matrix contains every non-empty subset of the four stable phase-zero logical route
requirements: four singles, six pairs, four triples, and the complete set. Every case is rebuilt
from only the selected `EdgeInput` values before network normalization and external-requirement
partitioning. Excluded requirements leave no endpoint, connector, geometry, collision, or objective
variables in the exact model.

Requirement-subset composition is the only independent matrix axis. Request bounds, workload,
formulation, build profile, solver, and search budget remain constant.

## Resource Budget And Isolation

- Hard ceiling: 12 by 12 for this experiment only.
- Search budget: 5,000 ms per case.
- Execution: optimized release binary.
- Process isolation: one process per subset case.
- Memory: macOS `/usr/bin/time -l` maximum resident set size, recorded in bytes and MiB.
- Repetitions: one structural diagnostic run per case. Counts are deterministic; timings and RSS
  are observations rather than distributions.

A timeout or incomplete objective stage is `unknown` or `feasible, unproven`, never infeasible.

## Outputs

Each isolated case writes:

- one machine-readable JSON report containing sorted route indices, stable requirement IDs, item,
  transport kind, external direction, budget, and the complete integrated-layout report;
- one self-contained HTML view for success, rejected incumbent, or failure evidence; and
- one external process record containing wall time and peak RSS.

The final report compares variable families and domains, log-domain volume, constraint families,
terms, factor incidences, placement-connector coupling, construction time, primary-stage search
time, first incumbent, objective vector, bounds, proof, termination, validation, and RSS. Deltas are
computed across the first proof-status cliff or, if none exists, the sharpest measured cost jump.

## Stopping Point

This checkpoint identifies the smallest costly connector composition, attributes its structural
growth, states what the matrix rules out, and proposes exactly one next discriminating experiment.
It does not reformulate connector variables, remove a constraint family, add a search strategy, or
silently perform the proposed next experiment. The completed diagnostic is committed and paused
for user review.
