# Procedural routed layout experiment

## Result

The standalone generator completed and physically validated all 224 requested
seeds across seven profiles, including a profile without bridges and a mixed
belt/pipe DAG. It produced varied nonoverlapping placement, actual directed routes
and catalog crossings. The bounded experiment supports using it as a synthetic
physical-layout source. It does not establish recipe-balanced factories, uniform
sampling, optimality, player-distribution similarity or pretraining benefit.

Implementation follows the [accepted design](../designs/2026-09-15.00-procedural-layout-generation.md).
Three responsibilities now have separate owners: `aic-layout` for common physical
data/validation, `aic-routing` for the existing native kernel, and `aic-layout-gen`
for complete procedural generation. Catalogs, exact rates, geometry, projection,
RNG and routing are shared rather than duplicated. Existing native factory tests
exercise the extracted implementation through their original callers.

## Reproduction and provenance

Measured on Apple M4, arm64 macOS, Rust 1.98.0, release mode, four workers. Each
profile requested 32 consecutive seeds. Configuration is tracked under
`crates/aic-layout-gen/examples/`; runtime assets remain in ignored
`artifacts/procedural-layout-generator/matrix/PROFILE/`.

| Profile | First seed | Requested block range |
| --- | ---: | ---: |
| default | 2000 | 12–20 |
| small | 3000 | 8–12 |
| large | 4000 | 32–48 |
| compact | 5000 | 12–20 |
| spacious | 6000 | 12–20 |
| no-bridges | 7000 | 8–12 |
| mixed-dag | 8000 | 16–24 |

```sh
cargo build --release -p aic-layout-gen
/usr/bin/time -l target/release/aic-layout-gen generate \
  --config crates/aic-layout-gen/examples/default.json \
  --catalog-dir data/game/normalized --seed 2000 --count 32 --workers 4 \
  --output artifacts/procedural-layout-generator/reproduction/default
```

Substitute the profile and first seed from the table; use a new output directory.
Inputs are `data/game/normalized/{facilities,transports,logistics-components}.json`,
snapshotted once in each batch. The batch catalog SHA-256 is
`6e483b5b903fe759008823f50dfe55d6e9a7b3d8d36978864fb292a76d5684de`.
Reports retain actual per-layout SHA-256 values, metrics, work counters, all attempt
seeds and statuses. Layout JSON files retain the exact accepted problem/solution.
No raw benchmark outputs need to be tracked in Git.

## Completion and cost

| Profile | Complete | Attempts | Batch seconds | Layouts/s | Peak RSS MiB | Mean validation ms | Mean publication ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| default | 32/32 | 32 | 0.302 | 106.00 | 27.3 | 0.188 | 0.839 |
| small | 32/32 | 32 | 0.039 | 818.42 | 17.2 | 0.082 | 0.547 |
| large | 32/32 | 112 | 14.092 | 2.27 | 48.6 | 0.473 | 1.650 |
| compact | 32/32 | 34 | 0.702 | 45.55 | 25.1 | 0.151 | 0.818 |
| spacious | 32/32 | 34 | 0.378 | 84.63 | 35.6 | 0.189 | 0.936 |
| no-bridges | 32/32 | 32 | 0.088 | 363.05 | 14.1 | 0.068 | 0.672 |
| mixed-dag | 32/32 | 40 | 0.837 | 38.23 | 28.2 | 0.213 | 0.949 |

Batch time includes catalog loading, attempts, final independent validation,
layout serialization/hashing, previews, per-case metadata and gallery publication;
it excludes the final report write and process startup. Per-case publication
timing excludes its final metadata write, which remains included in batch time.
Peak RSS comes from `/usr/bin/time -l`. These are single bounded measurements,
not repeated statistical estimates. Short-profile throughput is particularly
sensitive to startup and filesystem noise.

