# Phase 3 Representative Prior-Input Port Pair

## Question

After exactly removing the two boundary-impossible single-port values, does fixing both remaining
belt inputs on the old powder-producing facility cross the five-second first-feasible cliff inside
representative source leaf 0?

## Exact Coverage

The complete control hierarchy is rerun. It again proves `input-belt-4` infeasible for either
terminal while the other remains free. Those two proofs close nine of the original 25 ordered
port pairs. The pair portfolio then executes every ordered combination over each residual domain
`input-belt-0` through `input-belt-3`, including equal-port assignments.

Each of the 16 cases inherits the same nine fixed terminal choices and adds only the two selected
input-port equalities. The old source facility placement is retained from the prior-overlap
diagnostic. Every other facility port, external terminal, route, material, flow, topology, capacity,
occupancy, component, bridge, and objective decision remains in the exact model.

The 16 cases plus the nine previously proven empty pairs exactly cover the representative leaf.
They do not cover the other 39 source leaves or global Phase 3.

## Result Matrix

Rows are the first demand's port and columns are the second demand's port:

| A / B | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| 0 | Proven infeasible | Unknown | Unknown | Unknown |
| 1 | Unknown | Proven infeasible | Unknown | Unknown |
| 2 | Unknown | Unknown | Proven infeasible | Unknown |
| 3 | Unknown | Unknown | Unknown | Proven infeasible |

All four equal-port assignments reject at root in 54--70 ms with zero decisions, zero backtracks,
one conflict, and one learned clause. All 12 unequal-port assignments consume the complete
five-second budget without an incumbent.

This establishes a second exact geometric/material exclusion: the two distinct belt demands cannot
simultaneously use the same physical facility port in this fixed state. It was not encoded as a
hand-written all-different shortcut; the full exact cases were retained and Pumpkin proved each
diagonal empty.

The representative leaf remains unresolved: no witness, 13 total proven-empty atomic pairs when
the preceding row/column proofs and the four new diagonal proofs are combined, and 12 unknown
atomic pairs.

## Model And Search Measurements

Every pair case has:

- 64,471 variables;
- 163,832 constraints;
- 626,034 incidences; and
- 244,622 placement-routing incidences.

Relative to the representative source parent, this is exactly two added research equalities and
incidences, with no variable or placement-routing increase.

Across all 16 cases:

| Metric | Total |
|---|---:|
| Search time summed across workers | 60,342 ms |
| Branch decisions | 461,510 |
| Backtracks | 39,284 |
| Conflicts | 39,276 |
| Learned clauses | 39,276 |
| Solver propagations | 43,264,185 |
| Restarts | 0 |

Among the 12 unknown unequal-port cases, decisions range from 33,263 to 49,147 and propagations
from 3,224,734 to 4,378,963. The 12-worker pair wave finishes in 11,712 ms. The complete hierarchy
plus pair wave takes 109,367 ms. These counters describe separate solver runs and do not establish
a runtime improvement.

The still-unresolved exact model contains four facilities, eight commodity networks, 26 network
terminals, ten external boundary terminals, 1,024 route-cell variables, 1,920 directed route-arc
variables, 1,920 flow variables, and 4,096 branch-component variables.

## Interpretation

Complete old-source facility endpoint fixation is not sufficient to cross the five-second cliff in
any physically distinct pair. This is stronger than the single-port result: the solver no longer
has to select either of these two endpoint cells, yet every unequal pair remains unknown.

It is still incorrect to call the remaining problem routing-only. Four two-value facility pipe
port domains on other facilities and ten actual external boundary terminals remain solver choices,
in addition to all shared routing, material, flow, capacity, topology, and component state.

The supported boundary is therefore:

```text
old-source input A free or input B free
  -> in-bounds values remain unknown
both old-source inputs fixed to the same port
  -> root infeasible
both old-source inputs fixed to distinct ports
  -> all 12 remain unknown at five seconds
```

No further port partition exists on this old-source facility: its placement is fixed, its powder
output is fixed, its two belt inputs are fixed in each pair, and its pipe input has a singleton
domain.

## Next Diagnostic

Pair index 0 was predeclared before observing outcomes, but it is one of the root-infeasible
diagonal cases. For the passive root snapshot, use the deterministic fallback rule
"lowest-index unknown pair". This selects pair index 1:

```text
A = input-belt-0 at (0, 11)
B = input-belt-1 at (0, 12)
```

The selection uses outcome category only, not timing or event counts, and supports observation of
that leaf only.

At the first branch after root propagation, verify all inherited and pair-fixed endpoint geometry
domains are singleton. Then record:

- the four remaining two-value facility pipe port and geometry domains;
- external boundary-terminal option cardinalities by network and material;
- per material, unresolved route cells, directed arcs, route arms, and arm-item support;
- flow variables with positive lower bound, zero upper bound, or unresolved bounds;
- facility-terminal and external-terminal routing-option selectors; and
- the decision family selected for the first branch.

The observer must be read-only and run separately from the uninstrumented five-second baseline.

## Artifacts

- Design: `docs/designs/phase3-representative-prior-input-port-pair.md`
- JSON and self-contained summary HTML:
  `/tmp/aic-phase3-prior-input-pair.gsiLz0/summary.json` and `summary.html`
- Sixteen automatically emitted standalone case HTML files in the same directory

## Independent Review And Verification

Three independent reviews checked the partition proof, implementation, artifact arithmetic,
schema provenance, report scope, and next observation strategy. A transient schema-field rename was
reverted so the current schema-v1 source and both JSON artifacts consistently record
`predeclared_observation_pair_index: 0`. All reviewers then returned PASS.

Focused tests preserve equal-port pairs in Cartesian enumeration and remove a residual value only
for `ProvenInfeasible`; unknown and validated-feasible values remain, while invalid witnesses block
interpretation. Final verification passes:

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
  --pair-prior-input-ports \
  --input-pair-case-time-limit-ms 5000 \
  --output-dir /tmp/aic-phase3-prior-input-pair.gsiLz0
```
