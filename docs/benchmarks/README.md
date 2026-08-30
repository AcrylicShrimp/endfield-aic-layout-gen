# Layout Benchmark Artifacts

This directory stores normalized diagnostic measurements, not generated layouts or expected solver answers.

## Heavy Xiranite Known-Bad Baseline

`heavy-xiranite-forge.iterative-scc.known-bad.json` records the behavior before the incremental optimizer cutover. Its `baseline_status` is `known-bad-diagnostic-only`. Tests and later implementations must not preserve its coordinates, perimeter routes, or score.

The Heavy Xiranite research checkpoints are documented in
`heavy-xiranite-minimum-rate.first-search-space-report.md` and
`heavy-xiranite-minimum-rate.second-model-structure-report.md`. The third checkpoint,
`heavy-xiranite-minimum-rate.third-bound-sensitivity-report.md`, compares the same exact first SCC
phase under seven square request ceilings. These bounds are diagnostic request ceilings only, not
project defaults or canonical game limits.

The fourth checkpoint, `heavy-xiranite-minimum-rate.fourth-first-incumbent-ablation-report.md`,
uses a temporary release-only ablation build to identify the smallest tested model feature that
crosses the five-second first-solution budget. None of its relaxed variants are production solver
paths.

The fifth checkpoint,
`heavy-xiranite-minimum-rate.fifth-circulation-permitted-remeasurement-report.md`, repeats the
bound series after route acyclicity proof was removed. It includes the current model-family and
first-incumbent breakdowns and identifies the one-to-two-network transition as the next measured
research boundary.

The sixth checkpoint,
`heavy-xiranite-minimum-rate.sixth-network-interaction-decomposition-report.md`, isolates all three
phase-zero networks and pairs. It separates placement-port coupling from multiplicative routing
state and records the newly observed mismatch between circulation-permitted solving and witness
validation.

Regenerate it from a release binary with:

```bash
python3 tools/capture_iterative_scc_baseline.py \
  --output docs/benchmarks/heavy-xiranite-forge.iterative-scc.known-bad.json
```

The capture tool builds `aic-cli` before starting the timer, executes the exact command stored in the artifact, and commits only normalized metrics. It records input hashes, the source commit, host description, search bounds, independently measured used geometry, logical route cells, route edges, unique route tiles, turns, components, zero-flow routes, and solve time for every phase.

The baseline fixture contract is split across:

- `crates/aic-data/tests/iterative_scc_baseline.rs` for deterministic chain, branch, cycle, and one-facility graphs;
- `tools/tests/test_capture_iterative_scc_baseline.py` for metric normalization;
- the normalized JSON artifact for the release Heavy Xiranite observation.

The one-facility fixture now prevents regression to the captured behavior: every external route is one cell, the canonical witness is identical under 50 by 50 and 500 by 500 search bounds, and reported bounds describe used geometry only.

The first captured phase contains one facility but uses 53 logical route cells: 40 belt cells and 13 pipe cells. The final phase uses 10,338 logical route cells and 523 bridge components. These values are comparison evidence for later slices, not MVP acceptance targets.
