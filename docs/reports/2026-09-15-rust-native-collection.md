# Rust native collection integration gate

## Result and scope

The Rust collector now connects persistent native sessions, central batched CUDA
inference, actual behavior sampling, frozen historical observations, true-terminal
GAE and one real PPO/SIL minibatch per chunk. The executable has no Python runtime
or PyO3/NumPy dependency. The existing extension remains available to the Python
harness through its default feature.

This completes the next integration boundary after
[the numerical learner gate](2026-09-15-rust-learner-kernel.md). It is explicitly
**not the production training cutover**. The diagnostic consumes 128 retained,
authenticated canonical inputs and the existing kernel checkpoint; it does not
replace input generation, curriculum, the global success buffer, full PPO epoch/KL
scheduling, held-out evaluation or complete-wave publication. Those capabilities
remain required before adoption. No normal training run was started.

The implementation follows
[the collection design](../designs/2026-09-15-rust-native-collection.md) and the
existing native ownership/execution contracts. Domain logic stays in
`aic-pilot-native`, numerical collection/credit/update/storage in `aic-learning`,
and CLI parsing in `aic-cli`. Pure input/catalog loading is shared with the Python
bindings. There is one native session implementation and one model implementation.

## Correctness evidence

The final bounded run used 128 logical streams, 16 fixed native workers, two
32-action chunks and the original 64-action episode horizon. It collected 8,192
charged actions. The first chunk had no true terminals; the second had exactly
128. Every retained best was freshly validated before publishing the boundary
checkpoint. Actual elapsed-time observations and all native work limits remained
active.

The Python oracle consumed the **same recorded actual native arrays** and
pre-update model/RNG state. For both 4,096-sample chunks:

- All 128 first-wave root/conditional-port choices and the resulting CPU sampler
  state matched exactly.
- All recorded rewards, values and actual frontiers produced exactly matching
  float32 GAE advantages and targets.
- The actual first 256-row minibatch plus 16 retained historical SIL labels
  produced identical gradients and updated state: 28 gradient fields and 113
  model/Adam/shuffle fields, maximum absolute error zero.

The first-wave comparison is not a claim of identical complete trajectories
across separately timed environment executions. Native elapsed time is an actual
input. The comparison evaluates every recorded transition's credit, but tests
policy sampling on each chunk's first wave and optimizer math on its explicitly
selected minibatch.

An earlier small diagnostic exposed a conditional-port bug: prefixes longer than
one port used a feature sum where the established policy requires a mean. This
was fixed, verified with actual four-port choices, and covered by a regression
that also rejects duplicate selected ports. Retained native-array ownership,
stale handles, logical ordering, partial worker replies and restore with a changed
physical worker count have native regression coverage.

## Recovery and rejection

A separate 8-stream run saved after 32 actions per episode. A new process restored
it with two physical workers and completed the remaining 32 actions. Every saved
learner tensor, including model, both Adam states and independent RNGs, matched
the restored pre-collection tensor exactly. All eight episodes ended at their
actual horizon. The resumed chunk also passed the Python sampling/credit/update
oracle.

The CLI rejected reserved geometry, changed input bytes and a corrupted checkpoint
tensor archive. A one-byte retained-input budget deliberately failed after the
first charged wave. Its failure artifact retained all eight historical numerical
inputs and choices, eight current observations, native snapshots and chunk
evidence, plus the learner/RNG state. It did not publish a completed checkpoint.
Failure artifacts are explicitly incomplete diagnostic evidence, not an alternate
resume format. A worker failure also retains successful replies from the same
exchange, including other workers, for the cold failure export.

Normal collection moves owned arrays through bounded queues and keeps historical
arrays immutable; it does not serialize/save per action. Numerical-input byte
accounting limits retained observation arrays, with failure checked after capturing
the charged wave. It is not an RSS cap: model tensors, current observations,
metadata and explicit oracle exports also consume memory. The measurement-only
wrapper reused the existing process-tree guard (180 seconds, 40 GiB RSS, 4 GiB
minimum free memory). That Python monitor is external to the measured executable.

## Bounded runtime measurement

The server used release Rust, Torch 2.13.0+cu130, CUDA device 0, deterministic
kernels, one central Torch CPU thread and foreach Adam. The collection diagnostic
performed one 256-row PPO/SIL minibatch after each frozen chunk.

