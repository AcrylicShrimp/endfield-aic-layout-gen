# Local complete-root catalog measurement for Plan 03

## Question and method

Check whether the source-bound exact Gumbel catalog can process two retained physical partial factories locally through complete initial placement and lower witness collection, 128 sparse Gumbel simulations at horizon 6, target freezing, descriptor/report publication and verified sidecar replay. This is a throughput and contract probe with randomly initialized width-32 outer and guidance models, not a trained-policy quality result or an upper physical factory gate. No training server was used.

The 30-block, 44-connection input is `artifacts/plan01-placed-input-20260927/panel-branching38.json` (SHA-256 `4a0af7db3bc3d4de120767e368a7abb386dec274ba3764c17c2e1068d7642b8a`) after three accepted prefix commands. The 100-block, 99-connection input is `artifacts/plan01-placed-input-20260927/panel-prefix20.json` (SHA-256 `be4285895b507a6796372384066fde40e9020ee751404b4ba7497997c6ef14a`) after ten accepted prefix commands. Both contain zero terminals. The full legal native Place and PlaceTerminal domains were enumerated; there is no spatial or candidate-count truncation of these placement actions.

The probe starts from `docs/benchmarks/hierarchical-curriculum/search.json`, substitutes each panel's physical bounds and crossing allowance, and uses the search configuration's wider route limits (six candidates, 48 searches, 20,000 work per search and 400,000 expanded nodes). Placement page size is 256, Gumbel rank assignment uses 16 candidates with eight anchor candidates, simulations per move are 128, horizon is 6, and the shared tree budget is 4 GiB. The outer model is initialized from seed 51 and the guidance model from seed 17 on CPU. The probe's synthetic initial-model seed/config digest identifies these test weights; it is not a production checkpoint-file digest.

## Result

| Metric | 30 blocks | 100 blocks |
| --- | ---: | ---: |
| All initial legal actions | 545,381 | 5,715,348 |
| Placement actions | 545,380 | 5,715,346 |
| Initial lower witnesses | 1 | 2 |
| Simulations | 128 | 128 |
| Catalog collection | 0.395 s | 4.372 s |
| Sparse search | 0.298 s | 6.061 s |
| Collection through report publication and replay | 0.721 s | 10.593 s |
| Direct-process maximum RSS | 317,489,152 B | 1,755,299,840 B |
| Peak charged tree memory | 236,403,061 B | 1,845,544,044 B |
| Search-only sorted file | 13,089,144 B | 137,168,352 B |
| Published source-order prior sidecar | 8,726,096 B | 91,445,568 B |
| Phase-accounted temporary file payload peak | 28,359,812 B | 365,782,272 B |

The 30-block root uses one sorted run, so its largest simultaneous file payload is raw scores plus that run plus the ledger: `N × (12 + 24 + 16)`. The 100-block root merges multiple runs after removing the raw file, so its largest simultaneous payload is two sort copies plus the ledger: `N × (24 + 24 + 16)`. These values are derived from the observed exact action counts, fixed record widths and verified file lifetimes. They exclude filesystem block rounding and the small JSON report temporary. Both outputs were read back through the descriptor's checksum-verified handle. Raw, sort and ledger intermediates were deleted after publication; the probe then removed its report and sidecar.

The 100-block search's 1.76 GB measured RSS is materially above its placement-only probe's roughly 150 MB, because 128 horizon-6 simulations retain search state. The initial lower witness counts are small, so these two partial roots do not measure a broad lower-route distribution or prove that a larger upper physical root fits the 4 GiB tree budget. A separate placed lower fixture widens from two initial witnesses to twelve retained witnesses, including route updates both inside and outside the shortlist, and checks published/replayed frozen targets against the dense root. Representative upper physical inputs remain a later gate.

