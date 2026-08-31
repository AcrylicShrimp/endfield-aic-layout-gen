# Phase 3 Facility-State Exact Portfolio

## Question

The preceding diagnosis found that one unresolved four-way facility rotation domain could hide four
individually cheap infeasibility proofs. This slice tests whether exhaustively partitioning the newly
introduced facility's complete high-level state crosses the four-facility first-feasible cliff.

The portfolio fixes one diagnostic coordinate, `(5,5)`, and enumerates the Cartesian product of:

- all 125 compatible complete facility-port assignments; and
- all four legal facility rotations.

This creates 500 independent exact cases. Every case retains the other facilities' placement,
rotation, and ports, plus all belt/pipe routing, item, topology, capacity, and flow decisions. No
route, corridor, placement, or port heuristic is introduced. Stopping after a validated witness is
sound for this first-feasible diagnostic, although it would not prove the secondary transport and
turn objectives optimal.

## Command

```text
cargo build --release -p aic-cli
target/release/aic-cli research diagnose-cumulative-facility-states \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --used-width 16 \
  --used-height 16 \
  --facility-x 5 \
  --facility-y 5 \
  --worker-count 12 \
  --prefix-case-time-limit-ms 10000 \
  --state-case-time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-phase3-active-state-partition-16x16-x5-y5-5s
```

The ten-second prefix budget only obtains the preceding three-facility exact hint. Every Phase 3
state case uses the default five-second research budget.

## Result

| Metric | Value |
|---|---:|
| Fixed used dimensions | 16x16 |
| Fixed introduced-facility coordinate | (5,5) |
| Port assignments | 125 |
| Rotations per assignment | 4 |
| Exact cases | 500 |
| Workers | 12 |
| Validated feasible | 0 |
| Proven infeasible | 100 |
| Unknown at five seconds | 400 |
| Invalid witnesses | 0 |
| Outer wall time | 201,786 ms |

The 100 proven-infeasible cases finish in 71--103 ms, with an average of 82.28 ms. Every unknown
case consumes 5,005--5,033 ms, with an average of 5,010.41 ms.

The outcome is uniform by complete port assignment:

- 25 assignments are proven infeasible under all four rotations;
- 100 assignments remain unknown under all four rotations;
- no assignment has a mixture of infeasible and unknown rotations.

The 25 quickly rejected assignments are exactly those where the two belt-demand terminals select
the same input port. The portfolio therefore exposes an exact high-level incompatibility that could
be represented directly, but removing these 100 cheap cases would reduce total wall time by only
about one second with 12 workers. It is not the remaining cliff.

## Search evidence

Each fully fixed introduced-facility state still constructs the same large residual model:

| Model metric | Value |
|---|---:|
| Variables | 48,967 |
| Constraints | 163,878 |
| Constraint incidences | 626,176 |
| Placement-routing incidences | 244,622 |

Across all 500 cases, Pumpkin records:

| Search metric | Value |
|---|---:|
| Branch decisions | 4,494,375 |
| Backtracks | 421,772 |
| Conflicts | 421,473 |
| Learned clauses | 421,473 |
| Solver propagations | 1,292,973,530 |

## Worker sharing

The workers are independent Pumpkin instances. They share only an atomic validated-witness signal,
which stops undispatched work after any worker finds a witness. They do not exchange learned clauses
or nogoods.

There is no useful scalar objective bound to share in this diagnostic: width and height are fixed to
16x16, and every case performs feasibility-only search rather than optimizing transport tiles or
turns. The earlier exact dimension portfolio does share improved area upper bounds because those
bounds can eliminate complete larger-area cases. That mechanism does not apply to these equal-size
facility-state cases.

## Interpretation

The exact port-and-rotation partition does not cross the four-facility cliff at coordinate `(5,5)`.
It confirms two different layers:

1. A small high-level incompatibility exists and becomes cheap after state partitioning.
2. For every remaining distinct-input-port assignment, fully fixing the introduced facility's
   coordinate, ports, and rotation still leaves a five-second first-feasible cliff.

The second result is decisive. Raw facility-state candidate count contributes to outer wall time,
but it is not the dominant per-case blocker. The next blocker lies below the introduced facility's
complete state, among the remaining facilities' endpoint ownership and the shared routing/flow
model.

## Improvements retained in the active stack

The current result includes the accumulated exact improvements from earlier slices:

- one shared belt layer and one shared pipe layer instead of one dense grid per logical line;
- separate placement and port domains instead of flattened placement-port candidates;
- external connectors represented inside the shared transport network;
- canonical physical occupancy channeling between facilities and transport cells;
- exact parallel used-dimension partitioning with incumbent upper-bound propagation;
- possible-graph demand reachability propagation;
- watched unique-support chains; and
- event-driven exact local-continuation propagation.

These changes moved the meaningful structural cliff to the four-facility cumulative model under the
larger diagnostic prefix budget. The present result does not invalidate them; it identifies the next
remaining coupling.

## Next exact decomposition

Choose one of the 400 unknown complete states and partition only the remaining unresolved facility
endpoint state. The first comparison should distinguish:

1. all remaining facility coordinates/rotations fixed while their ports remain free;
2. all remaining facility coordinates/rotations/ports fixed while routing remains free; and
3. one fixed complete facility state plus one fixed same-item network terminal-ownership choice.

If case 2 remains unknown, the blocker is a routing/flow witness problem even after every facility
state is known. If case 2 becomes fast, the blocker is the disjunction over the other facilities'
states. This split should precede learned-clause exchange research because it identifies whether the
repeated worker failures share a compact high-level nogood or only low-level route clauses.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```

All 27 CLI tests and 217 data-library tests passed. The CLI automatically emitted both a structured
summary and a representative failure visualization despite returning no witness.
