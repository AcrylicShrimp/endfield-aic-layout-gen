# Local 249-facility medical source construction probe

## Question and method

Can the actual full-factory `Source::generate` path produce a 240–256-facility medical input locally, so Plan .24 can measure producer/read and checkpoint byte admission? This is one predetermined source target and seed, not a distribution or global feasibility test. The ignored `measure_source_generated_input_band` test loads catalogs and the medical source plan named by `tools/native-training/training.json` (`docs/benchmarks/constructive-recipe-breadth/requests/medical-3.json`), calculates contextual wiring, then runs the real source constructor, committed reuse and native input read. It overrides the production 64-facility ceiling for this probe; no generator rule or routing policy was changed.

The target is one `item-bottled-rec-hp-3` every 1,900 ms. The source plan has SHA-256 `31524da79fa6cda4eaa59a7635ce385dd12f5ace314f152799372170c3dcac9c`, with recipe selections `shaper-iron-enr-bottle-1` and `furnance-iron-nugget-1` on their respective input paths. The generated wiring contains 249 facilities. The test sets `duration_step_ms=100`, `max_facilities=256`, `max_attempts=1`, `max_steps=1024`, `max_placement_attempts=1024` and `max_route_searches=16384` **per growth step**, and `construction.elapsed_ms=1800000`; `episode_setup_ms` is that construction allowance plus the configured setup allowance. The source seed is `722900000`. These are experimental budgets, not new product defaults or evidence that they suffice.

Run from the repository root. The exact commands were:

```sh
/usr/bin/time -l env AIC_PLAN24_OUTPUT=target/plan24-local/medical-249-1800s \
AIC_PLAN24_PRODUCT=medical AIC_PLAN24_DURATION_MS=1900 \
AIC_PLAN24_CONSTRUCTION_MS=1800000 \
target/debug/deps/aic_pilot_native-70ae48dba33cdf30 \
measure_source_generated_input_band --ignored --nocapture

/usr/bin/time -l env AIC_PLAN24_OUTPUT=target/plan24-local/medical-249-release \
AIC_PLAN24_PRODUCT=medical AIC_PLAN24_DURATION_MS=1900 \
AIC_PLAN24_CONSTRUCTION_MS=1800000 \
target/release/deps/aic_pilot_native-aba32589b111542b \
measure_source_generated_input_band --ignored --nocapture
```

The binary SHA-256 digests were `1de80cedfaf8996b7eaa6a08566a72a5498dddf0663956c14c42e20079340a20` (debug) and `58e22256123c766963c042547b40db8ab4c83e7cc2c3e541072e0f31fb47b3c0` (release). The training config SHA-256 was `424a278bf71ae7459649c71be7d856ed2ec392d58ff3edd89ea604c935e3b64e`. Git HEAD was `42796d6a1422b721f593fc0859e903727527a5d1`. Relevant uncommitted source/test/input changes are retained in [the provenance patch](2026-09-28-plan24-medical-249-relevant.patch), SHA-256 `d8ce01988d3f905b72d66a44faae826d36d29c996380b376d4db5fb63cb4c1a0`. This zero-context diff applies to that HEAD with `git apply --unidiff-zero`. `aic-data/src/layouts/constructive/production.rs` was unchanged from HEAD (file SHA-256 `fc9a86af13e05d2119d41b6e12271facfbf6653aff6170b747c0ee46eb4dbfa4`); the source generator and test file digests were `154c1605caf02b494302c2ed97a69486129bbf1fedfcfa8f9166320854669388` and `08160eb094aaec15fa9d238ba3d6b1a9fc62163c8274043f3dd85245aee00957`. The release test binary was built after those source files' last edits. This patch is retained because the overall module worktree is not yet complete or committed.

## Results and verdict

| Build | Elapsed | Max RSS | Exact outcome | Input file |
| --- | ---: | ---: | --- | --- |
| Debug | 1,801.33 s | 142,508,032 bytes | One construction attempt was interrupted by `native-work.elapsed-budget`; `Source::generate` returned `aic-generation.exhausted`. | None |
| Release | 1,655.50 s | 307,118,080 bytes | One construction attempt returned incomplete `growth-exhausted` with `production-route-budget-exhausted` in a growth step; `Source::generate` returned `aic-generation.exhausted`. | None |

The release failure code is emitted only when `failed_statistics.astar_searches >= request.max_route_searches`. The constructor resets that counter at the start of each growth step, so **one step reached its 16,384-search limit**; this report does not know the whole attempt's cumulative A* count. The constructor also has an inherited fixed `CONNECTION_ROUTE_LIMIT=64` on each connection attempt. Existing small diagnostic reports document this 64-call policy, but they do not establish that it fits a 249-facility medical source. This probe does **not** isolate that per-connection limit as the cause of step-budget exhaustion; increasing it, increasing the step budget, changing the seed and changing placement order remain separate hypotheses. A short local process sample during the debug run showed active work in production `connect_first` and `RouteWorkspace::route_crossings`, not an idle wait. The raw sample was removed after inspection.

**Verdict: blocked for Plan .24 upper physical byte calibration.** Both runs stopped before serialization, so neither measures the generated-input limit, checkpoint limit, loader agreement, or upper physical distribution. The stops are censored budget results, not proof that a 249-facility medical layout is infeasible. A future controlled source-construction study should compare the existing 64-per-connection and 16,384-per-growth-step budgets against larger reviewed allowances and multiple predetermined seeds, preserving exact requested size and candidate rules; do not substitute replicated topology or a smaller product for the production case.

## Cleanup

The two failed output directories held only identical 616-byte `source-plan.json` files and their compact `failure.json` files; no input, checkpoint, replay buffer or irreplaceable state was produced. The failure JSON SHA-256 values were `047fc53437884f32067f329cb6a6e8899ccb1616c5a2e368758d821f4192c00a` (debug) and `10609869f7f59f7302a7006e654e9cc8330a7f981b40bcb03036d9cf3d1fe1b0` (release). After verifying this report and the retained patch, both `target/plan24-local/medical-249-1800s/` and `target/plan24-local/medical-249-release/` were removed. They occupied 8 KiB each. Filesystem free space was 17,808,852 KiB before and 17,788,400 KiB after; unrelated concurrent disk activity exceeded the 16 KiB cleanup, so the global free-space delta does not measure this deletion.