The placed lower fixture was also measured as a direct release test process. It has two initial legal routes, twelve retained lower witnesses after widening, a one-route shortlist, and both inside- and outside-shortlist route updates. Catalog collection took 0.002 s, sparse search 0.003 s, and staging, one intentional failed report serialization, successful publication and verified replay 0.019 s. Its maximum process RSS was 119,635,968 B and charged tree peak 34,689,825 B. The identity-noise sorted file was 64 B, published prior sidecar 32 B and published report 117,811 B. The largest simultaneous file payload was 117,843 B during report publication, exceeding the 136 B raw/run/ledger phase; retained report plus sidecar was also 117,843 B. Direct-process RSS includes the dense comparison oracle and both models, while the stage times exclude their setup and dense search. The generated report and sidecar were deleted after verification. The fixture is `docs/benchmarks/hierarchical-route-guidance/training-panel.json` case zero with its accepted prefix.

## Reproduction and provenance

The latest measurement used Git base `42796d6a1422b721f593fc0859e903727527a5d1` plus the uncommitted source snapshot in ignored `artifacts/plan03-local-full-root-evidence/`: `tracked.patch` SHA-256 `a222f06c8a6d0c4b3b0948bff484eebd4f59e45f3019544e61921f4eb69af0c4` and `untracked-rust.tar` SHA-256 `3ea5a6276d5fd919d361a6d00ade453b181829f94a00ed777de5e11575977a26`. The patch covers tracked Cargo, crate and benchmark files; the archive covers untracked Rust source listed in `untracked-rust-files.txt`. The release test executable was `target/release/deps/aic_learning-d9c4ff19846b1193` at SHA-256 `16ddaefe133643b009f0e7d4fc0b97911cf3565f5e8cd322192a6b94fb16c0df`. Reconcile this source snapshot with the eventual completed implementation commit before citing it as final.

Build from that source, then run the test binary directly under `/usr/bin/time -l` so compilation does not enter the RSS measurement:

```sh
LIBTORCH="$PWD/artifacts/local-libtorch/torch" LIBTORCH_CXX11_ABI=1 DYLD_LIBRARY_PATH="$PWD/artifacts/local-libtorch/torch/lib" CARGO_INCREMENTAL=0 cargo test --release -p aic-learning --features libtorch --lib complete_physical_root_catalog_probe --no-run
/usr/bin/time -l env LIBTORCH="$PWD/artifacts/local-libtorch/torch" LIBTORCH_CXX11_ABI=1 DYLD_LIBRARY_PATH="$PWD/artifacts/local-libtorch/torch/lib" AIC_ROOT_CATALOG_PANEL="$PWD/artifacts/plan01-placed-input-20260927/panel-branching38.json" AIC_ROOT_CATALOG_PREFIX=3 target/release/deps/aic_learning-d9c4ff19846b1193 --ignored complete_physical_root_catalog_probe --nocapture
/usr/bin/time -l env LIBTORCH="$PWD/artifacts/local-libtorch/torch" LIBTORCH_CXX11_ABI=1 DYLD_LIBRARY_PATH="$PWD/artifacts/local-libtorch/torch/lib" AIC_ROOT_CATALOG_PANEL="$PWD/artifacts/plan01-placed-input-20260927/panel-prefix20.json" AIC_ROOT_CATALOG_PREFIX=10 target/release/deps/aic_learning-d9c4ff19846b1193 --ignored complete_physical_root_catalog_probe --nocapture
/usr/bin/time -l env LIBTORCH="$PWD/artifacts/local-libtorch/torch" LIBTORCH_CXX11_ABI=1 DYLD_LIBRARY_PATH="$PWD/artifacts/local-libtorch/torch/lib" target/release/deps/aic_learning-d9c4ff19846b1193 collector_keeps_all_initial_lower_witnesses --nocapture
```

## Verdict and remaining gates

The local 30/100 complete-root probes succeed without a placement action cap, and the disk phase admission passed an independent focused review. A separate bounded 64-action near-tie test changed the selected physical action at rank seed zero after page scoring (maximum prior error 7.451e-9; 124 pair-order reversals). Against a dense reference fed those same page-computed priors, the sparse catalog matched the shortlist, selected action, 128 visits and simulations, Q values, and reconstructed `f32` targets; maximum Q and target error were both zero. This test uses actual legal action IDs and child values with controlled injected model features, so it establishes selection/target logic under altered priors rather than trained-model quality. Independent full-module review `/root/review_plan03_full_module_r3` returned PASS for Plan 03. Plan 04 owns the production generation/trainer cutover and replayed gradient parity; broader training distribution and upper physical/GPU gates remain separate. No server was used.
