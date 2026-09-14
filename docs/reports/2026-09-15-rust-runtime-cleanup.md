# Rust runtime cleanup after porting verification

## Result

Removed the Rust Python extension and its dependencies, three obsolete editor
paths and the numerical porting CLI/checkpoint machinery. Python files are
unchanged. The native editor and learning layers now execute through one Rust
path, without compatibility forwarding or feature switches for deleted code.
The implementation and migration contract are documented in
[the cutover design](../designs/2026-09-15-rust-runtime-cleanup.md).

The actual PPO batch is built from the frozen rollout. A lean seed provides only
initial model/Adam/RNG state and historical replay. Resume never opens the initial
state archive. Native catalogs and their identity are shared once; admission does
not construct an immediately discarded observation. Porting tensor/gradient/RNG
exports and repeated ownership/final-validity audits are gone. Buffered JSON
persistence fixes the remaining per-token filesystem writes.

The current command still covers collection and one actual minibatch per chunk.
This cleanup does not implement the outstanding full training scheduler, global
success buffer, curriculum/evaluation or complete-wave publication.

## Comparable bounded execution

Reused the prior actual 128-stream/16-worker input and initial learner/replay
values, two 32-action chunks, 64-action horizon, CUDA device 0, Torch 2.13.0 and
foreach Adam. External timing uses the existing measurement-only process guard.
No normal training process was started.

| Measure | Verified porting command | Clean collection command |
| --- | ---: | ---: |
| Charged actions | 8,192 | 8,192 |
| Native collection | 3.413 s | 3.320 s |
| Two minibatch updates | 0.426 s | 0.429 s |
| Boundary checkpoint | 0.816 s | 0.179 s |
| Whole CLI | 17.178 s | 4.622 s |
| External wall time | 18.139 s | 5.577 s |
| Peak child RSS | 5,953,642,496 bytes | 5,665,640,448 bytes |

The reduction measures removed diagnostics and improved persistence, not an
optimizer speedup or full-training sample rate. Actual elapsed time remains a
policy input, so separately timed runs are not claimed to have identical full
trajectories. A retained-array regression separately proves the unchanged update
math exactly. An intermediate cleanup run without buffered JSON took 13.064 s
inside the CLI, exposing how much per-token writes still cost.

## Verification

- Default workspace check/tests: 552 passed, three explicitly ignored. Release
  feature-enabled workspace check/tests: 557 passed, five explicitly ignored.
  The three retained-input/lifecycle/audit tests below also passed when invoked
  explicitly with their required artifact paths.
- Exact update regression against both retained 4,096-sample collection fixtures:
  every model/Adam/shuffle/replay state field matches; the test applies recorded
  minibatches and therefore excludes the environment sampler advancement.
- Separate-process CLI continuation with two workers after deleting the initial
  state archive; all eight streams reach their real 64-action horizon.
- Reserved geometry and changed input hash rejection; memory-budget failure keeps
  actual partial numerical inputs, current observations, native snapshots and
  learner state, without a completed checkpoint.
- Native routing, port atomicity, unchanged-action work skipping, partial worker
  replies, resource exhaustion and checkpoint/state ownership regressions.
- Explicit cold physical audit of all 128 retained best layouts, outside normal
  collection.

Python extension tests are intentionally retired with the Rust extension, rather
than used to keep its implementation alive. Historical source and artifacts remain
available at the pre-cutover commit `cd6d7fc`.

## Reproduction and migration

Use the same LibTorch environment as the previous collection report:

```sh
export LIBTORCH="$PWD/.venv/lib/python3.12/site-packages/torch"
export LIBTORCH_CXX11_ABI=1
export LD_LIBRARY_PATH="$LIBTORCH/lib"
export CUBLAS_WORKSPACE_CONFIG=:4096:8
cargo build --release -p aic-cli --features learning
target/release/aic-cli learning collect \
  --config artifacts/native-collection-input-128/manifest.json \
  --seed artifacts/native-cleanup-seed/manifest.json \
  --output artifacts/native-cleanup-final-128 --chunks 2
```

The retained `native-cleanup-seed` was extracted once from
`kernel-input-multistage`: preserve `model.*`, `adam.*`, `rng.*` in
`state.safetensors`, and `sil.*` in `replay.safetensors`; remove all `ppo.*` data.
The new manifest changes `schema` to `aic-learner-seed-v1`, removes `ppo_graphs`,
`ppo_factors`, `tensors` and `tensors_sha256`, and adds `state`/`replay` references
with SHA-256 hashes. It preserves the model, update, parameter order, SIL counts,
source metadata and byte bound. This is one-time artifact extraction, not a
runtime compatibility layer. Retain the original input/checkpoints as evidence.

Explicit retained-evidence tests:

```sh
AIC_RETAINED_COLLECTION="$PWD/artifacts/native-collection-complete-128" \
  cargo test --release -p aic-learning --features libtorch \
  retained_collection_updates_remain_exact -- --ignored
AIC_COLLECTION_CONFIG="$PWD/artifacts/native-collection-input-8/manifest.json" \
AIC_LEARNER_SEED="$PWD/artifacts/native-cleanup-seed/manifest.json" \
  cargo test --release -p aic-cli --features learning \
  collection_resume_skips_initial_state_and_failure_preserves_partial_inputs -- --ignored
AIC_COLLECTION_CONFIG="$PWD/artifacts/native-collection-input-128/manifest.json" \
AIC_COLLECTION_RESULTS="$PWD/artifacts/native-cleanup-final-128/results.json" \
  cargo test --release -p aic-pilot-native \
  retained_best_layouts_pass_explicit_physical_audit -- --ignored
```

Remote artifacts remain under
`/home/ashrimp/Srv/aic-rust-learning-20260915/artifacts`; local copies are under
`artifacts/rust-runtime-cleanup-20260915`. Compact identities and measurements are
in [the evidence manifest](2026-09-15-rust-runtime-cleanup-evidence.json).


Retained cleanup seed, final run and measurement outputs total 129,572,916 bytes
excluding verification logs. Source hashes match the tested server checkout.
PyO3/NumPy and extension-only direct dependencies are gone. Numerical crates such
as ndarray still appear transitively under the live `aic-data` solver code; those
other Rust commands remain supported and are not legacy Python compatibility.
