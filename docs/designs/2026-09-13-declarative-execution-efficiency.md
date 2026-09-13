# Declarative execution efficiency for extended training

Status: complete; verified execution migration launched and normal training resumed. The user requests improving
the active four-hour experiment before scaling to 12/24/48-hour training. Keep
training running during source inspection; pause it at a durable episode boundary
when an isolated server experiment is needed. Tests must not compete with main
training. Preserve the accumulated learner and remaining four-hour time budget.

## Observed baseline and bounded diagnostic

Retained records 58–73 cover 638.239 seconds (39.890 seconds/update): collection
15.384, publication 12.392 and PPO execution 3.458 seconds/update. Publication
includes next-curriculum selection and durable storage; the remaining 8.655
seconds includes replay/cache/state work and routine evaluation. Do not label
the whole publication duration as disk I/O or the learner duration as GPU-only.

Inspect the current worker/input, replay, storage, curriculum and learner paths.
Then stop main training with its existing SIGTERM handler, preserve the completed
boundary, and use an independent process and ignored artifact directory. Profile
at most one representative retained update/storage roundtrip and one normal
two-update cycle before selecting changes. Controlled old/new measurements must
use the same actual recorded/generated inputs, actions, model and optimizer state
where applicable; explicitly distinguish cache warmup and input generation.
Do not add periodic profiling, full-buffer inference, replay audits or extra
hash scans to normal execution. End all diagnostics before resuming training.

## Continuity checklist and stage contract

| Existing benefit / failure prevented | Required placement and verification |
| --- | --- |
| Fresh randomized products/rates/complete layouts and frozen evaluation | Preserve generator settings, canonical slots and actual request/start bindings; exercise fresh and inherited inputs |
| 80/10/10 strictly improving previous-wave starts | Same donor RNG, exact selected plans and parent best payloads; no fresh-start fallback introduced |
| Exact directed native edit obligations and valid retained best | Native semantics and routing budgets unchanged; validate committed/evaluation output and live restoration |
| Frozen CPU behavior, ordered joint root/port sampling | Preserve input tensors, action domains, stream order and sampler RNG; compare old/new on identical observations |
| True-terminal GAE, PPO clipping/KL tail, two Adam states and SIL | Preserve numerical configuration, credit, likelihoods, replay selection/utility and optimizer transition on recorded input |
| Bounded replay with balanced coverage and measured-utility eviction | Preserve ordered victim decisions, admissions, source references and replay draw sequence at full capacity |
| Persistent unfinished episodes, failure evidence and exclusive run lock | Check live-draft save/restore, atomic publication, missing/corrupt input rejection and worker failure behavior |
| Fixed-panel best model and durable checkpoint/source retention | Keep all/small/large best and latest two recovery states, actual input/source chunks and pause evaluations |
| Four-hour remaining clock, resource ceilings and separate diagnostics | Preserve accumulated seconds and operational guards across an explicit source migration; normal timing proves diagnostic work is absent |
| Source provenance and readable reports | Keep predecessor immutable; describe any storage/schema migration explicitly and retain compact old/new evidence |

Input: current paused source-bound duration state, actual retained update payloads,
native library and external catalogs, plus isolated diagnostic configuration.
Output: a measured execution improvement, exact learning/recovery continuity
receipt, explicit migration if source/storage bindings change, and resumed normal
training with its remaining time. Do not discard established evidence merely to
reduce storage, or weaken recovery/checks without the user's authorization.

Candidate changes must follow measured costs. Source inspection identifies full
nested-object conversion around transport/storage, per-update chunk/checkpoint
serialization, and repeated full-buffer victim scans as candidates. These are
hypotheses until isolated measurement establishes their cost. Prefer removing
duplicate work and compact representations over adding asynchronous ownership,
queues or alternate algorithms. No speedup or completed cutover is claimed yet.

The user highlighted an omitted predecessor requirement: ordinary training had
already stopped retaining complete detailed trajectories. The predecessor's
`ppo_harness_worker.py` writes experience only when `retain_details` is explicitly
selected. The declarative replacement regressed this by storing each full chunk
as replay evidence. Restore minimal normal replay storage; faster compression of
the full trace alone does not satisfy the continuity requirement.

The user also explicitly requests higher collection concurrency and GPU policy
inference. Preserve the logical batch/horizon while measuring physical worker
counts and CPU versus GPU batched inference. Preserve the CPU categorical RNG's
draw order; GPU numerical tolerance and action equivalence on recorded inputs
need verification before selecting the normal inference device.

## Closed implementation decision

The report `docs/reports/2026-09-13-declarative-execution-efficiency.md` closes every
continuity row with retained-input equivalence, actual normal-command coverage,
resource measurements and recovery/failure tests. Adopt compact normal replay,
one indexed eviction pass, direct tensor serialization, owned persistent receive
buffers, 16 physical workers and CUDA behavior inference. Categorical RNG order,
native semantics, learning configuration and the existing CPU evaluation path
remain intact. Explicit input-location migration links actual files unchanged and
rebases only the private catalog locator. The original run remains immutable.

The main learner resumes from update 80 with 2,170.087758415 seconds already
charged. A matching source/config/parent verification receipt is required before
preparing the continuation. No isolated diagnostic is invoked by normal training.
