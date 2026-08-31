# Event-Selective Route Connectivity Propagator Experiment

## Question

Can the exact possible-route connectivity inference retain its first-witness benefit while avoiding
wakeups that cannot invalidate a possible source-to-demand path?

## Unchanged semantics

The possible graph and pruning rule remain unchanged from
`possible-route-connectivity-propagator-experiment.md`.

- No placement, port, route, path, parent, depth, or objective variable is added.
- No route candidate, corridor, path order, or coordinate is selected outside Pumpkin.
- A demand option is removed only when no compatible supply can reach it in the current
  over-approximated possible graph.
- The production solve path remains unchanged.

## Relevant monotone events

A possible path can disappear only when at least one of these predicates becomes true:

1. a physical directed arc can no longer be selected;
2. the material is removed from the source arm of a physical arc;
3. the material is removed from the destination arm of a physical arc; or
4. a compatible supply-terminal option can no longer be selected.

The event-selective propagator registers exactly those exclusion predicates. It does not subscribe
to unrelated lower-bound changes, assignments that preserve the relevant value, or demand-option
changes.

Backtracking only restores arc, item, or supply possibilities. Any demand pruning derived below the
restored decision level is restored by Pumpkin at the same time. A graph expansion cannot create a
new unsupported demand, so graph restoration does not require an eager propagator wakeup.

## Search statistics contract

Every exact solve report records the following Pumpkin search counters when available:

- branch decisions (`nodes` in Pumpkin 0.5 statistics);
- backtrack events observed through the public brancher callback;
- conflicts (`failures`);
- learned clauses (`nogoods`; Pumpkin 0.5 currently reports this as its conflict count);
- solver propagator calls (`propagations`);
- atomic propagated constraints (`numAtomicConstraintsPropagated`); and
- restarts.

Unavailable native counters must be serialized as `null`, not invented or inferred from elapsed
time. Capturing statistics must not change branching or conflict resolution.

## Controlled comparison

Repeat the fixed-placement, fixed-terminal cumulative Phase 2 `12x12` feasibility problem with
three cases:

1. unchanged baseline;
2. the from-scratch possible-graph propagator with broad integer-domain subscriptions; and
3. the same possible-graph inference with exclusion-predicate subscriptions.

Record first-witness time, validation, model size, all search counters, propagator executions,
predicate notifications, arc scans, demand checks, prunings, conflicts, and maximum reason size.

## Acceptance and stopping rule

The event-selective case must preserve the exact model variable count and pass the unchanged witness
validator. Small tests must show that irrelevant domain changes do not wake the propagator while
path-removing predicates do.

Report the controlled comparison before implementing incremental adjacency maintenance,
cut-arc forcing, custom branching, or production enablement.
