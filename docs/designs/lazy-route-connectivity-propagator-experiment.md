# Lazy Route Connectivity Propagator Experiment

## Question

Can the exact broad-event possible-graph propagator preserve its search-tree reduction while doing
less CPU work on each execution?

## Unchanged contract

The new variant uses the same broad integer-domain subscriptions, possible graph, demand pruning,
and eager explanation content as the accepted diagnostic propagator.

- It adds no solver variable or constraint decision.
- It does not change propagation priority or wakeup events.
- It removes exactly the same unsupported demand options.
- It produces the same exclusion predicates when Pumpkin requests a conflict reason.
- It remains disabled in the production solver path.

## Internal work reduction

The original implementation performs these steps on every wakeup:

1. allocate a fresh adjacency vector for every grid cell;
2. inspect every physical directed arc and populate the possible adjacency graph;
3. construct a complete absent-arc and absent-supply explanation; and
4. run reachability from possible supplies.

The lazy variant instead:

1. builds immutable outgoing-arc index lists once when the propagator is constructed;
2. starts from possible supplies and inspects only outgoing arcs of reached cells;
3. records reachable cells without allocating a new adjacency graph; and
4. scans all arcs to construct an explanation only when at least one still-selectable demand is
   unsupported.

The final explanation remains eager and identical to the original explanation. Only the time at
which it is constructed changes.

## Controlled comparison

Run the same fixed-placement, fixed-terminal cumulative Phase 2 `12x12` feasibility problem with:

1. unchanged baseline;
2. broad-event eager possible-graph propagation;
3. the rejected event-selective predicate-watcher variant; and
4. broad-event lazy traversal and lazy explanation construction.

Record native search statistics plus custom executions, reachability arc checks, explanation
builds, explanation arc scans, total arc scans, demand checks, prunings, conflicts, and maximum
reason size.

## Acceptance and stopping rule

The lazy case must preserve the exact variable count, validation result, branch decisions,
backtracks, conflicts, learned clauses, and witness objective of the broad-event case when neither
times out. A runtime improvement without that identical search evidence is not enough to claim a
pure internal-work optimization.

Report before implementing dynamic connectivity, cached domain state, layer-wide multi-material
propagation, mandatory-cut reasoning, or production enablement.
