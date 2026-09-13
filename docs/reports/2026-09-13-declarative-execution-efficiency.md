# Declarative execution efficiency and preserved learning state

Status: implementation and isolated verification complete. Resume the paused
four-hour learner through the verified execution migration; record its launch
separately. This slice changes execution and storage, not PPO settings, replay
selection, native editing semantics, the input curriculum or the duration budget.

## Findings and measured change

The retained main-run updates 58–73 averaged 39.890 seconds/update. Their mean
collection/publication/PPO durations were 15.384/12.392/3.458 seconds. Publication
was previously described too loosely as checkpoint storage: most of its cost was
serializing full trajectories, and it also includes next-wave publication.

A separate baseline started from the durable update-80 pause. It collected two
updates with eight physical workers, CPU behavior inference, 32 logical streams,
a 1,024-action batch and the original source. Its terminal update took 45.357
seconds. The optimized normal CLI used the same actual generated requests and
initial layouts, 16 physical workers and GPU behavior inference. The terminal
update took 9.715 seconds, approximately 4.67 times faster on this one wave.

| Measured operation | Before | After | Comparison scope |
| --- | ---: | ---: | --- |
| Terminal-update collection | 16.433 s | 4.254 s | Same initial factories; actual live collection |
| Terminal-update publication | 11.239 s | 1.096 s | Includes required checkpoint and next-wave work |
| Terminal-update PPO kernel | 3.695 s | 3.578 s | Same numerical settings |
| Terminal success ingestion | 13.588 s | 0.699 s | Same recorded payload and preceding learner state |
| Policy inference for 1,024 observations | 0.873 s CPU | 0.284 s GPU | Identical inputs, model and categorical RNG |
| Cold replay hydration | 219.004 s | 9.395 s | Same retained buffer, original versus compact sources |

The normal optimized command completed both updates, its fixed-panel evaluation
and final report in 40.863 seconds of learner time. The first update included cold
hydration and startup and reached publication at 24.900 seconds. These are short
measurements, not a sustained throughput or convergence claim. The baseline had
to generate its diagnostic input bank; the candidate read those actual inputs.
Warm terminal-update timings avoid that construction difference. The observation
includes elapsed time, so faster execution and CPU/GPU numerical differences can
change subsequent live trajectories. Exact optimizer equivalence was therefore
checked separately on retained observations and actions.

The 13-second ingestion figure includes admission, deduplication and old evidence
loading, not only eviction. A single isolated profile observed 360 evictions,
4,424,040 stratum calls and restoration of 129,228 small tensors. Profiled victim
selection accumulated 3.464 seconds. These profiler timings are attribution
measurements, not additional unbiased speedup comparisons.

## Implementation and ownership

- Build one eviction ordering per update, with separate heaps per stratum. Keep
  the same balancing, utility aging, maturity thresholds and deterministic ties.
- Hydrate newly admitted replay while its inputs are available. Keep the small
  published replay source in memory instead of writing and immediately reloading
  the whole previous trajectory.
- Persist only selected replay labels and unfinished credit inputs under
  `replay/`, using `declarative-replay-chunk-v1`. Reward/terminal history needed
  for future credit remains in the checkpoint. Original full trajectories require
  an explicit `retention.training_traces` update list; it defaults to empty.
- Serialize immutable tensor values directly through one pickle reducer. Remove
  recursive Tensor/NumPy object-tree conversions and the duplicate transmission
  of already received policy inputs. The checkpoint remains a standard Torch
  archive; old archives remain readable without an alternate learner path.
- Retain one copy of each episode's immutable blueprint/catalog observations.
  Dynamic placements, ports, conflicts, routes, work and clock features remain
  current on every action.
- Run batched root and conditional-port inference on the existing GPU. Keep
  categorical sampling on the established CPU RNG in the same stream/depth
  order. The 1,024 sampled actions matched exactly in the recorded comparison;
  maximum log-probability/value/entropy differences were respectively
  3.34e-6, 7.46e-8 and 6.68e-6. Fixed evaluation still uses its established CPU
  worker path, preserving the evaluation comparison.
- Use 16 independent worker cores for 32 logical streams, with coordinator cores
  16 and 17 on the 24-core server. Sixteen divides the logical batch evenly.
  This increases concurrent native environment work without changing the batch,
  episode horizon, policy version within a rollout or optimizer schedule.

Keeping received inputs across updates initially exposed Python 3.12 mmap file
handles: a 16-worker two-chunk run exceeded the existing 1,024-descriptor limit.
The declarative receiver now copies each contiguous shared buffer once before
unpickling and closes its mapping immediately. Tensor objects still decode
without recursive conversion or a second transmission. Transient predecessor
receivers retain their existing mapping path. A regression test retains 1,100
messages without descriptor growth. The normal loop also releases the complete
previous payload after publication. No operating-system limit was raised.

## Explicit source and input migration

The original source tree and paused run remain intact. Migration imports exactly
update 80, including both Adam states, both RNG states, replay utility/history,
actual input identities, selected next-wave starts, evaluations, latest recovery
states and all/small/large best models. Accumulated learner time is
2,170.087758415 seconds, leaving 12,229.912241585 seconds of the four-hour budget.
Diagnostic time is excluded; the four-hour clock does not restart.

