# Fixed-factory edit pilot

This is the explicit local-edit learning pilot, separate from the production constructor and
isolated global exact research. A 100,898-parameter CNN/object MLP scores the seven declared
local commands. PPO starts from fresh weights; every episode starts from the same complete
factory. There is no demonstration pretraining, route search, repair macro or global fallback.
See the [pilot contract](../../docs/designs/2026-09-07.01-factory-edit-learning-pilot.md) and
[Rust ownership contract](../../docs/designs/2026-09-08.04-rust-owned-factory-edit-environment.md).

Rust now owns runtime catalogs, exact request validation, draft/working-base/episode-best state,
geometry, candidates, edits, independent acceptance, scores/reward, traces and observation arrays.
Python owns PyTorch inference/sampling, PPO/GAE/Adam, rollout storage, evaluation scheduling,
plots and top-level checkpoints. Training and frozen evaluation use the same Rust engine.
The hot boundary returns owned NumPy arrays and an opaque candidate handle and accepts one
selected index. Python does not reconstruct factories or run a second rule implementation.

The current geometry scope is non-crossing directed belt paths with one source/demand per
commodity network. The four-facility battery case and its rates, poses, rotations, ports and
transport capacity come from external files. `case.json` is a compact human-authored fixture;
its improving witness checks the environment and is not training data. Width/height are actual
used outputs. The raster-work guard is an operational resource control, not a game boundary.

## Build and run

From the repository root:

```sh
uv sync --project tools/factory-edit-pilot --locked
PYO3_PYTHON="$PWD/tools/factory-edit-pilot/.venv/bin/python" cargo build --release -p aic-pilot-native --features extension-module
```

Select the built library explicitly: `target/release/libaic_pilot_native.so` on Linux or
`target/release/libaic_pilot_native.dylib` on macOS. For example, on Linux:

```sh
export AIC_NATIVE_LIBRARY="$PWD/target/release/libaic_pilot_native.so"
tools/factory-edit-pilot/.venv/bin/python -m unittest discover -s tools/factory-edit-pilot -v
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/run.py check --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/preflight
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/native-cpu-seed17
```

`--native-library` is required; a missing, incompatible or stale build is an explicit error.
The library embeds compiler/profile/target information and the hashes of its Rust sources,
manifests, build script and lockfile. Runtime startup checks those hashes against the checkout.
Rebuild after changing native source or Cargo inputs. Generated libraries and run outputs remain
ignored. An existing experiment metadata file is not overwritten; use fresh output directories.

A run records source/config/catalog/build hashes, actual package versions, raw training and
frozen-evaluation rows, full checkpoints, selected layouts/traces, and PNG/SVG figures.
The default allowance is 65,536 commands, 128 per episode, one training seed and five frozen
checkpoints. This fixed-root experiment does not test generalization. Flat learning is a valid
reported result. Score priority is used area, physical transport tiles, then turns; task reward
is only the improvement in best validated area relative to the initial area.

## CPU and CUDA execution

`device` selects the PPO learner. Both CPU and CUDA runs use the same native CPU environment,
CPU actor and CPU action-sampling RNG. CUDA learning refreshes the CPU actor after each complete
update. Float32, TF32 disabled, ordinary convolutions/autograd, model dimensions and the original
rollout/horizon/GAE ordering are preserved. CUDA availability is required when selected.

`run-server-cpu.json` and `run-server-cuda.json` use 16 intra-operation threads on the measured
server; `run.json` uses four on the local host. Inter-operation threads remain one. For example:

```sh
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run-server-cuda.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/native-cuda-seed17
```

CUDA PPO prepares each rollout once when its payload fits `prepared_rollout_max_bytes` and
reuses tensors across epochs, cropping every minibatch to its original dimensions. The default
payload ceiling is 1 GiB; the 32 GiB server configuration selects 4 GiB. Zero disables preparation.
Larger estimates use ordinary minibatch preparation with an explicit reported mode. The payload
ceiling excludes model/gradient memory. CPU updates keep ordinary minibatch preparation.

Optional `compile_actor` plus `compiler_cache_directory` compiles the CPU actor forward pass.
CUDA learning, CPU sampling and eager frozen evaluation retain their established behavior.
The matched `run-server-cuda-long-{eager,compiled}.json` files are historical long-run configs.
GPU batched collection remains a separate experiment and is not selected by this migration.

Metadata separates native candidates/observation/step/validation, array export, inference,
PPO stages and actor snapshot time. Validation is nested inside step; array export is separate
from observation. `costs.py` avoids double-counting those clocks. FFI dispatch and remaining
runner work stay in the residual wall time. All timings include actual work/transfer where
stated; they are not pure CUDA kernel times.

