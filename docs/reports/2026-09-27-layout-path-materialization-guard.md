# Shared layout path admission

## Question and boundary

Plan [20](../plans/2026-09-27.20-layout-path-materialization-guard.md) asks whether two independent 250,000-cell path guards reject otherwise legal large layouts. The compiled draft checks structure and aggregate sizes; the independent validator checks exact path geometry. Both already have a 2,000,000-cell **aggregate** path guard. This change removes only the separate per-path rejection and leaves the aggregate, entity, footprint, coordinate, span and catalog checks at their existing owners. The physical editor's separate `check_path` limit belongs to Plan .17.

The accepted production graph sampler declares at most 256 facilities, 512 source nodes and depth 64 in `docs/benchmarks/direct-constructive-full-graphs/sampling.json`. The earlier upper-range graph probe requested at most 1,024 terminals and 2,048 connections; those were probe inputs, **not declared production maxima**. Plan .08 has not yet versioned required maxima for terminals, connections, occupied cells, route cells, coordinates or physical bounding-box area by production use case. Accordingly, this step establishes a provisional *admission* envelope, not final broad-distribution readiness. Plan .08 must declare the required envelope before using any current finite guard as evidence that all intended cases fit. A required legal band beyond one of these guards needs a measured change at its owner, not a smaller substitute sample.

## Guard inventory and resource basis

| Owner | Remaining finite boundary | Basis and observed headroom |
| --- | --- | --- |
| Compiled editor | Embedded facility footprint area ≤2,000,000; component footprint exactly 1×1 | Limits body materialization from external compiled definitions; observed generated catalog footprints are far smaller. The 1×1 rule is the compiled component representation contract. |
| Compiled editor | Body cells ≤2,000,000; node/component reference counts ≤2,000,000 each; **total** path cells ≤2,000,000 | Bounds separately materialized body, relationship and path vectors. The 256-facility observed layout has 5,299 body cells and 52,402 listed route cells. |
| Compiled editor | Each and combined compiled-entity vector count ≤250,000; embedded ports ≤250,000; occupied-axis span ≤1,000,000 | Bounds vector/map and coordinate arithmetic before publication. The graph probe's 256 facilities, 1,024-terminal and 2,048-connection request is far below the count ceiling; the latter two are not final product maxima. |
| Independent validator | Blocks ≤4,096; terminals, connections, bridges and junctions ≤32,768 each | Finite external map/set growth and catalog enumeration. Observed 256-facility layout: 256 blocks, 0 terminals, 383 connections and 1,245 bridges. |
| Independent validator | Facility footprint width and height ≤1,000 each, area ≤10,000; body cells ≤2,000,000; **total** route cells ≤2,000,000 | Limits body iteration and retained route-site maps. At 250,001 cells on one legal path, measured peak live validator reservation was 62,130,079 bytes. Plan .00 separately measured 471,156,818 bytes of live reservation at 1.98 million straight route cells, including completed-layout materialization and validation. One-shot `validate()` grants its cursor reservations unconditionally, so the finite external guards remain necessary. |
| Independent validator | Absolute x/y coordinates 0–1,000,000 | Keeps external geometry and bounding arithmetic finite. The compiled editor's occupied-span rule is separate and does not imply this absolute range. |

The validator reports bounding-box **area** as a metric; it does not reject an area above 2,000,000. The legal 250,001-cell route below has a 1,000,004-cell bounding box because its height is four cells. No new per-sample distribution check or hot-loop validation was added.

## Observed physical cases and measurement

The 30-facility Tree Spacious seed 43000 was generated with the committed 100-facility config changed only to `facilities: [30,30]`; its config/layout SHA-256 values were `b70040226169135fa3866b347cb52cba2a66b60465b0f73e479b0a2331e2d653` / `030445f28948af7d27d082365696a43b639cb54406317354d94dfba1d41edd12`. The 100-facility Tree Spacious seed 43000 and 256-facility Branching Spacious seed 43001 were restored from Plan .22's retained archive. Their layout SHA-256 values were `094f2beec0d925a1fda86e6930685138e8a2c8d7cd9d10e0d86b6374784902a4` and `5b4879631e7b78059da0c10c7c817a1f75ca3363a83e17f72f2a51f42ba798b3`. Measurements below are single local debug-test observations with page credit 4,096; the RSS is the whole already-built test process, not solely the validator. Listed route cells count per-route materialized points; `transport_cells` counts unique occupied transport sites.

