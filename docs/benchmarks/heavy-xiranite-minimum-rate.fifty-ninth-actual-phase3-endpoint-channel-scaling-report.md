# Actual Phase 3 Endpoint-Channel Scaling Report

## Question

The controlled endpoint probe showed that the current nested-Element channel can preserve the
complete endpoint relation while failing to propagate interior domain holes. This experiment
scales that comparison to the actual facility introduced by Heavy Xiranite cumulative Phase 3,
without constructing the routing grid.

The diagnostic compares two exact encodings of the same complete relation:

1. the current nested placement-to-port-geometry and port-to-selected-geometry Elements; and
2. one positive allowed-tuple table per logical terminal, sharing the same placement variable.

This is a channel-only diagnostic. It does not change the authoritative joint solver.

## Fair starting domain

The 5 by 5 facility has 576 footprint-fitting `(x, y, rotation)` candidates in a 16 by 16 used
dimension. Ninety-six candidates cannot expose a legal endpoint for every one of the facility's
four logical terminals. They are exact game-rule contradictions, not heuristic reductions.

The first draft exposed an unfair comparison: nested Element removed some of these values during
root propagation while the positive table did not eagerly remove values absent from its projection.
The final diagnostic analytically projects both encodings to the identical 480-value placement
domain before measuring them. Terminal geometry domains are also the same sparse projections.

## Actual relation scale

| Terminal | Compatible ports | Reachable geometry keys | Legal tuple rows |
|---|---:|---:|---:|
| Pipe input | 1 | 480 | 480 |
| Belt input A | 5 | 600 | 2,200 |
| Belt input B | 5 | 600 | 2,200 |
| Belt output | 5 | 656 | 2,400 |
| **Total** | **16** | - | **7,280** |

The production-style bounded geometry key range would contain 1,024 values per terminal. The probe
uses only reachable keys in both encodings so the result measures propagation rather than ghost
domain values.

## Root propagation result

| Exact restriction | Nested Element placement | Positive table placement | Same oracle fixpoint? |
|---|---:|---:|---|
| Placement and one legal port fixed | 1 | 1 | Yes |
| One interior geometry key removed with port fixed | 480 | 479 | No |
| One world-direction class retained | 480 | 120 | No |
| Every geometry supporting one placement removed | 480 | 479 | No |
| One placement removed with port fixed | 479 | 479 | No; endpoint domains differ |
| Two terminals restricted to disjoint spatial supports | Root conflict | Root conflict | Yes for contradiction |

The fixed complete state is the only non-conflicting case where the current channel reaches the
same fixpoint as the table. This matches the earlier integrated evidence: manually fixing complete
facility endpoint geometry makes the residual routing problem easy, while partial endpoint facts
do not travel back through the current channel.

The direction restriction is the clearest actual-scale result:

```text
Nested Element: 480 placement values remain
Positive table:  120 placement values remain
```

The last-support restriction is equally decisive. Removing all five endpoint geometries that can
support one placement leaves that placement in the nested-Element domain, while the table removes
it immediately.

## Cost

Each encoding ran as an isolated release process under `/usr/bin/time -l`.

| Metric | Nested Element | Positive table | Ratio |
|---|---:|---:|---:|
| Authored integer variables | 25 | 9 | 0.36x |
| Element constraints | 20 | 0 | - |
| Table constraints | 0 | 4 | - |
| Hidden table row literals | 0 | 7,280 | - |
| Estimated table clauses | 0 | 26,116 | - |
| Representative model build | 0.902 ms | 34.086 ms | 37.8x |
| Maximum RSS | 13,172,736 bytes | 46,481,408 bytes | 3.53x |
| Retired instructions | 246,039,279 | 3,847,984,607 | 15.64x |
| CPU cycles | 60,185,070 | 781,931,306 | 12.99x |

The standalone shell wall times are not used for ranking because the nested process spent most of
its observed 0.63 seconds outside measured CPU work. Build time, RSS, retired instructions, and
cycles consistently show that the table is substantially more expensive.

No search was performed, so branch decisions, backtracks, conflicts, learned clauses, and solver
propagations are explicitly zero in the structured reports. Those metrics become mandatory in the
next integrated comparison.

## Root cause

The next confirmed coupling cliff is the current nested-Element endpoint channel:

```text
partial route endpoint facts
  -> interior holes in physical geometry domains
  -X-> logical port and shared placement pruning
```

The exact relation exists, but Pumpkin's Element filtering does not provide the needed generalized
support propagation for these sparse interior-domain changes. The solver therefore keeps exploring
facility placements and rotations whose final compatible route endpoint has already disappeared.

The result does not prove that endpoint channeling is the only Phase 3 blocker. It proves that the
actual Phase 3 relation contains a large, measured propagation gap of exactly the kind implicated by
the preceding fixed-state portfolios.

## Recommended next action

Run one separate production-faithful research comparison that replaces only the facility endpoint
channel with positive tables while leaving placement, port choice, routing, flow, occupancy,
capacity, topology, objectives, dimensions, and the five-second per-candidate budget unchanged.

The standard table should be tested before implementing a custom propagator because it is already
exact, proof-integrated, and demonstrably stronger. Record full model build time, peak RSS, first
incumbent, decisions, backtracks, conflicts, learned clauses, solver propagations, and final
validation.

- If Phase 3 first-feasible behavior improves materially within acceptable memory, continue
  optimizing or factoring the table representation.
- If table construction or RSS becomes the new cliff, build a sparse semantic support propagator
  that targets the measured direction and last-support gaps without one hidden literal per tuple.
- If search remains unchanged despite stronger root propagation, endpoint channeling is real but
  not the dominant integrated cliff; return to the residual routing hierarchy.

This is the user decision gate. No production endpoint cutover was made in this slice.

## Artifacts

- `/tmp/aic-scaled-endpoint-final.bZMe0M/nested.json`
- `/tmp/aic-scaled-endpoint-final.bZMe0M/nested.html`
- `/tmp/aic-scaled-endpoint-final.bZMe0M/nested.time.txt`
- `/tmp/aic-scaled-endpoint-final.bZMe0M/table.json`
- `/tmp/aic-scaled-endpoint-final.bZMe0M/table.html`
- `/tmp/aic-scaled-endpoint-final.bZMe0M/table.time.txt`

## Verification

```text
cargo fmt --all
cargo test -p aic-data scaled_endpoint_channel_probe -- --nocapture
cargo test -p aic-cli parses_scaled_endpoint_channel_probe
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```

The final workspace test run passed 31 CLI tests and 245 data-library tests.
