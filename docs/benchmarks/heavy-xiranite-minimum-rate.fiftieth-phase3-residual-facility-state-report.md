# Phase 3 Residual Facility-State Breakdown

## Question

The complete introduced-facility state portfolio left 400 cases unknown after five seconds. This
slice asks whether those cases are still difficult because the preceding three facilities retain
placement and port freedom, or because routing remains difficult after their state is known.

The controlled case uses:

- exact used dimensions 16x16;
- the new 5x5 facility at `(8,5)`, which does not overlap the validated Phase 2 placement and leaves
  at least one routing cell between it and the nearest prior facility;
- complete introduced-facility port assignment 5, whose two belt demands use distinct ports;
- introduced-facility rotation 0; and
- the default five-second search budget.

The active exact possible-graph, watched-demand, and local-continuation stack remains enabled.

## Cases

All three cases fix the complete introduced-facility state. They differ only in which state from the
validated three-facility Phase 2 incumbent is posted as diagnostic equalities:

1. `introduced-state-only`: no prior state is fixed;
2. `prior-overlap-placements`: the three prior facility placements are fixed; and
3. `prior-overlap-placements-and-facility-ports`: those placements plus 12 matching prior facility
   terminal ports are fixed.

These are diagnostic restrictions, not a replacement production architecture. They may exclude
other legal Phase 3 layouts and therefore cannot establish global infeasibility or optimality.

## Result

| Case | Added exact equalities | Outcome | Search | Decisions | Backtracks | Conflicts | Learned clauses | Solver propagations |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| Introduced state only | 0 | Unknown | 5,009 ms | 18,577 | 1,385 | 1,384 | 1,384 | 3,894,584 |
| + three prior placements | 3 | Unknown | 5,006 ms | 37,731 | 4,352 | 4,351 | 4,351 | 4,786,363 |
| + 12 prior facility ports | 15 | Proven infeasible | 150 ms | 248 | 15 | 16 | 16 | 333,458 |

Every case has 48,967 variables. The baseline has 163,878 constraints; the last case has 163,893.
Model construction remains approximately 0.5 seconds and is not the observed search cliff.

## Interpretation

Removing the preceding facilities' placement freedom does not cross the cliff. The fixed-placement
case still consumes its full five seconds and actually explores more decisions and conflicts than
the baseline during that interval.

Fixing the preceding facility ports changes the result qualitatively. The remaining routing model
proves this complete facility-state combination infeasible in 150 ms. This establishes that the
large routing model can propagate strongly once all facility endpoint geometry is known for this
case.

It does **not** establish that routing-only search is always cheap. This selected state is
infeasible, so the fast proof may be a local endpoint incompatibility. A feasible complete state is
needed before routing-only first-witness cost can be measured.

The current cliff hierarchy is therefore:

```text
free introduced facility coordinate
  -> still unknown
fixed introduced coordinate
  -> still unknown
fixed introduced coordinate + complete ports
  -> still unknown
fixed introduced coordinate + complete ports + rotation
  -> some states prove quickly, most remain unknown
plus fixed prior facility placements
  -> still unknown
plus fixed prior facility ports
  -> this state proves infeasible quickly
```

## Next exact experiment

At coordinate `(8,5)`, enumerate all 125 introduced-facility port assignments and four rotations
again while fixing the validated Phase 2 facilities' placements and matching facility ports. This is
an exhaustive 500-case partition of that restricted extension problem.

- If a validated witness appears, it gives a known feasible complete facility state and exposes the
  true routing-only first-witness cost.
- If every case proves infeasible, this particular Phase 2 state cannot be extended at `(8,5)`.
- If many cases remain unknown, endpoint geometry alone is insufficient and the next cliff remains
  inside routing/flow.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```

All 28 CLI tests and 217 data-library tests passed. The command emits summary JSON, summary HTML, and
one standalone HTML failure artifact per case.
