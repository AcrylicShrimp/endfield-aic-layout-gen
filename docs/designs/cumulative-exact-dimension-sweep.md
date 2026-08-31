# Cumulative Exact Dimension Sweep

## Status

Accepted research contract for the next cumulative SCC growth experiment.

## Purpose

Grow the exact dimension portfolio from cumulative SCC phase zero to a caller-selected later phase
without fixing any placement, port, or routing decision from an earlier phase.

## Inputs

- complete facility-instance wiring graph;
- validated facility, item, transport, and logistics-component catalogs;
- caller-supplied hard maximum width and height;
- zero-based cumulative SCC target phase;
- independent Pumpkin worker count;
- independent search budget for each exact dimension case.

## Execution Contract

The harness processes cumulative phases from zero through the requested target.

For each phase it:

1. projects the complete wiring graph onto every facility introduced through that phase;
2. prepares the complete joint placement, rotation, port, terminal, routing, flow, topology,
   capacity, and collision model for that cumulative graph;
3. derives sound dimension lower bounds;
4. enumerates every exact `(used_width, used_height)` pair within the caller ceilings;
5. solves those exact cases through the parallel upper-bound-sharing portfolio;
6. validates every feasible witness independently;
7. retains the best observed minimum-area witness as the non-binding prior hint for the next
   cumulative phase.

The prior solution may affect Pumpkin branching order only. It must not remove a value from any
domain, add a coordinate equality, constrain a retained facility to its old placement, or make an
otherwise legal enlarged-phase solution infeasible.

Every phase rebuilds the full exact model. Newly introduced facilities and routes are normal solver
decisions. Earlier facilities, ports, terminals, and routes are also free to move or change.

## Exactness

Dimension partition is an exact outer decomposition:

```text
F_phase = union over every legal (w, h) of F_phase,w,h
```

Skipping an unstarted case is allowed only after an independently validated witness establishes a
strictly smaller area upper bound. Cases at or below that upper bound remain eligible. Unknown and
invalid cases never prune another case.

The primary area optimum is proven only when every smaller exact dimension case is proven
infeasible. First-witness cases do not prove transport-tile or turn optimality.

## Outputs

The cumulative report contains:

- requested target phase and total planned phase count;
- one complete exact dimension sweep report per attempted phase;
- whether the requested phase was completed;
- the first blocking phase when growth stops;
- a final validated layout or structured blocking layout;
- phase snapshots for HTML pagination;
- explicit primary and secondary proof status inside every phase sweep.

The CLI writes a top-level JSON and HTML plus phase-local summary and per-case JSON/HTML artifacts.
Artifacts are written for feasible, infeasible, unknown, and invalid-witness outcomes.

## Failure Modes

- invalid target phase;
- invalid worker count or per-case budget;
- wiring projection or exact-model preparation failure;
- no feasible witness in one cumulative phase;
- unknown smaller dimension cases preventing an area proof;
- independently rejected solver witness;
- worker panic reported as structured research failure.

No failure triggers a heuristic fallback.