| Stage or resource | Observed value |
| --- | ---: |
| Admission, including learner/catalog/input setup | 0.224 s |
| First 4,096-action collection | 2.271 s |
| Second 4,096-action collection | 1.142 s |
| Both actual minibatch updates | 0.426 s |
| Boundary checkpoint | 0.816 s |
| Whole CLI, including explicit oracle/evidence exports | 17.178 s |
| External process wall time | 18.139 s |
| Maximum child RSS | 5,953,642,496 bytes |
| Retained numerical arrays, first/second chunk | 883,582,816 / 804,838,064 bytes |

The 3.413-second collection total establishes a bounded integrated native path;
it is not whole-training throughput. The prior Python baseline's 24,330 steps/min
includes different downstream work, and its recorded 8.364-second collection pair
is not a controlled speedup denominator for this new run. Policy state, actual
timing inputs, downstream PPO scheduling and publication must be aligned for an
adoption comparison. Expensive oracle exports deliberately dominate the diagnostic
wall time and must not become normal per-chunk work.

## Reproduction

Use the environment and retained kernel input from the numerical learner report.
The original source reader validates generator/input contracts during this one-time
export; it does not rebrand the old bank as generated by the new executable.

```sh
export LIBTORCH="$PWD/.venv/lib/python3.12/site-packages/torch"
export LIBTORCH_CXX11_ABI=1
export LD_LIBRARY_PATH="$LIBTORCH/lib"
export CUBLAS_WORKSPACE_CONFIG=:4096:8
cargo build --release -p aic-cli --features learning
.venv/bin/python tools/factory-edit-pilot/diagnostics/rust_collection_export.py \
  --source-root /home/ashrimp/Srv/aic-response-opt-20260915 \
  --launch /home/ashrimp/Srv/aic-response-opt-20260915/artifacts/shared-normal/launch.json \
  --output artifacts/native-collection-input-128 --streams 128
.venv/bin/python tools/factory-edit-pilot/diagnostics/rust_collection_measure.py \
  --measurement artifacts/native-collection-measure-complete -- \
  target/release/aic-cli learning verify-collection \
  --config artifacts/native-collection-input-128/manifest.json \
  --kernel-input artifacts/kernel-input-multistage/manifest.json \
  --output artifacts/native-collection-complete-128 --chunks 2
.venv/bin/python tools/factory-edit-pilot/diagnostics/rust_collection_compare.py \
  --run artifacts/native-collection-complete-128 \
  --output artifacts/native-collection-oracle-complete-128
```

Reuse retained inputs; output paths must be new. For the recovery diagnostic,
export eight streams, run one chunk to `artifacts/native-collection-prefix`, then
run one chunk to `artifacts/native-collection-resumed` with
`--resume artifacts/native-collection-prefix/checkpoint --workers 2`. Run its
oracle using the same comparison script, and the negative/recovery checks with:

```sh
.venv/bin/python tools/factory-edit-pilot/diagnostics/rust_collection_recovery.py \
  --binary target/release/aic-cli \
  --config artifacts/native-collection-input-8/manifest.json \
  --kernel-input artifacts/kernel-input-multistage/manifest.json \
  --prefix artifacts/native-collection-prefix \
  --resumed artifacts/native-collection-resumed \
  --output artifacts/native-collection-recovery-final
```

Actual arrays, checkpoints and incomplete-failure evidence remain under the
ignored remote root `/home/ashrimp/Srv/aic-rust-learning-20260915/artifacts`, with
local retained copies under `artifacts/rust-native-collection-20260915`. Compact
measurements, parity results, artifact identities and verification results are in
[the evidence manifest](2026-09-15-rust-native-collection-evidence.json).


## Verification and retention

`cargo fmt --all`, local `cargo check --workspace` and local
`cargo test --workspace` passed (575 tests; five existing ignored tests). The
release feature-enabled server workspace passed 580 tests, with five ignored;
the Python-disabled native crate passed 32 tests, with three ignored. Rebuilding
the extension and running `test_declarative_*.py` passed all 81 tests. The prior
16-update numerical/recovery matrix passed again on CPU foreach, CUDA foreach
and CUDA fused Adam. The final CLI dependency tree contains neither PyO3 nor
NumPy. File hygiene and source/artifact identity checks passed.

The retained integration inputs, outputs, oracles, failure evidence and regression
checkpoints total 409,193,989 bytes, excluding verification logs. Local copies were
checked against every retained remote artifact tree. Two failed, regenerable
Python oracle outputs were removed after retaining the corrected comparison;
native checkpoints and non-replayable evidence remain. All bounded diagnostic
processes exited; normal training remains stopped.
