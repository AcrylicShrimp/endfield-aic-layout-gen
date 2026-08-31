# Phase 3 Prior-State-Fixed Facility Portfolio

## Question

The preceding residual-state experiment showed that one introduced-facility state changed from a
five-second timeout to a 150 ms infeasibility proof only after the validated Phase 2 facility ports
were fixed. This slice tests all 125 introduced-facility port assignments and all four rotations at
the same coordinate while retaining that preceding facility state.

The exact diagnostic fixes:

- used dimensions to 16x16;
- the new 5x5 facility coordinate to `(8,5)`;
- the three placements from the validated Phase 2 incumbent; and
- the 12 matching Phase 2 facility-terminal port choices.

It then enumerates 500 independent exact cases: 125 complete port assignments for the introduced
facility times four legal rotations. Belt and pipe routing, item assignment, topology, capacity,
flow, and logistics-component decisions remain solver decisions. These diagnostic equalities can
exclude other legal Phase 3 layouts, so the result does not prove global Phase 3 infeasibility.

## Result

| Metric | Value |
|---|---:|
| Exact cases | 500 |
| Workers | 12 |
| Validated feasible | 0 |
| Proven infeasible | 500 |
| Unknown at five seconds | 0 |
| Search time | 94--597 ms |
| Median search time | 200 ms |
| Mean search time | 214.65 ms |
| 95th percentile search time | 349 ms |
| Outer wall time | 44,926 ms |

Every case keeps the same 48,967-variable model. The 15 diagnostic equalities increase the
constraint count from 163,878 to 163,893.

Across all 500 cases, Pumpkin records:

| Search metric | Value |
|---|---:|
| Branch decisions | 189,580 |
| Backtracks | 11,720 |
| Conflicts | 12,220 |
| Learned clauses | 12,220 |
| Solver propagations | 169,781,298 |

Exactly 100 cases require zero branch decisions and reject during root propagation. They are the 25
assignments in which both belt demands select the same input port, under all four rotations. The
remaining 400 cases still prove infeasible rather than timing out.

Rotation 270 degrees is measurably harder for this fixed geometry:

| Rotation | Mean search | Mean decisions | Root-only cases |
|---:|---:|---:|---:|
| 0 | 193.90 ms | 198.40 | 25 |
| 90 | 193.38 ms | 198.40 | 25 |
| 180 | 191.18 ms | 198.40 | 25 |
| 270 | 280.13 ms | 921.44 | 25 |

## Interpretation

The validated Phase 2 facility state cannot be extended by placing the introduced facility at
`(8,5)`, regardless of that facility's legal port assignment or rotation. This conclusion applies
only to the restricted extension state.

More importantly for the cliff diagnosis, making complete facility endpoint geometry visible
changes the search qualitatively:

| Facility state exposed to the model | Result |
|---|---|
| Introduced coordinate, ports, and rotation only | Most cases remain unknown after 5 s |
| Plus prior placements only | Selected case remains unknown after 5 s |
| Plus prior placements and prior facility ports | All 500 cases prove infeasible in at most 597 ms |

The residual routing model is therefore capable of strong propagation once every facility
endpoint's physical cell and world direction are known. The dominant cliff is not merely the size
of the routing grid. It is the weakly exposed disjunction connecting facility placement, rotation,
logical port selection, physical endpoint geometry, and route endpoint state.

The current encoding represents placement as one integer candidate index containing `(x, y,
rotation)`. Each compatible physical port uses a constant-element constraint to map that placement
index to a packed `cell * 4 + direction` geometry key. A variable-element constraint then selects
one of those per-port keys using the logical port-choice variable. There are no explicit `x`, `y`,
`rotation`, endpoint-cell, or endpoint-world-direction domains for routing deductions to prune
directly.

This encoding is semantically exact, but the experiment indicates that its bidirectional
propagation deserves direct measurement and reformulation research.

## Next exact research

Compare exact endpoint-channel formulations that preserve every legal placement and route:

1. the current nested element chain;
2. explicit `x`, `y`, and `rotation` variables channelled to the placement candidate;
3. explicit endpoint cell and world-direction variables channelled to placement plus port choice;
4. an allowed-tuple or support-literal channel over placement, port choice, cell, and direction; and
5. a dedicated semantic propagator if standard constraints cannot provide strong bidirectional
   filtering cheaply.

For each formulation, measure root-domain pruning in both directions before comparing five-second
first-feasible performance. In particular:

- fix or remove one `x`, `y`, or rotation value and count removed placement candidates and endpoint
  geometries;
- fix a world direction or endpoint cell and count removed physical ports, logical port choices,
  rotations, and placements;
- remove placement candidates and observe the resulting coordinate, rotation, port, and routing
  endpoint domains; and
- record model size, construction time, branch decisions, backtracks, conflicts, learned clauses,
  solver propagations, and wall time.

Independent reviewers should separately assess proof soundness, Pumpkin-specific propagation,
alternative exact representations, hidden memory/build costs, and counterexamples before a
reformulation is selected.

## Artifacts

- `heavy-xiranite-phase3-prior-state-fixed-portfolio-x8-y5-5s/summary.html`
- `heavy-xiranite-phase3-prior-state-fixed-portfolio-x8-y5-5s/representative-layout.html`
- raw JSON archived outside the repository at `/tmp/aic-prior-state-portfolio.nqvAqX/summary.json`

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```

All 28 CLI tests and 217 data-library tests passed before the release experiment. The CLI emitted
the structured HTML report and representative rejected layout automatically.
