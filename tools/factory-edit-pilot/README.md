# Fixed-factory edit pilot

## Blueprint-preserving draft experiment

The new `blueprint-edit-v1` experiment permits facility overlap and pending
connections across multiple private actions, while freezing the original directed
blueprint. Wrong endpoints, rewiring and altered metadata are rejected before
draft mutation. Only complete native validation permits a witness export; final
results always return the best valid factory. It is a separate experimental
contract with a new graph actor/critic input, not an existing PPO checkpoint mode.

After the native build below, select its library for your platform and run:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/probe_blueprint_edit.py --spec tools/factory-edit-pilot/blueprint-edit-probe.json --native-library "$AIC_NATIVE_LIBRARY" --output target/blueprint-edit-probe
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/plot_blueprint_edit.py --probe target/blueprint-edit-probe --output target/blueprint-edit-probe/contract.png
```

The probe uses the registered crossing-witness source fixture, checks original
branch/merge/bridge incidence, and runs an untrained graph-model forward/backward
check. It does not train a policy or establish a learning improvement. See the
[contract](../../docs/designs/2026-09-11-blueprint-edit-contract.md) and
[verified results](../../docs/reports/2026-09-11-blueprint-edit-contract.md).

The persistent-draft PPO learner keeps native sessions alive across 32-action
rollout chunks inside 256-action episodes. Its separate checkpoint contract
retains private geometry, both Adam states and sampling/shuffle RNGs. The
[registered learning screen](../../docs/designs/2026-09-11-blueprint-persistent-ppo.md)
uses two fresh seeds and compares timed inference to each untrained checkpoint.

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/blueprint_ppo.py train --config tools/factory-edit-pilot/blueprint-ppo.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/blueprint-readiness --readiness
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/audit_blueprint_ppo.py --run artifacts/factory-edit-pilot/blueprint-readiness
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/blueprint_ppo.py train --config tools/factory-edit-pilot/blueprint-ppo.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/blueprint-learning
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/audit_blueprint_ppo.py --run artifacts/factory-edit-pilot/blueprint-learning
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/summarize_blueprint_ppo.py --run artifacts/factory-edit-pilot/blueprint-learning
```

The registered PPO/BC comparison is defined in
`docs/designs/2026-09-11-blueprint-ppo-bc.md`. Its external configuration is
`blueprint-ppo-bc.json`. Build and audit the configured dataset with
`build_blueprint_bc_data.py --config CONFIG --output DATA_DIRECTORY` and
`audit_blueprint_bc_data.py --dataset DATA_DIRECTORY/dataset.pt.gz`.
Run each fresh arm using `blueprint_bc.py train --config CONFIG --native-library
LIBRARY --output ARM_DIRECTORY --coefficient 0` or `--coefficient 0.05`.
Add `--readiness` for the short mechanical check. Audit an arm using
`audit_blueprint_bc.py --run ARM_DIRECTORY`; `blueprint_bc.py resume --output
ARM_DIRECTORY` restores its durable checkpoint. Teacher paths contain the useful
move and validated repair/commit, and are auxiliary actor labels only.
After an arm passes its audit, run `evaluate_blueprint_bc_completion.py --run
ARM_DIRECTORY` and `audit_blueprint_bc_completion.py --run ARM_DIRECTORY` to
measure autonomous completion on held-out drafts and verify its recorded sampling.
Compare both audited arms with `summarize_blueprint_bc.py --control CONTROL
--treatment TREATMENT --output OUTPUT`. The
[BC screen report](../../docs/reports/2026-09-11-blueprint-ppo-bc.md) records improved
within-fixture compaction and the remaining limits of post-pose completion.

