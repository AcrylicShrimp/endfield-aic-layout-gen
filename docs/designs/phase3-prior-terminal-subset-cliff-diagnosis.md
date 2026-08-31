# Phase 3 Prior-Terminal Subset Cliff Diagnosis

## Purpose

Decompose the four reference port equalities of the preceding final-target facility after the
facility-level subset experiment identified that block as the Phase 3 transition.

## Inputs

The experiment retains the Phase 3 `16 x 16` selected state, sparse endpoint support, fixed
preceding placements, and five-second candidate budget from the facility-level diagnosis. Stable
preceding-facility bit 2 selects the final-target facility.

Its four matching facility terminal IDs are sorted lexicographically and assigned stable terminal
bits. Every mask in `0 .. 16` is executed. A selected bit posts that terminal's reference port
equality. All unselected preceding facility ports remain Pumpkin decisions.

## Outputs

The structured report adds:

- the target facility bit and instance ID;
- terminal bit, terminal ID, and reference port mapping;
- every terminal subset mask and selected terminal IDs;
- fixed equality count, outcome, model scale, timing, and native search counters; and
- complete layout or structured failure evidence per case.

The CLI emits JSON, HTML summary, and per-mask HTML automatically.

## Interpretation

- A singleton transition identifies one terminal's port-to-routing coupling.
- A pair or triple transition identifies an interaction among those terminal networks.
- Only the full mask transitioning identifies collective four-terminal state.
- No transition invalidates the previous run and requires a repeat before further diagnosis.

The selected reference state is known to be infeasible. Fast proof does not establish the cost of a
feasible routing-only state.

## Invariants

- All 16 subsets are executed without heuristic selection.
- Every equality is copied from the validated Phase 2 incumbent.
- Preceding placements are fixed equally in every case.
- Introduced facility state remains fixed equally in every case.
- Routing, flow, topology, capacity, items, occupancy, and components remain solver decisions.
- Diagnostic restrictions do not become production solver constraints.
