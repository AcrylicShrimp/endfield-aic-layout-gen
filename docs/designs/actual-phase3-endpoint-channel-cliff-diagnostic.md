# Actual Phase 3 Endpoint-Channel Cliff Diagnostic

## Status

Accepted experiment contract. This is a research-only diagnostic and does not change the
authoritative joint placement-routing formulation.

## Falsifiable question

At the actual Heavy Xiranite Phase 3 introduced-facility scale, is the next exact-search cliff
caused by weak propagation through the shared relation between:

- the facility placement candidate `(x, y, rotation)`;
- each logical terminal's compatible port choice; and
- the selected physical endpoint `(cell, world direction)`?

The hypothesis is supported only if an exact stronger channel removes endpoint and placement
values that the current nested-Element channel leaves unsupported, or detects a shared-placement
contradiction at root, without first expanding into an impractical hidden-variable or memory cost.

## Fixed workload and scope

- Workload: minimum-rate Heavy Xiranite benchmark.
- Cumulative growth phase: Phase 3.
- Diagnostic used dimensions: `16 x 16`.
- Subject: the facility introduced by Phase 3 and every Phase 3 logical terminal owned by it.
- Routing grid, flow, capacity, topology, occupancy, and objective variables are intentionally
  excluded. This isolates the endpoint channel; it is not a layout solve.
- Placement, rotation, port, and endpoint domains are generated from the same game data and exact
  candidate geometry used by the authoritative solver.

## Semantic invariant

Every compared encoding must accept exactly the same complete tuples:

```text
(shared placement candidate, logical port choice, packed endpoint geometry)
```

The complete legal tuple relation is generated from all legal placements and all compatible ports.
No coordinate, rotation, port, or endpoint is removed by a heuristic. Duplicate physical geometry
aliases remain distinct through the logical port value.

## Compared encodings

1. **Nested Element baseline**: the current exact two-stage placement-to-port-geometry and
   port-to-selected-geometry structure.
2. **Positive table oracle**: one complete allowed-tuple table per logical terminal, sharing the
   same placement domain. This establishes the desired generalized-arc-consistency fixpoint and
   exposes hidden row-literal cost; it is not presumed suitable for production.

The sparse support propagator is deliberately deferred. This checkpoint first establishes whether
the actual relation still has the propagation gap and whether the standard positive-table oracle
creates a new build or memory cliff. A custom propagator is justified only by those measurements.

## Controlled root restrictions

The harness applies the same restrictions to every encoding:

1. fixed placement and port, testing forward geometry propagation;
2. removal of an interior endpoint-geometry value, testing reverse propagation;
3. removal of one world-direction class, testing rotation-sensitive support;
4. removal of every endpoint geometry supporting one placement, testing last-support removal;
5. removal of one placement, testing forward cleanup of endpoint geometry;
6. multi-terminal restrictions whose union removes every shared placement support, testing whether
   a contradiction is detected at root.

Restrictions are derived from the actual relation rather than hard-coded IDs. Every restriction
must record its construction and whether it is applicable to the extracted relation.

## Measurements

For each encoding and restriction, record:

- placement, port, and endpoint domain sizes before and after root propagation;
- values removed in every direction;
- inconsistency at root;
- agreement with the positive-table oracle;
- authored integer variables and constraints;
- table rows, estimated hidden row literals, and estimated hidden clauses;
- model build time and root propagation time;
- post-build and peak RSS when measured in isolated release processes;

Search decisions, backtracks, conflicts, learned clauses, and solver propagations are recorded if
the diagnostic performs search. Root-only cases must explicitly report that these are not search
measurements.

## Decision gate

- If the baseline already matches the oracle at actual scale, endpoint channeling is not the next
  cliff and no production reformulation is justified.
- If the oracle is stronger, its measured cost decides whether the next separate slice should test
  the table in the faithful Phase 3 model or build a sparse semantic-support propagator first.
- This diagnostic does not select or install a production encoding. It ends with a measured next
  action for user review.
