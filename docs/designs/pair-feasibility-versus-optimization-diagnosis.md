# Pair Feasibility Versus Optimization Diagnosis

## Status

Accepted for implementation on 2026-08-31.

This research-only experiment follows the phase-0 network-composition diagnosis. It distinguishes
failure to construct any legal two-network witness from failure caused by the optimization search
wrapped around the same legal witness set.

## Falsifiable Question

Does Heavy Xiranite phase-0 `pair-0-1` find a complete legal placement and routing witness within
5,000 ms when Pumpkin is asked only for the first satisfying solution?

- If feasibility search succeeds while lexicographic optimization finds no incumbent, the immediate
  cliff lies in optimization strategy or objective-driven search orchestration.
- If both find no incumbent, the immediate cliff lies before optimization in the coupled hard model
  or common solver branching and propagation.

## Fixed Model

Both cases rebuild exactly the same
`joint-shared-boundary-terminals-canonical-occupancy-v4` model from:

- workload `heavy-xiranite-minimum-rate`;
- cumulative SCC phase 0;
- network 0: enriched Xiranite powder belt;
- network 1: Xiranite powder belt;
- a 12 by 12 caller-supplied diagnostic ceiling;
- the complete facility placement, rotation, port, boundary-terminal, route, item, direction,
  topology, flow, capacity, collision, bridge, and used-geometry domains.

Every objective variable and objective-definition constraint remains in both models. The cases have
identical variables, domains, constraints, terms, and legal solutions.

## Single Axis

Only the solver call changes:

| Case | Search request | Result status on a witness |
| --- | --- | --- |
| `optimize` | minimize area, then transport tiles, turns, and later tie-breakers | feasible or optimal |
| `feasibility-only` | stop at the first complete satisfying assignment | feasible |

The feasibility-only result is diagnostic. Its geometry and objective are measured but are not an
authoritative optimized layout.

## Measurements

Each case runs in an independent optimized release process with a 5,000 ms search budget and
records:

- model construction and search time;
- first incumbent time and incumbent count;
- objective value of any extracted witness;
- termination, proof, and independent witness validation;
- all existing variable, domain, constraint, term, and coupling metrics;
- process-isolated peak RSS using `/usr/bin/time -l` on macOS;
- JSON and self-contained HTML for success or failure.

## Invariants

1. No placement, rotation, port, boundary terminal, route, or objective value is fixed.
2. No legal solution is added or removed between cases.
3. Both cases use the same default Pumpkin brancher and conflict resolver; only optimization versus
   satisfaction orchestration differs.
4. A timeout is `unknown`, never infeasible.
5. Production solve behavior remains lexicographic and unchanged.

## Prohibited Changes

- no straight connector, corridor, crop, template, constructive seed, or fallback;
- no hard-constraint removal;
- no production search-mode switch;
- no adoption of feasibility-only geometry as a quality result;
- no follow-on reformulation in this slice.

## Outputs And Stopping Point

Write one JSON and HTML artifact per mode, an isolated-RSS comparison, and a Markdown report. Commit
the diagnostic slice and stop after identifying which side of the feasibility/optimization boundary
contains the cliff. Do not tune branching or reformulate the hard model until the user reviews the
result.