## Validate, visualize and audit

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/native.py check-layout --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --native-library "$AIC_NATIVE_LIBRARY" --layout artifacts/factory-edit-pilot/native-cpu-seed17/training-best.json
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/plots.py --case tools/factory-edit-pilot/case.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/native-cpu-seed17
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/audit.py --case tools/factory-edit-pilot/case.json --output artifacts/factory-edit-pilot/native-cpu-seed17 --source-commit <recorded-source-commit>
```

Plots draw native-validated geometry and scores. `audit.py` deliberately uses the isolated
`reference/` Python oracle to replay retained traces and independently accept selected layouts.
Normal training/evaluation/plotting never import that oracle. The audit checks source/input
provenance and, for native runs, embedded build hashes and the selected library artifact.
`learning-curves.png` separates frozen outcomes, episode reward and the training discovery
record. `factory-comparison.png` labels the scripted witness as unlearned.

## Checkpoints and continuation

Native run/checkpoint schema 2 replaces the former Python contract. Environment snapshots
contain all mutable layouts, scores, step/revision/generation, traces, counters, input identity
and configuration. Restore rebuilds acceptance and rejects incompatible inputs, malformed
accounting or false score claims before changing the live state. JSON float parsing preserves
roundtrip bits. Candidate handles from another engine or before reset/restore are invalid.

`run.py train --resume <trusted-final-checkpoint.pt>` supports completed CPU runs with an eager
actor. It restores model/Adam/RNG state, native environment state, cumulative episode/command
counts and the champion. Only total allowance and evaluation checkpoints may change. Runtime,
semantic source and native build changes are rejected. Legacy Python schema-1 checkpoints
produce `resume.legacy_checkpoint`; they are not silently converted or resumed.

```sh
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run-server-cpu-continue.json --native-library "$AIC_NATIVE_LIBRARY" --resume artifacts/factory-edit-pilot/native-cpu-seed17/checkpoint-65536.pt --output artifacts/factory-edit-pilot/native-cpu-seed17-continue
```

Continuation timing covers the current invocation; inherited time is retained in lineage.
Keep the parent directory. `plots.py --parent <parent-directory>` adds verified parent rows.
Interrupted runs retain a separate schema-2 `interrupted-state.pt`: native snapshot, partial
rollout, learner/actor/optimizer state, all RNG streams and the interrupted phase. It preserves
partial episodes for inspection/recovery; automatic CLI continuation still requires a completed
CPU final checkpoint. Do not discard this evidence assuming a seed can reproduce it.

## Parity and research tools

`native_transition_parity.py --case <case> --config <config> --input <input.pt> ...
--native-library <library> --output <fresh-result.json>` compares candidate order, every
observation byte, transitions, all mutable states, traces and counters with both the Python
oracle and retained observations. Direct native tests cover rotations, large coordinates,
array ownership, guards, handle errors, snapshot atomicity and continuation.

`native_engine_benchmark.py` takes the same inputs with a fresh output directory, `--repeats 11`
and `--collect-before-replay`. It alternates Python-reference/native timing after warm-up while
preserving automatic GC. Optional `--inference cpu` interleaves the actual CPU actor, and
`--inference cuda` probes GPU inference on the same recorded trajectory; inferred actions are
discarded. Setup and native array export are reported separately. The timed native loop uses
`observe()`/`step(handle, index)` and performs no diagnostic candidate/state export.

Historical Python-only environment and actor/update probes explicitly import `reference/`.
They remain research tools, never training fallbacks: `benchmark.py`, `environment_benchmark.py`,
`rollout_benchmark.py`, `actor_benchmark.py`, `actor_compile_benchmark.py`, `actor_parity.py`,
`collection_benchmark.py`, `preparation_benchmark.py` and `profile_environment.py`.
`compare.py --runs <run> ... --output <fresh-directory>` and `costs.py --runs <run> ...
--output <fresh-directory>` summarize retained outcomes and costs.

The old observation-only `GeometryEncoder` and Python bridge/benchmark/tests were removed.
To reproduce that historical experiment, check out `bcfcf34` and follow its README. Its
[report](../../docs/reports/2026-09-08.06-factory-edit-native-observation.md) measures a partial
migration and does not describe the current production environment.

Migration evidence: [state/snapshots](../../docs/reports/2026-09-08.07-factory-edit-native-state.md),
[candidates/transitions](../../docs/reports/2026-09-08.08-factory-edit-native-transitions.md), and
[complete observations](../../docs/reports/2026-09-08.09-factory-edit-native-observations.md),
[training cutover](../../docs/reports/2026-09-08.10-factory-edit-native-cutover.md), and
[final server results](../../docs/reports/2026-09-08.11-factory-edit-native-results.md).
Earlier learning and CPU/CUDA findings remain under `docs/reports/`; native migration does not
turn those single-case measurements into generalization or optimizer claims.
