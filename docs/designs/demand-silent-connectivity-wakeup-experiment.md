# Demand-Silent Connectivity Wakeup Experiment

## Question

Can the exact possible-graph propagator retain broad Pumpkin domain events for path state while
omitting demand-selection variables that cannot destroy reachability?

## Monotonicity argument

Pumpkin integer domains only shrink during search. The propagator's possible material graph can
therefore lose a path only when:

- a route arc can no longer be selected;
- the required item can no longer occupy an incident route arm; or
- a supply terminal option can no longer be selected.

A demand-option domain change does not alter any graph arc or supply root:

- `{0,1} -> {0}` removes a demand and needs no connectivity work;
- `{0,1} -> {1}` selects a demand but does not remove a path; and
- an unsupported `{0,1}` demand cannot later become selected because the prior propagator fixpoint
  already excludes it.

If a currently supported selected demand later becomes unsupported, the arc, item, or supply
domain reduction that destroys its last path wakes the propagator.

## Exact contract

The diagnostic variant keeps the accepted broad-lazy implementation's possible graph, traversal,
reason, pruning, priority, and solver variables. It changes only the set of integer variables
registered for wakeup. Demand variables remain part of the recorded logical constraint incidence
and are still read and pruned when path-state events run the propagator.

No placement, port, terminal, or route option is fixed or removed in advance.

## Controlled comparison

Extend the cumulative Phase 2 fixed-placement, fixed-terminal `12x12` diagnosis with a broad-lazy
case that registers only route-arc, route-item, and supply-option variables. Record registered
domain variables, custom executions and loop counts, native Pumpkin search counters, validation,
objective, and first-witness time.

## Acceptance and stopping rule

The demand-silent case must produce a validated witness with the same exact variable count and
objective. A controlled unit test must show that selecting a reachable demand causes no custom
execution and that removing its last possible path still wakes the propagator and produces a
conflict.

Report before changing route branching, explanation strength, or production enablement.
