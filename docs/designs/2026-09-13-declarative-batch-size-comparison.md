# Declarative PPO rollout batch comparison

Status: complete; bounded readiness and all four registered arms passed.
Protocol fixed before outcomes. User explicitly authorized the pending
256-versus-1,024 comparison.

## Registered comparison

Two paired fresh learner seeds (17, 29), each with 32,768 training transitions per
arm. Batch 256 has eight logical streams and 128 updates; batch 1,024 has thirty-two
logical streams and 32 updates. Every stream contributes 32 consecutive actions
per update, with a real terminal at 64 actions. Keep minibatches 64 and all model,
PPO, SIL, action, native resource and evaluation settings from the completed
curriculum continuation. No trained parent is imported: both arms start from the
same fresh model/empty Adam/replay state for each seed. Existing trained models
remain retained. This measures learning under matched total action budgets.

Use the same eight physical native workers/cores and coordinator/GPU allocation
in both arms. Each physical worker retains one or four independent native stream
objects. Collect all logical streams in canonical order under one frozen behavior
model, then perform one PPO update. Do not accumulate rollouts from updated
policies into the 1,024-action batch or lengthen the 64-action episode.

Each seed owns a separate canonical fresh-input store shared only by its two
arms. Global episode slot maps to one actual generated request/current witness,
independent of logical stream count. Generate on demand, preserve actual artifacts
and restore them without regeneration. Inherited starts are policy dependent and
are not claimed to be paired. Counterbalance arm order: seed 17 uses 256 then
1,024; seed 29 uses 1,024 then 256. Report actual input-access cost and phase times
alongside wall time because the second arm can reuse previously generated slots.

Keep 80/10/10 targets, strict latest-pass area improvement, previous-wave donors,
uniform eligible selection without replacement and fresh fallback on shortage.
Wave size consequently differs (8 versus 32 factories), as do bootstrap quotas,
parent pools and possible realized mix. Report this explicitly: it is a deployed
batch/collection configuration comparison, not an isolated gradient-noise study.

Reuse the frozen independent 32-factory panel and two sampling replicates at
0, 16,384 and 32,768 training actions. Same evaluation action seeds and 64-action /
10-second allowance. Report full and actual-training-geometry-disjoint results,
per seed, product stratum and paired factory. Two seeds are a short comparison,
not a robust convergence or arbitrary-input generalization claim. Do not extend
or tune arms after inspecting scores.

Limits: 1,800 seconds and 8 GiB retained outputs per arm, 7,800 seconds for the
four-arm matrix, 32 GiB process-tree RSS and 16 GiB free-disk reserve. Readiness
is a separate bounded process; no profiler, replay audit or measurement cycle
matrix is attached to training. Reuse completed native/model/optimizer proofs.

## Continuity checklist and stage contract

| Existing capability / failure prevented | Placement after change | Verification |
| --- | --- | --- |
| Eight distinct native worker cores, shared-memory bounded transport | Physical pool unchanged; explicit logical multiplexing | Actual process/affinity receipts, eight-stream equivalence and 32-stream execution |
| Packed ordered sampling, CPU behavior, CUDA/prefetching update | Same policy and update kernel; one full logical batch | Batch sample/frontier counts and frozen model identity |
| 64-action episodes, continuing GAE, actor-safe KL tail | Independent native streams and unchanged learner | Save/restore unfinished 32-stream chunk; existing optimizer proofs |
| Full-terminal SIL credit, following-update eligibility, cached inputs | Unchanged buffer and successful trajectories | Real terminal admission and retained source chunks |
| Fresh actual requests/layouts, no fixed fallback | Canonical slot store maps logical identities explicitly | Both mappings load the identical payload; swapped/missing binding rejection |
| 80/10/10 durable exact-best starts | Stream-count-aware identities and initial bootstrap plan | 32-stream slot identity, quotas, native inherited admission and restore |
| Valid best, frozen native blueprint, operational ceilings | Native semantics unchanged | Actual complete episodes/evaluations and recorded limits |
| Durable checkpoint/plan publication, no input regeneration | All logical streams included in snapshot and worker failure evidence | Process restoration and next-plan binding |
| Independent evaluation, honest timing and artifact hygiene | Reuse panel; compare at equal action counts | Actual CLI report, geometry exclusions, compact provenance and retained inputs |

Input DTO: existing declarative config plus explicit `execution` logical-pool
selection and canonical input-bank setting. Output DTO: unchanged learner chunks
with one unique stream/frontier per logical stream; snapshots include every live
stream. Worker messages keep physical worker identity separate from logical
stream identity. Failures preserve each child worker's partial chunk/native state.


## Readiness closure before the registered run

The separate release-native/CUDA readiness command passed in 152.50 seconds:
seven input/curriculum tests, existing-versus-multiplexed eight-stream equivalence,
and six real updates at each batch size. Both arms preserved unfinished actual
episodes across checkpoint 3 and process restoration, then completed all streams
at checkpoint 6. Actual eight worker affinities and maximum concurrency were
recorded; thirty-two logical streams shared those same eight workers. Fresh-slot
pairing and inherited starts were exercised. The equivalence fixture fixes only
the policy's elapsed-time observation for an identical-input comparison; native
deadlines remain active. Actual readiness rollouts and training use real elapsed
observations without that fixture wrapper. All eight existing learner tests also
passed with the selected release native library.

The unchanged native/model/optimizer proofs from the preceding completed slice
remain applicable. Integrated final evaluation/report coverage and resource
outcomes will be closed by the registered run. Readiness exited before launch;
the normal collection/update path has no profiler or replay audit. The observed
short readiness forecast is about 80 minutes for four arms, with the registered
130-minute hard matrix limit unchanged; it is not a throughput guarantee.


## Integrated completion

All four arms completed 131,072 total training actions and 2,048 full episodes
in 4,590.95 seconds. The report command checked all 512 actual input slots per
arm, equal evaluation boundaries, complete validated cohorts and identical
actual payloads on paired fresh slots. Common geometry exclusions, realized
curriculum and resource tradeoffs are recorded in the
[completed report](../reports/2026-09-13-declarative-batch-size-comparison.md).
The continuity checklist is closed: no input regeneration on restoration,
no native/model/optimizer contract changes, no hard resource-limit failures,
and retained valid output from every evaluation. Higher host RSS for 32 logical
streams and seed-dependent large-factory results are explicit measured tradeoffs.
