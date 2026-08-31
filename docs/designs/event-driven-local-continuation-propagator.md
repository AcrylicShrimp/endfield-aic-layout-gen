# Event-Driven Local Continuation Propagator

## Status

Experimental exact propagator contract. It is not a production architecture commitment.

## Purpose

Activate the non-bridge local positive-flow continuation rule measured by the passive Phase 45
diagnostic without repeating its full material-by-grid scan. The propagator runs beside the accepted
watched-demand chain so the experiment measures additional inference rather than replacing an
already verified rule.

## Exact semantic rule

For material `m` at cell `g`, forward propagation requires:

1. one concrete selected positive-flow witness entering `g`;
2. a same-layer bridge at `g` is proven unselected, when such a bridge variable exists;
3. every local demand terminal for `m` is excluded; and
4. zero or one outgoing arc can still be selected with both incident item values equal to `m`.

With one possible arc, select it and fix both incident item values to `m`. With zero possible arcs,
negate the selected predicate of the concrete positive witness, producing a conflict when it is
already selected. The backward rule is the exact dual.

Multiple supports, a possible opposing terminal, or a still-possible bridge produces no inference.
Selected circulation remains legal.

## Live reasons

Every deduction rebuilds its reason from current solver domains. A forcing reason contains:

- the concrete positive witness: one selected terminal predicate, or a selected arc plus its two
  fixed item predicates;
- `bridge selected = 0` when the cell has a same-layer bridge variable;
- every excluded opposing terminal selection; and
- one current blocking predicate for every alternative route arc.

A zero-support reason omits the selected predicate that it negates. For an arc witness it retains
the two fixed item predicates. No cached domain-derived support count, witness, blocker, or reason is
used after notification or backtracking.

## Exact event schedule

The propagator registers a reverse map from solver variables to dirty `(material, cell)` keys:

- an arc selection or incident item event dirties both arc endpoints for every layer material;
- a terminal selection event dirties that terminal's material and cell; and
- a bridge event dirties that cell for every layer material.

All keys are inspected on initial propagation. Incremental propagation drains only dirty keys.
Notifications raised by the propagator's own deductions remain queued for a later fixpoint pass.
Scheduling may perform redundant work but may never omit a key whose local rule changed.

## Acceptance

- Controlled normal and Pumpkin debug-check fixtures cover forward, backward, branch stopping,
  zero-support conflict, material loss, bridge transition, circulation, self-notification chains,
  and backtracking.
- Independent reviewers approve proof soundness, implementation behavior, and the event-coverage
  boundary.
- The Phase 2 A/B returns a validated witness or structured timeout and records decisions,
  backtracks, conflicts, learned clauses, solver propagations, active executions, dirty keys,
  forced predicates, conflicts, and maximum reason size.
- Adoption requires fewer decisions or backtracks than the watched-demand baseline. A larger forced
  count alone is not success.
