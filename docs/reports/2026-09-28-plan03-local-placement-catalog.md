# Local placement catalog measurement for Plan 03

## Question and method

Measure the complete legal placement portion of the proposed exact Gumbel root catalog on a retained 100-block physical partial state, without the training server. This probe is a search-stage measurement, not an integrated root search or training result. It includes native placement enumeration, analytic feature/anchor/child-value scoring, one encoded width-32 outer-model root, page logits, global normalization, source-order ledger writing, stable external sorting, Gumbel shortlist selection, sequential native witness resolution and unvisited target-scalar reconstruction. It does not enumerate initially exposed lower route/network/repair witnesses, simulate the shortlist, publish a descriptor, replay targets or train.

The source is `artifacts/plan01-placed-input-20260927/panel-prefix20.json` at SHA-256 `be4285895b507a6796372384066fde40e9020ee751404b4ba7497997c6ef14a`. The probe applies the first 10 prefix commands. It uses the panel's native `Limits` and candidate placement bounds, extension lengths `[1,2,4]`, page size 256, a CPU outer model with width 32 and one message round initialized at seed 51, rank Gumbel seed 7, first and second anchor masses 0.25, 16 shortlist candidates and four anchor-quota candidates. The model is randomly initialized for throughput measurement; this is not model-quality evidence. The catalog tree admission is 512 MiB.

Code base: Git `42796d6a1422b721f593fc0859e903727527a5d1` plus the uncommitted snapshot under ignored `artifacts/plan03-local-placement-catalog-evidence/`. The tracked binary patch has SHA-256 `046fb9d2a65227cc17e849abec241720f6fdc42179f8eb6c22d450db074a9690`; the untracked Rust-source tar has SHA-256 `511d79f62f113155f350325ab7472a511eeaa4d9f489198eaec39497d2ba1819`. The exact catalog file at measurement had SHA-256 `7dc63aea4cd797e53bd38d20e21ca4fed4a81d8007d7c540d1445ff62e9f8417`. Reconcile this snapshot with the eventual completed implementation commit before citing it as the final implementation.

Reproduce from the repository root:

```sh
LIBTORCH="$PWD/artifacts/local-libtorch/torch" LIBTORCH_CXX11_ABI=1 DYLD_LIBRARY_PATH="$PWD/artifacts/local-libtorch/torch/lib" AIC_ROOT_CATALOG_PANEL="$PWD/artifacts/plan01-placed-input-20260927/panel-prefix20.json" AIC_ROOT_CATALOG_PREFIX=10 cargo test --release -p aic-learning --features libtorch --lib complete_physical_placement_catalog_probe -- --ignored --nocapture
```

For runtime RSS, run the already-built test binary directly under `/usr/bin/time -l`; wrapping `cargo test` also measures the compiler and gave an irrelevant 1.89 GB maximum. The measured direct binary was `target/release/deps/aic_learning-3a898f0c85d461fc`. The probe deletes raw, sort and ledger temporaries after checking its result. The disk peak was sampled with `du -sk` while the temporary directory existed, so it is an observed lower bound rather than an exact maximum.

## Result

| Metric | Local observation |
| --- | ---: |
| Native legal Place actions | 5,715,346 |
| Placement scan and raw scoring | 3.146 s |
| Global normalization and run writing | 1.029 s |
| External merge after raw-file removal | 0.260 s |
| Whole probe including shortlist, resolution and target scalars | 5.778 s |
| Runtime maximum RSS, direct test binary | 150,437,888 B (143.47 MiB) |
| Peak charged tree memory | 34,863,552 B (33.25 MiB) |
| Raw rank-mode temporary, removed before merge | 68,584,152 B (65.41 MiB) |
| Retained source-order prior ledger during probe | 91,445,536 B (87.21 MiB) |
| Search-only sorted file | 137,168,304 B (130.81 MiB) |
| Largest observed temporary-directory sample | 330,500 KiB (322.75 MiB) |
| Phase-accounted upper file payload at this action count | 365,782,144 B (348.84 MiB) |

The phase-accounted bound is `N × max(12+24+16, 2×24+16)` for rank mode: raw score, one sort copy and ledger while normalizing, then two sort copies and ledger while merging. It excludes filesystem block rounding and unrelated files. The raw file was absent before merge and the temporary directory was empty and removed after the probe. A second direct run took 5.82 s as a test process; the first release run took 39.11 s including 32.40 s of compilation. The direct run's wall time was 5.91 s.

## 30-block cross-check

The same built binary also ran the retained 30-block partial panel `artifacts/plan01-placed-input-20260927/panel-branching38.json` (SHA-256 `4a0af7db3bc3d4de120767e368a7abb386dec274ba3764c17c2e1068d7642b8a`) after its first three prefix commands. Set `AIC_ROOT_CATALOG_PANEL` to that path and `AIC_ROOT_CATALOG_PREFIX=3` in the command above. It covered all 545,380 legal Place actions in 0.462 s total: 0.241 s scan, 0.037 s normalization/run writing and 0.079 s merge. The direct-process maximum RSS was 133,562,368 B, peak charged tree memory was 34,745,065 B, ledger was 8,726,080 B and sort file 13,089,120 B. The raw, sorted and ledger temporaries were removed after the check. This is a second physical input scale, still placement only.

## Verdict and limits

The local placement catalog completed the full legal 100-block placement domain without a candidate-count cap. Its RSS and file payload fit this local probe's 512 MiB tree admission and available disk. This establishes only the placement catalog stage. Plan 03 still needs initial lower witnesses, late updates, sequential-halving simulation, source-bound descriptor publication, replay checks and a final integrated timing/review. The hypothetical upper physical/GPU gate remains untested, and no server was used.

After retaining this compact result and the 1.6 MiB uncommitted-code snapshot, the raw score, sort runs, ledger and temporary probe directory were removed. The local disk had approximately 6.5 GiB available after the run.
