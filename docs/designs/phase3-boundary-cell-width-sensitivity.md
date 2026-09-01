# Phase 3 Boundary-Cell Width Sensitivity

## Purpose

The accepted external boundary-cell partition fixes one north-boundary endpoint exactly but leaves
ten singleton children `Unknown` after five seconds. This experiment tests whether replicated
grid-wide state in the controlled `16 x 16` canvas is the next cliff by varying width while holding
one unresolved endpoint and every previously fixed semantic decision constant.

## Parent Control

Reproduce the accepted boundary-cell report and require every interpretation gate to pass. Select
the lowest unresolved singleton key. Decode that parent key into the semantic tuple `(side, x, y)`
using the parent width, and fail closed unless it is a north-boundary endpoint.

For the accepted Heavy Xiranite report this selects key 24, meaning `(north, x=6, y=0)`.

## Exact Width Cases

Accept an explicit, strictly increasing list of widths from the CLI. Every width uses the inherited
fixed height. Re-encode the semantic endpoint for each width and validate that it remains on the
requested boundary; do not carry an opaque runtime key between grids.

For each width preserve:

- the same cumulative Phase 3 production graph;
- the same four facility placements and rotations;
- the same fifteen facility ports;
- the same selected external terminal and semantic boundary endpoint;
- every other external terminal, route cell, route arc, item, flow, topology, capacity, collision,
  and logistics-component decision as solver freedom.

The width cases are neighboring exact fixed-size problems. They are not a partition of the parent
`16 x 16` feasible set, and no result may be inferred from one width to another.

## Execution

Run widths in the explicit CLI order. Each case receives:

1. one authoritative feasibility-only solve without the root observer, with a 5,000 ms budget;
2. one separate root-observation solve with a 5,000 ms budget.

Search counters and cutoff runtime come only from the authoritative solve. Observation evidence may
contribute to the logical outcome but not to performance claims.

## Required Assertions

Interpretation is blocked unless:

- the boundary-cell parent is unblocked and unresolved;
- the selected key is the lowest unresolved singleton;
- widths are positive, unique, strictly increasing, within the request ceiling, and include the
  parent width;
- every inherited fixed facility footprint fits every width and the fixed height;
- facility, requirement, and material-network identities are identical across rebuilt width inputs;
- every selected child build certificate and observed root domain equal the re-encoded singleton;
- every non-selected external terminal uses the complete legal boundary domain for that width;
- authoritative and observation certificates agree;
- all four facilities and fifteen facility ports satisfy their fixed contracts;
- formulations and width-independent semantic model counts agree across cases;
- each case reports the expected `width * height` grid cell count;
- no invalid witness or witness/proof conflict appears.

## Measurements

Record for each width:

- encoded key and semantic `(side, x, y)`;
- model variables, constraints, incidences, family metrics, and build time;
- root external-domain cardinalities;
- root route-cell, route-arc, flow, arm-item, and component state;
- first semantic decision;
- first incumbent, search time, decisions, backtracks, conflicts, learned clauses, and solver
  propagations;
- witness, infeasibility, or `Unknown` outcome.

## Interpretation

- A validated witness proves only that width's controlled fixture feasible and must be rendered and
  semantically validated.
- An infeasibility result closes only that fixed-width fixture.
- A sharp improvement as width shrinks establishes a combined canvas/domain-scale effect; it does
  not isolate route-grid replication from the smaller external-terminal domains.
- Similar `Unknown` outcomes and normalized counters across every width reject width as the next
  practical cliff breaker and trigger a lower route/item/flow/topology exact partition.

## Stop Condition

Commit and report this width portfolio before changing routing semantics, search branching, or the
production dimension-sweep architecture.