Compact replay extraction covered 67 source chunks required by current and
protected checkpoints: 2,832,160,154 bytes became 197,074,241 bytes. This was one
237.140-second migration, outside normal training. Original predecessor evidence
is retained, so this is the new run's source footprint, not a claim that total
server disk use fell by that amount. The extraction has a 900-second resource
bound. No full-source conversion or audit runs per update.

The input store contract also binds its catalog locator to an absolute source
path. Reusing that locator under the new source tree was correctly rejected
before training. Migration now creates a private store with hard links to the
unaltered actual episode, generation, request and layout files. It verifies
catalog content equality and atomically replaces only the new store's catalog
locator, recording `declarative-input-location-migration-v1`. Parent files and
metadata remain unchanged. Fresh episodes append new canonical slots in the
private store; inherited starts retain their exact external source references.

## Continuity closure and verification

| Obligation | Evidence |
| --- | --- |
| Fresh products/rates/complete randomized starts and inherited lineage | Actual paired inputs reused without regeneration; native fresh-input and curriculum tests; normal CLI completes 32 episodes and selects the next wave |
| Frozen directed native semantics and valid retained best | Native library unchanged; all normal training/evaluation exports validate; pending-draft native tests pass |
| Sampling, PPO, credit, SIL and measured eviction | Identical recorded CPU/GPU actions; two retained PPO updates exactly match model, both Adam states, shuffle RNG, complete buffer and ordered events |
| Recovery across chunks and process restoration | Compact-source replay restores between updates; actual normal midpoint restores all 32 native streams at action 32 |
| Best/current checkpoint sources and failure handling | Protected-source tests, missing/corrupt input tests, worker-loss test, duration/stop tests and normal endpoint report |
| Resource and normal-path isolation | 50 targeted server tests pass without skips; normal run peaks at 20,792,164,352 RSS bytes; all probe workers and GPU observation processes end before main launch |
| Source-bound continuation | Matching parent/config/source/compact-manifest receipts required before prepare; private input-store relocation preserves actual file hashes |

`cargo check --workspace` and `cargo test --workspace` pass. No Rust code or native
binary changed. Local non-native storage/migration checks also pass; the final
server suite supplies Linux/shared-memory/CUDA/native coverage. The complete
isolated verification finished in 56.941 seconds.

Two preliminary verification runs exposed the catalog-location and descriptor
ownership problems described above. Two early retained-replay diagnostic attempts
stopped on their own path-containment and tuple-versus-JSON comparison mistakes;
those checks were corrected without weakening input containment or changing the
learner. Only the completed matching receipts authorize the resumed run.

An existing training metric counts inherited starts in `generated_episodes` and
can report negative per-update `generation_rejections`; do not use that scalar as
a fresh-generation count. The normal endpoint's input-coverage report separates
recorded attempts and clamps per-input rejection counts. This pre-existing metric
issue is recorded for a separate correction and is not a speedup premise.

## Reproduction and retained evidence

Run diagnostics only with the main learner paused. The frozen original source is
`/home/ashrimp/Srv/aic-declarative-four-hour-20260913`; the candidate source is
`/home/ashrimp/Srv/aic-declarative-efficiency-20260913`. Its ignored `artifacts/`
contains `baseline-cycle`, `compact-parent-v1`, `policy-device-v1.json`,
`replay-proof-v3` and `verification-v3`. The compact provenance manifest records
identities; it does not embed raw trajectories or learner checkpoints.

The explicit commands are:

```sh
python tools/factory-edit-pilot/declarative_execution.py compact \
  --parent "$AIC_PAUSED_RUN" --output "$AIC_COMPACT_OUTPUT"
python tools/factory-edit-pilot/diagnostics/declarative_execution_policy.py \
  --source . --parent "$AIC_PAUSED_RUN" --baseline "$AIC_BASELINE" \
  --output "$AIC_POLICY_PROOF"
python tools/factory-edit-pilot/diagnostics/declarative_execution_replay.py \
  --source . --parent "$AIC_PAUSED_RUN" --baseline "$AIC_BASELINE" \
  --compact-parent "$AIC_COMPACT_OUTPUT" --output "$AIC_REPLAY_PROOF"
python tools/factory-edit-pilot/declarative_execution.py verify \
  --parent "$AIC_PAUSED_RUN" --compact-parent "$AIC_COMPACT_OUTPUT" \
  --policy-proof "$AIC_POLICY_PROOF" --replay-proof "$AIC_REPLAY_PROOF" \
  --inputs "$AIC_PAIRED_INPUTS" --output "$AIC_VERIFICATION" \
  --workers 16 --behavior-device cuda:0 --environment "$AIC_TRACKING_ENV"
python tools/factory-edit-pilot/declarative_execution.py prepare \
  --parent "$AIC_PAUSED_RUN" --compact-parent "$AIC_COMPACT_OUTPUT" \
  --verification "$AIC_VERIFICATION/verification.json" --output "$AIC_RESUMED_RUN" \
  --workers 16 --behavior-device cuda:0
python tools/factory-edit-pilot/declarative_duration.py run \
  --output "$AIC_RESUMED_RUN" --environment "$AIC_TRACKING_ENV"
```

Preserve retained checkpoints and actual inputs. These local/server artifact
locations are not backups. Successful diagnostic work need not be repeated to
resume or complete normal training.
