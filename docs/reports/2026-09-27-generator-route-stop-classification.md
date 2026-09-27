# Procedural generator route-stop classification

## Question and method

Plan [18](../plans/2026-09-27.18-procedural-generator-route-stop.md) asks whether a bounded route, pair-trial, total-work or mid-attempt deadline stop can be mistaken for fully evaluated construction failure. The baseline is generator code at commit `ad44c490` after the scale-budget repair. The revised generator keeps its route search, semantic request, work allowances and one-shot router options unchanged. It classifies retained attempt outcomes once at the final failure boundary, propagates mid-attempt cancellation immediately, and publishes compact reason counts in Survey report schema `aic-layout-generation-survey-report-v3`.

The fixed-seed comparison uses the tracked [scale benchmark configs](../benchmarks/layout-generator-scale/README.md) and [measurement script](../benchmarks/layout-generator-scale/measure-route-stop.py). Both binaries were built in release mode on `ashrimp-arch` from the same checkout and catalog files. The baseline binary came from commit `ad44c490` (SHA-256 `b4d3c3caa61fe64a8dd26524089c0060fbd1a3bd65d5d17b78a29da2731e49d3`). The final generator code is commit `cdfd675de2e24044b2b552f477ae1d593577b012`; only its `generate.rs` and `survey.rs` were overlaid on the baseline checkout for the server build. Their byte hashes matched the committed source, and the final release binary SHA-256 is `f5945ce541b5a54d3adb5865f108bcb7f021b794d76e2640a2859d7bf922eeb5`. Runs were sequential, so wall and child peak RSS are comparable on this one machine, although they are only one observation per build.

## Findings

At seed 43000, the 100-facility Tree Spacious production `Generate` completed one layout before and after the classification change. Both files had SHA-256 `094f2beec0d925a1fda86e6930685138e8a2c8d7cd9d10e0d86b6374784902a4`, and the completed attempt charged exactly 45,444,012 route expansions in each build. Baseline/final wall time was 17.65/17.76 seconds and child peak RSS was 124,420/124,380 KiB. This bounded comparison establishes route/output identity for the control; its small timing difference is not a speed claim.

The exact final-source 256-facility Tree Balanced crossing-forbidden Standard one-attempt Survey connected 249/255, exhausted 48,960 pair trials after 158,890,240 route expansions, and reported `generation-search-incomplete`. The [baseline](2026-09-27-generator-scale-budget-contract.md) had exactly the same connected/work counts but incorrectly reported `generation-exhausted`. The revised row retained `pair-trials-exhausted: 1`, `found: 249` and `no-path: 139343`; both bucket and topology/packing `failure_reasons` counted the final diagnostic. The revised survey completed one executed slot, reported no success and no deadline interruption, and took 126.81 seconds with 85,000 KiB child peak RSS. This is a classification correction, not a claim that crossing-forbidden 256-facility generation now succeeds or is infeasible. An earlier release build before the packing-guard comparison fix had the same connected and work counts, 127.30 seconds and 84,940 KiB peak RSS; the final-source run is the reported result.

Offline forced-stop tests cover a per-call work stop followed by later candidate trials, a pair-trial stop with only complete local route results, a total-work stop immediately after a complete route, a packing canvas resource stop, and cancellation inside `connect`. They verify one charge per route call, retained exact attempt reason and final uncertainty. A two-facility fixed seed 9 completes a zero-length route, while the existing complete-generation case covers bridges and a successful final permitted pair. A synthetic two-attempt Survey result checks that an earlier work stop stays visible when the last attempt ends with a complete local rejection, while both summary paths group by the final diagnostic. A real one-second 256-facility Survey CLI run ended inside an attempt and reported `generation-interrupted`, `completed=false`, `incomplete=1`, `deadline_reached=true`, with `attempt_statuses={"interrupted":1}` and partial route-status counts.

## Reproduction and verdict

From a release build and fresh output directory, run the fixed controls with the tracked script:

```sh
python3 docs/benchmarks/layout-generator-scale/measure-route-stop.py \
  target/release/aic-layout-gen generate \
  docs/benchmarks/layout-generator-scale/configs/generate-100-tree-spacious.json \
  43000 artifacts/route-stop-control artifacts/route-stop-control-measure.json
python3 docs/benchmarks/layout-generator-scale/measure-route-stop.py \
  target/release/aic-layout-gen survey \
  docs/benchmarks/layout-generator-scale/configs/256-tree-balanced-no-crossings43000.json \
  43000 artifacts/route-stop-crossing artifacts/route-stop-crossing-measure.json
target/release/aic-layout-gen survey \
  --config docs/benchmarks/layout-generator-scale/configs/256-tree-balanced-no-crossings-deadline1.json \
  --catalog-dir data/game/normalized --seed 43000 \
  --workspace-budget-gib 16 --output artifacts/route-stop-deadline
```

The generator's `generation-search-incomplete` diagnostic means a bounded search left alternatives unevaluated. `generation-exhausted` means configured alternatives ended without such a stop; neither means global infeasibility. Survey `completed` describes execution of a raw slot, while `incomplete` counts interrupted or unstarted slots.

Verdict: Plan .18's route-stop classification contract passed. The result identity and work accounting in the 100-facility control stayed fixed, the censored 256-facility case is now reported as incomplete search, and the real deadline case is counted as interrupted execution. `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace --quiet`, and the tracked-file worktree check passed after the implementation and focused tests. The first independent implementation review blocked an unprefixed packing-guard comparison; this was fixed and tested. A second independent review returned `PASS_WITH_NOTES` with no code blocker and requested final evidence/cleanup, supplied here.

## Closeout and cleanup

The final generator/survey implementation is commit `cdfd675de2e24044b2b552f477ae1d593577b012`. The compact baseline and final-source reports, configurations, measurements and two 100-facility layout JSON files are retained locally in ignored `artifacts/plan18-route-stop-20260927/retained-results.tar.gz` (SHA-256 `292c0e4f31d59335f9991394ed8ad04d016d96a1ed395c4885d77ae3f626c0bf`). The exact one-second deadline report is retained in the same artifact directory. The reviewed source configurations and measurement script are tracked under `docs/benchmarks/layout-generator-scale/`; the cleanup manifest is [here](2026-09-27-generator-route-stop-cleanup.json). After checking the archive entries and matching its server/local digest, the server checkout, build, raw output and temporary panels were removed.
