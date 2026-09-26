# Local closeout: hierarchical routing request allowance

## Question and identity

Does the Plan 02 implementation preserve truthful route and network termination while bounding local request work, memory and time? This closeout covers bounded local verification, not training or production-scale cap calibration. Code: `1a62fa5c6c5f4cef2133088696bb3b51b3850d8a`, clean worktree at CLI execution. Catalog: tracked `data/game/normalized`. No training server was accessed.

## Method and reproduction

The source panel is `docs/benchmarks/hierarchical-route-candidates/panel.json`. All generated inputs and outputs were under ignored `artifacts/plan02-budget-sensitivity/`. The default probe changes only `repetitions` from 500 to 1. The generous probe adds this complete allowance object to `candidate`:

```json
{
  "endpoint_visits": 100000000, "raster_cells": 100000000,
  "graph_rows": 100000000, "route_cells": 100000000,
  "blocked_visits": 100000000, "copied_bytes": 8589934592,
  "inference_elements": 1000000000, "expansions": 1000000000,
  "frontier_entries": 100000000, "routing_scans": 1000000000,
  "replay_route_cells": 1000000000, "replay_users": 100000000,
  "catalog_visits": 100000000, "validation_records": 100000000,
  "published_bytes": 1073741824, "peak_live_bytes": 8589934592,
  "max_candidates": 64, "max_variants": 64,
  "max_inference_batch": 8, "deadline_ms": 600000
}
```

From the generous panel, the work-limit and deadline probes keep only the first case and set `candidate.request_allowance.endpoint_visits=1` or `deadline_ms=1`, respectively. The missing-endpoint probe keeps only the first default case and only its first placement command. These transformations, the tracked source panel and the code commit reconstruct every input. The local command used for each panel was:

```sh
cargo run -q -p aic-cli --bin aic-cli --features outcomes -- learning route-candidate-harness \
  --panel artifacts/plan02-budget-sensitivity/NAME-panel.json \
  --catalog-root data/game/normalized \
  --output artifacts/plan02-budget-sensitivity/NAME-report-final.json
```

The missing-endpoint command is expected to exit 1 without publishing a report. The final default and generous report SHA-256 values were `824af739124588930f956d6c4f721bc099322b999df4df72c846580e776b3b2d` and `d767344ac23dac1006516c10a7757db9d8f34fbc6811a03c29465e6fe823d901`.

## Results and verdict

| Case | Default and generous result | Default peak live bytes |
| --- | --- | ---: |
| open-single | 6 candidates; `candidate-limit` | 40,997,626 |
| crossed-followup | 6 candidates; `candidate-limit` | 41,005,050 |
| shared-capacity-followup | 6 candidates; `candidate-limit` | 41,008,954 |
| sealed-frontier | 0 candidates; proven local `frontier-exhausted` | 40,992,704 |

The two settings produced 90 candidate samples total across one repetition and identical per-case outcomes; neither stopped on the request allowance. The endpoint-visit probe returned `work-limit`, category `endpoint-visits`, limit and used count 1, and peak 39,936 bytes. The one-millisecond probe returned `deadline` with 0 ms observed overshoot in this run. The malformed stable request returned `constructive.unplaced-endpoint [target]` rather than an absent route demand. Unit and integrated cases additionally cover network no-compatible endpoints, zero-candidate search-count and expansion cutoffs, candidate publication after later exhaustion, adaptive memory-page retry and more than 32 request stops. Final checks: `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace`, `cargo test -p aic-learning --features libtorch`, tracked-file worktree/index checks and independent implementation review round 14 (`PASS_WITH_NOTES`).

Verdict: **Plan 02 contract verified locally.** These cases contain at most nine facilities. They do not show that default request caps or aggregate inference-batch memory admit medium or large production graphs. Plan 08 must measure those limits and actual model/optimizer exposure by scale and pattern before a training-readiness verdict.

## Cleanup

Before cleanup, ignored `artifacts/plan02-budget-sensitivity/` occupied 2,728 KiB and the filesystem reported 25,263,240 KiB available. The generated default, generous, work-limit, deadline and missing-endpoint panels, their final reports and earlier `v1`–`v5` candidate reports were removed with the whole Plan 02 artifact directory after checking its file list. Available space afterward was 25,263,332 KiB. Three sampled distribution panels (`canonical-1-3`, `production-4-64`, `production-65-256`; 2,436 KiB together) were moved to ignored `artifacts/plan08-distribution-inputs/` for the active Plan 08 scale audit. The local LibTorch runtime under `artifacts/local-libtorch/` is a test dependency, not an experiment checkpoint. No non-replayable checkpoint or raw training state was produced.
