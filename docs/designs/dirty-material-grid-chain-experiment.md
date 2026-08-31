# Dirty-Material Grid Chain Experiment

## Purpose

Test the smallest stateful refinement of the exact unique-support-chain rule: after initial full
propagation, rerun the chain scan only for materials affected by a support-loss event instead of
rescanning every material on the belt or pipe layer.

This experiment changes scheduling only. It preserves the feasible set, objective, solver
decisions, inference rule, and reason construction.

## Exact dependency map

Each registered Pumpkin variable maps to a conservative set of affected material indices:

- selecting a demand dirties that demand's material;
- excluding a possible local supply dirties that supply's material;
- excluding a directed route arc dirties every material on the layer; and
- changing either incident arm-item domain dirties every material on the layer.

The last two mappings are deliberately broad. One physical arc can carry any currently available
layer material, and one arm-item equality for one material removes other material codes.

The initial queue contains every material. Later notifications union their affected material sets
into a deterministic dirty set. Every subscribed event requests enqueue even when its materials
are already dirty: Pumpkin queue membership is distinct from dirty-set membership, and a
self-notification raised while the propagator is running must schedule a later execution.
Propagation drains the dirty set and invokes the unchanged full per-material demand-chain scan.

## Invariants

- `propagate_from_scratch` remains a complete stateless scan over every material for Pumpkin proof
  checking.
- Incremental state schedules recomputation only; it is never used in a reason or deduction.
- Every support loss that can create a unique predecessor dirties the affected material.
- Backtracking only restores possible supports and cannot create a new unique predecessor.
- No placement, port, terminal, route, flow, item, or component decision is fixed or removed.

## Decision rule

Accept dirty-material scheduling only if repeated release runs preserve the broad chain's
deterministic search tree and materially reduce material passes or first-witness time. If arc and
item events continue to dirty every material, reject this granularity and move to exact reverse
demand-chain watchers at `(material, cell)` granularity.
