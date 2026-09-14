# Rust learner numerical and recovery gate

## Outcome and scope

The first Rust learner slice passes the same-version Python numerical oracle
and separate-process recovery gate on CPU foreach, CUDA foreach and CUDA fused
Adam. All three modes match exactly on the retained fixture after 16 updates:
initial/final likelihood, entropy and value outputs, 28 first clipped gradients,
and 116 model/optimizer/RNG state tensors. The comparison thresholds remain
`atol=2e-6, rtol=2e-5`; observed maximum absolute error is zero. This is evidence
for this fixture and backend version, not a cross-device/version exactness claim.

This extends the [Python necessity audit](2026-09-15-python-boundary-necessity-audit.md)
after the user authorized moving the learner itself to Rust. The audit had
identified model/optimizer parity as a separate problem to establish first.
The Rust runtime imports no Python code and does not start Python. Its model,
PPO/SIL numerical update, independent Torch generators and optimizer recovery
live in systematic crate modules described by the
[foundation design](../designs/2026-09-15-rust-learner-foundation.md).
Python remains the explicit one-time export and numerical oracle tool.

Native persistent worker integration, real rollout/GAE construction, PPO epoch
and KL scheduling, success-buffer sampling/publication and complete-wave recovery
are not implemented by this numerical probe. The existing training harness has
not been replaced or restarted, and no new training-quality result is claimed.

## Reused behavior and optimizer choice

The implementation preserves the existing graph encoder and conditional port
head, packed inputs and cached device targets, PPO clipping/entropy, weighted
positive-advantage SIL with detached critic weights, separate actor/critic Adam
ownership and gradient clipping, and actor-safe critic-only updates.

`aic-torch` owns a narrow C++ bridge because `tch` 0.26 does not expose independent
Torch Generator state or the foreach/fused optimizer orchestration needed here.
The bridge calls existing ATen kernels, including batched gradient clipping. It
owns FP32 moments and step counters and imports/exports state by parameter name.
The stock C++ Adam per-parameter update path is not used. Fused Adam is explicit
and CUDA-only; foreach remains the default. Unsupported precision/decay/optimizer
modes fail at the boundary. The adapted PyTorch operation order and license are
recorded in `crates/aic-torch/native/NOTICE` and `PYTORCH-LICENSE`.

The first comparison exposed tiny backward accumulation differences from using
`index_select` and reusing indexed tensors where Python uses distinct advanced
index operations. The loss initially matched, but Adam amplified a tiny
shift-invariant actor-bias gradient difference beyond the threshold. Matching the
existing operation graph removed the difference without widening tolerances.

Linux `--as-needed` also discarded CUDA registration from the initial executable.
A retained CUDA symbol reference in the bridge fixes linkage; final checks use no
`LD_PRELOAD`. Build logic omits the CUDA reference for CPU-only installations.
Only the installed CUDA LibTorch build was exercised for the feature-enabled
backend; the feature-disabled workspace also passed on the local machine.

## Input and verification

The source is seed 149, update 4, from the retained normal baseline under
`/home/ashrimp/Srv/aic-response-opt-20260915/artifacts/shared-normal/seed-149`.
The export reads authenticated retained success references and both populated
optimizer states. It selects eight complete successful sequences: 16 labels
(8 placements, 7 band deletions, 1 band insertion), 17 conditional port factors,
and initially 16 positive imitation advantages.

PPO old log probabilities, advantages and terminal-return targets are frozen
for numerical comparison. These labels are not presented as a newly collected
on-policy rollout or a reconstructed GAE dataset. Actual masks and conditional
branch arrays remain in the input archive. Source checkpoint, launch, native
library, selected references and archive hashes are in the
[compact evidence manifest](2026-09-15-rust-learner-kernel-evidence.json).

The source checkpoint was written with Torch 2.14; export/import and both oracles
run in a separate Torch `2.13.0+cu130` environment matching `tch` 0.26. The version
guard was not bypassed. Hardware is the existing RTX 5090 server. Numerical
execution uses one Torch CPU thread and deterministic algorithms with
`CUBLAS_WORKSPACE_CONFIG=:4096:8`.

For each mode, run 16 continuous updates and compare with a separate process
that saves after update 2, followed by another process restoring and executing
14 updates. Update 3 and every third update thereafter are explicit critic-only
probe steps. Actor parameters and actor optimizer state remain unchanged on
those steps. All resumed records, subsequent shuffle/categorical/replay draws,
and final 116 state fields match exactly. The checkpoint contains one tensor
archive and one manifest, published by directory rename with file/directory
fsync. Restore validates required state and field names once; it does not replay
history or take a second full GPU state snapshot for validation.

