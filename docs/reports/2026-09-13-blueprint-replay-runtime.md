# Measured runtime for the replay-balance screen

The explicit child runtime passes the complete-cycle throughput gate. Main
learning remains a separate command after source, cohort and launch admission.
The [compact provenance](2026-09-13-blueprint-replay-runtime-provenance.json)
binds the final measurements, source bundles, verification, panel and launch.
The registered scientific comparison is unchanged; see the
[experiment design](../designs/2026-09-13-blueprint-replay-balance-experiment.md).

## Continuation and evidence contract

`blueprint_replay_balance.py` exposes `admit`, `readiness`, `verify-fork`,
`prepare-panel`, `prepare`, `run`, `probe` and `report`. The external configuration
is `tools/factory-edit-pilot/blueprint-replay-balance.json`.

- Admission authenticates each completed centered update-512 parent, its original
  protocol/runtime, both Adam states, input files and referenced complete episodes.
  It compiles 287/206/311 parent sequences for seeds 17/29/41 and the five specified
  imported large successes. The imports retain all 15 recorded labels, original
  observations, full-episode returns, source counters and reconstructed native
  pre-label states. Native witnesses authenticate the original useful commits.
- `blueprint-replay-balance-checkpoint-v1` binds parent and launch identities,
  device, arm, child/global update clocks, both optimizers, policy/shuffle RNGs,
  eight native stream snapshots and the child replay buffer. Device migration
  checks exact imported model/optimizer values and parameter ordering. Child
  update zero keeps the parent's episode-256 boundaries.
- Parent and imported sources are immutable. The child buffer starts its own
  discovery and use counters. Physical/action/return identities deduplicate
  entries while preserving every origin reference. Actual source product metadata
  chooses the registered stratum. A child success becomes eligible only after
  its true terminal, beginning with the following optimizer update.
- Uniform replay keeps eight replacement draws. Balanced replay uses four large
  and four small replacement draws and its own deterministic order RNG. Both
  retain equal weighting of present stages per sequence. Recorded labels never
  enter PPO ratios, GAE, value targets or on-policy reward totals.
- Publication writes a chunk, checkpoint and update record before advancing the
  durable job state. Retention keeps 0/64/128 checkpoints, recovery states, live
  episodes and the complete source episodes of child successes. Evaluation
  reports survive trace cleanup; ordinary traces are retained only at 0/64/128.

The new child contract replaces attempts to resume a historical run under a
changed protocol. Source reconstruction runs once at explicit admission; normal
updates use immutable cached replay tensors and worker-owned live sessions.
No global exact solver, construction rule or production CLI changes are involved.
This stays within the blueprint-edit exception and the constructive planner's
complete-valid-best requirement.

## Complete-cycle measurement

The final measurement uses the same sixteen actual generated inputs across all
modes. Each mode has one warmup and three measured repetitions. A repetition
includes four updates, 1,024 training actions, sixteen completed episodes, the
unchanged 32-factory evaluation, checkpoint publication and fixed-anchor label
measurement. Every evaluation executed all 64 actions in all three modes.

| Mode | Warmed median | Warmed range |
| --- | ---: | ---: |
| Serial collection/evaluation, CPU learner | 18.210 s | 17.978–18.226 s |
| Eight workers, CPU learner | 8.580 s | 8.557–9.175 s |
| Eight workers, CUDA learner | 7.932 s | 7.922–7.941 s |

CUDA is selected for every main arm. The measured schedule is **2.296 times**
faster than the serial reference, exceeding the required 1.5 threshold. Parallel
evaluation takes approximately 2.2 seconds instead of 12.2 seconds per panel;
the warmed CUDA learner takes approximately 0.61 seconds per four updates.
The measured process tree peaks at 11,784,900,608 bytes of RSS, below 32 GiB;
peak CUDA allocation is 115,075,584 bytes. Eight distinct workers execute on
cores 0–7, with measured step concurrency eight and separate coordinator cores.
Affinity and effective CPU quota are checked before launch.