`blueprint_ppo.py resume --output RUN` restores the last complete durable update
under identical recorded sources and inputs. Preserve `interrupted.pt.gz` when a
run fails: it records partial experience beyond that update. Resuming re-collects
that incomplete chunk from its last durable checkpoint; wall-clock observations
can differ. The audit exactly reconstructs every **retained learner update**,
checks native draft continuity and bootstraps, and independently validates every
unique retained commit/final witness. It does not claim future sampling replay.
The [first learning report](../../docs/reports/2026-09-11-blueprint-persistent-ppo.md)
records passing mechanics but lower aggregate final area improvement than the
untrained policies; the trained checkpoints are experimental evidence.

`probe_post_pose_recovery.py --run SOURCE_RUN --config
tools/factory-edit-pilot/blueprint-post-pose-recovery.json --output FRESH_OUTPUT`
restores every accepted training pose and probes bounded fixed-pose recovery.
`summarize_post_pose_recovery.py --run FRESH_OUTPUT` independently audits the
source states/witnesses and compares recovery opportunities with actual policy
continuations. The [recovery diagnosis](../../docs/reports/2026-09-11-blueprint-post-pose-recovery.md)
separates geometry requiring further movement, route-order failures and
recoverable drafts that the policy discards before committing.

## Historical fixed-factory pilot

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

## Two-case learning study

The [predeclared protocol](../../docs/designs/2026-09-08.05-two-case-learning-experiment.md)
compares A-only, B-only and alternating A+B at three training seeds. `case-spread.json` is a
selected external runtime fixture with the same production obligations and feature meanings as
`case.json`, but wider facility gaps, a bent internal route and longer tails. Its separate
21-command witness achieves five successive valid area reductions from 385 to 275. Witness
commands are not training demonstrations. Each training episode resets its selected original
root; this experiment adds neither curriculum nor best-layout carryover.

With a clean committed checkout and the explicit release library, run the entire sequential
matrix, independent audits and comparison plots:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/two_case_matrix.py --protocol tools/factory-edit-pilot/two-case-study.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/two-case-learning
```

Solo conditions receive 65,536 actions; mixed training receives 131,072, including a frozen
65,536 checkpoint. Evaluate both cases separately at every checkpoint. The final comparison
reports equal total allowance and equal per-case exposure separately, including all three
independently trained seeds. A single condition can be run with `two_case.py --protocol <path>
--condition A+B --seed 17 --native-library <path> --output <fresh-directory>`.

Per-case subdirectories retain validated roots, witnesses, training champions, frozen evaluation
rows and selected layouts/traces. Shared checkpoints store the model/Adam/RNG, both native
snapshots, scheduler and per-case exposure. A separate interrupted checkpoint preserves partial
collection and its active case. Automatic continuation of this study checkpoint format is not
implemented; do not pass it to the single-case `run.py --resume` interface.

`two_case_audit.py --protocol <path> --output <run> --source-commit <recorded-commit>` independently
checks both case outputs and the declared exposure. `two_case_compare.py --protocol <path>
--runs <all-declared-runs> --output <fresh-directory>` requires passing audits, common source/
runtime/configuration and equal initial parameters within each training seed.

The [completed three-seed results](../../docs/reports/2026-09-08.12-two-case-learning-results.md)
contain all nine runs: no positive training or evaluation reward, despite verified improving
witnesses. Neither matched-total nor matched-per-case comparison showed a benefit in this matrix.

## Curriculum and concurrent PPO study

The [frozen contract](../../docs/designs/2026-09-08.06-curriculum-parallel-ppo.md)
compares root/curriculum starts with serial/concurrent collection. All conditions use
1,024 sampled edits per PPO update. Curriculum uses verified zero-reward prefixes,
never supervised action targets; every frozen evaluation starts at the original root.
The concurrent collector runs eight native environments in a persistent thread pool
with the GIL released for native observation/transition work, and batches inference
on the learner device. It samples actions with independent CPU generators per stream.

After rebuilding the extension, run the matrix from a clean committed checkout:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/curriculum_matrix.py --protocol tools/factory-edit-pilot/curriculum-parallel-study.json --native-library target/release/libaic_pilot_native.so --output artifacts/factory-edit-pilot/fresh-curriculum-parallel
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/parallel_benchmark.py --protocol tools/factory-edit-pilot/curriculum-parallel-study.json --native-library target/release/libaic_pilot_native.so --output artifacts/factory-edit-pilot/fresh-parallel-benchmark --repeats 5
```

