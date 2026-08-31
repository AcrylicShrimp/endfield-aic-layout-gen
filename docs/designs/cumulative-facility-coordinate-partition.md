# Cumulative Facility Coordinate Partition Diagnostic

## Purpose

This diagnostic locates the Phase 2 first-witness cliff without changing the joint placement-routing semantics. It partitions one fixed-dimension cumulative model by the coordinate of the facility introduced in the target growth phase.

## Exactness contract

- Every legal `(x, y)` coordinate of the introduced facility is represented by one case.
- A case fixes only that facility's coordinate.
- All legal rotations at the fixed coordinate remain available.
- All other facility placements and rotations remain solver decisions.
- Port selection, boundary terminals, belt/pipe routing, flow, topology, capacity, and collision remain solver decisions.
- A prior cumulative solution may be supplied only as a non-binding placement hint.
- No case may crop, rank, or omit a legal coordinate before a validated witness exists.

The original fixed-dimension feasible set is the union of all coordinate-partition feasible sets. Therefore:

- one validated case witness proves that the original fixed-dimension model is feasible;
- all cases must be proven infeasible before the original fixed-dimension model may be reported infeasible;
- any unresolved case keeps the aggregate result unknown.

## Inputs

- Validated production wiring and game-data catalogs.
- A caller-supplied hard layout ceiling.
- A cumulative SCC target phase.
- Exact used width and height.
- A positive prefix dimension-sweep budget used to obtain the preceding phase's optional hint.
- A positive per-coordinate search budget.
- A positive worker count.

## Outputs

- Stable schema version.
- Target phase and introduced facility identity.
- Complete legal coordinate list.
- Per-coordinate execution disposition, result, timing, and compact model metrics.
- One selected validated witness, when found.
- Aggregate feasibility, complete-infeasibility, unknown, and invalid-witness counts.
- The preceding phase's primary-area proof state and optional hint bounds.

## Interpretation

- A fast witness after coordinate partitioning implicates weak propagation between the introduced facility coordinate and the remaining endpoint/routing model.
- Timeouts across many coordinate cases move the cliff inward; the next exact partition should separate port selection or boundary-terminal selection while retaining all remaining solver decisions.
- A proof of infeasibility for every coordinate proves only the requested exact dimensions infeasible.

## Port sub-partition

When one coordinate remains unresolved, a second diagnostic may fix that coordinate and partition every compatible port assignment of the introduced facility. It constructs the complete Cartesian product of the facility's logical terminal port domains. Each case fixes those port choices while retaining all other placement, boundary-terminal, routing, flow, topology, capacity, and collision decisions.

One validated assignment proves the fixed-coordinate case feasible. Every assignment must be proven infeasible before the fixed-coordinate case may be reported infeasible. Any unresolved assignment keeps the aggregate fixed-coordinate result unknown.

## Failure modes

- Invalid growth phase.
- Target phase introduces zero or multiple facilities.
- Requested exact dimensions are outside the legal dimension candidate set.
- The preceding cumulative phase cannot produce a validated hint. This is reported explicitly rather than replaced by a heuristic layout.
- Worker panic or invalid solver witness.