There were 316 attempts in total: 224 completed and 92 rejected. Of the rejected
attempts, 91 exhausted route work and one exhausted pair trials. No rejected attempt
published a physical layout. No seed exhausted its complete retry allowance.

Large cases expose clear selection bias: requested sizes spanned 32–48, but accepted
sizes were only 32–40. Of 112 attempts, 40 sampled sizes 41–48 and none completed.
Accepted counts by size were `32:3, 33:7, 34:9, 35:4, 36:3, 37:3, 38:1, 40:2`.
This profile is useful evidence of current scaling limits; its 32/32 seed completion
must not be interpreted as successful generation throughout the requested range.
Use fixed-size requests and report rejection rates when that distribution matters.

## Geometry and graph observations

Each profile had 32 distinct serialized problems and 32 distinct solutions. This
detects exact duplicates, not graph isomorphism or semantic diversity. Every
accepted graph was weakly connected and met its configured degree, density and
branch/merge minima. Mixed-DAG results met both transport quotas.

| Profile | Mean occupancy | Mean branch blocks | Mean merge blocks | Mean bridges | Mean detour | Distant edge share | Width/height range |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| default | 0.508 | 7.13 | 6.75 | 40.00 | 1.265 | 0.490 | 0.57–1.50 |
| small | 0.499 | 4.09 | 3.91 | 17.34 | 1.209 | 0.518 | 0.45–1.90 |
| large | 0.554 | 15.03 | 14.63 | 110.72 | 1.217 | 0.474 | 0.70–1.39 |
| compact | 0.624 | 7.38 | 7.06 | 23.94 | 1.426 | 0.448 | 0.57–1.76 |
| spacious | 0.317 | 6.97 | 6.78 | 36.16 | 1.106 | 0.341 | 0.53–1.59 |
| no-bridges | 0.467 | 3.34 | 3.09 | 0.00 | 1.415 | 0.321 | 0.54–1.85 |
| mixed-dag | 0.524 | 8.28 | 8.22 | 49.38 | 1.262 | 0.515 | 0.65–1.72 |

Occupancy is the union of body and transport coordinates divided by the used
bounding rectangle; overlapping belt/pipe layers count once in this ratio.
Detour averages path-step length divided by endpoint Manhattan distance (minimum
denominator one). Distant share is accepted edges above the per-attempt 75th
percentile block-origin distance threshold, pooled over accepted layouts.
Branch/merge means describe graph nodes, not logistics junction components.

The ordinary first seed from default, compact, spacious, no-bridges and large was
rendered and inspected. Different footprints, rotations, irregular gaps, branching,
long links and routed crossings are visible. All 224 SVGs parsed; scripts in all
231 HTML pages parsed and local links resolved. Browser interaction was not
automated. The generator still shows A* rectilinear preferences and can place many
bridges; visual appeal alone is not a metric of area or transport optimality.

## Continuity and checks

The fixed routing comparison runs 200 identical 64×64 queries. Before extraction
the native kernel took 266.346 ms; after extraction the shared kernel took
262.222 ms. Both expanded exactly 998,819 states and returned 25,020 path steps.
This supports preserved behavior and comparable cost, not a speedup claim.

```sh
cargo test --release -p aic-routing route_kernel_timing -- --ignored --nocapture
cargo fmt --all
cargo check --workspace
cargo test --workspace
node docs/maintenance/check-tracked-files.mjs --worktree
```

Workspace checks and tests pass. New coverage includes 80 varied sequence-pair
packings, seed determinism, completion on the final allowed trial, independent
rejection of corrupted routes/ports/bridges/capacities/identities, extreme integer
input rejection, bounded exhaustion, DAG/transport targets, persisted CLI
validation and identical layout bytes with one versus two workers. Existing native
generation, editing, ownership and restore tests remain active. Expensive ignored
research audits are not added to normal execution.

No PPO training was started. The next pretraining decision needs a shared-input
admission design and a comparison against existing training/evaluation behavior.
