# Phase 3 Sparse Residual-State Cliff Report

## Result

The exact sparse endpoint-support encoding does not move the Heavy Xiranite Phase 3 residual-state
cliff. Fixing the three preceding facility placements is insufficient. Fixing their 12 matching
facility ports changes the same selected state from a five-second unknown result into a 162 ms
infeasibility proof.

| Diagnostic state exposed | Outcome | Build | Search | Decisions | Backtracks | Conflicts | Learned | Propagations |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Introduced facility state only | Unknown | 547 ms | 5,006 ms | 16,404 | 1,346 | 1,345 | 1,345 | 3,239,980 |
| Plus three preceding placements | Unknown | 559 ms | 5,007 ms | 40,535 | 3,620 | 3,619 | 3,619 | 4,001,809 |
| Plus 12 preceding facility ports | Proven infeasible | 545 ms | 162 ms | 227 | 12 | 13 | 13 | 269,589 |

No case produced an incumbent. Unknown remains distinct from infeasible.

## Experiment

All cases use:

- the minimum-rate Heavy Xiranite workload;
- cumulative Phase 3;
- exact used dimensions `16 x 16`;
- introduced-facility coordinate `(8, 5)`;
- introduced-facility port assignment `5` and rotation `0`;
- the exact sparse endpoint-support encoding;
- the active possible-path, watched-demand, local-continuation, and guarded-intersection stack; and
- five seconds of search time per candidate.

The three cases differ only in diagnostic equalities copied from a validated Phase 2 incumbent.
Routing, flow, item assignment, topology, capacity, transport occupancy, and logistics-component
state remain Pumpkin decisions.

The sparse model has 64,471 variables. The cases contain 163,822, 163,825, and 163,837 constraints
respectively. Model construction remains approximately 0.55 seconds and is not the transition.

## Interpretation

The stronger endpoint relation removes unsupported placement-port-geometry values, but it does not
make unresolved preceding-facility port choices cheap enough for the current search. Coordinates
alone are not the blocker: fixing all preceding placements leaves the case unknown and increases
the explored decisions during the fixed budget.

The measured transition is:

```text
preceding facility placements known
  -> unknown after 5 seconds

preceding facility placements and all facility ports known
  -> infeasibility proof in 162 ms
```

This does not prove that all routing-only cases are cheap. The selected complete facility state is
infeasible, so the fast result may expose a local or network-level incompatibility. It does prove
that the unresolved preceding-facility port block still hides decisive routing information after
the sparse endpoint reformulation.

The older nested-Element residual experiment showed the same qualitative transition, but its model
predates several exact formulation changes. Its raw counts are not used as a paired performance
comparison here.

## Next cliff decomposition

The next experiment should partition the 12 preceding facility-port equalities by facility while
retaining the fixed preceding placements:

1. no preceding ports;
2. each single preceding facility's complete port set;
3. each pair of preceding facilities' complete port sets; and
4. all three facilities' complete port sets.

This seven-subset lattice identifies whether one facility, one pair, or only the complete set
causes the transition. If a smaller subset collapses search, its terminals can then be partitioned.
If only all three collapse, the blocker is an interaction across their endpoint networks rather
than one isolated port channel.

Every subset is diagnostic-only. No fixed port choice becomes a production restriction.

## Contract and artifacts

The experiment contract is in
`docs/designs/phase3-sparse-residual-state-cliff-diagnosis.md`.

The CLI emits JSON, an HTML summary, and one HTML result per case. The measured artifact is:

```text
/tmp/aic-phase3-sparse-residual-state
```

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```
