# Phase 3 Representative Prior-Input Port Controls

## Question

Inside one predeclared residual Phase 3 source leaf, does fixing either of the two remaining
five-value belt input ports on the old powder-producing facility cross the five-second
first-feasible cliff?

This is a local exact diagnosis. It does not represent the other 39 source leaves or prove global
Phase 3 feasibility or infeasibility.

## Exact Contract

The experiment reruns the complete prior hierarchy and selects the unique existing source report
leaf whose index is `0`, before observing the new outcomes. That parent remains `unknown` after
5,005 ms in this run and carries nine distinct terminal equalities.

The current exact Phase 3 input exposes exactly two remaining non-singleton belt-demand terminal
domains on the same old source facility. Each domain contains the five distinct values
`input-belt-0` through `input-belt-4`.

Two overlapping suites are executed:

- suite 0 fixes the first terminal to each of its five values while the second remains a solver
  decision;
- suite 1 fixes the second terminal to each of its five values while the first remains a solver
  decision.

Each suite separately partitions the complete representative parent. The suites overlap, so their
ten cases are not ten disjoint proof regions and are not added to the preceding 89-region coverage
count. Every child adds exactly one terminal equality. Placement, the other terminal, routing,
item, flow, topology, capacity, occupancy, component, and bridge decisions remain in the exact
solver model.

## Result

Both suites produce the same outcome shape:

| Fixed port | Connection cell | Suite 0 | Suite 1 |
|---|---:|---|---|
| `input-belt-0` | `(0, 11)` | Unknown | Unknown |
| `input-belt-1` | `(0, 12)` | Unknown | Unknown |
| `input-belt-2` | `(0, 13)` | Unknown | Unknown |
| `input-belt-3` | `(0, 14)` | Unknown | Unknown |
| `input-belt-4` | out of bounds | Proven infeasible | Proven infeasible |

The four in-bounds values in each suite consume the complete five-second budget without an
incumbent. The out-of-bounds value is rejected at root:

| Suite | Port-4 search | Decisions | Conflicts | Propagations |
|---:|---:|---:|---:|---:|
| 0 | 64 ms | 0 | 1 | 179,999 |
| 1 | 62 ms | 0 | 1 | 179,083 |

This is an exact domain exclusion, but not evidence of a deeper search separator. The fixed prior
facility lies against the left boundary, and its rotated `input-belt-4` connection would be outside
the `16 x 16` used dimensions. The diagnostic retains that declared domain value instead of
silently deleting it, and the solver proves it impossible.

No suite is completely infeasible, no witness is found, and no invalid witness occurs. Therefore
the representative leaf remains unresolved.

## Model And Search Measurements

The representative parent has:

- 64,471 variables;
- 163,830 constraints;
- 626,032 incidences; and
- 244,622 placement-routing incidences.

Each child preserves the variable and placement-routing counts while adding the one intended
research equality:

- 64,471 variables;
- 163,831 constraints;
- 626,033 incidences; and
- 244,622 placement-routing incidences.

Across the ten control cases, Pumpkin records:

| Metric | Total |
|---|---:|
| Search time summed across workers | 40,179 ms |
| Branch decisions | 313,189 |
| Backtracks | 24,326 |
| Conflicts | 24,320 |
| Learned clauses | 24,320 |
| Solver propagations | 27,257,560 |
| Restarts | 0 |

The control wave finishes in 5,880 ms with 12 workers. The complete hierarchy plus controls takes
98,252 ms. The parent and children are separate solver runs, so their event counts are descriptive;
they are not a controlled runtime-improvement claim.

## Exact Residual Reduction

Let the two port variables be `A` and `B`, each with domain `{0,1,2,3,4}`. Suite 0 proves the whole
row `A=4` infeasible while `B` is free. Suite 1 proves the whole column `B=4` infeasible while `A`
is free. Their union closes nine of the 25 ordered pairs:

```text
{(4,b) | b in 0..4} union {(a,4) | a in 0..4}
```

The overlap `(4,4)` is counted once. The exact unresolved residual is therefore the complete
`4 x 4` Cartesian product over values `0..3`, or 16 cases. This reduction is proof-based and does
not remove any still-legal representative solution.

## Interpretation

Fixing either input port alone does not cross the substantive search cliff for any in-bounds value.
The result nevertheless removes the boundary-impossible row and column exactly and shrinks the
next complete pair partition from 25 to 16 cases.

The supported conclusion is local: these two single-terminal partitions do not separate the
remaining in-bounds outcomes within five seconds for source leaf 0. It is not evidence that port
choice is irrelevant, that routing alone is the blocker, or that the other 39 source leaves behave
the same way.

## Next Experiment

Execute all 16 ordered assignments in the residual `A in 0..3`, `B in 0..3` Cartesian product under
the same representative parent. Include equal-port assignments; no all-different rule has been
accepted. Only the two port equalities may be added.

If all 16 remain unknown, take one pair-fixed leaf and record a read-only root-domain snapshot
before adding another propagator. First verify that every introduced, target, and old-source
facility endpoint geometry is singleton. Then measure unresolved material-capable route arcs,
arm-item domains, flows, terminal routing options, and the remaining external-terminal domains.

## Artifacts

- Design: `docs/designs/phase3-representative-prior-input-port-controls.md`
- Machine-readable report and self-contained HTML:
  `/tmp/aic-phase3-prior-input-controls.vQ6ZHE/summary.json` and `summary.html`
- Ten automatically emitted standalone case HTML files in the same directory

## Independent Review And Verification

Three independent reviews examined proof soundness, implementation behavior, experiment scope,
and the next exact strategy. All found the implementation and result valid after the design's stale
unconditional `5 x 5` follow-up was replaced by the proof-driven `4 x 4` residual rule. Reviewers
also confirmed that an absent display connection is not itself a pruning premise; only the two
`ProvenInfeasible` solver outcomes close the row and column.

Focused tests now protect that a partial suite cannot be aggregated into complete infeasibility and
that an out-of-bounds connection remains an emitted control value. Final verification passes:

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
git diff --check
```

## Reproduction

```text
target/release/aic-prior-terminal-pair \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --used-width 16 \
  --used-height 16 \
  --facility-x 8 \
  --facility-y 5 \
  --port-assignment-index 5 \
  --facility-rotation 0 \
  --prior-facility-bit 2 \
  --terminal-pair 2,3 \
  --worker-count 12 \
  --prefix-case-time-limit-ms 10000 \
  --pair-case-time-limit-ms 5000 \
  --complete-target-ports \
  --child-case-time-limit-ms 5000 \
  --split-prior-source-port \
  --source-case-time-limit-ms 5000 \
  --control-prior-input-ports \
  --representative-source-leaf-index 0 \
  --input-control-case-time-limit-ms 5000 \
  --output-dir /tmp/aic-phase3-prior-input-controls.vQ6ZHE
```