Six CLI rejection cases pass without publishing a checkpoint: out-of-range root,
wrong conditional-factor owner, missing Adam moments, negative second moment,
malformed RNG state and input hash mismatch. Errors include a stable code,
severity, stage, message and input entity. Backend tests additionally cover
independent RNG continuation for all three draw types, nonempty Adam restore,
missing-gradient no-advance behavior, snapshot isolation, unrepresentable and
maximum-int64 step counters, malformed state and fused-on-CPU rejection.

## Bounded runtime measurement

The following is one bounded numerical workload, not a whole-training throughput
benchmark. Each run executes the same 16 frozen minibatch updates; the mean uses
updates 5–16 after four warmup updates, including eight actor updates and four
critic-only updates. It includes forward/backward, both applicable optimizer
steps, finite checks and explicit diagnostic RNG draws. Per-step file output and
critic-tail snapshots are outside this timed interval. Inputs and device targets
are prepared once. The Python oracle uses the existing model/likelihood and
Python foreach/fused Adam.

| Device / Adam | Rust warm update mean | Python warm update mean | Rust initial load | Rust complete checkpoint save |
| --- | ---: | ---: | ---: | ---: |
| CPU / foreach | 6.50 ms | 6.78 ms | 3.29 ms | 24.96 ms |
| CUDA / foreach | 7.02 ms | 7.14 ms | 125.73 ms | 20.66 ms |
| CUDA / fused | 6.95 ms | 7.09 ms | 123.55 ms | 19.14 ms |

The checkpoint is approximately 430 KB per mode. Rust save cost includes CPU
state transfer, serialization, hashing, fsync and atomic publication. Python's
recorded tensor-save time excludes manifest/hash/fsync and is not a persistence
speed comparison. Rust complete probe times are 0.308 s, 0.816 s and 0.759 s in
table order, including validation, diagnostic files and checkpoint publication.
Cold GPU kernel initialization affected earlier probes substantially; compare
warm timings rather than mixing them with startup. The tiny differences here do
not establish a robust speedup, and fused has no demonstrated advantage on this
small model. Retain foreach as the default. Whole-run useful-work throughput and
resource behavior remain a gate for the native-worker integration.

## Reproduction and retained artifacts

Use a separate environment with Python 3.12, `torch==2.13.0`, NumPy and safetensors.
The following commands assume the shell is in the repository root. Set `LIBTORCH`
to that environment's `site-packages/torch` directory. The measured installation
uses the C++11 ABI value `1`; verify the ABI for another installation.

```sh
export LIBTORCH="$PWD/.venv/lib/python3.12/site-packages/torch"
export LIBTORCH_CXX11_ABI=1
export LD_LIBRARY_PATH="$LIBTORCH/lib"
export CUBLAS_WORKSPACE_CONFIG=:4096:8
cargo build --release -p aic-cli --features learning
.venv/bin/python tools/factory-edit-pilot/diagnostics/rust_learner_export.py \
  --source /home/ashrimp/Srv/aic-response-opt-20260915/artifacts/shared-normal/seed-149 \
  --output artifacts/kernel-input-multistage
.venv/bin/python tools/factory-edit-pilot/diagnostics/rust_learner_compare.py \
  --input artifacts/kernel-input-multistage/manifest.json \
  --binary target/release/aic-cli --output artifacts/kernel-gate-final --steps 16
```

Export and output destinations must not already exist; reuse the retained input
instead of re-exporting it when reproducing just the kernel gate. The comparison
script requires CUDA to exercise all three modes. For a single CPU diagnostic:

```sh
target/release/aic-cli learning verify-kernel \
  --input artifacts/kernel-input-multistage/manifest.json \
  --output artifacts/cpu-probe --device cpu --steps 4
```

Retained remote root: `/home/ashrimp/Srv/aic-rust-learning-20260915`, with
`artifacts/kernel-input-multistage` and `artifacts/kernel-gate-final`. Local copies
are under `artifacts/rust-learner-foundation-20260915/artifacts/`. These preserve
actual inputs, continuous/prefix/resumed checkpoints, oracle tensors and reports.
Regenerable intermediate probes are removed after final evidence is retained;
source training checkpoints and the previous normal environment remain intact.

Verification: local `cargo fmt --all`, `cargo check --workspace`, and
`cargo test --workspace`; server release CLI build, the numerical/recovery and
rejection matrix, and feature-enabled workspace check/tests. Run
`node docs/maintenance/check-tracked-files.mjs --worktree` and `--index` for the
completed source/documentation unit before committing.
