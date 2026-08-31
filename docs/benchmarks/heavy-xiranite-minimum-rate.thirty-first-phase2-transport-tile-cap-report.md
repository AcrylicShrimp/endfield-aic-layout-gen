# Heavy Xiranite Minimum-Rate Phase 2 Transport-Tile Cap Diagnosis

## Question

Does adding only a hard upper bound on physical belt and pipe tiles make the unchanged Phase 2
first-witness problem easier?

This experiment deliberately does not fix facility placement, rotation, facility ports, external
terminals, route geometry, topology, or flow. It keeps the existing exact `12x12` used-dimension
case and the preceding-phase non-binding placement hint. The capped cases add exactly one
constraint:

```text
physical_transport_tiles <= B
```

The physical count is the sum of unique occupied cells in the belt layer and unique occupied cells
in the pipe layer. Belt and pipe occupancy at the same `(x,y)` therefore counts as two physical
transport tiles, matching the existing secondary objective.

## Result

The cap alone does not collapse the Phase 2 first-witness cliff.

### Five-second screen

| Tile cap | Outcome | Build | Search | First witness | Variables | Constraints |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| none | unknown | 194 ms | 5,003 ms | none | 23,972 | 82,625 |
| 48 | unknown | 199 ms | 5,007 ms | none | 23,972 | 82,626 |
| 64 | unknown | 200 ms | 5,005 ms | none | 23,972 | 82,626 |
| 80 | unknown | 198 ms | 5,002 ms | none | 23,972 | 82,626 |
| 96 | unknown | 197 ms | 5,002 ms | none | 23,972 | 82,626 |

### Thirty-second confirmation

| Tile cap | Outcome | Build | Search | First witness |
| ---: | --- | ---: | ---: | ---: |
| none | unknown | 207 ms | 30,003 ms | none |
| 96 | unknown | 201 ms | 30,006 ms | none |

The earlier all-terminal reference ablation produced and validated a 96-tile witness in 6.636
seconds. That witness is also legal in the unrestricted model because removing diagnostic
placement and terminal equalities only restores choices. Therefore `B=96` is known to contain at
least one legal solution; its 30-second `unknown` result is not evidence of infeasibility.

## Interpretation

The experiment rejects the simple hypothesis that the solver is slow mainly because it explores
routes longer than the known useful range. A global cardinality cap removes such witnesses from the
feasible set, but it does not tell Pumpkin which route cells, directions, item identities, arcs,
flows, or topology components should form the compact witness. Those coupled low-level decisions
remain unresolved.

The cap is correctly connected to the existing objective state: capped cases have the same 23,972
variables as baseline and exactly one additional recorded constraint. Model construction time is
also unchanged within noise. The measured difference is therefore isolated to that one exact hard
bound.

## Consequence for binary search

Feasibility under `physical_transport_tiles <= B` is monotone:

- a feasible case supplies an upper bound;
- a proven-infeasible case eliminates that cap and every smaller cap.

That makes binary search or a parallel threshold portfolio logically valid and
completeness-preserving. It does not yet make the individual feasibility query cheap. Both the
baseline and the known-feasible `B=96` query time out, so binary search would currently organize a
set of hard `unknown` cases rather than solve the routing cliff.

The next diagnostic should retain the same cap while progressively removing already-localized
outer choices, beginning with the routing-only validated-reference case. If the cap helps there,
the cap is useful after an exact decomposition has broken placement and terminal coupling. If it
does not, the next target is stronger exact propagation inside shared routing state rather than
tile-count partitioning.

## Reproduction

Build and run the five-second screen:

```bash
cargo build --release -p aic-cli
target/release/aic-cli research diagnose-cumulative-transport-tile-caps \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.12x12.request.json \
  --target-phase 2 \
  --used-width 12 \
  --used-height 12 \
  --transport-tile-cap 48 \
  --transport-tile-cap 64 \
  --transport-tile-cap 80 \
  --transport-tile-cap 96 \
  --prefix-worker-count 4 \
  --prefix-case-time-limit-ms 5000 \
  --case-time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-phase2-transport-tile-caps-5s
```

The cases run sequentially so CPU contention cannot masquerade as a formulation improvement.

## Artifacts

- `heavy-xiranite-phase2-transport-tile-caps-5s/summary.json`
- `heavy-xiranite-phase2-transport-tile-caps-5s/summary.html`
- `heavy-xiranite-phase2-transport-tile-cap-96-30s/summary.json`
- `heavy-xiranite-phase2-transport-tile-cap-96-30s/summary.html`
- Per-case HTML outcome pages in both directories