Use `.dylib` for the local macOS extension. The frozen full protocol requires CUDA;
small CPU verification protocols can reference an explicit external configuration.
`curriculum_study.py` runs one condition/seed, `curriculum_audit.py` independently
replays selected traces and verifies source/exposure/checkpoints, and
`curriculum_compare.py` checks the complete matched matrix and renders root outcomes.
Setup actions, curriculum episode gains and original-root gains remain separate.
Interrupted checkpoints retain per-stream state, RNG, partial rollout and pending
transition evidence; no automatic resume is claimed.

The completed [curriculum/concurrent results](../../docs/reports/2026-09-08.13-curriculum-parallel-ppo-results.md)
report all 12 runs, separate throughput from root learning outcomes, and link compact
provenance. `curriculum_evaluation_replay.py` reproduces every positive original-root
evaluation in a completed run and independently replays its native transitions.

The [curriculum failure analysis](../../docs/reports/2026-09-08.14-curriculum-failure-analysis.md)
measures frozen success-action probabilities, reconstructs first-rollout GAE gradients,
and checks immediate recoverability after initial sampled edits. Reproduce it with
`curriculum_diagnosis.py`; it reads checkpoints and performs no optimizer updates.

## Short episodes and mastery-gated advancement

The [mastery contract](../../docs/designs/2026-09-08.07-mastery-curriculum.md) crosses
8/128-edit training episodes with fixed/performance-based advancement. Every condition
uses 1,024 PPO samples per update; original-root evaluation remains 128 edits. Gates
use separate 8-edit, 64-seed assessments, requiring both cases to pass 52/64 twice.
Assessment seeds are development feedback, not independent outcome evaluation.

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/mastery_matrix.py --protocol tools/factory-edit-pilot/mastery-study.json --native-library target/release/libaic_pilot_native.so --output artifacts/factory-edit-pilot/fresh-mastery-learning
```

`mastery_study.py` executes one condition/seed. `mastery_audit.py` checks sample/reset
exposure, advancement and checkpoints, and independently reproduces every native
assessment using the Python reference environment. `mastery_compare.py` requires all
matched runs before comparing root outcomes, learning time and stage progress.
Checkpoint files exist at every assessment, including assessments without root evaluation.
No critic initialization, reward, candidate mask or supervised loss is changed.

The [completed short/mastery results](../../docs/reports/2026-09-08.15-mastery-curriculum-results.md)
include twelve audited runs, stage transitions and root improvement in two short-mastery
training seeds. `curriculum_evaluation_replay.py` also accepts this experiment family;
all 70 positive root evaluation episodes were reproduced independently.

## Spatial BatchNorm experiment

`run-server-cuda-batchnorm.json` selects `normalization: "batch"`; the existing
no-normalization configurations remain controls. The three-seed
`mastery-batchnorm-study.json` repeats the short-mastery condition with affine BN
behind each convolution. Padding is excluded from statistics. Running statistics
stay fixed through collection and PPO, then receive one training-only calibration
pass per update; calibration counts/time and buffers are retained. See the
[contract](../../docs/designs/2026-09-08.08-batch-normalized-policy.md).

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/mastery_matrix.py --protocol tools/factory-edit-pilot/mastery-batchnorm-study.json --native-library target/release/libaic_pilot_native.so --output artifacts/factory-edit-pilot/fresh-batchnorm-learning
```

The [completed BN comparison](../../docs/reports/2026-09-08.16-batchnorm-policy-results.md)
reports higher B success consistency in all three seeds, limited A transfer and
calibration cost. The baseline remains available for controlled follow-up.

## Case-specific full curriculum