The forecast is 5,460 seconds, approximately 91 minutes. It includes 480 normal
cycles, initialization and transfer evaluation, a conservative undiscounted
3,600 two-second continuation branches divided among eight workers, startup and
setup reserve. This estimate is not a completion promise: each arm still has its
registered time/output ceiling and the matrix retains its full resource guards.

An earlier complete-cycle attempt exposed a serialization failure for empty
shared-memory tensor views of different dtypes at the same nonzero address.
The durable serializer now gives only empty tensors independent owning storage;
nonempty payloads keep the existing serialization behavior. A regression test
round-trips empty float and integer arrays sharing one backing buffer. The final
measurement includes this corrected storage path in every mode.

## Independent verification and panel preparation

The bounded eight-update fork exercises uniform and balanced replay, imported
labels, nonempty Adam, a forced KL tail and restoration of eight live 32-action
drafts. Replaying a saved update within CUDA reproduces model, actor Adam, critic
Adam, shuffle RNG, targets, advantages and buffer state exactly. A separate native
mechanical check verifies full-terminal child admission and the next-update rule;
its fixture never enters main replay. A one-update CUDA profiler records actual
device execution with PPO and SIL, then exits before main training.

The separate transfer preparation admitted sixteen cases in seventeen attempts:
eight food and eight medical. Selection uses product, native validity and exact/
rate-independent geometry identity, including comparison with all old training
inputs, imported anchors, historical evaluation and earlier new panel entries.
The final common transfer subset will additionally exclude overlap with the
actual 512 continuation inputs after training; no training input is resampled.

The autonomous continuation implementation restores original counters, native
elapsed allowance and recorded accumulated time. The separate two-second branch
guard does not increase the native action horizon or reset observation clocks.
A manual pre-pose branch preserved its recorded starting attempt 3 and terminated
at original attempt 64, returning the unchanged valid best without a useful
commit. This is implementation validation, not a treatment result.

## Reproduction

Use the frozen server runtime and its selected Torch/CUDA Python environment.
The admitted source directory is `artifacts/factory-edit-pilot/replay-sources`;
the final complete-cycle measurement is `replay-readiness-v3/measurement.json`.
All paths below are relative to that runtime root unless explicitly supplied.

```sh
python tools/factory-edit-pilot/blueprint_replay_balance.py --help
python tools/factory-edit-pilot/blueprint_replay_balance.py readiness \
  --parent-matrix "$PARENT_MATRIX" --sources "$SOURCES" --output "$READINESS"
python tools/factory-edit-pilot/blueprint_replay_balance.py verify-fork \
  --parent-matrix "$PARENT_MATRIX" --sources "$SOURCES" \
  --measurement "$READINESS/measurement.json" --output "$FORK"
python tools/factory-edit-pilot/blueprint_replay_balance.py prepare \
  --parent-matrix "$PARENT_MATRIX" --sources "$SOURCES" \
  --measurement "$READINESS/measurement.json" --verification "$FORK/verification.json" \
  --panel "$PANEL/panel.json" --output "$MATRIX"
python tools/factory-edit-pilot/blueprint_replay_balance.py run \
  --output "$MATRIX" --environment "$WANDB_ENVIRONMENT"
python tools/factory-edit-pilot/blueprint_replay_balance.py probe \
  --matrix "$MATRIX" --output "$CONTINUATION"
python tools/factory-edit-pilot/blueprint_replay_balance.py report \
  --matrix "$MATRIX" --output "$REPORT"
```

`prepare` binds measured runtime source hashes, device/CPU allocation, parent
bundles, cohort inputs, native library and arm order before the first main
optimizer update. `run` does not invoke readiness, profiling, exact replay,
continuation probing or report reconstruction automatically.

Validation completed locally: `cargo check --workspace`, `cargo test --workspace`,
Python compilation and the empty-storage/namespace/dedup contract tests. Native
contract tests also passed on the server; runtime execution tests and the final
fork receipt are recorded with the launch evidence. Generated traces, checkpoints
and measurement JSON remain under ignored artifacts.

The server execution suite passed seven tests; its additional opt-in constructor
test was not enabled in that invocation. Actual parallel fresh generation and
restoration were exercised by the eight-update fork. Main training has started
under the bound launch; this report makes no endpoint quality claim.
