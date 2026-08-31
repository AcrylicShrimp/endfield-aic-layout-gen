# Phase 3 Sparse Endpoint Support Propagator Comparison

Status: accepted experiment contract

## Question

Can one exact semantic propagator enforce each facility-terminal relation

```text
(placement, port, geometry) in legal_rows
```

with the same domain consistency observed from Pumpkin's positive table, but without one hidden,
branchable Boolean selector per legal row?

This is a bounded formulation experiment. It is not approval to change search order, preselect a
placement, port, or geometry, or remove any legal row.

## Semantic contract

For each terminal, the propagator receives the complete legal ternary relation derived from the
runtime facility catalog and all legal placements inside the caller's ceiling. A complete
assignment is accepted if and only if its tuple occurs in that relation.

The placement variable remains shared by every terminal of one facility. Each terminal keeps its
own port and geometry variables. Port identity remains observable even when two ports can produce
the same geometry.

The rule implements generalized arc consistency over the three current domains:

```text
remove placement p  iff no row (p, q, g) has q and g still present
remove port q       iff no row (p, q, g) has p and g still present
remove geometry g   iff no row (p, q, g) has p and q still present
```

It wakes on every integer-domain change of all three variables and reaches the same propagation
fixed point as the positive-table oracle when both start from the common legal-row projection.
When a caller deliberately starts from a larger declared domain, the propagator also removes values
that occur in no legal row; this is sound generalized arc consistency stronger than the native
table's observed root filtering. Cached row supports are only residues: every residue is
revalidated against current domains before use, so backtracking cannot make a stale residue sound.

## Explanation contract

For a removed target value, the eager reason contains one already-true blocker from every legal
row containing that target value. A blocker is a disequality predicate on one of the other two
columns whose row value is absent from the current domain. The target variable's own predicate is
never part of its reason.

This reason proves that every tuple containing the removed value is impossible. Empty-domain
conflicts arise only through explained value removals. No unresolved game-semantic assumption is a
pruning premise.

## Controlled comparison

The sparse propagator must match the positive-table oracle for all complete assignments of the
small endpoint relation and for these root restrictions:

1. fixed placement and port;
2. an interior geometry hole;
3. one direction class;
4. removal of every support for one placement;
5. a placement-domain hole propagated forward; and
6. a conflict shared through several terminals' common placement variable.

The actual Heavy Xiranite Phase 3 introduced-facility channel then repeats the same restriction
portfolio. The report records relation sizes, authored variables, hidden row selectors, build
time, root propagation time, executions, notifications, row scans, residue hits and misses,
removed values, conflicts, and maximum explanation size.

## Gate and follow-up

Pass the channel-only gate only if the sparse propagator:

- preserves the complete solution set;
- matches every applicable positive-table restriction result;
- has zero hidden row selectors; and
- materially reduces the positive table's construction or runtime cost without an unexplained
  pathological regression against nested Element.

If it passes, replace only the endpoint channel inside the faithful joint Phase 3 model and compare
three isolated release runs with the existing five-second candidate budget. Do not keep tuning this
component after that comparison. Grow the cumulative graph immediately and diagnose the next
observed cliff. If it fails the gate or produces only negligible joint improvement, retain the
authoritative nested Element encoding and move directly to the next cliff.