The [stage-one contract](../../docs/designs/2026-09-08.09-case-specific-curriculum.md)
advances A and B independently through every first-success witness prefix to their
own original roots. Fresh on-policy rehearsal occupies one quarter of each case's
episodes once easier stages exist. `case_curriculum_study.py` records per-stream
levels and actual root exposure; `case_curriculum_audit.py` reconstructs advancement
and rehearsal independently. Original-root outcome gates remain separate from feedback.

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/case_curriculum_matrix.py --protocol tools/factory-edit-pilot/case-curriculum-study.json --native-library target/release/libaic_pilot_native.so --output artifacts/factory-edit-pilot/fresh-case-curriculum
```

The collector now requires one level per stream, and start tables enumerate all
witness-prefix levels. Earlier controlled studies supply uniform level vectors;
historical artifacts remain associated with their original source revisions.

The [full case-curriculum results](../../docs/reports/2026-09-08.17-case-curriculum-results.md)
show original-root learning on both cases, followed by late regression in two policies.
The declared final-checkpoint milestone remains unmet.

The [mastery retention follow-up](../../docs/designs/2026-09-08.10-mastery-policy-selection.md)
selects the first jointly mastered checkpoint using development feedback, then evaluates
it on two untouched root sampling banks. It does not revise the failed continued-training
criterion. Keep the audited source checkpoints available at the protocol's source run root.

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/mastery_policy_selection.py --protocol tools/factory-edit-pilot/mastery-selection-study.json --native-library target/release/libaic_pilot_native.so --output artifacts/factory-edit-pilot/fresh-mastery-selection
```

The [relocation milestone](../../docs/designs/2026-09-08.11-learned-facility-relocation.md)
uses the selected `case-relocation.json` runtime fixture. Its fixed-pose area bound
prevents improvement by simply trimming boundary tails. Train its full nine-level
curriculum with `case_curriculum_matrix.py --protocol tools/factory-edit-pilot/relocation-curriculum-study.json`
and the usual native-library/output flags. Milestone acceptance uses a retained
root-mastered policy and requires final accepted x/y translation; the matrix's final
checkpoint score gate is a separate stability diagnostic.

The [frozen confirmation follow-up](../../docs/designs/2026-09-08.12-frozen-root-confirmation.md)
uses `mastery_policy_selection.py --protocol tools/factory-edit-pilot/relocation-confirmation-study.json`
with the usual native-library/output flags. It confirms each first root-passing policy
on an independent development bank before opening outcome banks. Failed confirmation
opens no outcome bank and never selects a later checkpoint.

## Sampling and action-length admission correction

Batched samplers now draw only from each observation's actual candidate count.
Previously zero-probability padding consumed generator draws: a fixed episode seed
could produce different traces when companion candidate counts or wave width
changed. This correction changes seeded trajectories relative to historical
source revisions. Retained historical experiments remain evidence for their frozen
sampling procedure; reproduce them with their recorded source and native hashes.
A seed alone does not establish cross-version or cross-hardware trace identity.
No compatibility sampler is retained. Independent generators now ignore companion
padding; floating-point differences between devices remain a separate limitation.

Native and reference admission reject duplicate `move_lengths` or `segment_lengths`
with `input.config.duplicate_length`, naming the affected key. Remove duplicates
from external configuration before rerunning. Existing tracked configurations
already contain distinct lengths. See the [four-perspective audit](../../docs/reports/2026-09-08.44-training-implementation-audit.md).

## Native PPO data supply and recovery

The native-v4 `ppo_harness.py` uses Linux collectors with anonymous shared-memory
array delivery. It stores full experiences and traces only for training batches
explicitly listed in `retention.training_batches`; held-out evaluation evidence
and compact result/checkpoint/curriculum records remain durable. `resume` restores
the last committed actor, critic, Adam, shuffle RNG and curriculum plan, then
resamples the entire uncommitted phase. Older runs require their frozen runtime.

