# Compact action cutover and fresh trial

## Status

Implementation and GPU readiness complete. The fresh one-hour session ended
at its 2026-09-10 13:42:28 KST deadline; bounded shutdown and export completed.
Results are preserved for joint review. No automatic continuation.
See the [action contract](../designs/2026-09-10.13-compact-factory-actions.md).

## Context and completion criteria

The user stopped the earlier trial after discovering one-strip contraction and
requested an audit of unnecessarily fragmented actions, a compact correction,
and another run. Keep 32 decisions, the selected actor, eight workers and existing
PPO settings. Preserve the earlier evidence; initialize fresh. Complete native
invariant tests, normal workspace/PPO checks, a bounded CUDA readiness run, and a
new isolated one-hour session. No automatic twelve-hour continuation.

## Activity

- Audited all eight action families. Change contraction, insertion and component
  destination enumeration; retain the already atomic pose/port/route/release/restore
  operations. Define fixed-size descriptors and explicit candidate caps first.
- Implement axis-wide contraction, bounded multi-cell insertion and direct
  translated/rotated component proposals. Reuse two spare action features; keep
  all tensor widths and the model unchanged. Replace obsolete whole-vocabulary
  and observation parity tests with four native invariant/acceptance tests.
- Local checks pass: `cargo fmt --all`, `cargo check --workspace`,
  `cargo test --workspace` (545 passed, four explicit historical tests ignored),
  and all 44 `test_ppo*.py` tests using the newly built native extension.
  The crossing fixture compresses both axes in two charged decisions. Multi-cell
  insertion validates and its contraction inverse preserves physical identity;
  a distant component translation plus rotation also validates as one edit.
- Schema cutover: input `factory-input-contract-screen-v2`, PPO experiences
  `ppo-critic-gae-direct-area-v2`, harness/checkpoints `ppo-training-harness-native-v3`.
  The old frozen runtime remains the reproduction path for old checkpoints.
- Diagnostic vocabulary JSON is schema 4. Implementation commits: `2d1f072`
  and `43c5d73`. The final workspace suite again passes 545 tests (four ignored).
- Apply two contractions to the exact 18-facility food input from the earlier
  visual preview: remove 24 columns, then 34 rows. Used dimensions change from
  65x85 to 41x51; area 5,525 to 2,091 (62.15% reduction), transport tiles 413 to
  292, and turns remain 76. Both edits validate and no strip remains removable.
  This is an action capability probe, not a learned-policy result. Local evidence:
  `artifacts/factory-edit-pilot/compact-action-launch/food-two-actions.json`
  (SHA-256 `fceba9c14296ab727a00d23776b37955a7cee9495cf5b749b85126f4e316e679`).
- The first server readiness attempt failed before collection because Cargo
  selected system Python 3.14 instead of the training environment's Python 3.12.
  Preserve that failed runtime at `/home/ashrimp/Srv/aic-ppo-compact-20260910`.
  Build the corrected, isolated runtime with explicit `PYO3_PYTHON`:

  ```sh
  PYO3_PYTHON=/home/ashrimp/Srv/endfield-aic-layout-gen/tools/factory-edit-pilot/.venv/bin/python \
    cargo build --release -p aic-pilot-native --features extension-module
  ```

- Corrected runtime: `/home/ashrimp/Srv/aic-ppo-compact-20260910-r2`, frozen from
  `43c5d73`; source archive SHA-256
  `569598c11f0dabc933f348ad9a2a0ff716463afdcfea5725446398fb3e27c1ce`.
  The unchanged CLI and witness probe are copied from the prior frozen runtime;
  the native library is rebuilt and its embedded source guard passes.
- GPU readiness uses the tracked small readiness config with these explicit
  overrides: one `current-history-16` job; eight workers; two training batches
  of eight attempts, horizon 32; four evaluation attempts at batches 0 and 2;
  32 GiB RSS; offline tracking; train generation/action bases 311000000/321000000,
  evaluation bases 331000000/341000000. The retained `artifacts/launch/readiness.json`
  has SHA-256 `57c7b4b69220ed15dbfec222ac1763e6a04b02fcad31a68900147dab05f57544`.
  All 23 admitted episodes validate (736 selected actions); one 26-facility food
  attempt exhausts the existing constructor route budget before setup or policy
  execution. The run completes in 20.6 seconds. The normal
  `check_ppo_harness_native.py` audit passes behavior probabilities/values and
  exact continuation from nonempty Adam state in 7.8 seconds. Evidence is under
  `artifacts/factory-edit-pilot/compact-readiness` and `compact-readiness-audit`.
- Fresh timed run uses the unchanged `ppo-one-hour-trial.json`: 32 attempts per
  training batch, horizon 32, eight workers, current-history-16, seed 17, CUDA
  FP32, evaluations every eight updates, unchanged PPO and retention settings.
  Run root is `artifacts/factory-edit-pilot/ppo-one-hour-trial` in the corrected
  runtime. Protocol SHA-256:
  `9d1c642898a05fe9fd5085dc2f47d66bf7c065a4bc2dff90ed4731df91e3082a`.
  It records 258 runtime/code/binary/input identities. The same one-hour launch
  driver uses GNU timeout, retains checkpoints, exports terminal metrics and
  never resumes automatically. Driver PID at launch: 548235.
- Live run: <https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/66a3fbde18eaf4949e488b51>.
  Startup verification at 78.7 seconds: two committed training batches, 2,048
  admitted training actions, active learner, live metric export, no diagnostic,
  and 14.34 GiB sampled aggregate RSS under the 32 GiB guard.
  Compare learning to this run's own initial evaluation: random setup also uses
  the new action vocabulary, so matching generation seeds need not produce the
  previous run's starting geometry. The stopped trial's runtime/checkpoints are
  preserved. Learning-quality conclusions await joint review after this hour.
- Timed session completed with 216 committed batches and 216,768 admitted
  training actions. Timeout exit 124 is the expected one-hour stop; terminal
  export exits successfully. The final checkpoint is
  `current-history-16--seed-17/training-0216/checkpoint-81ce25ed04d94dbaab03d3c1f5ef94bd.pt`,
  SHA-256 `704a554e294ad40e5c01a278614c04a4877217e943dbc94bcf348b8507ba70f0`.
  Its actual hash matches; the terminal summary verifies all durable records.
- For the user-requested visual, replay evaluation 208, factory index 0 from
  the actual policy trace. All 32 action keys, statuses, current/best identities,
  scores and area rewards match; the final best witness matches exactly.
  This 18-facility food case changes from 66x51 to 38x41. Its learned trace
  includes a two-facility rotation, exploration moves followed by compression,
  restoration and rejected moves. Local evidence is retained under
  `artifacts/factory-edit-pilot/compact-policy-visual`.

## Next experiment consideration

The user proposed considering a penalty for failed actions. This is a review
item, not an approved reward change or another training launch. Define the
failure classes and coefficient before a controlled comparison; distinguish
executor/validation failures from valid exploratory moves that do not immediately
improve area. Compare completion, final area, rejected-action rate and exploration
behavior against this unchanged direct-area baseline.
