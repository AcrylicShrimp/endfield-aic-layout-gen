# Phase 3 Sparse Residual-State Cliff Diagnosis

## Purpose

Identify the first facility-state coupling that changes the Heavy Xiranite Phase 3 fixed-dimension
case from a five-second unknown result into a fast witness or infeasibility proof after enabling the
exact sparse endpoint-support encoding.

This experiment diagnoses the next search cliff. It does not adopt diagnostic fixations as a
production strategy and does not propose another propagator before the transition is measured.

## Inputs

- the committed minimum-rate Heavy Xiranite workload;
- cumulative growth target Phase 3;
- exact used dimensions `16 x 16`;
- introduced-facility coordinate `(8, 5)`;
- introduced-facility port assignment `5` and rotation `0`;
- a validated exact Phase 2 incumbent used only as a non-binding hint and diagnostic reference;
- the sparse endpoint-support encoding; and
- five seconds of search time per residual case.

The existing prefix sweep retains its established exact solver stack. Only the three Phase 3
residual cases switch endpoint-channel encoding.

## Exact residual cases

The cases expose increasingly complete facility state while leaving routing, flow, topology,
capacity, transport occupancy, and logistics-component decisions to Pumpkin:

1. introduced facility coordinate, ports, and rotation only;
2. case 1 plus the three preceding facility placements; and
3. case 2 plus the matching preceding facility port choices.

These equalities are diagnostic restrictions. They may exclude other legal Phase 3 layouts and
therefore cannot establish global Phase 3 feasibility, infeasibility, or optimality.

## Output contract

The structured report records:

- endpoint encoding;
- case kind and outcome;
- construction and search time;
- first-incumbent time;
- branch decisions, backtracks, conflicts, learned clauses, and solver propagations;
- model variable and constraint counts; and
- the complete layout or structured failure report for every case.

JSON, HTML summary, and per-case HTML artifacts are emitted automatically, including for unknown or
infeasible cases.

## Interpretation rule

The first adjacent pair of cases whose outcome or runtime class changes materially is the measured
coupling transition. Only constraints added at that transition become candidates for the next
controlled decomposition.

If all three cases remain unknown, the next diagnostic must continue inside transport state. If the
same transition seen with nested Element remains, sparse endpoint support did not move the cliff.
If an earlier transition appears, the stronger channel changed which unresolved coupling dominates.

## Invariants

- Placement, rotation, port selection, and routing remain jointly represented in every case except
  for the explicitly reported diagnostic equalities.
- The sparse endpoint relation preserves every legal tuple.
- No corridor, coordinate template, path ordering, port heuristic, or route heuristic is added.
- Each candidate receives the standard five-second budget.
- Unknown is not reported as infeasible.

## Failure modes

- invalid workload or fixed dimensions produce structured invalid-input evidence;
- failure to obtain the exact Phase 2 prefix witness aborts the experiment;
- unsupported endpoint encodings are rejected before model construction; and
- a worker failure aborts rather than silently dropping a residual case.