| Case | Connections | Bounding area | Body / listed route / unique transport cells | Longest route | Validation visits / pages | Peak live reservation | One-shot / resumed validation | Process peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 30 facilities | 29 | 7,826 | 687 / 1,815 / 1,752 | 122 | 9,058 / 2 | 322,848 B | 6.29 / 6.26 ms | 56.2 MB |
| 100 facilities | 99 | 54,054 | 2,242 / 12,126 / 11,893 | 462 | 54,592 / 13 | 2,172,323 B | 38.77 / 39.02 ms | 57.0 MB |
| 256 facilities | 383 | 124,254 | 5,299 / 52,402 / 51,157 | 683 | 229,550 / 56 | 8,673,422 B | 162.70 / 163.69 ms | 56.5 MB |
| One legal long route | 1 | 1,000,004 | 9 / 250,001 / 250,001 | 250,001 | 1,000,037 / 244 | 62,130,079 B | 847.34 / 838.32 ms | 90.6 MB |

The one-shot and resumed variants returned identical metrics and visit totals for every case. Re-enabling the old guard in the same working tree for a normal-size control gave 100-facility resumed validation 38.80 ms and 256-facility 164.36 ms, versus 39.02 and 163.69 ms with the guard removed. These single-run differences are noise-sized; the new code removes one length comparison per route and adds no per-cell work. The large-route measurement demonstrates finite memory admission at this point, not a promise that arbitrary larger external input fits.

## Verification and reproduction

The compiled library fixture admits 250,001 structurally valid path cells and still rejects an empty path. The independent validator accepts a 250,001-cell straight route from a catalog facility to a boundary terminal, both one-shot and over 244 pages. Short variants with a repeated point, nonadjacent step, negative coordinate and missing facility definition are rejected. The existing physical-editor `procedural_sources_round_trip_through_the_common_boundary` fixture passed, and the real `aic-layout-gen validate` CLI returned `physical_validated=true` for the long route and retained 256-facility layout. `cargo fmt --all`, `cargo check --workspace --quiet`, `cargo test --workspace --quiet` and the tracked-file worktree check passed after the guard and test changes.

The long CLI fixture is generated by its Rust test and can be recreated without retaining a large JSON file:

```sh
mkdir -p target/plan20-long-path
PLAN20_WRITE_LAYOUT=target/plan20-long-path/generated-fixture.json \
  cargo test -p aic-layout-gen long_legal_route_matches_one_shot_and_resumed_validation
cargo run -p aic-layout-gen -- validate \
  --layout target/plan20-long-path/generated-fixture.json \
  --catalog-dir data/game/normalized
PLAN20_LAYOUT=target/plan20-long-path/generated-fixture.json \
  cargo test -p aic-layout-gen layout_validation_calibration -- --ignored --nocapture
```

The 30/100/256 cases can be passed to the same ignored calibration test with `PLAN20_LAYOUT` set to their extracted layout JSON paths. Its output includes exact metrics, retained-page visits, reservation peak and elapsed times. Plan .22's retained archive is `artifacts/plan22-scale-20260927/retained-results.tar.gz`; the 100 and 256 archive members are `generated-100-tree-spacious/layout-00000.json` and `generated-v4-256-branching-spacious-43001/layout-00000.json`.

The 30-facility case is reproduced by changing only `facilities` in the tracked `configs/generate-100-tree-spacious.json`, then running `aic-layout-gen generate --config <derived-config> --catalog-dir data/game/normalized --seed 43000 --count 1 --workers 1 --output <fresh-dir>`. The derived config was written with Python `json.dumps(config, indent=2) + "\n"`; its hash above pins the exact bytes.

## Verdict and follow-on

The shared path guard removal admits the measured legal long path while preserving finite aggregate admission and independent geometry rejection. Final production scale remains unproven until Plan .08 declares and verifies the physical envelope. Plan .17 still owns the physical editor's separate path limit and its integrated long-route test. Independent implementation review `/root/review_plan20_implementation_r1` returned `PASS_WITH_NOTES` with no code blocker; its notes on final verification and 30-facility provenance are addressed above. The shared validation foundation is commit `1cdf01f4`, and this guard/test change is commit `3dc7a786`. The disposable generated layouts and comparison worktree were removed after retaining their hashes, measurements and reproduction commands; [cleanup details](2026-09-27-layout-path-guard-cleanup.json) record the paths and disk space.
