# Watched-Demand Grid Chain Experiment

## Purpose

Preserve the exact unique-support-chain inference while replacing full per-material demand scans
with a reverse dependency frontier from changed `(material, cell)` states to only the selected
demand chains that previously inspected those states.

Watcher state is scheduling metadata only. Every deduction, conflict, and explanation is rebuilt
from current Pumpkin domains by the unchanged unique-support-chain rule.

## Stable indices

Within one belt or pipe layer:

```text
DemandId = stable flattening of (material_index, demand_option_index)
WatchKey = (material_index, cell)
```

The propagator stores:

```text
dirty_demands: ordered set of DemandId
watchers[WatchKey]: ordered set of DemandId
event_impacts[LocalId]: direct DemandIds plus changed WatchKeys
```

Watcher membership is monotone during one solver run. A recomputation adds every inspected cell to
the demand's watcher set but does not remove stale cells. Stale watchers can schedule unnecessary
recomputation but cannot justify or suppress a deduction.

## Complete trace

A selected demand watches every cell read by its backward chain, including the cell where the scan
stops because of:

- a possible local supply;
- multiple possible incoming supports;
- zero possible incoming supports;
- a repeated cell or legal circulation; or
- the end of a recursively forced unique-support chain.

Recording stop cells is required. Losing a supply or one branch support at a stop cell can extend
the chain and create a new exact deduction.

## Event contract

- A demand-selection lower-bound event directly dirties that `DemandId`.
- A local-supply upper-bound loss visits watchers at its `(material, cell)`.
- A route-activation upper-bound loss visits watchers at `(material, arc.to)` for every material
  on the layer.
- Any incident arm-item domain event visits every `(material, arc.to)` incidence for every directed
  arc using that item variable.

One `DomainId` may map to several cells and materials. Event impacts are unioned before Pumpkin
registration and retain every incidence.

A relevant event always requests enqueue, even if all affected demands are already dirty. Pumpkin
queue membership and the dirty set are distinct; self-notifications raised during propagation must
be able to reserve a later execution.

## Backtracking contract

Backtracking restores supports and therefore cannot create a longer unique-support chain. Monotone
stale watcher membership is a conservative superset of the restored shallower trace. The
propagator does not cache support counts, unique arcs, or reasons, so no domain-derived proof state
requires trailing or restoration.

`propagate_from_scratch` remains a complete stateless scan over every material and demand for
Pumpkin debug checking.

## Acceptance criteria

- Targeted watched-demand tests cover demand selection, branch loss, supply loss, interior item
  loss, cross-material item assignment, and cycle stopping. Pumpkin debug checks use the stateless
  broad scan as the oracle for incremental propagation and explanations.
- A deterministic search fixture enters a conflicting descendant, observes backtracking, and
  compares the broad and watched variants after restoration.
- The fixed Phase 2 release comparison preserves the broad chain's decisions, backtracks,
  conflicts, forced predicates, objective, and independent witness validity. Native propagation
  counts may fall because skipped no-op executions are the intended optimization.
- Demand recomputations or chain steps fall materially. Elapsed time is reported separately and
  must not be inferred from work counters alone.
