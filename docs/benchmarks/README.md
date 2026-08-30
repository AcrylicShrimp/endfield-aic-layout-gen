# Layout Benchmark Artifacts

This directory stores normalized diagnostic measurements, not generated layouts or expected solver answers.

## Heavy Xiranite Known-Bad Baseline

`heavy-xiranite-forge.iterative-scc.known-bad.json` records the behavior before the incremental optimizer cutover. Its `baseline_status` is `known-bad-diagnostic-only`. Tests and later implementations must not preserve its coordinates, perimeter routes, or score.

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

The one-facility fixture intentionally demonstrates the obsolete perimeter-terminal behavior. The external-terminal cutover must replace that assertion with the new minimal dangling-connection contract rather than keep it as a compatibility test.

The first captured phase contains one facility but uses 53 logical route cells: 40 belt cells and 13 pipe cells. The final phase uses 10,338 logical route cells and 523 bridge components. These values are comparison evidence for later slices, not MVP acceptance targets.
