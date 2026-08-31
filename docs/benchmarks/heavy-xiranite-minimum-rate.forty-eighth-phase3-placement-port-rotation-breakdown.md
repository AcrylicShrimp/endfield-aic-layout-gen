# Phase 3 Placement, Port, and Rotation Breakdown

## Question

The active cumulative growth experiment found a new four-facility first-feasible cliff. This slice asks whether the cliff is caused by the newly introduced facility's placement state:

1. its coordinate;
2. its compatible facility-port assignment;
3. its rotation after coordinate and ports are fixed.

Every partition is exhaustive and semantics-preserving. Other facilities, their ports, all shared commodity routing, item assignment, flow, capacity, and topology remain solver decisions. The active possible-graph plus local-continuation stack is used in the prefix and partition cases.

## Fixed model

- target phase: 3
- used dimensions: 16x16
- cumulative facilities: 4
- commodity networks: 8
- network terminals: 26
- exact model variables: 48,967
- exact model constraints after full introduced-facility fixation: 163,878
- per-case search budget: 5 seconds

The preceding three-facility layout is obtained with a separate ten-second per-dimension diagnostic budget and is used only as a non-binding hint.

## Coordinate partition

The introduced 5x5 facility has 144 legal coordinates in the 16x16 domain.

| Result | Cases |
|---|---:|
| Validated feasible | 0 |
| Proven infeasible | 4 |
| Unknown at 5 seconds | 140 |

The four corners `(0,0)`, `(0,11)`, `(11,0)`, and `(11,11)` were proven infeasible. Every other coordinate timed out. Fixing the introduced facility coordinate alone therefore does not remove the cliff.

## Facility-port partition

At coordinate `(5,5)`, the introduced facility has four logical terminal choices:

- one pipe input with one compatible port;
- two belt inputs with five compatible ports each;
- one belt output with five compatible ports.

This gives `1 * 5 * 5 * 5 = 125` complete port assignments.

| Result | Cases |
|---|---:|
| Validated feasible | 0 |
| Proven infeasible | 22 |
| Unknown at 5 seconds | 103 |

Fixing the coordinate and every introduced-facility port still does not remove the cliff.

## Rotation partition

Port assignment 0 was unknown after five seconds when rotation remained a four-value solver domain. The same exact case was then partitioned into its four legal rotations.

| Rotation | Result | Search time |
|---:|---|---:|
| 0 | Proven infeasible | 65 ms |
| 90 | Proven infeasible | 67 ms |
| 180 | Proven infeasible | 67 ms |
| 270 | Proven infeasible | 66 ms |

All four partitions are infeasible, so port assignment 0 is proven infeasible. The key propagation gap is that the unpartitioned solver did not combine these four small proofs within five seconds, even though each fixed-rotation proof takes about 66 milliseconds.

## Interpretation

The cliff is not explained by the introduced facility's coordinate alone. It is also not removed by fixing its complete port assignment while leaving rotation unresolved.

The first confirmed blocker is a weak disjunction between high-level facility state and the large routing model:

```text
port assignment
  -> one of four rotations
  -> rotated terminal geometry
  -> shared routing, item, and flow constraints
```

The routing consequences become strong only after the rotation branch is selected. Pumpkin does not cheaply lift the four individually infeasible routing proofs back into the unresolved rotation domain in this case.

## Next exact experiment

Build the Cartesian product of all 125 port assignments and all four rotations as 500 independent exact cases. This is a complete partition of the same `(coordinate=5,5)` subproblem, so it cannot remove a legal solution or lose the global optimum within the fixed dimension and coordinate partition.

If one case finds a validated witness, the exact outer portfolio has crossed the four-facility cliff and identifies high-level terminal geometry disjunction as the dominant blocker. If all 500 cases are proven infeasible, repeat at another non-corner coordinate. If many cases remain unknown, the next blocker lies below complete introduced-facility geometry, in the remaining facilities' terminal ownership or shared routing/flow state.

## Verification

The final slice verification runs:

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
git diff --check
```
