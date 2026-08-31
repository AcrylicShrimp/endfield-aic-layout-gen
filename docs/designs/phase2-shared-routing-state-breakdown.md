# Phase 2 Shared Routing State Breakdown

## Question

Which shared-routing decision family, or smallest coupled group of families, accounts for the
remaining Phase 2 first-witness cliff after facility placement and every terminal are fixed to a
validated reference?

## Controlled baseline

- Heavy Xiranite minimum-rate cumulative SCC Phase 2.
- Exact used dimensions fixed to `12x12`.
- Every facility placement and every facility/external terminal fixed to the validated Phase 2
  reference.
- Feasibility-only Pumpkin search with an equal per-case wall-clock budget.
- Production solver behavior remains unchanged. Every fixation is diagnostic-only.

The routing reference is a complete independently validated solver witness. A fixation adds
equalities matching that witness. It is not a constructive route, fallback, or production hint.

## Routing decision families

| ID | Fixed state | State left free |
| --- | --- | --- |
| A | Belt/pipe physical route-cell occupancy | Arm direction, item, arcs, flow, components |
| B | Item identity on each directional cell arm, including zero for absent arms | Incoming/outgoing orientation, arcs, flow, components |
| C | Every directed grid arc activation Boolean | Item identity, integer flow, components |
| D | Every directed grid arc integer flow | Item identity, activation implications, components |
| E | Splitter, converger, bridge, and bridge-rotation selections | Route geometry, item identity, arcs, flow |

Reference reconstruction must cover every modeled variable in the selected family, including zero
values. A missing or conflicting reference mapping is invalid input, not an unfixed variable.

## Matrix

Run two complementary matrices:

1. independent cases `A`, `B`, `C`, `D`, and `E`;
2. cumulative cases `A`, `A+B`, `A+B+C`, `A+B+C+D`, and `A+B+C+D+E`.

Also rerun the routing-free baseline in the same process and budget. Cases execute sequentially so
CPU contention cannot change the comparison.

## Evidence

Record for every case:

- exact fixation families and equality count;
- construction time and search time;
- first incumbent time;
- outcome, proof, and witness-validation status;
- variables, constraints, factor-graph incidences, and placement-routing incidences;
- observed objective vector for successful cases;
- machine-readable JSON and self-contained HTML, including failures.

## Interpretation

- A fast independent case identifies one decision family whose domain was sufficient to block
  propagation.
- No fast independent case but a fast cumulative case identifies coupling between the newly added
  family and the preceding cumulative set.
- The first fast cumulative transition is the next family to decompose internally.
- If only the fully fixed case is fast, the blocker is distributed across all routing state and the
  next experiment must split the strongest remaining coupled family by transport layer, item, or
  cell subset without changing production semantics.

Timings are not additive. A timeout is `unknown`, never infeasible.

## Stopping rule

Continue exact diagnostic subdivision only at the first cumulative transition that materially
reduces first-witness time. Stop when one concrete variable/constraint coupling is isolated well
enough to propose a semantics-preserving reformulation. Do not implement that reformulation in the
same checkpoint.
