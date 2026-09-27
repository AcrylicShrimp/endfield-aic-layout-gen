# Plan .14 one-shot routing cutover: bounded server comparison

## Question and method

Does replacing `aic_routing::shortest_path`'s duplicate A* with the prepared graph remove its fixed 250,000-cell and one-million-state barriers without changing legal routes or imposing a severe ordinary-call cost? This is a release-mode comparison of the whole public call, including input admission, reverse probe, forward search and result construction. It is a kernel comparison, not evidence that the broad factory distribution or downstream caller budgets are ready.

The two deterministic fixtures in `docs/benchmarks/route-one-shot/one_shot_probe.rs` cover a 128×128 blocked/penalized grid and a 300×300 blocked/penalized grid. Each invocation runs each case nine times, discards the first time and reports the fifth sorted time of eight measured calls plus status, path hash and work counters. Five separate invocations of `docs/benchmarks/route-one-shot/run_mem.py` were made for each source in fresh SSH commands after compilation; the table uses the median of those five outputs. `run_mem.py` uses child `ru_maxrss`, so the RSS value covers the process running both cases. Compilation and benchmarking were separate SSH commands to exclude compiler child accounting from that peak. The runs used `ashrimp-arch` (Linux x86_64, kernel 7.2.7), release Cargo profile, no GPU training.

## Source and reproduction

- Git parent at experiment start: `b76c4bea9bfbe3a10998df221e6bfab9e8040ac4`.
- Old source: the exact prior worktree overlay `artifacts/plan01-placed-input-20260927/server-export/current-measured-source.tar.gz`, SHA-256 `48064c29f7aae43af0e4d0b3edb3c729cfaaf95271fc0832bfa5be4b2feb6ec3`, applied to a `git archive` of that parent. It includes the already implemented prepared-search work while retaining the old public one-shot A*.
- New source: `artifacts/plan14-one-shot-20260927/current-source.tar.gz`, SHA-256 `0e308604f829936fc968c30fe99ee60de159df3a4341d6172ff5322e90aaf081`, containing the routing crate source/manifests, workspace Cargo manifests/lock, native pager lifetime adjustment and benchmark source. Routing kernel and benchmark code were committed as `07a1fd12`; the snapshot additionally captures the uncommitted native pager integration from the separate Plan .04 work.
- Benchmark source SHA-256: Rust `477ed9c4df2215314e27a2e26b6c43bb39bc5e475460c1e7ee68588668361de8`; Python `66caa8d99a68dec99a8e3e68e1fbb138d296ff8dd53b1fa1ee4b92bc233ce999`.
- Work directories on the server: `~/Srv/plan14-old-20260927` and `~/Srv/plan14-new-20260927`. All server experiment files stayed under `~/Srv`.

Reproduce the old checkout from the Git parent plus the old full-worktree overlay. Reproduce the new checkout from that same parent and old overlay, then apply the new routing-source snapshot over it. Copy the tracked Rust benchmark to `crates/aic-routing/examples/one_shot_probe.rs` and the Python script to the checkout root:

```sh
cargo build --release -p aic-routing --example one_shot_probe --quiet
python3 run_mem.py
```

The Rust benchmark config is `work_budget=200000`, `scan_budget=2000000`, `frontier_budget=200000`, no deadline or crossings, with deterministic blocked and cell-penalty formulas in the tracked source. No setting reduces the requested problem size for a successful result.

## Results

| Case | Old median | New median | Difference | Same route outcome | Old/new scans |
| --- | ---: | ---: | ---: | --- | ---: |
| 128×128 | 9.76 ms | 10.48 ms | +7.4% | found, 254 steps, 13 turns, path hash `4010017945376446314` | 203,878 / 206,276 |
| 300×300 | 34.50 ms | 43.67 ms | +26.6% | found, 598 steps, 46 turns, path hash `4224435567279194650` | 706,408 / 719,141 |

The old and new calls also agree on total expansions (25,506 / 89,204), including 256 reverse-probe expansions in each case; forward expansions are therefore 25,250 / 88,948. They agree on frontier peaks (25,693 / 92,234) and `preflight_status=unknown`. The reverse probe is bounded and the A* continues; `unknown` here is not an infeasibility verdict. Five-run process peak RSS was 48,468 KiB old and 57,764 KiB new (+19.2%). The new representation initially used about 71 MiB; compacting duplicate per-state coordinates/direction and retaining only the distinct priority cost lowered the stable peak to about 58 MiB without changing route signatures.

The larger case is about 27% slower, so performance continuity is qualified. The prepared graph's sparse checked keys, wider checked scores and transient successor construction plausibly contribute, but this comparison does not isolate their individual costs. There is no evidence yet that this kernel difference dominates an integrated factory run. Plan .15 and the final readiness gate must measure the real caller mix before a long training launch; the smaller factory case is near parity.

## Correctness and review

The public router now uses the prepared graph's reverse probe, successor generation, forward frontier and path reconstruction. It no longer rejects by fixed grid area or a one-million-state constant. Sparse above-250,000-cell bounds and a requested work budget above one million succeed when actual search is small. Checked coordinate offsets skip neighbors outside representable coordinates; unrepresentable scores return typed resource stops. Existing penalty and crossing input rejection is at the call boundary. Focused tests cover sparse bounds, near-coordinate-limit neighbors, inconclusive reverse probes, exact cell and edge penalty choices, and the two issues found by implementation review: the reverse probe now applies the public frontier cap during exploration, and A* again returns up to 2,048 blocked cells when the reverse probe was inconclusive. The full-factory reembedding caller uses those blocker diagnostics.

`cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace --quiet` and `node docs/maintenance/check-tracked-files.mjs --worktree` passed on the final worktree. The routing-kernel commit was also applied alone to a clean detached checkout; `cargo check --workspace` and `cargo test --workspace --quiet` passed there after creating the conventional local `target/` directory expected by the generator CLI tests. `node docs/maintenance/check-tracked-files.mjs --index` and its six focused tests passed before commit. Independent implementation review `/root/review_plan14_implementation` first returned BLOCK on the two probe/diagnostic bugs, then PASS_WITH_NOTES after correction and PASS on the compact-state follow-up and corrected report. Its remaining note is the measured sparse-route cost.

## Verdict and limits

The one-shot kernel meets Plan .14's safety and route-preservation contract on these bounded fixtures and focused edge cases. It is suitable for caller integration and scale calibration; it does not by itself establish broad-distribution readiness. The measured sparse slowdown and RSS increase must remain visible in downstream integrated comparisons. No GPU experiment was launched.

## Cleanup manifest

After the routing source commit and the independent report review, removed only `~/Srv/plan14-old-20260927` (112,280 KiB) and `~/Srv/plan14-new-20260927` (156,548 KiB). Server free space was 158,853,796 KiB before removal and 159,122,624 KiB after, an increase of 268,828 KiB equal to the directory sizes. Both paths were confirmed absent. The temporary local detached checkout under ignored `target/plan14-index-check` was also removed after isolated tests. The old WIP and exact new source overlays remain under ignored local `artifacts/` while their uncommitted integration provenance is needed. No checkpoint or irreplaceable training result was in the removed directories.
