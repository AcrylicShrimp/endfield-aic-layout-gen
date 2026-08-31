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

The seventh checkpoint, `heavy-xiranite-minimum-rate.seventh-route-state-cliff-report.md`, rebuilds
clean network subsets, locates the first cliff at two simultaneously free route-state spaces, and
records the exact cancellation dominance rule for co-located terminals.

The eighth checkpoint, `heavy-xiranite-minimum-rate.eighth-shared-transport-layer-report.md`,
compares the dense per-network routing grid with an exact shared belt/pipe layer formulation. The
shared formulation reduces the recorded 12 by 12 phase-zero model substantially but does not find
a first incumbent within five or fifteen seconds.

The ninth checkpoint, `heavy-xiranite-minimum-rate.ninth-factored-endpoint-report.md`, replaces
flattened placement-times-port endpoint Booleans with exact independent port choices and
solver-native element geometry. It measures endpoint state, placement-routing coupling, build
time, and peak RSS reductions, while recording that the first-incumbent cliff remains.

The tenth checkpoint, `heavy-xiranite-minimum-rate.tenth-factored-network-decomposition-report.md`,
rebuilds every single, pair, and full phase-zero network subset under the factored shared-layer
formulation. It locates the earliest validated-output boundary inside the two-requirement Xiranite
Powder network and separates that boundary from the later one-layer versus two-layer composition
growth.

The eleventh checkpoint, `heavy-xiranite-minimum-rate.eleventh-requirement-decomposition-report.md`,
rebuilds the hard Xiranite Powder network from each logical requirement. It shows that both
requirements are individually tractable and that the first local cliff is the second terminal
pair's placement-port-to-grid coupling, not additional routing-grid state.

The twenty-fourth checkpoint,
`heavy-xiranite-minimum-rate.twenty-fourth-full-phase0-dimension-sweep-report.md`, applies the
parallel exact dimension portfolio to all three phase-zero networks. It proves minimum used area
42 in about 0.8 seconds after the same current free-dimension formulation finds no incumbent in
five seconds, and identifies the used-dimension/boundary-terminal/routing propagation cycle as the
closed phase-zero cliff.

The twenty-fifth checkpoint,
`heavy-xiranite-minimum-rate.twenty-fifth-cumulative-dimension-growth-phase1-report.md`, grows the
same exact portfolio through cumulative SCC phase 1 with a non-binding prior placement hint. It
proves minimum area 77 for the two-facility, six-network graph in 3.65 seconds and moves the next
first-feasible cliff to phase 2 or later.

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