CPU input preparation overlaps one minibatch ahead, with immutable per-update
feature caching and asynchronous CUDA tensor transfers. PPO minibatch size,
shuffle order, FP32 arithmetic and stopping rules are unchanged. See the
[data-supply contract](../../docs/designs/2026-09-10-ppo-data-supply.md) and
[verified measurements](../../docs/reports/2026-09-11-ppo-data-supply.md).

Learner execution v3 additionally batches variable-size CNN inputs by padding
to each minibatch's maximum extent. An explicit validity mask excludes padding
from normalization, every spatial layer and pooling. CPU-prepared addresses
combine node/action/member gathers across factories. Model channels and PPO
settings are unchanged, but floating-point reduction order changes: old and new
Adam continuations are not bit-exact. Use `benchmark_ppo_indexing.py` with an
explicit frozen baseline to measure forward/gradient differences and full-update
roundoff. See the [padding measurements and limitations](../../docs/reports/2026-09-11-ppo-batched-indexing.md).

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/ppo_harness.py prepare --config tools/factory-edit-pilot/ppo-data-supply-readiness.json --output artifacts/factory-edit-pilot/data-supply-readiness
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/ppo_harness.py run --run artifacts/factory-edit-pilot/data-supply-readiness --duration-seconds 900
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/check_ppo_harness_native.py --run artifacts/factory-edit-pilot/data-supply-readiness --output artifacts/factory-edit-pilot/data-supply-readiness-audit
```

This bounded configuration requires the explicit frozen initial-factory manifest
at `artifacts/factory-edit-pilot/region-starts/manifest.json`, as used by the
64-factory curriculum readiness experiment. Retained experience profiling and
old-runtime timing comparisons are separate diagnostic CLIs:
`profile_ppo_data_supply.py`, `profile_ppo_payload.py`,
`benchmark_ppo_data_supply.py`, `benchmark_ppo_collection_supply.py` and
`compare_ppo_supply_runs.py`. `check_ppo_batch_recovery.py` exercises actual CLI
stop/resume against an uninterrupted readiness reference. Each requires explicit
inputs and fresh output.

For a bounded live progress snapshot, use `profile_ppo_progress.py --run RUN
--through COMMITTED_BATCH --output FRESH_OUTPUT --plot`. It reads committed rows
and complete evaluation traces without GPU execution or changing the learner.
The default windows cover twenty training batches and five evaluations. It
reports action-weighted rejection rates, per-kind realized area contribution,
physical repeats, early/later gains, curriculum depth and KL stopping. The
optional static figure requires matplotlib in the diagnostic environment.

## Actor/critic combination screen

`run_ppo_combinations.py` prepares the registered 2 x 2 x 2 comparison of shared
encoders, critic action history and critic fitting after policy KL stopping.
The [experiment contract](../../docs/designs/2026-09-11-ppo-actor-critic-combinations.md)
specifies matched fresh starts, two initialization seeds and a fixed actor input.
Shared weights have one optimizer owner. Critic-only tails freeze every actor
parameter, including shared encoders; only private critic parameters keep learning.
Evaluation records include predictive value error against realized remaining
reward, separate from the training-target error measured after an update.

```sh
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env prepare --config tools/factory-edit-pilot/ppo-actor-critic-screen.json --output artifacts/factory-edit-pilot/combinations-readiness --readiness
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env run --matrix artifacts/factory-edit-pilot/combinations-readiness/matrix.json
```

Audit each completed readiness cell with `check_ppo_harness_native.py`. Then
prepare a new output directory without `--readiness` and execute its matrix.
Cells run sequentially through the supervised public PPO CLI. The screen retains
all checkpoints, selected training traces and every evaluation trace. Use
`compare_ppo_combinations.py --matrix MATRIX --output FRESH_OUTPUT --validate-final-layouts`
to check hashes, matching inputs, predictive diagnostics and paired factor effects.
Render its report with `plot_ppo_combinations.py --report REPORT --output FRESH_OUTPUT`.
The figures show both seeds and their range; those ranges are not confidence
intervals. Critic errors are measured on each policy's own trajectories, so
changed state visitation and return variance can affect comparisons.

`probe_ppo_critic_tail.py --run STOPPED_RUN --output FRESH_OUTPUT` checks the
original independent critic at retained batches 128, 192 and 256. It first
reproduces each recorded update exactly, then finishes the remaining critic
epochs while freezing the actor and its optimizer. Both critics predict on
identical, newly collected held-out trajectories. This separates the immediate
predictive effect of extra fitting from subsequent policy improvement. It
requires the original CUDA environment and retained training experiences; run
it after the matrix to avoid interfering with measured training time. Default
limits are 32 held-out attempts per snapshot, eight workers and 900 seconds.
Pass its completed report to the plot command with `--critic-probe REPORT` to
show mean predictions, realized returns and predictive MAE by action position.

For an online matrix, run `check_ppo_combinations_tracking.py --matrix MATRIX
--output FRESH_REPORT_JSON --env-file /path/to/tracking.env` after completion.
It verifies all sixteen remote runs, every exported scalar and final history
ordinals, including the five comparison rows added after each seed finishes.

The [completed factorial and mature-critic findings](../../docs/reports/2026-09-11-ppo-actor-critic-combinations.md)
include all eight combinations, both seeds, conditional effects and limitations.

### Lower learning rates for the wider shared model

`ppo-learning-rate-screen.json` fixes width 40, shared encoders, critic history
and critic tails. The explicit `--learning-rates` matrix changes only the fixed
Adam learning rate for both learners, to 0.0001 and 0.00003. Compare against the
retained 0.0003 cell of the completed capacity experiment. The
[registered contract](../../docs/designs/2026-09-11-ppo-learning-rate-screen.md)
specifies matched seeds, workload, initial conditions and required audits.
The [completed comparison](../../docs/reports/2026-09-11-ppo-learning-rate-screen.md)
reports worse final quality at both lower rates on both seeds under this budget,
despite fewer KL stops and more actor updates.

```sh
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env prepare --config tools/factory-edit-pilot/ppo-learning-rate-screen.json --output artifacts/factory-edit-pilot/learning-rate-readiness --learning-rates --readiness
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env run --matrix artifacts/factory-edit-pilot/learning-rate-readiness/matrix.json
python tools/factory-edit-pilot/check_ppo_harness_native.py --run artifacts/factory-edit-pilot/learning-rate-readiness/lr-1e-4 --output artifacts/factory-edit-pilot/learning-rate-readiness-audit-1e-4
python tools/factory-edit-pilot/check_ppo_harness_native.py --run artifacts/factory-edit-pilot/learning-rate-readiness/lr-3e-5 --output artifacts/factory-edit-pilot/learning-rate-readiness-audit-3e-5
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env prepare --config tools/factory-edit-pilot/ppo-learning-rate-screen.json --output artifacts/factory-edit-pilot/learning-rate-matrix --learning-rates
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env run --matrix artifacts/factory-edit-pilot/learning-rate-matrix/matrix.json
```

After both cells finish, audit their native results and every checkpoint's Adam
learning rate, then verify W&B and compare against the retained capacity audit.
The rate comparison requires identical initial evaluation and first-batch
collection, including action choices, behavior probabilities, value predictions
and native outcomes. It reports actual epochs, optimizer steps, KL and clipping
diagnostics alongside quality. Earlier source trees and evidence must remain
available at the paths recorded in the baseline provenance.

```sh
python tools/factory-edit-pilot/compare_ppo_combinations.py --matrix NEW_MATRIX --output RATE_AUDIT --validate-final-layouts
python tools/factory-edit-pilot/check_ppo_combinations_tracking.py --matrix NEW_MATRIX --output TRACKING_REPORT --env-file /path/to/tracking.env
python tools/factory-edit-pilot/compare_ppo_learning_rates.py --baseline-provenance docs/reports/2026-09-11-ppo-shared-capacity-provenance.json --lower-report RATE_AUDIT/report.json --lower-inputs RATE_AUDIT/inputs.json --output RATE_COMPARISON
python tools/factory-edit-pilot/plot_ppo_learning_rates.py --report RATE_COMPARISON/report.json --output RATE_PLOTS
```

### Shared model with approximately matched total capacity

`ppo-shared-capacity-screen.json` uses the explicit `ppo-shared-capacity-v1`
model: shared CNN/graph/history encoders and coupled heads at width 40. It has
86,282 unique parameters, versus 93,090 for the separate width-32 pair and
58,530 for the shared width-32 pair. Depth and normalization remain unchanged.
See the [capacity contract](../../docs/designs/2026-09-11-ppo-shared-capacity.md).

The supplied cell enables critic history and stops both learners at policy KL
stopping. Its other settings match the previous 32-batch, two-seed screen.
Both critic factor booleans remain configurable; sharing must be enabled.
Prepare this individual cell through the harness, or use the explicit
`--shared-capacity` matrix mode for all four history/tail combinations.
Use fresh weights because width-32 checkpoints cannot load into
width 40. The completed [four-condition, two-seed experiment](../../docs/reports/2026-09-11-ppo-shared-capacity.md)
reports all final scores, learning curves, paired controls and audit evidence.

```sh
python tools/factory-edit-pilot/ppo_harness.py --env-file /path/to/tracking.env prepare --config tools/factory-edit-pilot/ppo-shared-capacity-screen.json --output artifacts/factory-edit-pilot/shared-capacity-screen
python tools/factory-edit-pilot/ppo_harness.py --env-file /path/to/tracking.env run --run artifacts/factory-edit-pilot/shared-capacity-screen
```

The [capacity experiment contract](../../docs/designs/2026-09-11-ppo-shared-capacity-experiment.md)
registers the full comparison against the earlier width-32 controls. Run bounded
native/CUDA readiness before the main four-cell matrix:

```sh
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env prepare --config tools/factory-edit-pilot/ppo-shared-capacity-screen.json --output artifacts/factory-edit-pilot/shared-capacity-readiness --shared-capacity --readiness
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env run --matrix artifacts/factory-edit-pilot/shared-capacity-readiness/matrix.json
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env prepare --config tools/factory-edit-pilot/ppo-shared-capacity-screen.json --output artifacts/factory-edit-pilot/shared-capacity-matrix --shared-capacity
python tools/factory-edit-pilot/run_ppo_combinations.py --env-file /path/to/tracking.env run --matrix artifacts/factory-edit-pilot/shared-capacity-matrix/matrix.json
```

After completion, `compare_ppo_combinations.py` audits either registered matrix
schema, including native final layouts and exact stop/tail prefixes. The wider
screen reports the two history/schedule effects and their interaction. Verify
its online records with `check_ppo_combinations_tracking.py`.

`compare_ppo_capacity.py` then checks the retained baseline report and every
referenced input identity against its compact provenance, verifies equal
workloads/native binaries and paired factory geometry, and compares each wider
cell with its matching shared and separate width-32 controls. It reports final
levels and changes from each model's own initialization separately. The baseline
source trees and retained results must remain available at their recorded paths.

```sh
python tools/factory-edit-pilot/compare_ppo_combinations.py --matrix NEW_MATRIX --output NEW_AUDIT --validate-final-layouts
python tools/factory-edit-pilot/check_ppo_combinations_tracking.py --matrix NEW_MATRIX --output TRACKING_REPORT --env-file /path/to/tracking.env
python tools/factory-edit-pilot/compare_ppo_capacity.py --baseline-provenance docs/reports/2026-09-11-ppo-combinations-provenance.json --wide-report NEW_AUDIT/report.json --wide-inputs NEW_AUDIT/inputs.json --output CAPACITY_COMPARISON
python tools/factory-edit-pilot/plot_ppo_capacity.py --report CAPACITY_COMPARISON/report.json --output CAPACITY_PLOTS
```
Lower later absolute gain can reflect improvements moving earlier; compare it
with final factory quality rather than treating it as a separate objective.
